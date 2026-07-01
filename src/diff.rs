//! Structural, key-path-based diff between two `JNode` trees.
//!
//! Array comparison is naive index-alignment (no LCS / reordering-aware diff) — a v1
//! limitation. Inserting an element at the front of an array will show every later
//! element as "changed" rather than as a single insertion.

use anyhow::{anyhow, Context, Result};
use std::path::Path;

use crate::convert::parse_any;
use crate::tree::{path_to_string, JKey, JNode, JPath, JScalar};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffStatus {
    Added,
    Removed,
    Changed,
    Unchanged,
}

#[derive(Debug, Clone)]
pub struct DiffRow {
    pub depth: usize,
    pub key: Option<String>,
    pub index: Option<usize>,
    pub path: JPath,
    pub status: DiffStatus,
    pub type_label: String,
    pub old_value: Option<String>,
    pub new_value: Option<String>,
}

pub fn diff(a: Option<&JNode>, b: Option<&JNode>) -> Vec<DiffRow> {
    let mut rows = Vec::new();
    diff_node(a, b, 0, None, None, &[], &mut rows);
    rows
}

fn diff_node(
    a: Option<&JNode>,
    b: Option<&JNode>,
    depth: usize,
    key: Option<String>,
    index: Option<usize>,
    path: &[JKey],
    out: &mut Vec<DiffRow>,
) -> DiffStatus {
    match (a, b) {
        (None, None) => unreachable!("diff_node called with both sides absent"),
        (None, Some(present)) => {
            push_one_sided(present, DiffStatus::Added, depth, key, index, path, out);
            DiffStatus::Added
        }
        (Some(present), None) => {
            push_one_sided(present, DiffStatus::Removed, depth, key, index, path, out);
            DiffStatus::Removed
        }
        (Some(a_node), Some(b_node)) => {
            if same_shape(a_node, b_node) {
                match (a_node, b_node) {
                    (JNode::Scalar(sa), JNode::Scalar(sb)) => {
                        let status = if sa == sb { DiffStatus::Unchanged } else { DiffStatus::Changed };
                        out.push(DiffRow {
                            depth,
                            key,
                            index,
                            path: path.to_vec(),
                            status,
                            type_label: node_type_label(a_node),
                            old_value: Some(scalar_str(sa)),
                            new_value: Some(scalar_str(sb)),
                        });
                        status
                    }
                    (JNode::Object { entries: ea, .. }, JNode::Object { entries: eb, .. }) => {
                        let self_idx = out.len();
                        out.push(DiffRow {
                            depth,
                            key,
                            index,
                            path: path.to_vec(),
                            status: DiffStatus::Unchanged,
                            type_label: node_type_label(a_node),
                            old_value: Some(node_summary(a_node)),
                            new_value: Some(node_summary(b_node)),
                        });

                        let mut agg = DiffStatus::Unchanged;
                        let mut seen: Vec<&String> = Vec::new();
                        for (k, a_child) in ea.iter() {
                            seen.push(k);
                            let mut child_path = path.to_vec();
                            child_path.push(JKey::Field(k.clone()));
                            let b_child = eb.get(k);
                            let child_status = diff_node(
                                Some(a_child), b_child, depth + 1, Some(k.clone()), None, &child_path, out,
                            );
                            agg = fold(agg, child_status);
                        }
                        for (k, b_child) in eb.iter() {
                            if seen.contains(&k) {
                                continue;
                            }
                            let mut child_path = path.to_vec();
                            child_path.push(JKey::Field(k.clone()));
                            let child_status = diff_node(
                                None, Some(b_child), depth + 1, Some(k.clone()), None, &child_path, out,
                            );
                            agg = fold(agg, child_status);
                        }

                        out[self_idx].status = agg;
                        agg
                    }
                    (JNode::Array { items: ia, .. }, JNode::Array { items: ib, .. }) => {
                        let self_idx = out.len();
                        out.push(DiffRow {
                            depth,
                            key,
                            index,
                            path: path.to_vec(),
                            status: DiffStatus::Unchanged,
                            type_label: node_type_label(a_node),
                            old_value: Some(node_summary(a_node)),
                            new_value: Some(node_summary(b_node)),
                        });

                        let mut agg = DiffStatus::Unchanged;
                        let max_len = ia.len().max(ib.len());
                        for i in 0..max_len {
                            let mut child_path = path.to_vec();
                            child_path.push(JKey::Index(i));
                            let child_status = diff_node(
                                ia.get(i), ib.get(i), depth + 1, None, Some(i), &child_path, out,
                            );
                            agg = fold(agg, child_status);
                        }

                        out[self_idx].status = agg;
                        agg
                    }
                    _ => unreachable!("same_shape guarantees matching variants"),
                }
            } else {
                out.push(DiffRow {
                    depth,
                    key,
                    index,
                    path: path.to_vec(),
                    status: DiffStatus::Changed,
                    type_label: format!("{} → {}", node_type_label(a_node), node_type_label(b_node)),
                    old_value: Some(node_summary(a_node)),
                    new_value: Some(node_summary(b_node)),
                });
                DiffStatus::Changed
            }
        }
    }
}

