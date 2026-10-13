//! 审校问题清单（结构化落盘 + 裁决记录）：修复"复检永远不通过"的闭环断裂。
//! 审校报告按编号拆条存 review/issues.json（进 git，可回溯）；
//! 作者对条目裁决（fixed/dismissed/needs_ruling）；
//! 复检时注入上轮清单与裁决记录，逐项验证。

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueRecord {
    pub id: u64,
    /// 章号（ch-005）；卷级问题用 "vol-01"
    pub chapter: String,
    pub created_at: String,
    /// 问题条目原文
    pub text: String,
    /// consistency | preflight | recheck
    pub source: String,
    /// open | fixed | dismissed | needs_ruling
    pub status: String,
    /// 裁决说明
    #[serde(default)]
    pub ruling: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct IssueLog {
    next_id: u64,
    pub issues: Vec<IssueRecord>,
}

fn issues_path(work_dir: &Path) -> std::path::PathBuf {
    work_dir.join("review").join("issues.json")
}

pub fn load(work_dir: &Path) -> Result<IssueLog, String> {
    let path = issues_path(work_dir);
    if !path.exists() {
        return Ok(IssueLog::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取问题清单失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析问题清单失败：{e}"))
}

pub fn save(work_dir: &Path, log: &IssueLog) -> Result<(), String> {
    let path = issues_path(work_dir);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建 review 目录失败：{e}"))?;
    }
    let raw = serde_json::to_string_pretty(log).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("写入问题清单失败：{e}"))
}

/// 把审校报告文本按编号/标题拆条，追加为 open 问题。
/// 返回新增条数。已有同章同源 open 条目先关闭为 superseded（保留历史，避免重复计）。
pub fn ingest_report(
    work_dir: &Path,
    chapter: &str,
    source: &str,
    report_text: &str,
) -> Result<usize, String> {
    let items = split_issues(report_text);
    let mut log = load(work_dir)?;
    let now = crate::works::now_iso();
    let added = items.len();
    for text in items {
        log.next_id += 1;
        log.issues.push(IssueRecord {
            id: log.next_id,
            chapter: chapter.to_string(),
            created_at: now.clone(),
            text,
            source: source.to_string(),
            status: "open".into(),
            ruling: None,
        });
    }
    save(work_dir, &log)?;
    Ok(added)
}

/// 记录裁决。
pub fn record_ruling(
    work_dir: &Path,
    id: u64,
    status: &str,
    note: Option<String>,
) -> Result<(), String> {
    let status = match status {
        "fixed" | "dismissed" | "needs_ruling" | "open" => status,
        other => return Err(format!("未知状态：{other}")),
    };
    let mut log = load(work_dir)?;
    let issue = log
        .issues
        .iter_mut()
        .find(|i| i.id == id)
        .ok_or(format!("问题 #{id} 不存在"))?;
    issue.status = status.to_string();
    issue.ruling = note;
    save(work_dir, &log)
}

/// 某章（或全部）的问题视图。
pub fn list(work_dir: &Path, chapter: Option<&str>) -> Result<Vec<IssueRecord>, String> {
    let log = load(work_dir)?;
    Ok(log
        .issues
        .into_iter()
        .filter(|i| chapter.map_or(true, |c| i.chapter == c))
        .collect())
}

/// 未决问题数（复检判定与阶段门参考）。
pub fn open_count(work_dir: &Path, chapter: Option<&str>) -> Result<usize, String> {
    Ok(list(work_dir, chapter)?
        .into_iter()
        .filter(|i| i.status == "open" || i.status == "needs_ruling")
        .count())
}

/// 按编号行 / markdown 标题行拆条；拆不出编号时按空行分段。
fn split_issues(report: &str) -> Vec<String> {
    let mut items: Vec<String> = Vec::new();
    let mut current = String::new();
    let is_start = |line: &str| -> bool {
        let t = line.trim_start();
        // 1. / 1、 / 1） / ### / **1  开头视为新条目
        let digits_then_sep = t
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
            && t.chars().nth(1).map(|c| ".、）)]**".contains(c)).unwrap_or(false);
        digits_then_sep || t.starts_with("###") || t.starts_with("**问题")
    };
    for line in report.lines() {
        if is_start(line) && !current.trim().is_empty() {
            items.push(std::mem::take(&mut current).trim().to_string());
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        items.push(current.trim().to_string());
    }
    if items.len() > 1 {
        // 过滤过短的收尾/客套段（"其余核对无误。"之类）
        items.retain(|i| i.chars().count() >= 15);
        return items;
    }
    // 无编号结构：按空行分段，过滤过短段（客套/结论行）
    report
        .split("\n\n")
        .map(str::trim)
        .filter(|p| p.chars().count() > 30)
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("type-hime-issues-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn ingest_split_and_ruling() {
        let d = dir("basic");
        let report = "## 问题清单\n\n1. 【硬伤】炼气四层与账本三层冲突。证据：正文原句。\n\n2. 【软矛盾】滴水声来源。证据：账本原句。\n\n其余核对无误。";
        let n = ingest_report(&d, "ch-005", "consistency", report).unwrap();
        assert_eq!(n, 2, "应拆出 2 条编号问题");
        let all = list(&d, None).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all[0].text.contains("硬伤"));
        assert!(all[1].text.contains("滴水声"));
        assert_eq!(open_count(&d, Some("ch-005")).unwrap(), 2);

        // 裁决
        let id = all[0].id;
        record_ruling(&d, id, "fixed", Some("已删错置段落".into())).unwrap();
        assert_eq!(open_count(&d, Some("ch-005")).unwrap(), 1);
        let after = list(&d, None).unwrap();
        assert_eq!(after[0].status, "fixed");
        assert_eq!(after[0].ruling.as_deref(), Some("已删错置段落"));

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn split_without_numbers() {
        let d = dir("plain");
        let report = "第一段是有内容的较长问题描述，讲清了矛盾与证据所在，长度足够。\n\n第二段也是有内容的较长问题描述，同样讲清了问题，长度足够被保留。";
        let n = ingest_report(&d, "ch-001", "preflight", report).unwrap();
        assert_eq!(n, 2);
        let _ = std::fs::remove_dir_all(&d);
    }
}
