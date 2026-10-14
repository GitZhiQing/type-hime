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
    // 保存前重读磁盘上的并发可变字段（R4.5-③）：本轮运行期间，直调命令可能已改会话档案
    // （暂停/清除目标、切权限档位、重命名标题）——load 的是轮初快照，不能覆盖这些变化。
    if let Ok(fresh) = session::load_session(store, &sess_id) {
        sess.goal = fresh.goal;
        sess.permission_mode = fresh.permission_mode;
        sess.title = fresh.title;
    }
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
    // /goal 指令分流（R4.5-② · D31）：控制子命令（暂停/恢复/清除/状态）不进 LLM；
    // 设置 = 写会话档案 + 本轮 prompt 追加目标指令块（验证器循环在 R4.5-③ 接入）。
    let mut prompt = prompt;
    match agent::goal::parse_goal_command(&prompt) {
        agent::goal::GoalCommand::NotGoal => {}
        agent::goal::GoalCommand::Set(objective) => {
            let store = SessionStore::Work(work_dir.clone());
            let mut sess = session::load_or_new_session(&store, &session_id)?;
            let goal = agent::goal::SessionGoal::new(objective.clone());
            let view = goal.view();
            sess.goal = Some(goal); // 设置即替换：旧目标（若有）直接被替换
            session::save_session(&store, &sess)?;
            on_event(AgentEvent::GoalUpdate { goal: view });
            prompt = format!("{prompt}\n\n{}", agent::goal::format_goal_directive(&objective));
        }
        agent::goal::GoalCommand::Status => {
            let store = SessionStore::Work(work_dir.clone());
            let sess = session::load_or_new_session(&store, &session_id)?;
            if let Some(goal) = sess.goal.as_ref().map(|g| g.view()) {
                on_event(AgentEvent::GoalUpdate { goal });
            }
            on_event(AgentEvent::Final {
                output: agent::goal::summarize(sess.goal.as_ref()),
            });
            return Ok(());
        }
        cmd => {
            // Pause | Resume | Clear → 控制动作（非法迁移的 Err 文案面向作者，不按运行失败处理）
            let action = match cmd {
                agent::goal::GoalCommand::Pause => agent::goal::GoalControlAction::Pause,
                agent::goal::GoalCommand::Resume => agent::goal::GoalControlAction::Resume,
                _ => agent::goal::GoalControlAction::Clear,
            };
            let store = SessionStore::Work(work_dir.clone());
            // 恢复需要流式通道驱动续跑（R4.5-③）：翻状态 → 跑恢复轮 → 进入轮末循环
            if action == agent::goal::GoalControlAction::Resume {
                let (goal, text) = match session::update_goal(&store, &session_id, action) {
                    Ok((g, msg)) => (g, msg),
                    Err(msg) => {
                        on_event(AgentEvent::Final { output: msg });
                        return Ok(());
                    }
                };
                let goal = goal.expect("恢复成功必有目标");
                on_event(AgentEvent::GoalUpdate {
                    goal: goal.view(),
                });
                on_event(AgentEvent::Final { output: text });
                let last = goal
                    .iterations
                    .last()
                    .and_then(|it| it.verification.as_ref());
                let (reason, next) = match last {
                    Some(v) if !v.passed => (
                        v.reason.clone(),
                        v.next_action
                            .clone()
                            .unwrap_or_else(|| "继续完成目标的剩余要求。".to_string()),
                    ),
                    _ => (
                        "目标此前被暂停，从当前进度继续推进。".to_string(),
                        "继续完成目标的剩余要求。".to_string(),
                    ),
                };
                let round_prompt = agent::goal::format_continuation_prompt(
                    &goal.objective,
                    goal.iterations.len() as u32,
                    &goal.budget,
                    goal.tokens_used,
                    &reason,
                    &next,
                );
                on_event(AgentEvent::GoalRoundStart {
                    iteration: goal.iterations.len() as u32 + 1,
                    title: next,
                });
                run_goal_round(
                    queue,
                    approval.clone(),
                    &work_dir,
                    &work_path,
                    &session_id,
                    &round_prompt,
                    &on_event,
                    &on_proposal,
                )
                .await?;
                return goal_loop(
                    queue,
                    approval,
                    work_dir,
                    session_id,
                    round_prompt,
                    on_event,
                    on_proposal,
                )
                .await;
            }
            let (goal, text) = session::update_goal(&store, &session_id, action)
                .map(|(g, msg)| (g, msg))
                .unwrap_or_else(|msg| (None, msg));
            if let Some(goal) = goal.as_ref().map(|g| g.view()) {
                on_event(AgentEvent::GoalUpdate { goal });
            }
            on_event(AgentEvent::Final { output: text });
            return Ok(());
        }
    }
    // slash 指令化（R4-⑥）：/命令 参数 → 结构化指令块（附在消息尾，原命令留在消息首供展示）
    let prompt = match slash_instruction(&prompt) {
        Some(block) => format!("{prompt}\n\n{block}"),
        None => prompt,
    };
    let spec = chat_spec_for_prompt(&prompt, &work_dir);
    let store = SessionStore::Work(work_dir.clone());
    let tool_ctx = chat_tool_ctx(
        queue,
        approval.clone(),
        work_dir.clone(),
        work_path.clone(),
        &session_id,
        &on_event,
        &on_proposal,
    )?;
    // R4.5-③：目标活跃（本轮设置或既有）才有轮末循环；Paused/终态会话的普通消息正常聊天
    let goal_active = session::load_session(&store, &session_id)
        .ok()
        .and_then(|s| s.goal)
        .is_some_and(|g| g.status == agent::goal::GoalStatus::Active);
    run_and_save_session(session_id.clone(), &store, Some(&tool_ctx), &spec, &prompt, &on_event)
        .await?;
    autotitle(&store, &session_id).await;
    if goal_active {
        goal_loop(
            queue,
            approval,
            work_dir,
            session_id,
            prompt,
            on_event,
            on_proposal,
        )
        .await?;
    }
    Ok(())
}

