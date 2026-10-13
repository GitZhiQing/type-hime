pub mod agent;
pub mod assembly;
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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use agent::events::AgentEvent;
use agent::kernel;
use agent::session::{self, Session};
use agent::spec::{chat_spec, ledger_spec, reviewer_spec, writer_spec, AgentSpec};
use agent::tools::ToolCtx;
use lore::{SettingMeta, Visibility};
use pipeline::{PipelineView, Stage};
use review::{Proposal, ReviewQueue, SharedQueue};
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
        expect: None,
    };

    let mut sess = session::load_session(&work_dir, &session_id).unwrap_or(Session {
        id: session_id.clone(),
        created_at: works::now_iso(),
        messages: Vec::new(),
        usage: None,
    });

    let history = sess.messages.clone();
    let outcome = kernel::run(
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

    sess.messages.extend(outcome.messages);
    sess.usage = merge_usage(sess.usage, outcome.usage);
    session::save_session(&work_dir, &sess)
}

/// 累计 usage（None + x = x；Some + Some = 和）。
fn merge_usage(
    base: Option<agent::kernel::UsageReport>,
    add: Option<agent::kernel::UsageReport>,
) -> Option<agent::kernel::UsageReport> {
    match (base, add) {
        (None, None) => None,
        (None, Some(u)) => Some(u),
        (Some(b), None) => Some(b),
        (Some(b), Some(u)) => Some(agent::kernel::UsageReport {
            input_tokens: b.input_tokens + u.input_tokens,
            output_tokens: b.output_tokens + u.output_tokens,
        }),
    }
}

// ---------------------------------------------------------------------------
// 生成流水线（内核组合 Agent：writer / ledger）

/// 生成章纲或正文：装配 → writer agent 流式生成 → 提议经审阅队列。
/// target: "chapter_outline" | "chapter"
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
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.clone();
    opts.stage = if target == "chapter_outline" {
        pipeline::Stage::ChapterOutline
    } else {
        pipeline::Stage::Chapters
    };

    let assembled = assembly::assemble(&work_dir, &opts)?;
    if assembled.over_budget {
        return Err(format!(
            "装配已自动降档至 {} 仍约 {} tokens，超出预算 {}；请精简账本/设定或提高预算",
            assembled.applied_detail, assembled.total_tokens, opts.token_budget
        ));
    }

    // 预检目标产物路径，写进任务指令；写入校验期望（工具即校验器）
    let (file_hint, stage_hint, expect) = if target == "chapter_outline" {
        (
            assembly::chapter_outline_path(volume, &chapter),
            "撰写本章章纲（用 propose_write 提交，路径见下）",
            Some(agent::tools::WriteExpect {
                min_chars: 80,
                max_chars: 800,
                check_halfwidth: true,
            }),
        )
    } else {
        (
            assembly::chapter_path(volume, &chapter),
            "撰写本章正文全文（用 propose_write 提交，路径见下）",
            Some(agent::tools::WriteExpect {
                min_chars: 1500,
                max_chars: 3800,
                check_halfwidth: true,
            }),
        )
    };

    let mut spec = writer_spec(&assembled.system);
    spec.sections.push((
        "task".to_string(),
        format!("{stage_hint}\n目标文件：{file_hint}"),
    ));

    let tool_ctx = ToolCtx {
        work_root: work_dir.clone(),
        queue: Arc::clone(&queue.0),
        on_proposal: Arc::new(move |p: &review::Proposal| {
            let _ = app.emit("review:new", p);
        }),
        expect,
    };

    let prompt = format!(
        "{}\n\n# 输出文件\n{file_hint}\n\n请开始工作：完成后用 propose_write 提交全文。",
        assembly::render_prompt(&assembled)
    );

    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| {
            let _ = on_event.send(ev);
        },
    )
    .await?;

    // 生成会话落盘（审计 + usage）
    let sess = Session {
        id: format!("gen-{target}-{volume:02}-{chapter}"),
        created_at: works::now_iso(),
        messages: outcome.messages,
        usage: outcome.usage,
    };
    let _ = session::save_session(&work_dir, &sess);
    Ok(())
}

