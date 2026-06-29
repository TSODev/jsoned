use anyhow::{anyhow, Context, Result};
use std::path::Path;

pub fn convert_file(input: &Path, to_fmt: &str, output: Option<&Path>) -> Result<()> {
    let src = std::fs::read_to_string(input)
        .with_context(|| format!("cannot read {}", input.display()))?;

    let ext = input.extension().and_then(|e| e.to_str()).unwrap_or("json");
    let value = parse_any(&src, ext)?;

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
    let arr = value.as_array().ok_or_else(|| anyhow!("CSV export requires a JSON array at root"))?;
    if arr.is_empty() {
        return Ok(String::new());
    }
    let headers: Vec<String> = arr[0].as_object()
        .ok_or_else(|| anyhow!("CSV export requires an array of objects"))?
        .keys().cloned().collect();

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(&headers)?;
    for row in arr {
        let obj = row.as_object().ok_or_else(|| anyhow!("each row must be an object"))?;
        let record: Vec<String> = headers.iter()
            .map(|h| obj.get(h).map(|v| value_to_csv_str(v)).unwrap_or_default())
            .collect();
        wtr.write_record(&record)?;
    }
    Ok(String::from_utf8(wtr.into_inner()?)?)
}

fn value_to_csv_str(v: &serde_json::Value) -> String {
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
