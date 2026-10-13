//! Skill 加载器（借鉴 deepseek-harness 的分层技能注册表）。
//! Skill = md + frontmatter（name/description/whenToUse）。
//! 两层来源：应用内置（include 进二进制）→ 作品目录 skills/*.md（同名覆盖，nearest layer wins）。
//! 目录与正文分离：装配时每次重读文件，编辑技能正文无需失效缓存。

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Skill {
    pub name: String,
    pub description: String,
    #[serde(default, rename = "whenToUse")]
    pub when_to_use: Option<String>,
    /// md 正文（frontmatter 之后）
    pub body: String,
}

/// 内置技能（编译进二进制；后续切片按需扩充）
const BUILTIN_SKILLS: &[(&str, &str)] = &[
    (
        "writer-basics.md",
        include_str!("skills/writer-basics.md"),
    ),
];

/// 解析单个 md 文件为 Skill；frontmatter 缺 name 时以文件名为名。
pub fn parse_skill(raw: &str, fallback_name: &str) -> Skill {
    let mut skill = Skill {
        name: fallback_name.to_string(),
        ..Default::default()
    };
    let body;
    if let Some(rest) = raw.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            body = rest[end + 4..].trim_start_matches('\n').to_string();
            for line in fm.lines() {
                if let Some(v) = line.strip_prefix("name:") {
                    skill.name = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("description:") {
                    skill.description = v.trim().to_string();
                } else if let Some(v) = line.strip_prefix("whenToUse:") {
                    skill.when_to_use = Some(v.trim().to_string());
                }
            }
        } else {
            body = raw.to_string();
        }
    } else {
        body = raw.to_string();
    }
    skill.body = body;
    skill
}

/// 加载全部技能：内置层 + 作品目录层（作品层同名覆盖内置层）。
pub fn load_skills(work_dir: Option<&Path>) -> Vec<Skill> {
    let mut by_name: BTreeMap<String, Skill> = BTreeMap::new();
    for (file, raw) in BUILTIN_SKILLS {
        let s = parse_skill(raw, file.trim_end_matches(".md"));
        by_name.insert(s.name.clone(), s);
    }
    if let Some(dir) = work_dir {
        let skills_dir = dir.join("skills");
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
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
                    let s = parse_skill(&raw, &fallback);
                    by_name.insert(s.name.clone(), s); // 作品层覆盖内置层
                }
            }
        }
    }
    by_name.into_values().collect()
}

/// 把选中的技能渲染为 preamble 分节。
pub fn render_skills(skills: &[Skill]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut out = String::from("\n# 技能指令\n");
    for s in skills {
        out.push_str(&format!("\n## 技能：{}\n{}\n", s.name, s.body.trim()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_layering() {
        let raw = "---\nname: 章节\n\ndescription: 写章节\nwhenToUse: 写正文时\n---\n\n正文指令内容";
        let s = parse_skill(raw, "fallback");
        assert_eq!(s.name, "章节");
        assert_eq!(s.description, "写章节");
        assert_eq!(s.when_to_use.as_deref(), Some("写正文时"));
        assert!(s.body.contains("正文指令内容"));

        // 空渲染
        assert_eq!(render_skills(&[]), "");
        // 有渲染
        let r = render_skills(&[s]);
        assert!(r.contains("# 技能指令"));
        assert!(r.contains("## 技能：章节"));
    }
}
