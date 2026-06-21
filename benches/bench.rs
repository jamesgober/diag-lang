//! Benchmarks for the two costs that matter: building a diagnostic, which is on
//! the front-end's hot path, and rendering one, which is not but should still be
//! cheap. Construction is measured because the engineering law caps its cost — a
//! diagnostic must allocate only what its message and label require.

use criterion::{Criterion, criterion_group, criterion_main};
use diag_lang::{Diagnostic, Label, Renderer, SourceMap, Span};
use std::hint::black_box;

/// A representative single-line source with the offending token mid-line.
fn sample_map() -> SourceMap {
    let mut map = SourceMap::new();
    map.add("main.rs", "fn main() {\n    let x = foo();\n}\n")
        .expect("benchmark input fits");
    map
}

fn bench_construct(c: &mut Criterion) {
    c.bench_function("construct", |b| {
        b.iter(|| {
            Diagnostic::new(
                black_box(diag_lang::Severity::Error),
                black_box("cannot find value `foo` in this scope"),
                Label::new(
                    black_box(Span::new(24, 27)),
                    black_box("not found in this scope"),
                ),
            )
        });
    });
}

fn bench_render(c: &mut Criterion) {
    let map = sample_map();
    let diag = Diagnostic::new(
        diag_lang::Severity::Error,
        "cannot find value `foo` in this scope",
        Label::new(Span::new(24, 27), "not found in this scope"),
    );
    let renderer = Renderer::new();

    c.bench_function("render", |b| {
        b.iter(|| renderer.render(black_box(&diag), black_box(&map)));
    });
}

criterion_group!(benches, bench_construct, bench_render);
criterion_main!(benches);
