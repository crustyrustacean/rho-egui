// src/util/json.rs

use serde_json::Value;

pub(crate) fn jstr(value: Option<&Value>, key: &str) -> String {
    value
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .to_string()
}

pub(crate) fn ju64(value: Option<&Value>, key: &str) -> u64 {
    value
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_u64())
        .unwrap_or(0)
}

pub(crate) fn jf64(value: Option<&Value>, key: &str) -> f64 {
    value
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_f64())
        .unwrap_or(0.0)
}

pub(crate) fn jbool(value: Option<&Value>, key: &str) -> bool {
    value
        .and_then(|value| value.get(key))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn jstr_extracts_string() {
        let value = json!({ "name": "cargo_check" });
        assert_eq!(jstr(Some(&value), "name"), "cargo_check");
    }

    #[test]
    fn jstr_returns_empty_for_missing_key() {
        let value = json!({});
        assert_eq!(jstr(Some(&value), "missing"), "");
    }

    #[test]
    fn ju64_extracts_number() {
        let value = json!({ "count": 1234 });
        assert_eq!(ju64(Some(&value), "count"), 1234);
    }

    #[test]
    fn jf64_extracts_float() {
        let value = json!({ "cost": 0.0042 });
        assert!((jf64(Some(&value), "cost") - 0.0042).abs() < 1e-9);
    }

    #[test]
    fn jbool_extracts_boolean() {
        let value = json!({ "is_error": true });
        assert!(jbool(Some(&value), "is_error"));
    }
}
