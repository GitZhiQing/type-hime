//! 流式命令的业务体：Tauri IPC（lib.rs）与 HTTP SSE（examples/webapi.rs）双入口共用。
//! 回调参数化：事件与提议的产生方不关心传输层。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use agent::events::AgentEvent;
use agent::kernel;
use agent::session::{self, Session, SessionStore};
use agent::spec::{global_chat_spec, ledger_spec, ledger_tidy_spec, reviewer_spec, writer_spec, AgentSpec};
use agent::tools::{FlowRequest, FlowRunner, ToolCtx};
use pipeline::Stage;
use review::{Proposal, ReviewQueue};

use crate::{agent, assembly, issues, ledger, pipeline, preflight, review, works};

pub type ProposalSink = Arc<dyn Fn(&Proposal) + Send + Sync>;
pub use agent::events::EventSink;

/// 累计 usage（None + x = x；Some + Some = 和）。
fn merge_usage(
    base: Option<agent::kernel::UsageReport>,
    add: Option<agent::kernel::UsageReport>,
) -> Option<agent::kernel::UsageReport> {
    match (base, add) {
        (None, None) => None,
        (None, Some(u)) => Some(u),
        (Some(b), None) => Some(b),
        (Some(b), Some(u)) => Some(agent::kernel::UsageReport {
            input_tokens: b.input_tokens + u.input_tokens,
            output_tokens: b.output_tokens + u.output_tokens,
        }),
    }
}

/// 通用会话运行（chat / tidy 等单 prompt 场景）。
async fn run_and_save_session(
    sess_id: String,
    store: &SessionStore,
    tool_ctx: Option<&ToolCtx>,
    spec: &AgentSpec,
    prompt: &str,
    on_event: &EventSink,
) -> Result<(), String> {
    let settings = kernel::current_settings()?;
    let mut sess = session::load_session(store, &sess_id)
        .unwrap_or_else(|_| Session::new(sess_id.clone()));
    let history = sess.messages.clone();
    let sink = Arc::clone(on_event);
    let outcome = kernel::run(
        &settings,
        tool_ctx,
        spec,
        history,
        prompt,
        move |ev| sink(ev),
    )
    .await?;
    sess.messages.extend(outcome.messages);
    sess.last_usage = outcome.usage;
    sess.usage = merge_usage(sess.usage, outcome.usage);
    session::save_session(store, &sess)
}

/// 会话自动标题：首轮完成后生成一句短标题；任何失败回退首条用户消息截断（静默）。
async fn autotitle(store: &SessionStore, sess_id: &str) {
    let Ok(mut sess) = session::load_session(store, sess_id) else {
        return;
    };
    if sess.title.is_some() || sess.messages.is_empty() {
        return;
    }
    let first_user = session::first_user_text(&sess);
    let preview: String = first_user.chars().take(24).collect();
    let generated = try_generate_title(&first_user).await;
    sess.title = Some(generated.unwrap_or(preview));
    let _ = session::save_session(store, &sess);
}

async fn try_generate_title(first_user: &str) -> Option<String> {
    let settings = kernel::current_settings().ok()?;
    let head: String = first_user.chars().take(200).collect();
    let prompt = format!(
        "为下面的对话起一个不超过16字的简短标题，直接输出标题本身，不要引号和标点：\n{head}"
    );
    let out = kernel::one_shot_completion(&settings, "你是对话标题生成器，只输出标题。", &prompt)
        .await
        .ok()?;
    let line = out.lines().next().unwrap_or("").trim();
    let line = line.trim_matches(|c: char| c == '"' || c == '“' || c == '”' || c == '#');
    let line: String = line.chars().take(24).collect();
    if line.is_empty() {
        None
    } else {
        Some(line)
    }
}

