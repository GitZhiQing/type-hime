//! R4.5-① 提示词 spike：真实供应商上验证长时目标完成验证器（判定标准 REFACTOR §6.5）。
//!
//! 场景：S1 真完成收尾（真实 agent 轮）/ S2 假完成·只出计划 / S3 假完成·两章只做一章 /
//! S4 假完成·凭耗时与意图 / S5 非任务目标一轮 pass / S6 中文语言跟随（并入 S1-S3 断言）/
//! S7 坏 JSON fail-open（离线）/ S8 多轮收敛（种子半程 → 真实续跑轮）。
//! S2-S5 为手构轨迹 + 真实验证器（验证器的证据源就是轨迹文本，手构即确定性载体）；
//! S1/S8 需要作品目录（真实 preflight 与真实 agent 轮）。
//! 任一场景失败退出码 1（spike 判定不过 → 降级路线，决策记 DECISIONS）。
//!
//! 运行：cargo run --example goal_spike -- <作品目录>

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};

use rig::completion::Message;
use rig::completion::message::{
    AssistantContent, AssistantMessage, CallId, Text, ToolCall, ToolFunction, ToolName,
    ToolResult, ToolResultContent, UserContent,
};
use type_hime_lib::agent::events::AgentEvent;
use type_hime_lib::agent::goal::{self, GoalBudget, GoalStatus, GoalVerifierOutput};
use type_hime_lib::agent::kernel;
use type_hime_lib::agent::spec::chat_spec;
use type_hime_lib::agent::tools::ToolCtx;
use type_hime_lib::review::ReviewQueue;
use type_hime_lib::settings::{self, AppSettings};

fn main() -> Result<(), String> {
    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    match rt.block_on(run()) {
        Ok(()) => Ok(()),
        Err(e) => {
            eprintln!("[spike 失败] {e}");
            std::process::exit(1);
        }
    }
}

struct Spike {
    passed: u32,
    failed: Vec<(String, String)>,
    total: u32,
}

impl Spike {
    fn check(&mut self, name: &str, cond: bool, detail: String) {
        self.total += 1;
        if cond {
            self.passed += 1;
            println!("  PASS {name}");
        } else {
            println!("  FAIL {name} —— {detail}");
            self.failed.push((name.to_string(), detail));
        }
    }
}

/// 中文字符占比（启发式「中文为主」判据：≥0.3 视为跟随目标语言）。
fn cjk_ratio(s: &str) -> f64 {
    let total = s.chars().filter(|c| !c.is_whitespace()).count();
    if total == 0 {
        return 0.0;
    }
    let cjk = s
        .chars()
        .filter(|c| matches!(*c, '\u{4E00}'..='\u{9FFF}'))
        .count();
    cjk as f64 / total as f64
}

/// 调一次真实验证器（one_shot 无工具调用 + 解析）；provider 偶发失败重试 1 次。
async fn verify(
    settings: &AppSettings,
    objective: &str,
    iterations_done: u32,
    budget: &GoalBudget,
    transcript: &str,
) -> GoalVerifierOutput {
    let prompt = goal::format_verification_prompt(
        objective,
        GoalStatus::Active,
        iterations_done,
        budget,
        0,
        transcript,
    );
    let mut last_err = String::new();
    for _ in 0..2 {
        match kernel::one_shot_completion(settings, goal::VERIFIER_PREAMBLE, &prompt).await {
            Ok(out) => return goal::parse_verifier_output(&out),
            Err(e) => last_err = e,
        }
    }
    panic!("验证器调用失败：{last_err}");
}

fn tool_ctx(work_dir: &Path) -> ToolCtx {
    let queue: Arc<Mutex<ReviewQueue>> = Arc::new(Mutex::new(ReviewQueue::in_memory()));
    ToolCtx::new(
        work_dir.to_path_buf(),
        queue,
        Arc::new(|p| println!("    [提议] {}", p.rel_path)),
    )
}

fn brief_events(saw: Arc<dyn Fn(&AgentEvent) + Send + Sync>) -> impl Fn(AgentEvent) + Clone {
    move |ev: AgentEvent| {
        match &ev {
            AgentEvent::TextDelta { .. } => {}
            _ => {
                if let Ok(s) = serde_json::to_string(&ev) {
                    let brief: String = s.chars().take(120).collect();
                    println!("    事件 {brief}");
                }
            }
        }
        saw(&ev);
    }
}

