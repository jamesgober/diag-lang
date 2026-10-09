//! The 1.1 path end to end: a check reports coded diagnostics with fix-its into
//! a collector, the driver renders them for a person, emits them as JSON for a
//! tool, and applies the machine-applicable fixes.
//!
//! Run with `cargo run --example fixes`.

use diag_lang::{
    Applicability, Code, Diagnostic, Diagnostics, Edit, Label, Renderer, Severity, SourceMap, Span,
    Suggestion, apply_fixes,
};

/// A tiny code registry: one prefix, lexical codes below 1000, parse codes in
/// the 1000s, semantic codes from 2000 (the convention `docs/API.md` describes).
mod codes {
    use diag_lang::Code;

    pub const MISSING_SEMICOLON: Code = Code::new("LS", 1001).unwrap();
    pub const UNKNOWN_NAME: Code = Code::new("LS", 2001).unwrap();
    pub const UNUSED_VARIABLE: Code = Code::new("LS", 8001).unwrap();
}

/// A deliberately naive "checker" for a three-line program, reporting what a
/// real front end would.
fn check(text: &str) -> Diagnostics {
    let at = |needle: &str| text.find(needle).map_or(0, |i| i as u32);
    let mut diags = Diagnostics::with_limit(100);

    // Line 1 lacks its `;`.
    let end1 = at("\nlet y");
    diags.push(
        Diagnostic::new(
            Severity::Error,
            "expected `;`",
            Label::new(Span::empty(end1), "statement ends here"),
        )
        .with_code(codes::MISSING_SEMICOLON)
        .with_suggestion(
            Suggestion::new("add `;`", Applicability::MachineApplicable)
                .with_edit(Edit::insert(end1, ";")),
        ),
    );

    // `cout` is a typo for `count`.
    let typo = at("cout");
    diags.push(
        Diagnostic::new(
            Severity::Error,
            "cannot find value `cout` in this scope",
            Label::new(Span::new(typo, typo + 4), "not found in this scope"),
        )
        .with_code(codes::UNKNOWN_NAME)
        .with_suggestion(
            Suggestion::new(
                "a value with a similar name exists",
                Applicability::MaybeIncorrect,
            )
            .with_edit(Edit::new(Span::new(typo, typo + 4), "count")),
        ),
    );

    // `unused` is never read; removing the line is machine-applicable.
    let line = at("let unused");
    let line_end = at("\nprint") + 1;
    diags.push(
        Diagnostic::new(
            Severity::Warning,
            "unused variable `unused`",
            Label::new(Span::new(line + 4, line + 10), "never read"),
        )
        .with_code(codes::UNUSED_VARIABLE)
        .with_suggestion(
            Suggestion::new("remove the binding", Applicability::MachineApplicable)
                .with_edit(Edit::delete(Span::new(line, line_end))),
        ),
    );

    diags.sort();
    diags
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let text = "let count = 1\nlet y = cout + 1;\nlet unused = 2;\nprint(y);\n";
    let mut map = SourceMap::new();
    let _ = map.add("main.ls", text)?;

    let diags = check(text);

    // For a person.
    print!("{}", Renderer::new().render_all(&diags, &map));
    println!(
        "\nerrors: {}, warnings: {}\n",
        diags.error_count(),
        diags.warning_count()
    );

    // For a tool: one JSON document, deterministic and versioned.
    println!("{}\n", diags.to_json(&map));

    // `--fix`: apply what is safe to apply, leave the rest for a person.
    let fixed = apply_fixes(text, 0, &diags)?;
    print!("{fixed}");

    // The phase is a range check on the code, not a guess from the message.
    let parse_errors = diags
        .iter()
        .filter_map(Diagnostic::code)
        .filter(|c: &Code| (1000..2000).contains(&c.number()))
        .count();
    println!("\nparse errors: {parse_errors}");
    Ok(())
}
