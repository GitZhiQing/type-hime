//! 会话管理：会话历史持久化（D4）。
//! 作品会话存作品目录 .sessions/（gitignore）；全局对话会话存 app config 目录
//! global-sessions/（无作品上下文，REDESIGN §5.1）。历史以 rig Message 原生 serde
//! 存 JSON；提供前端展示视图。旧档兼容：title 缺失时回退首条用户消息截断。

use std::path::PathBuf;

use rig::completion::Message;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    /// 会话标题（自动生成或用户重命名；旧档无此字段时按 None 读取）
    #[serde(default)]
    pub title: Option<String>,
    pub messages: Vec<Message>,
    /// 会话全部轮次累计用量（成本参考）
    #[serde(default)]
    pub usage: Option<crate::agent::kernel::UsageReport>,
    /// 最近一轮用量（R3：窗口占用口径 = 最近一轮 input_tokens ≈ 当前对话历史大小）
    #[serde(default)]
    pub last_usage: Option<crate::agent::kernel::UsageReport>,
}

/// 会话列表项（前端展示与搜索过滤；title 已做回退填充）。
#[derive(Debug, Serialize, Clone)]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    /// 首条用户消息预览（搜索过滤的兜底字段）
    pub preview: String,
    pub created_at: String,
}

/// 会话存储位置：作品内 .sessions/ 或全局 config 目录。
#[derive(Debug, Clone)]
pub enum SessionStore {
    Work(PathBuf),
    Global,
    /// 指定目录（测试用）
    At(PathBuf),
}

impl SessionStore {
    pub fn dir(&self) -> Result<PathBuf, String> {
        let dir = match self {
            SessionStore::Work(d) => d.join(".sessions"),
            SessionStore::At(d) => d.clone(),
            SessionStore::Global => {
                let base = dirs::config_dir()
                    .ok_or("无法定位用户配置目录")?
                    .join("type-hime")
                    .join("global-sessions");
                std::fs::create_dir_all(&base)
                    .map_err(|e| format!("创建全局会话目录失败：{e}"))?;
                base
            }
        };
        Ok(dir)
    }
}

/// 前端展示用的扁平消息
#[derive(Debug, Serialize, Clone)]
pub struct MessageView {
    pub role: String,
    pub text: String,
}

pub fn new_session_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let pid = std::process::id();
    format!("s{secs:x}-{pid:x}")
}

/// 首条用户消息文本（无则空串）。
pub fn first_user_text(s: &Session) -> String {
    s.messages
        .iter()
        .find_map(|m| match m {
            Message::User { content } => content.iter().find_map(|c| match c {
                rig::completion::message::UserContent::Text(t) => Some(t.text.clone()),
                _ => None,
            }),
            _ => None,
        })
        .unwrap_or_default()
}

fn preview_of(s: &Session) -> String {
    let text = first_user_text(s);
    let preview: String = if text.is_empty() {
        "（空会话）".to_string()
    } else {
        text.chars().take(40).collect()
    };
    preview
}

pub fn save_session(store: &SessionStore, session: &Session) -> Result<(), String> {
    let dir = store.dir()?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建会话目录失败：{e}"))?;
    let path = dir.join(format!("{}.json", session.id));
    let raw = serde_json::to_string(session).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("保存会话失败：{e}"))
}

