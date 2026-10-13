//! bash 工具的命令守卫与执行（R4）。
//! Spike A 骨架：内置 git 只读白名单 + 写产物拦截 + 无 shell 直执行（不做字符串拼接进 shell，杜绝注入面）。
//! R4-② 扩展：白名单配置文件两层（全局 app data + 作品级）、登记 CLI、完整黑名单表。
//!
//! 红线（AGENTS.md）：任何写产物/改历史的命令一律拦截并引导 propose_edit/propose_write——
//! bash 永不绕过提议审阅通道。

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// 管道右侧白名单：只读过滤命令（内置起步集）。
const PIPE_FILTERS: &[&str] = &["grep", "rg", "head", "tail", "wc", "sort", "uniq", "cat"];

/// git 只读子命令白名单。
const GIT_READONLY: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "rev-parse",
    "ls-files",
    "blame",
    "describe",
    "shortlog",
];

/// `git branch` 的写操作参数（删/移/拷贝分支）——子命令在只读白名单内也要拦。
const GIT_BRANCH_WRITE_ARGS: &[&str] = &["-d", "-D", "-m", "-M", "-c", "-C", "--delete", "--move", "--copy"];

/// 写产物 / 间接写 / 改动历史模式：命中即拦截并引导 propose_*。
const DENY_PATTERNS: &[&str] = &[
    ">",
    "sed -i",
    "rm ",
    "rm\t",
    "mv ",
    "cp ",
    "tee ",
    "truncate",
    "dd ",
    "chmod",
    "chown",
    "touch ",
    "mkdir",
    "rmdir",
    "ln ",
    "shred",
    "git add",
    "git commit",
    "git reset",
    "git checkout",
    "git clean",
    "git rm",
    "git mv",
    "git push",
    "git pull",
    "git fetch",
    "git stash",
    "git apply",
    "git restore",
    "git switch",
    "git rebase",
    "git merge",
    "git tag",
    "git init",
    "git config",
];

/// 命令替换 / 链式 / 重定向：不被支持（起步保守，R4-② 再议放宽面）。
const CHAIN_PATTERNS: &[&str] = &["$(", "`", ";", "&&", "||", "&", "<", "\n"];

/// 守卫：写产物拦截 → 链式/替换拒绝 → 结构校验（白名单）。
pub fn check_command(command: &str) -> Result<(), String> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return Err("命令为空。".to_string());
    }
    for pat in DENY_PATTERNS {
        if cmd.contains(pat) {
            return Err(format!(
                "`{pat}` 属于写产物或改动历史的模式，禁止执行。作品文件的一切修改请使用 propose_edit / propose_write 工具提交提议（由作者在 diff 审阅中决定）。"
            ));
        }
    }
    for pat in CHAIN_PATTERNS {
        if cmd.contains(pat) {
            return Err(format!(
                "`{pat}`（命令替换/链式/重定向）不被支持；请提交单条只读命令（管道仅限只读过滤命令）。"
            ));
        }
    }
    if cmd.contains('"') || cmd.contains('\'') {
        return Err(
            "暂不支持引号参数；请改用不含引号的等价命令（如 `git log --oneline -5`）。".to_string(),
        );
    }
    parse_segments(cmd).map(|_| ())
}

/// 解析为管道分段并做白名单校验（空白分词；引号/转义已被前置守卫拒绝）。
pub fn parse_segments(cmd: &str) -> Result<Vec<Vec<String>>, String> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    for raw in cmd.split('|') {
        let tokens: Vec<String> = raw.split_whitespace().map(String::from).collect();
        if tokens.is_empty() {
            return Err("存在空的管道段。".to_string());
        }
        segments.push(tokens);
    }

    let head = &segments[0];
    if head[0] != "git" {
        return Err(format!(
            "`{}` 不在内置白名单；当前仅允许 git 只读命令（{}）。（登记 CLI 与配置文件见 R4 后续片）",
            head[0],
            GIT_READONLY.join(" / ")
        ));
    }
    let sub = head.get(1).map(String::as_str).unwrap_or("");
    if !GIT_READONLY.contains(&sub) {
        return Err(format!(
            "git 只允许只读子命令：{}；`git {sub}` 被拦截。",
            GIT_READONLY.join(" / ")
        ));
    }
    if sub == "branch" {
        if let Some(bad) = head[2..].iter().find(|a| GIT_BRANCH_WRITE_ARGS.contains(&a.as_str())) {
            return Err(format!("`git branch {bad}` 会改动分支，被拦截（只读分支操作如 -a/-v/--show-current）。"));
        }
    }
    for seg in &segments[1..] {
        if !PIPE_FILTERS.contains(&seg[0].as_str()) {
            return Err(format!(
                "管道右侧仅允许只读过滤命令（{}）；`{}` 被拦截。",
                PIPE_FILTERS.join(" / "),
                seg[0]
            ));
        }
    }
    Ok(segments)
}