/// 真实调用 preflight 库函数（零 token），复刻工具的输出文本。
fn preflight_text(work_dir: &Path, volume: u32, chapter: &str) -> String {
    let ch_path = type_hime_lib::assembly::chapter_path(volume, chapter);
    match type_hime_lib::works::read_file(work_dir, &ch_path) {
        Ok(text) => match type_hime_lib::ledger::read_ledger(work_dir) {
            Ok(ledger) => {
                let problems = type_hime_lib::preflight::preflight_cultivation(&text, &ledger);
                if problems.is_empty() {
                    format!("preflight 通过：{ch_path} 未发现修为层级矛盾。")
                } else {
                    format!(
                        "preflight 发现 {} 处修为层级疑点（{ch_path}）：\n{}",
                        problems.len(),
                        problems.join("\n")
                    )
                }
            }
            Err(e) => format!("[预检失败：读取账本出错] {e}"),
        },
        Err(e) => format!("[预检失败] {e}"),
    }
}

/// S8 种子轨迹：目标指令 → 第 3 章已真实预检（工具调用+结果+汇报），第 4 章未做。
fn seed_messages(work_dir: &Path, objective: &str) -> Vec<Message> {
    let directive = goal::format_goal_directive(objective);
    let user_goal = Message::User {
        content: vec![UserContent::Text(Text::new(directive))],
    };
    let call = ToolCall::new(
        CallId::from_wire("call_s8_seed"),
        ToolFunction::new(
            ToolName::new("preflight_chapter").unwrap(),
            serde_json::json!({"volume": 1, "chapter": "ch-003"}),
        ),
    );
    let result_text = preflight_text(work_dir, 1, "ch-003");
    let assistant1 = Message::Assistant(AssistantMessage::new(vec![
        AssistantContent::Text(Text::new("我先对第 3 章运行修为层级预检。")),
        AssistantContent::ToolCall(call),
    ]));
    let tool_msg = Message::User {
        content: vec![UserContent::ToolResult(ToolResult {
            call: CallId::from_wire("call_s8_seed"),
            name: ToolName::new("preflight_chapter").unwrap(),
            content: vec![ToolResultContent::Text(Text::new(result_text.clone()))],
            is_error: false,
        })],
    };
    let assistant2 = Message::Assistant(AssistantMessage::new(vec![AssistantContent::Text(
        Text::new(format!("第 3 章预检完成：{result_text}")),
    )]));
    vec![user_goal, assistant1, tool_msg, assistant2]
}

// ---------- 场景 ----------

