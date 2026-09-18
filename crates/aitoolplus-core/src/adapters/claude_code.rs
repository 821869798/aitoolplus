//! Claude Code adapter: writes `~/.claude/settings.json`.
//!
//! Ported from ai-toolbox `coding/claude_code/settings_merge.rs` semantics:
//! - `env` is the provider's managed namespace: AUTH_TOKEN/API_KEY/BASE_URL
//!   plus model mappings (`model`->ANTHROPIC_MODEL, haikuModel->…, etc.)
//! - `enabledPlugins`/`extraKnownMarketplaces`/`hooks` are PROTECTED and
//!   never written by provider/common layers
//! - switching providers removes the previous provider's managed env keys
//!   and previous managed top-level fields, then applies the new layers
//! - three merge strategies (provider-overrides-common is the default)

use serde_json::{Map, Value};

use crate::adapters::{
    AppliedReport, ApplyCtx, ApplyError, ToolAdapter, backup_file, write_atomic,
};
use crate::paths::Paths;
use crate::tools::ToolId;

pub struct ClaudeCodeAdapter;

/// Fields never written by provider/common config (owned by plugin system).
pub const PROTECTED_TOP_LEVEL_FIELDS: [&str; 3] =
    ["enabledPlugins", "extraKnownMarketplaces", "hooks"];

/// Provider field -> env var for model selection.
pub const PROVIDER_MODEL_FIELD_MAPPINGS: [(&str, &str); 5] = [
    ("model", "ANTHROPIC_MODEL"),
    ("haikuModel", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
    ("sonnetModel", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
    ("opusModel", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    ("fableModel", "ANTHROPIC_DEFAULT_FABLE_MODEL"),
];

/// Model-name env fields managed alongside the mappings.
pub const PROVIDER_MODEL_NAME_ENV_FIELDS: [&str; 4] = [
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
];

/// Env keys this app manages when applying a provider.
pub const KNOWN_ENV_FIELDS: [&str; 19] = [
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
    "ANTHROPIC_REASONING_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "CUSTOM_HEADERS",
    "ANTHROPIC_CUSTOM_HEADERS",
    "USER_AGENT",
    "ANTHROPIC_USER_AGENT",
    "API_FORMAT",
];

fn is_provider_model_field(key: &str) -> bool {
    PROVIDER_MODEL_FIELD_MAPPINGS
        .iter()
        .any(|(field, _)| *field == key)
        || key == "reasoningModel"
}

fn is_provider_model_env_field(key: &str) -> bool {
    PROVIDER_MODEL_FIELD_MAPPINGS
        .iter()
        .any(|(_, env)| *env == key)
        || PROVIDER_MODEL_NAME_ENV_FIELDS.contains(&key)
}

// -- merge helpers (upstream parity) ---------------------------------------

fn merge_json_preserving(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(t), Value::Object(s)) => {
            for (k, sv) in s {
                match t.get_mut(k) {
                    Some(tv) => merge_json_preserving(tv, sv),
                    None => {
                        t.insert(k.clone(), sv.clone());
                    }
                }
            }
        }
        (t, s) => *t = s.clone(),
    }
}

#[allow(dead_code)] // kept for MergeCommonAndProvider strategy parity
fn merge_json_array_union(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(t), Value::Object(s)) => {
            for (k, sv) in s {
                match t.get_mut(k) {
                    Some(tv) => merge_json_array_union(tv, sv),
                    None => {
                        t.insert(k.clone(), sv.clone());
                    }
                }
            }
        }
        (Value::Array(t), Value::Array(s)) => {
            for item in s {
                if !t.contains(item) {
                    t.push(item.clone());
                }
            }
        }
        (t, s) => *t = s.clone(),
    }
}

