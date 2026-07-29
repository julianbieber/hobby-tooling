//! `mml-lsp`: a `tower-lsp` language server for `.hmml` (Mental Model Language)
//! specs, built on the `mml-parser` crate.
//!
//! This file is protocol glue only: it caches a parsed [`DocState`] per open
//! document, publishes diagnostics on change, and turns the pure results from
//! [`analysis`] into `lsp_types` values. All cursor/text logic lives in
//! [`analysis`] as pure, unit-tested functions; file resolution and the
//! filesystem diagnostics are shared with the CLI via `mml_parser::paths` /
//! `mml_parser::lint`.

mod analysis;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use analysis::{CompletionContext, SectionKind, Symbols};
use mml_parser::ast::{Document, TypeDef};
use mml_parser::span::Span;
use mml_parser::{LineIndex, Severity, analyze, lint, paths};

/// Everything cached for one open document. Reparse-on-change (files are tiny),
/// so this is rebuilt wholesale in `did_open`/`did_change`.
struct DocState {
    text: String,
    doc: Document,
    line_index: LineIndex,
    symbols: Symbols,
}

impl DocState {
    fn range(&self, span: Span) -> Range {
        range_of(&self.line_index, span)
    }

    fn offset(&self, pos: Position) -> usize {
        analysis::offset_of(&self.text, to_mml_pos(pos))
    }
}

fn range_of(li: &LineIndex, span: Span) -> Range {
    Range {
        start: to_lsp_pos(li.position(span.start)),
        end: to_lsp_pos(li.position(span.end)),
    }
}

struct Backend {
    client: Client,
    /// First workspace root announced at `initialize`; fallback base for
    /// resolving relative `@ path` / `source.paths` on goto-into-source.
    workspace_root: OnceLock<PathBuf>,
    documents: Mutex<HashMap<Url, DocState>>,
}

impl Backend {
    fn new(client: Client) -> Backend {
        Backend {
            client,
            workspace_root: OnceLock::new(),
            documents: Mutex::new(HashMap::new()),
        }
    }

    /// Rebuild the cached state for `uri` and publish its diagnostics. The parse
    /// happens under the lock; the async publish happens after it is released.
    async fn refresh(&self, uri: Url, text: String, version: Option<i32>) {
        let diags = {
            let (doc, mut all) = analyze(&text);
            // Filesystem pass: warn on `@ spec` refs whose target is missing.
            if let Ok(file) = uri.to_file_path() {
                let ws = self.workspace_root.get().map(PathBuf::as_path);
                all.extend(lint::unresolved_spec_refs(&doc, &file, ws));
                all.extend(lint::unresolved_imports(&doc, &file, ws));
                all.sort_by_key(|d| (d.span.start, d.span.end));
            }
            let line_index = LineIndex::new(&text);
            let symbols = analysis::symbols(&doc);
            let diags: Vec<Diagnostic> = all
                .iter()
                .map(|d| to_lsp_diagnostic(&line_index, d))
                .collect();
            let ds = DocState {
                text,
                doc,
                line_index,
                symbols,
            };
            self.documents.lock().unwrap().insert(uri.clone(), ds);
            diags
        };
        self.client.publish_diagnostics(uri, diags, version).await;
    }
}

/// The byte span of a type definition in its own document.
fn type_def_span(t: &TypeDef) -> Span {
    t.span()
}

/// Resolve an import/spec ref to its target `.hmml` and return the target path,
/// its source text, and its parsed document. `None` if the ref does not resolve
/// to an existing file.
fn read_target(
    raw: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Option<(PathBuf, String, Document)> {
    let (target, true) = paths::resolve_spec_ref(raw, current_file, workspace_root)? else {
        return None;
    };
    let src = std::fs::read_to_string(&target).ok()?;
    let doc = mml_parser::parse(&src);
    Some((target, src, doc))
}

/// Goto-definition target for an imported type: the type's definition in the
/// spec that owns it.
fn import_type_definition(
    spec_raw: &str,
    type_name: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> Option<Location> {
    let (target, src, doc) = read_target(spec_raw, current_file, workspace_root)?;
    let def = doc.all_type_defs().find(|t| t.name().value == type_name)?;
    let li = LineIndex::new(&src);
    Some(Location {
        uri: Url::from_file_path(&target).ok()?,
        range: range_of(&li, def.name().span),
    })
}

/// Hover markdown for an imported type: its definition rendered from the owning
/// spec, with a note where it came from. Unresolved targets get a note instead.
fn import_type_hover(
    spec_raw: &str,
    type_name: &str,
    current_file: &Path,
    workspace_root: Option<&Path>,
) -> String {
    match read_target(spec_raw, current_file, workspace_root) {
        Some((_, src, doc)) => match doc.all_type_defs().find(|t| t.name().value == type_name) {
            Some(def) => {
                let span = type_def_span(def);
                format!(
                    "```hmml\n{}\n```\n\n_imported from `{spec_raw}`_",
                    &src[span.start..span.end]
                )
            }
            None => format!("_type `{type_name}` is not defined in `{spec_raw}`_"),
        },
        None => format!("_cannot resolve import `{spec_raw}`_"),
    }
}

/// The `(owner-unit, type)` a references request targets when the cursor is on
/// an imported type name or on a local type definition — used to find every
/// `use` site importing that type across the task's spec directory.
fn import_reference_target(ds: &DocState, offset: usize) -> Option<(String, String)> {
    // Cursor on an imported type name: owner is the referenced unit.
    if let Some((spec, ty)) = analysis::import_type_at(&ds.doc, offset) {
        let (_system, unit) = mml_parser::ast::split_spec_ref(&spec.value);
        return Some((unit.to_string(), ty.value.clone()));
    }
    // Cursor on a local type name (its definition or a use of it) in a
    // type-declaring section: this file's unit owns it, so importers reference
    // it by this unit.
    if analysis::section_at(&ds.text, offset).declares_types() {
        let word = analysis::word_at(&ds.text, offset)?;
        if analysis::type_named(&ds.symbols, &word.text).is_some()
            && let Some(unit) = &ds.doc.header.unit
        {
            return Some((unit.value.clone(), word.text));
        }
    }
    None
}

/// Scan the spec files under `root` for `use` imports of `target`
/// `(owner-unit, type)` and return the locations of the matching type names.
/// Mirrors [`spec_references`].
fn import_references(target: &(String, String), root: &Path) -> Vec<Location> {
    let (owner_unit, type_name) = target;
    let mut out = Vec::new();
    for unit in paths::list_spec_entries(root) {
        let path = root.join(format!("{unit}.{}", mml_parser::SPEC_EXT));
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc = mml_parser::parse(&src);
        if doc.all_imports().next().is_none() {
            continue;
        }
        let Ok(uri) = Url::from_file_path(&path) else {
            continue;
        };
        let li = LineIndex::new(&src);
        for import in doc.all_imports() {
            let (_rsystem, runit) = mml_parser::ast::split_spec_ref(&import.spec.value);
            if runit != owner_unit {
                continue;
            }
            for ty in &import.types {
                if &ty.value == type_name {
                    out.push(Location {
                        uri: uri.clone(),
                        range: range_of(&li, ty.span),
                    });
                }
            }
        }
    }
    out
}

// ---- pure request handlers (borrow a DocState, return owned lsp values) ----

fn hover_at(
    ds: &DocState,
    uri: &Url,
    workspace_root: Option<&Path>,
    pos: Position,
) -> Option<Hover> {
    let offset = ds.offset(pos);

    // Cross-spec ref: summarize the referenced spec.
    if let Some(r) = analysis::spec_ref_at(&ds.doc, offset) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: spec_hover(&r.value, uri, workspace_root),
            }),
            range: Some(ds.range(r.span)),
        });
    }

    // Type import (§4.5): on the imported type, show its definition; on the
    // `use` ref, summarize the owning spec.
    if let Some((spec, ty)) = analysis::import_type_at(&ds.doc, offset) {
        let value = uri
            .to_file_path()
            .ok()
            .map(|f| import_type_hover(&spec.value, &ty.value, &f, workspace_root))
            .unwrap_or_else(|| format!("_cannot resolve import `{}`_", spec.value));
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value,
            }),
            range: Some(ds.range(ty.span)),
        });
    }
    if let Some(r) = analysis::import_ref_at(&ds.doc, offset) {
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: spec_hover(&r.value, uri, workspace_root),
            }),
            range: Some(ds.range(r.span)),
        });
    }

    let word = analysis::word_at(&ds.text, offset)?;
    let section = analysis::section_at(&ds.text, offset);
    let markdown = match section {
        SectionKind::Behavior | SectionKind::Invariants => module_hover(ds, &word.text)
            .or_else(|| system_hover(ds, &word.text))
            .or_else(|| type_hover(ds, &word.text)),
        SectionKind::Modules => module_hover(ds, &word.text),
        // In an access block a name is a system, a declared world type, or a
        // Bevy built-in the spec never had to declare.
        s if s.has_access() => system_hover(ds, &word.text)
            .or_else(|| type_hover(ds, &word.text))
            .or_else(|| builtin_hover(&word.text)),
        s if s.declares_types() => type_hover(ds, &word.text),
        _ => None,
    }?;
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: markdown,
        }),
        range: Some(ds.range(word.span)),
    })
}