/// 流式对话（旁路会话）：产物经提议审阅回流。拒绝带评论的反馈注入本轮上下文。
/// approval：审批经纪人（R4：bash 等需审批工具挂起等 UI 决策；None=无审批通道）。
pub async fn chat_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    approval: Option<Arc<agent::approval::ApprovalBroker>>,
    work_path: String,
    session_id: String,
    prompt: String,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    // 取走本作品的拒绝反馈，拼进 prompt 开头（驱动 Agent 按反馈修改重提）
    let feedback = {
        let mut q = queue.lock().unwrap();
        q.take_feedback(&work_path)
    };
    let prompt = if feedback.is_empty() {
        prompt
    } else {
        let blocks: Vec<String> = feedback
            .iter()
            .map(|f| {
                format!(
                    "【作者拒绝了对 {} 的提议】作者反馈：{}。请按反馈调整后重新提交提议。",
                    f.rel_path, f.comment
                )
            })
            .collect();
        format!("{}\n\n{}", blocks.join("\n"), prompt)
    };
    // slash 指令化（R4-⑥）：/命令 参数 → 结构化指令块（附在消息尾，原命令留在消息首供展示）
    let prompt = match slash_instruction(&prompt) {
        Some(block) => format!("{prompt}\n\n{block}"),
        None => prompt,
    };
    let spec = {
        // 技能动态挂载（R4-④ · REDESIGN §8.2）：默认 writer-basics + slash 强制 + 关键词路由
        let mut extra = agent::skills::route_skills(&prompt, Some(&work_dir));
        if let Some(s) = agent::spec::slash_forced_skill(&prompt) {
            extra.push(s.to_string());
        }
        agent::spec::chat_spec_with_skills(&extra)
    };
    let runner = chat_flow_runner(Arc::clone(queue), work_path.clone(), Arc::clone(&on_proposal));
    // 权限上下文（R4-①）：会话档位（存会话档案）+ 作品覆盖表（project.json）
    let store = SessionStore::Work(work_dir.clone());
    let session_mode = session::load_session(&store, &session_id)
        .map(|s| s.permission_mode)
        .unwrap_or_default();
    let perm_config = pipeline::load_permission_config(&work_dir).unwrap_or_default();
    let permission = crate::permissions::PermissionContext::new(session_mode, perm_config);
    let tool_ctx = ToolCtx::new(work_dir.clone(), Arc::clone(queue), on_proposal)
        .approval(approval)
        .event_sink(Some(Arc::clone(&on_event)))
        .flow_runner(Some(runner))
        .permission(Some(permission));
    run_and_save_session(session_id.clone(), &store, Some(&tool_ctx), &spec, &prompt, &on_event)
        .await?;
    autotitle(&store, &session_id).await;
    Ok(())
}

