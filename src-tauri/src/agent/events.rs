//! 内核统一事件（D8）：rig 流事件 → 应用层事件 → Tauri Channel。
//! 前端只认识这些事件，不感知 rig。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// 文本增量
    TextDelta { text: String },
    /// 模型发起工具调用
    ToolCall { name: String, args: String },
    /// 工具执行结果（已回给模型）
    ToolResult { name: String, output: String },
    /// 运行结束，附最终文本
    Final { output: String },
    /// 运行失败
    Error { message: String },
}
