//! 模型接入设置（D7）：多供应商管理，key 只存本地 Rust 侧，任何命令不得原样返回前端。
//! OpenAI 兼容协议为基准（D3）；内置主流供应商预设。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// 供应商预设

#[derive(Debug, Clone, Serialize)]
pub struct ProviderPreset {
    pub id: String,
    pub name: String,
    pub base_url: String,
    /// 该供应商是否无需 key（本地服务）
    pub no_key: bool,
    /// 建议的默认模型（列表获取失败时的回落）
    pub fallback_model: String,
}

pub fn provider_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            no_key: false,
            fallback_model: "deepseek-chat".into(),
        },
        ProviderPreset {
            id: "zhipu".into(),
            name: "智谱 GLM".into(),
            base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
            no_key: false,
            fallback_model: "glm-4.7".into(),
        },
        ProviderPreset {
            id: "moonshot".into(),
            name: "月之暗面 Kimi".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            no_key: false,
            fallback_model: "kimi-latest".into(),
        },
        ProviderPreset {
            id: "openai".into(),
            name: "OpenAI".into(),
            base_url: "https://api.openai.com/v1".into(),
            no_key: false,
            fallback_model: "gpt-5-mini".into(),
        },
        ProviderPreset {
            id: "openrouter".into(),
            name: "OpenRouter".into(),
            base_url: "https://openrouter.ai/api/v1".into(),
            no_key: false,
            fallback_model: "deepseek/deepseek-chat".into(),
        },
        ProviderPreset {
            id: "ollama".into(),
            name: "Ollama（本地）".into(),
            base_url: "http://localhost:11434/v1".into(),
            no_key: true,
            fallback_model: "qwen3:8b".into(),
        },
        ProviderPreset {
            id: "custom".into(),
            name: "自定义（OpenAI 兼容）".into(),
            base_url: String::new(),
            no_key: false,
            fallback_model: String::new(),
        },
    ]
}

// ---------------------------------------------------------------------------
// 设置模型

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderProfile {
    pub id: String,
    /// 展示名（通常取预设名或用户自定义）
    pub name: String,
    pub base_url: String,
    /// 明文 key，只存本地
    pub api_key: String,
    pub default_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    #[serde(default)]
    pub providers: Vec<ProviderProfile>,
    /// 当前启用的供应商 id
    #[serde(default)]
    pub active_provider: Option<String>,
    /// 按 agent id 覆盖模型（D0-8），回落 active 供应商的 default_model
    #[serde(default)]
    pub agent_overrides: std::collections::HashMap<String, String>,
}

impl AppSettings {
    pub fn active(&self) -> Option<&ProviderProfile> {
        let id = self.active_provider.as_ref()?;
        self.providers.iter().find(|p| &p.id == id)
    }
}

/// 前端可见的供应商视图：key 打码。
#[derive(Debug, Serialize)]
pub struct ProviderView {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub default_model: String,
    pub masked_key: String,
    pub no_key: bool,
}

#[derive(Debug, Serialize)]
pub struct SettingsView {
    pub providers: Vec<ProviderView>,
    pub active_provider: Option<String>,
    pub agent_overrides: std::collections::HashMap<String, String>,
}

fn settings_path() -> Result<PathBuf, String> {
    let base = dirs::config_dir().ok_or("无法定位用户配置目录")?;
    let dir = base.join("type-hime");
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建配置目录失败：{e}"))?;
    Ok(dir.join("settings.json"))
}

pub fn load() -> Result<AppSettings, String> {
    let path = settings_path()?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("读取设置失败：{e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("解析设置失败：{e}"))
}

pub fn save(settings: &AppSettings) -> Result<(), String> {
    let path = settings_path()?;
    let raw = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, raw).map_err(|e| format!("写入设置失败：{e}"))
}

pub fn to_view(s: &AppSettings) -> SettingsView {
    SettingsView {
        providers: s
            .providers
            .iter()
            .map(|p| ProviderView {
                id: p.id.clone(),
                name: p.name.clone(),
                base_url: p.base_url.clone(),
                default_model: p.default_model.clone(),
                masked_key: mask_key(&p.api_key),
                no_key: p.api_key.is_empty(),
            })
            .collect(),
        active_provider: s.active_provider.clone(),
        agent_overrides: s.agent_overrides.clone(),
    }
}

/// 新增或更新供应商；activate 同时设为当前启用。
pub fn upsert_provider(
    mut settings: AppSettings,
    profile: ProviderProfile,
    activate: bool,
) -> Result<AppSettings, String> {
    if profile.base_url.trim().is_empty() {
        return Err("base_url 不能为空".into());
    }
    let key = if profile.api_key.is_empty() {
        // 保留旧 key（编辑时前端不回传）
        settings
            .providers
            .iter()
            .find(|p| p.id == profile.id)
            .map(|p| p.api_key.clone())
            .unwrap_or_default()
    } else {
        profile.api_key
    };
    let merged = ProviderProfile { api_key: key, ..profile };
    match settings.providers.iter().position(|p| p.id == merged.id) {
        Some(i) => settings.providers[i] = merged.clone(),
        None => settings.providers.push(merged.clone()),
    }
    if activate || settings.active_provider.is_none() {
        settings.active_provider = Some(merged.id);
    }
    Ok(settings)
}

pub fn remove_provider(mut settings: AppSettings, id: &str) -> Result<AppSettings, String> {
    settings.providers.retain(|p| p.id != id);
    if settings.active_provider.as_deref() == Some(id) {
        settings.active_provider = settings.providers.first().map(|p| p.id.clone());
    }
    Ok(settings)
}

pub fn set_active(mut settings: AppSettings, id: &str) -> Result<AppSettings, String> {
    if !settings.providers.iter().any(|p| p.id == id) {
        return Err(format!("供应商 {id} 不存在"));
    }
    settings.active_provider = Some(id.to_string());
    Ok(settings)
}

/// key 打码：保留前 3 后 4，中间星号；过短则统一 4 星（不暴露长度）。
fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() < 12 {
        return "****".to_string();
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}***{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_key_rules() {
        assert_eq!(mask_key("sk-1234567890abcdef"), "sk-***cdef");
        assert_eq!(mask_key("short"), "****");
        assert_eq!(mask_key(""), "");
    }

    #[test]
    fn provider_crud_and_active() {
        let s = AppSettings::default();
        let p1 = ProviderProfile {
            id: "deepseek".into(),
            name: "DeepSeek".into(),
            base_url: "https://api.deepseek.com/v1".into(),
            api_key: "sk-1111111111111111111111111111".into(),
            default_model: "deepseek-chat".into(),
        };
        let s = upsert_provider(s, p1.clone(), true).unwrap();
        assert_eq!(s.active().unwrap().id, "deepseek");

        // 编辑保留旧 key
        let p1_edit = ProviderProfile { api_key: String::new(), ..p1.clone() };
        let s = upsert_provider(s, p1_edit, false).unwrap();
        assert_eq!(s.active().unwrap().api_key, "sk-1111111111111111111111111111");

        // 第二个供应商 + 切换 + 删除回落
        let p2 = ProviderProfile {
            id: "kimi".into(),
            name: "Kimi".into(),
            base_url: "https://api.moonshot.cn/v1".into(),
            api_key: "sk-2222222222222222222222222222".into(),
            default_model: "kimi-latest".into(),
        };
        let s = upsert_provider(s, p2, false).unwrap();
        let s = set_active(s, "kimi").unwrap();
        assert_eq!(s.active().unwrap().id, "kimi");
        let s = remove_provider(s, "kimi").unwrap();
        assert_eq!(s.active().unwrap().id, "deepseek");
    }
}