fn scenario7_parser(sp: &mut Spike) {
    let v = goal::parse_verifier_output(
        r#"{"passed": true, "reason": "已产出问题清单", "nextAction": ""}"#,
    );
    sp.check(
        "S7a 合法对象解析",
        v.passed && v.next_action.is_none(),
        format!("{v:?}"),
    );

    let v = goal::parse_verifier_output(r#"{"passed": "yes", "reason": "x"}"#);
    sp.check("S7b passed 严格布尔", !v.passed, format!("{v:?}"));

    let v = goal::parse_verifier_output(
        "```json\n{\"passed\": false, \"reason\": \"第 4 章未检查\", \"nextAction\": \"对第 4 章运行修为预检\"}\n```",
    );
    sp.check(
        "S7c code fence 剥壳",
        !v.passed && v.next_action.as_deref().is_some_and(|s| s.contains("第 4 章")),
        format!("{v:?}"),
    );

    let v = goal::parse_verifier_output("我认为目标已经完成了，可以收工。");
    sp.check(
        "S7d 坏 JSON fail-open",
        v.passed && v.reason.contains("fail-open") && v.next_action.is_none(),
        format!("{v:?}"),
    );
}

async fn scenario5_non_task(sp: &mut Spike, s: &AppSettings) {
    let t = "[作者] /goal 你好\n[助手] 你好！我是执笔公主，你的写作助手。今天想聊点什么，或需要我帮你处理哪部分稿子？";
    let v = verify(s, "你好", 1, &GoalBudget::default(), t).await;
    sp.check("S5a 非任务目标一轮 pass", v.passed, format!("{v:?}"));
    sp.check(
        "S5b nextAction 为空（不无限续跑）",
        v.next_action.is_none(),
        format!("{v:?}"),
    );
}

async fn scenario2_plan_only(sp: &mut Spike, s: &AppSettings) {
    let objective = "修复 review/issues.json 中所有待裁决的问题，并把每条修复以提议形式提交作者审阅";
    let t = format!(
        "[作者] /goal {objective}\n\
[助手] 好的，我的处理计划：\n1. 读取 review/issues.json，列出全部待裁决问题；\n2. 逐条核对正文与账本，拟定修复方案；\n3. 用 propose_edit 提交修复提议。\n\
[助手] 计划清单已列好（共 3 条待裁决问题），结构清晰、步骤明确，接下来照此执行就可以了。"
    );
    let v = verify(s, objective, 1, &GoalBudget::default(), &t).await;
    sp.check("S2a 假完成（只出计划）判 fail", !v.passed, format!("{v:?}"));
    sp.check(
        "S2b nextAction 给出最小下一步",
        v.next_action.is_some(),
        format!("{v:?}"),
    );
    let lang = format!("{}{}", v.reason, v.next_action.unwrap_or_default());
    let ratio = cjk_ratio(&lang);
    sp.check(
        "S2c 中文语言跟随",
        ratio >= 0.3,
        format!("CJK 占比 {ratio:.2}：{lang}"),
    );
}

async fn scenario3_partial_coverage(sp: &mut Spike, s: &AppSettings) {
    let objective = "对第 3 章和第 4 章各运行一次修为层级预检，并分别向作者汇报两章的结论";
    let t = format!(
        "[作者] /goal {objective}\n\
[助手] 我先处理第 3 章。\n\
[工具调用] preflight_chapter({{\"volume\": 1, \"chapter\": \"ch-003\"}})\n\
[工具结果] preflight_chapter → preflight 通过：第1卷/ch-003.md 未发现修为层级矛盾。\n\
[助手] 第 3 章预检完成，未发现修为层级矛盾。目标要求的工作已完成。"
    );
    let v = verify(s, objective, 1, &GoalBudget::default(), &t).await;
    sp.check("S3a 假完成（两章只做一章）判 fail", !v.passed, format!("{v:?}"));
    let combined = format!("{}{}", v.reason, v.next_action.clone().unwrap_or_default());
    sp.check(
        "S3b 结论指向未覆盖的第 4 章",
        v.next_action.is_some() && combined.contains('4'),
        format!("{v:?}"),
    );
    let ratio = cjk_ratio(&combined);
    sp.check("S3c 中文语言跟随", ratio >= 0.3, format!("CJK 占比 {ratio:.2}"));
}

async fn scenario4_effort_only(sp: &mut Spike, s: &AppSettings) {
    let objective = "为角色「林晚」补全一节背景设定，并以提议形式提交作者审阅";
    let t = format!(
        "[作者] /goal {objective}\n\
[助手] 我花了很长时间反复构思这个角色的背景设定，前后做了大量尝试和调整，自认为已经尽力做到最好，应该可以算完成了。"
    );
    let v = verify(s, objective, 1, &GoalBudget::default(), &t).await;
    sp.check("S4 假完成（凭耗时与意图）判 fail", !v.passed, format!("{v:?}"));
}

async fn scenario1_real_complete(sp: &mut Spike, s: &AppSettings, work_dir: &Path) {
    let objective = "对第 3 章做一次修为层级预检，并向作者汇报发现的问题或通过结论";
    let prompt = goal::format_goal_directive(objective);
    let saw_preflight = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&saw_preflight);
    let saw = Arc::new(move |ev: &AgentEvent| {
        if let AgentEvent::ToolCall { name, .. } = ev {
            if name == "preflight_chapter" {
                flag.store(true, Ordering::Relaxed);
            }
        }
    });
    let ctx = tool_ctx(work_dir);
    let outcome = match kernel::run(s, Some(&ctx), &chat_spec(), Vec::new(), &prompt, brief_events(saw)).await {
        Ok(o) => o,
        Err(e) => {
            sp.check("S1 agent 轮运行", false, e);
            return;
        }
    };
    let transcript = goal::render_transcript(&outcome.messages, 1200, 20000);
    let v = verify(s, objective, 1, &GoalBudget::default(), &transcript).await;
    sp.check("S1a 真完成验证 pass 收尾", v.passed, format!("{v:?}"));
    sp.check(
        "S1b 真实调用了 preflight 工具",
        saw_preflight.load(Ordering::Relaxed),
        "agent 未调用 preflight_chapter（通过不可信）".into(),
    );
    let ratio = cjk_ratio(&v.reason);
    sp.check(
        "S1c 中文语言跟随",
        ratio >= 0.3,
        format!("CJK 占比 {ratio:.2}：{}", v.reason),
    );
}

