use crate::tree::{JKey, JNode, JPath};
use std::rc::Rc;

const MAX_DEPTH: usize = 20;

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
