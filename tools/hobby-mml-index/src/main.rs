//! Generate `index.md` — a manifest of every `.mml` spec in a spec repository.
//!
//! Walks each `<system>/` subdirectory of a spec-repo root (root-level `.mml`
//! files such as `template.mml` are not specs and are skipped), parses each
//! `.mml` with the `mml-parser` crate, resolves each source repo to a local
//! checkout under `~/fun_repos/<repo>` to capture its current git sha + commit
//! date, and writes a `index.md` grouped by system — including the spec→spec
//! edges declared by `@ spec` annotations (grammar §5.4) and the type-import
//! edges declared by `use` lines (grammar §4.5).
//!
//! As before, a spec without `source.repo` gets no table row (linkage.md: "a
//! spec with no source block does not appear in index.md"), but its `@ spec`
//! and `use` edges still count toward the `Referenced by` / `Imported by`
//! columns of specs that do.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

/// A `(system, unit)` spec identity.
type SpecId = (String, String);

/// Everything the manifest needs from one parsed `.mml` file.
struct SpecEntry {
    system: String,
    unit: String,
    kind: String,
    /// `source.repo`; entries without one get no table row.
    repo: Option<String>,
    paths: Vec<String>,
    /// Outgoing `@ spec` refs, bare refs resolved against this spec's system.
    refs: Vec<SpecId>,
    /// Outgoing `use` type imports, at unit granularity (one edge per owning
    /// unit regardless of how many types are imported from it); bare refs
    /// resolved against this spec's system.
    imports: Vec<SpecId>,
}

impl SpecEntry {
    fn id(&self) -> SpecId {
        (self.system.clone(), self.unit.clone())
    }
}

/// Git state of a source checkout, captured at index-generation time. `None`
/// when the repo has no local checkout under `~/fun_repos`.
#[derive(Clone)]
struct SourceState {
    sha: String,
    date: String,
}

const UNKNOWN: &str = "unknown";

fn main() -> ExitCode {
    let root = match std::env::args().nth(1) {
        Some(arg) => PathBuf::from(arg),
        None => default_root(),
    };

    if !root.is_dir() {
        eprintln!("hobby-mml-index: spec root {} is not a directory", root.display());
        return ExitCode::FAILURE;
    }

    // Specs live at <root>/<system>/<unit>.mml, so only descend into the system
    // subdirectories — `.mml` files directly in the root (e.g. `template.mml`)
    // are not specs and are ignored.
    let mut mml_files = Vec::new();
    for entry in fs::read_dir(&root).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() && path.file_name().is_none_or(|n| n != ".git") {
            collect_mml(&path, &mut mml_files);
        }
    }
    mml_files.sort();

    let mut entries = Vec::new();
    for file in &mml_files {
        match fs::read_to_string(file) {
            Ok(text) => match parse_spec(&text) {
                Some(e) => entries.push(e),
                None => eprintln!("hobby-mml-index: skipping {}: incomplete header", file.display()),
            },
            Err(e) => eprintln!("hobby-mml-index: cannot read {}: {e}", file.display()),
        }
    }

    // Warn on `@ spec` and `use` import edges whose target spec is not in the repo.
    let known: BTreeSet<SpecId> = entries.iter().map(SpecEntry::id).collect();
    for e in &entries {
        for (system, unit) in &e.refs {
            if !known.contains(&(system.clone(), unit.clone())) {
                eprintln!(
                    "hobby-mml-index: warning: {}/{} references missing spec {system}/{unit}",
                    e.system, e.unit
                );
            }
        }
        for (system, unit) in &e.imports {
            if !known.contains(&(system.clone(), unit.clone())) {
                eprintln!(
                    "hobby-mml-index: warning: {}/{} imports from missing spec {system}/{unit}",
                    e.system, e.unit
                );
            }
        }
    }

    // Resolve each distinct source repo's git state once and reuse it.
    let projects = projects_dir();
    let mut source_cache: BTreeMap<String, SourceState> = BTreeMap::new();
    for h in &entries {
        if let Some(repo) = &h.repo {
            source_cache
                .entry(repo.clone())
                .or_insert_with(|| resolve_source(&projects, repo));
        }
    }

    let index = render_index(&entries, &source_cache);
    let out_path = root.join("index.md");
    if let Err(e) = fs::write(&out_path, &index) {
        eprintln!("hobby-mml-index: cannot write {}: {e}", out_path.display());
        return ExitCode::FAILURE;
    }

    eprintln!(
        "hobby-mml-index: wrote {} ({} specs across {} systems)",
        out_path.display(),
        entries.len(),
        known.iter().map(|(s, _)| s).collect::<BTreeSet<_>>().len(),
    );
    ExitCode::SUCCESS
}

