//! 连续性账本（D0-10）：结构化 md，记录角色状态/已交代事实/伏笔登记。
//! 完稿一章后 agent 提议回写（切片 7）；装配时注入分层压缩摘要（出场规模化修复）。
//! 账本不受上下文窗口限制，是长篇"记忆缺锚"的解。

use serde::{Deserialize, Serialize};

pub const LEDGER_PATH: &str = "ledger/账本.md";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub title: String,
    /// 条目正文（### 之下到下一个标题之前）
    pub body: String,
    /// 最近更新的章号（从 body 中的 ch-00N / 第N章 标记提取；None=未标注）
    #[serde(default)]
    pub last_touched: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    /// 角色状态条目
    pub characters: Vec<LedgerEntry>,
    /// 已交代事实
    pub facts: Vec<LedgerEntry>,
    /// 伏笔登记
    pub foreshadows: Vec<LedgerEntry>,
    /// 时间线（### 章号 小节，body=距开篇天数+关键事件）
    #[serde(default)]
    pub timeline: Vec<LedgerEntry>,
}

/// 账本摘要压缩选项。
#[derive(Debug, Clone)]
pub struct SummaryOpts {
    /// 当前章号（如 6）
    pub current_chapter: u32,
    /// 活跃窗口：最近 N 章有更新的条目注入完整正文
    pub window: u32,
    /// 全量模式（审校/整理/回写用，不做压缩）
    pub full: bool,
}

