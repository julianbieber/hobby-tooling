//! One fixture per diagnostic code, asserting the exact code and span.

use mml_parser::diagnostic::code;
use mml_parser::{Diagnostic, Severity, analyze};

/// The single diagnostic with the given code (panics if zero or many).
fn only(src: &str, wanted: &str) -> Diagnostic {
    let (_doc, diags) = analyze(src);
    let matches: Vec<_> = diags.iter().filter(|d| d.code == wanted).collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one `{wanted}` diagnostic, got: {diags:#?}"
    );
    matches[0].clone()
}

/// Assert the diagnostic's span covers exactly `expected` in `src`.
fn assert_span(src: &str, d: &Diagnostic, expected: &str) {
    assert_eq!(
        &src[d.span.start..d.span.end],
        expected,
        "span mismatch for `{}`",
        d.code
    );
}

const VALID_HEADER: &str = "system: s\nunit: u\nkind: job\n\n";

#[test]
fn undefined_type() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
struct A {\n  x: Bogus,\n}\n\
in: A\nout: A\n\n\
## behavior\n- do it\n\n\
## invariants\n";
    let d = only(src, code::UNDEFINED_TYPE);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "Bogus");
}

#[test]
fn undefined_type_inside_generic() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
struct A {\n  xs: Vec<Bogus>,\n}\n\
in: A\nout: A\n\n\
## behavior\n- do it\n\n\
## invariants\n";
    let d = only(src, code::UNDEFINED_TYPE);
    assert_span(src, &d, "Bogus");
}

#[test]
fn duplicate_field() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
struct A {\n  x: String,\n  x: i64,\n}\n\
in: A\nout: A\n\n\
## behavior\n- do it\n\n\
## invariants\n";
    let d = only(src, code::DUPLICATE_FIELD);
    // The span points at the *second* `x`.
    assert_span(src, &d, "x");
    let second_x = src.rfind("x:").unwrap();
    assert_eq!(d.span.start, second_x);
}

#[test]
fn duplicate_type() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
struct A {\n  x: String,\n}\n\
struct A {\n  y: i64,\n}\n\
in: A\nout: A\n\n\
## behavior\n- do it\n\n\
## invariants\n";
    let d = only(src, code::DUPLICATE_TYPE);
    assert_span(src, &d, "A");
    // Second definition, i.e. after the first `struct A`.
    let first = src.find("struct A").unwrap();
    assert!(d.span.start > first + "struct A".len());
}

#[test]
fn duplicate_module() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\
Foo — first\n\
Foo — second\n\n\
## data\nin: String\nout: String\n\n\
## behavior\n- do it\n\n\
## invariants\n";
    let d = only(src, code::DUPLICATE_MODULE);
    assert_span(src, &d, "Foo");
    let second = src.rfind("Foo").unwrap();
    assert_eq!(d.span.start, second);
}

#[test]
fn missing_header_key() {
    // No `unit`.
    let src = "system: s\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MISSING_HEADER_KEY);
    assert!(d.message.contains("unit"), "message: {}", d.message);
}

#[test]
fn bad_kind() {
    let src = "system: s\nunit: u\nkind: banana\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::BAD_KIND);
    assert_span(src, &d, "banana");
}

#[test]
fn malformed_struct_missing_name() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
struct {\n  x: String,\n}\n\
in: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MALFORMED_STRUCT);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn malformed_field_no_colon() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
struct A {\n  justtext\n}\n\
in: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MALFORMED_FIELD);
    assert_span(src, &d, "justtext");
}

#[test]
fn tab_indent() {
    let src = format!(
        "{VALID_HEADER}## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- outer\n\t- tabbed child\n\n## invariants\n"
    );
    let d = only(&src, code::TAB_INDENT);
    assert_eq!(d.severity, Severity::Error);
    // Span covers the leading tab.
    assert_eq!(&src[d.span.start..d.span.end], "\t");
}

#[test]
fn missing_section() {
    // Omit `## invariants`.
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n## behavior\n- do it\n";
    let d = only(src, code::MISSING_SECTION);
    assert!(d.message.contains("invariants"), "message: {}", d.message);
}

#[test]
fn dangling_else() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do a thing\n- else:\n  - fallback\n\n## invariants\n";
    let d = only(src, code::DANGLING_ELSE);
    assert_span(src, &d, "else:");
}