fn module_hover(ds: &DocState, name: &str) -> Option<String> {
    let m = analysis::module_named(&ds.symbols, name)?;
    let mut s = format!("**{}** — {}", m.name, m.responsibility);
    if let Some(p) = &m.path {
        s.push_str(&format!("\n\n`@ {}`", p.value));
    }
    Some(s)
}

fn type_hover(ds: &DocState, name: &str) -> Option<String> {
    let t = analysis::type_named(&ds.symbols, name)?;
    let src = &ds.text[t.def_span.start..t.def_span.end];
    let mut out = format!("```hmml\n{src}\n```");
    if t.role.is_some() {
        out.push_str(&format!("\n\n_{}_", t.detail()));
        // The whole point of role-checked access: answer "who touches this?"
        // without leaving the definition.
        let mentions = analysis::access_mentions(&ds.doc, name);
        if !mentions.is_empty() {
            let mut keys: Vec<&str> = mentions.iter().map(|(k, _)| *k).collect();
            keys.sort_unstable();
            keys.dedup();
            out.push_str(&format!(
                "\n\n{} access site(s): {}",
                mentions.len(),
                keys.join(", ")
            ));
        }
    }
    Some(out)
}

/// Hover for a `## systems` entry: its schedule, ordering, and access lists.
fn system_hover(ds: &DocState, name: &str) -> Option<String> {
    let sym = analysis::system_named(&ds.symbols, name)?;
    let system = ds.doc.systems.iter().find(|s| s.name.value == name)?;
    let mut out = format!("**{}** — {}", sym.name, sym.responsibility);
    let access = &system.access;
    if let Some(schedule) = &sym.schedule {
        out.push_str(&format!("\n\n`schedule: {schedule}`"));
    }
    for (label, refs) in [("after", &access.after), ("before", &access.before)] {
        if !refs.is_empty() {
            let names: Vec<&str> = refs.iter().map(|r| r.value.as_str()).collect();
            out.push_str(&format!("\n\n`{label}: {}`", names.join(", ")));
        }
    }
    if let Some(run_if) = &access.run_if {
        out.push_str(&format!("\n\n`run_if: {}`", run_if.value));
    }
    for (key, _roles, items) in access.world_lists() {
        if items.is_empty() {
            continue;
        }
        let names: Vec<&str> = items.iter().map(|i| i.value.as_str()).collect();
        out.push_str(&format!("\n- **{key}** — {}", names.join(", ")));
    }
    if let Some(path) = &sym.path {
        out.push_str(&format!("\n\n`@ {}`", path.value));
    }
    Some(out)
}

/// Hover for a Bevy built-in named in an access list but never declared here.
fn builtin_hover(name: &str) -> Option<String> {
    let role = mml_parser::validate::builtin_role(name)?;
    Some(format!(
        "**{name}**\n\n_built-in Bevy {}; no declaration needed_",
        role.noun()
    ))
}

/// Markdown summary of the spec an `@ spec` ref points at: `system/unit`,
/// kind, and its modules. Unresolved refs get an explanatory note instead.
fn spec_hover(raw: &str, uri: &Url, workspace_root: Option<&Path>) -> String {
    let resolved = uri
        .to_file_path()
        .ok()
        .and_then(|f| paths::resolve_spec_ref(raw, &f, workspace_root));
    let target = match resolved {
        Some((target, true)) => target,
        Some((target, false)) => return format!("_target not found: {}_", target.display()),
        None => return format!("_cannot resolve spec `{raw}`_"),
    };
    let Ok(src) = std::fs::read_to_string(&target) else {
        return format!("_cannot read {}_", target.display());
    };
    let doc = mml_parser::parse(&src);
    let header = &doc.header;
    let name = match (&header.system, &header.unit) {
        (Some(s), Some(u)) => format!("{}/{}", s.value, u.value),
        _ => raw.to_string(),
    };
    let kind = header
        .kind
        .as_ref()
        .map(|k| format!(" ({:?})", k.value).to_lowercase())
        .unwrap_or_default();
    let mut out = format!("**{name}**{kind}");
    for m in doc.modules.iter().take(8) {
        out.push_str(&format!(
            "\n- **{}** — {}",
            m.name.value, m.responsibility.value
        ));
    }
    out
}

fn definition_at(
    ds: &DocState,
    uri: &Url,
    workspace_root: Option<&Path>,
    pos: Position,
) -> Option<GotoDefinitionResponse> {
    let offset = ds.offset(pos);

    // Cross-spec ref: jump to the referenced `.hmml` file.
    if let Some(r) = analysis::spec_ref_at(&ds.doc, offset) {
        let file = uri.to_file_path().ok()?;
        let (target, exists) = paths::resolve_spec_ref(&r.value, &file, workspace_root)?;
        if !exists {
            return None;
        }
        let target_uri = Url::from_file_path(&target).ok()?;
        let start = Position {
            line: 0,
            character: 0,
        };
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: Range { start, end: start },
        }));
    }

    // Type import (§4.5): on the imported type, jump to its definition in the
    // owning spec; on the `use` ref, jump to the owning spec file.
    if let Some((spec, ty)) = analysis::import_type_at(&ds.doc, offset) {
        let file = uri.to_file_path().ok()?;
        return import_type_definition(&spec.value, &ty.value, &file, workspace_root)
            .map(GotoDefinitionResponse::Scalar);
    }
    if let Some(r) = analysis::import_ref_at(&ds.doc, offset) {
        let file = uri.to_file_path().ok()?;
        let (target, exists) = paths::resolve_spec_ref(&r.value, &file, workspace_root)?;
        if !exists {
            return None;
        }
        let start = Position {
            line: 0,
            character: 0,
        };
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: Url::from_file_path(&target).ok()?,
            range: Range { start, end: start },
        }));
    }

    // Goto-into-real-source: cursor on an `@ path` or a `source.paths` entry.
    if let Some(raw) = path_target_at(ds, offset) {
        let current_file = uri.to_file_path().ok();
        let current_dir = current_file
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        // Resolution order: the spec's declared `base:` first, then the
        // `source.repo` checkout roots (`source.paths` are relative to the
        // checkout root), then the current file's directory, then the
        // workspace root.
        let base = ds
            .doc
            .header
            .base
            .as_ref()
            .and_then(|b| paths::base_dir(&b.value, current_dir.as_deref(), workspace_root));
        let repo_dirs = match (&ds.doc.header.source, &current_file) {
            (Some(src), Some(file)) => src
                .repo
                .as_ref()
                .map(|r| paths::repo_dirs(&r.value, file, workspace_root))
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let search_dirs: Vec<&Path> = base
            .as_deref()
            .into_iter()
            .chain(repo_dirs.iter().map(PathBuf::as_path))
            .chain(current_dir.as_deref())
            .chain(workspace_root)
            .collect();
        let (raw, line, col) = paths::split_suffix(raw);
        let target = paths::resolve(raw, &search_dirs)?;
        let target_uri = Url::from_file_path(&target).ok()?;
        let position = Position {
            line: line.map(|l| l.saturating_sub(1)).unwrap_or(0),
            character: col.map(|c| c.saturating_sub(1)).unwrap_or(0),
        };
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: target_uri,
            range: Range {
                start: position,
                end: position,
            },
        }));
    }

    // Inside an access block, resolve by role rather than by bare word: an
    // access-list entry is a world item, an `after:`/`before:` target is a
    // system. Falls through to the generic lookup when the name is a Bevy
    // built-in or an imported type with no local declaration.
    if let Some((_key, _roles, item)) = analysis::access_item_at(&ds.doc, offset)
        && let Some(t) = analysis::type_named(&ds.symbols, &item.value)
    {
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: ds.range(t.name_span),
        }));
    }
    if let Some(r) = analysis::ordering_ref_at(&ds.doc, offset)
        && let Some(s) = analysis::system_named(&ds.symbols, &r.value)
    {
        return Some(GotoDefinitionResponse::Scalar(Location {
            uri: uri.clone(),
            range: ds.range(s.name_span),
        }));
    }

    // In-spec: module / system / type name → its definition, same document.
    let word = analysis::word_at(&ds.text, offset)?;
    let section = analysis::section_at(&ds.text, offset);
    let span = if section.declares_types() {
        analysis::type_named(&ds.symbols, &word.text).map(|t| t.name_span)
    } else {
        analysis::module_named(&ds.symbols, &word.text)
            .map(|m| m.name_span)
            .or_else(|| analysis::system_named(&ds.symbols, &word.text).map(|s| s.name_span))
            .or_else(|| analysis::type_named(&ds.symbols, &word.text).map(|t| t.name_span))
    }?;
    Some(GotoDefinitionResponse::Scalar(Location {
        uri: uri.clone(),
        range: ds.range(span),
    }))
}

