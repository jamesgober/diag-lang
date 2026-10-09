//! # diag_lang
//!
//! The diagnostics layer of a compiler front-end: it collects diagnostics — a
//! severity, a message, and a labelled span — and renders them with source
//! context, drawing a caret underline exactly under the characters at fault, in
//! the style developers expect from a modern toolchain.
//!
//! It owns presentation only. A lexer, parser, or type checker decides *what* is
//! wrong and builds a [`Diagnostic`]; diag-lang decides how it looks. Positions
//! come from [`span_lang`] and source text from [`source_lang`]: a [`Label`]
//! points with a [`Span`] in a [`SourceMap`]'s global position space, and the
//! [`Renderer`] resolves that span back to a file, a line, and a column.
//!
//! ## Alignment
//!
//! The caret line is aligned in terminal display columns, not bytes: each
//! character takes its Unicode display width (two columns for CJK and wide emoji,
//! none for a combining mark), and a tab advances to the next tab stop. The same
//! diagnostic always renders byte-identical output, a zero-width label at the end
//! of a source renders in that source's frame, and a span that falls outside the
//! loaded sources renders a defined note rather than panicking.
//!
//! ## Untrusted text
//!
//! Every caller-supplied string the renderer writes (source lines, messages,
//! labels, notes, help, file names) has its control characters escaped visibly
//! as `\u{..}`, so a hostile source or message cannot inject terminal escape
//! sequences. See [`Renderer`] for the exact set.
//!
//! ## Quickstart
//!
//! ```
//! use diag_lang::{Diagnostic, Label, Renderer, Severity};
//! use diag_lang::{SourceMap, Span};
//!
//! let mut map = SourceMap::new();
//! map.add("main.rs", "fn main() {\n    let x = foo();\n}\n").expect("fits");
//!
//! let diag = Diagnostic::new(
//!     Severity::Error,
//!     "cannot find value `foo` in this scope",
//!     Label::new(Span::new(24, 27), "not found in this scope"),
//! );
//!
//! let rendered = Renderer::new().render(&diag, &map);
//! assert!(rendered.contains("^^^ not found in this scope"));
//! ```
//!
//! ## Codes, fix-its, and machine-readable output
//!
//! *Since 1.1.0.* A diagnostic can carry a stable [`Code`] (`E0001`, shown
//! rustc-style as `error[E0001]: …`) and any number of fix-it [`Suggestion`]s,
//! each a set of [`Edit`]s with an [`Applicability`]. The renderer shows a
//! suggestion as a `help:` block with the fixed source; [`apply_fixes`] applies
//! the machine-applicable ones to a source string, validated and atomic; and
//! [`Diagnostic::to_json`] / [`Diagnostics::to_json`] emit a versioned,
//! deterministic JSON form for language servers, MCP tools, and AI clients.
//!
//! ```
//! use diag_lang::{Applicability, Code, Diagnostic, Edit, Label, Renderer, Severity};
//! use diag_lang::{SourceMap, Span, Suggestion, apply_fixes};
//!
//! let text = "let total = count + 1\n";
//! let mut map = SourceMap::new();
//! map.add("calc.ls", text).expect("fits");
//!
//! let diag = Diagnostic::new(
//!     Severity::Error,
//!     "expected `;`",
//!     Label::new(Span::empty(21), "statement ends here"),
//! )
//! .with_code(Code::new("E", 1001).expect("well-formed"))
//! .with_suggestion(
//!     Suggestion::new("add `;`", Applicability::MachineApplicable).with_edit(Edit::insert(21, ";")),
//! );
//!
//! assert_eq!(Renderer::new().render(&diag, &map), "\
//! error[E1001]: expected `;`
//!  --> calc.ls:1:22
//!   |
//! 1 | let total = count + 1
//!   |                      ^ statement ends here
//!   |
//! help: add `;`
//!   |
//! 1 | let total = count + 1;
//!   |                      +
//!   |
//! ");
//!
//! assert_eq!(apply_fixes(text, 0, [&diag])?, "let total = count + 1;\n");
//! assert!(diag.to_json(&map).contains(r#""code":"E1001""#));
//! # Ok::<(), diag_lang::ApplyError>(())
//! ```
//!
//! A [`Diagnostics`] collector gathers a compilation's diagnostics: it counts
//! errors and warnings, drops exact duplicates, sorts by position, and caps
//! how many it stores, with [`Renderer::render_all`] printing an "N more" line
//! for the rest.
//!
//! ## Output
//!
//! [`Renderer::render`] returns a `String`; [`Renderer::render_to`] writes into
//! any [`core::fmt::Write`] sink, so the same renderer feeds a terminal, a reused
//! buffer, or a formatter without an intermediate allocation. With the `color`
//! feature, [`Renderer::with_color`] wraps the output in ANSI styling; the plain
//! output is complete and identically aligned, so colour only ever adds.
//!
//! ## Features
//!
//! - `std` (default) — the standard library; without it the crate is `no_std`
//!   (it always needs `alloc`). Forwards to `span-lang/std` and `source-lang/std`.
//! - `color` (default) — ANSI styling via [`Renderer::with_color`]. Disable it,
//!   or simply never call `with_color`, for plain output.
//!
//! ## Stability
//!
//! The public API is stable as of `1.0` and follows Semantic Versioning: no
//! breaking changes before `2.0`, additions arrive in minor releases, and the MSRV
//! (Rust 1.85) only rises in a minor. The full surface is catalogued in
//! [`docs/API.md`](https://github.com/jamesgober/diag-lang/blob/main/docs/API.md).

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![forbid(unsafe_code)]
#![deny(
    missing_docs,
    unused_must_use,
    unused_results,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented,
    clippy::unreachable,
    clippy::dbg_macro,
    clippy::print_stdout,
    clippy::print_stderr
)]
// Tests may unwrap: a failed unwrap there is a failed test, not a reachable panic.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

extern crate alloc;

mod apply;
mod code;
mod collect;
mod diagnostic;
mod json;
mod label;
mod render;
mod severity;
mod suggestion;

pub use apply::{ApplyError, apply_edits, apply_fixes};
pub use code::Code;
pub use collect::Diagnostics;
pub use diagnostic::Diagnostic;
pub use json::JSON_SCHEMA_VERSION;
pub use label::Label;
pub use render::Renderer;
pub use severity::Severity;
pub use suggestion::{Applicability, Edit, Suggestion};

// The README and the API reference are compiled and run as doctests, so every
// example in them is checked against the code. Neither item exists outside
// `cargo test --doc`.
#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeDoctests;

#[cfg(doctest)]
#[doc = include_str!("../docs/API.md")]
struct ApiDoctests;

// Re-exported so a downstream building and rendering diagnostics can spell the
// position and source types this crate's API uses without also having to name
// `span-lang` and `source-lang`.
pub use source_lang::SourceMap;
pub use span_lang::Span;