/// 账本回写：读本章正文 + 账本 → ledger agent 提议更新账本。
#[tauri::command]
async fn propose_ledger_update(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    chapter: String,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;
    let spec = ledger_spec();

    let ch_path = assembly::chapter_path(volume, &chapter);
    if !work_dir.join(&ch_path).exists() {
        return Err(format!("本章正文不存在：{ch_path}（先完成正文生成与审阅）"));
    }

    let tool_ctx = ToolCtx {
        work_root: work_dir.clone(),
        queue: Arc::clone(&queue.0),
        on_proposal: Arc::new(move |p: &review::Proposal| {
            let _ = app.emit("review:new", p);
        }),
        expect: None,
    };

    let prompt = format!(
        "第{volume}卷 {chapter} 已完稿。请先 read_file 读取正文 {ch_path} 与账本 {ledger_path}，\
         然后 propose_edit 提交更新后的账本全文（路径 {ledger_path}）。",
        ch_path = ch_path,
        ledger_path = ledger::LEDGER_PATH,
    );

    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| {
            let _ = on_event.send(ev);
        },
    )
    .await?;

    let sess = Session {
        id: format!("gen-ledger-{volume:02}-{chapter}"),
        created_at: works::now_iso(),
        messages: outcome.messages,
        usage: outcome.usage,
    };
    let _ = session::save_session(&work_dir, &sess);
    Ok(())
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

