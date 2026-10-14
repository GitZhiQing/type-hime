//! 作品目录文件监听（R13 · 资源树滞后修复）：notify 递归监听 → 过滤/防抖 → "work:changed"。
//! 覆盖应用外变更（外部编辑器 / 终端 git / 文件管理器）；应用内变更另有显式刷新链路，
//! watcher 的冗余触发经防抖合并后无感（不做写入抑制——复杂度不值）。
//! 忽略 .sessions/（每轮对话落盘会高频触发）与 .git/（内部变动频繁、不改变资源树）。

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

/// 防抖窗口：批量事件（保存 / git 操作 / 提议应用）合并为一次触发。
const DEBOUNCE: Duration = Duration::from_millis(400);

struct WatchState {
    work_dir: PathBuf,
    /// 保存 handle：drop 即停止底层监听
    _watcher: RecommendedWatcher,
}

static CURRENT: Mutex<Option<WatchState>> = Mutex::new(None);

/// 路径是否应忽略（会话存储 / git 内部——高频且不改变资源树）。
fn is_ignored(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == ".sessions" || s == ".git"
    })
}

/// 启动监听（幂等）：同作品重复调用不重启；换作品先停旧的。
/// `on_change` 在防抖窗口结束后于独立线程回调（发 "work:changed"）。
pub fn start<F>(work_dir: &Path, on_change: F) -> Result<(), String>
where
    F: Fn() + Send + 'static,
{
    let mut guard = CURRENT.lock().unwrap();
    if let Some(st) = guard.as_ref() {
        if st.work_dir == work_dir {
            return Ok(());
        }
    }
    *guard = None; // 停旧（drop watcher → 防抖线程随之退出）

    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res {
            // 事件路径全部落在忽略区 → 丢弃（如会话落盘）
            if ev.paths.iter().any(|p| !is_ignored(p)) {
                let _ = tx.send(());
            }
        }
    })
    .map_err(|e| format!("启动文件监听失败：{e}"))?;
    watcher
        .watch(work_dir, RecursiveMode::Recursive)
        .map_err(|e| format!("监听目录失败：{e}"))?;

    // 防抖线程：收到事件后吸收窗口内后续事件，窗口结束触发一次
    std::thread::spawn(move || {
        while rx.recv().is_ok() {
            loop {
                match rx.recv_timeout(DEBOUNCE) {
                    Ok(()) => continue, // 窗口内继续吸收
                    Err(mpsc::RecvTimeoutError::Timeout) => break,
                    Err(mpsc::RecvTimeoutError::Disconnected) => return, // watcher 已 drop
                }
            }
            on_change();
        }
    });

    *guard = Some(WatchState {
        work_dir: work_dir.to_path_buf(),
        _watcher: watcher,
    });
    Ok(())
}

/// 停止监听（作品页卸载时调用；drop watcher 使防抖线程自行退出）。
pub fn stop() {
    *CURRENT.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_sessions_and_git() {
        assert!(is_ignored(Path::new("/w/.sessions/s1.json")));
        assert!(is_ignored(Path::new("/w/.git/index")));
        assert!(!is_ignored(Path::new("/w/volumes/vol-01/chapters/ch-003.md")));
        assert!(!is_ignored(Path::new("/w/.gitignore")));
    }

    #[test]
    fn start_stop_lifecycle() {
        let dir = std::env::temp_dir().join(format!(
            "type-hime-watch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let hits = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let h = std::sync::Arc::clone(&hits);
        start(&dir, move || {
            h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();
        // 幂等：同路径再启动不报错
        start(&dir, || {}).unwrap();
        // 外部写文件 → 防抖后触发一次
        std::fs::write(dir.join("a.md"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(1200));
        assert!(hits.load(std::sync::atomic::Ordering::SeqCst) >= 1, "应触发变更回调");
        // 忽略区写入不触发（再写 .sessions，计数不变）
        let before = hits.load(std::sync::atomic::Ordering::SeqCst);
        std::fs::create_dir_all(dir.join(".sessions")).unwrap();
        std::fs::write(dir.join(".sessions/s.json"), "{}").unwrap();
        std::thread::sleep(Duration::from_millis(1000));
        assert_eq!(
            hits.load(std::sync::atomic::Ordering::SeqCst),
            before,
            ".sessions 变更不应触发"
        );
        stop();
        let _ = std::fs::remove_dir_all(&dir);
    }
}
