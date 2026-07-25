//! Error recovery: a mistake in one section still yields an AST for the rest —
//! the property the LSP relies on for partial ASTs on every keystroke.

use mml_parser::analyze;

#[test]
fn malformed_use_import_does_not_lose_following_struct() {
    // A `use` line with no colon is malformed, yet the struct that follows it
    // must still parse (and resolve the imported `Offer`).
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n\
## data\n\
use terrain-erosion/mesh-terrain-chunks\n\
struct Batch {\n  item: Offer,\n}\n\
in: Batch\nout: Batch\n\n\
## behavior\n- do it\n\n## invariants\n";
    let (doc, diags) = analyze(src);

    // The malformed import was reported.
    assert!(
        diags.iter().any(|d| d.code == "malformed-import"),
        "expected a malformed-import diagnostic, got: {diags:#?}"
    );
    // Recovery: the import is still recorded, and the struct after it survived.
    assert_eq!(doc.data.imports.len(), 1);
    assert_eq!(doc.data.types.len(), 1);
    assert_eq!(doc.data.types[0].name().value, "Batch");
}

#[test]
fn broken_data_block_still_yields_modules_and_behavior() {
    // The `## data` struct is unterminated (no closing `}`), yet modules and
    // behavior must still parse.
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\
Alpha — the first module\n\
Beta — the second module\n\n\
## data\n\
struct Broken {\n  x: String,\n\
in: String\nout: String\n\n\
## behavior\n\
- step one\n\
- step two\n\n\
## invariants\n\
- something holds\n";
    let (doc, diags) = analyze(src);

    // Recovery: both modules survived.
    assert_eq!(doc.modules.len(), 2);
    assert_eq!(doc.modules[0].name.value, "Alpha");
    assert_eq!(doc.modules[1].name.value, "Beta");

    // Recovery: behavior survived.
    assert_eq!(doc.behavior.len(), 2);
    assert_eq!(doc.behavior[0].text.value, "step one");

    // Recovery: invariant survived.
    assert_eq!(doc.invariants.len(), 1);

    // The data error was reported.
    assert!(
        diags.iter().any(|d| d.code == "unterminated-block"),
        "expected an unterminated-block diagnostic, got: {diags:#?}"
    );
}

#[test]
fn malformed_spec_ref_keeps_following_module() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\
Alpha — the first module @ spec not a ref\n\
Beta — the second module @ spec other-system/other-unit\n\n\
## data\nin: String\nout: String\n\n\
## behavior\n- step one\n\n\
## invariants\n- something holds\n";
    let (doc, diags) = analyze(src);

    assert_eq!(doc.modules.len(), 2);
    assert_eq!(doc.modules[0].name.value, "Alpha");
    assert_eq!(doc.modules[0].spec, None);
    assert_eq!(
        doc.modules[1].spec.as_ref().unwrap().value,
        "other-system/other-unit"
    );
    assert!(
        diags.iter().any(|d| d.code == "malformed-spec-ref"),
        "expected a malformed-spec-ref diagnostic, got: {diags:#?}"
    );
}

#[test]
fn missing_all_sections_still_parses_header() {
    let src = "system: bevy-survivor\nunit: thing\nkind: endpoint\n";
    let (doc, diags) = analyze(src);
    assert_eq!(doc.header.system.as_ref().unwrap().value, "bevy-survivor");
    assert_eq!(doc.header.unit.as_ref().unwrap().value, "thing");
    // Four missing-section diagnostics.
    assert_eq!(
        diags.iter().filter(|d| d.code == "missing-section").count(),
        4
    );
}

#[test]
fn empty_input_does_not_panic() {
    let (_doc, diags) = analyze("");
    // Missing everything, but no panic and diagnostics are produced.
    assert!(!diags.is_empty());
}
