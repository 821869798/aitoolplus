//! The config merge engine: how common config + provider settings become the
//! final payload written to a tool's config file.
//!
//! Strategies mirror ai-toolbox's Claude settings merge semantics.

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MergeStrategy {
    /// Provider settings override same-name top-level common fields
    /// (ai-toolbox historical default).
    #[default]
    ProviderOverridesCommon,
    /// Common config overlays the provider settings.
    CommonOverridesProvider,
    /// Recursive merge of both layers; arrays replaced by provider's.
    MergeCommonAndProvider,
}

/// Deep-merge `over` onto `base`: objects merge recursively, every other
/// value type is replaced by `over`.
pub fn deep_merge(base: &Value, over: &Value) -> Value {
    match (base, over) {
        (Value::Object(b), Value::Object(o)) => {
            let mut out = b.clone();
            for (k, v) in o {
                out.insert(
                    k.clone(),
                    match out.get(k) {
                        Some(existing) => deep_merge(existing, v),
                        None => v.clone(),
                    },
                );
            }
            Value::Object(out)
        }
        (_, o) => o.clone(),
    }
}

/// Produce the final merged config for a tool from its two JSON layers.
/// Invalid JSON layers are skipped (empty object).
pub fn merged_config(common: &str, provider_settings: &str, strategy: MergeStrategy) -> Value {
    let common: Value =
        serde_json::from_str(common).unwrap_or_else(|_| Value::Object(Default::default()));
    let provider: Value = serde_json::from_str(provider_settings)
        .unwrap_or_else(|_| Value::Object(Default::default()));

    match strategy {
        MergeStrategy::ProviderOverridesCommon => deep_merge(&common, &provider),
        MergeStrategy::CommonOverridesProvider => deep_merge(&provider, &common),
        MergeStrategy::MergeCommonAndProvider => deep_merge(&common, &provider),
    }
}

/// JSON merge object: read existing raw text (may be invalid/missing) and
/// merge the new object over it, returning pretty JSON text.
pub fn merge_into_raw(raw: &str, new_layer: &Value) -> String {
    let existing: Value =
        serde_json::from_str(raw).unwrap_or_else(|_| Value::Object(Default::default()));
    let merged = deep_merge(&existing, new_layer);
    serde_json::to_string_pretty(&merged).unwrap_or_else(|_| raw.to_string())
}

/// .env file editing: set/update keys, keep unrelated lines and comments.
pub fn env_upsert(raw: &str, key_values: &[(String, String)]) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = Default::default();

    for line in raw.lines() {
        let trimmed = line.trim();
        if let Some(eq) = trimmed.find('=') {
            let key = trimmed[..eq].trim();
            if let Some((k, v)) = key_values.iter().find(|(k, _)| k == key) {
                if seen.insert(k.clone()) {
                    kept.push(format!("{k}={v}"));
                }
                continue;
            }
        }
        kept.push(line.to_string());
    }
    for (k, v) in key_values {
        if seen.insert(k.clone()) {
            kept.push(format!("{k}={v}"));
        }
    }
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

/// Descend into (creating as needed) a nested table path like
/// "model_providers.my-provider", returning the innermost table.
fn table_or_create<'a>(root: &'a mut toml_edit::Table, path: &str) -> &'a mut toml_edit::Table {
    let mut cur = root;
    for part in path.split('.').filter(|p| !p.is_empty()) {
        let item = cur
            .entry(part)
            .or_insert_with(|| toml_edit::Item::Table(toml_edit::Table::new()));
        // If the existing entry is not a table (e.g. an inline value), replace it.
        if item.is_table_like() {
            cur = item.as_table_mut().unwrap();
        } else {
            *item = toml_edit::Item::Table(toml_edit::Table::new());
            cur = item.as_table_mut().unwrap();
        }
    }
    cur
}

/// TOML editing helper: read existing config text, set `key = value` at the
/// document root or inside a table, preserving everything else. `value` is a
/// TOML literal (e.g. `"https://api.x.ai/v1"`, `true`, `42`).
pub fn toml_set(raw: &str, table: Option<&str>, key: &str, value: &str) -> String {
    let mut doc = raw
        .parse::<toml_edit::DocumentMut>()
        .unwrap_or_else(|_| toml_edit::DocumentMut::new());

    if let Ok(v) = value.parse::<toml_edit::Value>() {
        match table {
            Some(t) => {
                table_or_create(doc.as_table_mut(), t).insert(key, toml_edit::Item::Value(v));
            }
            None => {
                doc.as_table_mut().insert(key, toml_edit::Item::Value(v));
            }
        }
    }
    doc.to_string()
}