/// 依次执行管道分段（无 shell 直执行，前段 stdout 接后段 stdin），返回最后一段 stdout（截断）。
pub fn run_command(work_root: &Path, command: &str, max_chars: usize) -> Result<String, String> {
    let segments = parse_segments(command)?;
    let mut input: Option<Vec<u8>> = None;
    let mut last_stdout: Vec<u8> = Vec::new();
    for (idx, seg) in segments.iter().enumerate() {
        let mut c = Command::new(&seg[0]);
        c.args(&seg[1..]).current_dir(work_root);
        c.stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() });
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = c
            .spawn()
            .map_err(|e| format!("无法启动 `{}`：{e}", seg[0]))?;
        if let Some(data) = input.take() {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(&data);
                // stdin 在此 drop → 子进程收到 EOF
            }
        }
        let out = child
            .wait_with_output()
            .map_err(|e| format!("命令执行失败：{e}"))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let brief: String = err.trim().chars().take(600).collect();
            return Err(format!(
                "第 {} 段 `{}` 退出码 {:?}：{}",
                idx + 1,
                seg.join(" "),
                out.status.code(),
                brief
            ));
        }
        input = Some(out.stdout);
        if idx == segments.len() - 1 {
            last_stdout = input.clone().unwrap_or_default();
        }
    }

    let text = String::from_utf8_lossy(&last_stdout);
    let text = text.trim_end();
    if text.is_empty() {
        Ok("[命令无输出]".to_string())
    } else if text.chars().count() > max_chars {
        Ok(format!(
            "{}…\n[输出超长，已截断]",
            text.chars().take(max_chars).collect::<String>()
        ))
    } else {
        Ok(text.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed(cmd: &str) {
        assert!(check_command(cmd).is_ok(), "应允许：{cmd} → {:?}", check_command(cmd));
    }

    fn blocked(cmd: &str, needle: &str) {
        let err = check_command(cmd).expect_err(&format!("应拦截：{cmd}"));
        assert!(err.contains(needle), "拦截信息应含 `{needle}`，实际：{err}");
    }

    #[test]
    fn allows_readonly_git_and_pipes() {
        allowed("git status --short");
        allowed("git log --oneline -5");
        allowed("git diff HEAD~1 -- ledger/账本.md");
        allowed("git show --stat");
        allowed("git rev-parse --abbrev-ref HEAD");
        allowed("git log --oneline | head -5");
        allowed("git log --oneline | grep fix | head -3");
        allowed("git branch -a");
        allowed("git branch --show-current");
    }

    #[test]
    fn blocks_write_products_with_propose_hint() {
        for cmd in [
            "echo x > 设定/测试.md",
            "echo x >> 设定/测试.md",
            "sed -i s/a/b/ 正文.md",
            "rm 正文.md",
            "mv a.md b.md",
            "cp a.md b.md",
            "tee 正文.md",
            "git add .",
            "git commit -m msg",
            "git push origin main",
            "git reset --hard HEAD",
            "git checkout -- .",
        ] {
            blocked(cmd, "propose_edit");
        }
    }

    #[test]
    fn blocks_chaining_substitution_and_quotes() {
        blocked("git log && git status", "链式");
        blocked("git log ; git status", "链式");
        blocked("git log $(git status)", "命令替换");
        blocked("git log `git status`", "命令替换");
        blocked("git log --grep='fix'", "引号");
        // 链式命令含写产物时按写产物拦截（propose_* 引导优先）
        blocked("git log && rm x", "propose_edit");
    }

    #[test]
    fn blocks_non_whitelist_and_branch_writes() {
        blocked("cat ledger/账本.md", "白名单");
        blocked("ls -la", "白名单");
        blocked("find . -name x", "白名单");
        blocked("git branch -d feature", "改动分支");
        blocked("git branch -D feature", "改动分支");
        blocked("git tag v1", "写产物");
    }

    #[test]
    fn parse_segments_structure() {
        let segs = parse_segments("git log --oneline | head -5").unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0][0], "git");
        assert_eq!(segs[1][0], "head");
        assert!(parse_segments("git log |").is_err(), "空管道段应报错");
    }

    #[test]
    fn run_command_executes_in_work_root() {
        // 临时目录按测试名区分后缀，避免并行测试竞态
        let dir = std::env::temp_dir().join(format!("type-hime-bash-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let init = Command::new("git")
            .arg("init")
            .current_dir(&dir)
            .output()
            .expect("环境应有 git");
        assert!(init.status.success());

        let out = run_command(&dir, "git status --short", 8000).unwrap();
        assert!(out.contains("命令无输出") || out.is_empty(), "实际：{out}");

        // 管道（容忍环境无过滤命令：如无 cat/head 则跳过，仅断言不 panic）
        match run_command(&dir, "git status --short | cat", 8000) {
            Ok(_) => {}
            Err(e) if e.contains("无法启动") => eprintln!("跳过管道断言：{e}"),
            Err(e) => panic!("管道执行失败：{e}"),
        }

        // 非仓库目录：git 输出出错 → 返回 Err（含退出码信息）
        let empty = std::env::temp_dir().join(format!("type-hime-bash-norepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        let err = run_command(&empty, "git status --short", 8000).unwrap_err();
        assert!(err.contains("退出码"), "实际：{err}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