/// If `offset` sits on a module or system `@ path`, or a header `source.paths`
/// entry, return the raw path string.
fn path_target_at(ds: &DocState, offset: usize) -> Option<&str> {
    let within = |span: Span| offset >= span.start && offset < span.end;
    for m in &ds.doc.modules {
        if let Some(p) = &m.path
            && within(p.span)
        {
            return Some(&p.value);
        }
    }
    for s in &ds.doc.systems {
        if let Some(p) = &s.path
            && within(p.span)
        {
            return Some(&p.value);
        }
    }
    if let Some(src) = &ds.doc.header.source {
        for p in &src.paths {
            if within(p.span) {
                return Some(&p.value);
            }
        }
    }
    None
}

fn references_at(ds: &DocState, uri: &Url, pos: Position, include_decl: bool) -> Vec<Location> {
    let offset = ds.offset(pos);
    let Some(word) = analysis::word_at(&ds.text, offset) else {
        return Vec::new();
    };

    // Only meaningful for a declared module, system, or type.
    let decl_span = analysis::module_named(&ds.symbols, &word.text)
        .map(|m| m.name_span)
        .or_else(|| analysis::system_named(&ds.symbols, &word.text).map(|s| s.name_span))
        .or_else(|| analysis::type_named(&ds.symbols, &word.text).map(|t| t.name_span));
    if decl_span.is_none() {
        return Vec::new();
    }

    // Behavior prose mentions, plus every access list naming it — the latter is
    // what answers "which systems write this component?".
    let mentions = analysis::find_mentions(&ds.doc.behavior, &word.text)
        .into_iter()
        .chain(
            analysis::access_mentions(&ds.doc, &word.text)
                .into_iter()
                .map(|(_key, span)| span),
        )
        .chain(
            ds.doc
                .access_blocks()
                .into_iter()
                .flat_map(|(_, b)| b.ordering_refs())
                .filter(|r| r.value == word.text)
                .map(|r| r.span),
        );
    let mut locs: Vec<Location> = mentions
        .map(|span| Location {
            uri: uri.clone(),
            range: ds.range(span),
        })
        .collect();
    if include_decl && let Some(span) = decl_span {
        locs.push(Location {
            uri: uri.clone(),
            range: ds.range(span),
        });
    }
    locs
}

/// The `unit` a references request at `offset` targets: the spec ref under the
/// cursor, or the current file's own unit when the cursor is on the header
/// `unit:` value (i.e. "who references this spec?").
fn spec_reference_target(ds: &DocState, offset: usize) -> Option<String> {
    if let Some(r) = analysis::spec_ref_at(&ds.doc, offset) {
        let (_system, unit) = mml_parser::ast::split_spec_ref(&r.value);
        return Some(unit.to_string());
    }
    let header = &ds.doc.header;
    let within = |sp: Span| offset >= sp.start && offset <= sp.end;
    if let Some(unit) = &header.unit
        && within(unit.span)
    {
        return Some(unit.value.clone());
    }
    None
}

/// Scan the spec files under `root` for `@ spec` refs resolving to `target`
/// (a unit name) and return their locations. Every spec is read and parsed on
/// demand — the files are tiny, so this stays well under interactive latency.
fn spec_references(target: &str, root: &Path) -> Vec<Location> {
    let mut out = Vec::new();
    for unit in paths::list_spec_entries(root) {
        let path = root.join(format!("{unit}.{}", mml_parser::SPEC_EXT));
        let Ok(src) = std::fs::read_to_string(&path) else {
            continue;
        };
        let doc = mml_parser::parse(&src);
        let refs = mml_parser::validate::collect_spec_refs(&doc);
        if refs.is_empty() {
            continue;
        }
        let Ok(uri) = Url::from_file_path(&path) else {
            continue;
        };
        let li = LineIndex::new(&src);
        for r in refs {
            let (_rsystem, runit) = mml_parser::ast::split_spec_ref(&r.value);
            if runit == target {
                out.push(Location {
                    uri: uri.clone(),
                    range: range_of(&li, r.span),
                });
            }
        }
    }
    out
}

#[allow(deprecated)]
fn doc_symbol(
    name: String,
    detail: Option<String>,
    kind: SymbolKind,
    range: Range,
    selection_range: Range,
    children: Vec<DocumentSymbol>,
) -> DocumentSymbol {
    DocumentSymbol {
        name,
        detail,
        kind,
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    }
}

fn document_symbols(ds: &DocState) -> Vec<DocumentSymbol> {
    let mut out = Vec::new();
    for (kind, range) in analysis::sections(&ds.text) {
        let header = ds.range(Span::new(range.start, range.start));
        let full = ds.range(Span::new(range.start, range.end));
        let (name, children) = match kind {
            SectionKind::Modules => ("modules", module_symbols(ds)),
            SectionKind::Data => ("data", type_symbols(ds, &ds.doc.data.types)),
            SectionKind::World(role) => {
                let section = ds
                    .doc
                    .world_sections()
                    .into_iter()
                    .find(|w| w.role == role)
                    .expect("one section per role");
                (role.section(), type_symbols(ds, &section.types))
            }
            SectionKind::Systems => ("systems", system_symbols(ds)),
            SectionKind::Access => ("access", Vec::new()),
            SectionKind::Behavior => ("behavior", Vec::new()),
            SectionKind::Invariants => ("invariants", Vec::new()),
            SectionKind::Header | SectionKind::Unknown => continue,
        };
        out.push(doc_symbol(
            name.to_string(),
            None,
            SymbolKind::NAMESPACE,
            full,
            header,
            children,
        ));
    }
    out
}

fn module_symbols(ds: &DocState) -> Vec<DocumentSymbol> {
    ds.doc
        .modules
        .iter()
        .map(|m| {
            doc_symbol(
                m.name.value.clone(),
                Some(m.responsibility.value.clone()),
                SymbolKind::MODULE,
                ds.range(m.span),
                ds.range(m.name.span),
                Vec::new(),
            )
        })
        .collect()
}

/// Document symbols for one system entry, with its schedule as the detail and
/// its access lists flattened into children — the outline answers "what does
/// this system touch?" without opening the block.
fn system_symbols(ds: &DocState) -> Vec<DocumentSymbol> {
    ds.doc
        .systems
        .iter()
        .map(|s| {
            let mut children = Vec::new();
            for (key, _roles, items) in s.access.world_lists() {
                for item in items {
                    children.push(doc_symbol(
                        item.value.clone(),
                        Some(key.to_string()),
                        SymbolKind::FIELD,
                        ds.range(item.span),
                        ds.range(item.span),
                        Vec::new(),
                    ));
                }
            }
            let detail = match &s.access.schedule {
                Some(schedule) => format!("{} — {}", schedule.value, s.responsibility.value),
                None => s.responsibility.value.clone(),
            };
            doc_symbol(
                s.name.value.clone(),
                Some(detail),
                SymbolKind::FUNCTION,
                ds.range(s.span),
                ds.range(s.name.span),
                children,
            )
        })
        .collect()
}

