//! 连续性账本（D0-10）：结构化 md，记录角色状态/已交代事实/伏笔登记。
//! 完稿一章后 agent 提议回写（切片 7）；装配时注入摘要（切片 6）。
//! 账本不受上下文窗口限制，是长篇"记忆缺锚"的解。

use serde::{Deserialize, Serialize};

pub const LEDGER_PATH: &str = "ledger/账本.md";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub title: String,
    /// 条目正文（### 之下到下一个标题之前）
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    /// 角色状态条目
    pub characters: Vec<LedgerEntry>,
    /// 已交代事实
    pub facts: Vec<LedgerEntry>,
    /// 伏笔登记
    pub foreshadows: Vec<LedgerEntry>,
}

/// 解析账本：`## 角色状态` / `## 已交代事实` / `## 伏笔登记` 三个二级小节，
/// 各自下的 `###` 三级小节为条目（标题 + 正文）。
pub fn parse(raw: &str) -> Ledger {
    let mut ledger = Ledger::default();
    // 0=characters 1=facts 2=foreshadows
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
        let e = LedgerEntry { title, body };
        match b {
            0 => ledger.characters.push(e),
            1 => ledger.facts.push(e),
            _ => ledger.foreshadows.push(e),
        }
    }
    ledger
}

/// 装配用摘要（切片 6 注入 prompt）。
pub fn render_summary(ledger: &Ledger) -> String {
    let mut out = String::from("# 连续性账本摘要\n");
    let mut sec = |out: &mut String, title: &str, items: &[LedgerEntry]| {
        if !items.is_empty() {
            out.push_str(&format!("\n## {title}\n"));
            for e in items {
                let body: String = e.body.chars().take(200).collect();
                if body.trim().is_empty() {
                    out.push_str(&format!("- {}\n", e.title));
                } else {
                    out.push_str(&format!("- {}：{}\n", e.title, body.trim()));
                }
            }
        }
    };
    sec(&mut out, "角色当前状态", &ledger.characters);
    sec(&mut out, "已交代事实（不可矛盾）", &ledger.facts);
    sec(&mut out, "伏笔登记（未回收前不得说破）", &ledger.foreshadows);
    if out == "# 连续性账本摘要\n" {
        out.push_str("\n（账本为空：尚无角色状态与事实记录）");
    }
    out
}

pub fn read_ledger(work_dir: &std::path::Path) -> Result<Ledger, String> {
    let raw = crate::works::read_file(work_dir, LEDGER_PATH)?;
    Ok(parse(&raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_summary() {
        let raw = r#"---
type: ledger
---
# 连续性账本

## 角色状态

### 林晚
位置：青云宗外门 / 修为：炼气三层

### 谢无咎
身份：执法堂副手，已起疑

## 已交代事实

### 黑令牌来历
黑令牌为前朝国师遗物

## 伏笔登记

### 黑龙身份（未回收）
埋设：第1章 / 计划回收：第一卷末
"#;
        let l = parse(raw);
        assert_eq!(l.characters.len(), 2);
        assert_eq!(l.facts.len(), 1);
        assert_eq!(l.foreshadows.len(), 1);

        let s = render_summary(&l);
        assert!(s.contains("### 林晚".trim_start_matches("### ")));
        assert!(s.contains("- 林晚"));
        assert!(s.contains("- 黑令牌来历"));
        assert!(s.contains("不得说破"));

        // 空账本
        let empty = render_summary(&Ledger::default());
        assert!(empty.contains("账本为空"));
    }
}
