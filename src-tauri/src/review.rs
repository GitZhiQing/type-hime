//! diff 审阅写入（D0-6 编辑层）：agent 以 patch 提议 → diff 视图 → 人审 → 应用/丢弃。
//! 提议队列持久化于 app data（应用重启恢复未审提议）。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use similar::TextDiff;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proposal {
    pub id: u64,
    /// 提议归属的作品绝对路径（跨作品队列过滤）
    #[serde(default)]
    pub work_path: String,
    /// 相对作品根路径
    pub rel_path: String,
    /// unified diff（行级）
    pub diff: String,
    /// 提议理由（agent 给出）
    pub reason: String,
    /// 新全文（应用时写入）
    pub new_content: String,
    /// 是否新建文件
    pub is_new_file: bool,
    /// 必须人工审阅（R4 · D32：触及可见性 frontmatter 的提议永不自动应用；
    /// 旧档无此字段按 false 读取）
    #[serde(default)]
    pub requires_review: bool,
    pub created_at: String,
}

/// 持久化文件（app data；应用重启恢复未审提议）。
fn persist_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("type-hime").join("proposals.json"))
}

fn persist(queue: &ReviewQueue) {
    if !queue.persist {
        return; // 内存态队列（examples/测试）：不写共享文件，避免跨进程覆盖应用队列
    }
    if let Some(path) = persist_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(raw) = serde_json::to_string(queue) {
            let _ = std::fs::write(path, raw);
        }
    }
}

fn restore() -> ReviewQueue {
    persist_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

#[derive(Default, Serialize, Deserialize)]
pub struct ReviewQueue {
    counter: u64,
    queue: Vec<Proposal>,
    /// 拒绝反馈（带评论的拒绝）：chat_stream 时注入对应作品的会话上下文，驱动 Agent 修改重提
    #[serde(default)]
    feedback: Vec<RejectionFeedback>,
    /// 是否持久化到 app data（应用进程 true；examples/测试用 `in_memory()` 关掉，
    /// 否则测试进程的队列会把共享的 proposals.json 覆盖成自己的状态——R4 验收实见：
    /// regression 运行清空了应用里问剑青云的待审提议）
    #[serde(default = "default_persist")]
    persist: bool,
}

fn default_persist() -> bool {
    true
}

/// 拒绝反馈记录（R2-③：拒绝带评论，评论回流 Agent 上下文）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectionFeedback {
    pub work_path: String,
    pub rel_path: String,
    pub comment: String,
    pub created_at: String,
}

impl ReviewQueue {
    /// 内存态队列（不落盘）：examples/测试进程用，避免覆盖应用进程的共享提议文件。
    pub fn in_memory() -> Self {
        Self {
            persist: false,
            ..Default::default()
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn push(
        &mut self,
        work_path: String,
        rel_path: String,
        old_content: &str,
        new_content: String,
        reason: String,
        is_new_file: bool,
    ) -> Proposal {
        self.push_with_review_flag(work_path, rel_path, old_content, new_content, reason, is_new_file, false)
    }

    /// 同 push，附带「必须人工审阅」标记（R4：可见性变更等永久留人提议）。
    #[allow(clippy::too_many_arguments)]
    pub fn push_with_review_flag(
        &mut self,
        work_path: String,
        rel_path: String,
        old_content: &str,
        new_content: String,
        reason: String,
        is_new_file: bool,
        requires_review: bool,
    ) -> Proposal {
        self.counter += 1;
        let diff = unified_diff(old_content, &new_content, &rel_path);
        let p = Proposal {
            id: self.counter,
            work_path,
            rel_path,
            diff,
            reason,
            new_content,
            is_new_file,
            requires_review,
            created_at: crate::works::now_iso(),
        };
        self.queue.push(p.clone());
        persist(self);
        p
    }

    pub fn list(&self) -> Vec<Proposal> {
        self.queue.clone()
    }

    pub fn take(&mut self, id: u64) -> Option<Proposal> {
        let idx = self.queue.iter().position(|p| p.id == id)?;
        let p = self.queue.remove(idx);
        persist(self);
        Some(p)
    }

    /// 拒绝提议；带非空评论时记录反馈（等待下次 chat_stream 注入）。
    pub fn reject_with_comment(
        &mut self,
        id: u64,
        comment: Option<String>,
    ) -> Option<Proposal> {
        let p = self.take(id)?;
        if let Some(c) = comment.filter(|c| !c.trim().is_empty()) {
            self.feedback.push(RejectionFeedback {
                work_path: p.work_path.clone(),
                rel_path: p.rel_path.clone(),
                comment: c.trim().to_string(),
                created_at: crate::works::now_iso(),
            });
            persist(self);
        }
        Some(p)
    }

    /// 取走该作品的未消费拒绝反馈（其他作品的保留）。
    pub fn take_feedback(&mut self, work_path: &str) -> Vec<RejectionFeedback> {
        if self.feedback.is_empty() {
            return Vec::new();
        }
        let (hit, rest): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.feedback)
                .into_iter()
                .partition(|f| f.work_path == work_path);
        self.feedback = rest;
        if !hit.is_empty() {
            persist(self);
        }
        hit
    }
}

/// 生成 unified diff（无 hunk 头上下文行数 3）。
pub fn unified_diff(old: &str, new: &str, rel_path: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut out = format!("--- a/{rel_path}\n+++ b/{rel_path}\n");
    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        out.push_str(&hunk.to_string());
    }
    if out == format!("--- a/{rel_path}\n+++ b/{rel_path}\n") {
        out.push_str("（无差异）\n");
    }
    out
}

