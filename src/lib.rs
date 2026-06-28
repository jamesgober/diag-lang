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
//! The caret line is aligned in display columns, not bytes: a multi-byte UTF-8
//! character counts as one column, and a tab advances to the next tab stop. The
//! same diagnostic always renders byte-identical output, and a span that falls
//! outside the loaded sources renders a defined note rather than panicking.
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
#![deny(missing_docs)]
#![forbid(unsafe_code)]

extern crate alloc;

mod diagnostic;
mod label;
mod render;
mod severity;

pub use diagnostic::Diagnostic;
pub use label::Label;
pub use render::Renderer;
pub use severity::Severity;

// Re-exported so a downstream building and rendering diagnostics can spell the
// position and source types this crate's API uses without also having to name
// `span-lang` and `source-lang`.
pub use source_lang::SourceMap;
pub use span_lang::Span;