fn type_symbols(ds: &DocState, types: &[TypeDef]) -> Vec<DocumentSymbol> {
    types
        .iter()
        .map(|t| match t {
            TypeDef::Struct { name, fields, span } => {
                let children = fields
                    .iter()
                    .map(|f| {
                        doc_symbol(
                            f.name.value.clone(),
                            Some(ds.text[f.ty.span.start..f.ty.span.end].to_string()),
                            SymbolKind::FIELD,
                            ds.range(f.span),
                            ds.range(f.name.span),
                            Vec::new(),
                        )
                    })
                    .collect();
                doc_symbol(
                    name.value.clone(),
                    None,
                    SymbolKind::STRUCT,
                    ds.range(*span),
                    ds.range(name.span),
                    children,
                )
            }
            TypeDef::Enum {
                name,
                variants,
                span,
            } => {
                let children = variants
                    .iter()
                    .map(|v| {
                        doc_symbol(
                            v.value.clone(),
                            None,
                            SymbolKind::ENUM_MEMBER,
                            ds.range(v.span),
                            ds.range(v.span),
                            Vec::new(),
                        )
                    })
                    .collect();
                doc_symbol(
                    name.value.clone(),
                    None,
                    SymbolKind::ENUM,
                    ds.range(*span),
                    ds.range(name.span),
                    children,
                )
            }
            TypeDef::Marker { name, span } => doc_symbol(
                name.value.clone(),
                Some("marker".to_string()),
                SymbolKind::STRUCT,
                ds.range(*span),
                ds.range(name.span),
                Vec::new(),
            ),
        })
        .collect()
}

/// Spec-repo context for `@ spec` completions, gathered from the filesystem by
/// the completion handler (the pure logic stays in [`completions_at`]).
struct SpecEntries {
    /// The unit names (`.hmml` stems) in the current file's own directory.
    entries: Vec<String>,
    /// The current file's unit (its file stem) — excluded from completions.
    current_unit: Option<String>,
}

impl SpecEntries {
    fn gather(uri: &Url) -> Option<SpecEntries> {
        let file = uri.to_file_path().ok()?;
        let root = paths::spec_root(&file)?;
        Some(SpecEntries {
            entries: paths::list_spec_entries(&root),
            current_unit: file.file_stem().and_then(|s| s.to_str()).map(String::from),
        })
    }
}

fn completions_at(
    ds: &DocState,
    pos: Position,
    spec_entries: Option<&SpecEntries>,
    import_types: &[String],
) -> Vec<CompletionItem> {
    fn item(label: impl Into<String>, kind: CompletionItemKind, detail: &str) -> CompletionItem {
        CompletionItem {
            label: label.into(),
            kind: Some(kind),
            detail: Some(detail.to_string()),
            ..Default::default()
        }
    }
    fn type_item(t: &analysis::TypeSym) -> CompletionItem {
        let kind = match t.shape {
            analysis::TypeShape::Enum => CompletionItemKind::ENUM,
            _ => CompletionItemKind::STRUCT,
        };
        CompletionItem {
            label: t.name.clone(),
            kind: Some(kind),
            detail: Some(t.detail()),
            ..Default::default()
        }
    }
    let names = |ds: &DocState| -> Vec<CompletionItem> {
        let modules = ds
            .symbols
            .modules
            .iter()
            .map(|m| item(m.name.clone(), CompletionItemKind::MODULE, "module"));
        let systems = ds
            .symbols
            .systems
            .iter()
            .map(|s| item(s.name.clone(), CompletionItemKind::FUNCTION, "system"));
        let types = ds.symbols.types.iter().map(type_item);
        modules.chain(systems).chain(types).collect()
    };

    let offset = ds.offset(pos);
    match analysis::completion_context(&ds.text, offset) {
        CompletionContext::SectionHeader => analysis::SECTION_HEADERS
            .iter()
            .map(|h| item(*h, CompletionItemKind::KEYWORD, "section"))
            .collect(),
        CompletionContext::HeaderKey => analysis::HEADER_KEYS
            .iter()
            .map(|k| item(format!("{k}:"), CompletionItemKind::FIELD, "header key"))
            .collect(),
        CompletionContext::SourceKey => analysis::SOURCE_KEYS
            .iter()
            .map(|k| item(format!("{k}:"), CompletionItemKind::FIELD, "source key"))
            .collect(),
        CompletionContext::KindValue => analysis::KIND_VALUES
            .iter()
            .map(|v| item(*v, CompletionItemKind::ENUM_MEMBER, "kind"))
            .collect(),
        CompletionContext::MethodValue => analysis::METHOD_VALUES
            .iter()
            .map(|v| item(*v, CompletionItemKind::ENUM_MEMBER, "method"))
            .collect(),
        CompletionContext::AccessKey => analysis::ACCESS_KEYS
            .iter()
            .map(|k| item(format!("{k}:"), CompletionItemKind::FIELD, "access key"))
            .collect(),
        CompletionContext::AccessValue(value) => match value {
            analysis::AccessValue::Schedule => analysis::SCHEDULES
                .iter()
                .map(|s| item(*s, CompletionItemKind::ENUM_MEMBER, "schedule"))
                .collect(),
            analysis::AccessValue::Ordering => ds
                .symbols
                .systems
                .iter()
                .map(|s| item(s.name.clone(), CompletionItemKind::FUNCTION, "system"))
                .chain(
                    ds.doc
                        .access_blocks()
                        .into_iter()
                        .flat_map(|(_, b)| b.sets.iter())
                        .map(|s| {
                            item(s.value.clone(), CompletionItemKind::INTERFACE, "system set")
                        }),
                )
                .collect(),
            analysis::AccessValue::Free => Vec::new(),
            // Role-filtered: only names that fit this key, declared here or
            // built into Bevy.
            v => {
                let roles = v.roles();
                let mut out: Vec<CompletionItem> = roles
                    .iter()
                    .flat_map(|r| ds.symbols.with_role(*r))
                    .map(type_item)
                    .collect();
                for role in roles {
                    let (builtins, detail) = match role {
                        mml_parser::ast::Role::Component => {
                            (analysis::BUILTIN_COMPONENTS, "bevy component")
                        }
                        mml_parser::ast::Role::Resource => {
                            (analysis::BUILTIN_RESOURCES, "bevy resource")
                        }
                        mml_parser::ast::Role::Event => (analysis::BUILTIN_EVENTS, "bevy event"),
                    };
                    out.extend(
                        builtins
                            .iter()
                            .map(|b| item(*b, CompletionItemKind::STRUCT, detail)),
                    );
                }
                out
            }
        },
        CompletionContext::Type => {
            let mut out: Vec<CompletionItem> = ds.symbols.types.iter().map(type_item).collect();
            out.extend(
                analysis::PRIMITIVES
                    .iter()
                    .map(|p| item(*p, CompletionItemKind::VALUE, "primitive")),
            );
            out.extend(
                analysis::OPAQUE
                    .iter()
                    .map(|o| item(*o, CompletionItemKind::STRUCT, "opaque type")),
            );
            out.extend(
                analysis::CONTAINERS
                    .iter()
                    .map(|c| item(*c, CompletionItemKind::CLASS, "container")),
            );
            out
        }
        CompletionContext::BehaviorKeyword => {
            let mut out: Vec<CompletionItem> = analysis::CONTROL_FLOW
                .iter()
                .map(|k| item(*k, CompletionItemKind::KEYWORD, "control flow"))
                .collect();
            out.extend(names(ds));
            out
        }
        CompletionContext::BehaviorProse => names(ds),
        CompletionContext::ImportType => import_types
            .iter()
            .map(|t| item(t.clone(), CompletionItemKind::STRUCT, "imported type"))
            .collect(),
        CompletionContext::SpecRef | CompletionContext::ImportRef => {
            let Some(se) = spec_entries else {
                return Vec::new();
            };
            let mut out = Vec::new();
            for unit in &se.entries {
                if se.current_unit.as_deref() == Some(unit.as_str()) {
                    continue; // no self-references
                }
                out.push(item(unit.clone(), CompletionItemKind::FILE, "spec"));
            }
            out
        }
        CompletionContext::Other => Vec::new(),
    }
}

