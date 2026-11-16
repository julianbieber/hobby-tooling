//! Counting a repo against the standards its `.hstruct` file states.
//!
//! Every metric answers with a **count of offenders and the offenders
//! themselves**, but only the count is compared to the file. The offenders are
//! printed on request and never stored: a stored list is what goes stale when
//! a file is renamed, split, or merely edited, and that churn is what this
//! format exists to avoid.

use crate::ast::{Metric, Spec, Standard};
use crate::diagnostic::{Diagnostic, Severity, code};
use crate::rust_source::{FileFacts, facts_in};
use crate::span::Span;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The outcome of checking one repo.
pub struct Checked {
    pub diagnostics: Vec<Diagnostic>,
    /// One entry per counted standard, in file order.
    pub measured: Vec<Measured>,
}

/// A standard, what its metric actually counts right now, and who the offenders
/// are.
pub struct Measured {
    pub standard: String,
    pub metric: Metric,
    pub budget: usize,
    pub actual: usize,
    /// The offenders, for `--detail`. Never written back to the file.
    pub offenders: Vec<String>,
    /// Where the `budget:` line sits, so `--ratchet` can rewrite it.
    pub budget_span: Span,
}

/// Check `spec` against the repo rooted at `repo_root`.
pub fn check(spec: &Spec, repo_root: &Path) -> Checked {
    let mut diagnostics = Vec::new();
    manifest(spec, repo_root, &mut diagnostics);

    let Some(root) = spec.header.root.as_ref() else {
        return Checked {
            diagnostics,
            measured: Vec::new(),
        };
    };
    let root = repo_root.join(&root.value);
    if !root.is_dir() {
        diagnostics.push(Diagnostic::error(
            spec.header.span,
            code::MISSING_ROOT,
            format!("no source root at `{}`", root.display()),
        ));
        return Checked {
            diagnostics,
            measured: Vec::new(),
        };
    }

    let repo = Repo::read(&root);
    let mut measured = Vec::new();

    for (standard, metric, budget) in spec.counted() {
        let offenders = repo.count(metric.value);
        let actual = offenders.len();
        report(standard, actual, budget.value, &mut diagnostics);
        measured.push(Measured {
            standard: standard.name.value.clone(),
            metric: metric.value,
            budget: budget.value,
            actual,
            offenders,
            budget_span: budget.span,
        });
    }

    Checked {
        diagnostics,
        measured,
    }
}

fn report(standard: &Standard, actual: usize, budget: usize, out: &mut Vec<Diagnostic>) {
    if actual > budget {
        out.push(Diagnostic::error(
            standard.name.span,
            code::BUDGET_EXCEEDED,
            format!("{actual} over a budget of {budget} — this standard got further away"),
        ));
    } else if actual < budget {
        out.push(Diagnostic::warning(
            standard.name.span,
            code::BUDGET_SLACK,
            format!("{actual} under a budget of {budget} — tighten it to {actual} (`--ratchet`)"),
        ));
    }
}

fn manifest(spec: &Spec, repo_root: &Path, out: &mut Vec<Diagnostic>) {
    let Some(declared) = spec.header.krate.as_ref() else {
        return;
    };
    let path = repo_root.join("Cargo.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        out.push(Diagnostic::error(
            declared.span,
            code::MISSING_MANIFEST,
            format!("no Cargo.toml at `{}`", path.display()),
        ));
        return;
    };
    let name = text
        .lines()
        .skip_while(|l| l.trim() != "[package]")
        .find_map(|l| l.trim().strip_prefix("name"))
        .and_then(|rest| rest.trim().strip_prefix('='))
        .map(|rest| rest.trim().trim_matches('"').to_owned());
    if let Some(name) = name
        && name != declared.value
    {
        out.push(Diagnostic::error(
            declared.span,
            code::CRATE_NAME_MISMATCH,
            format!("Cargo.toml names this package `{name}`"),
        ));
    }
}

/// The repo's Rust, reduced to what the metrics ask about.
///
/// A module is a file. Nothing in the `.hstruct` file names one, so the set is
/// derived here and a new file joins every metric without an edit anywhere.
struct Repo {
    files: BTreeMap<PathBuf, FileFacts>,
    /// Module path to the modules it imports.
    imports: BTreeMap<Vec<String>, BTreeSet<Vec<String>>>,
    root: PathBuf,
}

impl Repo {
    fn read(root: &Path) -> Repo {
        let mut files = BTreeMap::new();
        for path in rust_files_under(root) {
            files.insert(path.clone(), facts_in(&path));
        }

        let known: BTreeSet<Vec<String>> = files.keys().map(|p| rust_path(p, root)).collect();
        let mut imports: BTreeMap<Vec<String>, BTreeSet<Vec<String>>> = BTreeMap::new();
        for (path, facts) in &files {
            let here = rust_path(path, root);
            let parent = here.get(..here.len().saturating_sub(1)).unwrap_or(&[]);
            let mut deps = BTreeSet::new();
            for used in &facts.use_paths {
                if let Some(target) = resolve(used, &here, parent, &known)
                    && target != here
                {
                    deps.insert(target);
                }
            }
            imports.insert(here, deps);
        }

        Repo {
            files,
            imports,
            root: root.to_path_buf(),
        }
    }

