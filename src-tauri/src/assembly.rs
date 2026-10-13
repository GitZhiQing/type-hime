//! 规则装配引擎（D0-7）：按阶段/规则组装 prompt。
//! 正文阶段默认装配 = 文风指令 + 本卷卷纲 + 本章章纲 + 出场角色卡 + 账本摘要 + 最近章结尾。
//! **可见性过滤是红线**：所有设定条目在装配出口强制过 is_prompt_visible，档位调不到隐藏条目。
//! token 为本地近似估算（D9：CJK≈1 字/token，其他≈4 字符/token）。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::frontmatter;
use crate::ledger;
use crate::lore::{self, Detail};
use crate::pipeline::Stage;

/// 章纲文件名约定：ch-001-章纲.md；正文：ch-001.md
pub fn chapter_outline_path(volume: u32, chapter: &str) -> String {
    let vol = volume_dir(volume);
    format!("{vol}/chapters/{chapter}-章纲.md")
}

pub fn chapter_path(volume: u32, chapter: &str) -> String {
    let vol = volume_dir(volume);
    format!("{vol}/chapters/{chapter}.md")
}

pub fn volume_dir(volume: u32) -> String {
    format!("volumes/vol-{volume:02}")
}

/// 近似 token 估算（UI 标注"估算"）。
pub fn estimate_tokens(text: &str) -> u64 {
    let mut cjk: u64 = 0;
    let mut other: u64 = 0;
    for ch in text.chars() {
        if is_cjk(ch) {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk + other.div_ceil(4)
}

fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x3000..=0x303F).contains(&cp) // CJK 标点
        || (0xFF00..=0xFFEF).contains(&cp) // 全角
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssemblyOptions {
    pub stage: Stage,
    pub volume: u32,
    /// 目标章节 id（如 ch-001）
    pub chapter: String,
    /// 设定条目档位
    pub setting_detail: Detail,
    /// token 预算
    pub token_budget: u64,
}

impl Default for AssemblyOptions {
    fn default() -> Self {
        Self {
            stage: Stage::Chapters,
            volume: 1,
            chapter: "ch-001".into(),
            setting_detail: Detail::Full,
            token_budget: 16_000,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AssembledComponent {
    pub name: String,
    pub content: String,
    pub tokens: u64,
    /// false = 上游产物缺失，未装配
    pub included: bool,
    pub missing_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssemblyResult {
    /// 置于最前的系统提示词（文风指令 + 阶段任务说明）
    pub system: String,
    pub system_tokens: u64,
    pub components: Vec<AssembledComponent>,
    /// 被可见性红线过滤掉的条目数（审计展示）
    pub filtered_by_visibility: usize,
    pub total_tokens: u64,
    pub over_budget: bool,
}

fn read_stripped(work_dir: &Path, rel: &str) -> Option<String> {
    let raw = crate::works::read_file(work_dir, rel).ok()?;
    Some(match frontmatter::split_raw(&raw) {
        Some((_, body)) => body.trim().to_string(),
        None => raw.trim().to_string(),
    })
}

/// 装配（预览与实际生成共用同一出口）。
pub fn assemble(work_dir: &Path, opts: &AssemblyOptions) -> Result<AssemblyResult, String> {
    let mut components: Vec<AssembledComponent> = Vec::new();
    let mut filtered = 0usize;

    let mut push = |name: String, content: Option<String>, hint: &str| {
        match content {
            Some(c) if !c.trim().is_empty() => components.push(AssembledComponent {
                tokens: estimate_tokens(&c),
                name,
                content: c,
                included: true,
                missing_hint: None,
            }),
            _ => components.push(AssembledComponent {
                name,
                content: String::new(),
                tokens: 0,
                included: false,
                missing_hint: Some(hint.to_string()),
            }),
        }
    };

    // 1. 本卷卷纲
    let vol_outline = read_stripped(work_dir, &format!("{}/卷纲.md", volume_dir(opts.volume)));
    push("本卷卷纲".into(), vol_outline, "卷纲尚未撰写");

    // 2. 本章章纲
    let ch_outline = read_stripped(work_dir, &chapter_outline_path(opts.volume, &opts.chapter));
    push("本章章纲".into(), ch_outline, "本章章纲尚未撰写（滚动生成）");

    // 3+7. 设定条目（角色/世界观/悬念）——可见性红线 + 档位
    let settings = lore::list_settings(work_dir)?;
    let mut visible_settings = Vec::new();
    for s in &settings {
        if lore::is_prompt_visible(s, opts.volume) {
            visible_settings.push(s.clone());
        } else {
            filtered += 1;
        }
    }
    if visible_settings.is_empty() {
        push("设定条目".into(), None, "没有可见的设定条目");
    } else {
        let mut text = String::from("## 设定条目\n");
        for s in &visible_settings {
            let kind_label = match s.kind.as_str() {
                "character" => "人物",
                "worldview" => "世界观",
                "mystery" => "悬念",
                other => other,
            };
            match opts.setting_detail {
                Detail::Full => {
                    let body = read_stripped(work_dir, &s.path).unwrap_or_default();
                    text.push_str(&format!("\n### [{kind_label}] {}\n{}\n", s.name, body));
                }
                Detail::NameOnly => {
                    text.push_str(&format!("\n- [{kind_label}] {}\n", s.name));
                }
            }
        }
        push("设定条目".into(), Some(text), "");
    }

    // 4. 账本摘要
    let ledger_text = ledger::read_ledger(work_dir)
        .ok()
        .map(|l| ledger::render_summary(&l));
    push("账本摘要".into(), ledger_text, "账本为空");

    // 5. 最近章结尾（上一章正文尾部）
    match previous_chapter(work_dir, opts) {
        Some((ch_id, full)) => {
            let chars: Vec<char> = full.chars().collect();
            let start = chars.len().saturating_sub(600);
            let tail: String = chars[start..].iter().collect();
            push(
                format!("上一章（{ch_id}）结尾"),
                Some(format!("…{tail}")),
                "",
            );
        }
        None => push("上一章结尾".into(), None, "这是第一章，无上文"),
    }

    // 系统提示词：文风指令 + 阶段任务
    let style = read_stripped(work_dir, "style/文风指令.md")
        .unwrap_or_else(|| "（未配置文风指令，使用默认：简体中文网文风格）".into());
    let stage_task = match opts.stage {
        Stage::ChapterOutline => "任务：依据卷纲与账本，为本章撰写章纲（情节点/冲突/章末钩子）。",
        Stage::Chapters => "任务：依据章纲与全部上下文，撰写本章正文全文。",
        Stage::Review => "任务：对照账本与设定审校本章正文，输出问题清单与修复提议。",
        _ => "任务：按当前流水线阶段工作。",
    };
    let system = format!("# 文风指令\n{style}\n\n# 阶段任务\n{stage_task}");
    let system_tokens = estimate_tokens(&system);

    let total = system_tokens + components.iter().map(|c| c.tokens).sum::<u64>();
    Ok(AssemblyResult {
        system,
        system_tokens,
        components,
        filtered_by_visibility: filtered,
        total_tokens: total,
        over_budget: total > opts.token_budget,
    })
}

/// 找到目标章之前最近的一章正文（按文件名排序）。
fn previous_chapter(work_dir: &Path, opts: &AssemblyOptions) -> Option<(String, String)> {
    let dir = work_dir.join(volume_dir(opts.volume)).join("chapters");
    let mut chapters: Vec<String> = std::fs::read_dir(&dir)
        .ok()?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".md") && !name.contains("-章纲") {
                Some(name.trim_end_matches(".md").to_string())
            } else {
                None
            }
        })
        .collect();
    chapters.sort();
    // 目标章在正文文件列表中：取其前一章；目标章尚无正文（待生成）：取最新一章作上文
    let prev: &String = match chapters.iter().position(|c| c == &opts.chapter) {
        Some(0) => return None,
        Some(i) => &chapters[i - 1],
        None if !chapters.is_empty() => chapters.last()?,
        None => return None,
    };
    let content = read_stripped(work_dir, &chapter_path(opts.volume, prev))?;
    Some((prev.clone(), content))
}

/// 组装最终 prompt（系统提示词 + 各组件正文），生成用。
pub fn render_prompt(result: &AssemblyResult) -> String {
    let mut out = String::new();
    for c in &result.components {
        if c.included {
            out.push_str(&format!("\n\n# {}\n{}", c.name, c.content));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_estimate() {
        // 纯中文 1:1
        assert_eq!(estimate_tokens("一二三四五"), 5);
        // ASCII 4:1
        assert_eq!(estimate_tokens("abcdefgh"), 2);
        // 混合
        assert_eq!(estimate_tokens("一二ab"), 3);
    }

    #[test]
    fn assembly_visibility_and_detail() {
        let dir = std::env::temp_dir().join(format!("type-hime-asm-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for p in ["outline", "volumes/vol-01/chapters", "settings/characters", "style", "ledger"] {
            std::fs::create_dir_all(dir.join(p)).unwrap();
        }
        let _ = std::process::Command::new("git").current_dir(&dir).args(["init"]).output();
        std::fs::write(dir.join("style/文风指令.md"), "---\ntype: style\n---\n快节奏网文风").unwrap();
        std::fs::write(dir.join("volumes/vol-01/卷纲.md"), "第一卷：入门到筑基").unwrap();
        std::fs::write(dir.join("volumes/vol-01/chapters/ch-002-章纲.md"), "第二章章纲：夜探藏经阁").unwrap();
        std::fs::write(dir.join("volumes/vol-01/chapters/ch-001.md"), "第一章正文").unwrap();
        std::fs::write(
            dir.join("volumes/vol-01/chapters/ch-002.md"),
            format!("第二章正文{}", "很长的内容".repeat(200)),
        )
        .unwrap();
        lore::create_setting(&dir, "character", "林晚", "外门弟子，剑心通明。", lore::Visibility::Visible, None).unwrap();
        lore::create_setting(&dir, "mystery", "黑龙身份", "真身先帝私生子。", lore::Visibility::Hidden, None).unwrap();
        std::fs::write(
            dir.join("ledger/账本.md"),
            "# 账本\n\n## 角色状态\n\n### 林晚\n炼气三层\n",
        )
        .unwrap();

        // Full 档
        let mut opts = AssemblyOptions::default();
        opts.chapter = "ch-002".into();
        let r = assemble(&dir, &opts).unwrap();
        assert!(r.system.contains("快节奏网文风"));
        assert!(r.system.contains("撰写本章正文"));
        // 隐藏条目被红线过滤
        assert_eq!(r.filtered_by_visibility, 1);
        let all = format!("{}{}", r.system, render_prompt(&r));
        assert!(!all.contains("先帝私生子"), "隐藏悬念不得进 prompt");
        assert!(all.contains("剑心通明"));
        // 卷纲/章纲/账本/上一章结尾都在
        assert!(all.contains("入门到筑基"));
        assert!(all.contains("夜探藏经阁"));
        assert!(all.contains("炼气三层"));
        assert!(all.contains("上一章（ch-001）结尾"));
        assert!(r.total_tokens > 0);

        // 切 NameOnly：正文细节消失，只留名称
        opts.setting_detail = Detail::NameOnly;
        let r2 = assemble(&dir, &opts).unwrap();
        let all2 = format!("{}{}", r2.system, render_prompt(&r2));
        assert!(all2.contains("林晚"));
        assert!(!all2.contains("剑心通明"), "仅名称档不进条目正文");

        // 预算飘红
        opts.token_budget = 10;
        let r3 = assemble(&dir, &opts).unwrap();
        assert!(r3.over_budget);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
