//! The JSON output, read back by a strict RFC 8259 parser written here (no
//! third-party parser): every emitted document must parse, must contain exactly
//! the schema's keys in the schema's order, and must decode to exactly the
//! diagnostic it came from, whatever text the diagnostic carries. Hostile text
//! (quotes, backslashes, control characters, U+2028, braces) can therefore
//! never change the document's structure.

#![allow(clippy::unwrap_used)]

use diag_lang::{
    Applicability, Code, Diagnostic, Diagnostics, Edit, JSON_SCHEMA_VERSION, Label, Severity,
    SourceMap, Span, Suggestion,
};
use proptest::prelude::*;

// ---------------------------------------------------------------------------
// A strict JSON parser
// ---------------------------------------------------------------------------

/// A parsed JSON value. Objects keep their key order, so tests can check it;
/// numbers keep their literal text, validated against the RFC grammar.
#[derive(Clone, Debug, PartialEq)]
enum Json {
    Null,
    Bool(bool),
    Num(String),
    Str(String),
    Arr(Vec<Json>),
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, key: &str) -> &Json {
        match self {
            Json::Obj(fields) => {
                &fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .unwrap_or_else(|| panic!("missing key {key:?} in {self:?}"))
                    .1
            }
            other => panic!("not an object: {other:?}"),
        }
    }
    fn keys(&self) -> Vec<&str> {
        match self {
            Json::Obj(fields) => fields.iter().map(|(k, _)| k.as_str()).collect(),
            other => panic!("not an object: {other:?}"),
        }
    }
    fn str(&self) -> &str {
        match self {
            Json::Str(s) => s,
            other => panic!("not a string: {other:?}"),
        }
    }
    fn u64(&self) -> u64 {
        match self {
            Json::Num(n) => n.parse().unwrap_or_else(|_| panic!("not a u64: {n}")),
            other => panic!("not a number: {other:?}"),
        }
    }
    fn arr(&self) -> &[Json] {
        match self {
            Json::Arr(items) => items,
            other => panic!("not an array: {other:?}"),
        }
    }
    fn bool(&self) -> bool {
        match self {
            Json::Bool(b) => *b,
            other => panic!("not a bool: {other:?}"),
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
    depth: usize,
}

/// Parses `text` as exactly one JSON value (RFC 8259), rejecting anything the
/// grammar does not allow: trailing data, trailing commas, leading zeros, raw
/// control characters or invalid escapes in strings, lone surrogates, and —
/// stricter than the RFC requires — duplicate keys.
fn parse(text: &str) -> Result<Json, String> {
    let mut p = Parser {
        bytes: text.as_bytes(),
        at: 0,
        depth: 0,
    };
    p.ws();
    let value = p.value()?;
    p.ws();
    if p.at != p.bytes.len() {
        return Err(format!("trailing data at {}", p.at));
    }
    Ok(value)
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn bump(&mut self) -> Result<u8, String> {
        let b = self.peek().ok_or("unexpected end")?;
        self.at += 1;
        Ok(b)
    }
    fn expect(&mut self, b: u8) -> Result<(), String> {
        match self.bump()? {
            got if got == b => Ok(()),
            got => Err(format!(
                "expected {:?}, got {:?} at {}",
                b as char, got as char, self.at
            )),
        }
    }
    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }
    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.at..].starts_with(word.as_bytes()) {
            self.at += word.len();
            Ok(value)
        } else {
            Err(format!("bad literal at {}", self.at))
        }
    }
    fn value(&mut self) -> Result<Json, String> {
        match self.peek().ok_or("unexpected end")? {
            b'n' => self.literal("null", Json::Null),
            b't' => self.literal("true", Json::Bool(true)),
            b'f' => self.literal("false", Json::Bool(false)),
            b'"' => Ok(Json::Str(self.string()?)),
            b'[' => self.array(),
            b'{' => self.object(),
            b'-' | b'0'..=b'9' => self.number(),
            other => Err(format!("unexpected {:?} at {}", other as char, self.at)),
        }
    }
    fn nest(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("too deep".into());
        }
        Ok(())
    }
    fn array(&mut self) -> Result<Json, String> {
        self.nest()?;
        self.expect(b'[')?;
        let mut items = Vec::new();
        self.ws();
        if self.peek() == Some(b']') {
            self.at += 1;
        } else {
            loop {
                self.ws();
                items.push(self.value()?);
                self.ws();
                match self.bump()? {
                    b',' => continue,
                    b']' => break,
                    other => return Err(format!("bad array separator {:?}", other as char)),
                }
            }
        }
        self.depth -= 1;
        Ok(Json::Arr(items))
    }
    fn object(&mut self) -> Result<Json, String> {
        self.nest()?;
        self.expect(b'{')?;
        let mut fields: Vec<(String, Json)> = Vec::new();
        self.ws();
        if self.peek() == Some(b'}') {
            self.at += 1;
        } else {
            loop {
                self.ws();
                let key = self.string()?;
                if fields.iter().any(|(k, _)| *k == key) {
                    return Err(format!("duplicate key {key:?}"));
                }
                self.ws();
                self.expect(b':')?;
                self.ws();
                let value = self.value()?;
                fields.push((key, value));
                self.ws();
                match self.bump()? {
                    b',' => continue,
                    b'}' => break,
                    other => return Err(format!("bad object separator {:?}", other as char)),
                }
            }
        }
        self.depth -= 1;
        Ok(Json::Obj(fields))
    }
    fn number(&mut self) -> Result<Json, String> {
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.bump()? {
            b'0' => {}
            b'1'..=b'9' => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.at += 1;
                }
            }
            _ => return Err("bad number".into()),
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.digits()?;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.at]).unwrap();
        Ok(Json::Num(text.to_string()))
    }
    fn digits(&mut self) -> Result<(), String> {
        if !matches!(self.peek(), Some(b'0'..=b'9')) {
            return Err("expected a digit".into());
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.at += 1;
        }
        Ok(())
    }
    fn hex4(&mut self) -> Result<u32, String> {
        let mut n = 0;
        for _ in 0..4 {
            let d = (self.bump()? as char)
                .to_digit(16)
                .ok_or("bad hex digit in \\u escape")?;
            n = n * 16 + d;
        }
        Ok(n)
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = String::new();
        loop {
            let start = self.at;
            // A run of plain bytes; the input is a &str, so runs are valid UTF-8.
            while let Some(b) = self.peek() {
                if b == b'"' || b == b'\\' || b < 0x20 {
                    break;
                }
                self.at += 1;
            }
            out.push_str(std::str::from_utf8(&self.bytes[start..self.at]).unwrap());
            match self.bump()? {
                b'"' => return Ok(out),
                b'\\' => {
                    let c = match self.bump()? {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            let code = if (0xd800..0xdc00).contains(&hi) {
                                self.expect(b'\\')?;
                                self.expect(b'u')?;
                                let lo = self.hex4()?;
                                if !(0xdc00..0xe000).contains(&lo) {
                                    return Err("unpaired high surrogate".into());
                                }
                                0x10000 + ((hi - 0xd800) << 10) + (lo - 0xdc00)
                            } else if (0xdc00..0xe000).contains(&hi) {
                                return Err("unpaired low surrogate".into());
                            } else {
                                hi
                            };
                            char::from_u32(code).ok_or("bad code point")?
                        }
                        other => return Err(format!("bad escape \\{}", other as char)),
                    };
                    out.push(c);
                }
                b => return Err(format!("raw control byte {b:#x} in string")),
            }
        }
    }
}

