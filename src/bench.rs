//! Permanent, reproducible performance benchmarks for the incremental-patch machinery
//! (`tree::patch_flat`, `pretty::patch_annotated`, `lint::patch_lint`) added this session.
//! See BENCHMARK.md for methodology, caveats, and reference results.
//!
//! `#[cfg(test)]`-gated at the `mod bench;` declaration in main.rs — compiled only for test
//! builds, zero cost/footprint in the shipped release binary.
//!
//! Run: `cargo test --release bench -- --ignored --nocapture`
//!
//! No `criterion`/external `benches/` harness: jsoned is a binary-only crate (no `lib.rs`), so
//! an external bench file would only see `pub` items — several things benchmarked here
//! (`App`'s private methods, see the separate bench in `app/mod.rs`) are deliberately not part
//! of the public surface. Splitting into lib+bin just to enable `criterion` would either expose
//! internals that should stay private or require a non-trivial restructure — not worth it for a
//! project this size. Manual `Instant` timing, `REPEATS` runs per size reporting the minimum
//! (least affected by transient OS/scheduler noise), is simpler and sufficient here: the
//! differences this suite measures are 10-100x, far beyond what timing noise could produce.

use crate::lint::{lint, patch_lint};
use crate::pretty::{annotate, patch_annotated};
use crate::tree::{flatten, patch_flat, set_node_at_path, JKey, JNode, JPath, JScalar};
use std::rc::Rc;
use std::time::Instant;

const SIZES: [usize; 4] = [1_000, 10_000, 50_000, 100_000];
const REPEATS: usize = 3;

fn synthetic(items: usize) -> serde_json::Value {
    let arr: Vec<serde_json::Value> = (0..items)
        .map(|i| {
            serde_json::json!({
                "id": i,
                "name": format!("item-{}", i),
                "active": i % 2 == 0,
                "score": i as f64 * 1.5,
                "tags": ["a", "b", "c"],
                "meta": { "created": "2026-07-02", "owner": "svc" },
                "note": "some representative string payload for a field",
                "count": i * 3,
            })
        })
        .collect();
    serde_json::Value::Array(arr)
}

/// Times `op` `REPEATS` times, returns the minimum elapsed ms. No setup phase — for pure
/// functions that don't mutate their input (`flatten`/`annotate`/`lint`).
fn min_ms<F: FnMut()>(mut op: F) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..REPEATS {
        let t0 = Instant::now();
        op();
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if ms < best {
            best = ms;
        }
    }
    best
}

/// Same, but with an untimed `setup` phase run fresh before each timed iteration — for the
/// patch functions, which mutate their target `Vec` in place (need a fresh clone per iteration,
/// and that clone must not count toward the timed cost).
fn min_ms_with_setup<S, F: FnMut() -> S, G: FnMut(&mut S)>(mut setup: F, mut op: G) -> f64 {
    let mut best = f64::MAX;
    for _ in 0..REPEATS {
        let mut state = setup();
        let t0 = Instant::now();
        op(&mut state);
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        if ms < best {
            best = ms;
        }
    }
    best
}

#[test]
#[ignore]
fn bench_full_rebuild_vs_patch() {
    println!(
        "{:>7}  {:>10}  {:>10}  {:>10}  {:>11} |  {:>9} {:>9} {:>9}  {:>10} | {:>8}",
        "items", "full_flat", "full_ann", "full_lint", "full_total",
        "p_flat", "p_ann", "p_lint", "p_total", "speedup"
    );

    for &items in &SIZES {
        let root = JNode::from_value(synthetic(items));

        // Baseline: full rebuild cost with the current implementation (already includes the
        // no-clone/Rc<str> fixes from earlier this session — this is "today's full cost", not
        // the original pre-optimization numbers, which live in CHANGELOG.md/BENCHMARK.md as
        // historical reference points).
        let full_flat_ms = min_ms(|| {
            let _ = flatten(&root);
        });
        let full_ann_ms = min_ms(|| {
            let _ = annotate(&root);
        });
        let full_lint_ms = min_ms(|| {
            let _ = lint(&root);
        });
        let full_total = full_flat_ms + full_ann_ms + full_lint_ms;

        // Patched: single-node edit near the END of the document — the worst case for the
        // position()-scan cost documented in tree::patch_flat (has to scan almost the whole
        // vector), so this is a conservative, not a best-case, number.
        let target: JPath = vec![JKey::Index(items - 1), JKey::Field(Rc::from("name"))];
        let mut edited_root = root.clone();
        set_node_at_path(&mut edited_root, &target, JNode::Scalar(JScalar::String("edited".into())));

        let flat = flatten(&root);
        let annotated = annotate(&root);
        let warnings = lint(&root);

        let p_flat_ms = min_ms_with_setup(
            || flat.clone(),
            |f| {
                patch_flat(f, &edited_root, &target);
            },
        );
        let p_ann_ms = min_ms_with_setup(
            || annotated.clone(),
            |a| {
                patch_annotated(a, &edited_root, &target);
            },
        );
        let p_lint_ms = min_ms_with_setup(
            || warnings.clone(),
            |w| {
                patch_lint(w, &edited_root, &target);
            },
        );
        let p_total = p_flat_ms + p_ann_ms + p_lint_ms;

        println!(
            "{:>7}  {:>8.2}ms  {:>8.2}ms  {:>8.2}ms  {:>9.2}ms |  {:>7.3}ms {:>7.3}ms {:>7.3}ms  {:>8.3}ms | {:>6.0}x",
            items,
            full_flat_ms,
            full_ann_ms,
            full_lint_ms,
            full_total,
            p_flat_ms,
            p_ann_ms,
            p_lint_ms,
            p_total,
            full_total / p_total.max(0.001)
        );
    }
}
