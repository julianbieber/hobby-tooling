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

// ---- Bevy dialect (grammar §7.2, §8) ---------------------------------------

/// A minimal well-formed `kind: plugin` spec with `body` spliced into the
/// `## systems` section, so a test can vary one entry and nothing else.
fn plugin_with_systems(systems: &str) -> String {
    format!(
        "system: s\nunit: u\nkind: plugin\n\n\
## components\nmarker Foe\n\n\
## resources\nstruct Cfg {{\n  n: u32,\n}}\n\n\
## events\nstruct Cleared {{\n  n: u32,\n}}\n\n\
## systems\n{systems}\n\n\
## behavior\n- do it\n\n\
## invariants\n- always\n"
    )
}

#[test]
fn data_section_declares_role_less_types_in_a_plugin() {
    // Grammar §4.6: a Bevy kind may declare the shapes that are neither
    // component nor resource nor event, and use them as field types.
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## data\nenum Pattern {\n  ring,\n  line,\n}\n\n\
## components\nstruct Spawn {\n  pattern: Pattern,\n}\n\n\
## systems\ngo — does it\n  schedule: Update\n  query: Spawn\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let (doc, diags) = analyze(src);
    assert!(diags.is_empty(), "{diags:#?}");
    assert_eq!(doc.data.types.len(), 1);
}

#[test]
fn in_out_are_rejected_in_a_bevy_data_section() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## data\nin: String\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MALFORMED_STRUCT);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "in: String");
    assert!(d.message.contains("access block"), "{}", d.message);
    // Rejected, not recorded: a plugin has no input type.
    let (doc, _) = analyze(src);
    assert!(doc.data.input.is_none());
}

#[test]
fn marker_in_data_points_at_components() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## data\nmarker Foe\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MALFORMED_MARKER);
    assert_span(src, &d, "marker Foe");
    assert!(d.message.contains("## components"), "{}", d.message);
}

#[test]
fn a_data_type_named_in_an_access_list_is_a_role_mismatch() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## data\nstruct Pattern {\n  n: u32,\n}\n\n\
## systems\ngo — does it\n  schedule: Update\n  query: Pattern\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::ACCESS_ROLE_MISMATCH);
    assert_eq!(d.severity, Severity::Error);
    assert_span(src, &d, "Pattern");
    assert!(d.message.contains("`## data`"), "{}", d.message);
    assert!(d.message.contains("## components"), "{}", d.message);
}

#[test]
fn a_name_in_both_data_and_a_world_section_is_a_duplicate() {
    // One type namespace across the whole file, so a shape cannot be role-less
    // and a component at once.
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## data\nstruct Foe {\n  n: u32,\n}\n\n\
## components\nmarker Foe\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::DUPLICATE_TYPE);
    assert_span(src, &d, "Foe");
}

#[test]
fn systems_section_is_not_allowed_in_a_job() {
    let src = "system: s\nunit: u\nkind: job\n\n\
## modules\n\n## data\nin: String\nout: String\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::SECTION_NOT_ALLOWED);
    assert_span(src, &d, "## systems");
}

#[test]
fn plugin_without_systems_section_is_missing_it() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## components\nmarker Foe\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MISSING_SECTION);
    assert_eq!(d.severity, Severity::Error);
    assert!(d.message.contains("systems"), "{}", d.message);
    assert!(d.message.contains("kind: plugin"), "{}", d.message);
}

#[test]
fn system_kind_requires_an_access_section() {
    let src = "system: s\nunit: u\nkind: system\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MISSING_SECTION);
    assert!(d.message.contains("access"), "{}", d.message);
}

#[test]
fn plugin_keeps_the_four_classic_sections_optional() {
    // `## modules` is allowed but not required for a plugin, and its absence
    // must not resurrect the classic schema's `## data` requirement.
    let (_doc, diags) = analyze(&plugin_with_systems("go — does it\n  schedule: Update"));
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
}

#[test]
fn malformed_system_without_separator() {
    let src = plugin_with_systems("go does it\n  schedule: Update");
    let d = only(&src, code::MALFORMED_SYSTEM);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "go does it");
}

#[test]
fn system_name_not_snake_case_warns() {
    let src = plugin_with_systems("SpawnEnemies — does it\n  schedule: Update");
    let d = only(&src, code::SYSTEM_NAME_NOT_SNAKE);
    assert_eq!(d.severity, Severity::Warning);
    assert_span(&src, &d, "SpawnEnemies");
}

