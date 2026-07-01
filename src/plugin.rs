use anyhow::{anyhow, Result};

use crate::tree::JNode;

/// A transformation that can be invoked on the selected node.
///
/// The trait is deliberately narrow — `JNode in, string argument, JNode out` — because
/// that's the shape `jq` needs. A plugin whose input/output isn't a JSON node (codegen,
/// a web import) will need the trait to grow; don't widen it before that plugin exists.
pub trait Plugin {
    fn name(&self) -> &'static str;
    /// Label shown above the argument input (e.g. "jq filter:").
    fn prompt(&self) -> &'static str;
    fn run(&self, input: &JNode, arg: &str) -> Result<JNode>;
}

pub fn registry() -> Vec<Box<dyn Plugin>> {
    vec![Box::new(JqPlugin)]
}

pub struct JqPlugin;

impl Plugin for JqPlugin {
    fn name(&self) -> &'static str {
        "jq"
    }

    fn prompt(&self) -> &'static str {
        "jq filter:"
    }

    fn run(&self, input: &JNode, arg: &str) -> Result<JNode> {
        run_jq(input, arg)
    }
}

fn run_jq(input: &JNode, expr: &str) -> Result<JNode> {
    use jaq_core::load::{Arena, File, Loader};
    use jaq_core::{data, Ctx, Vars};
    use jaq_json::Val;

    let defs = jaq_core::defs().chain(jaq_std::defs()).chain(jaq_json::defs());
    let funs = jaq_core::funs().chain(jaq_std::funs()).chain(jaq_json::funs());

    let program = File { code: expr, path: () };
    let loader = Loader::new(defs);
    let arena = Arena::default();

    let modules = loader
        .load(&arena, program)
        .map_err(|e| anyhow!("jq parse error: {:?}", e))?;

    let filter = jaq_core::Compiler::default()
        .with_funs(funs)
        .compile(modules)
        .map_err(|e| anyhow!("jq compile error: {:?}", e))?;

    let json_text = serde_json::to_string(&input.to_value())?;
    let val = jaq_json::read::parse_single(json_text.as_bytes())
        .map_err(|e| anyhow!("jq input error: {e}"))?;

    let ctx = Ctx::<data::JustLut<Val>>::new(&filter.lut, Vars::new([]));
    let outputs: Vec<Val> = filter
        .id
        .run((ctx, val))
        .map(jaq_core::unwrap_valr)
        .collect::<Result<_, _>>()
        .map_err(|e: jaq_core::Error<Val>| anyhow!("jq runtime error: {e}"))?;

    let values: Vec<serde_json::Value> = outputs
        .iter()
        .map(|v| serde_json::from_str(&v.to_string()))
        .collect::<Result<_, _>>()?;

    let result = match values.len() {
        1 => values.into_iter().next().unwrap(),
        _ => serde_json::Value::Array(values),
    };

    Ok(JNode::from_value(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::JScalar;

    fn sample() -> JNode {
        JNode::from_value(serde_json::json!({
            "users": [
                {"name": "Ada", "age": 36},
                {"name": "Grace", "age": 85}
            ]
        }))
    }

    #[test]
    fn selects_field() {
        let out = run_jq(&sample(), ".users[0].name").unwrap();
        assert_eq!(out, JNode::Scalar(JScalar::String("Ada".to_string())));
    }

    #[test]
    fn maps_multiple_outputs_into_array() {
        let out = run_jq(&sample(), ".users[].name").unwrap();
        assert_eq!(out.to_value(), serde_json::json!(["Ada", "Grace"]));
    }

    #[test]
    fn empty_stream_becomes_empty_array() {
        let out = run_jq(&sample(), ".users[] | select(.age > 200)").unwrap();
        assert_eq!(out.to_value(), serde_json::json!([]));
    }

    #[test]
    fn invalid_expr_is_an_error() {
        assert!(run_jq(&sample(), ".users[").is_err());
    }
}
