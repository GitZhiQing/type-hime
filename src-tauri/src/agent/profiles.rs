//! 子 Agent 定义（R8-② · 多 Agent 团队）：AgentProfile = md + loose frontmatter，
//! 正文即 system prompt。两层来源：内置 general（编译进二进制，通用基础模板）
//! → 用户级 `%CONFIG%/type-hime/agents/*.md`（同名覆盖内置；管理页 CRUD 落此层）。
//! 形态来源：ZCode subagent-profile（md+frontmatter/正文=systemPrompt/项目级同名覆盖）。
//!
//! 调度（spawn_agent 工具）在 tools.rs；用户级文件不经提议通道（属应用配置，非作品产物）。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 子 Agent 定义（frontmatter + 正文=system prompt）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentProfile {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub model: Option<String>,
    /// 工具白名单（空 = 通用集；spawn_agent 永远被剔除——子 Agent 不可递归派生）
    #[serde(default)]
    pub tools: Vec<String>,
    /// 挂载技能（拼入 preamble 尾部，复用 skills 加载器）
    #[serde(default)]
    pub skills: Vec<String>,
    #[serde(default)]
    pub max_turns: usize,
    /// md 正文（frontmatter 之后）= system prompt
    #[serde(default)]
    pub body: String,
}

/// 内置子 Agent（通用基础模板；用户可基于此在管理页复制自定义）。
const BUILTIN_PROFILES: &[(&str, &str)] = &[("general.md", include_str!("profiles/general.md"))];

/// loose frontmatter 解析（与 skills::parse_skill 同语义：容忍逗号串/裸列表）。
/// frontmatter 缺 name 时以文件名为名；缺 max_turns 时取 12。
pub fn parse_profile(raw: &str, fallback_name: &str) -> AgentProfile {
    let mut p = AgentProfile {
        name: fallback_name.to_string(),
        max_turns: 12,
        ..Default::default()
    };
    let body;
    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            body = rest[end + 4..].trim_start_matches('\n').to_string();
            for line in fm.lines() {
                if let Some(v) = line.strip_prefix("name:") {
                    p.name = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("description:") {
                    p.description = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("model:") {
                    let v = v.trim();
                    if !v.is_empty() {
                        p.model = Some(v.to_string());
                    }
                } else if let Some(v) = line.strip_prefix("tools:") {
                    p.tools = split_list(v);
                } else if let Some(v) = line.strip_prefix("skills:") {
                    p.skills = split_list(v);
                } else if let Some(v) = line.strip_prefix("max_turns:") {
                    if let Ok(n) = v.trim().parse::<usize>() {
                        p.max_turns = n;
                    }
                }
            }
        } else {
            body = raw.to_string();
        }
    } else {
        body = raw.to_string();
    }
    p.body = body;
    p
}

fn split_list(v: &str) -> Vec<String> {
    v.split([',', '，'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 用户级子 Agent 目录（应用配置层，与 settings.json 同级）。
pub fn user_profiles_dir() -> Result<PathBuf, String> {
    let dir = dirs::config_dir()
        .ok_or("无法定位用户配置目录")?
        .join("type-hime")
        .join("agents");
    Ok(dir)
}

/// 加载全部生效子 Agent：内置层 → 用户层（同名覆盖）。
pub fn load_profiles() -> Vec<AgentProfile> {
    let mut by_name: std::collections::BTreeMap<String, AgentProfile> =
        std::collections::BTreeMap::new();
    for (file, raw) in BUILTIN_PROFILES {
        let p = parse_profile(raw, file.trim_end_matches(".md"));
        by_name.insert(p.name.clone(), p);
    }
    if let Ok(dir) = user_profiles_dir() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("md") {
                    continue;
                }
                let fallback = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if let Ok(raw) = std::fs::read_to_string(&path) {
                    let p = parse_profile(&raw, &fallback);
                    by_name.insert(p.name.clone(), p); // 用户层覆盖内置层
                }
            }
        }
    }
    by_name.into_values().collect()
}

/// 管理页清单项。
#[derive(Debug, Serialize)]
pub struct AgentMeta {
    pub name: String,
    pub description: String,
    /// builtin | user
    pub source: String,
    /// 用户层是否覆盖了同名内置
    pub overridden: bool,
    pub max_turns: usize,
}

/// 子 Agent 原始 md 读取结果（管理页编辑用）。
#[derive(Debug, Serialize)]
pub struct AgentRaw {
    /// 全文（含 frontmatter）
    pub content: String,
    /// builtin | user（用户层覆盖）
    pub source: String,
}

pub fn list_profile_meta() -> Vec<AgentMeta> {
    let builtin_names: Vec<String> = BUILTIN_PROFILES
        .iter()
        .map(|(f, _)| f.trim_end_matches(".md").to_string())
        .collect();
    load_profiles()
        .into_iter()
        .map(|p| AgentMeta {
            name: p.name.clone(),
            description: p.description,
            source: if builtin_names.contains(&p.name) {
                "builtin".to_string()
            } else {
                "user".to_string()
            },
            overridden: false, // 加载结果已合并，covered 由 source 推导；保留字段对齐技能清单形态
            max_turns: p.max_turns,
        })
        .collect()
}

