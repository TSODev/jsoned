use crate::tree::{JKey, JNode, JPath, JScalar};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegColor {
    Key,
    Str,
    Num,
    Bool,
    Null,
    Punct,
}

#[derive(Debug, Clone)]
pub struct Seg {
    pub text: String,
    pub color: SegColor,
}

#[derive(Debug, Clone)]
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
                cp.push(JKey::Field(k.clone()));
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