/// 全局对话（rail「对话」页）：无作品上下文、无文件工具、无提议。
pub async fn chat_flow_global(
    session_id: String,
    prompt: String,
    on_event: EventSink,
) -> Result<(), String> {
    let spec = global_chat_spec();
    let store = SessionStore::Global;
    run_and_save_session(session_id.clone(), &store, None, &spec, &prompt, &on_event).await?;
    autotitle(&store, &session_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// run_flow 执行器（R4 Spike B）：chat 会话内的流程编排端口实现
// 语义（REFACTOR R4）：必须触发既有 flows（保副作用闭环），触发前补 pipeline::gate_check。

/// chat 上下文里的 run_flow 执行器：kind → 既有 flows。
pub fn chat_flow_runner(
    queue: Arc<Mutex<ReviewQueue>>,
    work_path: String,
    on_proposal: ProposalSink,
) -> FlowRunner {
    Arc::new(move |req: FlowRequest, sink: EventSink| {
        let queue = Arc::clone(&queue);
        let work_path = work_path.clone();
        let prop = Arc::clone(&on_proposal);
        Box::pin(async move { run_flow_request(&queue, &work_path, &req, sink, prop).await })
    })
}

/// slash 指令化（R4-⑥ · REDESIGN §5.3）：`/命令 参数` → 结构化指令块，驱动 Agent 走既有能力
/// （run_flow / 直连检查工具），而不是前端直调命令。返回 None 表示不是已知 slash 命令。
pub fn slash_instruction(prompt: &str) -> Option<String> {
    let head = prompt.trim_start();
    if !head.starts_with('/') {
        return None;
    }
    let mut it = head.split_whitespace();
    let cmd = it.next()?.trim_start_matches('/');
    let rest: Vec<&str> = it.collect();
    // 参数：数字序列（一个=章号，两个=卷+章）或显式 ch-00N；审卷的单个数字=卷号
    let nums: Vec<u32> = rest.iter().filter_map(|s| s.parse::<u32>().ok()).collect();
    let explicit_chapter = rest.iter().find(|s| s.starts_with("ch-")).map(|s| s.to_string());
    let (volume, chapter) = if cmd == "审卷" {
        (nums.first().copied().unwrap_or(1), explicit_chapter.unwrap_or_default())
    } else {
        match nums.len() {
            0 => (1u32, explicit_chapter.unwrap_or_default()),
            1 => (
                1u32,
                explicit_chapter.unwrap_or_else(|| format!("ch-{:03}", nums[0])),
            ),
            _ => (
                nums[0],
                explicit_chapter.unwrap_or_else(|| format!("ch-{:03}", nums[1])),
            ),
        }
    };
    let flow = match cmd {
        "章纲" => Some(("chapter_outline", "生成章纲")),
        "正文" => Some(("chapter", "生成正文")),
        "回写" => Some(("ledger_update", "账本回写")),
        "一致性" => Some(("consistency", "一致性检查")),
        "去AI味" => Some(("deai", "去 AI 味")),
        "复检" => Some(("recheck", "复检")),
        "审卷" => Some(("review_volume", "卷级批审")),
        "整理账本" => Some(("ledger_tidy", "账本整理")),
        _ => None,
    };
    if let Some((kind, label)) = flow {
        let (params, chapter_hint) = match kind {
            "review_volume" => (format!("volume={volume}"), String::new()),
            "ledger_tidy" => ("（无参数）".to_string(), String::new()),
            _ if chapter.is_empty() => (
                format!("volume={volume}"),
                "\n（作者未给出章号，请先向作者确认章号再执行。）".to_string(),
            ),
            _ => (
                format!("volume={volume}, chapter={chapter}"),
                String::new(),
            ),
        };
        return Some(format!(
            "【技能指令】{label}（{cmd}）\n请调用 run_flow 工具执行（kind={kind}, {params}），不要用 read_file/propose_* 手工模拟该流程；完成后向作者简要汇报结果与产出的提议。{chapter_hint}"
        ));
    }
    match cmd {
        "泄漏检查" => Some(
            "【技能指令】关联泄漏检查\n请调用 check_visibility_leak 工具（零 token，不发模型请求），把结果汇报给作者。".to_string(),
        ),
        "preflight" => Some(format!(
            "【技能指令】修为预检\n请调用 preflight_chapter 工具（volume={volume}, chapter={chapter}），把结果汇报给作者。"
        )),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::slash_instruction;

    #[test]
    fn slash_instruction_parses_params() {
        let s = slash_instruction("/正文 1 3").unwrap();
        assert!(s.contains("kind=chapter"), "{s}");
        assert!(s.contains("volume=1, chapter=ch-003"), "{s}");

        // 单数字 = 章号（默认第 1 卷）
        let s = slash_instruction("/一致性 5").unwrap();
        assert!(s.contains("kind=consistency"), "{s}");
        assert!(s.contains("chapter=ch-005"), "{s}");

        // 显式 ch-00N
        let s = slash_instruction("/复检 ch-002").unwrap();
        assert!(s.contains("chapter=ch-002"), "{s}");

        // 无章号 → 提示先问作者
        let s = slash_instruction("/正文").unwrap();
        assert!(s.contains("未给出章号"), "{s}");

        // 审卷只带卷号；整理账本无参数
        let s = slash_instruction("/审卷 2").unwrap();
        assert!(s.contains("kind=review_volume") && s.contains("volume=2"), "{s}");
        let s = slash_instruction("/整理账本").unwrap();
        assert!(s.contains("kind=ledger_tidy"), "{s}");

        // 直连检查工具
        let s = slash_instruction("/泄漏检查").unwrap();
        assert!(s.contains("check_visibility_leak"), "{s}");
        let s = slash_instruction("/preflight 1 3").unwrap();
        assert!(s.contains("preflight_chapter") && s.contains("chapter=ch-003"), "{s}");

        // 非 slash / 未知命令
        assert!(slash_instruction("普通消息").is_none());
        assert!(slash_instruction("/不存在 1").is_none());
    }
}
/// kind → Stage 的阶段门映射（ledger_tidy 为维护操作，不设门）。
fn flow_gate_stage(kind: &str) -> Option<Stage> {
    match kind {
        "chapter_outline" => Some(Stage::ChapterOutline),
        "chapter" | "ledger_update" => Some(Stage::Chapters),
        "consistency" | "deai" | "recheck" | "review_volume" => Some(Stage::Review),
        _ => None,
    }
}

const FLOW_KINDS: &[&str] = &[
    "chapter_outline",
    "chapter",
    "ledger_update",
    "consistency",
    "deai",
    "recheck",
    "review_volume",
    "ledger_tidy",
];

async fn run_flow_request(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_path: &str,
    req: &FlowRequest,
    sink: EventSink,
    prop: ProposalSink,
) -> Result<String, String> {
    let work_dir = PathBuf::from(work_path);
    if !FLOW_KINDS.contains(&req.kind.as_str()) {
        return Err(format!(
            "未知流程类型 `{}`；可用：{}",
            req.kind,
            FLOW_KINDS.join(" / ")
        ));
    }
    let volume = req.volume.unwrap_or(1);
    let chapter = req.chapter.clone().unwrap_or_default();
    let need_chapter = matches!(
        req.kind.as_str(),
        "chapter_outline" | "chapter" | "ledger_update" | "consistency" | "deai" | "recheck"
    );
    if need_chapter && chapter.trim().is_empty() {
        return Err(format!("{} 需要章号参数（如 ch-003）", req.kind));
    }
    // 阶段门（现状 flows 不查门，门原先只拦 UI 导航；R4 起 run_flow 触发前补查）
    if let Some(stage) = flow_gate_stage(&req.kind) {
        let p = pipeline::load_pipeline(&work_dir)?;
        p.gate_check(stage, &work_dir)
            .map_err(|e| format!("{e}。可请作者在 PLAN 条确认/推进阶段后重试"))?;
    }

    match req.kind.as_str() {
        "chapter_outline" => {
            generate_artifact_flow(
                queue,
                work_path.to_string(),
                volume,
                chapter.clone(),
                "chapter_outline".to_string(),
                sink,
                prop,
            )
            .await?;
            Ok(format!("章纲已生成，提议已入审阅队列（{chapter}）"))
        }
        "chapter" => {
            generate_artifact_flow(
                queue,
                work_path.to_string(),
                volume,
                chapter.clone(),
                "chapter".to_string(),
                sink,
                prop,
            )
            .await?;
            Ok(format!("正文已生成，提议已入审阅队列（{chapter}）"))
        }
        "ledger_update" => {
            ledger_update_flow(queue, work_path.to_string(), volume, chapter.clone(), sink, prop)
                .await?;
            Ok("账本回写提议已提交（待审阅）".to_string())
        }
        "consistency" => {
            review_flow(queue, &work_dir, volume, &chapter, "consistency", &sink, prop).await?;
            let n = issues::list(&work_dir, Some(&chapter))
                .map(|l| l.len())
                .unwrap_or(0);
            Ok(format!(
                "一致性检查完成：{chapter} 问题清单现有 {n} 条；报告与清单已落盘（可用 list_issues 查看）"
            ))
        }
        "deai" => {
            review_flow(queue, &work_dir, volume, &chapter, "deai", &sink, prop).await?;
            Ok(format!("去 AI 味改写完成（{chapter}）；如有修改已提交提议"))
        }
        "recheck" => {
            recheck_flow(queue, work_path.to_string(), volume, chapter.clone(), sink, prop)
                .await?;
            let n = issues::list(&work_dir, Some(&chapter))
                .map(|l| l.len())
                .unwrap_or(0);
            Ok(format!("复检完成：{chapter} 问题清单共 {n} 条，复检结论已并入"))
        }
        "review_volume" => {
            review_volume_flow(queue, work_path.to_string(), volume, sink, prop).await?;
            Ok(format!("第 {volume} 卷批审完成，问题清单已更新"))
        }
        "ledger_tidy" => {
            tidy_flow(queue, work_path.to_string(), sink, prop).await?;
            Ok("账本整理完成，提议已入审阅队列".to_string())
        }
        _ => unreachable!("kind 已在 FLOW_KINDS 校验"),
    }
}

/// 生成章纲或正文。target: "chapter_outline" | "chapter"
pub async fn generate_artifact_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_path: String,
    volume: u32,
    chapter: String,
    target: String,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.clone();
    opts.stage = if target == "chapter_outline" {
        Stage::ChapterOutline
    } else {
        Stage::Chapters
    };
    // R3：应用 project.json 的装配偏好（上下文面板控制的档位/预算/组件开关）
    if let Ok(prefs) = pipeline::load_assembly_prefs(&work_dir) {
        if let Some(d) = &prefs.setting_detail {
            opts.setting_detail = match d.as_str() {
                "name_only" => crate::lore::Detail::NameOnly,
                "compact" => crate::lore::Detail::Compact,
                _ => crate::lore::Detail::Full,
            };
        }
        if let Some(b) = prefs.token_budget {
            opts.token_budget = b;
        }
        opts.exclude_components = prefs.exclude_components.clone();
    }

    // R3：超预算不再拦截（仅装配软提示）；生成照常进行，由用户按需调档位/预算
    let assembled = assembly::assemble(&work_dir, &opts)?;

    let (file_hint, stage_hint, expect) = if target == "chapter_outline" {
        (
            assembly::chapter_outline_path(volume, &chapter),
            "撰写本章章纲（用 propose_write 提交，路径见下）",
            Some(agent::tools::WriteExpect {
                min_chars: 80,
                max_chars: 800,
                check_halfwidth: true,
            }),
        )
    } else {
        (
            assembly::chapter_path(volume, &chapter),
            "撰写本章正文全文（用 propose_write 提交，路径见下）",
            Some(agent::tools::WriteExpect {
                min_chars: 1500,
                max_chars: 3800,
                check_halfwidth: true,
            }),
        )
    };

    let mut spec = writer_spec(&assembled.system);
    spec.sections.push((
        "task".to_string(),
        format!("{stage_hint}\n目标文件：{file_hint}"),
    ));

    let tool_ctx = ToolCtx::new(work_dir.clone(), Arc::clone(queue), on_proposal)
        .expect(expect)
        .event_sink(Some(Arc::clone(&on_event)));

    let prompt = format!(
        "{}\n\n# 输出文件\n{file_hint}\n\n请开始工作：完成后用 propose_write 提交全文。",
        assembly::render_prompt(&assembled)
    );

    let sess = Session::new(format!("gen-{target}-{volume:02}-{chapter}"));
    let sink = Arc::clone(&on_event);
    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| sink(ev),
    )
    .await?;
    let mut sess = sess;
    sess.messages = outcome.messages;
    sess.usage = outcome.usage;
    let _ = session::save_session(&SessionStore::Work(work_dir), &sess);
    Ok(())
}

/// 账本回写：读本章正文 + 账本 → ledger agent 提议更新。
pub async fn ledger_update_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_path: String,
    volume: u32,
    chapter: String,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let ch_path = assembly::chapter_path(volume, &chapter);
    if !work_dir.join(&ch_path).exists() {
        return Err(format!("本章正文不存在：{ch_path}（先完成正文生成与审阅）"));
    }
    let ledger_path = ledger::LEDGER_PATH;
    let prompt = format!(
        "第{volume}卷 {chapter} 已完稿。请先 read_file 读取正文 {ch_path} 与账本 {ledger_path}，\
         然后 propose_edit 提交更新后的账本全文（路径 {ledger_path}）。"
    );
    let spec = ledger_spec();
    let tool_ctx = ToolCtx::new(work_dir.clone(), Arc::clone(queue), on_proposal)
        .event_sink(Some(Arc::clone(&on_event)));
    run_and_save_session(
        format!("gen-ledger-{volume:02}-{chapter}"),
        &SessionStore::Work(work_dir),
        Some(&tool_ctx),
        &spec,
        &prompt,
        &on_event,
    )
    .await
}

