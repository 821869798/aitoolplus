//! Provider presets: pre-configured templates where the user only supplies
//! the API key (cc-switch parity).
//!
//! Data is ported from cc-switch's `src/config/*ProviderPresets.ts` — the
//! real base URLs and model ids vendors expect. Affiliate codes are stripped
//! from URLs; we keep the plain vendor sites.

use serde_json::{Map, Value};

use crate::tools::ToolId;

/// A preset provider template.
#[derive(Debug, Clone)]
pub struct ProviderPreset {
    pub name: &'static str,
    pub category: &'static str,
    /// Vendor website (affiliate codes stripped).
    pub website_url: &'static str,
    /// Which env field the user's key goes into (Claude presets).
    pub api_key_field: &'static str,
    /// Pre-built settings_config JSON for the tool.
    pub settings: Value,
    /// Extra key->value env entries applied over `settings` on select.
    pub extra_env: &'static [(&'static str, &'static str)],
}

impl ProviderPreset {
    /// Build the final settings JSON with the user's key injected.
    pub fn settings_with_key(&self, api_key: &str) -> Value {
        let mut out = self.settings.clone();
        // Claude/Gemini presets inject into env.
        if let Some(env) = out.get_mut("env").and_then(Value::as_object_mut) {
            env.insert(
                self.api_key_field.to_string(),
                Value::String(api_key.to_string()),
            );
            for (k, v) in self.extra_env {
                env.insert(k.to_string(), Value::String(v.to_string()));
            }
            return out;
        }
        // Codex presets carry a TOML document. Replace the placeholder key,
        // or append api_key to the selected provider table.
        if let Some(toml) = out.get("toml").and_then(Value::as_str) {
            let mut text = toml.to_string();
            if text.contains("api_key = \"\"") {
                text = text.replace("api_key = \"\"", &format!("api_key = \"{api_key}\""));
            } else if !api_key.is_empty() {
                text.push_str(&format!("api_key = \"{api_key}\"\n"));
            }
            if let Some(obj) = out.as_object_mut() {
                obj.insert("toml".into(), Value::String(text));
            }
        }
        out
    }
}

/// Build `{ "env": { ... } }` for Claude-style presets.
fn s(env_pairs: &[(&str, &str)]) -> Value {
    let mut env = Map::new();
    for (k, v) in env_pairs {
        env.insert(k.to_string(), Value::String(v.to_string()));
    }
    serde_json::json!({ "env": Value::Object(env) })
}