    fn label(&self, module: &[String]) -> String {
        module.join("::")
    }

    fn count(&self, metric: Metric) -> Vec<String> {
        match metric {
            Metric::ImportCycles => {
                let mut out = BTreeSet::new();
                for (module, deps) in &self.imports {
                    for dep in deps {
                        if self
                            .imports
                            .get(dep)
                            .is_some_and(|back| back.contains(module))
                        {
                            let (a, b) = if module <= dep {
                                (module, dep)
                            } else {
                                (dep, module)
                            };
                            out.insert(format!("{} <-> {}", self.label(a), self.label(b)));
                        }
                    }
                }
                out.into_iter().collect()
            }
            Metric::LargeFiles { lines } => self
                .files
                .iter()
                .filter(|(_, f)| f.lines > lines)
                .map(|(p, f)| {
                    format!(
                        "{} ({} lines)",
                        p.strip_prefix(&self.root).unwrap_or(p).display(),
                        f.lines
                    )
                })
                .collect(),
            Metric::WideModules { limit } => self
                .imports
                .iter()
                .filter(|(_, deps)| deps.len() > limit)
                .map(|(m, deps)| format!("{} ({})", self.label(m), deps.len()))
                .collect(),
            Metric::MultiWriterTypes => {
                let mut writers: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
                for (path, facts) in &self.files {
                    let module = self.label(&rust_path(path, &self.root));
                    for ty in &facts.writes {
                        writers.entry(ty).or_default().insert(module.clone());
                    }
                }
                writers
                    .into_iter()
                    .filter(|(_, w)| w.len() > 1)
                    .map(|(ty, w)| {
                        format!("{ty} ({})", w.into_iter().collect::<Vec<_>>().join(", "))
                    })
                    .collect()
            }
            Metric::UnfilteredQueries => self
                .files
                .iter()
                .filter(|(_, f)| f.unfiltered_queries > 0)
                .flat_map(|(p, f)| {
                    let name = p
                        .strip_prefix(&self.root)
                        .unwrap_or(p)
                        .display()
                        .to_string();
                    std::iter::repeat_n(name, f.unfiltered_queries)
                })
                .collect(),
            Metric::AllocatingSystems => self
                .files
                .iter()
                .filter(|(_, f)| f.allocating_systems > 0)
                .flat_map(|(p, f)| {
                    let name = p
                        .strip_prefix(&self.root)
                        .unwrap_or(p)
                        .display()
                        .to_string();
                    std::iter::repeat_n(name, f.allocating_systems)
                })
                .collect(),
        }
    }
}

fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(rust_files_under(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    out.sort();
    out
}

/// A file's Rust module path, from its location under the source root.
fn rust_path(path: &Path, root: &Path) -> Vec<String> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut segments: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    if let Some(last) = segments.last_mut() {
        *last = last.trim_end_matches(".rs").to_owned();
    }
    if segments.last().is_some_and(|s| s == "mod" || s == "lib") {
        segments.pop();
    }
    segments
}

/// Which module a `use` path refers to, or `None` when it leaves the crate.
///
/// The answer is the longest prefix of the path that names a file, because a
/// path continues past the module into the item it imports.
fn resolve(
    used: &[String],
    here: &[String],
    parent: &[String],
    known: &BTreeSet<Vec<String>>,
) -> Option<Vec<String>> {
    let (anchor, rest) = used.split_first()?;
    let mut absolute: Vec<String> = match anchor.as_str() {
        "crate" => Vec::new(),
        "super" => parent.to_vec(),
        "self" => here.to_vec(),
        _ => return None,
    };
    absolute.extend_from_slice(rest);

    (1..=absolute.len())
        .rev()
        .filter_map(|end| absolute.get(..end))
        .find(|candidate| known.contains(*candidate))
        .map(<[String]>::to_vec)
}

/// Rewrite every `budget:` line the code has moved below, and return how many
/// were tightened.
///
/// Only downward. A budget that rose would record a regression as if it were
/// the new normal, which is the one thing a ratchet must never do.
pub fn ratchet(source: &str, measured: &[Measured]) -> (String, usize) {
    let mut edits: Vec<(&Measured, usize)> = measured
        .iter()
        .filter(|m| m.actual < m.budget)
        .map(|m| (m, m.budget_span.start))
        .collect();
    edits.sort_by_key(|(_, start)| std::cmp::Reverse(*start));

    let mut out = source.to_owned();
    let mut tightened = 0;
    for (m, start) in &edits {
        let Some(line) = out.get(*start..) else {
            continue;
        };
        let end = line.find('\n').map_or(out.len(), |i| start + i);
        out.replace_range(*start..end, &format!("budget: {}", m.actual));
        tightened += 1;
    }
    (out, tightened)
}

/// Whether any diagnostic is an error.
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(|d| d.severity == Severity::Error)
}