/// 读取子 Agent 原始 md（用户层优先，回落内置）。
pub fn read_profile_raw(name: &str) -> Option<(String, String)> {
    if !valid_name(name) {
        return None;
    }
    if let Ok(dir) = user_profiles_dir() {
        if let Ok(raw) = std::fs::read_to_string(dir.join(format!("{name}.md"))) {
            return Some((raw, "user".to_string()));
        }
    }
    BUILTIN_PROFILES
        .iter()
        .find(|(f, _)| f.trim_end_matches(".md") == name)
        .map(|(_, raw)| ((*raw).to_string(), "builtin".to_string()))
}

/// 保存子 Agent（用户级；同名覆盖内置）。返回保存路径供提示。
pub fn write_profile(name: &str, content: &str) -> Result<PathBuf, String> {
    if !valid_name(name) {
        return Err(format!("非法子 Agent 名：{name}（仅字母/数字/-/_，且不以点开头）"));
    }
    // 先解析校验：正文与 name 必须合法，防写坏定义
    let parsed = parse_profile(content, name);
    if parsed.name != name {
        return Err(format!("frontmatter name（{}）与请求名（{name}）不一致", parsed.name));
    }
    let dir = user_profiles_dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 agents 目录失败：{e}"))?;
    let path = dir.join(format!("{name}.md"));
    std::fs::write(&path, content).map_err(|e| format!("保存失败：{e}"))?;
    Ok(path)
}

/// 删除用户级子 Agent；内置不可删（回落内置层生效）。
pub fn delete_profile(name: &str) -> Result<bool, String> {
    if !valid_name(name) {
        return Err(format!("非法子 Agent 名：{name}"));
    }
    if BUILTIN_PROFILES
        .iter()
        .any(|(f, _)| f.trim_end_matches(".md") == name)
    {
        return Err(format!("{name} 是内置子 Agent，不可删除（可保存用户层覆盖）"));
    }
    let path = user_profiles_dir()?.join(format!("{name}.md"));
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除失败：{e}"))?;
    Ok(true)
}

/// 名称守卫：防路径穿越（与技能读写同口径）。
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && !name.starts_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_loose_frontmatter() {
        let raw = "---\nname: 写手\ndescription: 写正文\nmodel: deepseek-chat\ntools: read_file, propose_write，propose_edit\nskills: writer-basics\nmax_turns: 8\n---\n\n你是写手。";
        let p = parse_profile(raw, "fallback");
        assert_eq!(p.name, "写手");
        assert_eq!(p.model.as_deref(), Some("deepseek-chat"));
        assert_eq!(
            p.tools,
            vec!["read_file", "propose_write", "propose_edit"]
        );
        assert_eq!(p.skills, vec!["writer-basics"]);
        assert_eq!(p.max_turns, 8);
        assert_eq!(p.body, "你是写手。");

        // 缺字段回退：无 frontmatter / 无 name / 无 max_turns
        let p2 = parse_profile("正文而已", "fb");
        assert_eq!(p2.name, "fb");
        assert_eq!(p2.max_turns, 12);
        assert_eq!(p2.body, "正文而已");
        let p3 = parse_profile("---\ndescription: d\n---\nbody", "fb3");
        assert_eq!(p3.name, "fb3");
    }

    #[test]
    fn builtin_general_loads() {
        let all = load_profiles();
        let g = all.iter().find(|p| p.name == "general").expect("内置 general 必在");
        assert!(g.description.contains("通用创作执行者"));
        assert!(g.tools.contains(&"propose_write".to_string()));
        assert!(!g.tools.contains(&"spawn_agent".to_string()), "模板不含派生工具");
        assert!(g.body.contains("propose_edit"));
        // 调度层读取原始 md（write 模板复制用）
        let (raw, source) = read_profile_raw("general").unwrap();
        assert_eq!(source, "builtin");
        assert!(raw.contains("name: general"));
    }

    #[test]
    fn user_layer_overrides_and_crud() {
        // 直接操作用户目录会污染真实配置；用隔离断言只测 write/read/delete 的守卫路径
        assert!(write_profile("../escape", "x").is_err());
        assert!(write_profile(".hidden", "x").is_err());
        assert!(read_profile_raw("../x").is_none());
        // name 不一致拒绝
        assert!(write_profile("tmp-agent", "---\nname: other\n---\nx").is_err());
        // 内置不可删
        assert!(delete_profile("general").is_err());
        // 完整 CRUD 落盘往返（真实用户目录，测试后清理）
        let content = "---\nname: tmp-r8-agent\ndescription: 测试\n---\n测试体";
        let path = write_profile("tmp-r8-agent", content).unwrap();
        assert!(path.exists());
        let (raw, source) = read_profile_raw("tmp-r8-agent").unwrap();
        assert_eq!(source, "user");
        assert!(raw.contains("测试体"));
        let deleted = delete_profile("tmp-r8-agent").unwrap();
        assert!(deleted);
        assert!(!path.exists());
    }
}
