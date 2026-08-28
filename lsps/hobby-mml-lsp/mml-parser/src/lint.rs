//! Filesystem-aware lint checks, shared by the CLI (`--lint`) and the LSP.
//!
//! The pure parse/validate passes never touch the filesystem; these checks do.
//! Each takes the spec's on-disk location (`current_file`) plus an optional
//! workspace root and warns about references that do not resolve to a real
//! file: `@ spec` refs and `use` imports (sibling `.hmml` files) and — the check
//! goto-definition otherwise fails silently on — the header `source.paths`
//! entries and module `@ path`s.

use std::path::{Path, PathBuf};

use crate::ast::Document;
use crate::diagnostic::code;
use crate::{Diagnostic, paths, validate};

/// All lint checks combined: unresolved `@ spec` refs, `use` imports, source
/// paths, and Rust type references.
///
/// `src` is the text `doc` was parsed from. The Rust-reference checks need it
/// because they assert things about the code *as it is today*, and a `# +`
/// change annotation (grammar §10) marks a line that is deliberately not true
/// yet — the marker is a comment the parser drops, so the AST alone cannot tell
/// a proposed member from an asserted one.
pub fn lint(
    doc: &Document,
    src: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    let mut out = unresolved_spec_refs(doc, current_file, workspace_root);
    out.extend(unresolved_imports(doc, current_file, workspace_root));
    out.extend(unresolved_source_paths(doc, current_file, workspace_root));
    out.extend(rust_references(doc, src, current_file, workspace_root));
    out
}

/// Warnings for well-formed `@ spec` refs that do not resolve to a spec file
/// on disk (grammar §5.4). Malformed refs already carry a parse/validate
/// error and are skipped.
pub fn unresolved_spec_refs(
    doc: &Document,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    validate::collect_spec_refs(doc)
        .into_iter()
        .filter(|s| crate::ast::is_valid_spec_ref(&s.value))
        .filter_map(|s| {
            let message = match paths::resolve_spec_ref(&s.value, current_file, workspace_root) {
                Some((_, true)) => return None,
                Some((candidate, false)) => format!(
                    "spec `{}` does not resolve to {}",
                    s.value,
                    candidate.display()
                ),
                None => format!("cannot resolve spec `{}`", s.value),
            };
            Some(Diagnostic::warning(
                s.span,
                code::UNRESOLVED_SPEC_REF,
                message,
            ))
        })
        .collect()
}

/// Warnings for `use` type imports (grammar §4.5) whose target spec is missing,
/// or which name a type the target does not define. Malformed refs already carry
/// a `malformed-import` error and are skipped. Every section that may import is
/// checked — `## data` and the three world sections alike, since the section
/// decides the imported name's role and not whether it has to resolve.
pub fn unresolved_imports(
    doc: &Document,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for import in doc.all_imports() {
        if !crate::ast::is_valid_spec_ref(&import.spec.value) {
            continue;
        }
        match paths::resolve_spec_ref(&import.spec.value, current_file, workspace_root) {
            Some((target, true)) => {
                let defined = paths::defined_type_names(&target);
                for ty in &import.types {
                    if !defined.contains(&ty.value) {
                        out.push(Diagnostic::warning(
                            ty.span,
                            code::UNRESOLVED_IMPORT,
                            format!(
                                "type `{}` is not defined in `{}`",
                                ty.value, import.spec.value
                            ),
                        ));
                    }
                }
            }
            Some((candidate, false)) => out.push(Diagnostic::warning(
                import.spec.span,
                code::UNRESOLVED_IMPORT,
                format!(
                    "import `{}` does not resolve to {}",
                    import.spec.value,
                    candidate.display()
                ),
            )),
            None => out.push(Diagnostic::warning(
                import.spec.span,
                code::UNRESOLVED_IMPORT,
                format!("cannot resolve import `{}`", import.spec.value),
            )),
        }
    }
    out
}

