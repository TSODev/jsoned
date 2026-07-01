use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use std::path::Path;

pub fn convert_file(input: &Path, to_fmt: &str, output: Option<&Path>, redact_keys: &[String]) -> Result<()> {
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;

    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("json");
    let value = parse_any(&src, ext)?;
    let value = crate::redact::redact(&value, redact_keys);

    let out = serialize_to(&value, to_fmt)?;

    match output {
        Some(path) => std::fs::write(path, out).with_context(|| format!("cannot write {}", path.display()))?,
        None => print!("{}", out),
    }
    Ok(())
}

pub fn parse_any(src: &str, hint: &str) -> Result<serde_json::Value> {
    match hint {
        "yaml" | "yml" => {
            let v: serde_json::Value = serde_yaml::from_str(src).context("invalid YAML")?;
            Ok(v)
        }
        "toml" => {
            let t: toml::Value = toml::from_str(src).context("invalid TOML")?;
            let j = toml_to_json(t);
            Ok(j)
        }
        "csv" => csv_to_json(src),
        _ => serde_json::from_str(src).context("invalid JSON"),
    }
}

pub fn serialize_to(value: &serde_json::Value, fmt: &str) -> Result<String> {
    match fmt {
        "json" => Ok(serde_json::to_string_pretty(value)?),
        "yaml" | "yml" => Ok(serde_yaml::to_string(value)?),
        "toml" => {
            let t = json_to_toml(value)?;
            toml::to_string_pretty(&t).context("TOML serialization failed")
        }
        "csv" => json_to_csv(value),
        _ => Err(anyhow!("unsupported format: {} (supported: json, yaml, toml, csv)", fmt)),
    }
}

fn csv_to_json(src: &str) -> Result<serde_json::Value> {
    let mut rdr = csv::Reader::from_reader(src.as_bytes());
    let headers: Vec<String> = rdr.headers()?.iter().map(|s| s.to_string()).collect();
    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result?;
        let obj: serde_json::Map<String, serde_json::Value> = headers.iter()
            .zip(record.iter())
            .map(|(h, v)| (h.clone(), serde_json::Value::String(v.to_string())))
            .collect();
        rows.push(serde_json::Value::Object(obj));
    }
    Ok(serde_json::Value::Array(rows))
}

fn json_to_csv(value: &serde_json::Value) -> Result<String> {
    let roots: Vec<&serde_json::Value> = match value {
        serde_json::Value::Array(arr) => arr.iter().collect(),
        obj @ serde_json::Value::Object(_) => vec![obj],
        _ => return Err(anyhow!("CSV export requires a JSON object or array at root")),
    };

    if roots.is_empty() {
        return Ok(String::new());
    }

    let mut flat_rows: Vec<IndexMap<String, String>> = Vec::new();
    for root in roots {
        flat_rows.extend(flatten_to_rows(root, "", true));
    }

    if flat_rows.is_empty() {
        return Ok(String::new());
    }

    // Collect headers in first-seen order across all rows
    let mut headers: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for row in &flat_rows {
        for key in row.keys() {
            if seen.insert(key.clone()) {
                headers.push(key.clone());
            }
        }
    }

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(&headers)?;
    for row in &flat_rows {
        let record: Vec<&str> = headers.iter()
            .map(|h| row.get(h).map(|s| s.as_str()).unwrap_or(""))
            .collect();
        wtr.write_record(&record)?;
    }
    Ok(String::from_utf8(wtr.into_inner()?)?)
}

// Flatten one JSON value into one or more CSV rows.
// allow_explode=true: the first array-of-objects field is expanded into N rows.
// allow_explode=false: arrays are serialized into a single cell (used for nested levels).
fn flatten_to_rows(value: &serde_json::Value, prefix: &str, allow_explode: bool) -> Vec<IndexMap<String, String>> {
    let obj = match value.as_object() {
        Some(o) => o,
        None => {
            let mut row = IndexMap::new();
            let key = if prefix.is_empty() { "value".to_string() } else { prefix.to_string() };
            row.insert(key, scalar_to_str(value));
            return vec![row];
        }
    };

    let mut base: IndexMap<String, String> = IndexMap::new();
    let mut explode_key: Option<String> = None;
    let mut explode_items: Vec<&serde_json::Value> = Vec::new();

    for (key, val) in obj {
        let full_key = if prefix.is_empty() { key.clone() } else { format!("{}.{}", prefix, key) };

        match val {
            serde_json::Value::Null          => { base.insert(full_key, String::new()); }
            serde_json::Value::Bool(b)       => { base.insert(full_key, b.to_string()); }
            serde_json::Value::Number(n)     => { base.insert(full_key, n.to_string()); }
            serde_json::Value::String(s)     => { base.insert(full_key, s.clone()); }
            serde_json::Value::Object(_)     => {
                let sub = flatten_to_rows(val, &full_key, false);
                if let Some(row) = sub.into_iter().next() {
                    base.extend(row);
                }
            }
            serde_json::Value::Array(arr)    => {
                if allow_explode && explode_key.is_none()
                    && !arr.is_empty()
                    && arr.iter().all(|v| v.is_object())
                {
                    explode_key = Some(full_key);
                    explode_items = arr.iter().collect();
                } else {
                    base.insert(full_key, serialize_array_cell(arr));
                }
            }
        }
    }

    if let Some(exp_key) = explode_key {
        let mut result = Vec::new();
        for item in explode_items {
            for sub_row in flatten_to_rows(item, &exp_key, false) {
                let mut row = base.clone();
                row.extend(sub_row);
                result.push(row);
            }
        }
        result
    } else {
        vec![base]
    }
}

fn serialize_array_cell(arr: &[serde_json::Value]) -> String {
    if arr.iter().all(|v| !v.is_object() && !v.is_array()) {
        arr.iter().map(scalar_to_str).collect::<Vec<_>>().join(";")
    } else {
        serde_json::to_string(arr).unwrap_or_default()
    }
}

fn scalar_to_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn toml_to_json(t: toml::Value) -> serde_json::Value {
    match t {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Datetime(d) => serde_json::Value::String(d.to_string()),
        toml::Value::Array(arr) => serde_json::Value::Array(arr.into_iter().map(toml_to_json).collect()),
        toml::Value::Table(t) => {
            let map = t.into_iter().map(|(k, v)| (k, toml_to_json(v))).collect();
            serde_json::Value::Object(map)
        }
    }
}

fn json_to_toml(v: &serde_json::Value) -> Result<toml::Value> {
    match v {
        serde_json::Value::Null => Err(anyhow!("TOML does not support null values")),
        serde_json::Value::Bool(b) => Ok(toml::Value::Boolean(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() { Ok(toml::Value::Integer(i)) }
            else if let Some(f) = n.as_f64() { Ok(toml::Value::Float(f)) }
            else { Err(anyhow!("cannot convert number to TOML")) }
        }
        serde_json::Value::String(s) => Ok(toml::Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<_>> = arr.iter().map(json_to_toml).collect();
            Ok(toml::Value::Array(items?))
        }
        serde_json::Value::Object(map) => {
            let table: Result<toml::map::Map<_, _>> = map.iter()
                .map(|(k, v)| json_to_toml(v).map(|tv| (k.clone(), tv)))
                .collect();
            Ok(toml::Value::Table(table?))
        }
    }
}