#[test]
fn duplicate_system_name() {
    let src = plugin_with_systems(
        "go — does it\n  schedule: Update\ngo — does it again\n  schedule: Update",
    );
    let d = only(&src, code::DUPLICATE_SYSTEM);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn missing_schedule_on_a_system() {
    let src = plugin_with_systems("go — does it\n  reads: Cfg");
    let d = only(&src, code::MISSING_SCHEDULE);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "go — does it");
}

#[test]
fn missing_schedule_in_an_access_section() {
    let src = "system: s\nunit: u\nkind: system\n\n\
## access\nreads: Time\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MISSING_SCHEDULE);
    assert!(d.message.contains("## access"), "{}", d.message);
}

#[test]
fn unknown_schedule_warns_but_state_schedules_do_not() {
    let src = plugin_with_systems("go — does it\n  schedule: Whenever");
    let d = only(&src, code::UNKNOWN_SCHEDULE);
    assert_eq!(d.severity, Severity::Warning);
    assert_span(&src, &d, "Whenever");

    // `OnEnter(..)` / `OnExit(..)` are recognised by shape.
    for label in ["OnEnter(Playing)", "OnExit(Paused)", "FixedPostUpdate"] {
        let src = plugin_with_systems(&format!("go — does it\n  schedule: {label}"));
        let (_doc, diags) = analyze(&src);
        assert!(diags.is_empty(), "{label} should be clean: {diags:#?}");
    }
}

#[test]
fn unknown_access_key() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  mutates: Cfg");
    let d = only(&src, code::UNKNOWN_ACCESS_KEY);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "mutates");
}

#[test]
fn duplicate_access_key() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  reads: Cfg\n  reads: Cfg");
    let d = only(&src, code::DUPLICATE_ACCESS_KEY);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn access_attribute_without_a_system_entry() {
    let src = plugin_with_systems("  schedule: Update");
    let d = only(&src, code::MALFORMED_ACCESS);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn undefined_access_item() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  reads: Nonsense");
    let d = only(&src, code::UNDEFINED_ACCESS_ITEM);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "Nonsense");
}

#[test]
fn builtin_bevy_names_need_no_declaration() {
    let src = plugin_with_systems(
        "go — does it\n  schedule: Update\n  query: Transform + Foe\n  reads: Time\n  emits: AppExit",
    );
    let (_doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
}

#[test]
fn access_role_mismatch_emitting_a_component() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  emits: Foe");
    let d = only(&src, code::ACCESS_ROLE_MISMATCH);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "Foe");
    assert!(d.message.contains("component"), "{}", d.message);
}

#[test]
fn access_role_mismatch_querying_a_resource() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  query: Cfg");
    let d = only(&src, code::ACCESS_ROLE_MISMATCH);
    assert!(d.message.contains("resource"), "{}", d.message);
}

#[test]
fn reads_accepts_both_components_and_resources() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  reads: Foe, Cfg");
    let (_doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
}

#[test]
fn unresolved_system_ref_warns_in_a_plugin() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  after: nobody");
    let d = only(&src, code::UNRESOLVED_SYSTEM_REF);
    assert_eq!(d.severity, Severity::Warning);
    assert_span(&src, &d, "nobody");
}

#[test]
fn ordering_against_a_declared_set_or_system_is_silent() {
    let src = plugin_with_systems(
        "first — runs first\n  schedule: Update\n  set: Early\n\
         second — runs after\n  schedule: Update\n  after: first, Early",
    );
    let (_doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
}

#[test]
fn a_system_kind_may_order_against_specs_elsewhere() {
    // A standalone `kind: system` file has no local system list, so its
    // ordering refs are not checked.
    let src = "system: s\nunit: u\nkind: system\n\n\
## access\nschedule: Update\nafter: some_other_plugins_system\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let (_doc, diags) = analyze(src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
}

#[test]
fn query_filter_splits_with_and_without() {
    let src = plugin_with_systems("go — does it\n  schedule: Update\n  query: Foe - Cleared");
    // `Cleared` is an event, so excluding it is still a role mismatch — the
    // point here is that the `-` term is parsed as its own item.
    let d = only(&src, code::ACCESS_ROLE_MISMATCH);
    assert_span(&src, &d, "Cleared");
}

#[test]
fn marker_outside_components_is_rejected() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## resources\nmarker Foe\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MALFORMED_MARKER);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn in_out_lines_are_rejected_in_a_world_section() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## components\nin: String\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::MALFORMED_STRUCT);
    assert!(d.message.contains("access block"), "{}", d.message);
}

