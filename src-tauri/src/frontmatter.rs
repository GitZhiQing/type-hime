//! 极简 YAML frontmatter 工具：`---` 围起来的头部 + md 正文。

/// 返回 frontmatter 块内容（不含 --- 行）；无 frontmatter 时 None。
pub fn split(raw: &str) -> Option<String> {
    split_raw(raw).map(|(fm, _)| fm)
}

/// 返回 (frontmatter 内容, 正文)。
pub fn split_raw(raw: &str) -> Option<(String, String)> {
    let rest = raw.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    let fm = rest[..end].to_string();
    let body = rest[end + 4..]
        .strip_prefix('\n')
        .unwrap_or(&rest[end + 4..])
        .to_string();
    Some((fm, body))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_basics() {
        let (fm, body) = split_raw("---\nname: a\nvisibility: hidden\n---\n\n正文").unwrap();
        assert_eq!(fm, "name: a\nvisibility: hidden");
        assert_eq!(body, "\n正文");
        assert!(split_raw("没有头").is_none());
    }
}