/// 从文本中提取最高章号标记（ch-5 / ch-005 / 第5章）。
pub fn extract_touched_chapter(text: &str) -> Option<u32> {
    let mut max: Option<u32> = None;
    let mut bump = |n: u32, max: &mut Option<u32>| {
        if max.map_or(true, |m| n > m) {
            *max = Some(n);
        }
    };
    // ch-00N / ch-N
    let mut rest = text;
    while let Some(pos) = rest.find("ch-") {
        let after = &rest[pos + 3..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u32>() {
            bump(n, &mut max);
        }
        rest = after;
    }
    // 第N章
    let mut rest = text;
    while let Some(pos) = rest.find('第') {
        let after = &rest[pos + '第'.len_utf8()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = digits.parse::<u32>() {
            bump(n, &mut max);
        }
        rest = after;
    }
    max
}

/// 解析账本：`## 角色状态` / `## 已交代事实` / `## 伏笔登记` / `## 时间线` 二级小节，
/// 各自下的 `###` 三级小节为条目（标题 + 正文，body 中的章号提取为最近更新）。
pub fn parse(raw: &str) -> Ledger {
    let mut ledger = Ledger::default();
    // 0=characters 1=facts 2=foreshadows 3=timeline
    let mut bucket: Vec<(u8, String, String)> = Vec::new();
    let mut cur: Option<u8> = None;
    let mut title: Option<String> = None;
    let mut body = String::new();
    for line in raw.lines() {
        if line.starts_with("## ") || line.starts_with("### ") {
            // 先收上一条
            if let Some(t) = title.take() {
                bucket.push((cur.unwrap_or(2), t, std::mem::take(&mut body)));
            }
        }
        if line.starts_with("## ") {
            cur = match line.trim_start_matches("## ").trim() {
                "角色状态" => Some(0),
                "已交代事实" => Some(1),
                "伏笔登记" => Some(2),
                "时间线" => Some(3),
                _ => None,
            };
            title = None;
        } else if line.starts_with("### ") {
            let t = line.trim_start_matches("### ").trim().to_string();
            if !t.is_empty() && cur.is_some() {
                title = Some(t);
            }
        } else if title.is_some() {
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(line.trim());
        }
    }
    if let Some(t) = title.take() {
        bucket.push((cur.unwrap_or(2), t, body));
    }
    for (b, title, body) in bucket {
        let last_touched = extract_touched_chapter(&body).or_else(|| {
            // 时间线条目标题本身是章号
            extract_touched_chapter(&title)
        });
        let e = LedgerEntry { title, body, last_touched };
        match b {
            0 => ledger.characters.push(e),
            1 => ledger.facts.push(e),
            3 => ledger.timeline.push(e),
            _ => ledger.foreshadows.push(e),
        }
    }
    ledger
}

/// 装配用摘要（分层压缩）：
/// - 活跃条目（最近 window 章有更新）：标题 + 正文（截 200 字）
/// - 休眠条目：仅标题一行（伏笔即使休眠也保留标题——防提前说破需要 agent 知晓其存在）
pub fn render_summary_with(ledger: &Ledger, opts: &SummaryOpts) -> String {
    let mut out = String::from("# 连续性账本摘要\n");
    let is_active = |e: &LedgerEntry| -> bool {
        opts.full
            || e.last_touched
                .map_or(true, |t| t + opts.window >= opts.current_chapter)
    };
    let mut sec = |out: &mut String, title: &str, items: &[LedgerEntry]| {
        if items.is_empty() {
            return;
        }
        out.push_str(&format!("\n## {title}\n"));
        for e in items {
            if !is_active(e) {
                out.push_str(&format!("- {}（长期未变更，略）\n", e.title));
                continue;
            }
            let body: String = e.body.chars().take(200).collect();
            if body.trim().is_empty() {
                out.push_str(&format!("- {}\n", e.title));
            } else {
                out.push_str(&format!("- {}：{}\n", e.title, body.trim()));
            }
        }
    };
    sec(&mut out, "角色当前状态", &ledger.characters);
    sec(&mut out, "已交代事实（不可矛盾）", &ledger.facts);
    sec(&mut out, "伏笔登记（未回收前不得说破）", &ledger.foreshadows);
    // 时间线：注入最近锚点（全量模式给全部，压缩模式给最近 5 条）
    if !ledger.timeline.is_empty() {
        out.push_str("\n## 时间线（硬约束：正文中的时间表述必须与之相容）\n");
        let items: Vec<&LedgerEntry> = if opts.full {
            ledger.timeline.iter().collect()
        } else {
            let mut sorted: Vec<&LedgerEntry> = ledger.timeline.iter().collect();
            sorted.sort_by_key(|e| e.last_touched.unwrap_or(0));
            sorted.into_iter().rev().take(5).collect::<Vec<_>>().into_iter().rev().collect()
        };
        for e in items {
            let body: String = e.body.chars().take(120).collect();
            out.push_str(&format!("- {}：{}\n", e.title, body.trim()));
        }
    }
    if out == "# 连续性账本摘要\n" {
        out.push_str("\n（账本为空：尚无角色状态与事实记录）");
    }
    out
}

/// 全量摘要（向后兼容：审校/整理/账本回写读全量）。
pub fn render_summary(ledger: &Ledger) -> String {
    render_summary_with(
        ledger,
        &SummaryOpts { current_chapter: 0, window: 0, full: true },
    )
}

pub fn read_ledger(work_dir: &std::path::Path) -> Result<Ledger, String> {
    let raw = crate::works::read_file(work_dir, LEDGER_PATH)?;
    Ok(parse(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touched_extraction() {
        assert_eq!(extract_touched_chapter("ch-002 中更新，后又提 ch-005"), Some(5));
        assert_eq!(extract_touched_chapter("第3章发生"), Some(3));
        assert_eq!(extract_touched_chapter("没有标记"), None);
        assert_eq!(extract_touched_chapter("ch-007 与 第9章 都有"), Some(9));
    }

    #[test]
    fn layered_summary() {
        let raw = r#"---
type: ledger
---
# 连续性账本

## 角色状态

### 林晚
修为炼气三层。ch-005 更新：大比报名。

### 谢无咎
执法堂副手。ch-001 出场。

## 伏笔登记

### 黑龙身份（未回收）
埋设：第1章
"#;
        let l = parse(raw);
        // 分层：当前第 6 章，窗口 3 → 林晚(5)活跃，谢无咎(1)休眠
        let s = render_summary_with(
            &l,
            &SummaryOpts { current_chapter: 6, window: 3, full: false },
        );
        assert!(s.contains("大比报名"), "活跃条目应有正文");
        assert!(s.contains("谢无咎（长期未变更，略）"), "休眠条目应压缩为标题");
        // 伏笔即使未标注 touched（None）也保留标题
        assert!(s.contains("黑龙身份"));
        // full 模式不压缩
        let full = render_summary(&l);
        assert!(full.contains("执法堂副手"));

        // 窗口放宽后谢无咎也活跃
        let s2 = render_summary_with(
            &l,
            &SummaryOpts { current_chapter: 4, window: 3, full: false },
        );
        assert!(s2.contains("执法堂副手"));
    }

    #[test]
    fn parse_and_summary() {
        let raw = r#"---
type: ledger
---
# 连续性账本

## 角色状态

### 林晚
位置：青云宗外门 / 修为：炼气三层

## 已交代事实

### 黑令牌来历
黑令牌为前朝国师遗物

## 伏笔登记

### 黑龙身份（未回收）
埋设：第1章 / 计划回收：第一卷末
"#;
        let l = parse(raw);
        assert_eq!(l.characters.len(), 1);
        assert_eq!(l.facts.len(), 1);
        assert_eq!(l.foreshadows.len(), 1);

        let s = render_summary(&l);
        assert!(s.contains("- 林晚"));
        assert!(s.contains("- 黑令牌来历"));
        assert!(s.contains("不得说破"));

        let empty = render_summary(&Ledger::default());
        assert!(empty.contains("账本为空"));
    }
}
