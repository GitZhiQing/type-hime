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
    /// 路由关键词（R4-④：自然语言命中任一关键词即挂载；frontmatter `keywords: 甲,乙`）
    #[serde(default)]
    pub keywords: Vec<String>,
    /// md 正文（frontmatter 之后）
    pub body: String,
}

/// 内置技能（编译进二进制；后续切片按需扩充）
const BUILTIN_SKILLS: &[(&str, &str)] = &[
    ("writer-basics.md", include_str!("skills/writer-basics.md")),
    ("ledger-update.md", include_str!("skills/ledger-update.md")),
    (
        "review-consistency.md",
        include_str!("skills/review-consistency.md"),
    ),
    ("ledger-tidy.md", include_str!("skills/ledger-tidy.md")),
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
                } else if let Some(v) = line.strip_prefix("keywords:") {
                    skill.keywords = v
                        .split([',', '，'])
                        .map(|k| k.trim().to_string())
                        .filter(|k| !k.is_empty())
                        .collect();
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

/// 技能清单项（R2：`/` 补全菜单与制品面板技能文档的数据源）。
#[derive(Debug, Serialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub when_to_use: Option<String>,
    /// 路由关键词（R4-④：自然语言命中即挂；无 keywords 时回退 whenToUse 全文 + 技能名）
    pub keywords: Vec<String>,
    /// builtin | work
    pub source: String,
    /// 作品层是否覆盖了同名内置技能
    pub overridden: bool,
}

/// 生效技能清单（含来源与覆盖信息）。
pub fn list_skill_meta(work_dir: Option<&Path>) -> Vec<SkillMeta> {
    let builtin = load_skills(None);
    let effective = load_skills(work_dir);
    effective
        .into_iter()
        .map(|s| {
            let is_override = builtin
                .iter()
                .find(|b| b.name == s.name)
                .is_some_and(|b| b.description != s.description);
            let mut keywords = s.keywords.clone();
            if keywords.is_empty() {
                keywords.push(s.name.clone());
                if let Some(w) = &s.when_to_use {
                    keywords.push(w.clone());
                }
            }
            if builtin.iter().any(|b| b.name == s.name) {
                SkillMeta {
                    name: s.name,
                    description: s.description,
                    when_to_use: s.when_to_use,
                    keywords,
                    source: "builtin".to_string(),
                    overridden: is_override,
                }
            } else {
                SkillMeta {
                    name: s.name,
                    description: s.description,
                    when_to_use: s.when_to_use,
                    keywords,
                    source: "work".to_string(),
                    overridden: false,
                }
            }
        })
        .collect()
}

/// 技能原始 md 读取结果（R5 制品面板技能文档）。
#[derive(Debug, Serialize)]
pub struct SkillRaw {
    /// 全文（含 frontmatter）
    pub content: String,
    /// builtin | work（作品层覆盖）
    pub source: String,
}

/// 读取技能原始 md（R5 制品面板技能文档）：作品层覆盖优先，回落内置层。
/// 返回 (原始全文含 frontmatter, 来源 builtin|work)；name 带路径分隔符或 .. 时拒绝。
pub fn read_skill_raw(work_dir: Option<&Path>, name: &str) -> Option<(String, String)> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name.contains("..")
    {
        return None;
    }
    if let Some(dir) = work_dir {
        let path = dir.join("skills").join(format!("{name}.md"));
        if let Ok(raw) = std::fs::read_to_string(&path) {
            return Some((raw, "work".to_string()));
        }
    }
    BUILTIN_SKILLS
        .iter()
        .find(|(file, _)| file.trim_end_matches(".md") == name)
        .map(|(_, raw)| ((*raw).to_string(), "builtin".to_string()))
}

/// 保存技能文件（R5 制品面板；人类直写通道：落作品级 skills/<name>.md 覆盖内置层，git 留痕）。
/// 父目录自动创建（works::write_file 不建目录）；name 带路径分隔符或 .. 时拒绝。
pub fn write_skill_file(work_dir: &Path, name: &str, content: &str) -> Result<(), String> {
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(format!("非法技能名：{name}"));
    }
    let dir = work_dir.join("skills");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建 skills 目录失败：{e}"))?;
    crate::works::write_file(work_dir, &format!("skills/{name}.md"), content)
}