/// 审校核心（命令与卷级批审共用）。mode: "consistency" | "deai"
async fn run_review_core(
    app: &AppHandle,
    queue_arc: &Arc<std::sync::Mutex<ReviewQueue>>,
    work_dir: &Path,
    volume: u32,
    chapter: &str,
    mode: &str,
    on_event: &Channel<AgentEvent>,
) -> Result<(), String> {
    let app = app.clone();
    let on_event = on_event.clone();
    let settings = kernel::current_settings()?;

    let ch_path = assembly::chapter_path(volume, chapter);
    if !work_dir.join(&ch_path).exists() {
        return Err(format!("本章正文不存在：{ch_path}"));
    }

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.to_string();
    opts.stage = pipeline::Stage::Review;
    let assembled = assembly::assemble(work_dir, &opts)?;

    // 代码级预检（零 token）：修为层级等连续量与账本比对，结果注入审校
    let preflight_report = if mode != "deai" {
        let text = works::read_file(work_dir, &ch_path)?;
        let ledger = ledger::read_ledger(work_dir)?;
        preflight::preflight_cultivation(&text, &ledger)
    } else {
        Vec::new()
    };
    if !preflight_report.is_empty() {
        for line in &preflight_report {
            let _ = on_event.send(AgentEvent::TextDelta {
                text: format!("[代码预检] {line}\n"),
            });
        }
    }

    let mut spec = reviewer_spec(&assembled.system);
    let task = if mode == "deai" {
        format!(
            "对 {ch_path} 执行「去 AI 味」：按文风指令改写整章，用 propose_edit 提交整章全文（路径 {ch_path}）。"
        )
    } else {
        let preflight_block = if preflight_report.is_empty() {
            String::new()
        } else {
            format!(
                "\n代码预检已发现以下确定问题（直接核实并纳入修复）：\n{}\n",
                preflight_report
                    .iter()
                    .map(|s| format!("- {s}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        format!(
            "对 {ch_path} 执行「一致性检查」：先 read 正文与账本/设定，输出编号问题清单；\
             确定的硬伤用 propose_edit 提交修复（路径 {ch_path}），拿不准的只列清单。{preflight_block}"
        )
    };
    spec.sections.push(("task".to_string(), task));

    let tool_ctx = ToolCtx {
        work_root: work_dir.to_path_buf(),
        queue: Arc::clone(queue_arc),
        on_proposal: Arc::new(move |p: &review::Proposal| {
            let _ = app.emit("review:new", p);
        }),
        expect: None,
    };

    let prompt = format!(
        "{}\n\n# 待审章节\n{ch_path}\n\n请开始审校。",
        assembly::render_prompt(&assembled)
    );

    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| {
            let _ = on_event.send(ev);
        },
    )
    .await?;
    let report_text = outcome.text;

    let sess = Session {
        id: format!("review-{mode}-{volume:02}-{chapter}"),
        created_at: works::now_iso(),
        messages: outcome.messages,
        usage: outcome.usage,
    };
    let _ = session::save_session(work_dir, &sess);

    // 一致性模式的报告结构化落盘：全文存档 + 按编号拆条入问题清单
    if mode == "consistency" && !report_text.trim().is_empty() {
        let report_dir = work_dir.join("review");
        let _ = std::fs::create_dir_all(&report_dir);
        let _ = std::fs::write(report_dir.join(format!("报告-{chapter}.md")), &report_text);
        let _ = issues::ingest_report(work_dir, chapter, "consistency", &report_text);
        for p in &preflight_report {
            let _ = issues::ingest_report(work_dir, chapter, "preflight", p);
        }
    }
    Ok(())
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
    run_review_core(
        &app,
        &queue.0,
        &PathBuf::from(work_path),
        volume,
        &chapter,
        &mode,
        &on_event,
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
    let work_dir = PathBuf::from(&work_path);
    let settings = kernel::current_settings()?;
    let ch_path = assembly::chapter_path(volume, &chapter);
    if !work_dir.join(&ch_path).exists() {
        return Err(format!("本章正文不存在：{ch_path}"));
    }

    // 上轮清单 + 裁决记录
    let all = issues::list(&work_dir, Some(&chapter))?;
    if all.is_empty() {
        return Err(format!("{chapter} 没有审校记录，请先执行一致性检查"));
    }
    let dossier = all
        .iter()
        .map(|i| {
            format!(
                "- [#{}][{}] {}\n  裁决：{}",
                i.id,
                i.status,
                i.text.replace('\n', " "),
                i.ruling.clone().unwrap_or_else(|| "（未裁决）".into())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut opts = assembly::AssemblyOptions::default();
    opts.volume = volume;
    opts.chapter = chapter.clone();
    opts.stage = pipeline::Stage::Review;
    let assembled = assembly::assemble(&work_dir, &opts)?;

    let mut spec = reviewer_spec(&assembled.system);
    spec.sections.push(("task".to_string(), format!(
        "复检 {ch_path}：下面是上轮问题清单与作者裁决。逐项核验：\
         (a) 已修复项确认修复生效；(b) 未裁决(open)项若为硬伤给出修复提议，若不成立说明理由建议驳回；\
         (c) 输出「复检结论：通过/不通过 + 原因」。\n\n# 上轮问题清单\n{dossier}"
    )));

    let tool_ctx = ToolCtx {
        work_root: work_dir.clone(),
        queue: Arc::clone(&queue.0),
        on_proposal: Arc::new(move |p: &review::Proposal| {
            let _ = app.emit("review:new", p);
        }),
        expect: None,
    };
    let prompt = format!(
        "{}\n\n# 待复检章节\n{ch_path}\n\n请开始复检。",
        assembly::render_prompt(&assembled)
    );
    let outcome = kernel::run(
        &settings,
        Some(&tool_ctx),
        &spec,
        Vec::new(),
        &prompt,
        move |ev| {
            let _ = on_event.send(ev);
        },
    )
    .await?;
    let recheck_text = outcome.text;
    if !recheck_text.trim().is_empty() {
        let _ = issues::ingest_report(&work_dir, &chapter, "recheck", &recheck_text);
    }
    Ok(())
}

/// 卷级批审：逐章一致性检查（复用 review 逻辑），进度经 Channel 汇报。
#[tauri::command]
async fn review_volume(
    app: AppHandle,
    queue: State<'_, SharedQueue>,
    work_path: String,
    volume: u32,
    on_event: Channel<AgentEvent>,
) -> Result<(), String> {
    let queue_arc: Arc<std::sync::Mutex<ReviewQueue>> = Arc::clone(&queue.0);
    let _work_dir = PathBuf::from(&work_path);
    let work_dir = PathBuf::from(&work_path);
    // 收集本卷正文章节
    let chapters_dir = work_dir
        .join(assembly::volume_dir(volume))
        .join("chapters");
    let mut chapters: Vec<String> = std::fs::read_dir(&chapters_dir)
        .map_err(|e| format!("读取章节目录失败：{e}"))?
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.ends_with(".md") && !name.contains("-章纲") {
                Some(name.trim_end_matches(".md").to_string())
            } else {
                None
            }
        })
        .collect();
    chapters.sort();
    if chapters.is_empty() {
        return Err(format!("第 {volume} 卷没有正文章节"));
    }

    let _ = on_event.send(AgentEvent::TextDelta {
        text: format!("[卷级批审] 共 {} 章：{}\n", chapters.len(), chapters.join("、")),
    });
    let mut failed = Vec::new();
    for ch in &chapters {
        let _ = on_event.send(AgentEvent::TextDelta {
            text: format!("\n[卷级批审] 开始 {ch} …\n"),
        });
        let result = run_review_core(
            &app,
            &queue_arc,
            &work_dir,
            volume,
            ch,
            "consistency",
            &on_event,
        )
        .await;
        if let Err(e) = result {
            failed.push(format!("{ch}: {e}"));
        }
    }
    let summary = if failed.is_empty() {
        format!("\n[卷级批审] 全部 {} 章完成，问题清单见右栏/裁决后复检。\n", chapters.len())
    } else {
        format!("\n[卷级批审] 完成，部分失败：{}\n", failed.join("；"))
    };
    let _ = on_event.send(AgentEvent::TextDelta { text: summary });
    Ok(())
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
            // 会话
            list_sessions,
            load_session_view,
            chat_stream,
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