/// Merge a JSON object into a TOML document recursively. Arrays/scalars
/// replace; protected top-level keys are ignored so specialized writers keep
/// ownership of provider/MCP namespaces.
pub fn merge_json_into_toml(
    document: &mut toml_edit::DocumentMut,
    json: &Value,
    protected_top_level: &[&str],
) -> Result<(), String> {
    let Some(object) = json.as_object() else {
        return Err("common config must be a JSON object".into());
    };
    for (key, value) in object {
        if protected_top_level.contains(&key.as_str()) {
            continue;
        }
        merge_json_toml_table(document.as_table_mut(), key, value)?;
    }
    Ok(())
}

fn merge_json_toml_table(
    table: &mut toml_edit::Table,
    key: &str,
    value: &Value,
) -> Result<(), String> {
    if let Value::Object(object) = value {
        if !table.contains_key(key) || table.get(key).and_then(|item| item.as_table()).is_none() {
            table.insert(key, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let target = table
            .get_mut(key)
            .and_then(|item| item.as_table_mut())
            .ok_or_else(|| format!("{key} is not a TOML table"))?;
        for (child, value) in object {
            merge_json_toml_table(target, child, value)?;
        }
    } else {
        let wrapped = serde_json::json!({"value": value});
        let serialized = toml::to_string(&wrapped).map_err(|error| error.to_string())?;
        let mut parsed = serialized
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| error.to_string())?;
        let item = parsed
            .remove("value")
            .ok_or_else(|| format!("could not serialize {key}"))?;
        table.insert(key, item);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deep_merge_objects() {
        let base = json!({"a": 1, "env": {"X": "1", "Y": "2"}});
        let over = json!({"env": {"X": "9", "Z": "3"}, "b": true});
        let m = deep_merge(&base, &over);
        assert_eq!(m["a"], 1);
        assert_eq!(m["b"], true);
        assert_eq!(m["env"]["X"], "9");
        assert_eq!(m["env"]["Y"], "2");
        assert_eq!(m["env"]["Z"], "3");
    }

    #[test]
    fn deep_merge_replaces_scalars_and_arrays() {
        let base = json!({"s": "old", "arr": [1, 2]});
        let over = json!({"s": "new", "arr": [3]});
        let m = deep_merge(&base, &over);
        assert_eq!(m["s"], "new");
        assert_eq!(m["arr"], json!([3]));
    }

    #[test]
    fn strategies() {
        let common = r#"{"top": 1, "x": "c"}"#;
        let provider = r#"{"top": 2, "y": "p"}"#;
        let m = merged_config(common, provider, MergeStrategy::ProviderOverridesCommon);
        assert_eq!(m["top"], 2);
        assert_eq!(m["x"], "c");
        let m = merged_config(common, provider, MergeStrategy::CommonOverridesProvider);
        assert_eq!(m["top"], 1);
        assert_eq!(m["y"], "p");
        let m = merged_config(common, provider, MergeStrategy::MergeCommonAndProvider);
        assert_eq!(m["top"], 2);
    }

    #[test]
    fn invalid_layers_degrade_to_empty() {
        let m = merged_config("{bad", "{alsobad", MergeStrategy::default());
        assert!(m.as_object().unwrap().is_empty());
    }

    #[test]
    fn merge_into_raw_keeps_unrelated() {
        let raw = r#"{"keep": 1, "env": {"A": "1"}}"#;
        let out = merge_into_raw(raw, &json!({"env": {"B": "2"}}));
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["keep"], 1);
        assert_eq!(v["env"]["A"], "1");
        assert_eq!(v["env"]["B"], "2");
    }

    #[test]
    fn env_upsert_preserves_and_replaces() {
        let raw = "# comment\nFOO=old\nBAR=1\n";
        let out = env_upsert(
            raw,
            &[("FOO".into(), "new".into()), ("BAZ".into(), "3".into())],
        );
        assert!(out.contains("# comment"));
        assert!(out.contains("FOO=new"));
        assert!(!out.contains("FOO=old"));
        assert!(out.contains("BAR=1"));
        assert!(out.contains("BAZ=3"));
    }

    #[test]
    fn env_upsert_from_empty() {
        let out = env_upsert("", &[("K".into(), "v".into())]);
        assert_eq!(out.trim(), "K=v");
    }

    #[test]
    fn toml_set_root_and_table() {
        let raw = "model = \"gpt-4o\"\n\n[profile]\nname = \"x\"\n";
        let out = toml_set(raw, None, "model", "\"grok-4\"");
        assert!(out.contains("model = \"grok-4\""));
        let out = toml_set(&out, Some("mcp_servers.fs"), "command", "\"npx\"");
        assert!(out.contains("[mcp_servers.fs]"));
        assert!(out.contains("command = \"npx\""));
    }
}
