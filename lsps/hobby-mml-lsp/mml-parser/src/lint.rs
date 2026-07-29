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

/// All lint checks combined: unresolved `@ spec` refs, `use` imports, and
/// source paths.
pub fn lint(doc: &Document, current_file: &Path, workspace_root: Option<&Path>) -> Vec<Diagnostic> {
    let mut out = unresolved_spec_refs(doc, current_file, workspace_root);
    out.extend(unresolved_imports(doc, current_file, workspace_root));
    out.extend(unresolved_source_paths(doc, current_file, workspace_root));
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

    let current_dir = current_file.parent().map(Path::to_path_buf);
    let base = doc
        .header
        .base
        .as_ref()
        .and_then(|b| paths::base_dir(&b.value, current_dir.as_deref(), workspace_root));
    let repo_dirs: Vec<PathBuf> = doc
        .header
        .source
        .as_ref()
        .and_then(|s| s.repo.as_ref())
        .map(|r| paths::repo_dirs(&r.value, current_file, workspace_root))
        .unwrap_or_default();
    let search_dirs: Vec<&Path> = base
        .as_deref()
        .into_iter()
        .chain(repo_dirs.iter().map(PathBuf::as_path))
        .chain(current_dir.as_deref())
        .chain(workspace_root)
        .collect();
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

    #[test]
    fn lint_combines_all_checks() {
        let dir = tempfile::tempdir().unwrap();
        // `supervisor.py`/`child.py` missing AND a spec ref to a missing
        // sibling → one source-path warning per path + one spec-ref warning.
        let src = spec_src("").replace("- run", "- run @ spec other");
        let current = dir.path().join("one.hmml");
        let diags = lint(&doc(&src), &current, None);
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
