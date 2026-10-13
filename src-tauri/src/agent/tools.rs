//! 工具注册表（内核插件化的一环）。
//! 工具 = 实现 rig Tool 的结构体；注册表按名字供给 AgentSpec 组装。
//! 红线：任何工具不得直接写盘——ProposeEdit/ProposeWrite 只入审阅队列，人审后才落盘。

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rig::schemars::JsonSchema;
use rig::tool::{Tool, ToolContext};
use serde::Deserialize;

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

/// 工具构建上下文（提议队列共享 + 完成回调 + 写入期望）。
#[derive(Clone)]
pub struct ToolCtx {
    pub work_root: PathBuf,
    pub queue: Arc<Mutex<ReviewQueue>>,
    /// 新提议产生时的回调（Tauri emit / 冒烟打印）
    pub on_proposal: Arc<dyn Fn(&Proposal) + Send + Sync>,
    /// 写入校验期望（None=不校验）
    #[allow(clippy::type_complexity)]
    pub expect: Option<WriteExpect>,
}

impl ToolCtx {
    /// 不带校验的上下文（对话/审校等自由场景）。
    pub fn new(
        work_root: PathBuf,
        queue: Arc<Mutex<ReviewQueue>>,
        on_proposal: Arc<dyn Fn(&Proposal) + Send + Sync>,
    ) -> Self {
        Self { work_root, queue, on_proposal, expect: None }
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
// 注册表

/// 按名字构建的工具实例（保持具体类型，配合 typestate builder 组装）。
pub enum ToolEntry {
    ReadFile(ReadFile),
    ProposeEdit(ProposeEdit),
    ProposeWrite(ProposeWrite),
}

pub fn build_tools(ctx: &ToolCtx, names: &[String]) -> Vec<ToolEntry> {
    let mut out = Vec::new();
    for name in names {
        match name.as_str() {
            "read_file" => out.push(ToolEntry::ReadFile(ReadFile {
                work_root: ctx.work_root.clone(),
            })),
            "propose_edit" => out.push(ToolEntry::ProposeEdit(ProposeEdit { ctx: ctx.clone() })),
            "propose_write" => out.push(ToolEntry::ProposeWrite(ProposeWrite { ctx: ctx.clone() })),
            other => eprintln!("[agent] 未知工具名：{other}"),
        }
    }
    out
}
