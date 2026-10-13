pub mod agent;
mod git;
pub mod settings;
pub mod works;

use std::path::PathBuf;

use tauri::ipc::Channel;

use agent::events::AgentEvent;
use agent::kernel;
use agent::session::{self, Session};
use agent::spec::{chat_spec, AgentSpec};
use settings::{ProviderProfile, SettingsView};
use works::{TreeNode, WorkSummary};

// ---------------------------------------------------------------------------
// 设置：供应商管理 + 可达性测试 + 模型列表

#[tauri::command]
fn get_settings() -> Result<SettingsView, String> {
    Ok(settings::to_view(&settings::load()?))
}

#[tauri::command]
fn list_provider_presets() -> Vec<settings::ProviderPreset> {
    settings::provider_presets()
}

/// 新增/编辑供应商；api_key 为空表示沿用已存 key。
#[tauri::command]
fn upsert_provider(
    id: String,
    name: String,
    base_url: String,
    api_key: String,
    default_model: String,
    activate: bool,
) -> Result<(), String> {
    let s = settings::load()?;
    let profile = ProviderProfile {
        id,
        name,
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        api_key,
        default_model: default_model.trim().to_string(),
    };
    let s = settings::upsert_provider(s, profile, activate)?;
    settings::save(&s)
}

#[tauri::command]
fn remove_provider(id: String) -> Result<(), String> {
    let s = settings::remove_provider(settings::load()?, &id)?;
    settings::save(&s)
}

#[tauri::command]
fn set_active_provider(id: String) -> Result<(), String> {
    let s = settings::set_active(settings::load()?, &id)?;
    settings::save(&s)
}

/// 可达性测试；空字段沿用指定供应商的已存配置。
#[tauri::command]
async fn test_provider(
    base_url: String,
    api_key: String,
    model: String,
) -> Result<String, String> {
    let s = settings::load()?;
    // 空字段回落 active 供应商
    let active = s.active();
    let base_url = if base_url.is_empty() {
        active.map(|p| p.base_url.clone()).ok_or("缺少 base_url")?
    } else {
        base_url.trim().trim_end_matches('/').to_string()
    };
    let key = if api_key.is_empty() {
        active.map(|p| p.api_key.clone()).ok_or("缺少 api_key")?
    } else {
        api_key
    };
    let model = if model.is_empty() {
        active
            .map(|p| p.default_model.clone())
            .ok_or("缺少 model")?
    } else {
        model.trim().to_string()
    };
    kernel::test_connection(&base_url, &key, &model).await
}

/// 自动获取模型列表；空字段沿用 active 供应商。
#[tauri::command]
async fn list_models(base_url: String, api_key: String) -> Result<Vec<String>, String> {
    let s = settings::load()?;
    let active = s.active();
    let base_url = if base_url.is_empty() {
        active.map(|p| p.base_url.clone()).ok_or("缺少 base_url")?
    } else {
        base_url.trim().trim_end_matches('/').to_string()
    };
    let key = if api_key.is_empty() {
        active.map(|p| p.api_key.clone()).unwrap_or_default()
    } else {
        api_key
    };
    kernel::list_models(&base_url, &key).await
}

/// 设置某 agent 的模型覆盖；model 传 None 删除覆盖。
#[tauri::command]
fn set_agent_model_override(agent_id: String, model: Option<String>) -> Result<(), String> {
    let mut s = settings::load()?;
    match model {
        Some(m) if !m.trim().is_empty() => {
            s.agent_overrides.insert(agent_id, m.trim().to_string());
        }
        _ => {
            s.agent_overrides.remove(&agent_id);
        }
    }
    settings::save(&s)
}

// ---------------------------------------------------------------------------
// 会话（通用对话 agent，中栏会话流）

#[tauri::command]
fn list_sessions(work_path: String) -> Result<Vec<(String, String, String)>, String> {
    session::list_sessions(&PathBuf::from(work_path))
}

#[tauri::command]
fn load_session_view(work_path: String, session_id: String) -> Result<Vec<session::MessageView>, String> {
    let s = session::load_session(&PathBuf::from(work_path), &session_id)?;
    Ok(s.messages.iter().map(session::message_view).collect())
}

/// 流式对话：事件经 Channel 推前端；结束后历史落盘。
#[tauri::command]
async fn chat_stream(
    work_path: String,
    session_id: String,
    prompt: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;
    let spec: AgentSpec = chat_spec();

    let mut sess = session::load_session(&work_dir, &session_id).unwrap_or(Session {
        id: session_id.clone(),
        created_at: works::now_iso(),
        messages: Vec::new(),
    });

    let history = sess.messages.clone();
    let (_text, new_messages) =
        kernel::run(&settings, Some(&work_dir), &spec, history, &prompt, move |ev| {
            let _ = on_event.send(ev);
        })
        .await?;

    sess.messages.extend(new_messages);
    session::save_session(&work_dir, &sess)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // 作品底座
            get_works_root,
            list_works,
            create_work,
            read_tree,
            read_file,
            write_file,
            create_file,
            delete_file,
            restore_file,
            git_log,
            // 设置
            get_settings,
            list_provider_presets,
            upsert_provider,
            remove_provider,
            set_active_provider,
            test_provider,
            list_models,
            set_agent_model_override,
            // 会话
            list_sessions,
            load_session_view,
            chat_stream,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ---------------------------------------------------------------------------
// 作品底座命令（切片 1）

#[tauri::command]
fn get_works_root() -> Result<String, String> {
    works::load_config().map(|c| c.works_root)
}

#[tauri::command]
fn list_works() -> Result<Vec<WorkSummary>, String> {
    works::list_works()
}

#[tauri::command]
fn create_work(name: String, base: Option<String>) -> Result<WorkSummary, String> {
    works::create_work(&name, base)
}

#[tauri::command]
fn read_tree(work_path: String) -> Result<TreeNode, String> {
    works::read_tree(&PathBuf::from(work_path))
}

#[tauri::command]
fn read_file(work_path: String, rel_path: String) -> Result<String, String> {
    works::read_file(&PathBuf::from(work_path), &rel_path)
}

#[tauri::command]
fn write_file(work_path: String, rel_path: String, content: String) -> Result<(), String> {
    works::write_file(&PathBuf::from(work_path), &rel_path, &content)
}

#[tauri::command]
fn create_file(work_path: String, rel_path: String, content: String) -> Result<(), String> {
    works::create_file(&PathBuf::from(work_path), &rel_path, &content)
}

#[tauri::command]
fn delete_file(work_path: String, rel_path: String) -> Result<(), String> {
    works::delete_file(&PathBuf::from(work_path), &rel_path)
}

#[tauri::command]
fn restore_file(work_path: String, rel_path: String) -> Result<(), String> {
    works::restore_file(&PathBuf::from(work_path), &rel_path)
}

#[tauri::command]
fn git_log(work_path: String, limit: Option<usize>) -> Result<Vec<String>, String> {
    git::log_oneline(&PathBuf::from(work_path), limit.unwrap_or(20))
}
