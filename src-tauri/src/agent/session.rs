//! 会话管理：会话历史持久化于作品目录 .sessions/（gitignore，D4）。
//! 历史以 rig Message 原生 serde 存 JSON；提供前端展示视图。

use std::path::{Path, PathBuf};

use rig::completion::Message;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub messages: Vec<Message>,
}

/// 前端展示用的扁平消息
#[derive(Debug, Serialize, Clone)]
pub struct MessageView {
    pub role: String,
    pub text: String,
}

fn sessions_dir(work_dir: &Path) -> PathBuf {
    work_dir.join(".sessions")
}

pub fn new_session_id() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let pid = std::process::id();
    format!("s{secs:x}-{pid:x}")
}

pub fn save_session(work_dir: &Path, session: &Session) -> Result<(), String> {
    let dir = sessions_dir(work_dir);
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建会话目录失败：{e}"))?;
    let path = dir.join(format!("{}.json", session.id));
    let raw = serde_json::to_string(session).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("保存会话失败：{e}"))
}

pub fn load_session(work_dir: &Path, id: &str) -> Result<Session, String> {
    let path = sessions_dir(work_dir).join(format!("{id}.json"));
    if !path.exists() {
        return Err(format!("会话 {id} 不存在"));
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取会话失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析会话失败：{e}"))
}

/// 会话列表（按创建时间倒序），附首条用户消息预览。
pub fn list_sessions(work_dir: &Path) -> Result<Vec<(String, String, String)>, String> {
    let dir = sessions_dir(work_dir);
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = std::fs::read_to_string(&path) {
                if let Ok(s) = serde_json::from_str::<Session>(&raw) {
                    let preview = s
                        .messages
                        .iter()
                        .find_map(|m| match m {
                            Message::User { content } => content.iter().find_map(|c| match c {
                                rig::completion::message::UserContent::Text(t) => {
                                    Some(t.text.clone())
                                }
                                _ => None,
                            }),
                            _ => None,
                        })
                        .unwrap_or_else(|| "（空会话）".to_string());
                    let preview: String = preview.chars().take(40).collect();
                    out.push((s.id.clone(), s.created_at.clone(), preview));
                }
            }
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(out)
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
