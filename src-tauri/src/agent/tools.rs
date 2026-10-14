//! 工具注册表（内核插件化的一环）。
//! 工具 = 实现 rig Tool 的结构体；注册表按名字供给 AgentSpec 组装。
//! 红线：任何工具不得直接写盘——ProposeEdit/ProposeWrite 只入审阅队列，人审后才落盘。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rig::schemars::JsonSchema;
use rig::tool::{DynamicTool, IntoToolOutput, Tool, ToolContext, ToolExecutionError};
use serde::Deserialize;

use super::events::{AgentEvent, EventSink};
use crate::agent::approval::{self, ApprovalBroker, Decision};
use crate::review::{Proposal, ReviewQueue};

/// 写入内容的轻校验期望（工具即校验器：不合规返回错误让模型当场自改）。
#[derive(Debug, Clone)]
pub struct WriteExpect {
    pub min_chars: usize,
    pub max_chars: usize,
    /// 是否检查中文语境的半角标点
    pub check_halfwidth: bool,
}

/// 校验生成内容：字数区间 + 中文正文半角标点混用。
/// 返回 Err(提示文本) 时调用方应把提示作为工具结果返回给模型。
pub fn validate_content(content: &str, expect: &WriteExpect) -> Result<(), String> {
    let n = content.chars().count();
    if n < expect.min_chars {
        return Err(format!(
            "[校验未过] 全文仅 {n} 字，要求不少于 {} 字。请补足内容后重新提交完整全文。",
            expect.min_chars
        ));
    }
    if n > expect.max_chars {
        return Err(format!(
            "[校验未过] 全文 {n} 字，超出上限 {} 字。请精简后重新提交完整全文。",
            expect.max_chars
        ));
    }
    if expect.check_halfwidth {
        let chars: Vec<char> = content.chars().collect();
        let is_cjk = |c: char| ('\u{4E00}'..='\u{9FFF}').contains(&c);
        let mut hits = 0;
        for i in 1..chars.len() {
            let prev_cjk = is_cjk(chars[i - 1]);
            let cur_half = matches!(chars[i], ',' | ':' | ';' | '!' | '?');
            let next_ok = chars
                .get(i + 1)
                .map(|c| is_cjk(*c) || c.is_whitespace())
                .unwrap_or(true);
            if prev_cjk && cur_half && next_ok {
                hits += 1;
            }
        }
        if hits > 3 {
            return Err(format!(
                "[校验未过] 检测到 {hits} 处中文语句中的半角标点（, : ; ! ?），请统一为全角（，：；！？）后重新提交。"
            ));
        }
    }
    Ok(())
}

/// run_flow 的执行请求（工具 → 业务 flow 的端口类型；业务实现在 flows.rs）。
#[derive(Debug, Clone)]
pub struct FlowRequest {
    pub kind: String,
    pub volume: Option<u32>,
    pub chapter: Option<String>,
}

/// 子 Agent 执行器（R8-③）：由业务层（flows.rs）注入。
/// 工具收到 spawn_agent 参数后交给它：它负责 profile→AgentSpec 构建、独立子会话、
/// 事件桥（SubTask*）+ 权限继承，并返回给根 Agent 的摘要文本。
/// 出处：ZCode Agent 工具（description/prompt/subagent_type 三参形态）。
pub type AgentSpawner = Arc<
    dyn Fn(AgentSpawnRequest, EventSink) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, String>> + Send>,
        > + Send
        + Sync,
>;

/// spawn_agent 的执行请求（工具 → 业务层的端口类型）。
#[derive(Debug, Clone)]
pub struct AgentSpawnRequest {
    /// 子 Agent 名（profiles::load_profiles 的 name）
    pub subagent: String,
    /// 3-5 词任务描述（前端任务卡标题；ZCode 同语义）
    pub description: String,
    /// 任务全文（子 Agent 的首条输入）
    pub prompt: String,
}