/// `~/fun_repos/hobby-mimisbrunnr/notes` — hobby specs live per-task inside the
/// notes tree at `notes/{project}/{topic}/{task}/<unit>.mml`, so indexing starts
/// at the notes root and walks down.
fn default_root() -> PathBuf {
    home_dir().join("fun_repos/hobby-mimisbrunnr/notes")
}

/// `~/fun_repos` — the hobby main checkouts a spec's `source.repo` resolves to.
fn projects_dir() -> PathBuf {
    home_dir().join("fun_repos")
}

fn home_dir() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()))
}

/// Recursively collect `*.mml` files, skipping `.git`.
fn collect_mml(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }
            collect_mml(&path, out);
        } else if path.extension().is_some_and(|e| e == "mml") {
            out.push(path);
        }
    }
}

/// Parse a `.mml` with the real grammar parser. Returns `None` when any of
/// system/unit/kind is missing (an unusable spec). `@ spec` refs are resolved
/// to `(system, unit)` with bare refs falling back to the spec's own system;
/// malformed refs are dropped (the parser CLI/LSP report those).
fn parse_spec(text: &str) -> Option<SpecEntry> {
    let doc = mml_parser::parse(text);

    let system = doc.header.system.as_ref()?.value.clone();
    let unit = doc.header.unit.as_ref()?.value.clone();
    let kind = format!("{:?}", doc.header.kind.as_ref()?.value).to_lowercase();
    let (repo, paths) = match &doc.header.source {
        Some(src) => (
            src.repo.as_ref().map(|r| r.value.clone()),
            src.paths.iter().map(|p| p.value.clone()).collect(),
        ),
        None => (None, Vec::new()),
    };

    let mut refs: Vec<SpecId> = mml_parser::validate::collect_spec_refs(&doc)
        .into_iter()
        .filter(|s| mml_parser::ast::is_valid_spec_ref(&s.value))
        .map(|s| {
            let (ref_system, ref_unit) = mml_parser::ast::split_spec_ref(&s.value);
            (
                ref_system.unwrap_or(&system).to_string(),
                ref_unit.to_string(),
            )
        })
        .collect();
    refs.sort();
    refs.dedup();

    let mut imports: Vec<SpecId> = doc
        .data
        .imports
        .iter()
        .filter(|i| mml_parser::ast::is_valid_spec_ref(&i.spec.value))
        .map(|i| {
            let (ref_system, ref_unit) = mml_parser::ast::split_spec_ref(&i.spec.value);
            (
                ref_system.unwrap_or(&system).to_string(),
                ref_unit.to_string(),
            )
        })
        .collect();
    imports.sort();
    imports.dedup();

    Some(SpecEntry {
        system,
        unit,
        kind,
        repo,
        paths,
        refs,
        imports,
    })
}

/// Capture the current HEAD sha + commit date of `~/fun_repos/<repo>`.
/// Returns an `unknown`/`unknown` state when the checkout or git call fails.
fn resolve_source(projects: &Path, repo: &str) -> SourceState {
    let checkout = projects.join(repo);
    if !checkout.is_dir() {
        return SourceState {
            sha: UNKNOWN.to_string(),
            date: UNKNOWN.to_string(),
        };
    }
    let sha =
        git(&checkout, &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| UNKNOWN.to_string());
    // %cs = committer date, ISO short (YYYY-MM-DD) — deterministic for a fixed sha.
    let date = git(&checkout, &["show", "-s", "--format=%cs", "HEAD"])
        .unwrap_or_else(|| UNKNOWN.to_string());
    SourceState { sha, date }
}

