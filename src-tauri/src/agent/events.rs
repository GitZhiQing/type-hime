//! 内核统一事件（D8）：rig 流事件 → 应用层事件 → Tauri Channel。
//! 前端只认识这些事件，不感知 rig。

use std::sync::Arc;

use serde::Serialize;

/// 事件出口（Tauri Channel / Web SSE / 冒烟打印共用；flows 与工具层共享）。
pub type EventSink = Arc<dyn Fn(AgentEvent) + Send + Sync>;

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
    /// 工具请求人工审批（R4：bash 等；前端渲染 Confirmation 卡，工具挂起等待）
    ApprovalRequest {
        id: u64,
        tool: String,
        title: String,
        detail: String,
    },
    /// 审批已决策（允许/拒绝/超时；note 说明超时等例外情形）
    ApprovalResolved {
        id: u64,
        allowed: bool,
        note: Option<String>,
    },
}
