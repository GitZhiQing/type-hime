//! 权限模型（R4 · D30）：两档 × per-tool/per-action 覆盖矩阵。
//! 来源（REFACTOR A1）：goose smart_approve per-tool 覆盖、Cursor per-tool auto-approval、
//! ZCode 会话级模式 + 工具权限块门控（会话内决定档位、工具执行前 resolve）。
//!
//! 档位在会话中决定（存会话档案，前端输入框左下角下拉切换）；
//! 覆盖表存作品 project.json（bash 命令模式 / propose_* 路径模式）。
//! 红线：四类留人操作（可见性设置、删除/恢复文件、跳过阶段、装配切全量）不在此模型内——
//! 硬编码在工具层（D32），任何档位与覆盖均不可豁免。

use serde::{Deserialize, Serialize};

/// 权限档位（会话级）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// 变更前确认（默认）：提议逐条人工审阅；白名单 bash 逐条确认执行。
    #[default]
    Confirm,
    /// 完全访问：提议自动应用（触及可见性 frontmatter 的除外，见 D32）；白名单 bash 直通。
    FullAccess,
}

impl PermissionMode {
    pub fn label(self) -> &'static str {
        match self {
            PermissionMode::Confirm => "变更前确认",
            PermissionMode::FullAccess => "完全访问",
        }
    }
}

/// 单条覆盖：按工具名（+ 可选模式）覆盖档位。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOverride {
    /// 工具名：bash / propose_edit / propose_write / run_flow …
    pub tool: String,
    /// 命令/路径模式：尾部 `*` 前缀通配（bash 匹配命令，propose_* 匹配目标路径）；None=该工具全部
    #[serde(default)]
    pub pattern: Option<String>,
    pub mode: PermissionMode,
}

/// 作品级权限覆盖表（存 project.json）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PermissionConfig {
    #[serde(default)]
    pub overrides: Vec<ToolOverride>,
}

/// 工具侧权限上下文（ToolCtx 持有；工具执行前 resolve）。
#[derive(Debug, Clone)]
pub struct PermissionContext {
    /// 会话档位
    pub session: PermissionMode,
    /// 作品级覆盖表
    pub config: PermissionConfig,
}

impl PermissionContext {
    pub fn new(session: PermissionMode, config: PermissionConfig) -> Self {
        Self { session, config }
    }

    /// 解析某次操作生效的档位（更具体 pattern 的覆盖 → 工具级覆盖 → 会话档位）。
    pub fn resolve(&self, tool: &str, subject: Option<&str>) -> PermissionMode {
        resolve(&self.config, self.session, tool, subject)
    }
}

/// 模式匹配：`pattern` 尾 `*` 为前缀通配；无 `*` 时精确匹配或「命令 + 参数」前缀匹配。
pub fn pattern_matches(pattern: &str, subject: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('*') {
        subject.starts_with(prefix)
    } else {
        subject == pattern || subject.starts_with(&format!("{pattern} "))
    }
}

/// 解析生效档位：更具体 pattern 的覆盖优先于工具级覆盖，同具体度先声明者胜；无命中回落会话档位。
pub fn resolve(
    config: &PermissionConfig,
    session: PermissionMode,
    tool: &str,
    subject: Option<&str>,
) -> PermissionMode {
    let mut best: Option<(u8, PermissionMode)> = None;
    for ov in &config.overrides {
        if ov.tool != tool {
            continue;
        }
        let score = match (&ov.pattern, subject) {
            (Some(p), Some(s)) if pattern_matches(p, s) => 2,
            (Some(_), _) => continue, // 声明了 pattern 但未命中 → 不适用
            (None, _) => 1,
        };
        if best.map(|(b, _)| score > b).unwrap_or(true) {
            best = Some((score, ov.mode));
        }
    }
    best.map(|(_, m)| m).unwrap_or(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ov(tool: &str, pattern: Option<&str>, mode: PermissionMode) -> ToolOverride {
        ToolOverride {
            tool: tool.to_string(),
            pattern: pattern.map(String::from),
            mode,
        }
    }

    #[test]
    fn resolve_priority_matrix() {
        let cfg = PermissionConfig {
            overrides: vec![
                ov("bash", None, PermissionMode::Confirm),          // 工具级：bash 恒确认
                ov("bash", Some("git log*"), PermissionMode::FullAccess), // 更具体：git log 直通
                ov("propose_edit", Some("ledger/*"), PermissionMode::FullAccess), // 仅账本自动
            ],
        };
        // 会话默认 Confirm
        assert_eq!(resolve(&cfg, PermissionMode::Confirm, "read_file", None), PermissionMode::Confirm);
        // 具体 pattern 优先于工具级
        assert_eq!(
            resolve(&cfg, PermissionMode::Confirm, "bash", Some("git log --oneline -5")),
            PermissionMode::FullAccess
        );
        // 工具级覆盖生效（非命中 pattern）
        assert_eq!(
            resolve(&cfg, PermissionMode::FullAccess, "bash", Some("git status --short")),
            PermissionMode::Confirm
        );
        // 路径 pattern：ledger 命中，volumes 不命中 → 回落会话档
        assert_eq!(
            resolve(&cfg, PermissionMode::Confirm, "propose_edit", Some("ledger/账本.md")),
            PermissionMode::FullAccess
        );
        assert_eq!(
            resolve(&cfg, PermissionMode::Confirm, "propose_edit", Some("volumes/vol-01/chapters/ch-001.md")),
            PermissionMode::Confirm
        );
        assert_eq!(
            resolve(&cfg, PermissionMode::FullAccess, "propose_write", Some("volumes/vol-01/chapters/ch-007.md")),
            PermissionMode::FullAccess
        );
    }

    #[test]
    fn same_specificity_first_declared_wins() {
        let cfg = PermissionConfig {
            overrides: vec![
                ov("bash", Some("git*"), PermissionMode::FullAccess),
                ov("bash", Some("git log*"), PermissionMode::Confirm),
            ],
        };
        // 两条都命中（score 同为 2）→ 先声明者胜
        assert_eq!(
            resolve(&cfg, PermissionMode::Confirm, "bash", Some("git log -5")),
            PermissionMode::FullAccess
        );
    }

    #[test]
    fn pattern_semantics() {
        assert!(pattern_matches("git log*", "git log --oneline"));
        assert!(pattern_matches("git log", "git log -5")); // 无 * 时命令+参数前缀
        assert!(pattern_matches("git log", "git log"));
        assert!(!pattern_matches("git log", "git logs"));
        assert!(pattern_matches("ledger/*", "ledger/账本.md"));
        assert!(!pattern_matches("ledger/*", "volumes/ledger/账本.md"));
    }

    #[test]
    fn serde_compat_defaults() {
        // 旧 project.json 无 permission 字段 → 空表
        let cfg: PermissionConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.overrides.is_empty());
        // 档位序列化为 snake_case
        assert_eq!(
            serde_json::to_string(&PermissionMode::FullAccess).unwrap(),
            "\"full_access\""
        );
        // 缺 mode 的旧模式串 → 反序列化失败（明确报错而非静默）
        assert!(serde_json::from_str::<ToolOverride>(r#"{"tool":"bash"}"#).is_err());
        // 正常条目
        let one: ToolOverride =
            serde_json::from_str(r#"{"tool":"bash","pattern":"git log*","mode":"full_access"}"#)
                .unwrap();
        assert_eq!(one.mode, PermissionMode::FullAccess);
    }
}
