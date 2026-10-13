//! 工具注册表（内核插件化的一环）。
//! 工具 = 实现 rig Tool 的结构体；注册表按名字供给 AgentSpec 组装。
//! 红线：任何工具不得直接写盘——ProposeEdit/ProposeWrite 只入审阅队列，人审后才落盘。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rig::schemars::JsonSchema;
use rig::tool::{DynamicTool, IntoToolOutput, Tool, ToolContext, ToolExecutionError};
use serde::Deserialize;

use super::events::EventSink;
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

/// 工具构建上下文（提议队列共享 + 完成回调 + 写入期望 + 审批/事件通道）。
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
}

impl ToolCtx {
    /// 基础上下文（对话/审校等自由场景）：无写入校验、无审批、无事件出口。
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
        if let Some(expect) = &self.ctx.expect {
            if let Err(msg) = validate_content(&args.new_content, expect) {
                return Ok(msg);
            }
        }
        Ok(match crate::works::read_file(&self.ctx.work_root, &args.path) {
            Ok(old) => {
                let p = self
                    .ctx
                    .queue
                    .lock()
                    .unwrap()
                    .push(
                        self.ctx.work_root.to_string_lossy().into_owned(),
                        args.path.clone(),
                        &old,
                        args.new_content,
                        args.reason,
                        false,
                    );
                (self.ctx.on_proposal)(&p);
                format!(
                    "已生成修改提议 #{}（{}），等待作者在 diff 审阅中处理；在作者处理前不要重复提交同一修改。",
                    p.id, p.rel_path
                )
            }
            Err(e) => format!("[无法提议：读取 {path} 失败：{e}]", path = args.path),
        })
    }

    fn description(&self) -> String {
        "提议修改作品中的既有文件：给出修改后全文与理由，生成 diff 供作者人审。不会直接写盘——接受与否由作者决定。".to_string()
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
        let p = self.ctx.queue.lock().unwrap().push(
            self.ctx.work_root.to_string_lossy().into_owned(),
            args.path.clone(),
            "",
            args.content,
            args.reason,
            true,
        );
        (self.ctx.on_proposal)(&p);
        Ok(format!(
            "已生成新建提议 #{}（{}），等待作者在 diff 审阅中处理。",
            p.id, p.rel_path
        ))
    }

    fn description(&self) -> String {
        "提议新建文件（如章节、设定条目）：给出全文与理由，供作者人审。不会直接写盘。".to_string()
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

/// 执行超时（秒）：只读命令应短小；超时放弃等待（spawn_blocking 里子进程可能仍在后台，R4-② 补进程终止）。
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
        // 1) 守卫：写产物/链式/白名单（红线：写产物一律引导 propose_*）
        if let Err(msg) = crate::agent::bash::check_command(&args.command) {
            return Ok(format!("[命令被拦截] {msg}"));
        }
        // 2) 审批：变更前确认档挂起等 UI 决策（档位模型 R4-① 接入；Spike A 固定走审批路径）
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
        // 3) 执行（无 shell 直执行；阻塞进程放 spawn_blocking）
        let root = self.ctx.work_root.clone();
        let command = args.command.clone();
        let text = match tokio::time::timeout(
            std::time::Duration::from_secs(BASH_TIMEOUT_SECS),
            tokio::task::spawn_blocking(move || {
                crate::agent::bash::run_command(&root, &command, 8000)
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

    fn description(&self) -> String {
        "执行只读 shell 命令查看仓库状态与历史（白名单：git 只读子命令 status/log/diff/show/branch/rev-parse 等；管道仅限只读过滤命令 grep/head/tail 等）。写产物/改动历史的命令会被拦截——作品文件修改请用 propose_edit/propose_write。执行前需要作者确认。"
            .to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        rig::schemars::schema_for!(BashArgs).to_value()
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
            other => eprintln!("[agent] 未知工具名：{other}"),
        }
    }
    out
}
