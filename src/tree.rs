use indexmap::IndexMap;
use std::rc::Rc;

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
    Field(Rc<str>),
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
#[derive(Debug, Clone, PartialEq)]
pub struct FlatRow {
    pub depth: usize,
    pub key: Option<String>,     // None for array elements shown inline
    pub index: Option<usize>,    // set for array elements
    pub path: JPath,
}

/// Describes the contiguous range that was replaced by a `patch_flat`/`patch_annotated` call.
/// The two vectors being spliced (`FlatRow`/`AnnotatedLine`) have different lengths for the same
/// edit, so callers get one `PatchSpan` back from each call — they are not interchangeable.
#[derive(Debug, Clone, Copy)]
pub struct PatchSpan {
    pub start: usize,
    pub new_len: usize,
}

impl PatchSpan {
    pub fn new_end(&self) -> usize { self.start + self.new_len }
}

pub fn path_to_string(path: &[JKey]) -> String {
    path.iter().map(|k| match k {
        JKey::Field(s) => s.to_string(),
        JKey::Index(i) => i.to_string(),
    }).collect::<Vec<_>>().join(".")
}

pub fn get_node_at_path<'a>(root: &'a JNode, path: &[JKey]) -> Option<&'a JNode> {
    if path.is_empty() {
        return Some(root);
    }
    match root {
        JNode::Object { entries, .. } => {
            if let JKey::Field(k) = &path[0] {
                entries.get(k.as_ref()).and_then(|c| get_node_at_path(c, &path[1..]))
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
                entries.get_mut(k.as_ref()).and_then(|c| get_node_at_path_mut(c, rest))
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
                if let Some(child) = entries.get_mut(k.as_ref()) {
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
    out.push(FlatRow { depth, key: key.clone(), index, path: path.to_vec() });

    if node.is_collapsed() {
        return;
    }

    match node {
        JNode::Object { entries, .. } => {
            for (k, child) in entries.iter() {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Field(Rc::from(k.as_str())));
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

/// Replace the contiguous block of `flat` belonging to `target` (its own row + every visible
/// descendant) with a freshly-flattened version of the current subtree at `target` in `root`.
/// Everything outside that block is left completely untouched — no clone, no re-walk.
///
/// Returns `None` (leaving `flat` unmodified) if `target` isn't found in `flat`/`root` — both
/// defensive, should never happen for callers that pass a path known to exist, but no panics per
/// this codebase's convention. Caller must fall back to a full `flatten()` in that case.
pub fn patch_flat(flat: &mut Vec<FlatRow>, root: &JNode, target: &JPath) -> Option<PatchSpan> {
    // This position() scan is the one remaining O(N) cost — but it's pure path-equality
    // comparison, no cloning/formatting, far cheaper than a full flatten()+annotate().
    let start = flat.iter().position(|r| r.path == *target)?;
    let mut end = start + 1;
    while end < flat.len() && flat[end].path.starts_with(target.as_slice()) {
        end += 1;
    }

    let node = get_node_at_path(root, target)?;
    let (depth, key, index) = target_identity(target);
    let mut new_rows = Vec::new();
    flatten_node(node, depth, key, index, target, &mut new_rows);

    let new_len = new_rows.len();
    flat.splice(start..end, new_rows);
    Some(PatchSpan { start, new_len })
}

/// Derive the (depth, key, index) triple that `flatten_node`'s caller would have passed in for
/// the node living at `target`, purely from `target`'s own last path segment (mirrors exactly
/// what the recursive match arms in `flatten_node` compute when descending into a child).
fn target_identity(target: &JPath) -> (usize, Option<String>, Option<usize>) {
    match target.last() {
        Some(JKey::Field(k)) => (target.len(), Some(k.to_string()), None),
        Some(JKey::Index(i)) => (target.len(), None, Some(*i)),
        None => (0, None, None), // target == root path []
    }
}

#[cfg(test)]
mod patch_tests {
    use super::*;

    /// ~4 levels deep, mixes Object/Array, multi-child siblings, an empty container.
    fn sample_tree() -> JNode {
        JNode::from_value(serde_json::json!({
            "meta": { "name": "demo", "tags": ["x", "y", "z"] },
            "items": [
                { "id": 1, "nested": { "a": 1, "b": 2 } },
                { "id": 2, "nested": { "a": 3, "b": 4 } },
                { "id": 3, "nested": {} }
            ],
            "empty_obj": {},
            "flag": true
        }))
    }

    fn field(name: &str) -> JKey {
        JKey::Field(Rc::from(name))
    }

    fn assert_patch_matches_full(root: &JNode, flat: &mut Vec<FlatRow>, target: &JPath) {
        let span = patch_flat(flat, root, target).expect("patch should find target");
        assert_eq!(*flat, flatten(root), "patched flat differs from full flatten()");
        assert_eq!(flat[span.start].path, *target, "target row not at span.start");
    }

    #[test]
    fn replace_scalar_first_middle_last_of_array() {
        for idx in [0usize, 1, 2] {
            let mut root = sample_tree();
            let mut flat = flatten(&root);
            let target: JPath = vec![field("meta"), field("tags"), JKey::Index(idx)];
            set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::String("changed".into())));
            assert_patch_matches_full(&root, &mut flat, &target);
        }
    }

    #[test]
    fn replace_scalar_with_object_grows_subtree() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("flag")];
        set_node_at_path(&mut root, &target, JNode::from_value(serde_json::json!({"a": 1, "b": 2})));
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn replace_object_with_scalar_shrinks_subtree() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta")];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::String("gone".into())));
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn delete_first_key_of_object() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_remove("name");
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn delete_last_item_of_array() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta"), field("tags")];
        if let JNode::Array { items, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            items.pop();
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn delete_root_level_key() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_remove("items");
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn append_item_to_array() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta"), field("tags")];
        if let JNode::Array { items, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            items.push(JNode::Scalar(JScalar::String("w".into())));
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn insert_key_into_middle_of_object() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_insert(1, "extra".to_string(), JNode::Scalar(JScalar::Bool(true)));
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn insert_first_key_into_previously_empty_object() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("empty_obj")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert("first".to_string(), JNode::Scalar(JScalar::Number("1".into())));
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn swap_adjacent_object_keys() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.swap_indices(0, 1);
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn swap_adjacent_array_items() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta"), field("tags")];
        if let JNode::Array { items, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            items.swap(0, 1);
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn rename_key_in_place_same_index() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            let (idx, _, val) = entries.shift_remove_full("name").unwrap();
            entries.shift_insert(idx, "full_name".to_string(), val);
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn root_as_target() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert("new_top_level".to_string(), JNode::Scalar(JScalar::Bool(false)));
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn deep_target_depth_four() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("items"), JKey::Index(1), field("nested"), field("a")];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::Number("999".into())));
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn collapse_toggle_hides_subtree() {
        let mut root = sample_tree();
        let mut flat = flatten(&root);
        let target: JPath = vec![field("items"), JKey::Index(0)];
        if let JNode::Object { collapsed, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            *collapsed = true;
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn expand_toggle_reveals_subtree() {
        let mut root = sample_tree();
        // pre-collapse before taking the "old" flat snapshot, then expand.
        if let JNode::Object { collapsed, .. } =
            get_node_at_path_mut(&mut root, &[field("items"), JKey::Index(0)]).unwrap()
        {
            *collapsed = true;
        }
        let mut flat = flatten(&root);
        let target: JPath = vec![field("items"), JKey::Index(0)];
        if let JNode::Object { collapsed, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            *collapsed = false;
        }
        assert_patch_matches_full(&root, &mut flat, &target);
    }

    #[test]
    fn patch_flat_returns_none_for_nonexistent_path() {
        let root = sample_tree();
        let mut flat = flatten(&root);
        let bogus: JPath = vec![field("does_not_exist")];
        assert!(patch_flat(&mut flat, &root, &bogus).is_none());
    }

    #[test]
    fn target_identity_of_root_path() {
        let root_path: JPath = vec![];
        assert_eq!(target_identity(&root_path), (0, None, None));
    }
}
