//! 会话内审批（R4 Spike A）：工具执行前挂起等 UI 决策。
//! 形态来源（D33）：goose 会话窗口 Allow/Deny 卡片、ZCode 工具权限块审批流；
//! 挂起机制：rig `Tool::call` 返回 Future，工具内 await oneshot 即可长期等待（已读源码确认）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::sync::oneshot;

use super::events::{AgentEvent, EventSink};

/// 审批等待上限（超时按拒绝处理，避免工具永久挂起）。
pub const APPROVAL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
}

/// 一条待决审批（前端 Confirmation 卡的数据源）。
#[derive(Debug, Clone, Serialize)]
pub struct ApprovalRequest {
    pub id: u64,
    /// 工具名（当前：bash）
    pub tool: String,
    /// 一句话标题（bash 为命令本身）
    pub title: String,
    /// 详情（工作目录、请求原因、风险说明）
    pub detail: String,
    pub created_at: String,
}

struct Pending {
    req: ApprovalRequest,
    tx: oneshot::Sender<Decision>,
}

/// 审批经纪人：请求登记 + UI 决策落地 + 超时清理。
/// 多工具并发（rig runner 可配置并发）时按 id 支持多条待决。
#[derive(Default)]
pub struct ApprovalBroker {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Pending>>,
}

impl ApprovalBroker {
    pub fn new() -> Self {
        Self::default()
    }

    /// 登记一条审批请求，返回请求体与决策接收端。
    pub fn register(
        &self,
        tool: &str,
        title: &str,
        detail: &str,
    ) -> (ApprovalRequest, oneshot::Receiver<Decision>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let req = ApprovalRequest {
            id,
            tool: tool.to_string(),
            title: title.to_string(),
            detail: detail.to_string(),
            created_at: crate::works::now_iso(),
        };
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap()
            .insert(id, Pending { req: req.clone(), tx });
        (req, rx)
    }

    /// UI 决策落地；返回是否命中待决请求（已被超时清理的返回 false）。
    pub fn resolve(&self, id: u64, decision: Decision) -> bool {
        match self.pending.lock().unwrap().remove(&id) {
            Some(p) => p.tx.send(decision).is_ok(),
            None => false,
        }
    }

    /// 移除待决（超时清理；随后同 id 的 resolve 返回 false）。
    pub fn cancel(&self, id: u64) {
        self.pending.lock().unwrap().remove(&id);
    }

    /// 当前待决清单（前端刷新/重连时恢复卡片）。
    pub fn list(&self) -> Vec<ApprovalRequest> {
        let mut v: Vec<_> = self
            .pending
            .lock()
            .unwrap()
            .values()
            .map(|p| p.req.clone())
            .collect();
        v.sort_by_key(|r| r.id);
        v
    }
}

/// Tauri/Web 双通道共享的经纪人句柄（镜像 SharedQueue 形态）。
pub struct SharedApproval(pub Arc<ApprovalBroker>);

impl SharedApproval {
    pub fn new() -> Self {
        Self(Arc::new(ApprovalBroker::new()))
    }
}

impl Default for SharedApproval {
    fn default() -> Self {
        Self::new()
    }
}

/// 请求审批并等待决策：发事件 → 等 oneshot（超时=拒绝）。
/// 无审批通道（broker 或 sink 缺失）时安全默认拒绝，绝不静默放行。
pub async fn request_approval(
    broker: Option<&Arc<ApprovalBroker>>,
    sink: Option<&EventSink>,
    tool: &str,
    title: &str,
    detail: &str,
) -> Decision {
    request_approval_with_timeout(broker, sink, tool, title, detail, APPROVAL_TIMEOUT).await
}

