//! bash 工具的命令守卫与执行（R4）。
//! 守卫：内置白名单（git 只读子命令 + 只读过滤管道）+ 写产物/间接写黑名单 +
//! 两层配置文件（全局 app data + 作品级 permissions.json，A3/aichat 声明式配置惯例）扩充。
//! 执行：无 shell 直执行（分段 spawn + 管道），杜绝字符串拼接进 shell 的注入面。
//!
//! 红线（AGENTS.md）：任何写产物/改历史的命令一律拦截并引导 propose_edit/propose_write——
//! bash 永不绕过提议审阅通道。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

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
    // 重定向与就地写
    ">",
    "tee ",
    "truncate",
    "dd ",
    "sed -i",
    "perl -i",
    "sponge ",
    // 文件增删改
    "rm ",
    "rm\t",
    "mv ",
    "cp ",
    "touch ",
    "mkdir",
    "rmdir",
    "ln ",
    "chmod",
    "chown",
    "shred",
    "install ",
    "rsync",
    "patch ",
    "mkfifo",
    // 交互式编辑器（挂起风险 + 写盘）
    "vi ",
    "vim ",
    "nano ",
    "emacs ",
    // find 的写动作
    " -delete",
    " -exec",
    // git 写操作（回滚走「重做阶段」与应用层通道，push/远端不进白名单）
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
    "git gc",
    "git prune",
    "git worktree",
    "git update-ref",
    "git cherry-pick",
    "git revert",
    "git bisect",
];

/// 命令替换 / 链式 / 重定向：不被支持（起步保守，扩展面走配置文件+登记 CLI）。
const CHAIN_PATTERNS: &[&str] = &["$(", "`", ";", "&&", "||", "&", "<", "\n"];

/// bash 工具策略（内置默认 + 两层配置文件合并后的结果）。
#[derive(Debug, Clone, Default)]
pub struct BashPolicy {
    /// 登记的额外程序（白名单扩展；程序名匹配）
    pub allow_commands: Vec<String>,
    /// 额外拦截模式（黑名单扩展）
    pub deny_patterns: Vec<String>,
    /// 额外允许的 git 只读子命令
    pub git_readonly: Vec<String>,
    /// 额外允许的管道过滤命令
    pub pipe_filters: Vec<String>,
}

/// permissions.json 的解析形态（两层文件同构）。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PolicyFile {
    #[serde(default)]
    pub allow_commands: Vec<String>,
    #[serde(default)]
    pub deny_patterns: Vec<String>,
    #[serde(default)]
    pub git_readonly: Vec<String>,
    #[serde(default)]
    pub pipe_filters: Vec<String>,
}

impl BashPolicy {
    fn from_files(files: Vec<PolicyFile>) -> Self {
        let mut p = BashPolicy::default();
        for f in files {
            p.allow_commands.extend(f.allow_commands);
            p.deny_patterns.extend(f.deny_patterns);
            p.git_readonly.extend(f.git_readonly);
            p.pipe_filters.extend(f.pipe_filters);
        }
        p
    }

    fn git_readonly_all(&self) -> Vec<&str> {
        GIT_READONLY
            .iter()
            .copied()
            .chain(self.git_readonly.iter().map(String::as_str))
            .collect()
    }

    fn pipe_filters_all(&self) -> Vec<&str> {
        PIPE_FILTERS
            .iter()
            .copied()
            .chain(self.pipe_filters.iter().map(String::as_str))
            .collect()
    }
}

/// 全局白名单文件路径（app data/type-hime/permissions.json）。
pub fn global_policy_path() -> Option<PathBuf> {
    dirs::config_dir().map(|b| b.join("type-hime").join("permissions.json"))
}

/// 作品级白名单文件路径。
pub fn work_policy_path(work_dir: &Path) -> PathBuf {
    work_dir.join("permissions.json")
}

/// 解析单个策略文件（不存在返回 None；格式错误按空处理并打点 stderr）。
fn read_policy_file(path: &Path) -> Option<PolicyFile> {
    let raw = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<PolicyFile>(&raw) {
        Ok(f) => Some(f),
        Err(e) => {
            eprintln!("[bash] permissions.json 解析失败（{}）：{e}", path.display());
            None
        }
    }
}

/// 加载策略：全局层 + 作品层（两层合并；内置默认始终生效）。
pub fn load_policy(work_dir: &Path) -> BashPolicy {
    let mut files = Vec::new();
    if let Some(g) = global_policy_path() {
        if let Some(f) = read_policy_file(&g) {
            files.push(f);
        }
    }
    if let Some(f) = read_policy_file(&work_policy_path(work_dir)) {
        files.push(f);
    }
    BashPolicy::from_files(files)
}

