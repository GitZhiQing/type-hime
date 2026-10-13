//! 工具注册表（内核插件化的一环）。
//! 工具 = 实现 rig Tool 的结构体；注册表按名字供给 AgentSpec 组装。
//! 红线：任何工具不得直接写盘——写操作只能走"提议→人审"通道（切片 5 落地）。

use std::path::PathBuf;

use rig::schemars::JsonSchema;
use rig::tool::{Tool, ToolContext};
use serde::Deserialize;

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
        // 失败不作为 Err 抛出：把错误文本作为结果回给模型，让它自行调整
        Ok(match crate::works::read_file(&self.work_root, &args.path) {
            Ok(content) => {
                if content.len() > 60_000 {
                    // 头部保留（frontmatter 与主要设定在开头），UTF-8 安全截断
                    let mut end = 60_000;
                    while !content.is_char_boundary(end) {
                        end -= 1;
                    }
                    format!("{}…\n[内容超长，已截断]", &content[..end])
                } else {
                    content
                }
            }
            Err(e) => format!("[读取失败] {e}"),
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
// 注册表

/// 按名字构建的工具实例（保持具体类型，配合 typestate builder 组装）。
pub enum ToolEntry {
    ReadFile(ReadFile),
}

pub fn build_tools(work_root: &PathBuf, names: &[String]) -> Vec<ToolEntry> {
    let mut out = Vec::new();
    for name in names {
        match name.as_str() {
            "read_file" => out.push(ToolEntry::ReadFile(ReadFile {
                work_root: work_root.clone(),
            })),
            other => eprintln!("[agent] 未知工具名：{other}"),
        }
    }
    out
}
