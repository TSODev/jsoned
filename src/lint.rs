use crate::tree::{get_node_at_path, JKey, JNode, JPath};
use std::rc::Rc;

const MAX_DEPTH: usize = 20;

#[derive(Debug, Clone, PartialEq)]
pub struct LintWarning {
    pub path: JPath,
    pub message: String,
}

pub fn lint(root: &JNode) -> Vec<LintWarning> {
    let mut warnings = Vec::new();
    lint_node(root, &[], 0, &mut warnings);
    warnings
}

fn lint_node(node: &JNode, path: &[JKey], depth: usize, out: &mut Vec<LintWarning>) {
    if depth > MAX_DEPTH {
        out.push(LintWarning {
            path: path.to_vec(),
            message: format!("excessive nesting depth ({})", depth),
        });
        return;
    }

    match node {
        JNode::Scalar(_) => {}
        JNode::Object { entries, .. } => {
            for (k, child) in entries {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Field(Rc::from(k.as_str())));
                if k.is_empty() {
                    out.push(LintWarning {
                        path: child_path.clone(),
                        message: "empty key".to_string(),
                    });
                }
                lint_node(child, &child_path, depth + 1, out);
            }
        }
        JNode::Array { items, .. } => {
            for (i, child) in items.iter().enumerate() {
                let mut child_path = path.to_vec();
                child_path.push(JKey::Index(i));
                lint_node(child, &child_path, depth + 1, out);
            }
        }
    }
}

/// Incrementally re-lint just the subtree at `target`, reusing the same contiguous-block splice
/// technique as `tree::patch_flat`/`pretty::patch_annotated` — `lint_node` is pre-order DFS too,
/// so even though `warnings` is a *sparse* subsequence (only violating nodes appear, not every
/// node), all of target's subtree's warnings still land in one contiguous run, in the same
/// relative order a full `lint()` rebuild would produce them in.
///
/// Returns `false` (leaving `warnings` unmodified) only when target's subtree had **no** warnings
/// before AND now has at least one new one — inserting a brand-new warning into a warning list
/// that was previously empty for this subtree requires knowing where it belongs relative to
/// *other* subtrees' warnings, which isn't derivable from `target`'s own path alone (Object key
/// order follows insertion order, not any simple lexicographic comparison). Caller must fall back
/// to a full `lint(root)` in that case. This is intentionally the one remaining unoptimized edge
/// case, not a general limitation — it only fires when an edit introduces a structural violation
/// (empty key / excessive depth) where none existed in that subtree before, which is rare; the
/// common case (no warnings before, none after) is the O(warnings_count) fast path this exists
/// to serve, and it's a true no-op — nothing removed, nothing spliced, nothing to do.
pub fn patch_lint(warnings: &mut Vec<LintWarning>, root: &JNode, target: &JPath) -> bool {
    let old_range = warnings.iter().position(|w| w.path.starts_with(target.as_slice())).map(|start| {
        let mut end = start + 1;
        while end < warnings.len() && warnings[end].path.starts_with(target.as_slice()) {
            end += 1;
        }
        start..end
    });

    let mut new_warnings = Vec::new();
    if let Some(node) = get_node_at_path(root, target) {
        lint_node(node, target, target.len(), &mut new_warnings);
    }

    match old_range {
        Some(range) => {
            warnings.splice(range, new_warnings);
            true
        }
        None if new_warnings.is_empty() => true, // nothing before, nothing after — no-op
        None => false, // new warning(s) appeared where none existed — caller falls back to full lint()
    }
}

#[cfg(test)]
mod patch_tests {
    use super::*;
    use crate::tree::{get_node_at_path_mut, set_node_at_path, JScalar};

    fn field(name: &str) -> JKey {
        JKey::Field(Rc::from(name))
    }

    /// A normal, warning-free fixture — the common case this optimization targets.
    fn clean_tree() -> JNode {
        JNode::from_value(serde_json::json!({
            "meta": { "name": "demo", "tags": ["x", "y"] },
            "items": [
                { "id": 1, "nested": { "a": 1 } },
                { "id": 2, "nested": { "a": 2 } }
            ],
            "flag": true
        }))
    }

