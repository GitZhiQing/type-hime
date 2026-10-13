//! 命令行 git 封装（D2）。
//! 作品目录即 git 仓库；所有落盘修改在接受时自动 commit。

use std::path::Path;
use std::process::Command;

/// 在指定目录执行 git 命令，成功返回 stdout，失败返回 stderr 消息。
fn run_git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .output()
        .map_err(|e| format!("无法启动 git（请确认已安装 git）：{e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(format!("git {} 失败：{}", args.join(" "), stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// 在 dir 下初始化仓库并设置仓库级身份（不依赖全局 git config）。
pub fn init_repo(dir: &Path) -> Result<(), String> {
    run_git(dir, &["init"])?;
    run_git(dir, &["config", "user.name", "type-hime"])?;
    run_git(dir, &["config", "user.email", "type-hime@local"])?;
    run_git(dir, &["config", "core.quotepath", "false"])?; // 中文文件名原样显示
    Ok(())
}

/// 暂存全部变更并提交；无变更时为空操作。
pub fn commit_all(dir: &Path, message: &str) -> Result<(), String> {
    let status = run_git(dir, &["status", "--porcelain"])?;
    if status.trim().is_empty() {
        return Ok(());
    }
    run_git(dir, &["add", "-A"])?;
    run_git(dir, &["commit", "-m", message])?;
    Ok(())
}

/// 单行历史，用于回溯展示。
pub fn log_oneline(dir: &Path, limit: usize) -> Result<Vec<String>, String> {
    let n = limit.to_string();
    let out = run_git(dir, &["log", "--oneline", "-n", &n])?;
    Ok(out.lines().map(|l| l.to_string()).collect())
}

/// 恢复文件：找到最近一次删除该文件的提交，从其父提交恢复；
/// 若删除未入历史（工作区删除未提交），则从 HEAD 恢复。
pub fn restore_file(dir: &Path, rel_path: &str) -> Result<(), String> {
    let out = run_git(
        dir,
        &["log", "--diff-filter=D", "--format=%H", "-n", "1", "--", rel_path],
    )?;
    let commit = out.trim();
    if commit.is_empty() {
        run_git(dir, &["checkout", "HEAD", "--", rel_path])?;
    } else {
        run_git(dir, &["checkout", &format!("{commit}~1"), "--", rel_path])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_commit_restore_roundtrip() {
        let dir = std::env::temp_dir().join(format!("type-hime-git-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        init_repo(&dir).unwrap();
        std::fs::write(dir.join("章一.md"), "第一章内容").unwrap();
        commit_all(&dir, "初始提交").unwrap();
        assert!(!log_oneline(&dir, 5).unwrap().is_empty());

        // 删除后恢复
        std::fs::remove_file(dir.join("章一.md")).unwrap();
        assert!(!dir.join("章一.md").exists());
        restore_file(&dir, "章一.md").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("章一.md")).unwrap(), "第一章内容");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
