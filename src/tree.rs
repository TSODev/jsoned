use indexmap::IndexMap;

/// Core mutable tree — the editing model.
/// Deliberately separate from serde_json::Value so we can track
/// cursor position, collapsed state, and undo history independently.
#[derive(Debug, Clone, PartialEq)]
pub enum JNode {
    Object { entries: IndexMap<String, JNode>, collapsed: bool },
    Array  { items: Vec<JNode>, collapsed: bool },
    Scalar(JScalar),
}

#[derive(Debug, Clone, PartialEq)]
pub enum JScalar {
    Null,
    Bool(bool),
    Number(String),   // kept as raw string to preserve formatting (1.0 vs 1)
    String(String),
}

/// A path segment into the tree
#[derive(Debug, Clone, PartialEq)]
pub enum JKey {
    Field(String),
    Index(usize),
}

/// Cursor = path from root to current node
pub type JPath = Vec<JKey>;

impl JNode {
    pub fn from_value(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => JNode::Scalar(JScalar::Null),
            serde_json::Value::Bool(b) => JNode::Scalar(JScalar::Bool(b)),
            serde_json::Value::Number(n) => JNode::Scalar(JScalar::Number(n.to_string())),
            serde_json::Value::String(s) => JNode::Scalar(JScalar::String(s)),
            serde_json::Value::Array(arr) => JNode::Array {
                items: arr.into_iter().map(JNode::from_value).collect(),
                collapsed: false,
            },
            serde_json::Value::Object(map) => JNode::Object {
                entries: map.into_iter().map(|(k, v)| (k, JNode::from_value(v))).collect(),
                collapsed: false,
            },
        }
    }

    pub fn to_value(&self) -> serde_json::Value {
        match self {
            JNode::Scalar(JScalar::Null) => serde_json::Value::Null,
            JNode::Scalar(JScalar::Bool(b)) => serde_json::Value::Bool(*b),
            JNode::Scalar(JScalar::Number(s)) => {
                serde_json::from_str(s).unwrap_or(serde_json::Value::String(s.clone()))
            }
            JNode::Scalar(JScalar::String(s)) => serde_json::Value::String(s.clone()),
            JNode::Array { items, .. } => {
                serde_json::Value::Array(items.iter().map(|n| n.to_value()).collect())
            }
            JNode::Object { entries, .. } => {
                let map: serde_json::Map<String, serde_json::Value> =
                    entries.iter().map(|(k, v)| (k.clone(), v.to_value())).collect();
                serde_json::Value::Object(map)
            }
        }
    }

    pub fn is_collapsed(&self) -> bool {
        match self {
            JNode::Object { collapsed, .. } | JNode::Array { collapsed, .. } => *collapsed,
            JNode::Scalar(_) => false,
        }
    }

}

/// Flat row produced by walking the tree for rendering
#[derive(Debug, Clone)]
pub struct FlatRow {
    pub depth: usize,
    pub key: Option<String>,     // None for array elements shown inline
    pub index: Option<usize>,    // set for array elements
    pub node: JNode,
    pub path: JPath,
}

pub fn get_node_at_path<'a>(root: &'a JNode, path: &[JKey]) -> Option<&'a JNode> {
    if path.is_empty() {
        return Some(root);
    }
    match root {
        JNode::Object { entries, .. } => {
            if let JKey::Field(k) = &path[0] {
                entries.get(k).and_then(|c| get_node_at_path(c, &path[1..]))
            } else {
                None
            }
        }
        JNode::Array { items, .. } => {
            if let JKey::Index(i) = path[0] {
                items.get(i).and_then(|c| get_node_at_path(c, &path[1..]))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn get_node_at_path_mut<'a>(root: &'a mut JNode, path: &[JKey]) -> Option<&'a mut JNode> {
    if path.is_empty() {
        return Some(root);
    }
    let (first, rest) = path.split_first()?;
    match root {
        JNode::Object { entries, .. } => {
            if let JKey::Field(k) = first {
                entries.get_mut(k.as_str()).and_then(|c| get_node_at_path_mut(c, rest))
            } else {
                None
            }
        }
        JNode::Array { items, .. } => {
            if let JKey::Index(i) = first {
                items.get_mut(*i).and_then(|c| get_node_at_path_mut(c, rest))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub fn set_node_at_path(root: &mut JNode, path: &[JKey], new_node: JNode) {
    if path.is_empty() {
        *root = new_node;
        return;
    }
    match root {
        JNode::Object { entries, .. } => {
            if let JKey::Field(k) = &path[0] {
                if let Some(child) = entries.get_mut(k) {
                    set_node_at_path(child, &path[1..], new_node);
                }
            }
        }
        JNode::Array { items, .. } => {
            if let JKey::Index(i) = path[0] {
                if let Some(child) = items.get_mut(i) {
                    set_node_at_path(child, &path[1..], new_node);
                }
            }
        }
        _ => {}
    }
}

pub fn flatten(root: &JNode) -> Vec<FlatRow> {
    let mut rows = Vec::new();
    flatten_node(root, 0, None, None, &[], &mut rows);
    rows
}

fn flatten_node(
    node: &JNode,
    depth: usize,
    key: Option<String>,
    index: Option<usize>,
    path: &[JKey],
    out: &mut Vec<FlatRow>,
) {
    out.push(FlatRow { depth, key: key.clone(), index, node: node.clone(), path: path.to_vec() });

    if node.is_collapsed() {
        return;
    }

    match node {
        JNode::Object { entries, .. } => {
            for (k, child) in entries.iter() {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Field(k.clone()));
                flatten_node(child, depth + 1, Some(k.clone()), None, &child_path, out);
            }
        }
        JNode::Array { items, .. } => {
            for (i, child) in items.iter().enumerate() {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Index(i));
                flatten_node(child, depth + 1, None, Some(i), &child_path, out);
            }
        }
        JNode::Scalar(_) => {}
    }
}
