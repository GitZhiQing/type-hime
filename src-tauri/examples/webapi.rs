//! Web 测试通道（D18）：把与 Tauri 命令相同的业务层以 HTTP+SSE 暴露在本机，
//! 浏览器直接测真实后端（git/装配/DeepSeek 流式），仅绑定 127.0.0.1，仅测试用。
//!
//! 启动：cargo run --example webapi   （默认 127.0.0.1:18360，与 vite proxy 对应）
//! 前端：pnpm dev 已将 /api 代理到本服务；tauri.ts 自动分流传输层。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::{get, post};
use axum::Router;
use serde_json::{json, Value};

use type_hime_lib::agent::events::AgentEvent;
use type_hime_lib::flows::{self, EventSink, ProposalSink};
use type_hime_lib::review::{Proposal, SharedQueue};
use type_hime_lib::{
    agent, assembly, git, issues, ledger, lore, pipeline, preflight, review, settings, works,
};

// ---------------------------------------------------------------------------
// 全局状态（与 Tauri manage 等价）

fn queue() -> &'static SharedQueue {
    static Q: OnceLock<SharedQueue> = OnceLock::new();
    Q.get_or_init(SharedQueue::restored)
}

fn events() -> &'static tokio::sync::broadcast::Sender<String> {
    static E: OnceLock<tokio::sync::broadcast::Sender<String>> = OnceLock::new();
    E.get_or_init(|| {
        let (tx, _) = tokio::sync::broadcast::channel(128);
        tx
    })
}

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    rt.block_on(async move {
        let app = Router::new()
            .route("/api/events", get(events_endpoint))
            .route("/api/{cmd}", post(dispatch));
        let addr = SocketAddr::from(([127, 0, 0, 1], 18360));
        println!("webapi 监听 http://{addr} （仅本机测试通道，配合 pnpm dev 的 /api 代理）");
        let listener = tokio::net::TcpListener::bind(addr).await.expect("bind 18360");
        axum::serve(listener, app).await.expect("serve");
    });
}