/// 应用提议：写盘 + git commit（提议语义的提交消息）。
pub fn apply(work_dir: &Path, p: Proposal) -> Result<String, String> {
    let target = crate::works::ensure_writable(work_dir, &p.rel_path)?;
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建目录失败：{e}"))?;
    }
    std::fs::write(&target, &p.new_content)
        .map_err(|e| format!("写入 {} 失败：{e}", p.rel_path))?;
    let reason: String = p.reason.chars().take(60).collect();
    let msg = if p.is_new_file {
        format!("接受提议：新建 {}（{reason}）", p.rel_path)
    } else {
        format!("接受提议：修改 {}（{reason}）", p.rel_path)
    };
    crate::git::commit_all(work_dir, &msg)?;
    Ok(msg)
}

/// Tauri 共享状态包装（工具与命令层共享同一队列；启动时恢复持久化的未审提议）。
#[derive(Clone, Default)]
pub struct SharedQueue(pub std::sync::Arc<Mutex<ReviewQueue>>);

impl SharedQueue {
    pub fn restored() -> Self {
        Self(Arc::new(Mutex::new(restore())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_generation() {
        let d = unified_diff("第一行\n第二行\n第三行\n", "第一行\n改过的行\n第三行\n", "outline/总纲.md");
        assert!(d.contains("--- a/outline/总纲.md"));
        assert!(d.contains("-第二行"));
        assert!(d.contains("+改过的行"));

        // 无差异
        let same = unified_diff("a\n", "a\n", "x.md");
        assert!(same.contains("（无差异）"));
    }

    #[test]
    fn reject_with_comment_feedback_roundtrip() {
        let mut q = ReviewQueue::default();
        let p = q.push(
            "C:/works/a".into(),
            "outline/总纲.md".into(),
            "旧\n".into(),
            "新\n".into(),
            "理由".into(),
            false,
        );
        // 空评论拒绝：不留反馈
        q.reject_with_comment(p.id, Some("   ".into())).unwrap();
        assert!(q.take_feedback("C:/works/a").is_empty());

        // 带评论拒绝：反馈按作品取走，其他作品保留
        let p2 = q.push(
            "C:/works/a".into(),
            "outline/总纲.md".into(),
            "旧\n".into(),
            "新2\n".into(),
            "理由".into(),
            false,
        );
        let p3 = q.push(
            "C:/works/b".into(),
            "x.md".into(),
            "".into(),
            "y\n".into(),
            "理由".into(),
            true,
        );
        q.reject_with_comment(p2.id, Some("语气太现代，重写".into())).unwrap();
        q.reject_with_comment(p3.id, Some("另一作品的反馈".into())).unwrap();

        let fb = q.take_feedback("C:/works/a");
        assert_eq!(fb.len(), 1);
        assert_eq!(fb[0].comment, "语气太现代，重写");
        assert_eq!(fb[0].rel_path, "outline/总纲.md");
        // 二次取走为空（已消费），另一作品的仍在
        assert!(q.take_feedback("C:/works/a").is_empty());
        let fb_b = q.take_feedback("C:/works/b");
        assert_eq!(fb_b.len(), 1);
    }

    #[test]
    fn queue_take_apply_roundtrip() {
        let dir = std::env::temp_dir().join(format!("type-hime-review-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("outline")).unwrap();
        std::fs::write(dir.join("outline/总纲.md"), "旧总纲\n").unwrap();
        let _ = std::process::Command::new("git").current_dir(&dir).args(["init"]).output();

        let mut q = ReviewQueue::default();
        let p = q.push(
            dir.to_string_lossy().into_owned(),
            "outline/总纲.md".into(),
            "旧总纲\n",
            "新总纲\n".into(),
            "AI 修改总纲".into(),
            false,
        );
        assert_eq!(q.list().len(), 1);
        assert!(p.diff.contains("+新总纲"));

        let taken = q.take(p.id).unwrap();
        assert!(q.list().is_empty());
        apply(&dir, taken).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join("outline/总纲.md")).unwrap(),
            "新总纲\n"
        );
        // git 留痕
        let log = crate::git::log_oneline(&dir, 5).unwrap();
        assert!(!log.is_empty());
        assert!(log[0].contains("提议") || log[0].contains("编辑"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
