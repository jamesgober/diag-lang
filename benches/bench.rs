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

/// A label covering a whole 50,000-line file: the worst case for multi-line
/// rendering (ISSUES M16/M17). The cost must be bounded by what is printed, not by
/// the number of lines the span covers.
fn bench_render_whole_file(c: &mut Criterion) {
    let mut text = String::new();
    for line in 0..50_000 {
        text.push_str(&format!(
            "    let value_{line:05} = compute({line});
"
        ));
    }
    let len = text.len() as u32;
    let mut map = SourceMap::new();
    map.add("big.rs", text).expect("benchmark input fits");

    let diag = Diagnostic::new(
        diag_lang::Severity::Error,
        "this whole file is wrong",
        Label::new(Span::new(0, len), "everything here"),
    );
    let renderer = Renderer::new();

    let mut group = c.benchmark_group("render_whole_file");
    group.sample_size(10);
    group.bench_function("50k_lines", |b| {
        b.iter(|| renderer.render(black_box(&diag), black_box(&map)));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_construct,
    bench_render,
    bench_render_whole_file
);
criterion_main!(benches);