/// Warnings for header `source.paths` entries and module `@ path`s that do not
/// resolve to a file on disk. Resolution uses exactly the goto-definition
/// search order: the spec's declared `base:` first, then the `source.repo`
/// checkout roots, then the spec file's directory, then the workspace root.
/// A broken reference here is what makes goto-into-source silently no-op.
pub fn unresolved_source_paths(
    doc: &Document,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    let source_paths = validate::collect_source_paths(doc);
    if source_paths.is_empty() {
        return Vec::new();
    }

    let dirs = paths::source_search_dirs(doc, current_file, workspace_root);
    let search_dirs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let searched = search_dirs
        .iter()
        .map(|d| d.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    source_paths
        .into_iter()
        .filter_map(|p| {
            let (raw, _line, _col) = paths::split_suffix(&p.value);
            if paths::resolve(raw, &search_dirs).is_some() {
                return None;
            }
            let message = if searched.is_empty() {
                format!("source path `{raw}` cannot be resolved (no search directories)")
            } else {
                format!("source path `{raw}` does not exist under any of: {searched}")
            };
            Some(Diagnostic::warning(
                p.span,
                code::UNRESOLVED_SOURCE_PATH,
                message,
            ))
        })
        .collect()
}

/// Warnings for type declarations that reference real Rust code (grammar §4.7):
/// the type is missing from the file it claims to live in, its `#[derive(…)]`
/// disagrees with the section it was filed under, its shape disagrees with the
/// keyword, or a member the spec asserts is not there.
///
/// **Only what the spec asserts about today's code is checked.** A declaration
/// or member introduced by a `# +` change annotation (grammar §10), and a legacy
/// inline `+ field` / `- field`, state what the spec wants *done* to the code, so
/// neither is an assertion and neither is diagnosed. A `# +` that has not landed
/// yet is the normal mid-implementation state — which is when a spec is read
/// most, so diagnosing it would make every in-flight spec noisy.
///
/// A path that does not resolve produces **one** `unresolved-source-path`
/// warning (from [`unresolved_source_paths`], which sees type paths too) and
/// nothing here, rather than one warning per unverifiable claim about a file
/// that is not there.
pub fn rust_references(
    doc: &Document,
    src: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    let refs = doc.rust_refs();
    if refs.is_empty() {
        return Vec::new();
    }
    let dirs = paths::source_search_dirs(doc, current_file, workspace_root);
    let search_dirs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    // Nothing under a `# +` is a claim about today's code, so none of the
    // checks below apply to it (grammar §10).
    let added = crate::annotations::added_lines(src);
    let index = crate::span::LineIndex::new(src);
    let is_proposed =
        |span: crate::span::Span| added.contains(&(index.position(span.start).line as usize));

    let mut out = Vec::new();
    for (def, role) in refs {
        if is_proposed(def.name().span) {
            continue; // a type this change adds; there is nothing there to check
        }
        let Some(path) = def.path() else { continue };
        let (raw, _line, _col) = paths::split_suffix(&path.value);
        let Some(target) = paths::resolve(raw, &search_dirs) else {
            continue; // already reported as `unresolved-source-path`
        };
        let items = crate::rust_source::items_in(&target);
        let name = def.name();
        let Some(item) = crate::rust_source::item_named(&items, &name.value) else {
            out.push(Diagnostic::warning(
                name.span,
                code::RUST_TYPE_NOT_FOUND,
                format!(
                    "`{}` declares no type named `{}`; check the path, or the name if it was \
                     renamed",
                    raw, name.value
                ),
            ));
            continue;
        };
        check_shape(def, item, &mut out);
        check_role(def, item, role, &mut out);
        check_members(def, item, &is_proposed, &mut out);
    }
    out
}

/// The spec's keyword against the code's: `struct` vs `enum`, and a `marker`
/// against a type that turns out to carry fields.
fn check_shape(
    def: &crate::ast::TypeDef,
    item: &crate::rust_source::RustItem,
    out: &mut Vec<Diagnostic>,
) {
    use crate::ast::TypeDef;
    use crate::rust_source::RustShape;
    let name = def.name();
    let complaint = match (def, item.shape) {
        (TypeDef::Enum { .. }, RustShape::Struct | RustShape::UnitStruct) => {
            Some("a `struct`".to_string())
        }
        (TypeDef::Struct { .. }, RustShape::Enum) => Some("an `enum`".to_string()),
        (TypeDef::Marker { .. }, RustShape::Struct) => Some(format!(
            "a struct with fields ({})",
            item.members.join(", ")
        )),
        (TypeDef::Marker { .. }, RustShape::Enum) => Some("an `enum`".to_string()),
        _ => None,
    };
    if let Some(found) = complaint {
        let spec_noun = match def {
            TypeDef::Struct { .. } => "struct",
            TypeDef::Enum { .. } => "enum",
            TypeDef::Marker { .. } => "marker",
        };
        out.push(Diagnostic::warning(
            name.span,
            code::RUST_SHAPE_MISMATCH,
            format!(
                "the spec calls `{}` a {spec_noun}, but the code declares {found}",
                name.value
            ),
        ));
    }
}

/// The derive against the section. This is the check that catches the classic
/// Bevy spec error — filing a resource under `## components` — at the one moment
/// it is cheap to fix, and it is why a reference is worth more than a copy.
fn check_role(
    def: &crate::ast::TypeDef,
    item: &crate::rust_source::RustItem,
    role: Option<crate::ast::Role>,
    out: &mut Vec<Diagnostic>,
) {
    use crate::ast::Role;
    let Some(role) = role else {
        return; // `## data` assigns no role, so there is nothing to disagree with
    };
    let wanted: &[&str] = match role {
        Role::Component => &["Component"],
        Role::Resource => &["Resource"],
        // Bevy renamed `Event` to `Message` for buffered events; both are the
        // same statement about the type, so either satisfies `## events`.
        Role::Event => &["Event", "Message", "BufferedEvent", "EntityEvent"],
    };
    if wanted.iter().any(|d| item.derives(d)) {
        return;
    }
    let name = def.name();
    let found = if item.derives.is_empty() {
        "it derives nothing".to_string()
    } else {
        format!("it derives {}", item.derives.join(", "))
    };
    out.push(Diagnostic::warning(
        name.span,
        code::RUST_ROLE_MISMATCH,
        format!(
            "`{}` is declared in `## {}`, which claims `#[derive({})]`, but {found}",
            name.value,
            role.section(),
            wanted[0],
        ),
    ));
}

/// Unmarked members against the real type's. A spec is free to mention only the
/// fields that matter, so a field the code has and the spec omits is silent —
/// abstraction is the point. The reverse is drift.
fn check_members(
    def: &crate::ast::TypeDef,
    item: &crate::rust_source::RustItem,
    is_proposed: &dyn Fn(crate::span::Span) -> bool,
    out: &mut Vec<Diagnostic>,
) {
    use crate::ast::{Delta, TypeDef};
    // A tuple struct has no member names to match against, so nothing is
    // checkable and everything would be a false positive.
    if item.members.is_empty() {
        return;
    }
    let asserted: Vec<(&crate::span::Spanned<String>, &str)> = match def {
        TypeDef::Struct {
            fields: Some(fields),
            ..
        } => fields
            .iter()
            .filter(|f| f.delta == Delta::Keep && !is_proposed(f.name.span))
            .map(|f| (&f.name, "field"))
            .collect(),
        TypeDef::Enum {
            variants: Some(variants),
            ..
        } => variants
            .iter()
            .filter(|v| v.delta == Delta::Keep && !is_proposed(v.name.span))
            .map(|v| (&v.name, "variant"))
            .collect(),
        _ => return,
    };
    for (member, noun) in asserted {
        if item.members.contains(&member.value) {
            continue;
        }
        out.push(Diagnostic::warning(
            member.span,
            code::RUST_MEMBER_UNKNOWN,
            format!(
                "`{}` has no {noun} `{}`; put it under a `# +` to ask for it, or drop the line",
                def.name().value,
                member.value,
            ),
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn doc(src: &str) -> Document {
        crate::parse(src)
    }

    /// A minimal spec whose `source.paths` and one module `@ path` reference
    /// `supervisor.py`, with `extra` appended to the header (e.g. a `base:`).
    fn spec_src(extra: &str) -> String {
        format!(
            "system: sys\nunit: one\nkind: workflow\n{extra}source:\n  repo: rw\n  paths:\n    - supervisor.py\n\n## modules\nWorker — runs @ child.py\n\n## data\nin:  Queue<String>\nout: Stream<String>\n\n## behavior\n- run\n\n## invariants\n"
        )
    }

    #[test]
    fn source_paths_next_to_spec_are_silent() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("supervisor.py"), "x").unwrap();
        fs::write(dir.path().join("child.py"), "x").unwrap();
        let current = dir.path().join("one.hmml");
        let diags = unresolved_source_paths(&doc(&spec_src("")), &current, None);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn missing_source_path_and_module_path_warn() {
        let dir = tempfile::tempdir().unwrap();
        // Only the module's `child.py` exists; the header path is missing.
        fs::write(dir.path().join("child.py"), "x").unwrap();
        let src = spec_src("");
        let current = dir.path().join("one.hmml");
        let diags = unresolved_source_paths(&doc(&src), &current, None);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, "unresolved-source-path");
        assert!(diags[0].message.contains("supervisor.py"));
        // The span points at the `supervisor.py` header entry.
        let span = diags[0].span;
        assert_eq!(&src[span.start..span.end], "supervisor.py");
    }

    #[test]
    fn module_path_without_source_block_warns_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let src = "unit: one\nkind: job\n\n## modules\nWorker — runs @ nope.py\n\n## data\nin:  Queue<String>\nout: Stream<String>\n\n## behavior\n- run\n\n## invariants\n";
        let current = dir.path().join("one.hmml");
        let diags = unresolved_source_paths(&doc(src), &current, None);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("nope.py"));
    }

    #[test]
    fn base_dir_resolves_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("checkout");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("supervisor.py"), "x").unwrap();
        fs::write(repo.join("child.py"), "x").unwrap();
        let current = dir.path().join("one.hmml");
        let diags = unresolved_source_paths(&doc(&spec_src("base: checkout\n")), &current, None);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn line_col_suffix_is_stripped_before_resolving() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("child.py"), "x").unwrap();
        let src = "unit: one\nkind: job\n\n## modules\nWorker — runs @ child.py:42:10\n\n## data\nin:  Queue<String>\nout: Stream<String>\n\n## behavior\n- run\n\n## invariants\n";
        let current = dir.path().join("one.hmml");
        let diags = unresolved_source_paths(&doc(src), &current, None);
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn workspace_root_is_searched_last() {
        let spec_dir = tempfile::tempdir().unwrap();
        let ws = tempfile::tempdir().unwrap();
        fs::write(ws.path().join("supervisor.py"), "x").unwrap();
        fs::write(ws.path().join("child.py"), "x").unwrap();
        let current = spec_dir.path().join("one.hmml");
        let diags = unresolved_source_paths(&doc(&spec_src("")), &current, Some(ws.path()));
        assert!(diags.is_empty(), "{diags:?}");
    }

    /// An import is checked wherever it sits: `## data` and the world sections
    /// differ in the role they assign, not in whether the name must resolve.
    #[test]
    fn imports_are_checked_in_every_section() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("model.hmml"),
            "system: s\nunit: model\nkind: plugin\n\n## data\nstruct Pattern {\n  n: u32,\n}\n\n## components\nmarker Foe\n\n## systems\n\n## behavior\n- x\n\n## invariants\n",
        )
        .unwrap();
        let src = "system: s\nunit: one\nkind: system\n\n\
## data\nuse model: Pattern, Ghost\n\n\
## components\nuse model: Foe, Wraith\n\n\
## access\nschedule: Update\n\n\
## behavior\n- run\n\n## invariants\n";
        let current = dir.path().join("one.hmml");
        let diags = unresolved_imports(&doc(src), &current, None);
        let missing: Vec<&str> = diags
            .iter()
            .map(|d| &src[d.span.start..d.span.end])
            .collect();
        assert_eq!(missing, ["Ghost", "Wraith"], "{diags:?}");
    }

    // ---- Rust references (grammar §4.7) ---------------------------------

    const WAVES_RS: &str = r#"
