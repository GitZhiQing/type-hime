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
    /// 子任务开始（R4：run_flow 触发长流程；前端渲染 Task 卡）
    SubTaskStart { id: String, label: String },
    /// 子任务文本增量（子流程的 TextDelta 转发）
    SubTaskDelta { id: String, text: String },
    /// 子任务内的工具调用（轨迹）
    SubTaskToolCall {
        id: String,
        name: String,
        args: String,
    },
    /// 子任务内的工具结果
    SubTaskToolResult {
        id: String,
        name: String,
        output: String,
    },
    /// 子任务结束（status: done | error；summary 为执行摘要）
    SubTaskEnd {
        id: String,
        status: String,
        summary: String,
    },
    /// 提议已被自动应用（R4：完全访问档；前端同步内联卡与审阅列表）
    ProposalApplied {
        id: u64,
        rel_path: String,
        result: String,
    },
    /// 长时目标状态更新（R4.5：设置/暂停/恢复/清除/轮末验证后；前端 GoalBar 数据源）
    GoalUpdate {
        goal: crate::agent::goal::SessionGoalView,
    },
}