pub fn load_session(store: &SessionStore, id: &str) -> Result<Session, String> {
    let path = store.dir()?.join(format!("{id}.json"));
    if !path.exists() {
        return Err(format!("会话 {id} 不存在"));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取会话失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析会话失败：{e}"))
}

/// 会话列表（按创建时间倒序），附标题与首条用户消息预览。
pub fn list_sessions(store: &SessionStore) -> Result<Vec<SessionMeta>, String> {
    let dir = store.dir()?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(s) = serde_json::from_str::<Session>(&raw) {
                    let preview = preview_of(&s);
                    let title = s
                        .title
                        .clone()
                        .unwrap_or_else(|| preview.clone());
                    out.push(SessionMeta {
                        id: s.id.clone(),
                        title,
                        preview,
                        created_at: s.created_at.clone(),
                    });
                }
            }
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// 重命名会话（标题以用户输入为准，不再被自动标题覆盖）。
pub fn rename_session(store: &SessionStore, id: &str, title: &str) -> Result<(), String> {
    let title: String = title.trim().chars().take(60).collect();
    if title.is_empty() {
        return Err("标题不能为空".to_string());
    }
    let mut sess = load_session(store, id)?;
    sess.title = Some(title);
    save_session(store, &sess)
}

/// 删除会话存档。
pub fn delete_session(store: &SessionStore, id: &str) -> Result<(), String> {
    let path = store.dir()?.join(format!("{id}.json"));
    if !path.exists() {
        return Err(format!("会话 {id} 不存在"));
    }
    std::fs::remove_file(&path).map_err(|e| format!("删除会话失败：{e}"))
}

/// rig Message → 前端视图（提取文本部分）。
pub fn message_view(m: &Message) -> MessageView {
    use rig::completion::message::UserContent;
    use rig::completion::AssistantContent;
    match m {
        Message::System { content } => MessageView {
            role: "system".into(),
            text: content.clone(),
        },
        Message::User { content } => {
            let text = content
                .iter()
                .filter_map(|c| match c {
                    UserContent::Text(t) => Some(t.text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            MessageView { role: "user".into(), text }
        }
        Message::Assistant(a) => {
            let text = a
                .content
                .iter()
                .filter_map(|c| match c {
                    AssistantContent::Text(t) => Some(t.text.clone()),
                    AssistantContent::ToolCall(tc) => {
                        Some(format!("[调用工具 {}]", tc.function.name))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            MessageView { role: "assistant".into(), text }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store(tag: &str) -> SessionStore {
        // 测试目录按测试名区分后缀，避免并行竞态
        let dir = std::env::temp_dir().join(format!(
            "type-hime-session-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        SessionStore::At(dir)
    }

    fn sample(id: &str) -> Session {
        Session {
            id: id.to_string(),
            created_at: format!("2026-10-14T00:0{id}:00"),
            title: None,
            messages: Vec::new(),
            usage: None,
            last_usage: None,
        }
    }

    #[test]
    fn title_compat_and_rename_delete_roundtrip() {
        let store = temp_store("roundtrip");
        // 旧档（无 title 字段）可解析，list 回退 preview
        let raw = r#"{"id":"old-1","created_at":"2026-10-13T10:00:00","messages":[]}"#;
        std::fs::write(
            store.dir().unwrap().join("old-1.json"),
            raw,
        )
        .unwrap();
        let list = list_sessions(&store).unwrap();
        assert_eq!(list[0].title, "（空会话）");

        // rename 后 title 生效
        rename_session(&store, "old-1", "  灵感：黑剑来历  ").unwrap();
        let s = load_session(&store, "old-1").unwrap();
        assert_eq!(s.title.as_deref(), Some("灵感：黑剑来历"));
        let list = list_sessions(&store).unwrap();
        assert_eq!(list[0].title, "灵感：黑剑来历");

        // 空标题被拒
        assert!(rename_session(&store, "old-1", "   ").is_err());

        // delete
        delete_session(&store, "old-1").unwrap();
        assert!(load_session(&store, "old-1").is_err());
        assert!(delete_session(&store, "old-1").is_err());

        let _ = std::fs::remove_dir_all(store.dir().unwrap());
    }

    #[test]
    fn list_sorted_desc_and_preview_fallback() {
        let store = temp_store("sorted");
        save_session(&store, &sample("a")).unwrap();
        let mut b = sample("b");
        b.created_at = "2026-10-14T12:00:00".to_string();
        save_session(&store, &b).unwrap();
        let list = list_sessions(&store).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, "b"); // 新的在前
        assert_eq!(list[1].title, "（空会话）");

        let _ = std::fs::remove_dir_all(store.dir().unwrap());
    }
}