fn push_one_sided(
    node: &JNode,
    status: DiffStatus,
    depth: usize,
    key: Option<String>,
    index: Option<usize>,
    path: &[JKey],
    out: &mut Vec<DiffRow>,
) {
    let (old_value, new_value) = match status {
        DiffStatus::Added => (None, Some(node_summary(node))),
        DiffStatus::Removed => (Some(node_summary(node)), None),
        _ => unreachable!("push_one_sided only used for Added/Removed"),
    };
    out.push(DiffRow {
        depth,
        key,
        index,
        path: path.to_vec(),
        status,
        type_label: node_type_label(node),
        old_value,
        new_value,
    });

    match node {
        JNode::Object { entries, .. } => {
            for (k, child) in entries.iter() {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Field(k.clone()));
                push_one_sided(child, status, depth + 1, Some(k.clone()), None, &child_path, out);
            }
        }
        JNode::Array { items, .. } => {
            for (i, child) in items.iter().enumerate() {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Index(i));
                push_one_sided(child, status, depth + 1, None, Some(i), &child_path, out);
            }
        }
        JNode::Scalar(_) => {}
    }
}

fn fold(acc: DiffStatus, child: DiffStatus) -> DiffStatus {
    match child {
        DiffStatus::Unchanged => acc,
        _ => DiffStatus::Changed,
    }
}

fn same_shape(a: &JNode, b: &JNode) -> bool {
    matches!(
        (a, b),
        (JNode::Object { .. }, JNode::Object { .. })
            | (JNode::Array { .. }, JNode::Array { .. })
            | (JNode::Scalar(_), JNode::Scalar(_))
    )
}

fn scalar_str(s: &JScalar) -> String {
    match s {
        JScalar::Null => "null".to_string(),
        JScalar::Bool(b) => b.to_string(),
        JScalar::Number(n) => n.clone(),
        JScalar::String(s) => s.clone(),
    }
}

fn node_type_label(node: &JNode) -> String {
    match node {
        JNode::Object { .. } => "Object".to_string(),
        JNode::Array { items, .. } => format!("Array ({})", items.len()),
        JNode::Scalar(JScalar::String(_)) => "String".to_string(),
        JNode::Scalar(JScalar::Number(_)) => "Number".to_string(),
        JNode::Scalar(JScalar::Bool(_)) => "Bool".to_string(),
        JNode::Scalar(JScalar::Null) => "Null".to_string(),
    }
}

fn node_summary(node: &JNode) -> String {
    match node {
        JNode::Object { entries, .. } => format!("{} items", entries.len()),
        JNode::Array { items, .. } => format!("{} items", items.len()),
        JNode::Scalar(s) => scalar_str(s),
    }
}

pub(crate) fn load_node(path: &Path) -> Result<JNode> {
    let src = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    let value = parse_any(&src, ext)?;
    Ok(JNode::from_value(value))
}

pub fn diff_file_headless(a: &Path, b: &Path, fmt: &str, output: Option<&Path>) -> Result<()> {
    let root_a = load_node(a)?;
    let root_b = load_node(b)?;
    let rows = diff(Some(&root_a), Some(&root_b));

    let out = match fmt {
        "text" => render_text(&rows),
        "json" => render_json(&rows)?,
        other => return Err(anyhow!("--diff --to only supports 'text' or 'json' (got '{}')", other)),
    };

    match output {
        Some(path) => std::fs::write(path, out)
            .with_context(|| format!("cannot write {}", path.display()))?,
        None => print!("{}", out),
    }
    Ok(())
}

fn render_text(rows: &[DiffRow]) -> String {
    rows.iter()
        .filter(|r| r.status != DiffStatus::Unchanged)
        .map(|r| {
            let p = path_to_string(&r.path);
            match r.status {
                DiffStatus::Added => format!("+ {}: {}\n", p, r.new_value.as_deref().unwrap_or("")),
                DiffStatus::Removed => format!("- {}: {}\n", p, r.old_value.as_deref().unwrap_or("")),
                DiffStatus::Changed => format!(
                    "~ {}: {} -> {}\n",
                    p,
                    r.old_value.as_deref().unwrap_or(""),
                    r.new_value.as_deref().unwrap_or("")
                ),
                DiffStatus::Unchanged => unreachable!(),
            }
        })
        .collect()
}

