//! Agent 内核：一切 LLM 编排的必经之路（D5）。
//! 职责：解析模型 → 拼装 preamble（分节 + skills）→ 挂载工具 → 驱动流式循环 → 翻译事件。

use std::path::Path;

use futures::StreamExt;
use rig::agent::MultiTurnStreamItem;
use rig::completion::Message;
use rig::prelude::AgentBuilder;
use rig::providers::openai::{OpenAIConfig, Route};
use rig::streaming::{Item, StreamEvent};
use serde::{Deserialize, Serialize};

use super::events::AgentEvent;
use super::skills;
use super::spec::AgentSpec;
use super::tools::{self, ToolCtx};
use crate::settings::{self, AppSettings};

/// 解析 spec 实际使用的模型 id：spec 覆盖 > settings 覆盖 > active 供应商默认。
pub fn resolve_model_id(s: &AppSettings, spec: &AgentSpec) -> Result<String, String> {
    let provider = s
        .active()
        .ok_or("尚未配置模型服务：请先在设置页添加并启用供应商")?;
    Ok(match (&spec.model_override, s.agent_overrides.get(&spec.id)) {
        (Some(m), _) => m.clone(),
        (None, Some(m)) => m.clone(),
        (None, None) => provider.default_model.clone(),
    })
}

/// 拼装 preamble：分节按序 + 技能渲染在尾部。
pub fn build_preamble(spec: &AgentSpec, work_dir: Option<&Path>) -> String {
    let mut parts: Vec<String> = spec
        .sections
        .iter()
        .map(|(name, content)| {
            if name == "identity" {
                content.clone() // 身份节在最前，不加小标题
            } else {
                format!("# {name}\n{content}")
            }
        })
        .collect();

    if !spec.skills.is_empty() {
        let all = skills::load_skills(work_dir);
        let chosen: Vec<skills::Skill> = all
            .into_iter()
            .filter(|s| spec.skills.contains(&s.name))
            .collect();
        let rendered = skills::render_skills(&chosen);
        if !rendered.is_empty() {
            parts.push(rendered);
        }
    }
    parts.join("\n\n")
}

/// 一次运行的成本报告（来自 provider 的真实计数）。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct UsageReport {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// kernel::run 的产出：最终文本 + 本轮消息 + usage。
pub struct RunOutcome {
    pub text: String,
    pub messages: Vec<Message>,
    pub usage: Option<UsageReport>,
}

/// 一次完整的 agent 运行：流式翻译事件并回调。
pub async fn run<F>(
    settings: &AppSettings,
    tool_ctx: Option<&ToolCtx>,
    spec: &AgentSpec,
    history: Vec<Message>,
    prompt: &str,
    on_event: F,
) -> Result<RunOutcome, String>
where
    F: Fn(AgentEvent) + Clone + Send + Sync + 'static,
{
    let provider = settings
        .active()
        .ok_or("尚未配置模型服务：请先在设置页添加并启用供应商")?;
    let model_id = resolve_model_id(settings, spec)?;

    let model = OpenAIConfig::new(provider.api_key.clone())
        .with_base_url(provider.base_url.trim_end_matches('/').to_string())
        .with_route(Route::Chat)
        .client()
        .chat(model_id);

    let preamble = build_preamble(spec, tool_ctx.map(|c| c.work_root.as_path()));

    let base = AgentBuilder::new(model)
        .preamble(preamble)
        .default_max_turns(spec.max_turns);

    // 工具注册面收敛（inner facade）：按 spec.tools 名单构建 DynamicTool 列表一次挂载
    // （rig 0.44 dynamic_tools；免去逐工具 typestate 链式 match，见 tools::erase）
    let agent = match tool_ctx {
        None => base.build(),
        Some(ctx) => {
            let entries = tools::build_tools(ctx, &spec.tools);
            if entries.is_empty() {
                base.build()
            } else {
                base.dynamic_tools(entries).build()
            }
        }
    };

    let mut stream = agent.prompt(prompt).history(history).stream();
    let mut final_text = String::new();
    let mut new_messages: Vec<Message> = Vec::new();
    let mut last_usage: Option<UsageReport> = None;

    while let Some(item) = stream.next().await {
        match item {
            Ok(MultiTurnStreamItem::StreamAssistantItem(Item::Event(
                StreamEvent::Text { text, .. },
            ))) => {
                final_text.push_str(&text);
                on_event(AgentEvent::TextDelta { text });
            }
            Ok(MultiTurnStreamItem::ToolCall { tool_call }) => {
                let args =
                    serde_json::to_string(&tool_call.function.arguments).unwrap_or_default();
                on_event(AgentEvent::ToolCall {
                    name: tool_call.function.name.to_string(),
                    args,
                });
            }
            Ok(MultiTurnStreamItem::ToolResult { tool_result }) => {
                let output = serde_json::to_string(&tool_result.content)
                    .unwrap_or_else(|_| "[不可序列化的结果]".into());
                on_event(AgentEvent::ToolResult {
                    name: tool_result.name.to_string(),
                    output,
                });
            }
            Ok(MultiTurnStreamItem::FinalResponse(resp)) => {
                final_text = resp.output().to_string();
                new_messages = resp.messages.clone();
                // usage 成本账（全程总计）
                let u = resp.usage();
                last_usage = Some(UsageReport {
                    input_tokens: u.input_tokens.unwrap_or(0),
                    output_tokens: u.output_tokens.unwrap_or(0),
                });
                on_event(AgentEvent::Final {
                    output: final_text.clone(),
                });
            }
            Ok(_) => {}
            Err(e) => {
                let msg = format!("{e}");
                // 部分成功语义：超轮次但已有文本产出（提议通常也已入队）时按完成收尾
                let is_max_turns = msg.to_lowercase().contains("max turns");
                if is_max_turns && !final_text.trim().is_empty() {
                    on_event(AgentEvent::Final {
                        output: final_text.clone(),
                    });
                    return Ok(RunOutcome {
                        text: final_text,
                        messages: Vec::new(), // 未经历 FinalResponse，无权威历史
                        usage: last_usage,
                    });
                }
                on_event(AgentEvent::Error {
                    message: msg.clone(),
                });
                return Err(msg);
            }
        }
    }

    Ok(RunOutcome {
        text: final_text,
        messages: new_messages,
        usage: last_usage,
    })
}