/// 全局对话（rail「对话」页）：无作品上下文、无文件工具、无提议。
pub async fn chat_flow_global(
    session_id: String,
    prompt: String,
    on_event: EventSink,
) -> Result<(), String> {
    // 长时目标是作品会话能力（R4.5）：全局对话无作品上下文
    if !matches!(
        agent::goal::parse_goal_command(&prompt),
        agent::goal::GoalCommand::NotGoal
    ) {
        on_event(AgentEvent::Final {
            output: "长时目标是作品会话能力：请打开作品后在会话中使用 /goal。".to_string(),
        });
        return Ok(());
    }
    let spec = global_chat_spec();
    let store = SessionStore::Global;
    run_and_save_session(session_id.clone(), &store, None, &spec, &prompt, &on_event).await?;
    autotitle(&store, &session_id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// 长时目标轮末循环（R4.5-③ · D31）：轮末独立完成验证器 + 迭代推进
// 迭代仅由验证器边界推进；暂停/清除在轮间生效（读改写会话档案，并发以磁盘为准）。

/// chat spec 构建：技能动态挂载（默认 writer-basics + slash 强制 + 关键词路由）
/// + 子 Agent 团队清单注入（R8-③：根 Agent 需要知道可委派谁）。
fn chat_spec_for_prompt(prompt: &str, work_dir: &std::path::Path) -> AgentSpec {
    let mut extra = agent::skills::route_skills(prompt, Some(work_dir));
    if let Some(s) = agent::spec::slash_forced_skill(prompt) {
        extra.push(s.to_string());
    }
    let mut spec = agent::spec::chat_spec_with_skills(&extra);
    if let Some(team) = agent::profiles::render_team_listing() {
        spec.sections.push(("team".to_string(), team));
    }
    spec
}

/// chat 工具上下文构建（每轮重建：档位/覆盖表随会话档案与 project.json 变化）。
fn chat_tool_ctx(
    queue: &Arc<Mutex<ReviewQueue>>,
    approval: Option<Arc<agent::approval::ApprovalBroker>>,
    work_dir: PathBuf,
    work_path: String,
    session_id: &str,
    on_event: &EventSink,
    on_proposal: &ProposalSink,
) -> Result<ToolCtx, String> {
    let runner = chat_flow_runner(Arc::clone(queue), work_path.clone(), Arc::clone(on_proposal));
    // 权限上下文（R4-①）：会话档位（存会话档案）+ 作品覆盖表（project.json）
    let store = SessionStore::Work(work_dir.clone());
    let session_mode = session::load_session(&store, session_id)
        .map(|s| s.permission_mode)
        .unwrap_or_default();
    let perm_config = pipeline::load_permission_config(&work_dir).unwrap_or_default();
    let permission = crate::permissions::PermissionContext::new(session_mode, perm_config);
    // 子 Agent 调度器（R8-③）：继承父会话权限/审批；子会话独立、事件经 SubTask* 回流
    let spawner = agent_spawner(
        Arc::clone(queue),
        approval.clone(),
        permission.clone(),
        work_dir.clone(),
        work_path,
        Arc::clone(on_proposal),
    );
    Ok(ToolCtx::new(work_dir, Arc::clone(queue), Arc::clone(on_proposal))
        .approval(approval)
        .event_sink(Some(Arc::clone(on_event)))
        .flow_runner(Some(runner))
        .agent_spawner(Some(spawner))
        .permission(Some(permission)))
}

// ---------------------------------------------------------------------------
// 子 Agent 调度（R8-③ · 多 Agent 团队）：spawn_agent 端口实现
// 形态来源 ZCode：profile→AgentSpec、独立子会话、事件桥、权限继承、递归硬禁。

/// 子 Agent 调度器：加载 profile → 构建 spec（工具面 ∩ 注册表 − spawn_agent）→
/// 独立子会话运行 → 返回给根 Agent 的摘要（最终文本 + 用量 + 提议计数）。
pub fn agent_spawner(
    queue: Arc<Mutex<ReviewQueue>>,
    approval: Option<Arc<agent::approval::ApprovalBroker>>,
    permission: crate::permissions::PermissionContext,
    work_dir: PathBuf,
    work_path: String,
    on_proposal: ProposalSink,
) -> agent::tools::AgentSpawner {
    Arc::new(move |req: agent::tools::AgentSpawnRequest, sink: EventSink| {
        let queue = Arc::clone(&queue);
        let approval = approval.clone();
        let permission = permission.clone();
        let work_dir = work_dir.clone();
        let work_path = work_path.clone();
        let prop = Arc::clone(&on_proposal);
        Box::pin(async move {
            run_subagent(&queue, approval, permission, &work_dir, &work_path, &req, sink, prop).await
        })
    })
}

/// 全部已实现工具名（profile.tools 的白名单基集；spawn_agent 递归硬禁）。
const TOOL_WHITELIST: &[&str] = &[
    "read_file",
    "propose_edit",
    "propose_write",
    "bash",
    "get_ledger",
    "list_issues",
    "list_lore",
    "check_visibility_leak",
    "preflight_chapter",
    "record_issues",
    "get_pipeline",
];

/// 子 Agent 工具面解析：空 profile.tools = 通用集；显式列表 = 白名单；
/// 永远剔除 spawn_agent（子 Agent 不可递归派生——ZCode 同款硬禁）。
fn resolve_subagent_tools(profile: &agent::profiles::AgentProfile) -> Vec<String> {
    let base: Vec<String> = if profile.tools.is_empty() {
        TOOL_WHITELIST.iter().map(|s| s.to_string()).collect()
    } else {
        profile
            .tools
            .iter()
            .filter(|t| TOOL_WHITELIST.contains(&t.as_str()))
            .cloned()
            .collect()
    };
    base.into_iter().filter(|t| t != "spawn_agent").collect()
}

/// 子 Agent 的一轮执行。
#[allow(clippy::too_many_arguments)]
async fn run_subagent(
    queue: &Arc<Mutex<ReviewQueue>>,
    approval: Option<Arc<agent::approval::ApprovalBroker>>,
    permission: crate::permissions::PermissionContext,
    work_dir: &std::path::Path,
    _work_path: &str,
    req: &agent::tools::AgentSpawnRequest,
    sink: EventSink,
    on_proposal: ProposalSink,
) -> Result<String, String> {
    let profile = agent::profiles::load_profiles()
        .into_iter()
        .find(|p| p.name == req.subagent)
        .ok_or_else(|| format!("子 Agent {} 不存在（用管理页创建，或检查名称）", req.subagent))?;
    let settings = kernel::current_settings()?;
    let started = std::time::Instant::now();
    // 子会话：独立存档（一次性任务；st-id 唯一避免并行覆盖），usage 自动计入成本汇总
    let sid = format!("agent-{}-{}", req.subagent, agent::subtask::next_id());
    let store = SessionStore::Work(work_dir.to_path_buf());
    let spec = agent::spec::AgentSpec {
        id: profile.name.clone(),
        sections: vec![(
            "identity".to_string(),
            format!("{}\n\n{}", profile.body.trim(), SUBAGENT_COMMON_NOTES),
        )],
        tools: resolve_subagent_tools(&profile),
        skills: profile.skills.clone(),
        model_override: profile.model.clone(),
        max_turns: profile.max_turns.max(1),
        tool_concurrency: 1,
        parallel_tool_calls: false,
    };
    let tool_ctx = ToolCtx::new(
        work_dir.to_path_buf(),
        Arc::clone(queue),
        Arc::clone(&on_proposal),
    )
    .approval(approval)
    .event_sink(Some(Arc::clone(&sink)))
    .permission(Some(permission));
    // 提议计数（摘要里告知根 Agent 有几件已入队等作者审阅）
    let before = queue.lock().unwrap().list().len();
    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &req.prompt,
        move |ev| sink(ev),
    )
    .await?;
    let sess = Session {
        messages: outcome.messages,
        last_usage: outcome.usage,
        ..Session::new(sid)
    };
    let _ = session::save_session(&store, &sess);
    let after = queue.lock().unwrap().list().len();
    let queued = after.saturating_sub(before);
    let secs = started.elapsed().as_secs();
    let tokens = outcome
        .usage
        .map(|u| u.input_tokens + u.output_tokens)
        .unwrap_or(0);
    // 摘要给根 Agent：最终文本（截断）+ 用量 + 提议计数（ZCode AgentCompletedOutput 同语义）
    let text: String = outcome.text.chars().take(8000).collect();
    let truncated = if outcome.text.chars().count() > 8000 {
        "\n…（子 Agent 输出过长已截断）"
    } else {
        ""
    };
    Ok(format!(
        "子 Agent「{}」（{}）完成任务。耗时 {secs}s，用量 ≈{tokens} tokens，新提议 {queued} 件（已入审阅队列等作者接受）。\n\n最终汇报：\n{text}{truncated}",
        profile.name, req.description
    ))
}

/// 子 Agent 公共约束（写死注入，profile 正文之外——ZCode Subagent Notes 同款）。
const SUBAGENT_COMMON_NOTES: &str = r#"## 子 Agent 公共约束
- 你的服务对象是根 Agent：结论、清单、要点直接写在最终回复里（根 Agent 只读你的文本）。
- 一切作品文件修改走 propose_edit / propose_write 提议（作者审阅后才落盘）；你无法直接写盘。
- 不要派生其他 Agent；超出任务范围的事项在回复里说明，交根 Agent 处理。
- 文件路径一律相对作品根（如 volumes/vol-01/chapters/ch-003.md）。"#;

/// 验证器基础设施失败时的 fail-open 产物（裁判链路故障 ≠ 目标未完成，放行不卡死）。
fn fail_open_verifier(reason: String) -> agent::goal::GoalVerifierOutput {
    agent::goal::GoalVerifierOutput {
        passed: true,
        reason,
        next_action: None,
    }
}

/// 轮末完成验证器（独立调用、无工具）；provider 失败重试 1 次，仍失败 fail-open 放行。
async fn run_goal_verifier(
    goal: &agent::goal::SessionGoal,
    iterations_done: u32,
    transcript: &str,
) -> agent::goal::GoalVerifierOutput {
    let settings = match kernel::current_settings() {
        Ok(s) => s,
        Err(e) => {
            return fail_open_verifier(format!("验证器无法取得模型配置（fail-open 放行）：{e}"))
        }
    };
    let prompt = agent::goal::format_verification_prompt(
        &goal.objective,
        goal.status,
        iterations_done,
        &goal.budget,
        goal.tokens_used,
        transcript,
    );
    let mut last_err = String::new();
    for _ in 0..2 {
        match kernel::one_shot_completion(&settings, agent::goal::VERIFIER_PREAMBLE, &prompt).await
        {
            Ok(out) => return agent::goal::parse_verifier_output(&out),
            Err(e) => last_err = e,
        }
    }
    fail_open_verifier(format!("验证器调用失败（fail-open 放行）：{last_err}"))
}

/// 目标续跑轮执行（goal_loop 与 /goal 恢复共用）：重建 spec/权限/工具上下文跑一轮。
async fn run_goal_round(
    queue: &Arc<Mutex<ReviewQueue>>,
    approval: Option<Arc<agent::approval::ApprovalBroker>>,
    work_dir: &PathBuf,
    work_path: &str,
    session_id: &str,
    prompt: &str,
    on_event: &EventSink,
    on_proposal: &ProposalSink,
) -> Result<(), String> {
    let store = SessionStore::Work(work_dir.clone());
    let spec = chat_spec_for_prompt(prompt, work_dir);
    let tool_ctx = chat_tool_ctx(
        queue,
        approval,
        work_dir.clone(),
        work_path.to_string(),
        session_id,
        on_event,
        on_proposal,
    )?;
    run_and_save_session(session_id.to_string(), &store, Some(&tool_ctx), &spec, prompt, on_event)
        .await
}

/// 轮末验证器循环（R4.5-③）：验证 → 边界判定 → 未过按 nextAction 续跑。
/// `last_round_prompt` = 刚跑完这一轮发给 agent 的指令（记入迭代档案）。
/// 并发一致性：轮中直调命令（goal_control）改会话档案，run_and_save_session 保存前
/// 会重读合并 goal/权限/标题；本循环落库走 mutate_goal 读改写，暂停/清除不会被覆盖。
async fn goal_loop(
    queue: &Arc<Mutex<ReviewQueue>>,
    approval: Option<Arc<agent::approval::ApprovalBroker>>,
    work_dir: PathBuf,
    session_id: String,
    last_round_prompt: String,
    on_event: EventSink,
    on_proposal: ProposalSink,
) -> Result<(), String> {
    let store = SessionStore::Work(work_dir.clone());
    let work_path = work_dir.to_string_lossy().to_string();
    let mut last_round_prompt = last_round_prompt;
    loop {
        // 1) 轮末重读（fresh）：并发暂停/清除以磁盘为准
        let sess = session::load_session(&store, &session_id)?;
        let Some(goal) = sess.goal.clone() else {
            return Ok(()); // 已清除
        };
        if goal.status != agent::goal::GoalStatus::Active {
            if goal.status == agent::goal::GoalStatus::Paused {
                on_event(AgentEvent::GoalUpdate {
                    goal: goal.view(),
                });
                on_event(AgentEvent::Final {
                    output: "长时目标已暂停：当前轮结束后停止迭代（/goal 恢复 继续）。".to_string(),
                });
            }
            return Ok(());
        }

        // 2) 完成验证器（独立调用、禁工具）
        let iterations_done = goal.iterations.len() as u32 + 1;
        let transcript = agent::goal::render_transcript(&sess.messages, 1200, 20000);
        let verdict = run_goal_verifier(&goal, iterations_done, &transcript).await;

        // 3) 记录迭代 + 应用边界（读改写：记录保留，状态仅在仍为 Active 时迁移）
        let tokens = sess
            .last_usage
            .map(|u| u.input_tokens + u.output_tokens)
            .unwrap_or(0);
        let boundary = agent::goal::boundary_decision(
            iterations_done,
            goal.tokens_used + tokens,
            &goal.budget,
            &verdict,
        );
        let record = agent::goal::GoalIteration {
            index: iterations_done,
            prompt: last_round_prompt.clone(),
            ended_at: Some(works::now_iso()),
            tokens,
            verification: Some(verdict.clone()),
        };
        let applied: agent::goal::BoundaryApply = session::mutate_goal(&store, &session_id, |g| {
            Ok(agent::goal::apply_boundary(g, boundary, record, tokens))
        })?;
        let fresh_view = session::load_session(&store, &session_id)
            .ok()
            .and_then(|s| s.goal.map(|g| g.view()));

        match applied {
            agent::goal::BoundaryApply::Continue(next_action) => {
                if let Some(goal) = fresh_view {
                    on_event(AgentEvent::GoalUpdate { goal });
                }
                on_event(AgentEvent::GoalRoundStart {
                    iteration: iterations_done + 1,
                    title: next_action.clone(),
                });
                last_round_prompt = agent::goal::format_continuation_prompt(
                    &goal.objective,
                    iterations_done,
                    &goal.budget,
                    goal.tokens_used + tokens,
                    &verdict.reason,
                    &next_action,
                );
                run_goal_round(
                    queue,
                    approval.clone(),
                    &work_dir,
                    &work_path,
                    &session_id,
                    &last_round_prompt,
                    &on_event,
                    &on_proposal,
                )
                .await?;
            }
            agent::goal::BoundaryApply::Complete => {
                if let Some(goal) = fresh_view {
                    on_event(AgentEvent::GoalUpdate { goal });
                }
                on_event(AgentEvent::Final {
                    output: format!("✅ 长时目标已完成\n验证结论：{}", verdict.reason),
                });
                return Ok(());
            }
            agent::goal::BoundaryApply::BudgetLimited => {
                if let Some(goal) = fresh_view {
                    on_event(AgentEvent::GoalUpdate { goal });
                }
                let remaining = verdict
                    .next_action
                    .as_deref()
                    .map(|n| format!("\n剩余工作：{n}"))
                    .unwrap_or_default();
                on_event(AgentEvent::Final {
                    output: format!(
                        "长时目标已达预算上限（轮数 {}/{} 或 token 上限），停止迭代。\n验证结论：{}{remaining}",
                        iterations_done, goal.budget.max_iterations, verdict.reason
                    ),
                });
                return Ok(());
            }
            agent::goal::BoundaryApply::PausedMidFlight => {
                if let Some(goal) = fresh_view {
                    on_event(AgentEvent::GoalUpdate { goal });
                }
                on_event(AgentEvent::Final {
                    output: "长时目标已暂停：本轮结果已记录，迭代停止（/goal 恢复 继续）。".to_string(),
                });
                return Ok(());
            }
            agent::goal::BoundaryApply::Cleared => return Ok(()),
        }
    }
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
    use super::*;

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

    /// /goal 控制子命令不进 LLM：改会话档案 + 发 GoalUpdate/Final 事件（R4.5-②）。
    #[test]
    fn goal_control_subcommands_without_llm() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let dir = std::env::temp_dir().join(format!(
                "type-hime-goal-flow-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            let store = SessionStore::Work(dir.clone());
            let sess_id = "goal-test";
            let mut sess = Session::new(sess_id.to_string());
            sess.goal = Some(agent::goal::SessionGoal::new("审校第一卷"));
            session::save_session(&store, &sess).unwrap();

            let queue = Arc::new(Mutex::new(ReviewQueue::in_memory()));
            let events = Arc::new(Mutex::new(Vec::<AgentEvent>::new()));
            let sink: EventSink = {
                let events = Arc::clone(&events);
                Arc::new(move |ev| events.lock().unwrap().push(ev))
            };
            let call = |prompt: &str, sink: EventSink| {
                chat_flow(
                    &queue,
                    None,
                    dir.to_string_lossy().to_string(),
                    sess_id.to_string(),
                    prompt.to_string(),
                    sink,
                    Arc::new(|_| {}),
                )
            };

            // 暂停：状态落档 + GoalUpdate + Final
            call("/goal 暂停", Arc::clone(&sink)).await.unwrap();
            {
                let evs = events.lock().unwrap();
                assert!(
                    evs.iter().any(|e| matches!(e, AgentEvent::GoalUpdate { goal }
                        if goal.status == agent::goal::GoalStatus::Paused))
                );
                assert!(evs
                    .iter()
                    .any(|e| matches!(e, AgentEvent::Final { output } if output.contains("暂停"))));
            }
            assert_eq!(
                session::load_session(&store, sess_id).unwrap().goal.unwrap().status,
                agent::goal::GoalStatus::Paused
            );

            // 重复暂停：非法迁移 → Final 文案（不是运行失败）
            events.lock().unwrap().clear();
            call("/goal pause", Arc::clone(&sink)).await.unwrap();
            assert!(events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, AgentEvent::Final { output } if output.contains("暂停状态"))));

            // 恢复 → Active（R4.5-③ 起恢复会驱动续跑循环＝LLM 路径，不经 chat_flow 测试；
            // 状态迁移已由 session::update_goal 单测覆盖，循环行为由 goal_spike/浏览器验收覆盖）
            session::update_goal(&store, sess_id, agent::goal::GoalControlAction::Resume)
                .unwrap();
            assert_eq!(
                session::load_session(&store, sess_id).unwrap().goal.unwrap().status,
                agent::goal::GoalStatus::Active
            );

            // 裸 /goal：状态摘要（含目标与轮次）
            events.lock().unwrap().clear();
            call("/goal", Arc::clone(&sink)).await.unwrap();
            assert!(events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, AgentEvent::Final { output } if output.contains("审校第一卷"))));

            // 清除 → goal None
            call("/goal 清除", Arc::clone(&sink)).await.unwrap();
            assert!(session::load_session(&store, sess_id).unwrap().goal.is_none());

            // 全局对话拒绝 /goal：Final 提示（不落任何 LLM/会话）
            events.lock().unwrap().clear();
            chat_flow_global(
                "global-x".to_string(),
                "/goal 你好".to_string(),
                Arc::clone(&sink),
            )
            .await
            .unwrap();
            assert!(events
                .lock()
                .unwrap()
                .iter()
                .any(|e| matches!(e, AgentEvent::Final { output } if output.contains("作品"))));

            let _ = std::fs::remove_dir_all(&dir);
        });
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