/// Whether every byte outside string literals is structural (no whitespace):
/// the emitter is compact.
fn is_compact(text: &str) -> bool {
    let mut in_string = false;
    let mut escaped = false;
    for b in text.bytes() {
        if in_string {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == b'"' {
                in_string = false;
            }
        } else if b == b'"' {
            in_string = true;
        } else if b.is_ascii_whitespace() {
            return false;
        }
    }
    !in_string
}

#[test]
fn test_parser_rejects_what_the_rfc_forbids() {
    for bad in [
        "",
        "{",
        "[1,]",
        "{\"a\":1,}",
        "01",
        "-",
        "1.",
        "1e",
        "\"\u{1}\"",
        "\"\\x\"",
        "\"\\ud800\"",
        "\"\\udc00\"",
        "{\"a\":1,\"a\":2}",
        "nul",
        "1 2",
        "[1 2]",
        "\"abc",
        "{\"a\" 1}",
        "+1",
    ] {
        assert!(parse(bad).is_err(), "accepted {bad:?}");
    }
    for good in [
        "null",
        "true",
        "0",
        "-0.5e+3",
        "\"\\ud83d\\ude00\"",
        "[]",
        "{}",
        " [ 1 , {\"a\":[]} ] ",
    ] {
        assert!(parse(good).is_ok(), "rejected {good:?}");
    }
    assert_eq!(
        parse("\"\\ud83d\\ude00\\u00e9\\/\"").unwrap(),
        Json::Str("😀é/".into())
    );
}