#[test]
fn bad_method_value() {
    let src = "system: s\nunit: u\nkind: endpoint\nmethod: FETCH\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::BAD_METHOD);
    assert_span(src, &d, "FETCH");
}

#[test]
fn method_on_non_endpoint_is_flagged() {
    // Valid verb, wrong kind → exactly one BAD_METHOD (the endpoint-only warning).
    let src = "system: s\nunit: u\nkind: job\nmethod: GET\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::BAD_METHOD);
    assert_eq!(d.severity, Severity::Warning);
}

#[test]
fn bad_param_source() {
    let src = "system: s\nunit: u\nkind: endpoint\n\n\
## modules\n\n## data\nstruct Req {\n  a: String @sideways,\n}\nin: Req\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::BAD_PARAM_SOURCE);
    assert_span(src, &d, "sideways");
}

#[test]
fn malformed_spec_ref_empty() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it @ spec\n\n## invariants\n";
    let d = only(src, code::MALFORMED_SPEC_REF);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "@ spec");
}

#[test]
fn malformed_spec_ref_with_spaces() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it @ spec foo bar\n\n## invariants\n";
    let d = only(src, code::MALFORMED_SPEC_REF);
    assert_span(src, &d, "foo bar");
}

#[test]
fn malformed_spec_ref_two_slashes() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it @ spec a/b/c\n\n## invariants\n";
    let d = only(src, code::MALFORMED_SPEC_REF);
    assert_span(src, &d, "a/b/c");
}

#[test]
fn malformed_spec_ref_bad_segment() {
    // A segment must start with a letter.
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it @ spec sys/-bad\n\n## invariants\n";
    let d = only(src, code::MALFORMED_SPEC_REF);
    assert_span(src, &d, "sys/-bad");
}

#[test]
fn duplicate_spec_ref() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it @ spec first @ spec second\n\n## invariants\n";
    let d = only(src, code::DUPLICATE_SPEC_REF);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "@ spec");
    // The last ref wins.
    let (doc, _) = analyze(src);
    assert_eq!(doc.behavior[0].spec.as_ref().unwrap().value, "second");
}

#[test]
fn spec_marker_in_prose_is_not_an_annotation() {
    // `@ spec` only matches on word boundaries: neither `spec-shaped` nor a
    // path like `@ spec_utils/x.py` is an annotation.
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\nFoo — reads @ spec_utils/x.py\n\n\
## data\nin: String\nout: String\n\n\
## behavior\n- compare against the @ spec-shaped baseline\n\n## invariants\n";
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    assert_eq!(doc.modules[0].spec, None);
    assert_eq!(
        doc.modules[0].path.as_ref().unwrap().value,
        "spec_utils/x.py"
    );
    assert_eq!(doc.behavior[0].spec, None);
    assert_eq!(
        doc.behavior[0].text.value,
        "compare against the @ spec-shaped baseline"
    );
}

#[test]
fn malformed_import_no_colon() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nuse foo/bar\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MALFORMED_IMPORT);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "use foo/bar");
}

#[test]
fn malformed_import_empty_type_list() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nuse foo/bar:\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MALFORMED_IMPORT);
    assert_span(src, &d, "use foo/bar:");
}

#[test]
fn malformed_import_bad_ref() {
    // A ref segment must start with a letter (validate: is_valid_spec_ref).
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nuse sys/-bad: Offer\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MALFORMED_IMPORT);
    assert_span(src, &d, "sys/-bad");
}

#[test]
fn malformed_import_non_pascal_type() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nuse foo/bar: offer\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::MALFORMED_IMPORT);
    assert_span(src, &d, "offer");
}

#[test]
fn duplicate_import_same_type_twice() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nuse a/b: Offer\nuse c/d: Offer\nin: String\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::DUPLICATE_IMPORT);
    assert_eq!(d.severity, Severity::Error);
    // The span points at the second (offending) import.
    let second = src.rfind("Offer").unwrap();
    assert_eq!(d.span.start, second);
}

#[test]
fn duplicate_import_collides_with_local_type() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nuse a/b: Foo\nstruct Foo {\n  x: String,\n}\nin: Foo\nout: Foo\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::DUPLICATE_IMPORT);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "Foo"); // the imported one
}

#[test]
fn malformed_binding_no_equals() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n- do it\nwhere:\n  X 4\n\n## invariants\n";
    let d = only(src, code::MALFORMED_BINDING);
    assert_eq!(d.severity, Severity::Error);
}