/// Run a git command in `dir`; `None` if it fails or produces no output.
fn git(dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Markdown cell listing spec ids: links for specs present in the repo,
/// `system/unit (missing)` otherwise, `—` when empty.
fn spec_id_cell(ids: &BTreeSet<SpecId>, known: &BTreeSet<SpecId>) -> String {
    if ids.is_empty() {
        return "—".to_string();
    }
    ids.iter()
        .map(|(system, unit)| {
            if known.contains(&(system.clone(), unit.clone())) {
                format!("[{system}/{unit}]({system}/{unit}.mml)")
            } else {
                format!("{system}/{unit} (missing)")
            }
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Render the manifest: a `## <system>` section per system, each holding a table
/// of its units (only specs with a `source.repo` get a row) plus the `@ spec`
/// edges. Deterministic ordering (BTree maps/sets throughout).
fn render_index(entries: &[SpecEntry], sources: &BTreeMap<String, SourceState>) -> String {
    let known: BTreeSet<SpecId> = entries.iter().map(SpecEntry::id).collect();
    let mut referenced_by: BTreeMap<SpecId, BTreeSet<SpecId>> = BTreeMap::new();
    let mut imported_by: BTreeMap<SpecId, BTreeSet<SpecId>> = BTreeMap::new();
    for e in entries {
        for target in &e.refs {
            referenced_by
                .entry(target.clone())
                .or_default()
                .insert(e.id());
        }
        for target in &e.imports {
            imported_by
                .entry(target.clone())
                .or_default()
                .insert(e.id());
        }
    }

    let mut by_system: BTreeMap<&str, Vec<&SpecEntry>> = BTreeMap::new();
    let mut rows = 0usize;
    for e in entries {
        if e.repo.is_some() {
            by_system.entry(&e.system).or_default().push(e);
            rows += 1;
        }
    }

    let mut out = String::new();
    out.push_str("# MML spec index\n\n");
    out.push_str("_Generated by `mml-index` — do not edit by hand._\n\n");
    out.push_str(&format!(
        "{} specs across {} systems.\n",
        rows,
        by_system.len()
    ));

    for (system, units) in &mut by_system {
        units.sort_by(|a, b| a.unit.cmp(&b.unit));
        out.push_str(&format!("\n## {system}\n\n"));
        out.push_str(
            "| Unit | Kind | Source repo | Source paths | Source sha | Source date | Refs | Referenced by | Imports | Imported by |\n",
        );
        out.push_str(
            "| ---- | ---- | ----------- | ------------ | ---------- | ----------- | ---- | ------------- | ------- | ----------- |\n",
        );
        for h in units {
            let repo = h.repo.as_deref().unwrap_or(UNKNOWN);
            let src = sources.get(repo).cloned().unwrap_or(SourceState {
                sha: UNKNOWN.to_string(),
                date: UNKNOWN.to_string(),
            });
            let paths = if h.paths.is_empty() {
                "—".to_string()
            } else {
                h.paths
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join("<br>")
            };
            let refs = spec_id_cell(&h.refs.iter().cloned().collect(), &known);
            let incoming = spec_id_cell(
                referenced_by.get(&h.id()).unwrap_or(&BTreeSet::new()),
                &known,
            );
            let imports = spec_id_cell(&h.imports.iter().cloned().collect(), &known);
            let imported =
                spec_id_cell(imported_by.get(&h.id()).unwrap_or(&BTreeSet::new()), &known);
            out.push_str(&format!(
                "| [{unit}]({system}/{unit}.mml) | {kind} | `{repo}` | {paths} | {sha} | {date} | {refs} | {incoming} | {imports} | {imported} |\n",
                unit = h.unit,
                system = system,
                kind = h.kind,
                repo = repo,
                paths = paths,
                sha = src.sha,
                date = src.date,
                refs = refs,
                incoming = incoming,
                imports = imports,
                imported = imported,
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(system: &str, unit: &str, repo: Option<&str>, refs: &[(&str, &str)]) -> SpecEntry {
        entry_with(system, unit, repo, refs, &[])
    }

    fn entry_with(
        system: &str,
        unit: &str,
        repo: Option<&str>,
        refs: &[(&str, &str)],
        imports: &[(&str, &str)],
    ) -> SpecEntry {
        let ids = |pairs: &[(&str, &str)]| {
            pairs
                .iter()
                .map(|(s, u)| (s.to_string(), u.to_string()))
                .collect()
        };
        SpecEntry {
            system: system.to_string(),
            unit: unit.to_string(),
            kind: "job".to_string(),
            repo: repo.map(String::from),
            paths: Vec::new(),
            refs: ids(refs),
            imports: ids(imports),
        }
    }

    #[test]
    fn parse_spec_collects_resolved_refs() {
        let text = "system: sys-a\nunit: one\nkind: job\nsource:\n  repo: r\n  paths:\n    - a.rs\n\n\
## modules\nFoo — does @ spec sys-b/other\n\n\
## data\nin: String\nout: String\n\n\
## behavior\n- go @ spec two\n\n\
## invariants\n- ok @ spec sys-b/other\n";
        let e = parse_spec(text).expect("spec");
        assert_eq!(e.system, "sys-a");
        assert_eq!(e.repo.as_deref(), Some("r"));
        // Bare ref resolved to own system; duplicate qualified ref deduped.
        assert_eq!(
            e.refs,
            vec![
                ("sys-a".to_string(), "two".to_string()),
                ("sys-b".to_string(), "other".to_string()),
            ]
        );
    }

    #[test]
    fn parse_spec_collects_resolved_imports() {
        let text = "system: sys-a\nunit: one\nkind: job\nsource:\n  repo: r\n  paths:\n    - a.rs\n\n\
## modules\n\n\
## data\nuse sys-b/model: Offer, Batch\nuse two: Local\nin: Offer\nout: Batch\n\n\
## behavior\n- go\n\n## invariants\n- ok\n";
        let e = parse_spec(text).expect("spec");
        // One edge per owning unit (both types from sys-b/model collapse); bare
        // ref resolved to own system.
        assert_eq!(
            e.imports,
            vec![
                ("sys-a".to_string(), "two".to_string()),
                ("sys-b".to_string(), "model".to_string()),
            ]
        );
    }

    #[test]
    fn render_includes_import_edges() {
        let entries = vec![
            entry_with("sys-a", "consumer", Some("r"), &[], &[("sys-b", "model")]),
            entry("sys-b", "model", Some("r"), &[]),
        ];
        let out = render_index(&entries, &BTreeMap::new());
        // The consumer's Imports cell links the owner; the owner's Imported by
        // cell links the consumer.
        assert!(
            out.contains("| [sys-b/model](sys-b/model.mml) | — |"),
            "imports cell: {out}"
        );
        assert!(out.contains("[sys-a/consumer](sys-a/consumer.mml)"));
        assert!(out.contains("Imports | Imported by"));
    }

    #[test]
    fn render_includes_edges_and_missing_marker() {
        let entries = vec![
            entry(
                "sys-a",
                "one",
                Some("r"),
                &[("sys-b", "other"), ("sys-c", "nope")],
            ),
            entry("sys-b", "other", Some("r"), &[]),
            // No repo → no row, but its edge still shows under `Referenced by`.
            entry("sys-a", "norepo", None, &[("sys-b", "other")]),
        ];
        let out = render_index(&entries, &BTreeMap::new());
        // Outgoing refs of sys-a/one: a link and a missing marker.
        assert!(out.contains("[sys-b/other](sys-b/other.mml)"));
        assert!(out.contains("sys-c/nope (missing)"));
        // Incoming refs on sys-b/other include both referrers (norepo has no
        // row of its own but is a known spec, so it links).
        assert!(out.contains("[sys-a/norepo](sys-a/norepo.mml)"));
        assert!(out.contains("[sys-a/one](sys-a/one.mml)"));
        assert!(!out.contains("| [norepo]"), "norepo must not get a row");
        assert!(out.contains("2 specs across 2 systems"));
    }
}