// ---- conversions -----------------------------------------------------------

fn to_lsp_pos(p: mml_parser::Position) -> Position {
    Position {
        line: p.line,
        character: p.character,
    }
}

fn to_mml_pos(p: Position) -> mml_parser::Position {
    mml_parser::Position {
        line: p.line,
        character: p.character,
    }
}

fn to_lsp_diagnostic(li: &LineIndex, d: &mml_parser::Diagnostic) -> Diagnostic {
    Diagnostic {
        range: range_of(li, d.span),
        severity: Some(match d.severity {
            Severity::Error => DiagnosticSeverity::ERROR,
            Severity::Warning => DiagnosticSeverity::WARNING,
            Severity::Info => DiagnosticSeverity::INFORMATION,
        }),
        code: Some(NumberOrString::String(d.code.to_string())),
        // Both dialects' servers can be live at once; this is what Helix prints
        // beside a diagnostic, so it must say which one spoke.
        source: Some(mml_parser::SPEC_EXT.to_string()),
        message: d.message.clone(),
        ..Default::default()
    }
}

// ---- protocol --------------------------------------------------------------

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let root = params
            .workspace_folders
            .as_ref()
            .and_then(|folders| folders.first())
            .and_then(|f| f.uri.to_file_path().ok())
            .or_else(|| {
                #[allow(deprecated)]
                params.root_uri.as_ref().and_then(|u| u.to_file_path().ok())
            });
        if let Some(root) = root {
            let _ = self.workspace_root.set(root);
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ":".into(),
                        " ".into(),
                        "<".into(),
                        "@".into(),
                        "/".into(),
                    ]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "hobby-mml-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {}

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let td = params.text_document;
        self.refresh(td.uri, td.text, Some(td.version)).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().next_back() {
            self.refresh(
                params.text_document.uri,
                change.text,
                Some(params.text_document.version),
            )
            .await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents
            .lock()
            .unwrap()
            .remove(&params.text_document.uri);
        self.client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let p = params.text_document_position_params;
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&p.text_document.uri).and_then(|ds| {
            hover_at(
                ds,
                &p.text_document.uri,
                self.workspace_root.get().map(PathBuf::as_path),
                p.position,
            )
        }))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let p = params.text_document_position_params;
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&p.text_document.uri).and_then(|ds| {
            definition_at(
                ds,
                &p.text_document.uri,
                self.workspace_root.get().map(PathBuf::as_path),
                p.position,
            )
        }))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let p = params.text_document_position;
        // Filesystem scan outside the documents lock.
        let spec_entries = SpecEntries::gather(&p.text_document.uri);
        let workspace_root = self.workspace_root.get().cloned();
        let docs = self.documents.lock().unwrap();
        Ok(docs.get(&p.text_document.uri).map(|ds| {
            // For `use <ref>: ` completion, list the types the referenced spec
            // defines (resolved against the filesystem; files are tiny).
            let offset = ds.offset(p.position);
            let import_types = analysis::import_on_line(&ds.doc, offset)
                .and_then(|imp| {
                    let file = p.text_document.uri.to_file_path().ok()?;
                    let (target, true) =
                        paths::resolve_spec_ref(&imp.spec.value, &file, workspace_root.as_deref())?
                    else {
                        return None;
                    };
                    Some(paths::defined_type_names(&target))
                })
                .unwrap_or_default();
            CompletionResponse::Array(completions_at(
                ds,
                p.position,
                spec_entries.as_ref(),
                &import_types,
            ))
        }))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let docs = self.documents.lock().unwrap();
        Ok(docs
            .get(&params.text_document.uri)
            .map(|ds| DocumentSymbolResponse::Nested(document_symbols(ds))))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let p = params.text_document_position;
        // In-file module/type mentions first; when the cursor isn't on one of
        // those, fall back to cross-spec references (computed after releasing
        // the lock — the scan reads sibling files).
        let (local, import_lookup, spec_lookup) = {
            let docs = self.documents.lock().unwrap();
            let Some(ds) = docs.get(&p.text_document.uri) else {
                return Ok(None);
            };
            let offset = ds.offset(p.position);
            let file = p.text_document.uri.to_file_path().ok();
            let local = references_at(
                ds,
                &p.text_document.uri,
                p.position,
                params.context.include_declaration,
            );
            // Type imports: `use` sites of the type under the cursor, across the
            // spec repo. Independent of the in-file mentions, so it augments them.
            let import_lookup = match &file {
                Some(f) => import_reference_target(ds, offset).map(|t| (t, f.clone())),
                None => None,
            };
            // Cross-spec `@ spec` references only when nothing more specific hit.
            let spec_lookup = if local.is_empty() && import_lookup.is_none() {
                match &file {
                    Some(f) => spec_reference_target(ds, offset).map(|t| (t, f.clone())),
                    None => None,
                }
            } else {
                None
            };
            (local, import_lookup, spec_lookup)
        };

        let mut out = local;
        if let Some((target, file)) = import_lookup {
            let root = paths::spec_root(&file).or_else(|| self.workspace_root.get().cloned());
            if let Some(r) = root {
                out.extend(import_references(&target, &r));
            }
        }
        if out.is_empty()
            && let Some((target, file)) = spec_lookup
        {
            let root = paths::spec_root(&file).or_else(|| self.workspace_root.get().cloned());
            out = root
                .map(|r| spec_references(&target, &r))
                .unwrap_or_default();
        }
        Ok(Some(out))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str =
        include_str!("../../mml-parser/tests/fixtures/examples/mesh-terrain-chunks.hmml");

    fn state() -> DocState {
        let text = SAMPLE.to_string();
        let (doc, _) = analyze(&text);
        let line_index = LineIndex::new(&text);
        let symbols = analysis::symbols(&doc);
        DocState {
            text,
            doc,
            line_index,
            symbols,
        }
    }

    fn at(text: &str, needle: &str, extra: u32) -> Position {
        let off = text.find(needle).unwrap() + extra as usize;
        to_lsp_pos(LineIndex::new(text).position(off))
    }

    fn uri() -> Url {
        Url::parse("file:///specs/mesh-terrain-chunks.hmml").unwrap()
    }

    #[test]
    fn hover_module_shows_responsibility() {
        let ds = state();
        // "HeightSampler" inside a behavior step.
        let pos = at(
            SAMPLE,
            "sample a HeightTile from HeightSampler",
            "sample a HeightTile from ".len() as u32,
        );
        let h = hover_at(&ds, &uri(), None, pos).expect("hover");
        let HoverContents::Markup(m) = h.contents else {
            panic!("expected markup")
        };
        assert!(m.value.contains("HeightSampler"));
        assert!(m.value.contains("samples the erosion heightfield"));
    }

    #[test]
    fn hover_type_shows_definition() {
        let ds = state();
        let pos = at(
            SAMPLE,
            "out: Stream<ChunkMesh>",
            "out: Stream<".len() as u32,
        );
        let h = hover_at(&ds, &uri(), None, pos).expect("hover");
        let HoverContents::Markup(m) = h.contents else {
            panic!("markup")
        };
        assert!(m.value.contains("struct ChunkMesh"));
    }

    #[test]
    fn goto_module_in_behavior() {
        let ds = state();
        let pos = at(
            SAMPLE,
            "sample a HeightTile from HeightSampler",
            "sample a HeightTile from ".len() as u32,
        );
        let resp = definition_at(&ds, &uri(), None, pos).expect("definition");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert_eq!(loc.uri, uri());
        // Jumps to the HeightSampler declaration line in `## modules`.
        let decl = at(SAMPLE, "HeightSampler  —", 0);
        assert_eq!(loc.range.start.line, decl.line);
    }

    #[test]
    fn goto_into_source_resolves_path() {
        use std::fs;
        // Build a doc whose module @ path exists on disk under a temp workspace.
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src/chunk");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("scheduler.rs"), "x").unwrap();

        let ds = state();
        // Offset on the `@ path` of the ChunkScheduler module.
        let text = &ds.text;
        let at_off = text.find("src/chunk/scheduler.rs").unwrap() + 2; // inside the path token
        let pos = to_lsp_pos(ds.line_index.position(at_off));
        let resp = definition_at(&ds, &uri(), Some(dir.path()), pos).expect("definition");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("src/chunk/scheduler.rs"));
    }

    #[test]
    fn goto_into_source_uses_base() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("pkg");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("worker.py"), "x").unwrap();

        let text = format!(
            "system: s\nunit: u\nkind: job\nbase: {}\n\n## modules\nWorker -- does work @ pkg/worker.py\n\n## data\nin:  String\nout: String\n\n## behavior\n- run\n\n## invariants\n- ok\n",
            dir.path().display()
        );
        let (doc, _) = analyze(&text);
        assert!(doc.header.base.is_some(), "base header parsed");
        let line_index = LineIndex::new(&text);
        let symbols = analysis::symbols(&doc);
        let ds = DocState {
            text,
            doc,
            line_index,
            symbols,
        };
        // Cursor on the module's `@ path`; no workspace root, so resolution must
        // come from the declared `base:`.
        let at_off = ds.text.find("pkg/worker.py").unwrap() + 2;
        let pos = to_lsp_pos(ds.line_index.position(at_off));
        let resp = definition_at(&ds, &uri(), None, pos).expect("definition via base");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("pkg/worker.py"));
    }

    #[test]
    fn goto_into_source_uses_source_repo() {
        use std::fs;
        // `source.paths` are relative to the `source.repo` checkout root. The
        // checkout is found under the workspace root here (the worktree and
        // `~/projects` candidates are covered by paths.rs unit tests).
        let task = tempfile::tempdir().unwrap();
        let spec_dir = task.path().join("notes/topic/task");
        fs::create_dir_all(&spec_dir).unwrap();
        let pkg = task.path().join("request-worker/pkg");
        fs::create_dir_all(&pkg).unwrap();
        fs::write(pkg.join("worker.py"), "x").unwrap();

        let text = "system: sys\nunit: u\nkind: job\nsource:\n  repo: request-worker\n  paths:\n    - pkg/worker.py\n\n## modules\nWorker -- does work @ pkg/worker.py\n\n## data\nin:  String\nout: String\n\n## behavior\n- run\n\n## invariants\n- ok\n".to_string();
        let (doc, _) = analyze(&text);
        assert!(doc.header.source.is_some(), "source header parsed");
        let line_index = LineIndex::new(&text);
        let symbols = analysis::symbols(&doc);
        let ds = DocState {
            text,
            doc,
            line_index,
            symbols,
        };
        let file_uri = Url::from_file_path(spec_dir.join("u.hmml")).unwrap();

        // Cursor on the `source.paths` entry; resolution comes from the checkout
        // named by `source.repo`, found under the workspace root.
        let at_off = ds.text.find("- pkg/worker.py").unwrap() + 4;
        let pos = to_lsp_pos(ds.line_index.position(at_off));
        let resp = definition_at(&ds, &file_uri, Some(task.path()), pos)
            .expect("definition via source.repo");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("request-worker/pkg/worker.py"));

        // The module's `@ path` resolves the same way.
        let at_off = ds.text.find("work @ pkg/worker.py").unwrap() + "work @ pk".len();
        let pos = to_lsp_pos(ds.line_index.position(at_off));
        let resp = definition_at(&ds, &file_uri, Some(task.path()), pos)
            .expect("module @ path via source.repo");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("request-worker/pkg/worker.py"));
    }

    #[test]
    fn document_symbols_lists_sections_and_defs() {
        let ds = state();
        let syms = document_symbols(&ds);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["modules", "data", "behavior", "invariants"]);
        let data = syms.iter().find(|s| s.name == "data").unwrap();
        let types: Vec<&str> = data
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(types, ["ChunkCoord", "HeightTile", "ChunkMesh"]);
    }

    #[test]
    fn completion_types_include_defined_and_primitives() {
        let ds = state();
        let pos = at(SAMPLE, "out: Stream", "out: ".len() as u32);
        let items = completions_at(&ds, pos, None, &[]);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"ChunkMesh"));
        assert!(labels.contains(&"String"));
        assert!(labels.contains(&"Vec"));
    }

    #[test]
    fn references_finds_module_mentions() {
        let ds = state();
        let pos = at(SAMPLE, "HeightSampler  —", 0); // declaration
        let refs = references_at(&ds, &uri(), pos, true);
        // one behavior mention + the declaration.
        assert_eq!(refs.len(), 2);
    }

    // --- cross-spec references ---

    fn mini_spec(system: &str, unit: &str, behavior: &str) -> String {
        format!(
            "system: {system}\nunit: {unit}\nkind: job\n\n## modules\n\n\
## data\nin: String\nout: String\n\n## behavior\n{behavior}\n\n## invariants\n"
        )
    }

    /// A temp task spec dir with sibling units one.hmml, two.hmml, other.hmml,
    /// where `one` references `other` and `two` (both bare) and a missing spec.
    fn spec_tree() -> (tempfile::TempDir, std::path::PathBuf, String) {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let one = mini_spec(
            "sys-a",
            "one",
            "- call the other unit @ spec other\n\
             - stay local @ spec two\n\
             - dangle @ spec missing",
        );
        for (unit, body) in [
            ("one", one.clone()),
            ("two", mini_spec("sys-a", "two", "- noop")),
            ("other", mini_spec("sys-b", "other", "- noop")),
        ] {
            fs::write(dir.path().join(format!("{unit}.hmml")), body).unwrap();
        }
        let path = dir.path().join("one.hmml");
        (dir, path, one)
    }

    fn state_of(text: &str) -> DocState {
        let text = text.to_string();
        let (doc, _) = analyze(&text);
        let line_index = LineIndex::new(&text);
        let symbols = analysis::symbols(&doc);
        DocState {
            text,
            doc,
            line_index,
            symbols,
        }
    }

    #[test]
    fn goto_spec_ref_resolves_same_dir() {
        let (_dir, path, text) = spec_tree();
        let ds = state_of(&text);
        let file_uri = Url::from_file_path(&path).unwrap();

        let pos = to_lsp_pos(
            ds.line_index
                .position(text.find("@ spec other").unwrap() + 7),
        );
        let resp = definition_at(&ds, &file_uri, None, pos).expect("goto ref");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("other.hmml"));

        let pos = to_lsp_pos(ds.line_index.position(text.find("@ spec two").unwrap() + 7));
        let resp = definition_at(&ds, &file_uri, None, pos).expect("goto second ref");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("two.hmml"));

        // Unresolved ref: no definition.
        let pos = to_lsp_pos(
            ds.line_index
                .position(text.find("@ spec missing").unwrap() + 7),
        );
        assert!(definition_at(&ds, &file_uri, None, pos).is_none());
    }

    #[test]
    fn spec_ref_completions_list_repo_entries() {
        let (_dir, path, text) = spec_tree();
        let ds = state_of(&text);
        let file_uri = Url::from_file_path(&path).unwrap();
        let se = SpecEntries::gather(&file_uri).expect("spec entries");

        // Cursor right after `@ spec ` on the first behavior line.
        let off = text.find("@ spec other").unwrap() + "@ spec ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let items = completions_at(&ds, pos, Some(&se), &[]);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        // Bare sibling units are offered; the current file itself is not.
        assert!(labels.contains(&"two"));
        assert!(labels.contains(&"other"));
        assert!(!labels.contains(&"one"));
    }

    #[test]
    fn unresolved_refs_warn_resolved_stay_silent() {
        let (_dir, path, text) = spec_tree();
        let (doc, _) = analyze(&text);
        let diags = lint::unresolved_spec_refs(&doc, &path, None);
        assert_eq!(diags.len(), 1, "only the dangling ref warns: {diags:#?}");
        assert_eq!(diags[0].code, "unresolved-spec-ref");
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(&text[diags[0].span.start..diags[0].span.end], "missing");
    }

    #[test]
    fn references_from_header_finds_incoming_refs() {
        let (dir, _path, _text) = spec_tree();
        // In other.hmml, ask for references while on the `unit:` value.
        let other_path = dir.path().join("other.hmml");
        let other_text = std::fs::read_to_string(&other_path).unwrap();
        let ds = state_of(&other_text);
        let offset = other_text.find("unit: other").unwrap() + "unit: ".len();
        let target = spec_reference_target(&ds, offset).expect("target");
        assert_eq!(target, "other");

        let locs = spec_references(&target, dir.path());
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.path().ends_with("one.hmml"));
    }

    #[test]
    fn references_on_a_ref_finds_all_refs_to_same_target() {
        let (dir, _path, text) = spec_tree();
        let ds = state_of(&text);
        // Cursor on the bare ref `two` → target is the `two` unit.
        let offset = text.find("@ spec two").unwrap() + 8;
        let target = spec_reference_target(&ds, offset).expect("target");
        assert_eq!(target, "two");
        let locs = spec_references(&target, dir.path());
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.path().ends_with("one.hmml"));
    }

    // --- type imports (grammar §4.5) ---

    /// A temp task spec dir: `model` defines `struct Offer`, and `importer`
    /// (a sibling) imports `Offer` (resolved) plus a `Missing` type
    /// (unresolved). Returns the dir, the importer's path, and its text.
    fn import_tree() -> (tempfile::TempDir, std::path::PathBuf, String) {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let owner = "system: owner\nunit: model\nkind: workflow\n\n## modules\n\n\
## data\nstruct Offer {\n  seller: String,\n}\nin: String\nout: String\n\n\
## behavior\n- noop\n\n## invariants\n";
        let importer = "system: consumer\nunit: importer\nkind: workflow\n\n## modules\n\n\
## data\nuse model: Offer\nuse model: Missing\nin: Offer\nout: Offer\n\n\
## behavior\n- handle an Offer\n\n## invariants\n";
        for (unit, body) in [("model", owner), ("importer", importer)] {
            fs::write(dir.path().join(format!("{unit}.hmml")), body).unwrap();
        }
        let path = dir.path().join("importer.hmml");
        (dir, path, importer.to_string())
    }

    #[test]
    fn goto_imported_type_jumps_to_owner_definition() {
        let (dir, path, text) = import_tree();
        let ds = state_of(&text);
        let file_uri = Url::from_file_path(&path).unwrap();
        // Cursor on `Offer` in the first `use` line.
        let off = text.find("use model: Offer").unwrap() + "use model: ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let resp = definition_at(&ds, &file_uri, None, pos).expect("goto imported type");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("model.hmml"));
        // It points at the `struct Offer` name line, not 0:0.
        let owner_src = std::fs::read_to_string(dir.path().join("model.hmml")).unwrap();
        let want = LineIndex::new(&owner_src).position(owner_src.find("Offer").unwrap());
        assert_eq!(loc.range.start.line, want.line);
    }

    #[test]
    fn goto_import_ref_jumps_to_owner_file() {
        let (_dir, path, text) = import_tree();
        let ds = state_of(&text);
        let file_uri = Url::from_file_path(&path).unwrap();
        // Cursor on the `model` ref itself.
        let off = text.find("use model").unwrap() + "use ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let resp = definition_at(&ds, &file_uri, None, pos).expect("goto import ref");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("model.hmml"));
    }

    #[test]
    fn hover_imported_type_shows_owner_definition() {
        let (_dir, path, text) = import_tree();
        let ds = state_of(&text);
        let file_uri = Url::from_file_path(&path).unwrap();
        let off = text.find("use model: Offer").unwrap() + "use model: ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let h = hover_at(&ds, &file_uri, None, pos).expect("hover");
        let HoverContents::Markup(m) = h.contents else {
            panic!("markup")
        };
        assert!(m.value.contains("struct Offer"));
        assert!(m.value.contains("imported from"));
    }

    #[test]
    fn import_type_completion_lists_owner_types() {
        let (dir, path, text) = import_tree();
        let ds = state_of(&text);
        // Types the owner defines, as the handler would compute them.
        let owner = dir.path().join("model.hmml");
        let import_types = paths::defined_type_names(&owner);
        assert!(import_types.contains(&"Offer".to_string()));
        // Cursor right after `use model: ` → offer the owner's types.
        let off = text.find("use model: Offer").unwrap() + "use model: ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let items = completions_at(&ds, pos, None, &import_types);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"Offer"));
        // And the ref position offers the sibling units.
        let se = SpecEntries::gather(&Url::from_file_path(&path).unwrap()).expect("entries");
        let off = text.find("use model").unwrap() + "use ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let items = completions_at(&ds, pos, Some(&se), &[]);
        let labels: Vec<&str> = items.iter().map(|i| i.label.as_str()).collect();
        assert!(labels.contains(&"model"));
    }

    #[test]
    fn unresolved_import_warns_on_missing_type_only() {
        let (_dir, path, text) = import_tree();
        let (doc, _) = analyze(&text);
        let diags = lint::unresolved_imports(&doc, &path, None);
        // `Offer` resolves; only `Missing` warns.
        assert_eq!(diags.len(), 1, "only the missing type warns: {diags:#?}");
        assert_eq!(diags[0].code, "unresolved-import");
        assert_eq!(diags[0].severity, Severity::Warning);
        assert_eq!(&text[diags[0].span.start..diags[0].span.end], "Missing");
    }

    #[test]
    fn import_references_finds_use_sites() {
        let (dir, _path, _text) = import_tree();
        // In model.hmml, ask for references while on the `struct Offer` name.
        let owner_path = dir.path().join("model.hmml");
        let owner_text = std::fs::read_to_string(&owner_path).unwrap();
        let ds = state_of(&owner_text);
        let offset = owner_text.find("Offer").unwrap() + 1;
        let target = import_reference_target(&ds, offset).expect("import target");
        assert_eq!(target, ("model".to_string(), "Offer".to_string()));
        let locs = import_references(&target, dir.path());
        assert_eq!(locs.len(), 1);
        assert!(locs[0].uri.path().ends_with("importer.hmml"));
    }

    // --- Bevy dialect handlers ---

    const PLUGIN: &str = include_str!("../../mml-parser/tests/fixtures/examples/enemy-waves.hmml");

    fn plugin_state() -> DocState {
        state_of(PLUGIN)
    }

    fn plugin_uri() -> Url {
        Url::parse("file:///specs/enemy-waves.hmml").unwrap()
    }

    #[test]
    fn document_symbols_list_bevy_sections_and_systems() {
        let ds = plugin_state();
        let syms = document_symbols(&ds);
        let names: Vec<&str> = syms.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "modules",
                "data",
                "components",
                "resources",
                "events",
                "systems",
                "behavior",
                "invariants"
            ]
        );

        // `## data` lists the role-less shapes — an enum and a struct that are
        // neither component nor resource nor event.
        let data = syms.iter().find(|s| s.name == "data").unwrap();
        let dtypes: Vec<(&str, SymbolKind)> = data
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| (s.name.as_str(), s.kind))
            .collect();
        assert_eq!(
            dtypes,
            [
                ("SpawnPattern", SymbolKind::ENUM),
                ("SpawnPoint", SymbolKind::STRUCT)
            ]
        );

        // The world sections list their own types, not each other's.
        let components = syms.iter().find(|s| s.name == "components").unwrap();
        let ctypes: Vec<&str> = components
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(ctypes, ["Wave", "EnemyHealth", "Spawner"]);
        let resources = syms.iter().find(|s| s.name == "resources").unwrap();
        let rtypes: Vec<&str> = resources
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(rtypes, ["WaveConfig", "WaveState"]);

        // A system's detail leads with its schedule; its access list is nested.
        let systems = syms.iter().find(|s| s.name == "systems").unwrap();
        let children = systems.children.as_ref().unwrap();
        let names: Vec<&str> = children.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            ["tick_wave_timer", "spawn_enemies", "detect_cleared"]
        );
        assert!(
            children[1]
                .detail
                .as_deref()
                .unwrap()
                .starts_with("FixedUpdate — "),
            "{:?}",
            children[1].detail
        );
        let access: Vec<(&str, &str)> = children[1]
            .children
            .as_ref()
            .unwrap()
            .iter()
            .map(|c| (c.name.as_str(), c.detail.as_deref().unwrap()))
            .collect();
        assert!(access.contains(&("WaveState", "writes")), "{access:?}");
        assert!(access.contains(&("Spawner", "query")), "{access:?}");
    }

    #[test]
    fn hover_on_a_system_shows_schedule_and_access() {
        let ds = plugin_state();
        let pos = at(PLUGIN, "spawn_enemies — spawns", 2);
        let h = hover_at(&ds, &plugin_uri(), None, pos).expect("hover");
        let HoverContents::Markup(m) = h.contents else {
            panic!("markup")
        };
        assert!(m.value.contains("spawn_enemies"), "{}", m.value);
        assert!(m.value.contains("schedule: FixedUpdate"), "{}", m.value);
        assert!(m.value.contains("after: tick_wave_timer"), "{}", m.value);
        assert!(m.value.contains("**spawns**"), "{}", m.value);
    }

    #[test]
    fn hover_on_a_component_reports_its_role_and_access_sites() {
        let ds = plugin_state();
        // Cursor on `WaveState` in detect_cleared's `reads:` line.
        let off = PLUGIN
            .find(
                "reads:    WaveState
  emits",
            )
            .unwrap()
            + "reads:    ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let h = hover_at(&ds, &plugin_uri(), None, pos).expect("hover");
        let HoverContents::Markup(m) = h.contents else {
            panic!("markup")
        };
        assert!(m.value.contains("struct WaveState"), "{}", m.value);
        assert!(m.value.contains("resource struct"), "{}", m.value);
        assert!(m.value.contains("access site"), "{}", m.value);
    }

    #[test]
    fn hover_on_a_bevy_builtin_says_so() {
        let ds = plugin_state();
        let off =
            PLUGIN.find("reads:    WaveConfig, Time").unwrap() + "reads:    WaveConfig, ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let h = hover_at(&ds, &plugin_uri(), None, pos).expect("hover");
        let HoverContents::Markup(m) = h.contents else {
            panic!("markup")
        };
        assert!(m.value.contains("built-in Bevy resource"), "{}", m.value);
    }

    #[test]
    fn goto_from_an_access_list_reaches_the_declaration() {
        let ds = plugin_state();

        // `writes: WaveState` → the `struct WaveState` declaration.
        let off = PLUGIN
            .find(
                "writes:   WaveState
  spawns",
            )
            .unwrap()
            + "writes:   ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let resp = definition_at(&ds, &plugin_uri(), None, pos).expect("goto type");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        let decl = at(PLUGIN, "struct WaveState", 0);
        assert_eq!(loc.range.start.line, decl.line);

        // `after: tick_wave_timer` → the system entry that declares it.
        let off = PLUGIN.find("after:    tick_wave_timer").unwrap() + "after:    ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let resp = definition_at(&ds, &plugin_uri(), None, pos).expect("goto system");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        let decl = at(PLUGIN, "tick_wave_timer — advances", 0);
        assert_eq!(loc.range.start.line, decl.line);
    }

    #[test]
    fn goto_into_source_follows_a_system_path() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("src/waves");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("timer.rs"), "x").unwrap();

        let ds = plugin_state();
        let off = PLUGIN.find("src/waves/timer.rs").unwrap() + 2;
        let pos = to_lsp_pos(ds.line_index.position(off));
        let resp =
            definition_at(&ds, &plugin_uri(), Some(dir.path()), pos).expect("goto into source");
        let GotoDefinitionResponse::Scalar(loc) = resp else {
            panic!("scalar")
        };
        assert!(loc.uri.path().ends_with("src/waves/timer.rs"));
    }

    #[test]
    fn references_on_a_resource_find_every_access_site() {
        let ds = plugin_state();
        // From the declaration of WaveState.
        let pos = at(PLUGIN, "struct WaveState", "struct ".len() as u32);
        let refs = references_at(&ds, &plugin_uri(), pos, false);
        // tick_wave_timer writes, spawn_enemies writes, detect_cleared reads,
        // plus the behavior prose mentions.
        assert!(
            refs.len() >= 3,
            "expected the three access sites, got {refs:#?}"
        );

        // From a system name: its declaration plus the `after:` ref that orders
        // against it, plus the behavior bullet naming it.
        let pos = at(PLUGIN, "tick_wave_timer — advances", 0);
        let refs = references_at(&ds, &plugin_uri(), pos, true);
        assert!(refs.len() >= 3, "{refs:#?}");
    }

    #[test]
    fn completion_offers_access_keys_and_role_filtered_values() {
        let ds = plugin_state();

        // Key position: the closed access-key set.
        let off = PLUGIN.find("  schedule: FixedUpdate").unwrap() + 2;
        let pos = to_lsp_pos(ds.line_index.position(off));
        let labels: Vec<String> = completions_at(&ds, pos, None, &[])
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.contains(&"schedule:".to_string()), "{labels:?}");
        assert!(labels.contains(&"query:".to_string()), "{labels:?}");

        // `emits:` offers events only — never a component or a resource.
        let off = PLUGIN.find("emits:    WaveCleared").unwrap() + "emits:    ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let labels: Vec<String> = completions_at(&ds, pos, None, &[])
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.contains(&"WaveCleared".to_string()), "{labels:?}");
        assert!(labels.contains(&"AppExit".to_string()), "{labels:?}");
        assert!(!labels.contains(&"Wave".to_string()), "{labels:?}");
        assert!(!labels.contains(&"WaveState".to_string()), "{labels:?}");

        // `query:` offers components, declared and built-in.
        let off = PLUGIN.find("query:    Transform + Spawner").unwrap() + "query:    ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let labels: Vec<String> = completions_at(&ds, pos, None, &[])
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.contains(&"Spawner".to_string()), "{labels:?}");
        assert!(labels.contains(&"Transform".to_string()), "{labels:?}");
        assert!(!labels.contains(&"WaveCleared".to_string()), "{labels:?}");

        // `schedule:` offers the built-in labels.
        let off = PLUGIN.find("schedule: Update\n  query:    Wave").unwrap() + "schedule: ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let labels: Vec<String> = completions_at(&ds, pos, None, &[])
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(labels.contains(&"FixedUpdate".to_string()), "{labels:?}");
        assert!(labels.contains(&"PostUpdate".to_string()), "{labels:?}");

        // `after:` offers the file's own systems.
        let off = PLUGIN.find("after:    tick_wave_timer").unwrap() + "after:    ".len();
        let pos = to_lsp_pos(ds.line_index.position(off));
        let labels: Vec<String> = completions_at(&ds, pos, None, &[])
            .into_iter()
            .map(|i| i.label)
            .collect();
        assert!(
            labels.contains(&"tick_wave_timer".to_string()),
            "{labels:?}"
        );
        assert!(labels.contains(&"detect_cleared".to_string()), "{labels:?}");
    }

    #[test]
    fn section_header_completion_includes_the_bevy_sections() {
        let ds = plugin_state();
        let pos = at(PLUGIN, "## systems", 3);
        let labels: Vec<String> = completions_at(&ds, pos, None, &[])
            .into_iter()
            .map(|i| i.label)
            .collect();
        for want in [
            "## components",
            "## resources",
            "## events",
            "## systems",
            "## access",
        ] {
            assert!(
                labels.contains(&want.to_string()),
                "{want} missing: {labels:?}"
            );
        }
    }

    #[test]
    fn a_bevy_plugin_lints_clean_against_a_real_tree() {
        use std::fs;
        // The fixture references a sibling spec and two source files; with both
        // present, every filesystem-aware check must stay silent.
        let dir = tempfile::tempdir().unwrap();
        let waves = dir.path().join("src/waves");
        fs::create_dir_all(&waves).unwrap();
        for f in ["mod.rs", "timer.rs"] {
            fs::write(waves.join(f), "x").unwrap();
        }
        let spec = dir.path().join("enemy-waves.hmml");
        fs::write(&spec, PLUGIN).unwrap();
        fs::write(
            dir.path().join("spawn-enemies.hmml"),
            include_str!("../../mml-parser/tests/fixtures/examples/spawn-enemies.hmml"),
        )
        .unwrap();

        let (doc, diags) = analyze(PLUGIN);
        assert!(diags.is_empty(), "{diags:#?}");
        let fs_diags = lint::lint(&doc, &spec, Some(dir.path()));
        assert!(fs_diags.is_empty(), "{fs_diags:#?}");
    }
}
