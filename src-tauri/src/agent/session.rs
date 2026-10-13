//! 会话管理：会话历史持久化（D4）。
//! 作品会话存作品目录 .sessions/（gitignore）；全局对话会话存 app config 目录
//! global-sessions/（无作品上下文，REDESIGN §5.1）。历史以 rig Message 原生 serde
//! 存 JSON；提供前端展示视图。旧档兼容：title 缺失时回退首条用户消息截断。

use std::path::PathBuf;

use rig::completion::Message;
use serde::{Deserialize, Serialize};

/// 会话存档格式版本（防御：rig Message 序列化演进的迁移锚点；旧档无此字段按当前版本读）。
pub const SESSION_FORMAT_VERSION: u32 = 1;

fn default_format_version() -> u32 {
    SESSION_FORMAT_VERSION
}

impl Session {
    /// 新会话骨架（空消息、默认「变更前确认」档）。
    pub fn new(id: String) -> Self {
        Self {
            id,
            created_at: crate::works::now_iso(),
            title: None,
            format_version: SESSION_FORMAT_VERSION,
            permission_mode: Default::default(),
            messages: Vec::new(),
            usage: None,
            last_usage: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    /// 会话标题（自动生成或用户重命名；旧档无此字段时按 None 读取）
    #[serde(default)]
    pub title: Option<String>,
    /// 存档格式版本（R4 inner facade：未来 rig 破坏性升级时按版本迁移）
    #[serde(default = "default_format_version")]
    pub format_version: u32,
    /// 会话权限档位（R4 · D30：会话级决定；旧档无此字段按「变更前确认」）
    #[serde(default)]
    pub permission_mode: crate::permissions::PermissionMode,
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
    /// 权限档位（R4：前端输入框左下角下拉的当前值）
    pub permission_mode: crate::permissions::PermissionMode,
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
    /// 本消息发起的工具调用（已配对结果）
    pub tool_calls: Vec<ToolCallView>,
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
                        permission_mode: s.permission_mode,
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

/// 设置会话权限档位（R4：输入框左下角下拉切换时调用）。
pub fn set_permission_mode(
    store: &SessionStore,
    id: &str,
    mode: crate::permissions::PermissionMode,
) -> Result<(), String> {
    let mut sess = load_session(store, id)?;
    sess.permission_mode = mode;
    save_session(store, &sess)
}

/// 一次工具调用（结构化持久化：前端可展开查看参数与结果，与流式事件同构）。
#[derive(Debug, Serialize, Clone)]
pub struct ToolCallView {
    pub name: String,
    /// JSON 字符串（与 AgentEvent.tool_call.args 同构）
    pub args: String,
    /// 已配对的工具结果（文本或 JSON 序列化）；None = 无结果记录
    pub result: Option<String>,
    pub is_error: bool,
}

/// 会话消息序列 → 前端视图（含工具调用配对）。
/// 工具结果在 rig 中以 User 角色承载（UserContent::ToolResult），按 call_id
/// 配对到发起它的 Assistant 工具调用上；纯工具结果消息不再输出为独立气泡。
pub fn message_views(messages: &[Message]) -> Vec<MessageView> {
    use rig::completion::message::UserContent;
    use rig::completion::AssistantContent;

    // 第一遍：收集全部工具结果（call_id → 文本 + 是否错误）
    let mut results: std::collections::HashMap<String, (String, bool)> =
        std::collections::HashMap::new();
    for m in messages {
        if let Message::User { content } = m {
            for c in content.iter() {
                if let UserContent::ToolResult(tr) = c {
                    let text = tr
                        .content
                        .iter()
                        .filter_map(|item| match item {
                            rig::completion::message::ToolResultContent::Text(t) => {
                                Some(t.text.clone())
                            }
                            rig::completion::message::ToolResultContent::Json { value } => {
                                Some(value.to_string())
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    results.insert(tr.call.to_string(), (text, tr.is_error));
                }
            }
        }
    }

    // 第二遍：生成视图
    let mut out = Vec::new();
    for m in messages {
        match m {
            Message::System { content } => out.push(MessageView {
                role: "system".into(),
                text: content.clone(),
                tool_calls: Vec::new(),
            }),
            Message::User { content } => {
                let text = content
                    .iter()
                    .filter_map(|c| match c {
                        UserContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                // 纯工具结果消息：配对后不再作为独立气泡输出
                if text.trim().is_empty() {
                    continue;
                }
                out.push(MessageView {
                    role: "user".into(),
                    text,
                    tool_calls: Vec::new(),
                });
            }
            Message::Assistant(a) => {
                let text = a
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        AssistantContent::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                let tool_calls: Vec<ToolCallView> = a
                    .content
                    .iter()
                    .filter_map(|c| match c {
                        AssistantContent::ToolCall(tc) => Some(ToolCallView {
                            name: tc.function.name.as_str().to_string(),
                            args: serde_json::Value::Object(tc.function.arguments.clone())
                                .to_string(),
                            result: results
                                .get(&tc.id.to_string())
                                .map(|(t, _)| t.clone()),
                            is_error: results
                                .get(&tc.id.to_string())
                                .map(|(_, e)| *e)
                                .unwrap_or(false),
                        }),
                        _ => None,
                    })
                    .collect();
                // 空助手消息（既无文本又无工具）不输出
                if text.trim().is_empty() && tool_calls.is_empty() {
                    continue;
                }
                out.push(MessageView {
                    role: "assistant".into(),
                    text,
                    tool_calls,
                });
            }
        }
    }
    out
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
            created_at: format!("2026-10-14T00:0{id}:00"),
            ..Session::new(id.to_string())
        }
    }

    #[test]
    fn message_views_pairs_tool_calls_and_skips_tool_messages() {
        use rig::completion::message::{
            AssistantContent, AssistantMessage, CallId, Text, ToolCall, ToolFunction,
            ToolName, ToolResult, ToolResultContent, UserContent,
        };

        let call = ToolCall::new(
            CallId::from_wire("call_1"),
            ToolFunction::new(
                ToolName::new("read_file").unwrap(),
                serde_json::json!({"path": "outline/总纲.md"}),
            ),
        );
        let assistant = Message::Assistant(AssistantMessage::new(vec![
            AssistantContent::ToolCall(call),
            AssistantContent::Text(Text::new("我先读取总纲。")),
        ]));
        let tool_msg = Message::User {
            content: vec![UserContent::ToolResult(ToolResult {
                call: CallId::from_wire("call_1"),
                name: ToolName::new("read_file").unwrap(),
                content: vec![ToolResultContent::Text(Text::new("总纲内容"))],
                is_error: false,
            })],
        };
        let user = Message::User {
            content: vec![UserContent::Text(Text::new("你好"))],
        };

        let views = message_views(&[assistant, tool_msg, user]);
        assert_eq!(views.len(), 2, "纯工具结果消息不输出为独立气泡");
        assert_eq!(views[0].role, "assistant");
        assert_eq!(views[0].text, "我先读取总纲。");
        assert_eq!(views[0].tool_calls.len(), 1);
        assert_eq!(views[0].tool_calls[0].name, "read_file");
        assert!(views[0].tool_calls[0].args.contains("总纲.md"));
        assert_eq!(
            views[0].tool_calls[0].result.as_deref(),
            Some("总纲内容"),
            "结果按 call_id 配对"
        );
        assert!(!views[0].tool_calls[0].is_error);
        assert_eq!(views[1].role, "user");
        assert!(views[1].tool_calls.is_empty());
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
