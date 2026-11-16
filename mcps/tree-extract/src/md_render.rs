//! Rendering extracted symbols as the markdown project map.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::containment::is_inside;
use crate::extractor::{Symbol, SymbolKind};

pub struct RenderOptions {
    /// Soft size limit. When exceeded, drop method-level docstrings (keep
    /// class-level ones) on a second render pass.
    pub size_threshold_bytes: usize,
    /// Repo root used to display files as relative paths.
    pub root: PathBuf,
    /// Optional HEAD hash to write into the front matter.
    pub head_hash: Option<String>,
}

impl RenderOptions {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            // Bumped from 50 KiB → 80 KiB to accommodate per-directory prose
            // blurbs that ground the agent (typically +5-10 KiB).
            size_threshold_bytes: 80 * 1024,
            root: root.into(),
            head_hash: None,
        }
    }
}

pub fn render(symbols: &[Symbol], opts: &RenderOptions) -> String {
    let first = render_inner(symbols, opts, false);
    if first.len() <= opts.size_threshold_bytes {
        first
    } else {
        render_inner(symbols, opts, true)
    }
}

fn render_inner(symbols: &[Symbol], opts: &RenderOptions, drop_method_docs: bool) -> String {
    let mut buf = String::new();
    write_front_matter(&mut buf, opts, drop_method_docs, symbols.len());
    write_dir_blurbs(&mut buf, &opts.root);

    let by_file: BTreeMap<&Path, Vec<&Symbol>> = group_by_file(symbols);

    if by_file.is_empty() {
        buf.push_str("_no symbols extracted_\n");
        return buf;
    }

    for (file, file_syms) in by_file {
        let rel = file.strip_prefix(&opts.root).unwrap_or(file);
        buf.push_str(&format!("## `{}`\n\n", rel.display()));

        // Heuristic: a "method" sym whose start_line falls inside a container
        // sym's [line_start..line_end] range is rendered nested under it.
        let containers: Vec<&Symbol> = file_syms
            .iter()
            .copied()
            .filter(|s| s.kind.is_container())
            .collect();

        let mut emitted: HashSet<usize> = HashSet::new();

        // Top-level (non-method, non-contained) first.
        for (i, sym) in file_syms.iter().enumerate() {
            if sym.kind == SymbolKind::Method {
                continue;
            }
            if is_inside_any(sym, &containers)
                && !std::ptr::eq(*sym, containers_owner(sym, &containers))
            {
                continue;
            }
            write_symbol(&mut buf, sym, 0, drop_method_docs);
            emitted.insert(i);
            // Methods/inner that fall inside this container.
            if sym.kind.is_container() {
                for (j, inner) in file_syms.iter().enumerate() {
                    if i == j || emitted.contains(&j) {
                        continue;
                    }
                    if is_inside(inner, sym) {
                        write_symbol(&mut buf, inner, 1, drop_method_docs);
                        emitted.insert(j);
                    }
                }
            }
        }

        // Anything still un-emitted (orphan methods, free functions extracted as method).
        for (i, sym) in file_syms.iter().enumerate() {
            if emitted.contains(&i) {
                continue;
            }
            write_symbol(&mut buf, sym, 0, drop_method_docs);
        }

        buf.push('\n');
    }
    buf
}

fn write_front_matter(buf: &mut String, opts: &RenderOptions, collapsed: bool, total: usize) {
    buf.push_str("---\n");
    buf.push_str(&format!("root: {}\n", opts.root.display()));
    if let Some(h) = &opts.head_hash {
        buf.push_str(&format!("head: {}\n", h));
    }
    buf.push_str(&format!("symbol_count: {}\n", total));
    if collapsed {
        buf.push_str("collapsed: methods-no-docstrings\n");
    }
    buf.push_str("generator: hobby-tree-extract\n");
    buf.push_str("---\n\n");
    buf.push_str("# Project structure\n\n");
    buf.push_str(
        "Auto-generated symbol map for this repo. Lists classes/structs/traits/interfaces/enums, \
        free functions, and methods with their file:line ranges and docstrings (where available). \
        Refresh with `tree-extract <repo>` or the `refresh_project_description` MCP tool.\n\n",
    );
}