#[allow(dead_code)] // used by remove_managed_paths in the full upstream flow
fn json_is_subset(target: &Value, source: &Value) -> bool {
    match source {
        Value::Object(s) => {
            let Some(t) = target.as_object() else {
                return false;
            };
            s.iter()
                .all(|(k, sv)| t.get(k).is_some_and(|tv| json_is_subset(tv, sv)))
        }
        Value::Array(s) => {
            let Some(t) = target.as_array() else {
                return false;
            };
            let mut matched = vec![false; t.len()];
            s.iter().all(|si| {
                t.iter()
                    .enumerate()
                    .find(|(i, ti)| !matched[*i] && json_is_subset(ti, si))
                    .map(|(i, _)| {
                        matched[i] = true;
                        true
                    })
                    .unwrap_or(false)
            })
        }
        _ => target == source,
    }
}

#[allow(dead_code)] // used by strip_claude_common_config in upstream flow
fn json_deep_remove(target: &mut Value, source: &Value) {
    let (Some(t), Some(s)) = (target.as_object_mut(), source.as_object()) else {
        return;
    };
    for (k, sv) in s {
        let mut remove = false;
        if let Some(tv) = t.get_mut(k) {
            if sv.is_object() && tv.is_object() {
                json_deep_remove(tv, sv);
                remove = tv.as_object().is_some_and(|o| o.is_empty());
            } else if let (Some(ta), Some(sa)) = (tv.as_array_mut(), sv.as_array()) {
                for si in sa {
                    if let Some(i) = ta.iter().position(|ti| json_is_subset(ti, si)) {
                        ta.remove(i);
                    }
                }
                remove = ta.is_empty();
            } else if json_is_subset(tv, sv) {
                remove = true;
            }
        }
        if remove {
            t.remove(k);
        }
    }
}

/// Build the provider's managed env from its config (upstream parity).
pub fn build_provider_managed_env(
    provider_config: &Value,
    known_env_fields: &[&str],
) -> Map<String, Value> {
    let mut env = Map::new();

    if let Some(penv) = provider_config.get("env").and_then(Value::as_object) {
        // AUTH_TOKEN wins over API_KEY as the canonical field.
        let api_key = penv
            .get("ANTHROPIC_AUTH_TOKEN")
            .or_else(|| penv.get("ANTHROPIC_API_KEY"));
        if let Some(v) = api_key {
            env.insert("ANTHROPIC_AUTH_TOKEN".into(), v.clone());
        }
        if let Some(v) = penv.get("ANTHROPIC_BASE_URL") {
            env.insert("ANTHROPIC_BASE_URL".into(), v.clone());
        }
        for (k, v) in penv {
            if is_provider_model_env_field(k) {
                env.insert(k.clone(), v.clone());
            }
        }
    }

    // Model fields project into env when not already set.
    for (field, env_field) in PROVIDER_MODEL_FIELD_MAPPINGS {
        if !env.contains_key(env_field)
            && let Some(v) = provider_config.get(field)
        {
            env.insert(env_field.to_string(), v.clone());
        }
    }

    env.retain(|k, v| {
        known_env_fields.contains(&k.as_str())
            && !v.is_null()
            && !v.as_str().is_some_and(str::is_empty)
    });
    env
}

/// Sanitize a common-config layer: strip protected + known env fields
/// (those belong to the provider).
fn sanitize_common(common: &Value, known: &[&str]) -> Result<Map<String, Value>, String> {
    match common {
        Value::Object(o) => {
            let mut m = o.clone();
            for f in PROTECTED_TOP_LEVEL_FIELDS {
                m.remove(f);
            }
            let mut drop_env = false;
            if let Some(Value::Object(e)) = m.get_mut("env") {
                for f in known {
                    e.remove(*f);
                }
                drop_env = e.is_empty();
            }
            if drop_env {
                m.remove("env");
            }
            Ok(m)
        }
        Value::Null => Ok(Map::new()),
        _ => Err("Claude common config must be a JSON object".into()),
    }
}

