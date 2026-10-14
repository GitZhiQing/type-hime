//! 子任务事件桥（R4 Spike B）：run_flow 等长流程工具执行期间，
//! 把子流程的事件包装为 `SubTask*` 事件回流父会话（前端渲染 Task 卡）。
//!
//! 语义：子流程的 TextDelta/ToolCall/ToolResult 归属该子任务；子流程的 Final/Error
//! 不上抛（收尾由 SubTaskEnd 承载）；审批等其余事件透传（子流程工具也可触发审批）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::events::{AgentEvent, EventSink};

/// 子任务 id（进程内递增；并行子 Agent 各自唯一，见 R8-③）。
pub fn next_id() -> String {
    static N: AtomicU64 = AtomicU64::new(1);
    format!("st-{}", N.fetch_add(1, Ordering::SeqCst))
}

/// 包装事件出口：映射为带子任务 id 的事件（见模块说明）。
/// call_id 随事件透传（R8-③：并行子 Agent 同名工具的精确配对）。
pub fn wrap_sink(parent: &EventSink, id: &str) -> EventSink {
    let parent = Arc::clone(parent);
    let id = id.to_string();
    Arc::new(move |ev: AgentEvent| match ev {
        AgentEvent::TextDelta { text } => parent(AgentEvent::SubTaskDelta {
            id: id.clone(),
            text,
        }),
        AgentEvent::ToolCall {
            name,
            args,
            call_id,
        } => parent(AgentEvent::SubTaskToolCall {
            id: id.clone(),
            name,
            args,
            call_id,
        }),
        AgentEvent::ToolResult {
            name,
            output,
            call_id,
        } => parent(AgentEvent::SubTaskToolResult {
            id: id.clone(),
            name,
            output,
            call_id,
        }),
        // 子流程收尾由 SubTaskEnd 承载，不上抛
        AgentEvent::Final { .. } | AgentEvent::Error { .. } => {}
        other => parent(other),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn wrap_sink_maps_child_events() {
        let log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let l = Arc::clone(&log);
        let parent: EventSink = Arc::new(move |ev| l.lock().unwrap().push(format!("{ev:?}")));
        let child = wrap_sink(&parent, "st-9");

        child(AgentEvent::TextDelta { text: "hi".into() });
        child(AgentEvent::ToolCall {
            name: "read_file".into(),
            args: "{}".into(),
            call_id: Some("c1".into()),
        });
        child(AgentEvent::ToolResult {
            name: "read_file".into(),
            output: "ok".into(),
            call_id: Some("c1".into()),
        });
        child(AgentEvent::Final { output: "不该上抛".into() });
        child(AgentEvent::Error { message: "不该上抛".into() });
        child(AgentEvent::ApprovalRequest { id: 1, tool: "bash".into(), title: "t".into(), detail: "d".into() });

        let events = log.lock().unwrap().clone();
        assert_eq!(events.len(), 4, "Final/Error 应被抑制：{events:?}");
        assert!(events[0].contains("SubTaskDelta") && events[0].contains("st-9"));
        assert!(events[1].contains("SubTaskToolCall"));
        assert!(events[2].contains("SubTaskToolResult"));
        assert!(events[3].contains("ApprovalRequest"), "其余事件应透传");
    }

    #[test]
    fn ids_are_unique() {
        assert_ne!(next_id(), next_id());
    }
}