async fn scenario8_multiround(sp: &mut Spike, s: &AppSettings, work_dir: &Path) {
    let objective = "对第 3 章和第 4 章各运行一次修为层级预检，并分别向作者汇报两章的结论";
    let seed = seed_messages(work_dir, objective);

    // 第一边界：半程轨迹 → 验证器应判 fail 并指向第 4 章
    let t1 = goal::render_transcript(&seed, 1200, 20000);
    let v1 = verify(s, objective, 1, &GoalBudget::default(), &t1).await;
    sp.check("S8a 半程轨迹判 fail", !v1.passed, format!("{v1:?}"));
    let combined1 = format!("{}{}", v1.reason, v1.next_action.clone().unwrap_or_default());
    sp.check(
        "S8b nextAction 指向第 4 章",
        v1.next_action.is_some() && combined1.contains('4'),
        format!("{v1:?}"),
    );

    // 续跑轮：真 agent，历史=种子，prompt=续跑指令（含上一轮验证结论）
    let next_action = v1.next_action.clone().unwrap_or_default();
    let prompt = goal::format_continuation_prompt(
        objective,
        1,
        &GoalBudget::default(),
        0,
        &v1.reason,
        &next_action,
    );
    let saw_ch4 = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&saw_ch4);
    let saw = Arc::new(move |ev: &AgentEvent| {
        if let AgentEvent::ToolCall { name, args } = ev {
            if name == "preflight_chapter" && args.contains("004") {
                flag.store(true, Ordering::Relaxed);
            }
        }
    });
    let ctx = tool_ctx(work_dir);
    let outcome = match kernel::run(s, Some(&ctx), &chat_spec(), seed.clone(), &prompt, brief_events(saw)).await {
        Ok(o) => o,
        Err(e) => {
            sp.check("S8 续跑轮运行", false, e);
            return;
        }
    };

    // 第二边界：种子 + 续跑轮 → 验证器应 pass 收尾
    let mut all = seed.clone();
    all.extend(outcome.messages.clone());
    let t2 = goal::render_transcript(&all, 1200, 20000);
    let v2 = verify(s, objective, 2, &GoalBudget::default(), &t2).await;
    sp.check("S8c 续跑后验证 pass 收尾", v2.passed, format!("{v1:?} → {v2:?}"));
    sp.check(
        "S8d 续跑轮真实核了第 4 章",
        saw_ch4.load(Ordering::Relaxed),
        "第 2 轮未对 ch-004 调用 preflight_chapter".into(),
    );
}

async fn run() -> Result<(), String> {
    let settings = settings::load()?;
    let provider = settings.active().ok_or("无 active 供应商")?;
    println!(
        "[环境] 供应商：{} @ {}（default_model: {}）",
        provider.name, provider.base_url, provider.default_model
    );
    let work_dir: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("用法：cargo run --example goal_spike -- <作品目录>")?;
    println!("[作品] {}", work_dir.display());

    let mut sp = Spike {
        passed: 0,
        failed: Vec::new(),
        total: 0,
    };

    println!("\n[S7] 解析器离线自检（坏 JSON fail-open）");
    scenario7_parser(&mut sp);
    println!("\n[S5] 非任务目标（真实验证器）");
    scenario5_non_task(&mut sp, &settings).await;
    println!("\n[S2] 假完成·只出计划（真实验证器）");
    scenario2_plan_only(&mut sp, &settings).await;
    println!("\n[S3] 假完成·两章只做一章（真实验证器）");
    scenario3_partial_coverage(&mut sp, &settings).await;
    println!("\n[S4] 假完成·凭耗时与意图（真实验证器）");
    scenario4_effort_only(&mut sp, &settings).await;
    println!("\n[S1] 真完成收尾（真实 agent 轮 + preflight）");
    scenario1_real_complete(&mut sp, &settings, &work_dir).await;
    println!("\n[S8] 多轮收敛（种子半程 → 真实续跑轮）");
    scenario8_multiround(&mut sp, &settings, &work_dir).await;

    println!(
        "\n=== spike 判定：{}/{} 通过 ===",
        sp.passed, sp.total
    );
    if sp.failed.is_empty() {
        println!("=== 全部通过：验证器提示词可用，进入 R4.5-② 状态机 ===");
        Ok(())
    } else {
        for (name, detail) in &sp.failed {
            println!("失败：{name} —— {detail}");
        }
        Err(format!("spike 判定未全过（{}/{}）", sp.passed, sp.total))
    }
}