/// Sanitize an extra-settings layer: strip protected, model fields and
/// known env fields (the provider re-adds its managed env last).
fn sanitize_extra(extra: &Value, known: &[&str]) -> Result<Map<String, Value>, String> {
    match extra {
        Value::Object(o) => {
            let mut m = o.clone();
            for f in PROTECTED_TOP_LEVEL_FIELDS {
                m.remove(f);
            }
            let keys: Vec<String> = m.keys().cloned().collect();
            for k in keys {
                if is_provider_model_field(&k) {
                    m.remove(&k);
                }
            }
            let mut drop_env = false;
            match m.get_mut("env") {
                Some(Value::Object(e)) => {
                    for f in known {
                        e.remove(*f);
                    }
                    if e.is_empty() {
                        drop_env = true;
                    }
                }
                Some(Value::Null) => drop_env = true,
                Some(_) => return Err("Claude extra settings env must be a JSON object".into()),
                None => {}
            }
            if drop_env {
                m.remove("env");
            }
            Ok(m)
        }
        Value::Null => Ok(Map::new()),
        _ => Err("Claude extra settings must be a JSON object".into()),
    }
}

/// The full upstream merge: previous layers are removed, next layers are
/// applied, then the provider's managed env is written.
#[allow(clippy::too_many_arguments)]
pub fn merge_settings_for_provider(
    current_disk: Option<&Value>,
    previous_common: Option<&Value>,
    next_common: &Value,
    previous_extra: Option<&Value>,
    next_extra: Option<&Value>,
    provider_config: &Value,
    known: &[&str],
) -> Result<Value, String> {
    let mut merged = match current_disk {
        Some(Value::Object(o)) => o.clone(),
        Some(_) => return Err("Current Claude settings must be a JSON object".into()),
        None => Map::new(),
    };

    let next_common_obj = sanitize_common(next_common, known)?;
    let previous_common_obj = match previous_common {
        Some(v) => sanitize_common(v, known)?,
        None => next_common_obj.clone(),
    };
    let previous_extra_obj = match previous_extra {
        Some(v) => sanitize_extra(v, known)?,
        None => sanitize_extra(next_extra.unwrap_or(&Value::Object(Map::new())), known)?,
    };
    let next_extra_obj = match next_extra {
        Some(v) => sanitize_extra(v, known)?,
        None => Map::new(),
    };

    // 1) Drop previous common-managed fields the next common no longer has.
    for k in previous_common_obj.keys() {
        if k == "env" {
            continue;
        }
        if !next_common_obj.contains_key(k) {
            merged.remove(k);
        }
    }

    // 2) Drop previous extra-managed top-level fields + their env keys.
    if !previous_extra_obj.is_empty() {
        for k in previous_extra_obj.keys() {
            if k == "env" {
                continue;
            }
            merged.remove(k);
        }
        if let Some(pe) = previous_extra_obj.get("env").and_then(Value::as_object)
            && let Some(Value::Object(te)) = merged.get_mut("env")
        {
            for k in pe.keys() {
                te.remove(k);
            }
            if te.is_empty() {
                merged.remove("env");
            }
        }
    }

    // 3) Apply common layer (recursive, preserving disk values).
    {
        let mut layer = Value::Object(merged.clone());
        merge_json_preserving(&mut layer, &Value::Object(next_common_obj));
        if let Value::Object(o) = layer {
            merged = o;
        }
    }

    // 4) Apply extra layer (top-level replace, env merge).
    for (k, v) in &next_extra_obj {
        if k == "env" {
            match merged.get_mut(k) {
                Some(ev) => merge_json_preserving(ev, v),
                None => {
                    merged.insert(k.clone(), v.clone());
                }
            }
        } else {
            merged.insert(k.clone(), v.clone());
        }
    }

    // 5) Rewrite the managed env namespace.
    let mut env = merged
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    for f in known {
        env.remove(*f);
    }
    for (k, v) in build_provider_managed_env(provider_config, known) {
        env.insert(k, v);
    }
    if env.is_empty() {
        merged.remove("env");
    } else {
        merged.insert("env".into(), Value::Object(env));
    }

    Ok(Value::Object(merged))
}

