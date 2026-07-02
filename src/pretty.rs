use crate::tree::{get_node_at_path, JKey, JNode, JPath, JScalar, PatchSpan};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegColor {
    Key,
    Str,
    Num,
    Bool,
    Null,
    Punct,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    pub text: String,
    pub color: SegColor,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatedLine {
    pub segs: Vec<Seg>,
    pub path: JPath,
}

/// Walk `root` and emit one `AnnotatedLine` per output line.
/// Each line is tagged with the `JPath` of the innermost node it belongs to,
/// so the renderer can highlight all lines where `line.path.starts_with(cursor_path)`.
pub fn annotate(root: &JNode) -> Vec<AnnotatedLine> {
    let mut lines = Vec::new();
    emit(root, 0, &[], None, false, &mut lines);
    lines
}

fn emit(
    node: &JNode,
    depth: usize,
    path: &[JKey],
    key: Option<&str>,
    comma: bool,
    out: &mut Vec<AnnotatedLine>,
) {
    let pv = path.to_vec();

    let mut prefix = indent_segs(depth);
    if let Some(k) = key {
        prefix.push(Seg { text: format!("\"{}\"", k), color: SegColor::Key });
        prefix.push(Seg { text: ": ".to_string(), color: SegColor::Punct });
    }

    match node {
        JNode::Scalar(s) => {
            let mut segs = prefix;
            let (txt, col) = scalar_seg(s);
            segs.push(Seg { text: txt, color: col });
            if comma { segs.push(Seg { text: ",".to_string(), color: SegColor::Punct }); }
            out.push(AnnotatedLine { segs, path: pv });
        }
        JNode::Object { entries, .. } => {
            if entries.is_empty() {
                let mut segs = prefix;
                segs.push(Seg { text: "{}".to_string(), color: SegColor::Punct });
                if comma { segs.push(Seg { text: ",".to_string(), color: SegColor::Punct }); }
                out.push(AnnotatedLine { segs, path: pv });
                return;
            }
            let mut open = prefix;
            open.push(Seg { text: "{".to_string(), color: SegColor::Punct });
            out.push(AnnotatedLine { segs: open, path: pv.clone() });

            let n = entries.len();
            for (i, (k, child)) in entries.iter().enumerate() {
                let mut cp = pv.clone();
                cp.push(JKey::Field(Rc::from(k.as_str())));
                emit(child, depth + 1, &cp, Some(k), i + 1 < n, out);
            }

            let mut close = indent_segs(depth);
            close.push(Seg { text: "}".to_string(), color: SegColor::Punct });
            if comma { close.push(Seg { text: ",".to_string(), color: SegColor::Punct }); }
            out.push(AnnotatedLine { segs: close, path: pv });
        }
        JNode::Array { items, .. } => {
            if items.is_empty() {
                let mut segs = prefix;
                segs.push(Seg { text: "[]".to_string(), color: SegColor::Punct });
                if comma { segs.push(Seg { text: ",".to_string(), color: SegColor::Punct }); }
                out.push(AnnotatedLine { segs, path: pv });
                return;
            }
            let mut open = prefix;
            open.push(Seg { text: "[".to_string(), color: SegColor::Punct });
            out.push(AnnotatedLine { segs: open, path: pv.clone() });

            let n = items.len();
            for (i, child) in items.iter().enumerate() {
                let mut cp = pv.clone();
                cp.push(JKey::Index(i));
                emit(child, depth + 1, &cp, None, i + 1 < n, out);
            }

            let mut close = indent_segs(depth);
            close.push(Seg { text: "]".to_string(), color: SegColor::Punct });
            if comma { close.push(Seg { text: ",".to_string(), color: SegColor::Punct }); }
            out.push(AnnotatedLine { segs: close, path: pv });
        }
    }
}

/// Replace the contiguous block of `annotated` belonging to `target` with a freshly-emitted
/// version of the current subtree at `target` in `root`. Mirrors `tree::patch_flat` exactly —
/// same contiguous-range-by-path-prefix technique, same `PatchSpan` return type.
///
/// The comma state to reuse is read back from the *last* line of the *old* range: in `emit()`,
/// a trailing comma is only ever appended to a node's own terminal line (its single line if
/// scalar/empty-container, or its closing-brace line if non-empty) — the close line is always
/// pushed last, after every descendant, so `annotated[end - 1]` is always target's own line,
/// never a descendant's. This holds for every patchable edit by construction: patching only ever
/// happens when `target`'s position among *its own parent's* siblings is unchanged (that's what
/// distinguishes a patchable edit from one requiring a full `annotate()`), so the comma state
/// carries over unchanged too.
pub fn patch_annotated(annotated: &mut Vec<AnnotatedLine>, root: &JNode, target: &JPath) -> Option<PatchSpan> {
    let start = annotated.iter().position(|l| l.path == *target)?;
    let mut end = start + 1;
    while end < annotated.len() && annotated[end].path.starts_with(target.as_slice()) {
        end += 1;
    }

    let node = get_node_at_path(root, target)?;
    let had_comma = annotated[end - 1]
        .segs
        .last()
        .is_some_and(|s| s.color == SegColor::Punct && s.text == ",");

    let depth = target.len();
    let key: Option<&str> = match target.last() {
        Some(JKey::Field(k)) => Some(k.as_ref()),
        _ => None,
    };
    let mut new_lines = Vec::new();
    emit(node, depth, target, key, had_comma, &mut new_lines);

    let new_len = new_lines.len();
    annotated.splice(start..end, new_lines);
    Some(PatchSpan { start, new_len })
}

fn indent_segs(depth: usize) -> Vec<Seg> {
    if depth == 0 {
        return vec![];
    }
    vec![Seg { text: "  ".repeat(depth), color: SegColor::Punct }]
}

fn scalar_seg(s: &JScalar) -> (String, SegColor) {
    match s {
        JScalar::Null => ("null".to_string(), SegColor::Null),
        JScalar::Bool(b) => (b.to_string(), SegColor::Bool),
        JScalar::Number(n) => (n.clone(), SegColor::Num),
        JScalar::String(s) => (format!("\"{}\"", s), SegColor::Str),
    }
}

#[cfg(test)]
mod patch_tests {
    use super::*;
    use crate::tree::{get_node_at_path_mut, set_node_at_path};

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

    fn assert_patch_matches_full(root: &JNode, annotated: &mut Vec<AnnotatedLine>, target: &JPath) {
        let span = patch_annotated(annotated, root, target).expect("patch should find target");
        assert_eq!(*annotated, annotate(root), "patched annotated differs from full annotate()");
        assert_eq!(annotated[span.start].path, *target, "target line not at span.start");
    }

    #[test]
    fn replace_scalar_first_middle_last_of_array() {
        for idx in [0usize, 1, 2] {
            let mut root = sample_tree();
            let mut annotated = annotate(&root);
            let target: JPath = vec![field("meta"), field("tags"), JKey::Index(idx)];
            set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::String("changed".into())));
            assert_patch_matches_full(&root, &mut annotated, &target);
        }
    }

    #[test]
    fn replace_scalar_with_object_grows_subtree() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("flag")];
        set_node_at_path(&mut root, &target, JNode::from_value(serde_json::json!({"a": 1, "b": 2})));
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn delete_first_key_of_object() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_remove("name");
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn insert_key_into_middle_of_object() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_insert(1, "extra".to_string(), JNode::Scalar(JScalar::Bool(true)));
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn swap_adjacent_object_keys() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.swap_indices(0, 1);
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn rename_key_in_place_same_index() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            let (idx, _, val) = entries.shift_remove_full("name").unwrap();
            entries.shift_insert(idx, "full_name".to_string(), val);
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn root_as_target() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert("new_top_level".to_string(), JNode::Scalar(JScalar::Bool(false)));
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn deep_target_depth_four() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("items"), JKey::Index(1), field("nested"), field("a")];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::Number("999".into())));
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    // --- comma-boundary focus: patch first/middle/last element of a 3-item array ---

    #[test]
    fn comma_boundary_first_element_of_array() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta"), field("tags"), JKey::Index(0)];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::String("Q".into())));
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn comma_boundary_middle_element_of_array() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta"), field("tags"), JKey::Index(1)];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::String("Q".into())));
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn comma_boundary_last_element_of_array_has_no_trailing_comma() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("meta"), field("tags"), JKey::Index(2)];
        set_node_at_path(&mut root, &target, JNode::Scalar(JScalar::String("Q".into())));
        let span = patch_annotated(&mut annotated, &root, &target).unwrap();
        let last_line = &annotated[span.start];
        assert!(
            !last_line.segs.last().is_some_and(|s| s.text == ","),
            "last array element must not have a trailing comma"
        );
        assert_eq!(annotated, annotate(&root));
    }

    // --- empty <-> non-empty container transitions ---

    #[test]
    fn insert_first_key_into_previously_empty_object() {
        let mut root = sample_tree();
        let mut annotated = annotate(&root);
        let target: JPath = vec![field("empty_obj")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert("first".to_string(), JNode::Scalar(JScalar::Number("1".into())));
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn delete_only_remaining_key_leaves_empty_object() {
        let mut root = sample_tree();
        let target: JPath = vec![field("empty_obj")];
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.insert("only".to_string(), JNode::Scalar(JScalar::Number("1".into())));
        }
        // snapshot AFTER inserting "only" (that's the "old" state being patched from)
        let mut annotated = annotate(&root);
        if let JNode::Object { entries, .. } = get_node_at_path_mut(&mut root, &target).unwrap() {
            entries.shift_remove("only");
        }
        assert_patch_matches_full(&root, &mut annotated, &target);
    }

    #[test]
    fn patch_annotated_returns_none_for_nonexistent_path() {
        let root = sample_tree();
        let mut annotated = annotate(&root);
        let bogus: JPath = vec![field("does_not_exist")];
        assert!(patch_annotated(&mut annotated, &root, &bogus).is_none());
    }
}