/// 守卫：写产物/黑名单（含配置扩展）→ 链式/替换拒绝 → 结构校验（白名单）。
pub fn check_command(command: &str, policy: &BashPolicy) -> Result<(), String> {
    let cmd = command.trim();
    if cmd.is_empty() {
        return Err("命令为空。".to_string());
    }
    for pat in DENY_PATTERNS.iter().copied().chain(policy.deny_patterns.iter().map(String::as_str))
    {
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
    parse_segments(cmd, policy).map(|_| ())
}

/// 解析为管道分段并做白名单校验（空白分词；引号/转义已被前置守卫拒绝）。
pub fn parse_segments(cmd: &str, policy: &BashPolicy) -> Result<Vec<Vec<String>>, String> {
    let mut segments: Vec<Vec<String>> = Vec::new();
    for raw in cmd.split('|') {
        let tokens: Vec<String> = raw.split_whitespace().map(String::from).collect();
        if tokens.is_empty() {
            return Err("存在空的管道段。".to_string());
        }
        segments.push(tokens);
    }

    let git_readonly = policy.git_readonly_all();
    let pipe_filters = policy.pipe_filters_all();

    let head = &segments[0];
    if head[0] == "git" {
        let sub = head.get(1).map(String::as_str).unwrap_or("");
        if !git_readonly.contains(&sub) {
            return Err(format!(
                "git 只允许只读子命令：{}；`git {sub}` 被拦截。",
                git_readonly.join(" / ")
            ));
        }
        if sub == "branch" {
            if let Some(bad) = head[2..].iter().find(|a| GIT_BRANCH_WRITE_ARGS.contains(&a.as_str())) {
                return Err(format!(
                    "`git branch {bad}` 会改动分支，被拦截（只读分支操作如 -a/-v/--show-current）。"
                ));
            }
        }
    } else if !policy.allow_commands.iter().any(|c| c == &head[0]) {
        return Err(format!(
            "`{}` 不在内置白名单；当前仅允许 git 只读命令（{}）与 permissions.json 登记的命令。",
            head[0],
            git_readonly.join(" / ")
        ));
    }
    for seg in &segments[1..] {
        if !pipe_filters.contains(&seg[0].as_str()) {
            return Err(format!(
                "管道右侧仅允许只读过滤命令（{}）；`{}` 被拦截。",
                pipe_filters.join(" / "),
                seg[0]
            ));
        }
    }
    Ok(segments)
}

/// 依次执行管道分段（无 shell 直执行，前段 stdout 接后段 stdin），返回最后一段 stdout（截断）。
pub fn run_command(
    work_root: &Path,
    command: &str,
    max_chars: usize,
    policy: &BashPolicy,
) -> Result<String, String> {
    let segments = parse_segments(command, policy)?;
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

    fn policy() -> BashPolicy {
        BashPolicy::default()
    }

    fn allowed(cmd: &str) {
        assert!(
            check_command(cmd, &policy()).is_ok(),
            "应允许：{cmd} → {:?}",
            check_command(cmd, &policy())
        );
    }

    fn blocked(cmd: &str, needle: &str) {
        let err = check_command(cmd, &policy()).expect_err(&format!("应拦截：{cmd}"));
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
            "touch 新文件.md",
            "patch -p1",
            "perl -i -pe s/a/b/ 正文.md",
            "rsync -a a/ b/",
            "dd if=x of=y",
            "find . -name x -delete",
            "git add .",
            "git commit -m msg",
            "git push origin main",
            "git reset --hard HEAD",
            "git checkout -- .",
            "git stash",
            "git gc",
        ] {
            blocked(cmd, "propose_edit");
        }
        // 白名单外的 git 子命令按白名单拦截（即使本身是写操作）
        blocked("git reflog expire", "只读子命令");
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
    fn policy_extends_whitelist_and_denies() {
        let p = BashPolicy {
            allow_commands: vec!["rg".to_string()],
            deny_patterns: vec!["uptime".to_string()],
            git_readonly: vec!["reflog".to_string()],
            pipe_filters: vec!["sed".to_string()],
        };
        // 登记程序可用
        assert!(check_command("rg 林晚 --count", &p).is_ok());
        // 未登记程序仍拦
        assert!(check_command("fd 林晚", &p).is_err());
        // git 只读扩展生效
        assert!(check_command("git reflog -5", &p).is_ok());
        assert!(check_command("git reflog -5", &policy()).is_err());
        // 管道扩展生效
        assert!(check_command("git log --oneline | sed s/a/b/", &p).is_ok());
        assert!(check_command("git log --oneline | sed s/a/b/", &policy()).is_err());
        // 追加拦截模式生效
        assert!(check_command("uptime", &p).unwrap_err().contains("uptime"));
    }

    #[test]
    fn load_policy_reads_work_layer() {
        let dir = std::env::temp_dir().join(format!("type-hime-bash-policy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            work_policy_path(&dir),
            r#"{"allow_commands":["rg"],"deny_patterns":[],"git_readonly":["reflog"],"pipe_filters":[]}"#,
        )
        .unwrap();
        let p = load_policy(&dir);
        assert_eq!(p.allow_commands, vec!["rg".to_string()]);
        assert_eq!(p.git_readonly, vec!["reflog".to_string()]);
        assert!(check_command("rg x", &p).is_ok());
        // 坏文件不 panic（按空处理）
        std::fs::write(work_policy_path(&dir), "{ 坏 json").unwrap();
        let p2 = load_policy(&dir);
        assert!(p2.allow_commands.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_segments_structure() {
        let segs = parse_segments("git log --oneline | head -5", &policy()).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0][0], "git");
        assert_eq!(segs[1][0], "head");
        assert!(parse_segments("git log |", &policy()).is_err(), "空管道段应报错");
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

        let out = run_command(&dir, "git status --short", 8000, &policy()).unwrap();
        assert!(out.contains("命令无输出") || out.is_empty(), "实际：{out}");

        // 管道（容忍环境无过滤命令：如无 cat/head 则跳过，仅断言不 panic）
        match run_command(&dir, "git status --short | cat", 8000, &policy()) {
            Ok(_) => {}
            Err(e) if e.contains("无法启动") => eprintln!("跳过管道断言：{e}"),
            Err(e) => panic!("管道执行失败：{e}"),
        }

        // 非仓库目录：git 输出出错 → 返回 Err（含退出码信息）
        let empty = std::env::temp_dir().join(format!("type-hime-bash-norepo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).unwrap();
        let err = run_command(&empty, "git status --short", 8000, &policy()).unwrap_err();
        assert!(err.contains("退出码"), "实际：{err}");

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }
}