/// 审校（一致性 / 去 AI 味）：预检 + reviewer + 报告结构化落盘。
pub async fn review_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_dir: &PathBuf,
    volume: u32,
    chapter: &str,
    mode: &str,
    on_event: &EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let settings = kernel::current_settings()?;
    let ch_path = assembly::chapter_path(volume, chapter);
    if !work_dir.join(&ch_path).exists() {
        return Err(format!("本章正文不存在：{ch_path}"));
    }

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.to_string();
    opts.stage = Stage::Review;
    let assembled = assembly::assemble(work_dir, &opts)?;

    // 代码级预检（零 token）：结果流式输出并注入审校
    let preflight_report = if mode != "deai" {
        let text = works::read_file(work_dir, &ch_path)?;
        let ledger = ledger::read_ledger(work_dir)?;
        preflight::preflight_cultivation(&text, &ledger)
    } else {
        Vec::new()
    };
    if !preflight_report.is_empty() {
        for line in &preflight_report {
            on_event(AgentEvent::TextDelta {
                text: format!("[代码预检] {line}\n"),
            });
        }
    }

    let mut spec = reviewer_spec(&assembled.system);
    let task = if mode == "deai" {
        format!(
            "对 {ch_path} 执行「去 AI 味」：按文风指令改写整章，用 propose_edit 提交整章全文（路径 {ch_path}）。"
        )
    } else {
        let preflight_block = if preflight_report.is_empty() {
            String::new()
        } else {
            format!(
                "\n代码预检已发现以下确定问题（直接核实并纳入修复）：\n{}\n",
                preflight_report
                    .iter()
                    .map(|s| format!("- {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        format!(
            "对 {ch_path} 执行「一致性检查」：先 read 正文与账本/设定，输出编号问题清单；\
             确定的硬伤用 propose_edit 提交修复（路径 {ch_path}），拿不准的只列清单。{preflight_block}"
        )
    };
    spec.sections.push(("task".to_string(), task));

    let tool_ctx = ToolCtx::new(work_dir.clone(), Arc::clone(queue), on_proposal)
        .event_sink(Some(Arc::clone(on_event)));
    let prompt = format!(
        "{}\n\n# 待审章节\n{ch_path}\n\n请开始审校。",
        assembly::render_prompt(&assembled)
    );
    let sink = Arc::clone(on_event);
    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| sink(ev),
    )
    .await?;
    let report_text = outcome.text;

    let sess = Session {
        messages: outcome.messages,
        usage: outcome.usage,
        last_usage: outcome.usage,
        ..Session::new(format!("review-{mode}-{volume:02}-{chapter}"))
    };
    let _ = session::save_session(&SessionStore::Work(work_dir.clone()), &sess);

    if mode == "consistency" && !report_text.trim().is_empty() {
        let report_dir = work_dir.join("review");
        let _ = std::fs::create_dir_all(&report_dir);
        let _ = std::fs::write(report_dir.join(format!("报告-{chapter}.md")), &report_text);
        let _ = issues::ingest_report(work_dir, chapter, "consistency", &report_text);
        for p in &preflight_report {
            let _ = issues::ingest_report(work_dir, chapter, "preflight", p);
        }
    }
    Ok(())
}

/// 复检：对照上轮问题清单与裁决记录逐项验证。
pub async fn recheck_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_path: String,
    volume: u32,
    chapter: String,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;
    let ch_path = assembly::chapter_path(volume, &chapter);
    if !work_dir.join(&ch_path).exists() {
        return Err(format!("本章正文不存在：{ch_path}"));
    }

    let all = issues::list(&work_dir, Some(&chapter))?;
    if all.is_empty() {
        return Err(format!("{chapter} 没有审校记录，请先执行一致性检查"));
    }
    let dossier = all
        .iter()
        .map(|i| {
            format!(
                "- [#{}][{}] {}\n  裁决：{}",
                i.id,
                i.status,
                i.text.replace('\n', " "),
                i.ruling.clone().unwrap_or_else(|| "（未裁决）".into())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.clone();
    opts.stage = Stage::Review;
    let assembled = assembly::assemble(&work_dir, &opts)?;

    let mut spec = reviewer_spec(&assembled.system);
    spec.sections.push(("task".to_string(), format!(
        "复检 {ch_path}：下面是上轮问题清单与作者裁决。逐项核验：\
         (a) 已修复项确认修复生效；(b) 未裁决(open)项若为硬伤给出修复提议，若不成立说明理由建议驳回；\
         (c) 输出「复检结论：通过/不通过 + 原因」。\n\n# 上轮问题清单\n{dossier}"
    )));

    let tool_ctx = ToolCtx::new(work_dir.clone(), Arc::clone(queue), on_proposal)
        .event_sink(Some(Arc::clone(&on_event)));
    let prompt = format!(
        "{}\n\n# 待复检章节\n{ch_path}\n\n请开始复检。",
        assembly::render_prompt(&assembled)
    );
    let sink = Arc::clone(&on_event);
    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| sink(ev),
    )
    .await?;
    if !outcome.text.trim().is_empty() {
        let _ = issues::ingest_report(&work_dir, &chapter, "recheck", &outcome.text);
    }
    Ok(())
}

/// 卷级批审：逐章一致性检查，进度经事件流汇报。
pub async fn review_volume_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_path: String,
    volume: u32,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let chapters_dir = work_dir
        .join(assembly::volume_dir(volume))
        .join("chapters");
    let mut chapters: Vec<String> = std::fs::read_dir(&chapters_dir)
        .map_err(|e| format!("读取章节目录失败：{e}"))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".md") && !name.contains("-章纲") {
                Some(name.trim_end_matches(".md").to_string())
            } else {
                None
            }
        })
        .collect();
    chapters.sort();
    if chapters.is_empty() {
        return Err(format!("第 {volume} 卷没有正文章节"));
    }

    on_event(AgentEvent::TextDelta {
        text: format!("[卷级批审] 共 {} 章：{}\n", chapters.len(), chapters.join("、")),
    });
    let mut failed = Vec::new();
    for ch in &chapters {
        on_event(AgentEvent::TextDelta {
            text: format!("\n[卷级批审] 开始 {ch} …\n"),
        });
        if let Err(e) = review_flow(queue, &work_dir, volume, ch, "consistency", &on_event, Arc::clone(&on_proposal)).await {
            failed.push(format!("{ch}: {e}"));
        }
    }
    let summary = if failed.is_empty() {
        format!("\n[卷级批审] 全部 {} 章完成，问题清单见右栏/裁决后复检。\n", chapters.len())
    } else {
        format!("\n[卷级批审] 完成，部分失败：{}\n", failed.join("；"))
    };
    on_event(AgentEvent::TextDelta { text: summary });
    Ok(())
}

