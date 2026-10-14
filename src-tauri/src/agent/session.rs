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
    /// 新会话骨架（空消息、默认「变更前确认」档、无长时目标）。
    pub fn new(id: String) -> Self {
        Self {
            id,
            created_at: crate::works::now_iso(),
            title: None,
            format_version: SESSION_FORMAT_VERSION,
            permission_mode: Default::default(),
            goal: None,
            messages: Vec::new(),
            usage: None,
            last_usage: None,
            subtasks: Vec::new(),
        }
    }
}

/// 子任务行（D51）：SubTask* 事件汇聚落档（ZCode SubagentRow 同语义）——
/// 重载后历史仍以任务卡呈现、可打开子会话档案。
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct SubTaskRecord {
    /// 任务 id（st-N；与子会话档案名 agent-{名}-{id} 同源）
    pub id: String,
    /// 子 Agent 名（None=普通子任务）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    pub label: String,
    /// 子会话档案 id（D50 贯通；前端据此回看）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// running | done | error
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
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
    /// 长时目标（R4.5 · D31；旧档无此字段按无目标读取；.sessions/ 受提议通道硬闸保护）
    #[serde(default)]
    pub goal: Option<crate::agent::goal::SessionGoal>,
    pub messages: Vec<Message>,
    /// 会话全部轮次累计用量（成本参考）
    #[serde(default)]
    pub usage: Option<crate::agent::kernel::UsageReport>,
    /// 最近一轮用量（R3：窗口占用口径 = 最近一轮 input_tokens ≈ 当前对话历史大小）
    #[serde(default)]
    pub last_usage: Option<crate::agent::kernel::UsageReport>,
    /// 子任务行（D51：spawn_agent 委派的历史记录；旧档无此字段按空表读取）
    #[serde(default)]
    pub subtasks: Vec<SubTaskRecord>,
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

/// 子任务行并入会话档案（D51）：同 id 覆盖（状态/用时更新）、新 id 追加，保持出现顺序。
/// 在消息落档成功后调用（避免孤儿行）；失败静默（记录是增强信息，不阻断会话保存）。
pub fn merge_subtasks(
    store: &SessionStore,
    session_id: &str,
    rows: &[SubTaskRecord],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let mut sess = load_session(store, session_id)?;
    for row in rows {
        match sess.subtasks.iter_mut().find(|x| x.id == row.id) {
            Some(existing) => *existing = row.clone(),
            None => sess.subtasks.push(row.clone()),
        }
    }
    save_session(store, &sess)
}

/// 内部会话前缀（子 Agent 与生成/审校流程的独立档案）：不进入用户会话列表，
/// 档案仍保留在磁盘（参与成本汇总等扫描）；R10 起过滤（D35 观察项落地）。
const INTERNAL_SESSION_PREFIXES: &[&str] = &[
    "agent-",     // 子 Agent 会话（spawn_agent）
    "gen-",       // 生成流会话（章纲/正文/回写/整理）
    "review-",    // 审校流会话（一致性/去AI味/复检/审卷）
    "ledger-",    // 账本类流程会话
];

/// 会话列表（按创建时间倒序），附标题与首条用户消息预览；内部会话（agent-/gen-/review-/ledger-）不列出。
pub fn list_sessions(store: &SessionStore) -> Result<Vec<SessionMeta>, String> {
    let dir = store.dir()?;
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if INTERNAL_SESSION_PREFIXES.iter().any(|p| stem.starts_with(p)) {
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

/// 读会话；存档不存在则新建骨架（会话 id 由前端生成，首条消息可能先于存档落盘）。
/// 存档存在但解析失败时如实传播（不吞坏档）。
pub fn load_or_new_session(store: &SessionStore, id: &str) -> Result<Session, String> {
    let path = store.dir()?.join(format!("{id}.json"));
    if !path.exists() {
        return Ok(Session::new(id.to_string()));
    }
    load_session(store, id)
}

/// 长时目标控制读改写（R4.5：load → apply_control → save）。
/// 返回 (更新后目标, 面向作者的提示文案)；非法迁移（如重复暂停）返回 Err 文案。
pub fn update_goal(
    store: &SessionStore,
    id: &str,
    action: crate::agent::goal::GoalControlAction,
) -> Result<(Option<crate::agent::goal::SessionGoal>, String), String> {
    let mut sess = load_session(store, id)?;
    let msg = crate::agent::goal::apply_control(&mut sess.goal, action)?;
    let goal = sess.goal.clone();
    save_session(store, &sess)?;
    Ok((goal, msg))
}

/// 目标字段读改写（R4.5-③ 迭代循环）：load fresh → f(&mut goal) → save。
/// 与直调控制命令同属读改写，轮中并发暂停/清除不会被循环落库覆盖；
/// f 返回 Err 时放弃保存。
pub fn mutate_goal<T>(
    store: &SessionStore,
    id: &str,
    f: impl FnOnce(&mut Option<crate::agent::goal::SessionGoal>) -> Result<T, String>,
) -> Result<T, String> {
    let mut sess = load_session(store, id)?;
    let out = f(&mut sess.goal)?;
    save_session(store, &sess)?;
    Ok(out)
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
    use crate::agent::goal;

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
    fn goal_field_compat_and_update_roundtrip() {
        let store = temp_store("goal");
        // 旧档（无 goal 字段）可解析，goal 为 None
        let raw = r#"{"id":"old-g","created_at":"2026-10-13T10:00:00","messages":[]}"#;
        std::fs::write(store.dir().unwrap().join("old-g.json"), raw).unwrap();
        assert!(load_session(&store, "old-g").unwrap().goal.is_none());
        // 无目标时控制动作报错
        assert!(update_goal(&store, "old-g", goal::GoalControlAction::Pause).is_err());

        // 设置目标 → 暂停 → 恢复 → 清除 全程读改写
        let mut sess = load_or_new_session(&store, "old-g").unwrap();
        sess.goal = Some(goal::SessionGoal::new("审校第一卷"));
        save_session(&store, &sess).unwrap();
        let (g, msg) = update_goal(&store, "old-g", goal::GoalControlAction::Pause).unwrap();
        assert_eq!(g.unwrap().status, goal::GoalStatus::Paused);
        assert!(msg.contains("暂停"));
        let (g, _) = update_goal(&store, "old-g", goal::GoalControlAction::Resume).unwrap();
        assert_eq!(g.unwrap().status, goal::GoalStatus::Active);
        let (g, _) = update_goal(&store, "old-g", goal::GoalControlAction::Clear).unwrap();
        assert!(g.is_none());
        assert!(load_session(&store, "old-g").unwrap().goal.is_none());

        // load_or_new：不存在的会话返回新建骨架
        let fresh = load_or_new_session(&store, "no-such").unwrap();
        assert!(fresh.messages.is_empty() && fresh.goal.is_none());

        let _ = std::fs::remove_dir_all(store.dir().unwrap());
    }

    /// D51：子任务行并入（同 id 覆盖、新 id 追加、旧档兼容）。
    #[test]
    fn merge_subtasks_upsert_roundtrip() {
        let store = temp_store("subtasks");
        // 旧档（无 subtasks 字段）可解析为空表
        let raw = r#"{"id":"old-s","created_at":"2026-10-13T10:00:00","messages":[]}"#;
        std::fs::write(store.dir().unwrap().join("old-s.json"), raw).unwrap();
        assert!(load_session(&store, "old-s").unwrap().subtasks.is_empty());

        let running = SubTaskRecord {
            id: "st-1".into(),
            agent: Some("general".into()),
            label: "写第 2 章正文".into(),
            session: Some("agent-general-st-1".into()),
            status: "running".into(),
            summary: None,
            tokens: None,
            duration_ms: None,
        };
        merge_subtasks(&store, "old-s", std::slice::from_ref(&running)).unwrap();
        // 完成态覆盖同 id 行（不重复追加）
        let done = SubTaskRecord {
            status: "done".into(),
            summary: Some("完成汇报".into()),
            tokens: Some(20480),
            duration_ms: Some(33000),
            ..running.clone()
        };
        merge_subtasks(&store, "old-s", &[done.clone()]).unwrap();
        let s = load_session(&store, "old-s").unwrap();
        assert_eq!(s.subtasks.len(), 1);
        assert_eq!(s.subtasks[0].status, "done");
        assert_eq!(s.subtasks[0].tokens, Some(20480));
        assert_eq!(s.subtasks[0].session.as_deref(), Some("agent-general-st-1"));
        // 空表调用是 no-op（会话不存在也不报错前提：存在时无副作用）
        merge_subtasks(&store, "old-s", &[]).unwrap();

        let _ = std::fs::remove_dir_all(store.dir().unwrap());
    }

    #[test]
    fn list_filters_internal_sessions() {
        // R10：子 Agent 与流程会话不进用户列表（档案仍在磁盘供成本汇总扫描）
        let store = temp_store("filter-internal");
        save_session(&store, &sample("s-normal")).unwrap();
        save_session(&store, &sample("agent-general-st-1")).unwrap();
        save_session(&store, &sample("gen-chapter-01-ch-003")).unwrap();
        save_session(&store, &sample("review-consistency-01-ch-003")).unwrap();
        save_session(&store, &sample("ledger-tidy-task")).unwrap();
        let list = list_sessions(&store).unwrap();
        assert_eq!(list.len(), 1, "仅用户会话列出：{list:?}");
        assert_eq!(list[0].id, "s-normal");
        // 档案仍在磁盘（成本汇总口径不因过滤丢失）
        assert!(load_session(&store, "agent-general-st-1").is_ok());
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