use bevy::prelude::*;

#[derive(Component)]
pub struct Wave {
    pub index: u32,
    pub remaining: u32,
}

#[derive(Resource)]
pub struct WaveConfig {
    pub interval_secs: f32,
}

#[derive(Component)]
pub struct Spawner;
"#;

    /// A plugin spec beside a `waves.rs` holding [`WAVES_RS`].
    fn rust_tree(sections: &str) -> (tempfile::TempDir, PathBuf, String) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("waves.rs"), WAVES_RS).unwrap();
        let src = format!(
            "system: s\nunit: u\nkind: plugin\n\n{sections}\n## systems\n\n## behavior\n- x\n\n## invariants\n"
        );
        let current = dir.path().join("u.hmml");
        (dir, current, src)
    }

    fn rust_codes(sections: &str) -> Vec<(String, String)> {
        let (_dir, current, src) = rust_tree(sections);
        rust_references(&doc(&src), &src, &current, None)
            .into_iter()
            .map(|d| {
                (
                    d.code.to_string(),
                    src[d.span.start..d.span.end].to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn a_reference_that_matches_the_code_is_silent() {
        let diags = rust_codes(
            "## components\nstruct Wave @ waves.rs {\n  index: u32,\n}\nmarker Spawner @ waves.rs\n\n\
             ## resources\nstruct WaveConfig @ waves.rs\n",
        );
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn a_missing_type_is_reported_against_the_name() {
        let diags = rust_codes("## components\nstruct Ghost @ waves.rs\n");
        assert_eq!(diags, [("rust-type-not-found".into(), "Ghost".into())]);
    }

    #[test]
    fn the_derive_must_match_the_section() {
        // WaveConfig is a Resource; filing it under `## components` is the
        // classic Bevy spec error, and the derive is what catches it.
        let diags = rust_codes("## components\nstruct WaveConfig @ waves.rs\n");
        assert_eq!(diags, [("rust-role-mismatch".into(), "WaveConfig".into())]);
    }

    #[test]
    fn a_role_less_data_reference_is_never_role_checked() {
        let diags = rust_codes("## data\nstruct WaveConfig @ waves.rs\n");
        assert!(diags.is_empty(), "{diags:?}");
    }

    #[test]
    fn a_marker_pointing_at_a_struct_with_fields_is_a_shape_mismatch() {
        let diags = rust_codes("## components\nmarker Wave @ waves.rs\n");
        assert_eq!(diags, [("rust-shape-mismatch".into(), "Wave".into())]);
    }

    #[test]
    fn only_unmarked_members_are_asserted() {
        // `index` exists; the legacy `+ streak` and `- remaining` state intent,
        // so neither is drift while the work is outstanding. Only `bogus` is a
        // claim the code contradicts.
        let diags = rust_codes(
            "## components\nstruct Wave @ waves.rs {\n  index: u32,\n  bogus: u32,\n  + streak: u32,\n  - remaining,\n}\n",
        );
        assert_eq!(diags, [("rust-member-unknown".into(), "bogus".into())]);
    }

    /// A `# +` marker is a comment, but the member beneath it is ordinary
    /// content the parser reads like any other — so without the §10 exclusion
    /// this check would read a field the change *proposes* as a field the spec
    /// asserts is already there, and warn on every in-flight spec.
    #[test]
    fn a_member_a_change_annotation_adds_is_not_asserted() {
        let diags = rust_codes(
            "## components\nstruct Wave @ waves.rs {\n  index: u32,\n  # +\n  streak: u32,\n}\n",
        );
        assert_eq!(diags, []);
    }

    /// Likewise for a whole type: a `# +` on the declaration means the change
    /// adds it, so `rust-type-not-found` would be reporting the work as drift.
    #[test]
    fn a_type_a_change_annotation_adds_is_not_looked_up() {
        let diags = rust_codes("## components\n# +\nstruct Missing @ waves.rs\n");
        assert_eq!(diags, []);
    }

    #[test]
    fn an_unresolved_path_warns_once_rather_than_per_claim() {
        // The file is not there, so nothing about it is knowable: one
        // `unresolved-source-path`, and no rust-* noise on top.
        let (_dir, current, src) =
            rust_tree("## components\nstruct Wave @ nope.rs {\n  index: u32,\n  other: u32,\n}\n");
        let d = doc(&src);
        assert!(rust_references(&d, &src, &current, None).is_empty());
        let codes: Vec<&str> = lint(&d, &src, &current, None)
            .iter()
            .map(|x| x.code)
            .collect();
        assert_eq!(codes, ["unresolved-source-path"]);
    }

    #[test]
    fn lint_combines_all_checks() {
        let dir = tempfile::tempdir().unwrap();
        // `supervisor.py`/`child.py` missing AND a spec ref to a missing
        // sibling → one source-path warning per path + one spec-ref warning.
        let src = spec_src("").replace("- run", "- run @ spec other");
        let current = dir.path().join("one.hmml");
        let diags = lint(&doc(&src), &src, &current, None);
        let codes: Vec<&str> = diags.iter().map(|d| d.code).collect();
        assert!(codes.contains(&"unresolved-spec-ref"), "{codes:?}");
        assert_eq!(
            codes
                .iter()
                .filter(|c| **c == "unresolved-source-path")
                .count(),
            2,
            "{diags:?}"
        );
    }
}