fn render_json(rows: &[DiffRow]) -> Result<String> {
    let entries: Vec<serde_json::Value> = rows
        .iter()
        .filter(|r| r.status != DiffStatus::Unchanged)
        .map(|r| {
            let status_str = match r.status {
                DiffStatus::Added => "added",
                DiffStatus::Removed => "removed",
                DiffStatus::Changed => "changed",
                DiffStatus::Unchanged => "unchanged",
            };
            serde_json::json!({
                "path": path_to_string(&r.path),
                "status": status_str,
                "old": r.old_value,
                "new": r.new_value,
            })
        })
        .collect();
    Ok(serde_json::to_string_pretty(&entries)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(v: serde_json::Value) -> JNode {
        JNode::from_value(v)
    }

    fn row_at<'a>(rows: &'a [DiffRow], path: &str) -> Option<&'a DiffRow> {
        rows.iter().find(|r| path_to_string(&r.path) == path)
    }

    #[test]
    fn added_key() {
        let a = node(serde_json::json!({"x": 1}));
        let b = node(serde_json::json!({"x": 1, "y": 2}));
        let rows = diff(Some(&a), Some(&b));

        let y = row_at(&rows, "y").unwrap();
        assert_eq!(y.status, DiffStatus::Added);

        let root = row_at(&rows, "").unwrap();
        assert_eq!(root.status, DiffStatus::Changed);
    }

    #[test]
    fn removed_key() {
        let a = node(serde_json::json!({"x": 1, "y": 2}));
        let b = node(serde_json::json!({"x": 1}));
        let rows = diff(Some(&a), Some(&b));

        let y = row_at(&rows, "y").unwrap();
        assert_eq!(y.status, DiffStatus::Removed);

        let root = row_at(&rows, "").unwrap();
        assert_eq!(root.status, DiffStatus::Changed);
    }

    #[test]
    fn changed_scalar() {
        let a = node(serde_json::json!({"x": 1}));
        let b = node(serde_json::json!({"x": 2}));
        let rows = diff(Some(&a), Some(&b));

        let x = row_at(&rows, "x").unwrap();
        assert_eq!(x.status, DiffStatus::Changed);
        assert_eq!(x.old_value.as_deref(), Some("1"));
        assert_eq!(x.new_value.as_deref(), Some("2"));
    }

    #[test]
    fn unchanged_nested_object() {
        let a = node(serde_json::json!({"n": {"a": 1, "b": 2}}));
        let b = node(serde_json::json!({"n": {"a": 1, "b": 2}}));
        let rows = diff(Some(&a), Some(&b));

        for r in &rows {
            assert_eq!(r.status, DiffStatus::Unchanged, "row {} should be Unchanged", path_to_string(&r.path));
        }
    }

    #[test]
    fn array_length_difference() {
        let a = node(serde_json::json!([1, 2]));
        let b = node(serde_json::json!([1, 2, 3]));
        let rows = diff(Some(&a), Some(&b));

        let idx0 = row_at(&rows, "0").unwrap();
        assert_eq!(idx0.status, DiffStatus::Unchanged);
        let idx1 = row_at(&rows, "1").unwrap();
        assert_eq!(idx1.status, DiffStatus::Unchanged);
        let idx2 = row_at(&rows, "2").unwrap();
        assert_eq!(idx2.status, DiffStatus::Added);

        let root = row_at(&rows, "").unwrap();
        assert_eq!(root.status, DiffStatus::Changed);
    }

    #[test]
    fn type_mismatch_same_path() {
        let a = node(serde_json::json!({"x": "hello"}));
        let b = node(serde_json::json!({"x": {"y": 1}}));
        let rows = diff(Some(&a), Some(&b));

        let x = row_at(&rows, "x").unwrap();
        assert_eq!(x.status, DiffStatus::Changed);

        assert!(rows.iter().all(|r| path_to_string(&r.path) != "x.y"));
    }

    #[test]
    fn root_type_mismatch() {
        let a = node(serde_json::json!("hello"));
        let b = node(serde_json::json!({"y": 1}));
        let rows = diff(Some(&a), Some(&b));

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].status, DiffStatus::Changed);
    }
}