/// 关键词路由（R4-④ · REDESIGN §8.2 实现从简）：prompt 命中技能名或任一关键词即挂载。
pub fn route_skills(prompt: &str, work_dir: Option<&Path>) -> Vec<String> {
    list_skill_meta(work_dir)
        .into_iter()
        .filter(|m| {
            prompt.contains(&m.name)
                || m.keywords
                    .iter()
                    .any(|k| !k.is_empty() && prompt.contains(k.as_str()))
        })
        .map(|m| m.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_layering() {
        let raw = "---\nname: 章节\n\ndescription: 写章节\nwhenToUse: 写正文时\nkeywords: 正文, 续写\n---\n\n正文指令内容";
        let s = parse_skill(raw, "fallback");
        assert_eq!(s.name, "章节");
        assert_eq!(s.description, "写章节");
        assert_eq!(s.when_to_use.as_deref(), Some("写正文时"));
        assert_eq!(s.keywords, vec!["正文".to_string(), "续写".to_string()]);
        assert!(s.body.contains("正文指令内容"));

        // 空渲染
        assert_eq!(render_skills(&[]), "");
        // 有渲染
        let r = render_skills(&[s]);
        assert!(r.contains("# 技能指令"));
        assert!(r.contains("## 技能：章节"));
    }

    #[test]
    fn read_skill_raw_layers_and_guards() {
        let dir = std::env::temp_dir().join(format!(
            "type-hime-skillraw-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // 内置层可读（source=builtin）
        let (raw, source) = read_skill_raw(Some(&dir), "writer-basics").unwrap();
        assert_eq!(source, "builtin");
        assert!(raw.contains("name: writer-basics"), "{raw}");
        // 作品层覆盖后 source=work
        std::fs::create_dir_all(dir.join("skills")).unwrap();
        std::fs::write(
            dir.join("skills/writer-basics.md"),
            "---\nname: writer-basics\ndescription: 覆盖版\n---\n覆盖正文",
        )
        .unwrap();
        let (raw, source) = read_skill_raw(Some(&dir), "writer-basics").unwrap();
        assert_eq!(source, "work");
        assert!(raw.contains("覆盖正文"));
        // 路径守卫
        assert!(read_skill_raw(Some(&dir), "../escape").is_none());
        assert!(read_skill_raw(Some(&dir), "a/b").is_none());
        assert!(read_skill_raw(Some(&dir), "不存在技能").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_skill_file_creates_dir_and_guards() {
        let dir = std::env::temp_dir().join(format!(
            "type-hime-skillwrite-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        // 无 git 仓库时 write 落盘成功、commit 失败被容忍？——works::write_file 会 commit 失败报错；
        // 技能保存前提是作品目录（必有 git），此处先 init 保证行为一致
        crate::git::init_repo(&dir).unwrap();
        write_skill_file(&dir, "writer-basics", "---\nname: writer-basics\n---\n覆盖").unwrap();
        let raw = std::fs::read_to_string(dir.join("skills/writer-basics.md")).unwrap();
        assert!(raw.contains("覆盖"));
        assert!(write_skill_file(&dir, "../x", "y").is_err());
        assert!(write_skill_file(&dir, "a/b", "y").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn routing_by_keywords() {
        // 内置技能挂载路由：命中关键词 / 技能名；未命中不挂
        let hit = route_skills("帮我看看这一章的正文有没有问题", None);
        assert!(hit.contains(&"writer-basics".to_string()), "正文应命中 writer-basics：{hit:?}");
        let hit2 = route_skills("跑一次一致性检查", None);
        assert!(
            hit2.contains(&"review-consistency".to_string()),
            "一致性应命中 review-consistency：{hit2:?}"
        );
        let hit3 = route_skills("把账本整理一下", None);
        assert!(hit3.contains(&"ledger-tidy".to_string()), "账本整理应命中：{hit3:?}");
        // 与写作无关的普通提问不挂载
        let none = route_skills("今天天气怎么样", None);
        assert!(none.is_empty(), "无关提问不应挂载：{none:?}");
        // 技能名直呼也命中
        let by_name = route_skills("请按 review-consistency 规程来", None);
        assert!(by_name.contains(&"review-consistency".to_string()));
    }
}
