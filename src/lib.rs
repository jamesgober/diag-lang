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