/// flow 执行器：由业务层注入（chat_flow 提供），返回执行摘要。
/// 出处（REFACTOR A/R4）：run_flow 必须触发既有 flows 而非让 Agent 手工模拟，
/// 保住 issues.json 落盘、preflight 注入、装配注入等副作用闭环。
/// R8-③：chat 工具面退役 run_flow（改由 spawn_agent 委派），此端口保留供 flows 层与示例使用。
pub type FlowRunner = Arc<
    dyn Fn(
            FlowRequest,
            EventSink,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<String, String>> + Send>,
        > + Send
        + Sync,
>;

/// 工具构建上下文（提议队列共享 + 完成回调 + 写入期望 + 审批/事件/流程通道）。
#[derive(Clone)]
pub struct ToolCtx {
    pub work_root: PathBuf,
    pub queue: Arc<Mutex<ReviewQueue>>,
    /// 新提议产生时的回调（Tauri emit / 冒烟打印）
    pub on_proposal: Arc<dyn Fn(&Proposal) + Send + Sync>,
    /// 写入校验期望（None=不校验）
    #[allow(clippy::type_complexity)]
    pub expect: Option<WriteExpect>,
    /// 审批经纪人（R4：bash 等需审批工具挂起等 UI 决策；None=安全默认拒绝）
    pub approval: Option<Arc<ApprovalBroker>>,
    /// 事件出口（审批请求/子任务事件回流前端；None=非流式上下文）
    pub event_sink: Option<EventSink>,
    /// flow 执行器（R4：run_flow 工具；None=该上下文不支持流程编排）
    pub flow_runner: Option<FlowRunner>,
    /// 子 Agent 调度器（R8-③：spawn_agent 工具；None=该上下文不支持委派）
    pub agent_spawner: Option<AgentSpawner>,
    /// 权限上下文（R4：会话档位 + 作品覆盖表；None=按安全默认「变更前确认」处理）
    pub permission: Option<crate::permissions::PermissionContext>,
}

impl ToolCtx {
    /// 基础上下文（对话/审校等自由场景）：无写入校验、无审批、无事件出口、无流程编排。
    pub fn new(
        work_root: PathBuf,
        queue: Arc<Mutex<ReviewQueue>>,
        on_proposal: Arc<dyn Fn(&Proposal) + Send + Sync>,
    ) -> Self {
        Self {
            work_root,
            queue,
            on_proposal,
            expect: None,
            approval: None,
            event_sink: None,
            flow_runner: None,
            agent_spawner: None,
            permission: None,
        }
    }

    /// 设置写入校验期望。
    pub fn expect(mut self, expect: Option<WriteExpect>) -> Self {
        self.expect = expect;
        self
    }

    /// 设置审批经纪人。
    pub fn approval(mut self, approval: Option<Arc<ApprovalBroker>>) -> Self {
        self.approval = approval;
        self
    }

    /// 设置事件出口。
    pub fn event_sink(mut self, sink: Option<EventSink>) -> Self {
        self.event_sink = sink;
        self
    }

    /// 设置 flow 执行器。
    pub fn flow_runner(mut self, runner: Option<FlowRunner>) -> Self {
        self.flow_runner = runner;
        self
    }

    /// 设置子 Agent 调度器（R8-③）。
    pub fn agent_spawner(mut self, spawner: Option<AgentSpawner>) -> Self {
        self.agent_spawner = spawner;
        self
    }

    /// 设置权限上下文。
    pub fn permission(mut self, permission: Option<crate::permissions::PermissionContext>) -> Self {
        self.permission = permission;
        self
    }

    /// 解析本次操作的生效档位（无权限上下文时按安全默认「变更前确认」）。
    pub fn resolve_mode(
        &self,
        tool: &str,
        subject: Option<&str>,
    ) -> crate::permissions::PermissionMode {
        self.permission
            .as_ref()
            .map(|p| p.resolve(tool, subject))
            .unwrap_or_default()
    }

    /// 完全访问档自动应用提议（R4-③ · D30/D32）：requires_review 的提议永不自动。
    /// 返回 Some(工具回执) 表示已自动应用（或尝试后失败，提议仍在队列）。
    pub fn maybe_auto_apply(&self, p: &Proposal) -> Option<String> {
        let tool = if p.is_new_file {
            "propose_write"
        } else {
            "propose_edit"
        };
        let mode = self.resolve_mode(tool, Some(&p.rel_path));
        if mode != crate::permissions::PermissionMode::FullAccess || p.requires_review {
            return None;
        }
        let taken = self
            .queue
            .lock()
            .unwrap()
            .list()
            .into_iter()
            .find(|x| x.id == p.id)?;
        match crate::review::apply(&self.work_root, taken) {
            Ok(msg) => {
                // 成功后从队列移除
                self.queue.lock().unwrap().take(p.id);
                if let Some(sink) = &self.event_sink {
                    sink(AgentEvent::ProposalApplied {
                        id: p.id,
                        rel_path: p.rel_path.clone(),
                        result: msg.clone(),
                    });
                }
                Some(format!(
                    "已按「完全访问」档自动应用提议 #{}（{}）：{}。",
                    p.id, p.rel_path, msg
                ))
            }
            Err(e) => Some(format!(
                "[自动应用失败] 提议 #{} 仍在审阅队列，等待人工处理：{e}",
                p.id
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// ReadFile：agent 只读访问作品文件

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadFileArgs {
    /// 相对作品根的路径，如 outline/总纲.md
    pub path: String,
}

#[derive(Clone)]
pub struct ReadFile {
    work_root: PathBuf,
}

impl Tool for ReadFile {
    const NAME: &'static str = "read_file";
    type Error = std::convert::Infallible;
    type Args = ReadFileArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        Ok(match crate::works::read_file(&self.work_root, &args.path) {
            Ok(content) => {
                if content.len() > 60_000 {
                    let mut end = 60_000;
                    while !content.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}…\n[内容超长，已截断]", &content[..end])
                } else {
                    content
                }
            }
            Err(e) => format!(
                "[读取失败] {}：{e}。注意路径是相对作品根的正斜杠路径（如 volumes/vol-01/chapters/ch-001.md、ledger/账本.md、outline/总纲.md、settings/characters/<人物名>.md）。请确认实际存在的路径后重试，不要连续尝试不存在的路径。",
                args.path
            ),
        })
    }

    fn description(&self) -> String {
        "读取当前作品仓库中的文件（设定/大纲/章节/账本等，相对作品根路径）。只读，不能修改。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(ReadFileArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// ProposeEdit：提议修改既有文件（不写盘）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeEditArgs {
    /// 要修改的文件（相对作品根路径，必须已存在）
    pub path: String,
    /// 修改后的完整全文（不是片段；请基于 read_file 的最新内容整体改写）
    pub new_content: String,
    /// 一句话说明本次修改意图（展示给人审阅）
    pub reason: String,
}

#[derive(Clone)]
pub struct ProposeEdit {
    ctx: ToolCtx,
}

impl Tool for ProposeEdit {
    const NAME: &'static str = "propose_edit";
    type Error = std::convert::Infallible;
    type Args = ProposeEditArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        // 控制文件硬闸（D32）：权限/项目状态/会话存档不经提议通道改动
        if crate::permissions::protected_control_path(&args.path) {
            return Ok(format!(
                "[拒绝] {} 是应用控制文件（权限/项目状态/会话存档），不通过提议通道修改；如需调整请在界面操作。",
                args.path
            ));
        }
        if let Some(expect) = &self.ctx.expect {
            if let Err(msg) = validate_content(&args.new_content, expect) {
                return Ok(msg);
            }
        }
        Ok(match crate::works::read_file(&self.ctx.work_root, &args.path) {
            Ok(old) => {
                // 可见性硬闸（D32）：触及 visibility/open_at_vol 的提议永久留人（不自动应用）
                let requires_review =
                    crate::permissions::touches_visibility(&old, &args.new_content);
                let p = self
                    .ctx
                    .queue
                    .lock()
                    .unwrap()
                    .push_with_review_flag(
                        self.ctx.work_root.to_string_lossy().into_owned(),
                        args.path.clone(),
                        &old,
                        args.new_content,
                        args.reason,
                        false,
                        requires_review,
                    );
                (self.ctx.on_proposal)(&p);
                if let Some(receipt) = self.ctx.maybe_auto_apply(&p) {
                    return Ok(receipt);
                }
                format!(
                    "已生成修改提议 #{}（{}）{}，等待作者在 diff 审阅中处理；在作者处理前不要重复提交同一修改。",
                    p.id,
                    p.rel_path,
                    if requires_review { "（含可见性变更，必须人工审阅）" } else { "" }
                )
            }
            Err(e) => format!("[无法提议：读取 {path} 失败：{e}]", path = args.path),
        })
    }

    fn description(&self) -> String {
        "提议修改作品中的既有文件：给出修改后全文与理由，生成 diff 供作者人审。不会直接写盘——接受与否由作者决定。project.json/permissions.json/.sessions 等控制文件不可修改。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(ProposeEditArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// ProposeWrite：提议新建文件（不写盘）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProposeWriteArgs {
    /// 新文件路径（相对作品根，如 volumes/vol-01/chapters/ch-001.md）
    pub path: String,
    /// 文件全文
    pub content: String,
    /// 一句话说明创建意图（展示给人审阅）
    pub reason: String,
}

#[derive(Clone)]
pub struct ProposeWrite {
    ctx: ToolCtx,
}

impl Tool for ProposeWrite {
    const NAME: &'static str = "propose_write";
    type Error = std::convert::Infallible;
    type Args = ProposeWriteArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        // 控制文件硬闸（D32）
        if crate::permissions::protected_control_path(&args.path) {
            return Ok(format!(
                "[拒绝] {} 是应用控制文件（权限/项目状态/会话存档），不通过提议通道创建；如需调整请在界面操作。",
                args.path
            ));
        }
        if let Some(expect) = &self.ctx.expect {
            if let Err(msg) = validate_content(&args.content, expect) {
                return Ok(msg);
            }
        }
        let exists = self.ctx.work_root.join(&args.path).exists();
        if exists {
            return Ok(format!(
                "[无法新建：{} 已存在；请改用 propose_edit]",
                args.path
            ));
        }
        // 可见性硬闸（D32）：新文件带 visibility/open_at_vol frontmatter 视同可见性设置
        let requires_review = crate::permissions::touches_visibility("", &args.content);
        let p = self.ctx.queue.lock().unwrap().push_with_review_flag(
            self.ctx.work_root.to_string_lossy().into_owned(),
            args.path.clone(),
            "",
            args.content,
            args.reason,
            true,
            requires_review,
        );
        (self.ctx.on_proposal)(&p);
        if let Some(receipt) = self.ctx.maybe_auto_apply(&p) {
            return Ok(receipt);
        }
        Ok(format!(
            "已生成新建提议 #{}（{}）{}，等待作者在 diff 审阅中处理。",
            p.id,
            p.rel_path,
            if requires_review { "（含可见性设置，必须人工审阅）" } else { "" }
        ))
    }

    fn description(&self) -> String {
        "提议新建文件（如章节、设定条目）：给出全文与理由，供作者人审。不会直接写盘。project.json/permissions.json/.sessions 等控制文件不可创建。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(ProposeWriteArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// Bash：只读命令（白名单 + 写产物拦截 + 变更前确认档逐条审批；R4）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BashArgs {
    /// 要执行的只读命令（白名单：git 只读子命令 status/log/diff/show 等；可用管道接只读过滤命令 grep/head/tail 等）
    pub command: String,
    /// 一句话说明为什么要执行（展示给作者确认）
    pub reason: String,
}

#[derive(Clone)]
pub struct Bash {
    ctx: ToolCtx,
}

/// 执行超时（秒）：只读命令应短小；超时放弃等待（子进程可能仍在后台——已知限制，
/// 需要强杀须引入进程组/Job Object，成本高于收益，暂不做）。
const BASH_TIMEOUT_SECS: u64 = 30;

impl Tool for Bash {
    const NAME: &'static str = "bash";
    type Error = std::convert::Infallible;
    type Args = BashArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        // 1) 守卫：写产物/链式/白名单（内置 + permissions.json 两层扩展；红线：一律引导 propose_*）
        let policy = crate::agent::bash::load_policy(&self.ctx.work_root);
        if let Err(msg) = crate::agent::bash::check_command(&args.command, &policy) {
            return Ok(format!("[命令被拦截] {msg}"));
        }
        // 2) 权限：变更前确认档挂起等 UI 决策；完全访问档白名单直通（R4-① 档位模型；
        //    覆盖表可按命令模式覆盖，如「git log* → 完全访问」）
        let mode = self.ctx.resolve_mode("bash", Some(&args.command));
        if mode == crate::permissions::PermissionMode::FullAccess {
            return self.execute_command(args.command, policy).await;
        }
        let detail = format!(
            "工作目录：{}\n请求原因：{}",
            self.ctx.work_root.display(),
            args.reason
        );
        let decision = approval::request_approval(
            self.ctx.approval.as_ref(),
            self.ctx.event_sink.as_ref(),
            "bash",
            &args.command,
            &detail,
        )
        .await;
        if decision != Decision::Allow {
            return Ok(format!(
                "[用户未批准] 未执行：{}。不要重复请求同一命令；如确有必要，请先向作者说明必要性。",
                args.command
            ));
        }
        self.execute_command(args.command, policy).await
    }

    fn description(&self) -> String {
        "执行只读 shell 命令查看仓库状态与历史（白名单：git 只读子命令 status/log/diff/show/branch/rev-parse 等；管道仅限只读过滤命令 grep/head/tail 等）。写产物/改动历史的命令会被拦截——作品文件修改请用 propose_edit/propose_write。执行前需要作者确认（完全访问档直通）。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(BashArgs).to_value()
    }
}

impl Bash {
    /// 执行（无 shell 直执行；阻塞进程放 spawn_blocking，30s 超时）。
    async fn execute_command(
        &self,
        command: String,
        policy: crate::agent::bash::BashPolicy,
    ) -> Result<String, std::convert::Infallible> {
        let root = self.ctx.work_root.clone();
        let text = match tokio::time::timeout(
            std::time::Duration::from_secs(BASH_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || {
                crate::agent::bash::run_command(&root, &command, 8000, &policy)
            }),
        )
        .await
        {
            Ok(Ok(Ok(out))) => out,
            Ok(Ok(Err(e))) => format!("[命令执行失败] {e}"),
            Ok(Err(e)) => format!("[命令执行失败] 任务异常：{e}"),
            Err(_) => format!(
                "[命令超时] 超过 {BASH_TIMEOUT_SECS} 秒未完成，已放弃等待（子进程可能仍在后台）。"
            ),
        };
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// RunFlow：触发既有流水线流程（章纲/正文/回写/审校…；子任务事件回流）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunFlowArgs {
    /// 流程类型：chapter_outline（章纲）/ chapter（正文）/ ledger_update（账本回写）/ consistency（一致性检查）/ deai（去 AI 味）/ recheck（复检）/ review_volume（卷级批审）/ ledger_tidy（账本整理）
    pub kind: String,
    /// 卷号（章级流程必填；审卷为该卷号）
    pub volume: Option<u32>,
    /// 章号（如 ch-003；章级流程必填）
    pub chapter: Option<String>,
}

/// 流程类型的中文标签（子任务卡标题用）。
fn describe_flow(kind: &str, volume: Option<u32>, chapter: Option<&str>) -> String {
    let name = match kind {
        "chapter_outline" => "生成章纲",
        "chapter" => "生成正文",
        "ledger_update" => "账本回写",
        "consistency" => "一致性检查",
        "deai" => "去 AI 味",
        "recheck" => "复检",
        "review_volume" => "卷级批审",
        "ledger_tidy" => "账本整理",
        other => other,
    };
    let at = match (volume, chapter) {
        (Some(v), Some(c)) if !c.is_empty() => format!("第{v}卷 {c}"),
        (Some(v), _) => format!("第{v}卷"),
        _ => String::new(),
    };
    if at.is_empty() {
        name.to_string()
    } else {
        format!("{name} · {at}")
    }
}

#[derive(Clone)]
pub struct RunFlow {
    ctx: ToolCtx,
}

impl Tool for RunFlow {
    const NAME: &'static str = "run_flow";
    type Error = std::convert::Infallible;
    type Args = RunFlowArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let (Some(sink), Some(runner)) = (self.ctx.event_sink.as_ref(), self.ctx.flow_runner.as_ref())
        else {
            return Ok("[流程不可用] 当前上下文没有流程执行通道（仅作品会话内的创作助手支持 run_flow）。"
                .to_string());
        };
        let label = describe_flow(&args.kind, args.volume, args.chapter.as_deref());
        let id = super::subtask::next_id();
        sink(AgentEvent::SubTaskStart {
            id: id.clone(),
            label: label.clone(),
            agent: None,
        });
        let child = super::subtask::wrap_sink(sink, &id);
        let result = runner(
            FlowRequest {
                kind: args.kind.clone(),
                volume: args.volume,
                chapter: args.chapter.clone(),
            },
            child,
        )
        .await;
        let (status, summary) = match result {
            Ok(summary) => ("done", summary),
            Err(e) => ("error", e),
        };
        sink(AgentEvent::SubTaskEnd {
            id: id.clone(),
            status: status.to_string(),
            summary: summary.clone(),
        });
        Ok(match status {
            "done" => format!("[子任务完成] {label}：{summary}"),
            _ => format!("[子任务失败] {label}：{summary}"),
        })
    }

    fn description(&self) -> String {
        "触发写作流水线流程（与界面按钮同源，副作用闭环保持：装配注入/零 token 预检/问题清单落盘/提议入审阅队列）。\
         kind 取值：chapter_outline（章纲）/ chapter（正文）/ ledger_update（账本回写）/ consistency（一致性检查）/ \
         deai（去 AI 味）/ recheck（复检）/ review_volume（卷级批审）/ ledger_tidy（账本整理）。\
         章级流程需给 volume 与 chapter（如 ch-003）；触发前校验阶段门。\
         不要用 read_file/propose_* 手工模拟这些流程。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(RunFlowArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// 查询与零 token 检查工具（R4-④：替代 read_file 读 json/md 的绕路，给模型结构化语义）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetLedgerArgs {}

/// 账本结构化查询（角色状态/已交代事实/伏笔/时间线）。
#[derive(Clone)]
pub struct GetLedgerTool {
    work_root: PathBuf,
}

impl Tool for GetLedgerTool {
    const NAME: &'static str = "get_ledger";
    type Error = std::convert::Infallible;
    type Args = GetLedgerArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        _args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        Ok(match crate::ledger::read_ledger(&self.work_root) {
            Ok(l) => serde_json::to_string_pretty(&l).unwrap_or_else(|_| "[账本序列化失败]".into()),
            Err(e) => format!("[读取账本失败] {e}"),
        })
    }

    fn description(&self) -> String {
        "读取连续性账本的结构化视图：角色状态 / 已交代事实 / 伏笔登记 / 时间线（每项含标题、正文、最近更新章）。只读。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(GetLedgerArgs).to_value()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListIssuesArgs {
    /// 只列某章的问题（如 ch-003）；省略=全部
    #[serde(default)]
    pub chapter: Option<String>,
}

/// 审校问题清单查询（含裁决状态与复检档案）。
#[derive(Clone)]
pub struct ListIssuesTool {
    work_root: PathBuf,
}

impl Tool for ListIssuesTool {
    const NAME: &'static str = "list_issues";
    type Error = std::convert::Infallible;
    type Args = ListIssuesArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        Ok(
            match crate::issues::list(&self.work_root, args.chapter.as_deref()) {
                Ok(list) if list.is_empty() => "[问题清单为空]".to_string(),
                Ok(list) => serde_json::to_string_pretty(&list)
                    .unwrap_or_else(|_| "[清单序列化失败]".into()),
                Err(e) => format!("[读取问题清单失败] {e}"),
            },
        )
    }

    fn description(&self) -> String {
        "查询审校问题清单（结构化）：编号/章节/问题文本/来源/裁决状态（open/fixed/dismissed/needs_ruling）/裁决记录。只读。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(ListIssuesArgs).to_value()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListLoreArgs {
    /// 名称/类型过滤关键词（如「林晚」或「character」）；省略=全部可见条目
    #[serde(default)]
    pub filter: Option<String>,
    /// 当前创作卷号（决定「未到开放卷」的可见性判定；默认 1）
    #[serde(default)]
    pub volume: Option<u32>,
}

/// 设定库查询（只列 prompt 可见条目：隐藏/未到开放卷的条目对 Agent 不存在）。
#[derive(Clone)]
pub struct ListLoreTool {
    work_root: PathBuf,
}

impl Tool for ListLoreTool {
    const NAME: &'static str = "list_lore";
    type Error = std::convert::Infallible;
    type Args = ListLoreArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let volume = args.volume.unwrap_or(1);
        Ok(match crate::lore::list_settings(&self.work_root) {
            Ok(all) => {
                let hidden = all
                    .iter()
                    .filter(|m| !crate::lore::is_prompt_visible(m, volume))
                    .count();
                let filter = args.filter.unwrap_or_default();
                let visible: Vec<serde_json::Value> = all
                    .iter()
                    .filter(|m| crate::lore::is_prompt_visible(m, volume))
                    .filter(|m| {
                        filter.is_empty()
                            || m.name.contains(&filter)
                            || m.kind.contains(&filter)
                    })
                    .map(|m| {
                        serde_json::json!({
                            "name": m.name,
                            "kind": m.kind,
                            "path": m.path,
                        })
                    })
                    .collect();
                let mut out = serde_json::json!({ "visible": visible }).to_string();
                if hidden > 0 {
                    out.push_str(&format!(
                        "\n（另有 {hidden} 条隐藏或未到开放卷的条目：对当前创作不可见也不可读，不要引用其内容。）"
                    ));
                }
                out
            }
            Err(e) => format!("[读取设定库失败] {e}"),
        })
    }

    fn description(&self) -> String {
        "列出设定库中的可见条目（名称/类型/路径），可用 filter 过滤、volume 指定当前卷。只读；隐藏与未到开放卷的条目不返回。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(ListLoreArgs).to_value()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmptyArgs {}

/// 关联泄漏检查（零 token）：可见条目是否大段复述隐藏条目。
#[derive(Clone)]
pub struct CheckLeakTool {
    work_root: PathBuf,
}

impl Tool for CheckLeakTool {
    const NAME: &'static str = "check_visibility_leak";
    type Error = std::convert::Infallible;
    type Args = EmptyArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        _args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        Ok(match crate::lore::check_visibility_leak(&self.work_root) {
            Ok(warnings) if warnings.is_empty() => {
                "泄漏检查通过：可见条目未复述隐藏设定。".to_string()
            }
            Ok(warnings) => format!(
                "泄漏检查发现 {} 处疑似复述：\n{}",
                warnings.len(),
                warnings.join("\n")
            ),
            Err(e) => format!("[泄漏检查失败] {e}"),
        })
    }

    fn description(&self) -> String {
        "零 token 检查：可见条目内容是否大段复述隐藏条目（4-gram 重叠）。只读，不发模型请求。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(EmptyArgs).to_value()
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PreflightChapterArgs {
    pub volume: u32,
    /// 章号（如 ch-003）
    pub chapter: String,
}

/// 修为层级代码预检（零 token）。
#[derive(Clone)]
pub struct PreflightChapterTool {
    work_root: PathBuf,
}

impl Tool for PreflightChapterTool {
    const NAME: &'static str = "preflight_chapter";
    type Error = std::convert::Infallible;
    type Args = PreflightChapterArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let ch_path = crate::assembly::chapter_path(args.volume, &args.chapter);
        Ok(
            match crate::works::read_file(&self.work_root, &ch_path) {
                Ok(text) => match crate::ledger::read_ledger(&self.work_root) {
                    Ok(ledger) => {
                        let problems = crate::preflight::preflight_cultivation(&text, &ledger);
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
            },
        )
    }

    fn description(&self) -> String {
        "零 token 检查：代码比对本章正文中的修为描述与账本记录（炼气/筑基/金丹等层级），列出矛盾。只读，不发模型请求。".to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(PreflightChapterArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// 注册表

/// 把具体 Tool 包装为 DynamicTool（工具注册面收敛：按 spec.tools 名单动态构建，
/// 免去逐工具 typestate 链式 match 的膨胀；rig 0.44 原生 dynamic_tools API）。
pub fn erase<T>(tool: T) -> DynamicTool
where
    T: Tool + Clone + 'static,
    T::Args: 'static,
{
    let description = tool.description();
    let parameters = tool.parameters();
    DynamicTool::new_with_context(
        rig::message::ToolName::new(T::NAME).expect("工具名非空"),
        description,
        parameters,
        move |context: &mut ToolContext, args: serde_json::Value| {
            let tool = tool.clone();
            Box::pin(async move {
                let typed: T::Args = serde_json::from_value(args).map_err(|e| {
                    ToolExecutionError::invalid_args(format!("工具参数解析失败：{e}"))
                })?;
                let out = tool
                    .call(context, typed)
                    .await
                    .map_err(|e| tool.map_error(e))?;
                out.into_tool_output()
            })
        },
    )
}

/// 按名字构建工具实例（DynamicTool 列表）。
/// R8-③：run_flow 从 chat 工具面退役（改名 agent:run_flow 供示例/兼容调试显式请求，
/// 不被任何默认 spec 挂载）——普通路径由 spawn_agent 委派子 Agent 执行。
pub fn build_tools(ctx: &ToolCtx, names: &[String]) -> Vec<DynamicTool> {
    let mut out = Vec::new();
    for name in names {
        match name.as_str() {
            "read_file" => out.push(erase(ReadFile {
                work_root: ctx.work_root.clone(),
            })),
            "propose_edit" => out.push(erase(ProposeEdit { ctx: ctx.clone() })),
            "propose_write" => out.push(erase(ProposeWrite { ctx: ctx.clone() })),
            "bash" => out.push(erase(Bash { ctx: ctx.clone() })),
            // 保留实现（flows 层与示例可用），chat 默认工具面不再列它
            "run_flow" => out.push(erase(RunFlow { ctx: ctx.clone() })),
            "spawn_agent" => out.push(erase(SpawnAgent { ctx: ctx.clone() })),
            "git_commit" => out.push(erase(GitCommit { ctx: ctx.clone() })),
            "get_pipeline" => out.push(erase(GetPipelineTool {
                work_root: ctx.work_root.clone(),
            })),
            "record_issues" => out.push(erase(RecordIssuesTool {
                work_root: ctx.work_root.clone(),
            })),
            "get_ledger" => out.push(erase(GetLedgerTool {
                work_root: ctx.work_root.clone(),
            })),
            "list_issues" => out.push(erase(ListIssuesTool {
                work_root: ctx.work_root.clone(),
            })),
            "list_lore" => out.push(erase(ListLoreTool {
                work_root: ctx.work_root.clone(),
            })),
            "check_visibility_leak" => out.push(erase(CheckLeakTool {
                work_root: ctx.work_root.clone(),
            })),
            "preflight_chapter" => out.push(erase(PreflightChapterTool {
                work_root: ctx.work_root.clone(),
            })),
            other => eprintln!("[agent] 未知工具名：{other}"),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// SpawnAgent：委派子 Agent（R8-③ · 多 Agent 团队）
// 出处：ZCode Agent 工具（description 3-5 词 + prompt 任务全文 + subagent_type），
// 刻意不给调用级 model 参数（防历史 override 污染当前配置——ZCode 同款取舍）。

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SpawnAgentArgs {
    /// 子 Agent 名（见团队清单，如 general）
    pub subagent: String,
    /// 任务描述（3-5 个词，任务卡标题用），如「写第 7 章正文」
    pub description: String,
    /// 任务全文：目标、交付物、上下文要点、约束规程、输出要求（子 Agent 的唯一输入）
    pub prompt: String,
}

#[derive(Clone)]
pub struct SpawnAgent {
    ctx: ToolCtx,
}

impl Tool for SpawnAgent {
    const NAME: &'static str = "spawn_agent";
    type Error = std::convert::Infallible;
    type Args = SpawnAgentArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let (Some(sink), Some(spawner)) = (self.ctx.event_sink.as_ref(), self.ctx.agent_spawner.as_ref())
        else {
            return Ok("[委派不可用] 当前上下文没有子 Agent 调度通道（仅作品会话内的创作助手支持 spawn_agent）。".to_string());
        };
        let id = super::subtask::next_id();
        let label = format!("{} · {}", args.subagent, args.description);
        sink(AgentEvent::SubTaskStart {
            id: id.clone(),
            label: label.clone(),
            agent: Some(args.subagent.clone()),
        });
        let child = super::subtask::wrap_sink(sink, &id);
        let result = spawner(
            AgentSpawnRequest {
                subagent: args.subagent.clone(),
                description: args.description.clone(),
                prompt: args.prompt.clone(),
            },
            child,
        )
        .await;
        let (status, summary) = match result {
            Ok(s) => ("done", s),
            Err(e) => ("error", e),
        };
        sink(AgentEvent::SubTaskEnd {
            id: id.clone(),
            status: status.to_string(),
            summary: summary.clone(),
        });
        Ok(match status {
            "done" => format!("[子 Agent 完成] {label}\n{summary}"),
            _ => format!("[子 Agent 失败] {label}：{summary}"),
        })
    }

    fn description(&self) -> String {
        "委派专职子 Agent 执行任务（独立会话与工具面；一切落稿仍经提议审阅）。\
         委派前先把必要上下文（卷纲/章纲/账本要点/文风指令/文件路径）写进 prompt——子 Agent 看不到本会话历史。\
         多个独立任务要并行时：在**同一条回复里发多个 spawn_agent 调用**，它们会并发执行。\
         不要让两个子 Agent 并行修改同一个文件（顺序委派）。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(SpawnAgentArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// GitCommit：用户主动提交（R8-③ 决定：替代原 R6「阶段确认自动 commit」自动化）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GitCommitArgs {
    /// 提交说明（中文一句；作者要求提交时按其意图书写）
    pub message: String,
}

#[derive(Clone)]
pub struct GitCommit {
    ctx: ToolCtx,
}

impl Tool for GitCommit {
    const NAME: &'static str = "git_commit";
    type Error = std::convert::Infallible;
    type Args = GitCommitArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        // 与 bash 白名单解耦的显式提交通道；变更前确认档挂起等作者决策，完全访问档直通
        let mode = self.ctx.resolve_mode("git_commit", Some(&args.message));
        if mode != crate::permissions::PermissionMode::FullAccess {
            let detail = format!(
                "工作目录：{}\n提交说明：{}",
                self.ctx.work_root.display(),
                args.message
            );
            let decision = approval::request_approval(
                self.ctx.approval.as_ref(),
                self.ctx.event_sink.as_ref(),
                "git_commit",
                &args.message,
                &detail,
            )
            .await;
            if decision != Decision::Allow {
                return Ok("[未提交] 作者未批准本次提交。如仍有未提交变更，请先向作者确认。".to_string());
            }
        }
        let root = self.ctx.work_root.clone();
        let msg = args.message.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::git::commit_all(&root, &msg)?;
            crate::git::log_oneline(&root, 1)
        })
        .await;
        match result {
            Ok(Ok(log)) => Ok(format!(
                "[已提交] {}\n最近提交：{}",
                args.message,
                log.first().map(|s| s.as_str()).unwrap_or("-")
            )),
            Ok(Err(e)) => Ok(format!("[提交失败] {e}")),
            Err(e) => Ok(format!("[提交失败] 执行线程异常：{e}")),
        }
    }

    fn description(&self) -> String {
        "把当前工作区全部变更提交为一个 git 提交（本地仓库；不 push）。\
         仅在作者明确要求提交（或同意你的提交建议）时调用；一次提交聚焦一个主题，message 用中文一句话。\
         无未提交变更时为空操作。执行前作者需要确认（完全访问档直通）。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(GitCommitArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// GetPipeline：流水线状态查询（R8-③：run_flow 阶段硬门退役后的软门数据源）

#[derive(Clone)]
pub struct GetPipelineTool {
    work_root: PathBuf,
}

impl Tool for GetPipelineTool {
    const NAME: &'static str = "get_pipeline";
    type Error = std::convert::Infallible;
    type Args = EmptyArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        _args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        match crate::pipeline::load_pipeline(&self.work_root) {
            Ok(state) => {
                let view = state.to_view(&self.work_root);
                let mut s = format!("当前阶段：{}（已确认：", view.stage_label);
                if view.confirmed.is_empty() {
                    s.push_str("无）");
                } else {
                    let labels: Vec<String> = view
                        .gates
                        .iter()
                        .filter(|g| g.confirmed)
                        .map(|g| g.label.clone())
                        .collect();
                    s.push_str(&labels.join("、"));
                    s.push('）');
                }
                s.push_str("\n各阶段进入条件：");
                for g in &view.gates {
                    s.push_str(&format!(
                        "\n- {}：{}{}",
                        g.label,
                        if g.confirmed {
                            "已确认".to_string()
                        } else if g.enterable {
                            "可进入".to_string()
                        } else {
                            format!("未满足（缺：{}）", g.blocked_by.clone().unwrap_or_default())
                        },
                        if g.current { "（当前）" } else { "" }
                    ));
                }
                s.push_str("\n遵守流水线顺序：上游阶段未确认时先向作者说明，不要越过确认点。");
                Ok(s)
            }
            Err(e) => Ok(format!("[读取流水线失败] {e}")),
        }
    }

    fn description(&self) -> String {
        "查询六阶段流水线状态（设定→总纲→卷纲→章纲→正文→审校）与各阶段进入条件。\
         委派写作任务前用它确认上游已确认；阶段推进由作者在 PLAN 条上确认。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(EmptyArgs).to_value()
    }
}

// ---------------------------------------------------------------------------
// RecordIssues：审校报告结构化落盘（R8-③：承接 run_flow 的 issues.json 副作用闭环）

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordIssuesArgs {
    /// 章号（如 ch-003；卷级问题用 vol-01）
    pub chapter: String,
    /// 审校报告全文（编号问题清单；工具按编号拆条落盘）
    pub report: String,
    /// 来源（consistency | preflight | recheck；默认 consistency）
    pub source: Option<String>,
}

#[derive(Clone)]
pub struct RecordIssuesTool {
    work_root: PathBuf,
}

impl Tool for RecordIssuesTool {
    const NAME: &'static str = "record_issues";
    type Error = std::convert::Infallible;
    type Args = RecordIssuesArgs;
    type Output = String;

    async fn call(
        &self,
        _ctx: &mut ToolContext,
        args: Self::Args,
    ) -> Result<Self::Output, Self::Error> {
        let source = args.source.clone().unwrap_or_else(|| "consistency".to_string());
        match crate::issues::ingest_report(&self.work_root, &args.chapter, &source, &args.report) {
            Ok(n) => Ok(format!(
                "[问题清单已落盘] {chapter}：按编号拆出 {n} 条（来源 {source}）。作者可在问题清单制品中逐条裁决。",
                chapter = args.chapter
            )),
            Err(e) => Ok(format!("[落盘失败] {e}")),
        }
    }

    fn description(&self) -> String {
        "把审校报告（编号问题清单）结构化落盘到作品的问题清单（issues.json，进 git）。\
         编辑/审校类子 Agent 完成检查后由根 Agent 调用（或委派时要求子 Agent 调用）：先落盘，再汇报。\
         已落盘的问题由作者逐条裁决（fixed/dismissed），复检时用 list_issues 读取。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(RecordIssuesArgs).to_value()
    }
}
