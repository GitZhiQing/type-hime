//! 连续量预检：把一部分一致性检查从"模型眼力"下沉到"代码必查"（零 token）。
//! 目前覆盖：修为层级（炼气N层/筑基/金丹）与账本角色条目的比对。

use crate::ledger::Ledger;

/// 中文/阿拉伯数字 → 层级序号（炼气一~九层 → 1~9；筑基 → 10；金丹 → 11）。
/// pub：供 regression 例子按账本实况自适应埋错（避免硬编码角色名导致假阴性）。
pub fn realm_level(text: &str) -> Option<u32> {
    for (i, cn) in ["一", "二", "三", "四", "五", "六", "七", "八", "九"].iter().enumerate() {
        let n = i as u32 + 1;
        if text.contains(&format!("炼气{n}层"))
            || text.contains(&format!("炼气{n:02}层"))
            || text.contains(&format!("炼气{cn}层"))
        {
            return Some(n);
        }
    }
    if text.contains("筑基") {
        return Some(10);
    }
    if text.contains("金丹") {
        return Some(11);
    }
    None
}

/// 层级序号 → 展示名（pub：regression 例子构造矛盾句用）。
pub fn realm_label(level: u32) -> String {
    match level {
        10 => "筑基".into(),
        11 => "金丹".into(),
        n => {
            let cn = ["一", "二", "三", "四", "五", "六", "七", "八", "九"]
                .get((n - 1) as usize)
                .copied()
                .unwrap_or("?");
            format!("炼气{cn}层")
        }
    }
}

/// 按句切分（中文句读）。
fn sentences(text: &str) -> Vec<&str> {
    text.split(|c: char| c == '。' || c == '！' || c == '？' || c == '\n')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// 预检：正文句中同时出现角色名与修为层级时，与账本该角色层级比对。
/// 返回问题清单（空=通过）。
pub fn preflight_cultivation(chapter_text: &str, ledger: &Ledger) -> Vec<String> {
    let mut issues = Vec::new();
    for entry in &ledger.characters {
        let Some(expected) = realm_level(&entry.body) else {
            continue;
        };
        for sent in sentences(chapter_text) {
            if !sent.contains(entry.title.as_str()) {
                continue;
            }
            if let Some(found) = realm_level(sent) {
                if found != expected && realm_level(&entry.body).is_some() {
                    issues.push(format!(
                        "{}：正文写 {}，账本记录为 {}。原句：{}",
                        entry.title,
                        realm_label(found),
                        realm_label(expected),
                        sent.chars().take(60).collect::<String>()
                    ));
                }
            }
        }
    }
    issues.dedup();
    issues
}

/// 对整卷所有正文做预检（卷级批审用）。
pub fn preflight_volume<'a>(
    chapters: impl Iterator<Item = (String, &'a str)>,
    ledger: &Ledger,
) -> Vec<String> {
    let mut all = Vec::new();
    for (ch, text) in chapters {
        for i in preflight_cultivation(text, ledger) {
            all.push(format!("[{ch}] {i}"));
        }
    }
    all
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::parse;

    #[test]
    fn realm_parsing() {
        assert_eq!(realm_level("炼气三层"), Some(3));
        assert_eq!(realm_level("炼气3层"), Some(3));
        assert_eq!(realm_level("已是筑基"), Some(10));
        assert_eq!(realm_level("没有修为"), None);
    }

    #[test]
    fn preflight_finds_mismatch() {
        let ledger = parse(
            "## 角色状态\n\n### 林晚\n修为：炼气三层，靠黑剑反哺。\n\n### 谢无咎\n执法堂副手，炼气七层。\n",
        );
        // 一致：不报
        let ok = "林晚握紧拳头。谢无咎淡淡说：炼气七层又如何。";
        assert!(preflight_cultivation(ok, &ledger).is_empty());
        // 矛盾：报
        let bad = "林晚盘膝坐下，炼气四层的修为稳固了几分。";
        let issues = preflight_cultivation(bad, &ledger);
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("炼气三层"), "{:?}", issues);
        assert!(issues[0].contains("炼气四层"));

        // 角色不同句不误报：句中无角色名时跳过
        let other = "远处有人议论：那可是炼气九层的高手。";
        assert!(preflight_cultivation(other, &ledger).is_empty());
    }
}