impl ToolAdapter for ClaudeCodeAdapter {
    fn tool(&self) -> ToolId {
        ToolId::ClaudeCode
    }

    fn apply(&self, ctx: &ApplyCtx) -> Result<AppliedReport, ApplyError> {
        let path = ctx.paths.primary_config(ToolId::ClaudeCode);
        if path.exists() {
            backup_file(ctx.paths, ToolId::ClaudeCode, &path);
        }

        let current_disk: Option<Value> = if path.exists() {
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_default()).ok()
        } else {
            None
        };

        // The previous common config is whatever common layer is stored in
        // the DB today; the provider's own extra layer is its config minus
        // the managed env (approximated by sanitize_extra).
        let next_common: Value =
            serde_json::from_str(ctx.common_config).unwrap_or(Value::Object(Map::new()));
        let provider_config: Value = serde_json::from_str(&ctx.provider.settings_config)
            .unwrap_or(Value::Object(Map::new()));

        let merged = merge_settings_for_provider(
            current_disk.as_ref(),
            None,
            &next_common,
            None,
            None,
            &provider_config,
            &KNOWN_ENV_FIELDS,
        )
        .map_err(ApplyError::Message)?;

        write_atomic(&path, &serde_json::to_string_pretty(&merged).unwrap())?;
        Ok(AppliedReport { files: vec![path] })
    }

    fn prompt_file(&self, paths: &Paths) -> Option<std::path::PathBuf> {
        Some(paths.tool_root(ToolId::ClaudeCode).join("CLAUDE.md"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::MergeStrategy;
    use crate::paths::Paths;
    use crate::providers::ProviderRecord;
    use serde_json::json;

    fn setup() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("home"), dir.path().join("appdata"));
        (dir, paths)
    }

    fn ctx_for<'a>(paths: &'a Paths, p: &'a ProviderRecord) -> ApplyCtx<'a> {
        ApplyCtx {
            paths,
            common_config: "{}",
            provider: p,
            strategy: MergeStrategy::default(),
            provider_optional: false,
        }
    }

    #[test]
    fn provider_env_manages_only_known_fields() {
        let (_dir, paths) = setup();
        let mut p = ProviderRecord::new("MyProxy", "custom");
        p.set_settings(&json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://proxy.example.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-token",
                "UNRELATED_CUSTOM": "keep-me-untouched",
            },
            "model": "my-model",
        }));
        let report = ClaudeCodeAdapter.apply(&ctx_for(&paths, &p)).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&report.files[0]).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://proxy.example.com");
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "sk-token");
        assert_eq!(v["env"]["ANTHROPIC_MODEL"], "my-model");
        // Provider env fields outside KNOWN_ENV_FIELDS are NOT written (upstream
        // semantics: only managed fields land in settings.json).
        assert!(v["env"].get("UNRELATED_CUSTOM").is_none());
    }

    #[test]
    fn switching_providers_replaces_managed_env_only() {
        let (_dir, paths) = setup();
        let settings = paths.home.join(".claude").join("settings.json");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{"env": {"ANTHROPIC_AUTH_TOKEN": "old-key", "ANTHROPIC_BASE_URL": "https://old", "USER_KEEP": "yes"}, "permissions": {"allow": ["Bash"]}}"#,
        )
        .unwrap();

        let mut p2 = ProviderRecord::new("NewProxy", "custom");
        p2.set_settings(&json!({
            "env": {"ANTHROPIC_AUTH_TOKEN": "new-key", "ANTHROPIC_BASE_URL": "https://new"}
        }));
        ClaudeCodeAdapter.apply(&ctx_for(&paths, &p2)).unwrap();

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "new-key");
        assert_eq!(v["env"]["ANTHROPIC_BASE_URL"], "https://new");
        assert_eq!(v["env"]["USER_KEEP"], "yes");
        assert_eq!(v["permissions"]["allow"][0], "Bash");
    }

    #[test]
    fn api_key_falls_back_to_auth_token_slot() {
        let (_dir, paths) = setup();
        let mut p = ProviderRecord::new("K", "custom");
        p.set_settings(&json!({"env": {"ANTHROPIC_API_KEY": "ak-1"}}));
        let report = ClaudeCodeAdapter.apply(&ctx_for(&paths, &p)).unwrap();
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(&report.files[0]).unwrap()).unwrap();
        assert_eq!(v["env"]["ANTHROPIC_AUTH_TOKEN"], "ak-1");
    }

    #[test]
    fn protected_fields_never_touched() {
        let (_dir, paths) = setup();
        let settings = paths.home.join(".claude").join("settings.json");
        std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
        std::fs::write(
            &settings,
            r#"{"enabledPlugins": {"repo": ["plugin1"]}, "hooks": {"x": 1}}"#,
        )
        .unwrap();

        let mut p = ProviderRecord::new("X", "custom");
        p.set_settings(&json!({"env": {"ANTHROPIC_AUTH_TOKEN": "k"}}));
        ClaudeCodeAdapter.apply(&ctx_for(&paths, &p)).unwrap();

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&settings).unwrap()).unwrap();
        assert_eq!(v["enabledPlugins"]["repo"][0], "plugin1");
        assert_eq!(v["hooks"]["x"], 1);
    }

    #[test]
    fn model_field_mappings_build_env() {
        let p = json!({
            "model": "m",
            "haikuModel": "h",
            "sonnetModel": "s",
            "opusModel": "o",
            "fableModel": "f",
        });
        let env = build_provider_managed_env(&p, &KNOWN_ENV_FIELDS);
        assert_eq!(env["ANTHROPIC_MODEL"], "m");
        assert_eq!(env["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "h");
        assert_eq!(env["ANTHROPIC_DEFAULT_SONNET_MODEL"], "s");
        assert_eq!(env["ANTHROPIC_DEFAULT_OPUS_MODEL"], "o");
        assert_eq!(env["ANTHROPIC_DEFAULT_FABLE_MODEL"], "f");
    }

    #[test]
    fn sanitize_common_strips_known_env() {
        let common = json!({
            "env": {"ANTHROPIC_AUTH_TOKEN": "x", "CUSTOM": "y"},
            "hooks": {"bad": true},
            "other": 1,
        });
        let m = sanitize_common(&common, &KNOWN_ENV_FIELDS).unwrap();
        assert!(!m.contains_key("hooks"));
        assert_eq!(m["other"], 1);
        assert_eq!(m["env"]["CUSTOM"], "y");
        assert!(m["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    }

    #[test]
    fn merge_settings_layers_full_flow() {
        let disk = json!({
            "env": {"ANTHROPIC_AUTH_TOKEN": "old", "USER": "1"},
            "statusLine": {"theme": "dark"},
            "extra": "from-extra",
            "toRemove": true,
        });
        let previous_common = json!({"toRemove": true});
        let next_common = json!({"statusLine": {"theme": "dark"}});
        let previous_extra = json!({"extra": "from-extra"});
        let next_extra = json!({"extra": "new-extra"});
        let provider = json!({"env": {"ANTHROPIC_AUTH_TOKEN": "fresh"}});

        let out = merge_settings_for_provider(
            Some(&disk),
            Some(&previous_common),
            &next_common,
            Some(&previous_extra),
            Some(&next_extra),
            &provider,
            &KNOWN_ENV_FIELDS,
        )
        .unwrap();

        assert_eq!(out["env"]["ANTHROPIC_AUTH_TOKEN"], "fresh");
        assert_eq!(out["env"]["USER"], "1");
        assert_eq!(out["extra"], "new-extra");
        assert!(
            out.get("toRemove").is_none(),
            "previous common field dropped"
        );
        assert_eq!(out["statusLine"]["theme"], "dark");
    }
}
