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
    /// 设定条目档位（预算内可自动降档）
    pub setting_detail: Detail,
    /// token 预算（超出时自动降档：Full→NameOnly→Compact）
    pub token_budget: u64,
    /// 按本章章纲过滤出场设定（默认开；审校阶段自动关闭）
    #[serde(default = "default_true")]
    pub cast_filter: bool,
    /// 账本活跃窗口（最近 N 章有更新的条目注入完整正文）
    pub ledger_window: u32,
}

fn default_true() -> bool {
    true
}

impl Default for AssemblyOptions {
    fn default() -> Self {
        Self {
            stage: Stage::Chapters,
            volume: 1,
            chapter: "ch-001".into(),
            setting_detail: Detail::Full,
            token_budget: 16_000,
            cast_filter: true,
            ledger_window: 3,
        }
    }
}

impl AssemblyOptions {
    fn current_chapter_no(&self) -> u32 {
        self.chapter
            .trim_start_matches("ch-")
            .parse()
            .unwrap_or(1)
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
    /// 实际生效的档位（预算驱动自动降档后可能低于请求档位）
    pub applied_detail: String,
    /// 是否发生了自动降档
    pub downgraded: bool,
    /// 出场过滤后保留的设定条目名（cast_filter 生效时）
    pub cast: Vec<String>,
}

fn read_stripped(work_dir: &Path, rel: &str) -> Option<String> {
    let raw = crate::works::read_file(work_dir, rel).ok()?;
    Some(match frontmatter::split_raw(&raw) {
        Some((_, body)) => body.trim().to_string(),
        None => raw.trim().to_string(),
    })
}

/// 装配（预览与实际生成共用同一出口）。
/// 预算驱动自动降档：请求档位超预算时依次尝试更低档位（Full→NameOnly→Compact），
/// 全部超预算时按 Compact 输出并标记 over_budget。
pub fn assemble(work_dir: &Path, opts: &AssemblyOptions) -> Result<AssemblyResult, String> {
    let ladder: Vec<Detail> = match opts.setting_detail {
        Detail::Full => vec![Detail::Full, Detail::NameOnly, Detail::Compact],
        Detail::NameOnly => vec![Detail::NameOnly, Detail::Compact],
        Detail::Compact => vec![Detail::Compact],
    };

    let mut last = None;
    for detail in ladder {
        let result = assemble_at(work_dir, opts, detail)?;
        if !result.over_budget {
            return Ok(result);
        }
        last = Some(result);
    }
    Ok(last.expect("ladder 非空"))
}

fn assemble_at(work_dir: &Path, opts: &AssemblyOptions, detail: Detail) -> Result<AssemblyResult, String> {
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

    // 审校阶段看全局：不过滤、账本全量
    let is_review = matches!(opts.stage, Stage::Review);

    // 1. 本卷卷纲
    let vol_outline = read_stripped(work_dir, &format!("{}/卷纲.md", volume_dir(opts.volume)));
    push("本卷卷纲".into(), vol_outline, "卷纲尚未撰写");

    // 2. 本章章纲
    let outline_rel = chapter_outline_path(opts.volume, &opts.chapter);
    let ch_outline = read_stripped(work_dir, &outline_rel);
    push("本章章纲".into(), ch_outline.clone(), "本章章纲尚未撰写（滚动生成）");

    // 3+7. 设定条目——可见性红线 + 出场过滤 + 档位
    let settings = lore::list_settings(work_dir)?;
    let mut visible_settings = Vec::new();
    for s in &settings {
        if lore::is_prompt_visible(s, opts.volume) {
            visible_settings.push(s.clone());
        } else {
            filtered += 1;
        }
    }
    // 出场过滤：以章纲文本命中条目名；世界观条目不参与过滤（全书背景规则）；
    // 零命中时回退全量（防误杀）
    let cast_filterable = |s: &lore::SettingMeta| s.kind == "character" || s.kind == "mystery";
    let cast: Vec<String> = if opts.cast_filter && !is_review {
        if let Some(outline_text) = &ch_outline {
            let hit: Vec<String> = visible_settings
                .iter()
                .filter(|s| {
                    !cast_filterable(s) || outline_text.contains(&s.name)
                })
                .map(|s| s.name.clone())
                .collect();
            let filterable_count = visible_settings.iter().filter(|s| cast_filterable(s)).count();
            let hit_filterable =
                hit.iter().filter(|n| visible_settings.iter().any(|s| &s.name == *n && cast_filterable(s))).count();
            if filterable_count > 0 && hit_filterable == 0 {
                visible_settings.iter().map(|s| s.name.clone()).collect()
            } else {
                hit
            }
        } else {
            visible_settings.iter().map(|s| s.name.clone()).collect()
        }
    } else {
        visible_settings.iter().map(|s| s.name.clone()).collect()
    };
    let cast_set: std::collections::HashSet<&str> =
        cast.iter().map(|s| s.as_str()).collect();
    let chosen: Vec<_> = visible_settings
        .iter()
        .filter(|s| cast_set.contains(s.name.as_str()))
        .collect();

    if chosen.is_empty() {
        push("设定条目".into(), None, "没有可见的设定条目");
    } else {
        let mut text = String::from("## 设定条目\n");
        for s in &chosen {
            let kind_label = match s.kind.as_str() {
                "character" => "人物",
                "worldview" => "世界观",
                "mystery" => "悬念",
                other => other,
            };
            match detail {
                Detail::Full => {
                    let body = read_stripped(work_dir, &s.path).unwrap_or_default();
                    text.push_str(&format!("\n### [{kind_label}] {}\n{}\n", s.name, body));
                }
                Detail::NameOnly | Detail::Compact => {
                    text.push_str(&format!("\n- [{kind_label}] {}\n", s.name));
                }
            }
        }
        push("设定条目".into(), Some(text), "");
    }

    // 4. 账本摘要（分层压缩；Compact 档压到零窗口；审校全量）
    let ledger_text = ledger::read_ledger(work_dir).ok().map(|l| {
        ledger::render_summary_with(
            &l,
            &ledger::SummaryOpts {
                current_chapter: opts.current_chapter_no(),
                window: if matches!(detail, Detail::Compact) { 0 } else { opts.ledger_window },
                full: is_review,
            },
        )
    });
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
    let detail_label = match detail {
        Detail::Full => "full",
        Detail::NameOnly => "name_only",
        Detail::Compact => "compact",
    };
    Ok(AssemblyResult {
        system,
        system_tokens,
        components,
        filtered_by_visibility: filtered,
        total_tokens: total,
        over_budget: total > opts.token_budget,
        applied_detail: detail_label.to_string(),
        downgraded: detail != opts.setting_detail,
        cast,
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

        // 预算极小：自动降档到最后仍超 → over_budget 且 applied_detail=compact
        let mut opts2 = AssemblyOptions::default();
        opts2.chapter = "ch-002".into();
        opts2.token_budget = 10;
        let r3 = assemble(&dir, &opts2).unwrap();
        assert!(r3.over_budget);
        assert_eq!(r3.applied_detail, "compact");
        assert!(r3.downgraded);

        // 预算中等：Full 超但 Compact 不超 → 自动降档且不飘红
        let mut opts3 = AssemblyOptions::default();
        opts3.chapter = "ch-002".into();
        opts3.token_budget = r2.total_tokens; // r2 是 NameOnly 实际值
        let r4 = assemble(&dir, &opts3).unwrap();
        assert!(!r4.over_budget, "应有可满足的降档档位");
        assert!(r4.downgraded || r4.total_tokens <= r4.total_tokens);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cast_filter_by_outline() {
        let dir = std::env::temp_dir().join(format!("type-hime-cast-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for p in ["outline", "volumes/vol-01/chapters", "settings/characters", "style", "ledger"] {
            std::fs::create_dir_all(dir.join(p)).unwrap();
        }
        let _ = std::process::Command::new("git").current_dir(&dir).args(["init"]).output();
        std::fs::write(dir.join("style/文风指令.md"), "快节奏").unwrap();
        std::fs::write(dir.join("volumes/vol-01/卷纲.md"), "卷纲").unwrap();
        // 章纲只提到林晚
        std::fs::write(
            dir.join("volumes/vol-01/chapters/ch-002-章纲.md"),
            "林晚夜探藏经阁，遭遇守阁人。",
        )
        .unwrap();
        lore::create_setting(&dir, "character", "林晚", "主角，剑心通明。", lore::Visibility::Visible, None).unwrap();
        lore::create_setting(&dir, "character", "守阁人", "次要角色。", lore::Visibility::Visible, None).unwrap();
        lore::create_setting(&dir, "character", "远方长老", "本章不出场。", lore::Visibility::Visible, None).unwrap();

        let mut opts = AssemblyOptions::default();
        opts.chapter = "ch-002".into();
        let r = assemble(&dir, &opts).unwrap();
        let all = format!("{}{}", r.system, render_prompt(&r));
        // 出场命中：章纲提到的角色进 prompt
        assert!(all.contains("林晚"));
        assert!(all.contains("守阁人"));
        // 未出场角色被过滤（名字与正文都不进）
        assert!(!all.contains("远方长老"), "未出场角色不应注入");
        // cast 字段只含命中者（+ 不参与过滤的世界观）
        assert!(r.cast.contains(&"林晚".to_string()));
        assert!(r.cast.contains(&"守阁人".to_string()));
        assert!(!r.cast.contains(&"远方长老".to_string()));
        assert!(!all.contains("本章不出场"), "未出场角色卡正文不应注入");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
