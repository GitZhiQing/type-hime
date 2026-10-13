pub mod agent;
pub mod assembly;
mod frontmatter;
pub mod git;
pub mod ledger;
pub mod lore;
mod pipeline;
pub mod review;
pub mod settings;
pub mod works;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use agent::events::AgentEvent;
use agent::kernel;
use agent::session::{self, Session};
use agent::spec::{chat_spec, AgentSpec};
use agent::tools::ToolCtx;
use lore::{SettingMeta, Visibility};
use pipeline::{PipelineState, PipelineView, Stage};
use review::{Proposal, SharedQueue};
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
// 流水线状态机

#[tauri::command]
fn get_pipeline(work_path: String) -> Result<PipelineView, String> {
    let dir = PathBuf::from(work_path);
    Ok(pipeline::load_pipeline(&dir)?.to_view(&dir))
}

#[tauri::command]
fn pipeline_enter(work_path: String, stage: Stage) -> Result<PipelineView, String> {
    let dir = PathBuf::from(work_path);
    let mut p = pipeline::load_pipeline(&dir)?;
    p.enter(stage, &dir)?;
    pipeline::save_pipeline(&dir, &p)?;
    Ok(p.to_view(&dir))
}

#[tauri::command]
fn pipeline_confirm(work_path: String, stage: Stage) -> Result<PipelineView, String> {
    let dir = PathBuf::from(work_path);
    let mut p = pipeline::load_pipeline(&dir)?;
    p.confirm(stage, &dir)?;
    pipeline::save_pipeline(&dir, &p)?;
    Ok(p.to_view(&dir))
}

/// 显式跳过（不校验产物，记录确认）。
#[tauri::command]
fn pipeline_skip(work_path: String, stage: Stage) -> Result<PipelineView, String> {
    let dir = PathBuf::from(work_path);
    let mut p = pipeline::load_pipeline(&dir)?;
    p.skip_confirmed(stage);
    pipeline::save_pipeline(&dir, &p)?;
    Ok(p.to_view(&dir))
}

#[tauri::command]
fn pipeline_redo(work_path: String, stage: Stage) -> Result<PipelineView, String> {
    let dir = PathBuf::from(work_path);
    let mut p = pipeline::load_pipeline(&dir)?;
    p.redo(stage);
    pipeline::save_pipeline(&dir, &p)?;
    Ok(p.to_view(&dir))
}

// ---------------------------------------------------------------------------
// 设定库 + 账本

#[tauri::command]
fn list_settings(work_path: String) -> Result<Vec<SettingMeta>, String> {
    lore::list_settings(&PathBuf::from(work_path))
}

#[tauri::command]
fn create_setting(
    work_path: String,
    kind: String,
    name: String,
    content: String,
    hidden: bool,
    open_at_vol: Option<u32>,
) -> Result<SettingMeta, String> {
    let visibility = if hidden { Visibility::Hidden } else { Visibility::Visible };
    lore::create_setting(
        &PathBuf::from(work_path),
        &kind,
        &name,
        &content,
        visibility,
        open_at_vol,
    )
}

#[tauri::command]
fn set_setting_visibility(
    work_path: String,
    rel_path: String,
    hidden: bool,
    open_at_vol: Option<u32>,
) -> Result<(), String> {
    let visibility = if hidden { Visibility::Hidden } else { Visibility::Visible };
    lore::set_visibility(&PathBuf::from(work_path), &rel_path, visibility, open_at_vol)
}

/// 账本视图（结构化解析）。
#[tauri::command]
fn get_ledger(work_path: String) -> Result<ledger::Ledger, String> {
    ledger::read_ledger(&PathBuf::from(work_path))
}

/// 装配预览（档位/预算可调，实际生成共用同一出口）。
#[tauri::command]
fn preview_assembly(
    work_path: String,
    volume: u32,
    chapter: String,
    setting_detail: String,
    token_budget: u64,
) -> Result<assembly::AssemblyResult, String> {
    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter;
    opts.setting_detail = if setting_detail == "name_only" {
        lore::Detail::NameOnly
    } else {
        lore::Detail::Full
    };
    opts.token_budget = token_budget;
    assembly::assemble(&PathBuf::from(work_path), &opts)
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
/// chat agent 带全工具（含提议工具）——旁路产物经 diff 审阅回流（D0-5）。
#[tauri::command]
async fn chat_stream(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    session_id: String,
    prompt: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;
    let spec: AgentSpec = chat_spec();

    let tool_ctx = ToolCtx {
        work_root: work_dir.clone(),
        queue: Arc::clone(&queue.0),
        on_proposal: Arc::new(move |p: &Proposal| {
            let _ = app.emit("review:new", p);
        }),
    };

    let mut sess = session::load_session(&work_dir, &session_id).unwrap_or(Session {
        id: session_id.clone(),
        created_at: works::now_iso(),
        messages: Vec::new(),
    });

    let history = sess.messages.clone();
    let (_text, new_messages) = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        history,
        &prompt,
        move |ev| {
            let _ = on_event.send(ev);
        },
    )
    .await?;

    sess.messages.extend(new_messages);
    session::save_session(&work_dir, &sess)
}

// ---------------------------------------------------------------------------
// diff 审阅

#[tauri::command]
fn list_proposals(queue: State<'_, SharedQueue>) -> Vec<Proposal> {
    queue.0.lock().unwrap().list()
}

#[tauri::command]
fn apply_proposal(queue: State<'_, SharedQueue>, work_path: String, id: u64) -> Result<String, String> {
    let p = queue
        .0
        .lock()
        .unwrap()
        .take(id)
        .ok_or(format!("提议 #{id} 不存在（可能已被处理）"))?;
    review::apply(&PathBuf::from(work_path), p)
}

#[tauri::command]
fn reject_proposal(queue: State<'_, SharedQueue>, id: u64) -> Result<(), String> {
    queue
        .0
        .lock()
        .unwrap()
        .take(id)
        .ok_or(format!("提议 #{id} 不存在（可能已被处理）"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SharedQueue::default())
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
            // 流水线
            get_pipeline,
            pipeline_enter,
            pipeline_confirm,
            pipeline_skip,
            pipeline_redo,
            // 设定库 + 账本
            list_settings,
            create_setting,
            set_setting_visibility,
            get_ledger,
            preview_assembly,
            // 会话
            list_sessions,
            load_session_view,
            chat_stream,
            // diff 审阅
            list_proposals,
            apply_proposal,
            reject_proposal,
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