/// 账本整理 pass。
pub async fn tidy_flow(
    queue: &Arc<Mutex<ReviewQueue>>,
    work_path: String,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let ledger_path = ledger::LEDGER_PATH;
    let spec = ledger_tidy_spec();
    let tool_ctx = ToolCtx::new(work_dir.clone(), Arc::clone(queue), on_proposal)
        .event_sink(Some(Arc::clone(&on_event)));
    let prompt = format!(
        "请整理账本 {ledger_path}：read 全文 → 按 ledger-tidy 规程整理 → propose_edit 提交整理后全文（路径 {ledger_path}）。"
    );
    run_and_save_session(
        "gen-ledger-tidy".to_string(),
        &SessionStore::Work(work_dir),
        Some(&tool_ctx),
        &spec,
        &prompt,
        &on_event,
    )
    .await
}

// ---------------------------------------------------------------------------
// 上下文总览（R3：上下文面板与窗口用量指示的数据源）

#[derive(Debug, Clone, serde::Serialize)]
pub struct ContextOverview {
    /// 会话全部轮次累计用量（成本参考；provider 计数）
    pub session_usage: Option<kernel::UsageReport>,
    /// 最近一轮输入 tokens（≈ 当前对话历史大小，窗口占用口径）
    pub last_input_tokens: Option<u64>,
    /// 最近一轮输入占窗口比例（0-100，两位小数；无会话轮次时为 0）
    pub session_percent: f64,
    /// 装配估算总量（D9 估算口径）
    pub assembly_tokens: u64,
    /// 装配估算是否超预算（R3 起仅软提示，不拦截）
    pub assembly_over_budget: bool,
    /// 实际生效档位（full/name_only/compact）
    pub applied_detail: String,
    /// 当前激活供应商的模型上下文窗口
    pub context_window: u64,
    /// 装配估算占窗口比例（0-100，两位小数）
    pub assembly_percent: f64,
}