#[test]
fn a_name_declared_in_two_world_sections_is_a_duplicate() {
    let src = "system: s\nunit: u\nkind: plugin\n\n\
## components\nmarker Foe\n\n\
## resources\nstruct Foe {\n  n: u32,\n}\n\n\
## systems\ngo — does it\n  schedule: Update\n\n\
## behavior\n- do it\n\n## invariants\n- always\n";
    let d = only(src, code::DUPLICATE_TYPE);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn plugin_is_offered_as_a_kind() {
    let src = "system: s\nunit: u\nkind: nonsense\n\n\
## behavior\n- do it\n\n## invariants\n";
    let d = only(src, code::BAD_KIND);
    assert!(d.message.contains("plugin"), "{}", d.message);
    assert!(d.message.contains("system"), "{}", d.message);
}

// ---- Rust references (grammar §4.7) and extern imports (§4.8) --------------

/// A `kind: plugin` spec body with `sections` spliced in.
fn plugin(sections: &str) -> String {
    format!(
        "system: s\nunit: u\nkind: plugin\n\n{sections}\n## systems\n\n## behavior\n- x\n\n## invariants\n"
    )
}

#[test]
fn a_reference_may_omit_its_body_entirely() {
    let src = plugin("## components\nstruct Wave @ src/waves.rs\nmarker Spawner @ src/waves.rs\n");
    let (doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    let wave = &doc.components.types[0];
    assert!(wave.is_reference());
    assert_eq!(wave.path().unwrap().value, "src/waves.rs");
    match wave {
        mml_parser::ast::TypeDef::Struct { fields, .. } => assert!(fields.is_none()),
        _ => panic!("Wave should be a struct"),
    }
}

#[test]
fn a_body_without_a_path_is_still_required() {
    let src = plugin("## components\nstruct Wave\n");
    let d = only(&src, code::MISSING_TYPE_BODY);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "Wave");
}

#[test]
fn a_type_points_at_code_not_at_a_spec() {
    let src = plugin("## components\nstruct Wave @ spec other\n");
    let d = only(&src, code::MALFORMED_TYPE_PATH);
    assert_eq!(d.severity, Severity::Error);
}

#[test]
fn deltas_parse_on_a_reference() {
    use mml_parser::ast::Delta;
    let src = plugin(
        "## components\nstruct Wave @ src/waves.rs {\n  index: u32,\n  + streak: u32,\n  - remaining,\n}\n",
    );
    let (doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    match &doc.components.types[0] {
        mml_parser::ast::TypeDef::Struct { fields, .. } => {
            let fields = fields.as_ref().unwrap();
            let got: Vec<(&str, Delta)> = fields
                .iter()
                .map(|f| (f.name.value.as_str(), f.delta))
                .collect();
            assert_eq!(
                got,
                [
                    ("index", Delta::Keep),
                    ("streak", Delta::Add),
                    ("remaining", Delta::Remove),
                ]
            );
            // A removal needs no type; the field being removed already has one.
            assert!(fields[2].ty.is_none());
        }
        _ => panic!("Wave should be a struct"),
    }
}

#[test]
fn a_delta_on_an_owned_type_is_rejected() {
    let src = plugin("## components\nstruct Wave {\n  + index: u32,\n}\n");
    let d = only(&src, code::MALFORMED_FIELD);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "+");
}

#[test]
fn extern_import_brings_a_crate_type_into_scope_with_the_sections_role() {
    let src = plugin(
        "## components\nuse extern avian3d::prelude: Collider\n\n## resources\nstruct Cfg {\n  hit: Collider,\n}\n",
    );
    let (doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "unexpected diagnostics: {diags:#?}");
    let imp = &doc.components.extern_imports[0];
    assert_eq!(imp.path.value, "avian3d::prelude");
    assert_eq!(imp.crate_name(), "avian3d");
    assert_eq!(
        imp.docs_url("Collider"),
        "https://docs.rs/avian3d/latest/avian3d/?search=Collider"
    );
    // The role came from the section, so it is queryable like any component.
    assert_eq!(
        doc.roles().get("Collider"),
        Some(&mml_parser::ast::Role::Component)
    );
}

#[test]
fn a_unit_named_extern_is_still_a_spec_import() {
    // `use extern: T` has no second token, so the keyword does not steal a
    // legal unit name.
    let src = plugin("## components\nuse extern: Wave\n");
    let (doc, _diags) = analyze(&src);
    assert_eq!(doc.components.imports[0].spec.value, "extern");
    assert!(doc.components.extern_imports.is_empty());
}

#[test]
fn a_malformed_crate_path_is_reported() {
    let src = plugin("## components\nuse extern 3avian::prelude: Collider\n");
    let d = only(&src, code::MALFORMED_EXTERN_IMPORT);
    assert_eq!(d.severity, Severity::Error);
    assert_span(&src, &d, "3avian::prelude");
}

// ---- change annotations (grammar §10) --------------------------------------

/// A `kind: job` spec whose `## behavior` is `steps`, everything else minimal.
fn annotated(steps: &[&str]) -> String {
    format!(
        "{VALID_HEADER}## modules\n\n## data\nin: String\nout: String\n\n\
## behavior\n{}\n\n## invariants\n",
        steps.join("\n")
    )
}

/// Without the removed line's text the baseline is gone for good — the one
/// thing the convention exists to prevent.
#[test]
fn empty_change_removal() {
    let src = annotated(&["- do it", "# -"]);
    let d = only(&src, code::EMPTY_CHANGE_REMOVAL);
    assert_eq!(d.severity, Severity::Warning);
    assert_span(&src, &d, "# -");
}

/// A `# +` covers the line beneath it; with nothing beneath, it adds nothing.
#[test]
fn empty_change_addition() {
    let src = annotated(&["- do it", "# +"]);
    let d = only(&src, code::EMPTY_CHANGE_ADDITION);
    assert_eq!(d.severity, Severity::Warning);
    assert_span(&src, &d, "# +");
}

/// The marker's own `-` stands in for the bullet, so a kept one is doubled.
#[test]
fn redundant_change_bullet() {
    let src = annotated(&["- do it", "# - - do it the old way"]);
    let d = only(&src, code::REDUNDANT_CHANGE_BULLET);
    assert_eq!(d.severity, Severity::Warning);
}

/// A repeated sign is a divider, not a marker — otherwise every rule line in a
/// spec would read as an empty addition or removal.
#[test]
fn a_repeated_sign_is_a_comment_not_a_marker() {
    let src = annotated(&["- do it", "# --- section divider ---", "# +++"]);
    let (_doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:#?}");
}

/// Marking as added a type the *baseline* already needs: the annotated spec is
/// fine, but the baseline it reconstructs to references a type that is no
/// longer defined — so the unannotated half does not stand on its own.
#[test]
fn unrecoverable_baseline() {
    let src = format!(
        "{VALID_HEADER}## modules\n\n## data\n\
# +\nstruct Added {{\n  x: i64,\n}}\n\
in: Added\nout: String\n\n\
## behavior\n- do it\n\n## invariants\n"
    );
    let d = only(&src, code::UNRECOVERABLE_BASELINE);
    assert_eq!(d.severity, Severity::Warning);
    assert!(
        d.message.contains("Added"),
        "message should name the broken claim, got: {}",
        d.message
    );
}

/// The everyday annotated spec — a replaced behavior step and a field added to
/// a referenced type (§4.7's member delta, now spelled the §10 way) — is clean.
#[test]
fn well_formed_annotations_are_silent() {
    let src = plugin(
        "## components\nstruct Wave @ src/waves.rs {\n  index: u32,\n  # - stale: u32,\n  # +\n  streak: u32,\n}\n\n\
## resources\nstruct Cfg {\n  n: u32,\n}\n",
    );
    let src = src.replace(
        "## behavior\n- x\n",
        "## behavior\n- read the wave\n# - spawn one enemy per tick\n# +\n- spawn Cfg.n enemies per tick\n# +\n- increment the spawn counter\n",
    );
    let (_doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:#?}");
}

/// A `# +` on a whole `## systems` entry covers its indented access block too:
/// one marker per addition, not one per line.
#[test]
fn an_added_marker_covers_the_nested_block() {
    let src = plugin_with_systems(
        "go — does it\n  schedule: Update\n# +\nlater — does it later\n  schedule: PostUpdate\n  after: go",
    );
    let (_doc, diags) = analyze(&src);
    assert!(diags.is_empty(), "expected no diagnostics, got: {diags:#?}");

    // The baseline drops the added system with its block, leaving only `go`.
    let recovered = mml_parser::annotations::baseline(&src).text;
    assert!(recovered.contains("go — does it"), "{recovered}");
    assert!(!recovered.contains("later"), "{recovered}");
    assert!(!recovered.contains("PostUpdate"), "{recovered}");
}

/// Un-commenting a `# -` restores the bullet the marker's `-` stood in for in a
/// bulleted section, and restores nothing in a section that has none.
#[test]
fn the_baseline_restores_a_bullet_only_where_the_section_has_them() {
    let src = plugin(
        "## components\nstruct Wave @ src/waves.rs {\n  # - stale: u32,\n  index: u32,\n}\n",
    );
    let src = src.replace("## behavior\n- x\n", "## behavior\n# - do the old thing\n");
    let recovered = mml_parser::annotations::baseline(&src).text;
    assert!(recovered.contains("\n  stale: u32,\n"), "{recovered}");
    assert!(recovered.contains("\n- do the old thing\n"), "{recovered}");
}
