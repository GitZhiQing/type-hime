//! diff 审阅写入（D0-6 编辑层）：agent 以 patch 提议 → diff 视图 → 人审 → 应用/丢弃。
//! 提议队列在内存（应用重启即弃，与 agent 会话同生命周期）。

use std::path::Path;
use std::sync::Mutex;

use serde::Serialize;
use similar::TextDiff;

#[derive(Debug, Clone, Serialize)]
pub struct Proposal {
    pub id: u64,
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
    pub created_at: String,
}

#[derive(Default)]
pub struct ReviewQueue {
    counter: u64,
    queue: Vec<Proposal>,
}

impl ReviewQueue {
    pub fn push(
        &mut self,
        rel_path: String,
        old_content: &str,
        new_content: String,
        reason: String,
        is_new_file: bool,
    ) -> Proposal {
        self.counter += 1;
        let diff = unified_diff(old_content, &new_content, &rel_path);
        let p = Proposal {
            id: self.counter,
            rel_path,
            diff,
            reason,
            new_content,
            is_new_file,
            created_at: crate::works::now_iso(),
        };
        self.queue.push(p.clone());
        p
    }

    pub fn list(&self) -> Vec<Proposal> {
        self.queue.clone()
    }

    pub fn take(&mut self, id: u64) -> Option<Proposal> {
        let idx = self.queue.iter().position(|p| p.id == id)?;
        Some(self.queue.remove(idx))
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

/// Tauri 共享状态包装（工具与命令层共享同一队列）。
#[derive(Clone, Default)]
pub struct SharedQueue(pub std::sync::Arc<Mutex<ReviewQueue>>);

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
    fn queue_take_apply_roundtrip() {
        let dir = std::env::temp_dir().join(format!("type-hime-review-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("outline")).unwrap();
        std::fs::write(dir.join("outline/总纲.md"), "旧总纲\n").unwrap();
        let _ = std::process::Command::new("git").current_dir(&dir).args(["init"]).output();

        let mut q = ReviewQueue::default();
        let p = q.push(
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