/// 组装上下文总览：会话真实用量 + 装配估算 + 模型窗口。
pub fn context_overview(
    work_path: &str,
    session_id: Option<&str>,
    volume: u32,
    chapter: &str,
) -> Result<ContextOverview, String> {
    let work_dir = PathBuf::from(work_path);
    let sess = session_id
        .and_then(|id| session::load_session(&SessionStore::Work(work_dir.clone()), id).ok());
    let session_usage = sess.as_ref().and_then(|s| s.usage);
    let last_input_tokens = sess.as_ref().and_then(|s| s.last_usage).map(|u| u.input_tokens);

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.to_string();
    let assembled = assembly::assemble(&work_dir, &opts)?;

    let context_window = kernel::current_settings()
        .ok()
        .and_then(|s| s.active().map(|p| p.context_window))
        .unwrap_or(crate::settings::DEFAULT_CONTEXT_WINDOW)
        .max(1);

    let session_percent = last_input_tokens
        .map(|t| ((t as f64 / context_window as f64) * 10000.0).round() / 100.0)
        .unwrap_or(0.0);
    let assembly_percent =
        ((assembled.total_tokens as f64 / context_window as f64) * 10000.0).round() / 100.0;

    Ok(ContextOverview {
        session_usage,
        last_input_tokens,
        session_percent,
        assembly_tokens: assembled.total_tokens,
        assembly_over_budget: assembled.over_budget,
        applied_detail: assembled.applied_detail,
        context_window,
        assembly_percent,
    })
}
