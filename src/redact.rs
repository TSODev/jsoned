//! Mask sensitive data before an export (Save-As / headless conversion).
//!
//! Deliberately mask-only, not delete: key names and document shape are preserved so the
//! recipient can still see the schema, just not the real values.
//!
//! Two complementary passes, sharing the same `keys` list:
//! - **Exact key match**: an object key named e.g. `apiKey` has its whole value replaced.
//! - **Inline match**: any string value (regardless of which key holds it — a pagination URL
//!   under `next`/`self` is the motivating case) is scanned for `<name>=<value>` occurrences
//!   (URL-query-string style) and only the value portion is masked, leaving the rest of the
//!   string — and the key name itself — intact.

use regex::Regex;
use serde_json::{Map, Value};
use std::collections::HashSet;

pub const REDACT_MASK: &str = "***REDACTED***";

/// Recursively mask sensitive data matching (case-insensitively) any name in `keys`. Returns a
/// new Value; `value` is never mutated in place.
pub fn redact(value: &Value, keys: &[String]) -> Value {
    if keys.is_empty() {
        return value.clone();
    }
    let target: HashSet<String> = keys.iter().map(|k| k.to_lowercase()).collect();
    let inline = build_inline_pattern(keys);
    redact_value(value, &target, inline.as_ref())
}

/// One combined regex for all target names, e.g. `(?i)\b(api_key|token)=([^&\s]+)` — compiled
/// once per `redact()` call, not once per string, since it's identical for every node visited.
fn build_inline_pattern(keys: &[String]) -> Option<Regex> {
    let alternation = keys.iter().map(|k| regex::escape(k)).collect::<Vec<_>>().join("|");
    Regex::new(&format!(r"(?i)\b({})=([^&\s]+)", alternation)).ok()
}

fn redact_value(value: &Value, target: &HashSet<String>, inline: Option<&Regex>) -> Value {
    match value {
        Value::Object(map) => {
            let out: Map<String, Value> = map
                .iter()
                .map(|(k, v)| {
                    if target.contains(&k.to_lowercase()) {
                        (k.clone(), Value::String(REDACT_MASK.to_string()))
                    } else {
                        (k.clone(), redact_value(v, target, inline))
                    }
                })
                .collect();
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(|v| redact_value(v, target, inline)).collect()),
        Value::String(s) => match inline {
            // Capture group 1 keeps the matched name's original casing as it appeared in the
            // string (the `(?i)` flag only makes matching case-insensitive, not the capture).
            Some(re) => Value::String(re.replace_all(s, format!("$1={}", REDACT_MASK)).into_owned()),
            None => Value::String(s.clone()),
        },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn masks_top_level_key() {
        let v = serde_json::json!({"user": "ada", "password": "hunter2"});
        let out = redact(&v, &keys(&["password"]));
        assert_eq!(out, serde_json::json!({"user": "ada", "password": REDACT_MASK}));
    }

    #[test]
    fn masks_nested_key() {
        let v = serde_json::json!({"user": "ada", "auth": {"apiKey": "sk-real", "note": "x"}});
        let out = redact(&v, &keys(&["apiKey"]));
        assert_eq!(out, serde_json::json!({"user": "ada", "auth": {"apiKey": REDACT_MASK, "note": "x"}}));
    }

    #[test]
    fn masks_key_inside_array_of_objects() {
        let v = serde_json::json!({"users": [{"name": "a", "token": "t1"}, {"name": "b", "token": "t2"}]});
        let out = redact(&v, &keys(&["token"]));
        assert_eq!(out, serde_json::json!({"users": [
            {"name": "a", "token": REDACT_MASK},
            {"name": "b", "token": REDACT_MASK}
        ]}));
    }

    #[test]
    fn case_insensitive_match_same_name() {
        let v = serde_json::json!({"ApiKey": "sk-real"});
        let out = redact(&v, &keys(&["apikey"]));
        assert_eq!(out, serde_json::json!({"ApiKey": REDACT_MASK}));
    }

    #[test]
    fn different_spelling_is_not_matched() {
        // "api_key" and "apiKey" are different strings — case-insensitivity does not fuzz-match
        // across naming conventions, only casing of the exact same name.
        let v = serde_json::json!({"api_key": "sk-real"});
        let out = redact(&v, &keys(&["apiKey"]));
        assert_eq!(out, v);
    }

    #[test]
    fn no_match_leaves_document_unchanged() {
        let v = serde_json::json!({"user": "ada", "age": 36});
        let out = redact(&v, &keys(&["password"]));
        assert_eq!(out, v);
    }

    #[test]
    fn empty_keys_is_a_no_op() {
        let v = serde_json::json!({"password": "hunter2"});
        let out = redact(&v, &[]);
        assert_eq!(out, v);
    }

    #[test]
    fn non_object_root_does_not_panic() {
        let v = serde_json::json!(["a", "b", "c"]);
        let out = redact(&v, &keys(&["password"]));
        assert_eq!(out, v);

        let v = serde_json::json!("just a string");
        let out = redact(&v, &keys(&["password"]));
        assert_eq!(out, v);
    }

    #[test]
    fn masks_query_param_inside_url_keeps_rest_of_string() {
        let v = serde_json::json!({
            "links": {
                "next": "http://api.nasa.gov/neo/rest/v1/neo/browse?page=1&size=20&api_key=MZ9QgVsoWjDUcKJZNWIqdAR4jhahpUmeRrLR8kFI",
                "self": "http://api.nasa.gov/neo/rest/v1/neo/browse?page=0&size=20&api_key=MZ9QgVsoWjDUcKJZNWIqdAR4jhahpUmeRrLR8kFI"
            }
        });
        let out = redact(&v, &keys(&["api_key"]));
        assert_eq!(out, serde_json::json!({
            "links": {
                "next": format!("http://api.nasa.gov/neo/rest/v1/neo/browse?page=1&size=20&api_key={}", REDACT_MASK),
                "self": format!("http://api.nasa.gov/neo/rest/v1/neo/browse?page=0&size=20&api_key={}", REDACT_MASK)
            }
        }));
    }

    #[test]
    fn inline_match_only_masks_matching_param_not_others() {
        let v = serde_json::json!({"url": "http://x.test?page=1&api_key=secret&size=20"});
        let out = redact(&v, &keys(&["api_key"]));
        assert_eq!(out, serde_json::json!({"url": format!("http://x.test?page=1&api_key={}&size=20", REDACT_MASK)}));
    }

    #[test]
    fn inline_match_preserves_original_casing_of_name() {
        let v = serde_json::json!({"url": "http://x.test?API_KEY=secret"});
        let out = redact(&v, &keys(&["api_key"]));
        assert_eq!(out, serde_json::json!({"url": format!("http://x.test?API_KEY={}", REDACT_MASK)}));
    }

    #[test]
    fn inline_match_no_pattern_leaves_string_unchanged() {
        let v = serde_json::json!({"url": "http://x.test?page=1&size=20"});
        let out = redact(&v, &keys(&["api_key"]));
        assert_eq!(out, v);
    }

    #[test]
    fn exact_key_match_still_wins_over_inline_scan() {
        // "apiKey" as an exact object key still gets fully masked, not just its value scanned.
        let v = serde_json::json!({"apiKey": "raw-secret-no-equals-sign"});
        let out = redact(&v, &keys(&["apiKey"]));
        assert_eq!(out, serde_json::json!({"apiKey": REDACT_MASK}));
    }
}
