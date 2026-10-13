//! 设定库（lore）：设定条目 = md + frontmatter（类型/可见性/开放卷）。
//! **可见性是红线**：被隐藏或未到开放卷的条目，无论装配怎么设都不进 prompt
//! （is_prompt_visible 是唯一裁决点，装配引擎出口强制调用）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::frontmatter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

/// 装配档位：完整正文 / 仅名称 / 紧凑（设定仅名称 + 账本压到零活跃窗口）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Detail {
    #[default]
    Full,
    NameOnly,
    Compact,
}

/// 设定条目元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingMeta {
    pub name: String,
    /// character | worldview | mystery（由所在目录决定）
    pub kind: String,
    pub visibility: Visibility,
    /// 第 N 卷起对 prompt 开放；None = 不限
    #[serde(default)]
    pub open_at_vol: Option<u32>,
    /// 相对作品根路径
    pub path: String,
}

pub fn kind_dir(kind: &str) -> Result<&'static str, String> {
    match kind {
        "character" => Ok("settings/characters"),
        "worldview" => Ok("settings/worldview"),
        "mystery" => Ok("settings/mysteries"),
        other => Err(format!("未知设定类型：{other}（应为 character/worldview/mystery）")),
    }
}

pub fn dir_kind(dir: &str) -> Option<&'static str> {
    match dir {
        "settings/characters" => Some("character"),
        "settings/worldview" => Some("worldview"),
        "settings/mysteries" => Some("mystery"),
        _ => None,
    }
}

/// **红线**：条目是否可进 prompt。
pub fn is_prompt_visible(meta: &SettingMeta, current_volume: u32) -> bool {
    if matches!(meta.visibility, Visibility::Hidden) {
        return false;
    }
    match meta.open_at_vol {
        Some(n) => current_volume >= n,
        None => true,
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// 新建设定条目（含 git 提交）。
pub fn create_setting(
    work_dir: &Path,
    kind: &str,
    name: &str,
    content: &str,
    visibility: Visibility,
    open_at_vol: Option<u32>,
) -> Result<SettingMeta, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("条目名不能为空".into());
    }
    let dir = kind_dir(kind)?;
    let rel = format!("{dir}/{}.md", sanitize_filename(name));
    let path = work_dir.join(&rel);
    if path.exists() {
        return Err(format!("{rel} 已存在"));
    }
    std::fs::create_dir_all(work_dir.join(dir))
        .map_err(|e| format!("创建目录失败：{e}"))?;

    let fm = render_frontmatter(name, visibility, open_at_vol);
    std::fs::write(&path, format!("{fm}\n{content}"))
        .map_err(|e| format!("写入失败：{e}"))?;
    crate::git::commit_all(work_dir, &format!("新建设定：{name}"))?;

    Ok(SettingMeta {
        name: name.to_string(),
        kind: kind.to_string(),
        visibility,
        open_at_vol,
        path: rel,
    })
}

fn render_frontmatter(name: &str, visibility: Visibility, open_at_vol: Option<u32>) -> String {
    let mut fm = format!("---\nname: {name}\ntype: setting\nvisibility: ");
    fm.push_str(match visibility {
        Visibility::Visible => "visible",
        Visibility::Hidden => "hidden",
    });
    fm.push('\n');
    if let Some(n) = open_at_vol {
        fm.push_str(&format!("openAtVol: {n}\n"));
    }
    fm.push_str("---");
    fm
}

/// 解析设定文件为元数据（正文在 body）。
pub fn parse_setting(raw: &str, rel_path: &str) -> SettingMeta {
    let dir = rel_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let kind = dir_kind(dir).unwrap_or("worldview").to_string();
    let fallback_name = rel_path
        .rsplit_once('/')
        .map(|(_, f)| f.trim_end_matches(".md"))
        .unwrap_or(rel_path);
    let mut meta = SettingMeta {
        name: fallback_name.to_string(),
        kind,
        visibility: Visibility::Visible,
        open_at_vol: None,
        path: rel_path.to_string(),
    };
    if let Some(fm) = frontmatter::split(raw) {
        for line in fm.lines() {
            if let Some(v) = line.strip_prefix("name:") {
                let v = v.trim();
                if !v.is_empty() {
                    meta.name = v.to_string();
                }
            } else if let Some(v) = line.strip_prefix("visibility:") {
                meta.visibility = match v.trim() {
                    "hidden" => Visibility::Hidden,
                    _ => Visibility::Visible,
                };
            } else if let Some(v) = line.strip_prefix("openAtVol:") {
                meta.open_at_vol = v.trim().parse::<u32>().ok();
            }
        }
    }
    meta
}