async fn events_endpoint() -> Response {
    let rx = events().subscribe();
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        match rx.recv().await {
            Ok(msg) => Some((Ok::<_, Infallible>(Event::default().data(msg)), rx)),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => None,
            Err(_) => None,
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}

// ---------------------------------------------------------------------------
// dispatch：POST /api/{cmd}，body 为 camelCase JSON（与前端 invoke 参数一致）

fn session_store_work(
    work_path: &str,
) -> Result<type_hime_lib::agent::session::SessionStore, String> {
    Ok(type_hime_lib::agent::session::SessionStore::Work(PathBuf::from(
        work_path,
    )))
}

async fn dispatch(Path(cmd): Path<String>, body: Option<Json<Value>>) -> Response {
    let body = body.map(|Json(v)| v).unwrap_or(Value::Null);
    let s = |k: &str| body[k].as_str().unwrap_or_default().to_string();
    let n = |k: &str| body[k].as_u64().unwrap_or(0) as u32;
    let opt_str = |k: &str| body[k].as_str().map(String::from);

    let q = queue();
    let q_arc = Arc::clone(&q.0);

    macro_rules! ok {
        ($e:expr) => {{
            match $e {
                Ok(v) => Json(v).into_response(),
                Err(e) => (StatusCode::BAD_REQUEST, Json(json!({"error": e}))).into_response(),
            }
        }};
    }

    match cmd.as_str() {
        // ---- 作品底座 ----
        "get_works_root" => ok!(works::load_config().map(|c| c.works_root)),
        "list_works" => ok!(works::list_works()),
        "create_work" => ok!(works::create_work(&s("name"), opt_str("base"))),
        "read_tree" => ok!(works::read_tree(&PathBuf::from(s("workPath")))),
        "read_file" => ok!(works::read_file(&PathBuf::from(s("workPath")), &s("relPath"))),
        "write_file" => ok!(works::write_file(&PathBuf::from(s("workPath")), &s("relPath"), &s("content"))),
        "create_file" => ok!(works::create_file(&PathBuf::from(s("workPath")), &s("relPath"), &s("content"))),
        "delete_file" => ok!(works::delete_file(&PathBuf::from(s("workPath")), &s("relPath"))),
        "restore_file" => ok!(works::restore_file(&PathBuf::from(s("workPath")), &s("relPath"))),
        "git_log" => ok!(git::log_oneline(
            &PathBuf::from(s("workPath")),
            body["limit"].as_u64().unwrap_or(20) as usize
        )),

        // ---- 设置 ----
        "get_settings" => ok!(settings::load().map(|x| settings::to_view(&x))),
        "list_provider_presets" => Json(settings::provider_presets()).into_response(),
        "upsert_provider" => {
            let cfg = settings::load().unwrap_or_default();
            let profile = settings::ProviderProfile {
                id: s("id"),
                name: s("name"),
                base_url: s("baseUrl").trim_end_matches('/').to_string(),
                api_key: s("apiKey"),
                default_model: s("defaultModel"),
                context_window: body["contextWindow"]
                    .as_u64()
                    .unwrap_or(settings::DEFAULT_CONTEXT_WINDOW),
            };
            let activate = body["activate"].as_bool().unwrap_or(false);
            ok!(settings::upsert_provider(cfg, profile, activate).and_then(|c| settings::save(&c)))
        }
        "remove_provider" => ok!(settings::load()
            .and_then(|c| settings::remove_provider(c, &s("id")))
            .and_then(|c| settings::save(&c))),
        "set_active_provider" => ok!(settings::load()
            .and_then(|c| settings::set_active(c, &s("id")))
            .and_then(|c| settings::save(&c))),
        "test_provider" => {
            let cfg = settings::load().unwrap_or_default();
            let base = if s("baseUrl").is_empty() {
                cfg.active().map(|p| p.base_url.clone()).unwrap_or_default()
            } else {
                s("baseUrl")
            };
            let key = if s("apiKey").is_empty() {
                cfg.active().map(|p| p.api_key.clone()).unwrap_or_default()
            } else {
                s("apiKey")
            };
            let model = if s("model").is_empty() {
                cfg.active().map(|p| p.default_model.clone()).unwrap_or_default()
            } else {
                s("model")
            };
            ok!(agent::kernel::test_connection(&base, &key, &model).await)
        }
        "list_models" => {
            let cfg = settings::load().unwrap_or_default();
            let base = if s("baseUrl").is_empty() {
                cfg.active().map(|p| p.base_url.clone()).unwrap_or_default()
            } else {
                s("baseUrl")
            };
            let key = if s("apiKey").is_empty() {
                cfg.active().map(|p| p.api_key.clone()).unwrap_or_default()
            } else {
                s("apiKey")
            };
            ok!(agent::kernel::list_models(&base, &key).await)
        }
        "set_agent_model_override" => {
            let mut c = settings::load().unwrap_or_default();
            match opt_str("model").filter(|m| !m.trim().is_empty()) {
                Some(m) => {
                    c.agent_overrides.insert(s("agentId"), m.trim().to_string());
                }
                None => {
                    c.agent_overrides.remove(&s("agentId"));
                }
            }
            ok!(settings::save(&c))
        }

        // ---- 流水线 ----
        "get_pipeline" => {
            let dir = PathBuf::from(s("workPath"));
            ok!(pipeline::load_pipeline(&dir).map(|p| p.to_view(&dir)))
        }
        "pipeline_enter" => {
            let stage: pipeline::Stage =
                serde_json::from_value(body["stage"].clone()).unwrap_or(pipeline::Stage::Settings);
            let dir = PathBuf::from(s("workPath"));
            ok!(pipeline::load_pipeline(&dir).and_then(|mut p| {
                p.enter(stage, &dir)?;
                pipeline::save_pipeline(&dir, &p)?;
                Ok(p.to_view(&dir))
            }))
        }
        "pipeline_confirm" => {
            let stage: pipeline::Stage =
                serde_json::from_value(body["stage"].clone()).unwrap_or(pipeline::Stage::Settings);
            let dir = PathBuf::from(s("workPath"));
            ok!(pipeline::load_pipeline(&dir).and_then(|mut p| {
                p.confirm(stage, &dir)?;
                pipeline::save_pipeline(&dir, &p)?;
                Ok(p.to_view(&dir))
            }))
        }
        "pipeline_skip" => {
            let stage: pipeline::Stage =
                serde_json::from_value(body["stage"].clone()).unwrap_or(pipeline::Stage::Settings);
            let dir = PathBuf::from(s("workPath"));
            ok!(pipeline::load_pipeline(&dir)
                .and_then(|mut p| {
                    p.skip_confirmed(stage);
                    pipeline::save_pipeline(&dir, &p)?;
                    Ok(p.to_view(&dir))
                }))
        }
        "pipeline_redo" => {
            let stage: pipeline::Stage =
                serde_json::from_value(body["stage"].clone()).unwrap_or(pipeline::Stage::Settings);
            let dir = PathBuf::from(s("workPath"));
            ok!(pipeline::load_pipeline(&dir)
                .and_then(|mut p| {
                    p.redo(stage);
                    pipeline::save_pipeline(&dir, &p)?;
                    Ok(p.to_view(&dir))
                }))
        }

        // ---- 设定/账本/装配 ----
        "list_settings" => ok!(lore::list_settings(&PathBuf::from(s("workPath")))),
        "create_setting" => {
            let vis = if body["hidden"].as_bool().unwrap_or(false) {
                lore::Visibility::Hidden
            } else {
                lore::Visibility::Visible
            };
            ok!(lore::create_setting(
                &PathBuf::from(s("workPath")),
                &s("kind"),
                &s("name"),
                &s("content"),
                vis,
                body["openAtVol"].as_u64().map(|v| v as u32),
            ))
        }
        "set_setting_visibility" => {
            let vis = if body["hidden"].as_bool().unwrap_or(false) {
                lore::Visibility::Hidden
            } else {
                lore::Visibility::Visible
            };
            ok!(lore::set_visibility(
                &PathBuf::from(s("workPath")),
                &s("relPath"),
                vis,
                body["openAtVol"].as_u64().map(|v| v as u32),
            ))
        }
        "get_ledger" => ok!(ledger::read_ledger(&PathBuf::from(s("workPath")))),
        "check_visibility_leak" => ok!(lore::check_visibility_leak(&PathBuf::from(s("workPath")))),
        "list_skills" => Json(agent::skills::list_skill_meta(
            opt_str("workPath").map(PathBuf::from).as_deref(),
        ))
        .into_response(),
        "context_overview" => ok!(flows::context_overview(
            &s("workPath"),
            opt_str("sessionId").as_deref(),
            n("volume"),
            &s("chapter")
        )),
        "preview_assembly" => {
            let mut opts = assembly::AssemblyOptions::default();
            opts.volume = n("volume");
            opts.chapter = s("chapter");
            opts.setting_detail = match s("settingDetail").as_str() {
                "name_only" => lore::Detail::NameOnly,
                "compact" => lore::Detail::Compact,
                _ => lore::Detail::Full,
            };
            opts.token_budget = body["tokenBudget"].as_u64().unwrap_or(16000);
            opts.exclude_components = body["excludeComponents"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            ok!(assembly::assemble(&PathBuf::from(s("workPath")), &opts))
        }
        "get_assembly_prefs" => ok!(pipeline::load_assembly_prefs(&PathBuf::from(s("workPath")))),
        "set_assembly_prefs" => {
            let prefs = pipeline::AssemblyPrefs {
                setting_detail: opt_str("settingDetail"),
                token_budget: body["tokenBudget"].as_u64(),
                exclude_components: body["excludeComponents"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
            };
            ok!(pipeline::save_assembly_prefs(&PathBuf::from(s("workPath")), &prefs))
        }

        // ---- 生成（流式 SSE） ----
        "chat_stream" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_path = s("workPath");
            let session_id = s("sessionId");
            let prompt = s("prompt");
            async move { flows::chat_flow(&q, work_path, session_id, prompt, sink, prop).await }
        }),
        "generate_artifact" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_path = s("workPath");
            let volume = n("volume");
            let chapter = s("chapter");
            let target = s("target");
            async move {
                flows::generate_artifact_flow(&q, work_path, volume, chapter, target, sink, prop)
                    .await
            }
        }),
        "propose_ledger_update" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_path = s("workPath");
            let volume = n("volume");
            let chapter = s("chapter");
            async move { flows::ledger_update_flow(&q, work_path, volume, chapter, sink, prop).await }
        }),
        "review_chapter" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_dir: std::path::PathBuf = s("workPath").into();
            let volume = n("volume");
            let chapter = s("chapter");
            let mode = s("mode");
            async move {
                flows::review_flow(&q, &work_dir, volume, &chapter, &mode, &sink, prop).await
            }
        }),
        "recheck_chapter" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_path = s("workPath");
            let volume = n("volume");
            let chapter = s("chapter");
            async move { flows::recheck_flow(&q, work_path, volume, chapter, sink, prop).await }
        }),
        "review_volume" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_path = s("workPath");
            let volume = n("volume");
            async move { flows::review_volume_flow(&q, work_path, volume, sink, prop).await }
        }),
        "tidy_ledger" => stream_sse(|sink, prop| {
            let q = Arc::clone(&q_arc);
            let work_path = s("workPath");
            async move { flows::tidy_flow(&q, work_path, sink, prop).await }
        }),
        "preflight_chapter" => {
            let dir = PathBuf::from(s("workPath"));
            let ch_path = assembly::chapter_path(n("volume"), &s("chapter"));
            ok!(works::read_file(&dir, &ch_path)
                .and_then(|text| {
                    let l = ledger::read_ledger(&dir)?;
                    Ok(preflight::preflight_cultivation(&text, &l))
                }))
        }

        // ---- 审阅 / 清单 / 成本 ----
        "list_proposals" => Json(q_arc.lock().unwrap().list()).into_response(),
        "apply_proposal" => {
            let id = body["id"].as_u64().unwrap_or(0);
            let taken = q_arc
                .lock()
                .unwrap()
                .take(id)
                .ok_or(format!("提议 #{id} 不存在（可能已被处理）"));
            ok!(taken.and_then(|p| review::apply(&PathBuf::from(s("workPath")), p)))
        }
        "reject_proposal" => {
            let id = body["id"].as_u64().unwrap_or(0);
            ok!(q_arc
                .lock()
                .unwrap()
                .reject_with_comment(id, opt_str("comment"))
                .map(|_| ())
                .ok_or(format!("提议 #{id} 不存在")))
        }
        "list_issues" => ok!(issues::list(
            &PathBuf::from(s("workPath")),
            opt_str("chapter").as_deref()
        )),
        "record_issue_ruling" => ok!(issues::record_ruling(
            &PathBuf::from(s("workPath")),
            body["id"].as_u64().unwrap_or(0),
            &s("status"),
            opt_str("note"),
        )),
        "get_usage_summary" => {
            let dir = PathBuf::from(s("workPath"));
            let mut total = agent::kernel::UsageReport::default();
            if let Ok(entries) = std::fs::read_dir(dir.join(".sessions")) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) == Some("json") {
                        if let Ok(raw) = std::fs::read_to_string(&p) {
                            if let Ok(sess) =
                                serde_json::from_str::<type_hime_lib::agent::session::Session>(&raw)
                            {
                                if let Some(u) = sess.usage {
                                    total.input_tokens += u.input_tokens;
                                    total.output_tokens += u.output_tokens;
                                }
                            }
                        }
                    }
                }
            }
            Json(total).into_response()
        }

        // ---- 会话（作品会话 + 全局对话会话） ----
        "list_sessions" => ok!(session_store_work(&s("workPath")).and_then(|st| {
            type_hime_lib::agent::session::list_sessions(&st)
        })),
        "rename_session" => ok!(session_store_work(&s("workPath")).and_then(|st| {
            type_hime_lib::agent::session::rename_session(&st, &s("sessionId"), &s("title"))
        })),
        "delete_session" => ok!(session_store_work(&s("workPath")).and_then(|st| {
            type_hime_lib::agent::session::delete_session(&st, &s("sessionId"))
        })),
        "load_session_view" => ok!(session_store_work(&s("workPath")).and_then(|st| {
            type_hime_lib::agent::session::load_session(&st, &s("sessionId"))
        })
        .map(|sess| type_hime_lib::agent::session::message_views(&sess.messages))),
        "list_sessions_global" => ok!(type_hime_lib::agent::session::list_sessions(
            &type_hime_lib::agent::session::SessionStore::Global
        )),
        "rename_session_global" => ok!(type_hime_lib::agent::session::rename_session(
            &type_hime_lib::agent::session::SessionStore::Global,
            &s("sessionId"),
            &s("title")
        )),
        "delete_session_global" => ok!(type_hime_lib::agent::session::delete_session(
            &type_hime_lib::agent::session::SessionStore::Global,
            &s("sessionId")
        )),
        "load_session_view_global" => ok!(
            type_hime_lib::agent::session::load_session(
                &type_hime_lib::agent::session::SessionStore::Global,
                &s("sessionId")
            )
            .map(|sess| type_hime_lib::agent::session::message_views(&sess.messages))
        ),
        "chat_stream_global" => stream_sse(|sink, _prop| {
            let session_id = s("sessionId");
            let prompt = s("prompt");
            async move { flows::chat_flow_global(session_id, prompt, sink).await }
        }),

        other => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": format!("未知命令：{other}")})),
        )
            .into_response(),
    }
}