    #[test]
    fn no_warnings_before_or_after_is_a_true_noop() {
        let mut root = clean_tree();
        let mut warnings = lint(&root);
        assert!(warnings.is_empty());
        let target: JPath = vec![field("flag")];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::Bool(false)));
        let ok = patch_lint(&mut warnings, &root, &target);
        assert!(ok);
        assert_eq!(warnings, lint(&root));
        assert!(warnings.is_empty());
    }

    #[test]
    fn existing_warning_resolved_by_edit() {
        let mut root = clean_tree();
        // introduce an empty key under "meta" up front
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert(String::new(), JNode::Scalar(JScalar::Number("1".into())));
        }
        let mut warnings = lint(&root);
        assert_eq!(warnings.len(), 1);

        // now remove the empty key — warning should be patched away
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_remove("");
        }
        let ok = patch_lint(&mut warnings, &root, &target);
        assert!(ok);
        assert_eq!(warnings, lint(&root));
        assert!(warnings.is_empty());
    }

    #[test]
    fn existing_warning_survives_unrelated_sibling_edit() {
        let mut root = clean_tree();
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert(String::new(), JNode::Scalar(JScalar::Number("1".into())));
        }
        let mut warnings = lint(&root);
        assert_eq!(warnings.len(), 1);

        // edit a different, unrelated key under the same parent
        let name_path: JPath = vec![field("meta"), field("name")];
        set_node_at_path(&mut root, &name_path, JNode::Scalar(JScalar::String("renamed".into())));
        let ok = patch_lint(&mut warnings, &root, &target);
        assert!(ok);
        assert_eq!(warnings, lint(&root));
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn deleting_parent_removes_all_descendant_warnings() {
        let mut root = clean_tree();
        let inner: JPath = vec![field("items"), JKey::Index(0), field("nested")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &inner).unwrap() {
            entries.insert(String::new(), JNode::Scalar(JScalar::Number("9".into())));
        }
        let mut warnings = lint(&root);
        assert_eq!(warnings.len(), 1);

        let parent: JPath = vec![field("items"), JKey::Index(0)];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &parent).unwrap() {
            entries.shift_remove("nested");
        }
        let ok = patch_lint(&mut warnings, &root, &parent);
        assert!(ok);
        assert_eq!(warnings, lint(&root));
        assert!(warnings.is_empty());
    }

    #[test]
    fn new_warning_in_previously_clean_subtree_signals_fallback() {
        let mut root = clean_tree();
        let mut warnings = lint(&root);
        assert!(warnings.is_empty());

        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert(String::new(), JNode::Scalar(JScalar::Number("1".into())));
        }
        let ok = patch_lint(&mut warnings, &root, &target);
        assert!(!ok, "patch_lint must signal false when a new warning appears where none existed");
        // caller's documented fallback:
        warnings = lint(&root);
        assert_eq!(warnings.len(), 1);
    }

    #[test]
    fn excessive_depth_detected_at_target_itself() {
        // build a chain deeper than MAX_DEPTH under "flag"'s position
        let mut root = clean_tree();
        let mut current = JNode::Scalar(JScalar::Bool(true));
        for _ in 0..(MAX_DEPTH + 5) {
            let mut entries = indexmap::IndexMap::new();
            entries.insert("nested".to_string(), current);
            current = JNode::Object { entries, collapsed: false };
        }
        let target: JPath = vec![field("flag")];
        set_node_at_path(&mut root, &target, current);

        let mut warnings = lint(&root);
        assert!(!warnings.is_empty());

        // no-op patch at the same target: re-lint should reproduce the identical warning set
        let ok = patch_lint(&mut warnings, &root, &target);
        assert!(ok);
        assert_eq!(warnings, lint(&root));
    }

    #[test]
    fn root_as_target() {
        let mut root = clean_tree();
        let mut warnings = lint(&root);
        assert!(warnings.is_empty());

        let target: JPath = vec![];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert(String::new(), JNode::Scalar(JScalar::Bool(false)));
        }
        let ok = patch_lint(&mut warnings, &root, &target);
        assert!(!ok); // new warning in a previously-clean root — same fallback case
        warnings = lint(&root);
        assert_eq!(warnings.len(), 1);
    }
}