/// 带自定义超时的审批（测试用）。
pub async fn request_approval_with_timeout(
    broker: Option<&Arc<ApprovalBroker>>,
    sink: Option<&EventSink>,
    tool: &str,
    title: &str,
    detail: &str,
    timeout: Duration,
) -> Decision {
    let (Some(broker), Some(sink)) = (broker, sink) else {
        return Decision::Deny;
    };
    let (req, rx) = broker.register(tool, title, detail);
    let id = req.id;
    sink(AgentEvent::ApprovalRequest {
        id,
        tool: req.tool.clone(),
        title: req.title.clone(),
        detail: req.detail.clone(),
    });
    let (decision, note) = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(d)) => (d, None),
        Ok(Err(_)) => (Decision::Deny, Some("审批通道中断，按拒绝处理".to_string())),
        Err(_) => {
            broker.cancel(id);
            (Decision::Deny, Some("超时未决策，按拒绝处理".to_string()))
        }
    };
    sink(AgentEvent::ApprovalResolved {
        id,
        allowed: decision == Decision::Allow,
        note,
    });
    decision
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn sink_recording(log: Arc<Mutex<Vec<String>>>) -> EventSink {
        Arc::new(move |ev| {
            let desc = match ev {
                AgentEvent::ApprovalRequest { id, title, .. } => {
                    format!("request:{id}:{title}")
                }
                AgentEvent::ApprovalResolved { id, allowed, .. } => {
                    format!("resolved:{id}:{allowed}")
                }
                _ => "other".to_string(),
            };
            log.lock().unwrap().push(desc);
        })
    }

    #[tokio::test]
    async fn allow_resumes_tool() {
        let broker = Arc::new(ApprovalBroker::new());
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = sink_recording(Arc::clone(&log));
        let b2 = Arc::clone(&broker);
        let s2 = Arc::clone(&sink);
        let task = tokio::spawn(async move {
            request_approval_with_timeout(
                Some(&b2),
                Some(&s2),
                "bash",
                "git log",
                "目录：/w",
                Duration::from_secs(5),
            )
            .await
        });
        // 等请求登记后允许
        for _ in 0..100 {
            if !broker.list().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let pending = broker.list();
        assert_eq!(pending.len(), 1);
        assert!(broker.resolve(pending[0].id, Decision::Allow));
        assert_eq!(task.await.unwrap(), Decision::Allow);
        let events = log.lock().unwrap().clone();
        assert_eq!(events[0], "request:1:git log");
        assert_eq!(events[1], "resolved:1:true");
    }

    #[tokio::test]
    async fn deny_and_timeout() {
        let broker = Arc::new(ApprovalBroker::new());
        let log = Arc::new(Mutex::new(Vec::new()));
        let sink = sink_recording(Arc::clone(&log));

        // 拒绝
        let b2 = Arc::clone(&broker);
        let s2 = Arc::clone(&sink);
        let task = tokio::spawn(async move {
            request_approval_with_timeout(
                Some(&b2),
                Some(&s2),
                "bash",
                "git status",
                "",
                Duration::from_secs(5),
            )
            .await
        });
        for _ in 0..100 {
            if !broker.list().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let id = broker.list()[0].id;
        assert!(broker.resolve(id, Decision::Deny));
        assert_eq!(task.await.unwrap(), Decision::Deny);
        // 二次 resolve 不命中
        assert!(!broker.resolve(id, Decision::Allow));

        // 超时：清理待决 + 事件带 note
        let decision = request_approval_with_timeout(
            Some(&broker),
            Some(&sink),
            "bash",
            "git diff",
            "",
            Duration::from_millis(30),
        )
        .await;
        assert_eq!(decision, Decision::Deny);
        assert!(broker.list().is_empty(), "超时后待决应被清理");

        // 无通道 → 安全默认拒绝（且不 panic）
        assert_eq!(
            request_approval(None, Some(&sink), "bash", "x", "").await,
            Decision::Deny
        );
        assert_eq!(
            request_approval(Some(&broker), None, "bash", "x", "").await,
            Decision::Deny
        );
    }

    #[tokio::test]
    async fn concurrent_requests_are_id_keyed() {
        let broker = Arc::new(ApprovalBroker::new());
        let hits = Arc::new(AtomicUsize::new(0));
        let b1 = Arc::clone(&broker);
        let b2 = Arc::clone(&broker);
        let t1 = tokio::spawn(async move {
            let (_, rx) = b1.register("bash", "cmd-a", "");
            rx.await.unwrap_or(Decision::Deny)
        });
        let t2 = tokio::spawn(async move {
            let (_, rx) = b2.register("bash", "cmd-b", "");
            rx.await.unwrap_or(Decision::Deny)
        });
        for _ in 0..100 {
            if broker.list().len() == 2 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let pending = broker.list();
        assert_eq!(pending.len(), 2);
        // 分别决策互不影响
        assert!(broker.resolve(pending[0].id, Decision::Allow));
        assert!(broker.resolve(pending[1].id, Decision::Deny));
        hits.fetch_add(1, Ordering::SeqCst);
        assert_eq!(t1.await.unwrap(), Decision::Allow);
        assert_eq!(t2.await.unwrap(), Decision::Deny);
    }
}