/// 列出全部设定条目。
pub fn list_settings(work_dir: &Path) -> Result<Vec<SettingMeta>, String> {
    let mut out = Vec::new();
    for dir in ["settings/characters", "settings/worldview", "settings/mysteries"] {
        let full = work_dir.join(dir);
        let Ok(entries) = std::fs::read_dir(&full) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let rel = format!("{dir}/{}", path.file_name().unwrap().to_string_lossy());
            if let Ok(raw) = std::fs::read_to_string(&path) {
                out.push(parse_setting(&raw, &rel));
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// 修改可见性（改写 frontmatter，保留正文）。
pub fn set_visibility(
    work_dir: &Path,
    rel_path: &str,
    visibility: Visibility,
    open_at_vol: Option<u32>,
) -> Result<(), String> {
    let raw = crate::works::read_file(work_dir, rel_path)?;
    let (meta_old, body) = match frontmatter::split_raw(&raw) {
        Some((fm, body)) => (parse_setting(&format!("---\n{fm}\n---"), rel_path), body),
        None => (parse_setting("", rel_path), raw.clone()),
    };
    let fm = render_frontmatter(&meta_old.name, visibility, open_at_vol);
    let content = format!("{fm}\n{body}");
    crate::works::write_file(work_dir, rel_path, &content)?;
    Ok(())
}

/// 关联泄漏检查：隐藏条目与可见条目的 4-gram（4 字滑窗）重叠检测。
/// 可见条目内容若大段复述隐藏条目（如"靠黑剑反哺凉气修行"泄露"反哺机制"），
/// 条目级可见性管不住——此检查在录入/修改后提示作者。
/// 返回警告行（空=通过）。
pub fn check_visibility_leak(work_dir: &Path) -> Result<Vec<String>, String> {
    fn cjk_grams(text: &str) -> std::collections::HashSet<String> {
        let cleaned: String = text
            .chars()
            .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
            .collect();
        let chars: Vec<char> = cleaned.chars().collect();
        chars
            .windows(4)
            .map(|w| w.iter().collect())
            .collect()
    }
    // 只比较正文（剥 frontmatter，避免 name/type/visibility 等英文元数据造成假重叠）
    fn body_of(work_dir: &Path, rel: &str) -> String {
        crate::works::read_file(work_dir, rel)
            .ok()
            .and_then(|raw| crate::frontmatter::split_raw(&raw).map(|(_, body)| body))
            .unwrap_or_default()
    }
    let all = list_settings(work_dir)?;
    let mut warnings = Vec::new();
    for hidden in all.iter().filter(|s| matches!(s.visibility, Visibility::Hidden)) {
        let h_grams = cjk_grams(&body_of(work_dir, &hidden.path));
        if h_grams.is_empty() {
            continue;
        }
        for visible in all.iter().filter(|s| !matches!(s.visibility, Visibility::Hidden)) {
            let v_grams = cjk_grams(&body_of(work_dir, &visible.path));
            let overlap = h_grams.intersection(&v_grams).count();
            // 阈值 4：约 7 字连续复述即提示（专有名词可能误报，检查为提示性而非阻断）
            if overlap >= 4 {
                warnings.push(format!(
                    "可见条目「{}」与隐藏条目「{}」有 {} 处 4 字连续片段相同——可见内容可能已泄露隐藏设定，建议改写措辞",
                    visible.name, hidden.name, overlap
                ));
            }
        }
    }
    Ok(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visibility_leak_detection() {
        let dir = std::env::temp_dir().join(format!("type-hime-leak-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::process::Command::new("git").current_dir(&dir).args(["init"]).output();

        // 隐藏条目与可见条目有大段重复措辞（"反哺丝丝凉气助其修行"）
        create_setting(&dir, "character", "林晚",
            "外门杂役。靠黑剑反哺丝丝凉气助其修行，无人知晓。", Visibility::Visible, None).unwrap();
        create_setting(&dir, "mystery", "黑剑来历",
            "黑剑内封残魂，可反哺丝丝凉气助无灵根者修行，此为本命剑之秘。", Visibility::Hidden, None).unwrap();
        let warnings = check_visibility_leak(&dir).unwrap();
        assert!(!warnings.is_empty(), "应检出关联泄漏：{warnings:?}");
        assert!(warnings[0].contains("林晚"));

        // 措辞不同则不报
        set_visibility(&dir, "settings/characters/林晚.md", Visibility::Visible, None).ok();
        std::fs::write(dir.join("settings/characters/林晚.md"),
            "---\nname: 林晚\ntype: setting\nvisibility: visible\n---\n外门杂役，体质特殊，修行另有机缘。").unwrap();
        let warnings2 = check_visibility_leak(&dir).unwrap();
        assert!(warnings2.is_empty(), "措辞不同不应报警：{warnings2:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn visibility_red_line() {
        let mk = |vis, open| SettingMeta {
            name: "t".into(),
            kind: "character".into(),
            visibility: vis,
            open_at_vol: open,
            path: "settings/characters/t.md".into(),
        };
        // 可见条目各卷均进 prompt
        assert!(is_prompt_visible(&mk(Visibility::Visible, None), 1));
        // 隐藏条目永不进
        assert!(!is_prompt_visible(&mk(Visibility::Hidden, None), 99));
        // 开放卷：卷 2 起，第 1 卷不可见，第 2 卷起可见
        assert!(!is_prompt_visible(&mk(Visibility::Visible, Some(2)), 1));
        assert!(is_prompt_visible(&mk(Visibility::Visible, Some(2)), 2));
        // 隐藏优先于开放卷
        assert!(!is_prompt_visible(&mk(Visibility::Hidden, Some(1)), 5));
    }

    #[test]
    fn create_parse_set_visibility_roundtrip() {
        let dir = std::env::temp_dir().join(format!("type-hime-lore-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::process::Command::new("git").current_dir(&dir).args(["init"]).output();

        let meta = create_setting(&dir, "mystery", "黑龙身份", "真身是先帝私生子。", Visibility::Hidden, Some(2)).unwrap();
        assert_eq!(meta.path, "settings/mysteries/黑龙身份.md");

        // 持久化后重读：隐藏与开放卷都在
        let list = list_settings(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].visibility, Visibility::Hidden);
        assert_eq!(list[0].open_at_vol, Some(2));
        assert!(!is_prompt_visible(&list[0], 1));

        // 改为可见（无开放卷）
        set_visibility(&dir, &meta.path, Visibility::Visible, None).unwrap();
        let list2 = list_settings(&dir).unwrap();
        assert_eq!(list2[0].visibility, Visibility::Visible);
        assert!(is_prompt_visible(&list2[0], 1));
        // 正文保留
        let raw = std::fs::read_to_string(dir.join(&meta.path)).unwrap();
        assert!(raw.contains("先帝私生子"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