/// 流式命令：跑业务，事件经 SSE 逐条返回（data 为 AgentEvent JSON）。
/// 业务结束后：出错先发 error 事件，随后发送端 drop，流自然关闭。
fn stream_sse<F, Fut>(make: F) -> Response
where
    F: FnOnce(EventSink, ProposalSink) -> Fut,
    Fut: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let bcast = events().clone();
    let end_tx = tx.clone();
    let sink: EventSink = Arc::new(move |ev: AgentEvent| {
        if let Ok(s) = serde_json::to_string(&ev) {
            let _ = tx.send(s);
        }
    });
    let prop: ProposalSink = Arc::new(move |p: &Proposal| {
        // payload 直接嵌入对象（与 Tauri emit 的语义一致）；
        // 此前双重编码（对象→字符串→再包 JSON）使前端 EventSource 拿到字符串，字段全失效
        let _ = bcast.send(json!({"event": "review:new", "payload": p}).to_string());
    });

    let fut = make(sink, prop);
    tokio::spawn(async move {
        if let Err(e) = fut.await {
            let _ = end_tx.send(json!({"type": "error", "message": e}).to_string());
        }
        // end_tx 随 task 结束 drop；sink 的 tx 随 fut drop → rx 关闭 → SSE 流结束
    });

    let stream = futures::stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|msg| (Ok::<_, Infallible>(Event::default().data(msg)), rx))
    });
    Sse::new(stream).keep_alive(KeepAlive::default()).into_response()
}