/// Claude Code presets (cc-switch parity, top vendors).
pub fn claude_code_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            name: "Kimi (Moonshot)",
            category: "official",
            website_url: "https://platform.moonshot.cn",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://api.moonshot.cn/anthropic")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "kimi-k2.7-code"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "kimi-k2.7-code"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "kimi-k2.7-code"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "kimi-k2.7-code"),
            ],
        },
        ProviderPreset {
            name: "Kimi For Coding",
            category: "official",
            website_url: "https://www.kimi.com/code/",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://api.kimi.com/coding/")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "kimi-for-coding"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "kimi-for-coding"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "kimi-for-coding"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "kimi-for-coding"),
                ("CLAUDE_CODE_MAX_CONTEXT_TOKENS", "262144"),
                ("CLAUDE_CODE_AUTO_COMPACT_WINDOW", "262144"),
            ],
        },
        ProviderPreset {
            name: "PackyCode",
            category: "custom",
            website_url: "https://www.packyapi.ai",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://www.packyapi.ai")]),
            extra_env: &[],
        },
        ProviderPreset {
            name: "DeepSeek",
            category: "official",
            website_url: "https://platform.deepseek.com",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://api.deepseek.com/anthropic")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "deepseek-v4-pro"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "deepseek-v4-flash"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "deepseek-v4-pro"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "deepseek-v4-pro"),
            ],
        },
        ProviderPreset {
            name: "智谱 GLM",
            category: "official",
            website_url: "https://open.bigmodel.cn",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://open.bigmodel.cn/api/anthropic",
            )]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "glm-4.7"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "glm-4.7-flash"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "glm-4.7"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "glm-4.7"),
            ],
        },
        ProviderPreset {
            name: "智谱 Z.AI (国际版)",
            category: "official",
            website_url: "https://z.ai",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://api.z.ai/api/anthropic")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "glm-4.7"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "glm-4.7-flash"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "glm-4.7"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "glm-4.7"),
            ],
        },
        ProviderPreset {
            name: "百炼 (Qwen)",
            category: "official",
            website_url: "https://www.aliyun.com/product/bailian",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://dashscope.aliyuncs.com/api/v2/anthropic",
            )]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "qwen3-coder-plus"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "qwen3-coder-flash"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "qwen3-coder-plus"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "qwen3-coder-plus"),
            ],
        },
        ProviderPreset {
            name: "火山引擎 Agent Plan",
            category: "subscription",
            website_url: "https://www.volcengine.com/product/ark",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://ark.cn-beijing.volces.com/api/plan",
            )]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "ark-code-latest"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "ark-code-latest"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "ark-code-latest"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "ark-code-latest"),
            ],
        },
        ProviderPreset {
            name: "火山引擎 Coding Plan",
            category: "subscription",
            website_url: "https://www.volcengine.com/product/ark",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://ark.cn-beijing.volces.com/api/coding",
            )]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "ark-code-latest"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "ark-code-latest"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "ark-code-latest"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "ark-code-latest"),
            ],
        },
        ProviderPreset {
            name: "豆包 Seed",
            category: "official",
            website_url: "https://console.volcengine.com/ark",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[
                (
                    "ANTHROPIC_BASE_URL",
                    "https://ark.cn-beijing.volces.com/api/compatible",
                ),
                ("API_TIMEOUT_MS", "3000000"),
            ]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "doubao-seed-2-1-pro-260628"),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                    "doubao-seed-2-1-pro-260628",
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL",
                    "doubao-seed-2-1-pro-260628",
                ),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "doubao-seed-2-1-pro-260628"),
            ],
        },
        ProviderPreset {
            name: "SiliconFlow 硅基流动",
            category: "custom",
            website_url: "https://siliconflow.cn",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://api.siliconflow.cn")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "Pro/MiniMaxAI/MiniMax-M2.5"),
                (
                    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
                    "Pro/MiniMaxAI/MiniMax-M2.5",
                ),
                (
                    "ANTHROPIC_DEFAULT_SONNET_MODEL",
                    "Pro/MiniMaxAI/MiniMax-M2.5",
                ),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "Pro/MiniMaxAI/MiniMax-M2.5"),
            ],
        },
        ProviderPreset {
            name: "MiniMax",
            category: "official",
            website_url: "https://platform.minimaxi.com",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://api.minimaxi.com/anthropic")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "MiniMax-M2.5"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "MiniMax-M2.5"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "MiniMax-M2.5"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "MiniMax-M2.5"),
            ],
        },
        ProviderPreset {
            name: "OpenRouter",
            category: "custom",
            website_url: "https://openrouter.ai",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://openrouter.ai/api")]),
            extra_env: &[],
        },
        ProviderPreset {
            name: "ModelScope 魔搭",
            category: "official",
            website_url: "https://modelscope.cn",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://api-inference.modelscope.cn/v1/anthropic",
            )]),
            extra_env: &[],
        },
        ProviderPreset {
            name: "OpenCode Go",
            category: "custom",
            website_url: "https://opencode.ai/go",
            // Go gateway only accepts x-api-key — must use ANTHROPIC_API_KEY
            api_key_field: "ANTHROPIC_API_KEY",
            settings: s(&[("ANTHROPIC_BASE_URL", "https://opencode.ai/zen/go")]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "deepseek-v4-flash"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "deepseek-v4-flash"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "deepseek-v4-flash"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "deepseek-v4-flash"),
            ],
        },
        ProviderPreset {
            name: "Tencent Token Plan",
            category: "subscription",
            website_url: "https://cloud.tencent.com/product/tokenhub",
            api_key_field: "ANTHROPIC_AUTH_TOKEN",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://api.lkeap.cloud.tencent.com/plan/anthropic",
            )]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "tc-code-latest"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "tc-code-latest"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "tc-code-latest"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "tc-code-latest"),
            ],
        },
        ProviderPreset {
            name: "Gemini Native",
            category: "custom",
            website_url: "https://ai.google.dev/gemini-api",
            api_key_field: "ANTHROPIC_API_KEY",
            settings: s(&[(
                "ANTHROPIC_BASE_URL",
                "https://generativelanguage.googleapis.com",
            )]),
            extra_env: &[
                ("ANTHROPIC_MODEL", "gemini-3.6-flash"),
                ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "gemini-3.6-flash"),
                ("ANTHROPIC_DEFAULT_SONNET_MODEL", "gemini-3.6-flash"),
                ("ANTHROPIC_DEFAULT_OPUS_MODEL", "gemini-3.6-flash"),
            ],
        },
    ]
}