/// 连通性测试：发一次最小请求（设置页"测试连接"）。
pub async fn test_connection(
    base_url: &str,
    api_key: &str,
    model: &str,
) -> Result<String, String> {
    let model = OpenAIConfig::new(api_key.to_string())
        .with_base_url(base_url.trim_end_matches('/').to_string())
        .with_route(Route::Chat)
        .client()
        .chat(model.to_string());

    let agent = AgentBuilder::new(model)
        .preamble("You are a connectivity probe.")
        .build();

    let resp = agent
        .prompt("回复两个字：正常")
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(resp.output().to_string())
}

/// 单次轻量补全（无工具、无历史）：会话自动标题等辅助调用。
pub async fn one_shot_completion(
    settings: &AppSettings,
    preamble: &str,
    prompt: &str,
) -> Result<String, String> {
    let provider = settings
        .active()
        .ok_or("尚未配置模型服务：请先在设置页添加并启用供应商")?;
    let model = OpenAIConfig::new(provider.api_key.clone())
        .with_base_url(provider.base_url.trim_end_matches('/').to_string())
        .with_route(Route::Chat)
        .client()
        .chat(provider.default_model.clone());

    let agent = AgentBuilder::new(model)
        .preamble(preamble.to_string())
        .build();

    let resp = agent.prompt(prompt).await.map_err(|e| format!("{e}"))?;
    Ok(resp.output().to_string())
}

/// 自动获取模型列表：GET {base_url}/models（OpenAI 兼容）。
pub async fn list_models(base_url: &str, api_key: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/models", base_url.trim_end_matches('/'));
    let client = rig::rig_reqwest::reqwest::Client::new();
    let mut req = client.get(&url);
    if !api_key.is_empty() {
        req = req.bearer_auth(api_key);
    }
    let resp = req
        .send()
        .await
        .map_err(|e| format!("请求失败：{e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let brief: String = body.chars().take(200).collect();
        return Err(format!("HTTP {status}：{brief}"));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("解析失败：{e}"))?;
    let mut models: Vec<String> = json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    Ok(models)
}

/// 读取设置的便捷入口（命令层用）。
pub fn current_settings() -> Result<AppSettings, String> {
    settings::load()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::ProviderProfile;

    fn test_settings() -> AppSettings {
        AppSettings {
            providers: vec![ProviderProfile {
                id: "test".into(),
                name: "Test".into(),
                base_url: "https://example.test/v1".into(),
                api_key: "sk-test".into(),
                default_model: "default-model".into(),
                context_window: crate::settings::DEFAULT_CONTEXT_WINDOW,
            }],
            active_provider: Some("test".into()),
            agent_overrides: [("chat".to_string(), "chat-model".to_string())]
                .into_iter()
                .collect(),
        }
    }

    #[test]
    fn model_resolution_priority() {
        let s = test_settings();
        // override > default
        assert_eq!(
            resolve_model_id(&s, &super::super::spec::chat_spec()).unwrap(),
            "chat-model"
        );
        // spec 内覆盖 > settings override
        let mut spec = super::super::spec::chat_spec();
        spec.model_override = Some("spec-model".into());
        assert_eq!(resolve_model_id(&s, &spec).unwrap(), "spec-model");
        // 无 override 回落 default
        let mut s2 = s.clone();
        s2.agent_overrides.clear();
        assert_eq!(
            resolve_model_id(&s2, &super::super::spec::chat_spec()).unwrap(),
            "default-model"
        );
        // 未配置 provider 报错
        let s3 = AppSettings::default();
        assert!(resolve_model_id(&s3, &spec).is_err());
    }

    #[test]
    fn preamble_sections_and_skills_layering() {
        // chat spec 默认挂 writer-basics（R4-④）：身份节 + 技能节
        let spec = super::super::spec::chat_spec();
        let p = build_preamble(&spec, None);
        assert!(p.contains("执笔公主"));
        assert!(p.contains("# 技能指令"));
        assert!(p.contains("## 技能：writer-basics"));

        // 无技能 spec（全局对话）：只有身份节
        let g = super::super::spec::global_chat_spec();
        let pg = build_preamble(&g, None);
        assert!(!pg.contains("# 技能指令"));

        // 临时作品目录：作品层 skill 覆盖内置层（nearest layer wins）
        let dir = std::env::temp_dir().join(format!("type-hime-skill-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("skills")).unwrap();
        std::fs::write(
            dir.join("skills").join("writer-basics.md"),
            "---\nname: writer-basics\ndescription: 覆盖版\n---\n\n作品层覆盖的写作指令",
        )
        .unwrap();

        let p2 = build_preamble(&spec, Some(&dir));
        assert!(p2.contains("# 技能指令"));
        assert!(p2.contains("作品层覆盖的写作指令"));
        assert!(!p2.contains("目标篇幅内完成")); // 内置版本被覆盖

        let _ = std::fs::remove_dir_all(&dir);
    }
}
