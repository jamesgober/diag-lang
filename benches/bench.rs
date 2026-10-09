//! Benchmarks for the two costs that matter: building a diagnostic, which is on
//! the front-end's hot path, and rendering one, which is not but should still be
//! cheap. Construction is measured because the engineering law caps its cost — a
//! diagnostic must allocate only what its message and label require. Since
//! 1.1.0 also the new paths: a coded diagnostic with a fix, its rendering and
//! JSON, the collector at 100k diagnostics, and applying 100k edits.

use criterion::{Criterion, criterion_group, criterion_main};
use diag_lang::{
    Applicability, Code, Diagnostic, Diagnostics, Edit, Label, Renderer, Severity, SourceMap, Span,
    Suggestion, apply_edits,
};
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

/// A code, checked at compile time.
const E0425: Code = Code::new("E", 425).unwrap();

/// A diagnostic with a code and a one-edit suggestion: what a 1.1 front end
/// builds for a typo. Construction stays on the hot path.
fn coded_with_fix() -> Diagnostic {
    Diagnostic::new(
        Severity::Error,
        "cannot find value `foo` in this scope",
        Label::new(Span::new(24, 27), "not found in this scope"),
    )
    .with_code(E0425)
    .with_suggestion(
        Suggestion::new(
            "a local with a similar name exists",
            Applicability::MaybeIncorrect,
        )
        .with_edit(Edit::new(Span::new(24, 27), "fob")),
    )
}

fn bench_construct_with_fix(c: &mut Criterion) {
    c.bench_function("construct_with_code_and_fix", |b| {
        b.iter(|| {
            Diagnostic::new(
                black_box(Severity::Error),
                black_box("cannot find value `foo` in this scope"),
                Label::new(
                    black_box(Span::new(24, 27)),
                    black_box("not found in this scope"),
                ),
            )
            .with_code(black_box(E0425))
            .with_suggestion(
                Suggestion::new(
                    black_box("did you mean `fob`?"),
                    Applicability::MaybeIncorrect,
                )
                .with_edit(Edit::new(black_box(Span::new(24, 27)), black_box("fob"))),
            )
        });
    });
}

fn bench_render_with_fix(c: &mut Criterion) {
    let map = sample_map();
    let diag = coded_with_fix();
    let renderer = Renderer::new();
    c.bench_function("render_with_code_and_fix", |b| {
        b.iter(|| renderer.render(black_box(&diag), black_box(&map)));
    });
}

fn bench_json(c: &mut Criterion) {
    let map = sample_map();
    let diag = coded_with_fix();
    c.bench_function("to_json", |b| {
        b.iter(|| black_box(&diag).to_json(black_box(&map)));
    });
}

/// 100,000 diagnostics into a collector, every one pushed twice (the second
/// push is a duplicate), at only 100 distinct positions: the cost of the
/// duplicate check at scale.
fn bench_collect(c: &mut Criterion) {
    let diags: Vec<Diagnostic> = (0..100_000u32)
        .map(|i| {
            Diagnostic::new(
                Severity::Error,
                format!("unexpected token #{i}"),
                Label::unlabelled(Span::empty(i % 100)),
            )
        })
        .collect();
    let mut group = c.benchmark_group("collect");
    group.sample_size(10);
    group.bench_function("push_100k_twice", |b| {
        b.iter(|| {
            let mut sink = Diagnostics::new();
            for d in &diags {
                let _ = sink.push(d.clone());
            }
            for d in &diags {
                let _ = sink.push(d.clone());
            }
            sink.len()
        });
    });
    group.bench_function("sort_100k", |b| {
        let mut sink: Diagnostics = diags.iter().rev().cloned().collect();
        b.iter(|| {
            sink.sort();
            sink.len()
        });
    });
    group.finish();
}

/// 100,000 insertions applied to a 1.9 MB text in one call.
fn bench_apply(c: &mut Criterion) {
    let text = "    let value = compute(42);\n".repeat(70_000);
    let edits: Vec<Edit> = (0..100_000u32).map(|i| Edit::insert(i * 19, ";")).collect();
    let mut group = c.benchmark_group("apply_edits");
    group.sample_size(10);
    group.bench_function("100k_edits", |b| {
        b.iter(|| apply_edits(black_box(&text), 0, black_box(&edits)).map(|s| s.len()));
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_construct,
    bench_render,
    bench_render_whole_file,
    bench_construct_with_fix,
    bench_render_with_fix,
    bench_json,
    bench_collect,
    bench_apply
);
criterion_main!(benches);
