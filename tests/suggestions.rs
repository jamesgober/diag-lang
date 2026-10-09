//! Byte-exact snapshot tests for diagnostic codes and fix-it suggestions
//! (`with_code`, `with_suggestion`), and for rendering a whole collector.
//!
//! The layout rules under test: a code sits in the header as `error[E0001]`;
//! each suggestion is a `help:` block after the notes and help lines, showing
//! the edited lines with `+` under insertions and `~` under replacements, or a
//! `-`/`+` diff when an edit only deletes; a block in another source names it;
//! a suggestion that cannot be placed shows its message alone.

#![allow(clippy::unwrap_used)]

use diag_lang::{
    Applicability, Code, Diagnostic, Diagnostics, Edit, Label, Renderer, Severity, SourceMap, Span,
    Suggestion,
};

/// The byte span of the first occurrence of `needle` in `text`.
fn span_of(text: &str, needle: &str) -> Span {
    let at = text.find(needle).expect("needle present in text") as u32;
    Span::new(at, at + needle.len() as u32)
}

fn one_file(name: &str, text: &str) -> SourceMap {
    let mut map = SourceMap::new();
    map.add(name, text).unwrap();
    map
}

fn render(diag: &Diagnostic, map: &SourceMap) -> String {
    Renderer::new().render(diag, map)
}

fn fix(message: &str) -> Suggestion {
    Suggestion::new(message, Applicability::MachineApplicable)
}

const E0425: Code = Code::new("E", 425).unwrap();

#[test]
fn test_code_appears_in_the_header() {
    let src = "let x = foo();";
    let map = one_file("main.rs", src);
    let diag = Diagnostic::new(
        Severity::Error,
        "cannot find value `foo` in this scope",
        Label::new(span_of(src, "foo"), "not found in this scope"),
    )
    .with_code(E0425);

    assert_eq!(
        render(&diag, &map),
        "\
error[E0425]: cannot find value `foo` in this scope
 --> main.rs:1:9
  |
1 | let x = foo();
  |         ^^^ not found in this scope
  |
"
    );
}

#[test]
fn test_code_on_every_severity_and_without_a_frame() {
    let map = SourceMap::new();
    for (severity, word) in [
        (Severity::Error, "error"),
        (Severity::Warning, "warning"),
        (Severity::Note, "note"),
        (Severity::Help, "help"),
    ] {
        let diag = Diagnostic::new(severity, "m", Label::unlabelled(Span::empty(3)))
            .with_code(Code::new("KRA", 9999).unwrap());
        assert_eq!(
            render(&diag, &map),
            format!("{word}[KRA9999]: m\n = note: span 3..3 is outside the loaded sources\n")
        );
    }
}

#[test]
fn test_insertion_is_marked_with_plus() {
    let src = "let x = 1\n";
    let map = one_file("a.ls", src);
    let diag = Diagnostic::new(
        Severity::Error,
        "expected `;`",
        Label::unlabelled(Span::empty(9)),
    )
    .with_code(Code::new("E", 1001).unwrap())
    .with_suggestion(fix("add `;`").with_edit(Edit::insert(9, ";")));

    assert_eq!(
        render(&diag, &map),
        "\
error[E1001]: expected `;`
 --> a.ls:1:10
  |
1 | let x = 1
  |          ^
  |
help: add `;`
  |
1 | let x = 1;
  |          +
  |
"
    );
}

#[test]
fn test_replacement_is_marked_with_tilde_under_the_new_text() {
    let src = "let y = cout + 1;";
    let map = one_file("a.ls", src);
    let typo = span_of(src, "cout");
    let diag = Diagnostic::new(
        Severity::Error,
        "unknown name",
        Label::new(typo, "not found"),
    )
    .with_suggestion(
        Suggestion::new("a similar name exists", Applicability::MaybeIncorrect)
            .with_edit(Edit::new(typo, "count")),
    );

    assert_eq!(
        render(&diag, &map),
        "\
error: unknown name
 --> a.ls:1:9
  |
1 | let y = cout + 1;
  |         ^^^^ not found
  |
help: a similar name exists
  |
1 | let y = count + 1;
  |         ~~~~~
  |
"
    );
}

#[test]
fn test_several_edits_on_one_line_share_a_marker_line() {
    // Wrap `a + b` in parentheses and rename `f` to `g`.
    let src = "x = f(a + b) * 2;";
    let map = one_file("a.ls", src);
    let sum = span_of(src, "a + b");
    let diag = Diagnostic::new(Severity::Warning, "precedence", Label::unlabelled(sum))
        .with_suggestion(
            fix("parenthesise and rename")
                // Added out of order on purpose: the block is drawn by position.
                .with_edit(Edit::insert(sum.end().to_u32(), ")"))
                .with_edit(Edit::new(span_of(src, "f"), "g"))
                .with_edit(Edit::insert(sum.start().to_u32(), "(")),
        );

    assert_eq!(
        render(&diag, &map),
        "\
warning: precedence
 --> a.ls:1:7
  |
1 | x = f(a + b) * 2;
  |       ^^^^^
  |
help: parenthesise and rename
  |
1 | x = g((a + b)) * 2;
  |     ~ +     +
  |
"
    );
}