/// Codex presets (cc-switch parity).
pub fn codex_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            name: "OpenAI 官方",
            category: "official",
            website_url: "https://platform.openai.com",
            api_key_field: "OPENAI_API_KEY",
            settings: serde_json::json!({
                "toml": "model_provider = \"openai\"\n\n[model_providers.openai]\nname = \"OpenAI\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n"
            }),
            extra_env: &[],
        },
        ProviderPreset {
            name: "PackyCode",
            category: "custom",
            website_url: "https://www.packyapi.ai",
            api_key_field: "OPENAI_API_KEY",
            settings: serde_json::json!({
                "toml": "model_provider = \"packycode\"\n\n[model_providers.packycode]\nname = \"PackyCode\"\nbase_url = \"https://www.packyapi.ai/v1\"\nwire_api = \"chat\"\napi_key = \"\"\n"
            }),
            extra_env: &[],
        },
        ProviderPreset {
            name: "AiHubMix",
            category: "custom",
            website_url: "https://aihubmix.com",
            api_key_field: "OPENAI_API_KEY",
            settings: serde_json::json!({
                "toml": "model_provider = \"aihubmix\"\n\n[model_providers.aihubmix]\nname = \"AiHubMix\"\nbase_url = \"https://aihubmix.com/v1\"\nwire_api = \"chat\"\napi_key = \"\"\n"
            }),
            extra_env: &[],
        },
        ProviderPreset {
            name: "DeepSeek",
            category: "official",
            website_url: "https://platform.deepseek.com",
            api_key_field: "OPENAI_API_KEY",
            settings: serde_json::json!({
                "toml": "model_provider = \"deepseek\"\n\n[model_providers.deepseek]\nname = \"DeepSeek\"\nbase_url = \"https://api.deepseek.com/v1\"\nwire_api = \"chat\"\napi_key = \"\"\nmodel = \"deepseek-chat\"\n"
            }),
            extra_env: &[],
        },
        ProviderPreset {
            name: "智谱 GLM",
            category: "official",
            website_url: "https://open.bigmodel.cn",
            api_key_field: "OPENAI_API_KEY",
            settings: serde_json::json!({
                "toml": "model_provider = \"zhipu\"\n\n[model_providers.zhipu]\nname = \"Zhipu\"\nbase_url = \"https://open.bigmodel.cn/api/coding/paas/v4\"\nwire_api = \"chat\"\napi_key = \"\"\nmodel = \"glm-4.7\"\n"
            }),
            extra_env: &[],
        },
        ProviderPreset {
            name: "Kimi (Moonshot)",
            category: "official",
            website_url: "https://platform.moonshot.cn",
            api_key_field: "OPENAI_API_KEY",
            settings: serde_json::json!({
                "toml": "model_provider = \"kimi\"\n\n[model_providers.kimi]\nname = \"Kimi\"\nbase_url = \"https://api.moonshot.cn/v1\"\nwire_api = \"chat\"\napi_key = \"\"\nmodel = \"kimi-k2.7\"\n"
            }),
            extra_env: &[],
        },
    ]
}

