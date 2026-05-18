//! Locale bundle loader (Slice 5).
//!
//! Frontends use `slab_plugins_load_locale_bundle(plugin_id, locale)`
//! to fetch the flat `{ "key": "translated" }` JSON shape that the
//! Slab i18n layer already consumes for built-in locales. The loader
//! delegates path resolution to [`crate::plugins::contributions::read_asset`]
//! so the path-traversal guard is inherited — the only added concern
//! here is JSON shape validation.
//!
//! Rejected:
//! - Non-string values (numbers, arrays, nested objects) — the
//!   frontend resolver expects strings.
//! - Malformed JSON.

use crate::plugins::contributions::read_asset;
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Read a locale bundle file under `plugin_dir` and return a flat
/// `key -> translation` map. Errors are human-readable strings (the
/// Tauri command surface wants `Result<_, String>`).
pub fn load_locale_bundle(
    plugin_dir: &Path,
    relative: &str,
) -> Result<HashMap<String, String>, String> {
    let raw = read_asset(plugin_dir, relative)?;
    let parsed: Value =
        serde_json::from_str(&raw).map_err(|e| format!("locale bundle is not valid JSON: {e}"))?;
    let obj = match parsed {
        Value::Object(m) => m,
        other => {
            return Err(format!(
                "locale bundle must be a JSON object at the top level, got {}",
                json_kind(&other)
            ))
        }
    };
    let mut out = HashMap::with_capacity(obj.len());
    for (k, v) in obj {
        match v {
            Value::String(s) => {
                out.insert(k, s);
            }
            other => {
                return Err(format!(
                    "locale bundle value for key {k:?} must be a string, got {}",
                    json_kind(&other)
                ))
            }
        }
    }
    Ok(out)
}

fn json_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, rel: &str, body: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, body).unwrap();
    }

    #[test]
    fn loads_flat_string_object() {
        let tmp = TempDir::new().unwrap();
        write(
            tmp.path(),
            "locales/ja.json",
            r#"{"hello": "こんにちは", "save": "保存"}"#,
        );
        let map = load_locale_bundle(tmp.path(), "locales/ja.json").unwrap();
        assert_eq!(map.get("hello").map(String::as_str), Some("こんにちは"));
        assert_eq!(map.get("save").map(String::as_str), Some("保存"));
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn empty_object_is_ok() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "empty.json", "{}");
        let map = load_locale_bundle(tmp.path(), "empty.json").unwrap();
        assert!(map.is_empty());
    }

    #[test]
    fn rejects_non_object_top_level() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "arr.json", r#"["a","b"]"#);
        let err = load_locale_bundle(tmp.path(), "arr.json").unwrap_err();
        assert!(err.contains("object at the top level"), "got: {err}");
    }

    #[test]
    fn rejects_nested_object_value() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "nested.json", r#"{"hello":{"x":"y"}}"#);
        let err = load_locale_bundle(tmp.path(), "nested.json").unwrap_err();
        assert!(err.contains("hello"), "got: {err}");
        assert!(err.contains("string"), "got: {err}");
    }

    #[test]
    fn rejects_number_value() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "nums.json", r#"{"count":5}"#);
        let err = load_locale_bundle(tmp.path(), "nums.json").unwrap_err();
        assert!(err.contains("count"), "got: {err}");
    }

    #[test]
    fn rejects_array_value() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "a.json", r#"{"items":["a","b"]}"#);
        let err = load_locale_bundle(tmp.path(), "a.json").unwrap_err();
        assert!(err.contains("items"), "got: {err}");
        assert!(err.contains("array"), "got: {err}");
    }

    #[test]
    fn rejects_invalid_json() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "bad.json", "{not valid");
        let err = load_locale_bundle(tmp.path(), "bad.json").unwrap_err();
        assert!(err.contains("valid JSON"), "got: {err}");
    }

    #[test]
    fn inherits_path_traversal_guard() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join("plug");
        // valid bundle inside the plugin dir
        write(&plugin_dir, "ok.json", "{}");
        // forbidden secret outside
        write(tmp.path(), "secret.json", r#"{"k":"v"}"#);
        let err = load_locale_bundle(&plugin_dir, "../secret.json").unwrap_err();
        // Either "escapes plugin dir" (after canonicalize succeeds) or
        // "asset not found" — both prevent the read.
        assert!(
            err.contains("escapes") || err.contains("not found"),
            "expected traversal rejection, got: {err}"
        );
    }

    #[test]
    fn rejects_absolute_path() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "ok.json", "{}");
        let err = load_locale_bundle(tmp.path(), "/etc/passwd").unwrap_err();
        assert!(err.contains("relative"), "got: {err}");
    }

    #[test]
    fn missing_file_returns_error() {
        let tmp = TempDir::new().unwrap();
        let err = load_locale_bundle(tmp.path(), "missing.json").unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }
}