#[test]
fn test_deletion_renders_as_a_diff() {
    let src = "let x = 1;;\n";
    let map = one_file("a.ls", src);
    let extra = Span::new(10, 11);
    let diag = Diagnostic::new(
        Severity::Warning,
        "unnecessary `;`",
        Label::unlabelled(extra),
    )
    .with_suggestion(fix("remove it").with_edit(Edit::delete(extra)));

    assert_eq!(
        render(&diag, &map),
        "\
warning: unnecessary `;`
 --> a.ls:1:11
  |
1 | let x = 1;;
  |           ^
  |
help: remove it
  |
1 - let x = 1;;
1 + let x = 1;
  |
"
    );
}

#[test]
fn test_whole_line_deletion_shows_only_the_removed_line() {
    let src = "let a = 1;\nlet unused = 2;\nprint(a);\n";
    let map = one_file("a.ls", src);
    let line = span_of(src, "let unused = 2;\n");
    let diag = Diagnostic::new(
        Severity::Warning,
        "unused variable",
        Label::unlabelled(span_of(src, "unused")),
    )
    .with_suggestion(fix("remove the binding").with_edit(Edit::delete(line)));

    assert_eq!(
        render(&diag, &map),
        "\
warning: unused variable
 --> a.ls:2:5
  |
2 | let unused = 2;
  |     ^^^^^^
  |
help: remove the binding
  |
2 - let unused = 2;
  |
"
    );
}

