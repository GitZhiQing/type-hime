pub mod agent;
pub mod assembly;
pub mod flows;
mod frontmatter;
pub mod git;
pub mod issues;
pub mod ledger;
pub mod lore;
pub mod pipeline;
pub mod preflight;
pub mod review;
pub mod settings;
pub mod works;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use agent::events::AgentEvent;
use agent::kernel;
use agent::session::{self, Session};
use agent::skills;
use lore::{SettingMeta, Visibility};
use pipeline::{PipelineView, Stage};
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
    context_window: Option<u64>,
    activate: bool,
) -> Result<(), String> {
    let s = settings::load()?;
    let profile = ProviderProfile {
        id,
        name,
        base_url: base_url.trim().trim_end_matches('/').to_string(),
        api_key,
        default_model: default_model.trim().to_string(),
        context_window: context_window.unwrap_or(settings::DEFAULT_CONTEXT_WINDOW),
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

/// 装配预览（档位/预算可调，实际生成共用同一出口；超预算自动降档）。
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
    opts.setting_detail = match setting_detail.as_str() {
        "name_only" => lore::Detail::NameOnly,
        "compact" => lore::Detail::Compact,
        _ => lore::Detail::Full,
    };
    opts.token_budget = token_budget;
    assembly::assemble(&PathBuf::from(work_path), &opts)
}

/// 上下文总览（R3）：会话真实用量 + 装配估算 + 模型窗口。
#[tauri::command]
fn context_overview(
    work_path: String,
    session_id: Option<String>,
    volume: u32,
    chapter: String,
) -> Result<flows::ContextOverview, String> {
    flows::context_overview(&work_path, session_id.as_deref(), volume, &chapter)
}

// ---------------------------------------------------------------------------
// 会话（通用对话 agent；作品会话 + 全局对话会话，REFACTOR R1）

#[tauri::command]
fn list_sessions(work_path: String) -> Result<Vec<session::SessionMeta>, String> {
    session::list_sessions(&session::SessionStore::Work(PathBuf::from(work_path)))
}

#[tauri::command]
fn rename_session(work_path: String, session_id: String, title: String) -> Result<(), String> {
    session::rename_session(
        &session::SessionStore::Work(PathBuf::from(work_path)),
        &session_id,
        &title,
    )
}

#[tauri::command]
fn delete_session(work_path: String, session_id: String) -> Result<(), String> {
    session::delete_session(
        &session::SessionStore::Work(PathBuf::from(work_path)),
        &session_id,
    )
}

#[tauri::command]
fn load_session_view(work_path: String, session_id: String) -> Result<Vec<session::MessageView>, String> {
    let s = session::load_session(
        &session::SessionStore::Work(PathBuf::from(work_path)),
        &session_id,
    )?;
    Ok(s.messages.iter().map(session::message_view).collect())
}

#[tauri::command]
fn list_sessions_global() -> Result<Vec<session::SessionMeta>, String> {
    session::list_sessions(&session::SessionStore::Global)
}

#[tauri::command]
fn rename_session_global(session_id: String, title: String) -> Result<(), String> {
    session::rename_session(&session::SessionStore::Global, &session_id, &title)
}

#[tauri::command]
fn delete_session_global(session_id: String) -> Result<(), String> {
    session::delete_session(&session::SessionStore::Global, &session_id)
}

#[tauri::command]
fn load_session_view_global(session_id: String) -> Result<Vec<session::MessageView>, String> {
    let s = session::load_session(&session::SessionStore::Global, &session_id)?;
    Ok(s.messages.iter().map(session::message_view).collect())
}

#[tauri::command]
async fn chat_stream(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    session_id: String,
    prompt: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::chat_flow(&queue.0, work_path, session_id, prompt, sink, prop).await
}

/// 全局对话（rail「对话」页）：无作品上下文，无提议。
#[tauri::command]
async fn chat_stream_global(
    session_id: String,
    prompt: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    flows::chat_flow_global(session_id, prompt, sink).await
}

// ---------------------------------------------------------------------------
// 生成流水线（内核组合 Agent：writer / ledger）

#[tauri::command]
async fn generate_artifact(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    chapter: String,
    target: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::generate_artifact_flow(&queue.0, work_path, volume, chapter, target, sink, prop).await
}

#[tauri::command]
async fn propose_ledger_update(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    chapter: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::ledger_update_flow(&queue.0, work_path, volume, chapter, sink, prop).await
}

/// 技能清单（R2：`/` 补全与技能文档制品的数据源；workPath 缺省=仅内置技能）。
#[tauri::command]
fn list_skills(work_path: Option<String>) -> Result<Vec<skills::SkillMeta>, String> {
    Ok(skills::list_skill_meta(work_path.map(PathBuf::from).as_deref()))
}

/// 关联泄漏检查（零 token）：可见条目内容是否大段复述隐藏条目。
#[tauri::command]
fn check_visibility_leak(work_path: String) -> Result<Vec<String>, String> {
    lore::check_visibility_leak(&PathBuf::from(work_path))
}

/// 账本整理 pass：合并同线索/归档已回收/压缩休眠/补时间线（提议经审阅）。
#[tauri::command]
async fn tidy_ledger(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::tidy_flow(&queue.0, work_path, sink, prop).await
}

/// 独立预检（零 token 快速检查，不发模型请求）。
#[tauri::command]
fn preflight_chapter(
    work_path: String,
    volume: u32,
    chapter: String,
) -> Result<Vec<String>, String> {
    let work_dir = PathBuf::from(work_path);
    let ch_path = assembly::chapter_path(volume, &chapter);
    let text = works::read_file(&work_dir, &ch_path)?;
    let ledger = ledger::read_ledger(&work_dir)?;
    Ok(preflight::preflight_cultivation(&text, &ledger))
}

/// 审校：一致性检查 / 去 AI 味。mode: "consistency" | "deai"
#[tauri::command]
async fn review_chapter(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    chapter: String,
    mode: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::review_flow(
        &queue.0,
        &PathBuf::from(work_path),
        volume,
        &chapter,
        &mode,
        &sink,
        prop,
    )
    .await
}
/// 复检：对照上轮问题清单与裁决记录逐项验证。
#[tauri::command]
async fn recheck_chapter(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    chapter: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::recheck_flow(&queue.0, work_path, volume, chapter, sink, prop).await
}

/// 卷级批审：逐章一致性检查，进度经 Channel 汇报。
#[tauri::command]
async fn review_volume(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let sink: flows::EventSink = Arc::new(move |ev| {
        let _ = on_event.send(ev);
    });
    let app2 = app.clone();
    let prop: flows::ProposalSink = Arc::new(move |p| {
        let _ = app2.emit("review:new", p);
    });
    flows::review_volume_flow(&queue.0, work_path, volume, sink, prop).await
}

/// 问题清单视图。
#[tauri::command]
fn list_issues(work_path: String, chapter: Option<String>) -> Result<Vec<issues::IssueRecord>, String> {
    issues::list(&PathBuf::from(work_path), chapter.as_deref())
}

/// 作品级 token 成本汇总（扫 .sessions 的真实 usage）。
#[tauri::command]
fn get_usage_summary(work_path: String) -> Result<agent::kernel::UsageReport, String> {
    let dir = PathBuf::from(work_path).join(".sessions");
    let mut total = agent::kernel::UsageReport::default();
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("读取会话目录失败：{e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(sess) = serde_json::from_str::<Session>(&raw) {
                if let Some(u) = sess.usage {
                    total.input_tokens += u.input_tokens;
                    total.output_tokens += u.output_tokens;
                }
            }
        }
    }
    Ok(total)
}

/// 裁决记录。
#[tauri::command]
fn record_issue_ruling(
    work_path: String,
    id: u64,
    status: String,
    note: Option<String>,
) -> Result<(), String> {
    issues::record_ruling(&PathBuf::from(work_path), id, &status, note)
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
fn reject_proposal(
    queue: State<'_, SharedQueue>,
    id: u64,
    comment: Option<String>,
) -> Result<(), String> {
    queue
        .0
        .lock()
        .unwrap()
        .reject_with_comment(id, comment)
        .ok_or(format!("提议 #{id} 不存在（可能已被处理）"))?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(SharedQueue::restored())
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
            list_skills,
            context_overview,
            // 会话
            list_sessions,
            rename_session,
            delete_session,
            load_session_view,
            list_sessions_global,
            rename_session_global,
            delete_session_global,
            load_session_view_global,
            chat_stream,
            chat_stream_global,
            // 生成流水线
            generate_artifact,
            propose_ledger_update,
            preflight_chapter,
            review_chapter,
            recheck_chapter,
            review_volume,
            list_issues,
            record_issue_ruling,
            get_usage_summary,
            check_visibility_leak,
            tidy_ledger,
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