/// Top-level subdirectory blurb: first paragraph from the dir's `README.md`,
/// falling back to `package.description` in `Cargo.toml`, falling back to none.
#[derive(Debug, Clone)]
pub struct DirBlurb {
    pub name: String,
    pub source: BlurbSource,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlurbSource {
    Readme,
    Cargo,
}

/// Collect prose blurbs for each top-level subdirectory of `root`. When a
/// top-level subdir has no README/Cargo description of its own (it's a
/// grouping dir like `mcps/`, `lsps/`, `skills/`), descend one level so the
/// individual crates/tools inside still surface.
pub fn dir_blurbs(root: &Path) -> Vec<DirBlurb> {
    let mut out = Vec::new();
    for dir in interesting_subdirs(root) {
        let name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        if let Some((src, text)) = read_blurb(&dir) {
            out.push(DirBlurb {
                name,
                source: src,
                text,
            });
            continue;
        }
        // Grouping dir: descend one level.
        for child in interesting_subdirs(&dir) {
            let child_name = child.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let nested_name = format!("{}/{}", name, child_name);
            if let Some((src, text)) = read_blurb(&child) {
                out.push(DirBlurb {
                    name: nested_name,
                    source: src,
                    text,
                });
            }
        }
    }
    out
}

fn read_blurb(dir: &Path) -> Option<(BlurbSource, String)> {
    if let Some(text) = read_readme_first_paragraph(dir)
        && !text.is_empty()
    {
        return Some((BlurbSource::Readme, text));
    }
    if let Some(text) = read_cargo_description(dir) {
        return Some((BlurbSource::Cargo, text));
    }
    None
}

fn interesting_subdirs(root: &Path) -> Vec<PathBuf> {
    const SKIP: &[&str] = &[
        ".git",
        "target",
        "node_modules",
        "dist",
        "build",
        ".venv",
        "__pycache__",
        ".idea",
        ".vscode",
        ".expert",
    ];
    let entries = match std::fs::read_dir(root) {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };
    let mut subdirs: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| !s.starts_with('.') && !SKIP.contains(&s))
                .unwrap_or(false)
        })
        .collect();
    subdirs.sort();
    subdirs
}

fn read_readme_first_paragraph(dir: &Path) -> Option<String> {
    // Try common README casings.
    for name in &["README.md", "Readme.md", "readme.md", "README.markdown"] {
        let p = dir.join(name);
        if let Ok(content) = std::fs::read_to_string(&p) {
            return Some(first_paragraph(&content));
        }
    }
    None
}

/// Returns the first non-heading, non-empty paragraph from a markdown blob.
/// Skips YAML front matter (`---\n...\n---`).
fn first_paragraph(md: &str) -> String {
    let mut s = md;
    if let Some(rest) = s.strip_prefix("---\n")
        && let Some((_, body)) = rest.split_once("\n---\n")
    {
        s = body;
    }
    let mut paragraph = String::new();
    for raw in s.lines() {
        let line = raw.trim();
        if paragraph.is_empty() {
            if line.is_empty() || line.starts_with('#') || line.starts_with('|') {
                continue;
            }
            paragraph.push_str(line);
        } else if line.is_empty() {
            break;
        } else {
            paragraph.push(' ');
            paragraph.push_str(line);
        }
    }
    truncate_chars(&paragraph, 480)
}