#[test]
fn test_joining_lines_shows_both_old_lines() {
    // Deleting the line break joins line 2 onto line 1.
    let src = "call(a,\nb);\n";
    let map = one_file("a.ls", src);
    let brk = Span::new(7, 8);
    let diag = Diagnostic::new(Severity::Note, "fits on one line", Label::unlabelled(brk))
        .with_suggestion(fix("join").with_edit(Edit::delete(brk)));

    assert_eq!(
        render(&diag, &map),
        "\
note: fits on one line
 --> a.ls:1:8
  |
1 | call(a,
  |        ^
  |
help: join
  |
1 - call(a,
2 - b);
1 + call(a,b);
  |
"
    );
}

#[test]
fn test_multi_line_replacement_marks_every_new_line() {
    let src = "if x { y }\n";
    let map = one_file("a.ls", src);
    let body = span_of(src, "{ y }");
    let diag = Diagnostic::new(Severity::Help, "style", Label::unlabelled(body))
        .with_suggestion(fix("expand the block").with_edit(Edit::new(body, "{\n    y\n}")));

    assert_eq!(
        render(&diag, &map),
        "\
help: style
 --> a.ls:1:6
  |
1 | if x { y }
  |      ^^^^^
  |
help: expand the block
  |
1 | if x {
  |      ~
2 |     y
  | ~~~~~
3 | }
  | ~
  |
"
    );
}

#[test]
fn test_long_suggestion_is_elided_like_a_long_span() {
    let src = "x\n";
    let map = one_file("a.ls", src);
    let at = Span::new(0, 1);
    let body = "a\nb\nc\nd\ne\nf\ng";
    let diag = Diagnostic::new(Severity::Error, "m", Label::unlabelled(at))
        .with_suggestion(fix("replace").with_edit(Edit::new(at, body)));

    assert_eq!(
        render(&diag, &map),
        "\
error: m
 --> a.ls:1:1
  |
1 | x
  | ^
  |
help: replace
  |
1 | a
  | ~
2 | b
  | ~
...
6 | f
  | ~
7 | g
  | ~
  |
"
    );
}

#[test]
fn test_suggestion_in_another_file_names_it() {
    let mut map = SourceMap::new();
    map.add("main.ls", "use helper;\n").unwrap();
    let lib = "fn helper() {}\n";
    let id = map.add("lib.ls", lib).unwrap();
    let base = map.source(id).unwrap().span().start().to_u32();
    let name = Span::new(base + 3, base + 9);

    let diag = Diagnostic::new(
        Severity::Error,
        "`helper` is private",
        Label::new(Span::new(4, 10), "private function"),
    )
    .with_suggestion(fix("make it public").with_edit(Edit::insert(base, "pub ")))
    .with_secondary(Label::new(name, "defined here"));

    assert_eq!(
        render(&diag, &map),
        "\
error: `helper` is private
 --> main.ls:1:5
  |
1 | use helper;
  |     ^^^^^^ private function
  |
 --> lib.ls:1:4
  |
1 | fn helper() {}
  |    ------ defined here
  |
help: make it public
 --> lib.ls:1:1
  |
1 | pub fn helper() {}
  | ++++
  |
"
    );
}

#[test]
fn test_suggestion_spanning_two_files_draws_a_block_per_file() {
    let mut map = SourceMap::new();
    map.add("a.ls", "old_name();\n").unwrap();
    map.add("b.ls", "fn old_name() {}\n").unwrap();
    let in_a = Span::new(0, 8);
    let in_b = Span::new(12 + 3, 12 + 11);
    let diag = Diagnostic::new(
        Severity::Warning,
        "deprecated name",
        Label::unlabelled(in_a),
    )
    .with_suggestion(
        Suggestion::new("rename everywhere", Applicability::MaybeIncorrect)
            .with_edit(Edit::new(in_b, "new_name"))
            .with_edit(Edit::new(in_a, "new_name")),
    );

    assert_eq!(
        render(&diag, &map),
        "\
warning: deprecated name
 --> a.ls:1:1
  |
1 | old_name();
  | ^^^^^^^^
  |
help: rename everywhere
  |
1 | new_name();
  | ~~~~~~~~
  |
 --> b.ls:1:4
  |
1 | fn new_name() {}
  |    ~~~~~~~~
  |
"
    );
}

#[test]
fn test_unplaceable_suggestions_show_only_their_message() {
    let src = "aé";
    let map = one_file("a.ls", src);
    let base = || Diagnostic::new(Severity::Error, "m", Label::unlabelled(Span::new(0, 1)));
    let header = "\
error: m
 --> a.ls:1:1
  |
1 | aé
  | ^
  |
";
    let cases = [
        // No edits at all.
        fix("just a hint"),
        // Outside every source.
        fix("just a hint").with_edit(Edit::insert(99, "x")),
        // Ending past the source.
        fix("just a hint").with_edit(Edit::new(Span::new(1, 50), "x")),
        // Splitting `é`.
        fix("just a hint").with_edit(Edit::insert(2, "x")),
        // Overlapping edits.
        fix("just a hint")
            .with_edit(Edit::new(Span::new(0, 1), "x"))
            .with_edit(Edit::new(Span::new(0, 3), "y")),
        // Only no-op edits.
        fix("just a hint").with_edit(Edit::insert(1, "")),
    ];
    for suggestion in cases {
        let diag = base().with_suggestion(suggestion.clone());
        assert_eq!(
            render(&diag, &map),
            format!("{header}help: just a hint\n"),
            "{suggestion:?}"
        );
    }
}

#[test]
fn test_suggestions_follow_notes_and_help_in_order() {
    let src = "f(x)";
    let map = one_file("a.ls", src);
    let x = span_of(src, "x");
    let diag = Diagnostic::new(Severity::Error, "bad argument", Label::unlabelled(x))
        .with_suggestion(fix("first").with_edit(Edit::new(x, "y")))
        .with_note("a note")
        .with_suggestion(fix("second").with_edit(Edit::new(x, "z")))
        .with_help("a help line");

    assert_eq!(
        render(&diag, &map),
        "\
error: bad argument
 --> a.ls:1:3
  |
1 | f(x)
  |   ^
  |
 = note: a note
 = help: a help line
help: first
  |
1 | f(y)
  |   ~
  |
help: second
  |
1 | f(z)
  |   ~
  |
"
    );
}

#[test]
fn test_markers_align_under_wide_characters_and_escapes() {
    let src = "名前 = 1";
    let map = one_file("a.ls", src);
    let eq = span_of(src, "=");
    let diag = Diagnostic::new(Severity::Error, "m", Label::unlabelled(eq)).with_suggestion(
        fix("use `:=`, \u{1b}[31mred")
            .with_edit(Edit::new(eq, ":=\u{7}"))
            .with_edit(Edit::insert(0, "世")),
    );

    // `世` is two columns, `名前 ` five; the bell is drawn as `\u{7}` (five
    // columns) and its markers cover the escape exactly. The message's escape
    // sequence is neutralised.
    assert_eq!(
        render(&diag, &map),
        "\
error: m
 --> a.ls:1:4
  |
1 | 名前 = 1
  |      ^
  |
help: use `:=`, \\u{1b}[31mred
  |
1 | 世名前 :=\\u{7} 1
  | ++     ~~~~~~~
  |
"
    );
}

#[test]
fn test_crlf_sources_render_without_carriage_returns() {
    let src = "a = 1\r\nb = 2\r\n";
    let map = one_file("w.ls", src);
    let at = span_of(src, "2");
    let diag = Diagnostic::new(Severity::Error, "m", Label::unlabelled(at))
        .with_suggestion(fix("use 3").with_edit(Edit::new(at, "3")))
        .with_suggestion(fix("drop line 1").with_edit(Edit::delete(Span::new(0, 7))));

    assert_eq!(
        render(&diag, &map),
        "\
error: m
 --> w.ls:2:5
  |
2 | b = 2
  |     ^
  |
help: use 3
  |
2 | b = 3
  |     ~
  |
help: drop line 1
  |
1 - a = 1
  |
"
    );
}

#[test]
fn test_unlocated_primary_still_frames_its_suggestion() {
    let map = one_file("a.ls", "abc");
    let diag = Diagnostic::new(Severity::Error, "m", Label::unlabelled(Span::empty(500)))
        .with_suggestion(fix("prefix").with_edit(Edit::insert(0, "x")));

    assert_eq!(
        render(&diag, &map),
        "\
error: m
 = note: span 500..500 is outside the loaded sources
help: prefix
 --> a.ls:1:1
  |
1 | xabc
  | +
  |
"
    );
}

#[test]
fn test_insertion_at_end_of_input() {
    let src = "a = b +";
    let map = one_file("a.ls", src);
    let end = src.len() as u32;
    let diag = Diagnostic::new(
        Severity::Error,
        "expected an expression",
        Label::new(Span::empty(end), "found end of input"),
    )
    .with_suggestion(
        Suggestion::new("add an operand", Applicability::HasPlaceholders)
            .with_edit(Edit::insert(end, " /* expr */")),
    );

    assert_eq!(
        render(&diag, &map),
        "\
error: expected an expression
 --> a.ls:1:8
  |
1 | a = b +
  |        ^ found end of input
  |
help: add an operand
  |
1 | a = b + /* expr */
  |        +++++++++++
  |
"
    );
}

#[test]
fn test_gutter_widens_for_the_suggestion_block() {
    let mut src = String::new();
    for i in 1..=9 {
        src.push_str(&format!("line{i}\n"));
    }
    let map = one_file("a.ls", &src);
    let nine = span_of(&src, "line9");
    let diag = Diagnostic::new(Severity::Error, "m", Label::unlabelled(nine))
        .with_suggestion(fix("split").with_edit(Edit::new(nine, "line9\nline10")));

    assert_eq!(
        render(&diag, &map),
        "\
error: m
 --> a.ls:9:1
  |
9 | line9
  | ^^^^^
  |
help: split
   |
 9 | line9
   | ~~~~~
10 | line10
   | ~~~~~~
   |
"
    );
}

#[test]
fn test_render_all_separates_diagnostics_and_summarises_the_rest() {
    let src = "a b c";
    let map = one_file("x.ls", src);
    let mut diags = Diagnostics::with_limit(2);
    for (needle, msg) in [("c", "third"), ("a", "first"), ("b", "second")] {
        diags.push(Diagnostic::new(
            Severity::Error,
            msg,
            Label::unlabelled(span_of(src, needle)),
        ));
    }
    diags.sort();

    assert_eq!(
        Renderer::new().render_all(&diags, &map),
        "\
error: first
 --> x.ls:1:1
  |
1 | a b c
  | ^
  |

error: third
 --> x.ls:1:5
  |
1 | a b c
  |     ^
  |

note: 1 more diagnostic not shown
"
    );
}

#[test]
fn test_render_all_of_an_empty_collector_is_empty() {
    let map = SourceMap::new();
    assert_eq!(Renderer::new().render_all(&Diagnostics::new(), &map), "");
}

#[test]
fn test_render_all_equals_concatenated_renders() {
    let src = "x y";
    let map = one_file("x.ls", src);
    let diags: Diagnostics = [
        Diagnostic::new(Severity::Error, "1", Label::unlabelled(Span::new(0, 1))),
        Diagnostic::new(Severity::Warning, "2", Label::unlabelled(Span::new(2, 3)))
            .with_suggestion(fix("s").with_edit(Edit::insert(3, ";"))),
    ]
    .into_iter()
    .collect();
    let renderer = Renderer::new();
    let joined = diags
        .iter()
        .map(|d| renderer.render(d, &map))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(renderer.render_all(&diags, &map), joined);
}

#[test]
fn test_diagnostics_without_codes_or_suggestions_render_as_in_1_0() {
    // The 1.0 single-line layout, unchanged by the 1.1 additions.
    let src = "fn main() {\n    let x = foo();\n}\n";
    let map = one_file("main.rs", src);
    let diag = Diagnostic::new(
        Severity::Error,
        "cannot find value `foo` in this scope",
        Label::new(Span::new(24, 27), "not found in this scope"),
    );
    assert_eq!(
        render(&diag, &map),
        "\
error: cannot find value `foo` in this scope
 --> main.rs:2:13
  |
2 |     let x = foo();
  |             ^^^ not found in this scope
  |
"
    );
}