// ---------------------------------------------------------------------------
// The schema
// ---------------------------------------------------------------------------

const DIAGNOSTIC_KEYS: [&str; 8] = [
    "version",
    "severity",
    "code",
    "message",
    "labels",
    "notes",
    "help",
    "suggestions",
];
const LABEL_KEYS: [&str; 4] = ["primary", "message", "span", "location"];
const LOCATION_KEYS: [&str; 3] = ["file", "start", "end"];
const POINT_KEYS: [&str; 3] = ["offset", "line", "column"];
const SUGGESTION_KEYS: [&str; 3] = ["message", "applicability", "edits"];
const EDIT_KEYS: [&str; 3] = ["span", "location", "replacement"];
const DOCUMENT_KEYS: [&str; 5] = ["version", "errors", "warnings", "omitted", "diagnostics"];

/// A reference for one end of a location in a single-source map: the offset
/// clamped to the text and floored to a character boundary, the line from
/// counting `\n`, and the column from counting characters.
fn reference_point(text: &str, at: usize) -> (u64, u64, u64) {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    let before = &text[..at];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap().chars().count() + 1;
    (at as u64, line as u64, column as u64)
}

fn point_of(json: &Json) -> (u64, u64, u64) {
    assert_eq!(json.keys(), POINT_KEYS);
    (
        json.get("offset").u64(),
        json.get("line").u64(),
        json.get("column").u64(),
    )
}

/// Checks `json` against `span` resolved in a single-source map named `file`
/// holding `text`.
fn check_span_and_location(json: &Json, span: Span, file: &str, text: &str) {
    let s = json.get("span");
    assert_eq!(s.keys(), ["start", "end"]);
    assert_eq!(s.get("start").u64(), u64::from(span.start().to_u32()));
    assert_eq!(s.get("end").u64(), u64::from(span.end().to_u32()));

    let start = span.start().to_usize();
    let location = json.get("location");
    if start > text.len() {
        assert_eq!(*location, Json::Null);
        return;
    }
    assert_eq!(location.keys(), LOCATION_KEYS);
    assert_eq!(location.get("file").str(), file);
    let lo = reference_point(text, start);
    let hi = reference_point(text, span.end().to_usize().max(start));
    assert_eq!(point_of(location.get("start")), lo);
    assert_eq!(point_of(location.get("end")), hi);
}