fn read_cargo_description(dir: &Path) -> Option<String> {
    let content = std::fs::read_to_string(dir.join("Cargo.toml")).ok()?;
    // Tiny inline parser; full toml dep is overkill. Looks for a `description`
    // line inside `[package]` or `[workspace.package]`.
    let mut in_package = false;
    for raw in content.lines() {
        let line = raw.trim();
        if line.starts_with('[') {
            in_package = matches!(line, "[package]" | "[workspace.package]");
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(rest) = line.strip_prefix("description") {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix('=') {
                let v = rest.trim().trim_matches(|c| c == '"' || c == '\'');
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn write_dir_blurbs(buf: &mut String, root: &Path) {
    let blurbs = dir_blurbs(root);
    if blurbs.is_empty() {
        return;
    }
    buf.push_str("## Top-level subdirectories\n\n");
    for b in blurbs {
        let tag = match b.source {
            BlurbSource::Readme => "README",
            BlurbSource::Cargo => "Cargo.toml",
        };
        buf.push_str(&format!("- **`{}/`** _({}):_ {}\n", b.name, tag, b.text));
    }
    buf.push('\n');
}

fn group_by_file(symbols: &[Symbol]) -> BTreeMap<&Path, Vec<&Symbol>> {
    let mut by_file: BTreeMap<&Path, Vec<&Symbol>> = BTreeMap::new();
    for s in symbols {
        by_file.entry(s.file.as_path()).or_default().push(s);
    }
    by_file
}

fn is_inside_any(sym: &Symbol, containers: &[&Symbol]) -> bool {
    containers
        .iter()
        .any(|c| !std::ptr::eq(*c, sym) && is_inside(sym, c))
}

fn containers_owner<'a>(sym: &'a Symbol, containers: &'a [&'a Symbol]) -> &'a Symbol {
    containers
        .iter()
        .copied()
        .find(|c| !std::ptr::eq(*c, sym) && is_inside(sym, c))
        .unwrap_or(sym)
}

fn write_symbol(buf: &mut String, sym: &Symbol, indent: usize, drop_method_docs: bool) {
    let pad = "  ".repeat(indent);
    let line_range = if sym.line_start == sym.line_end {
        format!("L{}", sym.line_start)
    } else {
        format!("L{}-L{}", sym.line_start, sym.line_end)
    };
    buf.push_str(&format!(
        "{pad}- **{kind}** `{name}` _{lang} · {range}_\n",
        pad = pad,
        kind = sym.kind.label(),
        name = sym.name,
        lang = sym.language,
        range = line_range,
    ));
    let show_doc = match sym.kind {
        SymbolKind::Method => !drop_method_docs,
        _ => true,
    };
    if show_doc && let Some(doc) = sym.docstring.as_ref() {
        let one_line: String = doc
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if !one_line.is_empty() {
            let truncated = truncate_chars(&one_line, 240);
            buf.push_str(&format!("{pad}  > {}\n", truncated));
        }
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn first_paragraph_skips_heading() {
        let md = "# Title\n\nThis is the description.\n\nNext paragraph.\n";
        assert_eq!(first_paragraph(md), "This is the description.");
    }

    #[test]
    fn first_paragraph_handles_front_matter() {
        let md = "---\ntitle: x\n---\n\n# Heading\n\nReal text here.\n";
        assert_eq!(first_paragraph(md), "Real text here.");
    }

    #[test]
    fn first_paragraph_joins_wrapped_lines() {
        let md = "# H\n\nLine one\nline two\nline three.\n\nNot this.";
        assert_eq!(first_paragraph(md), "Line one line two line three.");
    }

    #[test]
    fn first_paragraph_empty_when_no_prose() {
        let md = "# H\n\n## H2\n";
        assert_eq!(first_paragraph(md), "");
    }

    #[test]
    fn dir_blurbs_picks_up_readme_and_cargo_desc() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("alpha");
        let b = dir.path().join("bravo");
        let skip = dir.path().join(".git");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::create_dir_all(&skip).unwrap();
        std::fs::write(a.join("README.md"), "# Alpha\n\nAlpha is a useful crate.\n").unwrap();
        std::fs::write(
            b.join("Cargo.toml"),
            "[package]\nname=\"bravo\"\ndescription = \"the bravo tool\"\n",
        )
        .unwrap();

        let blurbs = dir_blurbs(dir.path());
        assert_eq!(blurbs.len(), 2);
        assert_eq!(blurbs[0].name, "alpha");
        assert_eq!(blurbs[0].source, BlurbSource::Readme);
        assert_eq!(blurbs[0].text, "Alpha is a useful crate.");
        assert_eq!(blurbs[1].name, "bravo");
        assert_eq!(blurbs[1].source, BlurbSource::Cargo);
        assert_eq!(blurbs[1].text, "the bravo tool");
    }

    #[test]
    fn dir_blurbs_skips_hidden_and_build_dirs() {
        let dir = tempdir().unwrap();
        for n in &["target", "node_modules", ".vscode"] {
            let sub = dir.path().join(n);
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join("README.md"), "# x\n\nshould not appear\n").unwrap();
        }
        let blurbs = dir_blurbs(dir.path());
        assert!(blurbs.is_empty(), "{:?}", blurbs);
    }
}