/// Gemini CLI presets.
pub fn gemini_presets() -> Vec<ProviderPreset> {
    vec![ProviderPreset {
        name: "Google AI Studio (API Key)",
        category: "official",
        website_url: "https://aistudio.google.com/app/apikey",
        api_key_field: "GEMINI_API_KEY",
        settings: serde_json::json!({
            "env": { "GEMINI_API_KEY": "" },
            "security": { "auth": { "selectedType": "gemini-api-key" } }
        }),
        extra_env: &[],
    }]
}

/// All presets for a tool.
pub fn presets_for(tool: ToolId) -> Vec<ProviderPreset> {
    match tool {
        ToolId::ClaudeCode => claude_code_presets(),
        ToolId::Codex => codex_presets(),
        ToolId::GeminiCli => gemini_presets(),
        _ => vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_presets_have_valid_shapes() {
        let presets = claude_code_presets();
        assert!(presets.len() >= 14, "expected a cc-switch-sized list");
        for p in &presets {
            assert!(!p.name.is_empty());
            assert!(p.website_url.starts_with("https://"));
            assert!(p.settings.get("env").is_some(), "{} missing env", p.name);
            assert!(
                p.api_key_field == "ANTHROPIC_AUTH_TOKEN" || p.api_key_field == "ANTHROPIC_API_KEY",
                "{} bad key field",
                p.name
            );
        }
        // key vendors present with the right endpoints
        let kimi = presets
            .iter()
            .find(|p| p.name == "Kimi (Moonshot)")
            .unwrap();
        assert_eq!(
            kimi.settings["env"]["ANTHROPIC_BASE_URL"],
            "https://api.moonshot.cn/anthropic"
        );
        let opencode_go = presets.iter().find(|p| p.name == "OpenCode Go").unwrap();
        assert_eq!(opencode_go.api_key_field, "ANTHROPIC_API_KEY");
    }

    #[test]
    fn key_injection_and_model_envs() {
        let presets = claude_code_presets();
        let kimi = presets
            .iter()
            .find(|p| p.name == "Kimi For Coding")
            .unwrap();
        let with_key = kimi.settings_with_key("sk-user-key");
        assert_eq!(with_key["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-user-key");
        assert_eq!(with_key["env"]["ANTHROPIC_MODEL"], "kimi-for-coding");
        assert_eq!(with_key["env"]["CLAUDE_CODE_MAX_CONTEXT_TOKENS"], "262144");
    }

    #[test]
    fn codex_presets_are_toml_shaped() {
        let presets = codex_presets();
        for p in &presets {
            assert!(
                p.settings.get("toml").is_some(),
                "{} must carry TOML",
                p.name
            );
            let toml = p.settings["toml"].as_str().unwrap();
            assert!(
                toml.contains("model_provider"),
                "{} missing selector",
                p.name
            );
            assert!(
                toml.contains("[model_providers."),
                "{} missing table",
                p.name
            );
        }
        let keyed = presets[1].settings_with_key("sk-live");
        assert!(
            keyed["toml"]
                .as_str()
                .unwrap()
                .contains("api_key = \"sk-live\"")
        );
    }

    #[test]
    fn presets_router() {
        assert!(!presets_for(ToolId::ClaudeCode).is_empty());
        assert!(!presets_for(ToolId::Codex).is_empty());
        assert_eq!(presets_for(ToolId::GeminiCli).len(), 1);
        assert!(
            presets_for(ToolId::Pi).is_empty(),
            "Pi discovers from runtime instead"
        );
    }
}