/// Checks that `json` is exactly `diag` serialised against a single-source
/// map (`file`, `text`).
fn check_diagnostic(json: &Json, diag: &Diagnostic, file: &str, text: &str) {
    assert_eq!(json.keys(), DIAGNOSTIC_KEYS);
    assert_eq!(json.get("version").u64(), u64::from(JSON_SCHEMA_VERSION));
    assert_eq!(json.get("severity").str(), diag.severity().as_str());
    match diag.code() {
        Some(code) => assert_eq!(json.get("code").str(), code.to_string()),
        None => assert_eq!(*json.get("code"), Json::Null),
    }
    assert_eq!(json.get("message").str(), diag.message());

    let labels = json.get("labels").arr();
    let expected: Vec<(bool, &Label)> = std::iter::once((true, diag.primary()))
        .chain(diag.secondary().iter().map(|l| (false, l)))
        .collect();
    assert_eq!(labels.len(), expected.len());
    for (json, (primary, label)) in labels.iter().zip(expected) {
        assert_eq!(json.keys(), LABEL_KEYS);
        assert_eq!(json.get("primary").bool(), primary);
        assert_eq!(json.get("message").str(), label.message());
        check_span_and_location(json, label.span(), file, text);
    }

    let strings = |j: &Json| {
        j.arr()
            .iter()
            .map(|s| s.str().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(strings(json.get("notes")), diag.notes().collect::<Vec<_>>());
    assert_eq!(strings(json.get("help")), diag.help().collect::<Vec<_>>());

    let suggestions = json.get("suggestions").arr();
    assert_eq!(suggestions.len(), diag.suggestions().len());
    for (json, suggestion) in suggestions.iter().zip(diag.suggestions()) {
        assert_eq!(json.keys(), SUGGESTION_KEYS);
        assert_eq!(json.get("message").str(), suggestion.message());
        assert_eq!(
            json.get("applicability").str(),
            suggestion.applicability().as_str()
        );
        let edits = json.get("edits").arr();
        assert_eq!(edits.len(), suggestion.edits().len());
        for (json, edit) in edits.iter().zip(suggestion.edits()) {
            assert_eq!(json.keys(), EDIT_KEYS);
            assert_eq!(json.get("replacement").str(), edit.replacement());
            check_span_and_location(json, edit.span(), file, text);
        }
    }
}

// ---------------------------------------------------------------------------
// Snapshots
// ---------------------------------------------------------------------------

#[test]
fn test_full_diagnostic_snapshot() {
    let text = "let x = cout;\n";
    let mut map = SourceMap::new();
    map.add("m.ls", text).unwrap();
    let diag = Diagnostic::new(
        Severity::Error,
        "unknown \"name\"",
        Label::new(Span::new(8, 12), "not found"),
    )
    .with_code(Code::new("E", 2001).unwrap())
    .with_secondary(Label::unlabelled(Span::new(4, 5)))
    .with_note("n1")
    .with_help("h1")
    .with_suggestion(
        Suggestion::new("did you mean `count`?", Applicability::MaybeIncorrect)
            .with_edit(Edit::new(Span::new(8, 12), "count")),
    );

    let expected = concat!(
        r#"{"version":1,"severity":"error","code":"E2001","message":"unknown \"name\"","#,
        r#""labels":["#,
        r#"{"primary":true,"message":"not found","span":{"start":8,"end":12},"#,
        r#""location":{"file":"m.ls","start":{"offset":8,"line":1,"column":9},"end":{"offset":12,"line":1,"column":13}}},"#,
        r#"{"primary":false,"message":"","span":{"start":4,"end":5},"#,
        r#""location":{"file":"m.ls","start":{"offset":4,"line":1,"column":5},"end":{"offset":5,"line":1,"column":6}}}"#,
        r#"],"notes":["n1"],"help":["h1"],"#,
        r#""suggestions":[{"message":"did you mean `count`?","applicability":"maybe_incorrect","#,
        r#""edits":[{"span":{"start":8,"end":12},"#,
        r#""location":{"file":"m.ls","start":{"offset":8,"line":1,"column":9},"end":{"offset":12,"line":1,"column":13}},"#,
        r#""replacement":"count"}]}]}"#,
    );
    assert_eq!(diag.to_json(&map), expected);
    check_diagnostic(&parse(expected).unwrap(), &diag, "m.ls", text);
}

#[test]
fn test_locations_in_a_multi_source_map_are_per_file() {
    let mut map = SourceMap::new();
    map.add("a.ls", "one\ntwo\n").unwrap(); // 0..8
    map.add("b.ls", "x = 1\ny = 2").unwrap(); // 8..19
    let diag = Diagnostic::new(
        Severity::Warning,
        "w",
        Label::new(Span::new(14, 15), "in b"),
    )
    .with_secondary(Label::new(Span::new(4, 7), "in a"))
    // The end of the last source, and a span from a into b clamped to a.
    .with_secondary(Label::unlabelled(Span::empty(19)))
    .with_secondary(Label::unlabelled(Span::new(6, 10)));

    let json = parse(&diag.to_json(&map)).unwrap();
    let labels = json.get("labels").arr();
    let loc = |i: usize| {
        let l = labels[i].get("location");
        (
            l.get("file").str().to_string(),
            point_of(l.get("start")),
            point_of(l.get("end")),
        )
    };
    assert_eq!(loc(0), ("b.ls".into(), (6, 2, 1), (7, 2, 2)));
    assert_eq!(loc(1), ("a.ls".into(), (4, 2, 1), (7, 2, 4)));
    assert_eq!(loc(2), ("b.ls".into(), (11, 2, 6), (11, 2, 6)));
    assert_eq!(loc(3), ("a.ls".into(), (6, 2, 3), (8, 3, 1)));
}

#[test]
fn test_unlocatable_spans_have_null_locations() {
    let diag = Diagnostic::new(Severity::Error, "e", Label::unlabelled(Span::new(5, 9)))
        .with_suggestion(
            Suggestion::new("s", Applicability::Unspecified).with_edit(Edit::insert(7, "x")),
        );
    let out = diag.to_json(&SourceMap::new());
    assert_eq!(
        out,
        concat!(
            r#"{"version":1,"severity":"error","code":null,"message":"e","labels":[{"primary":true,"#,
            r#""message":"","span":{"start":5,"end":9},"location":null}],"notes":[],"help":[],"#,
            r#""suggestions":[{"message":"s","applicability":"unspecified","edits":[{"span":{"start":7,"end":7},"#,
            r#""location":null,"replacement":"x"}]}]}"#,
        )
    );
}

#[test]
fn test_hostile_text_cannot_change_the_structure() {
    let hostile =
        "\"},{\"severity\":\"note\"}]}\\\u{0}\u{1b}[2J\n\r\t\u{2028}\u{2029}\u{7f}\u{ffff}😀";
    let mut map = SourceMap::new();
    map.add(hostile, "abc").unwrap();
    let diag = Diagnostic::new(
        Severity::Error,
        hostile,
        Label::new(Span::new(0, 1), hostile),
    )
    .with_note(hostile)
    .with_help(hostile)
    .with_suggestion(
        Suggestion::new(hostile, Applicability::MachineApplicable)
            .with_edit(Edit::new(Span::new(1, 2), hostile)),
    );
    let out = diag.to_json(&map);
    assert!(is_compact(&out));
    assert!(!out.contains('\u{2028}') && !out.contains('\u{2029}'));
    assert!(out.bytes().all(|b| b >= 0x20));
    check_diagnostic(&parse(&out).unwrap(), &diag, hostile, "abc");
}

#[test]
fn test_document_counts_and_order() {
    let mut map = SourceMap::new();
    map.add("f", "abcdef").unwrap();
    let mut diags = Diagnostics::with_limit(2);
    for (sev, at) in [
        (Severity::Warning, 4),
        (Severity::Error, 1),
        (Severity::Error, 3),
        (Severity::Note, 0),
    ] {
        diags.push(Diagnostic::new(
            sev,
            "m",
            Label::unlabelled(Span::empty(at)),
        ));
    }
    diags.sort();

    let out = diags.to_json(&map);
    assert!(is_compact(&out));
    let json = parse(&out).unwrap();
    assert_eq!(json.keys(), DOCUMENT_KEYS);
    assert_eq!(json.get("version").u64(), 1);
    assert_eq!(json.get("errors").u64(), 2);
    assert_eq!(json.get("warnings").u64(), 1);
    assert_eq!(json.get("omitted").u64(), 2);
    let items = json.get("diagnostics").arr();
    assert_eq!(items.len(), 2);
    for (item, diag) in items.iter().zip(diags.iter()) {
        check_diagnostic(item, diag, "f", "abcdef");
    }
    // The stored diagnostics, sorted: the error at 1, then the warning at 4.
    assert_eq!(items[0].get("severity").str(), "error");
    assert_eq!(items[1].get("severity").str(), "warning");
}

#[test]
fn test_json_lines_streaming() {
    let map = SourceMap::new();
    let mut stream = String::new();
    for i in 0..3 {
        let diag = Diagnostic::new(
            Severity::Error,
            format!("e{i}"),
            Label::unlabelled(Span::empty(i)),
        );
        diag.write_json(&mut stream, &map).unwrap();
        stream.push('\n');
    }
    let lines: Vec<&str> = stream.lines().collect();
    assert_eq!(lines.len(), 3);
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(parse(line).unwrap().get("message").str(), format!("e{i}"));
    }
}

// ---------------------------------------------------------------------------
// Properties
// ---------------------------------------------------------------------------

fn severity() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Error),
        Just(Severity::Warning),
        Just(Severity::Note),
        Just(Severity::Help),
    ]
}

fn applicability() -> impl Strategy<Value = Applicability> {
    prop_oneof![
        Just(Applicability::MachineApplicable),
        Just(Applicability::MaybeIncorrect),
        Just(Applicability::HasPlaceholders),
        Just(Applicability::Unspecified),
    ]
}

/// Text that stresses the escaper: quotes, backslashes, every control
/// character, the JavaScript line separators, braces, and non-ASCII.
fn nasty() -> impl Strategy<Value = String> {
    proptest::collection::vec(
        prop_oneof![
            Just('"'),
            Just('\\'),
            Just('{'),
            Just('}'),
            Just(','),
            Just(':'),
            Just('\u{2028}'),
            Just('\u{2029}'),
            Just('\u{7f}'),
            (0u32..0x20).prop_map(|c| char::from_u32(c).unwrap()),
            any::<char>(),
        ],
        0..12,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

fn code() -> impl Strategy<Value = Option<Code>> {
    proptest::option::of(("[A-Z]{1,4}", 0u16..=9999).prop_map(|(p, n)| Code::new(&p, n).unwrap()))
}

prop_compose! {
    fn diagnostic(margin: u32)(
        sev in severity(),
        code in code(),
        message in nasty(),
        primary in (0..margin, 0..margin, nasty()),
        secondary in proptest::collection::vec((0..margin, 0..margin, nasty()), 0..3),
        notes in proptest::collection::vec(nasty(), 0..3),
        help in proptest::collection::vec(nasty(), 0..3),
        suggestions in proptest::collection::vec(
            (nasty(), applicability(), proptest::collection::vec((0..margin, 0..margin, nasty()), 0..3)),
            0..3,
        ),
    ) -> Diagnostic {
        let mut d = Diagnostic::new(sev, message, Label::new(Span::new(primary.0, primary.1), primary.2));
        if let Some(code) = code {
            d = d.with_code(code);
        }
        for (a, b, m) in secondary {
            d = d.with_secondary(Label::new(Span::new(a, b), m));
        }
        for n in notes {
            d = d.with_note(n);
        }
        for h in help {
            d = d.with_help(h);
        }
        for (m, applicability, edits) in suggestions {
            let mut s = Suggestion::new(m, applicability);
            for (a, b, r) in edits {
                s = s.with_edit(Edit::new(Span::new(a, b), r));
            }
            d = d.with_suggestion(s);
        }
        d
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Every diagnostic serialises to compact JSON that a strict parser
    /// accepts, with exactly the schema's keys, decoding to exactly the
    /// diagnostic: strings round-trip byte for byte and every location agrees
    /// with a naive line/column count.
    #[test]
    fn json_round_trips_through_a_strict_parser(
        text in "(?s).{0,40}",
        name in nasty(),
        diag in diagnostic(48),
    ) {
        let mut map = SourceMap::new();
        map.add(name.as_str(), text.as_str()).unwrap();
        let out = diag.to_json(&map);
        prop_assert!(is_compact(&out));
        prop_assert!(out.bytes().all(|b| b >= 0x20));
        let json = parse(&out).map_err(TestCaseError::fail)?;
        check_diagnostic(&json, &diag, &name, &text);
        // Deterministic: the same input writes the same bytes.
        prop_assert_eq!(out, diag.to_json(&map));
    }

    /// A collection document parses and holds each stored diagnostic exactly.
    #[test]
    fn document_round_trips(
        text in "(?s).{0,20}",
        diags in proptest::collection::vec(diagnostic(24), 0..6),
        limit in 0usize..6,
    ) {
        let mut map = SourceMap::new();
        map.add("f", text.as_str()).unwrap();
        let mut collected = Diagnostics::with_limit(limit);
        collected.extend(diags);
        let out = collected.to_json(&map);
        let json = parse(&out).map_err(TestCaseError::fail)?;
        prop_assert_eq!(json.keys(), DOCUMENT_KEYS);
        prop_assert_eq!(json.get("errors").u64(), collected.error_count() as u64);
        prop_assert_eq!(json.get("warnings").u64(), collected.warning_count() as u64);
        prop_assert_eq!(json.get("omitted").u64(), collected.omitted() as u64);
        let items = json.get("diagnostics").arr();
        prop_assert_eq!(items.len(), collected.len());
        for (item, diag) in items.iter().zip(collected.iter()) {
            check_diagnostic(item, diag, "f", &text);
        }
    }
}
