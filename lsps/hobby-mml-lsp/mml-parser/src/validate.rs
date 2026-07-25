//! Semantic validation over a parsed [`Document`].
//!
//! These are the reliable "hard" diagnostics from the structured `## data`
//! section (grammar §4.3) plus duplicate-definition checks. Behavior cross-refs
//! are intentionally *not* validated: grammar §5.2 makes them link-if-known,
//! silent-if-unknown, so a mistyped module name in prose is a missing link, not
//! an error.

use crate::ast::{Document, Kind, Step, TypeDef, TypeExpr};
use crate::diagnostic::{Diagnostic, code};
use crate::span::Spanned;
use std::collections::HashSet;

/// Valid HTTP methods for the `method:` header key (grammar §2).
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Type idents that need no definition.
const PRIMITIVES: &[&str] = &[
    "String", "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
];
const OPAQUE: &[&str] = &["Uuid", "Money", "DateTime"];
const CONTAINERS: &[&str] = &["Option", "Vec", "Map", "Queue", "Stream"];

/// Run all semantic checks and return the diagnostics (unsorted; callers merge
/// with [`Document::diagnostics`] and sort).
pub fn validate(doc: &Document) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    // Header `method` (grammar §2): a known HTTP verb, and endpoint-only.
    if let Some(method) = &doc.header.method {
        if !HTTP_METHODS.contains(&method.value.as_str()) {
            diags.push(Diagnostic::error(
                method.span,
                code::BAD_METHOD,
                format!(
                    "`{}` is not a valid HTTP method; expected one of {HTTP_METHODS:?}",
                    method.value
                ),
            ));
        }
        if doc.header.kind.as_ref().map(|k| k.value) != Some(Kind::Endpoint) {
            diags.push(Diagnostic::warning(
                method.span,
                code::BAD_METHOD,
                "`method` is only meaningful for `kind: endpoint`",
            ));
        }
    }

    // Defined type names (also drives duplicate-type detection).
    let mut defined: HashSet<&str> = HashSet::new();
    for def in &doc.data.types {
        let name = def.name();
        if !defined.insert(name.value.as_str()) {
            diags.push(Diagnostic::error(
                name.span,
                code::DUPLICATE_TYPE,
                format!("duplicate type definition `{}`", name.value),
            ));
        }
    }

    // Duplicate fields within each struct.
    for def in &doc.data.types {
        if let TypeDef::Struct { fields, .. } = def {
            let mut seen: HashSet<&str> = HashSet::new();
            for field in fields {
                if !seen.insert(field.name.value.as_str()) {
                    diags.push(Diagnostic::error(
                        field.name.span,
                        code::DUPLICATE_FIELD,
                        format!("duplicate field `{}`", field.name.value),
                    ));
                }
            }
        }
    }

    // Type imports (grammar §4.5): shape, PascalCase, and duplicate checks. The
    // imported names join the resolution set below so `in:`/`out:`/field uses of
    // them stop erroring. Whether the target file/type actually exists needs a
    // filesystem and is a warning emitted by fs-side consumers (LSP, CLI, index).
    let mut imported: HashSet<&str> = HashSet::new();
    for import in &doc.data.imports {
        if !crate::ast::is_valid_spec_ref(import.spec.value.as_str()) {
            diags.push(Diagnostic::error(
                import.spec.span,
                code::MALFORMED_IMPORT,
                format!(
                    "malformed import reference `{}`; expected `system/unit` or `unit`",
                    import.spec.value
                ),
            ));
        }
        for ty in &import.types {
            let name = ty.value.as_str();
            if !is_pascal_type(name) {
                diags.push(Diagnostic::error(
                    ty.span,
                    code::MALFORMED_IMPORT,
                    format!("imported type `{name}` must be a PascalCase type name"),
                ));
                continue;
            }
            if defined.contains(name) {
                diags.push(Diagnostic::error(
                    ty.span,
                    code::DUPLICATE_IMPORT,
                    format!("`{name}` is imported but also defined locally in `## data`"),
                ));
            } else if !imported.insert(name) {
                diags.push(Diagnostic::error(
                    ty.span,
                    code::DUPLICATE_IMPORT,
                    format!("duplicate import of type `{name}`"),
                ));
            }
        }
    }

    // Every type referenced in a field / in: / out: must resolve.
    let known: HashSet<&str> = PRIMITIVES
        .iter()
        .chain(OPAQUE)
        .chain(CONTAINERS)
        .copied()
        .chain(defined.iter().copied())
        .chain(imported.iter().copied())
        .collect();

    for def in &doc.data.types {
        if let TypeDef::Struct { fields, .. } = def {
            for field in fields {
                check_type(&field.ty, &known, &mut diags);
            }
        }
    }
    if let Some(ty) = &doc.data.input {
        check_type(ty, &known, &mut diags);
    }
    if let Some(ty) = &doc.data.output {
        check_type(ty, &known, &mut diags);
    }

    // Duplicate module names.
    let mut seen_modules: HashSet<&str> = HashSet::new();
    for module in &doc.modules {
        if !seen_modules.insert(module.name.value.as_str()) {
            diags.push(Diagnostic::error(
                module.name.span,
                code::DUPLICATE_MODULE,
                format!("duplicate module `{}`", module.name.value),
            ));
        }
    }

    // Cross-spec references (grammar §5.4): shape only. Whether the target
    // spec file exists needs a filesystem and is checked by the LSP, the CLI,
    // and the index tool.
    for spec in collect_spec_refs(doc) {
        check_spec_ref(spec, &mut diags);
    }

    diags
}

/// All `@ spec` refs in the document: modules, behavior steps (recursively),
/// and invariants.
pub fn collect_spec_refs(doc: &Document) -> Vec<&Spanned<String>> {
    let mut refs = Vec::new();
    refs.extend(doc.modules.iter().filter_map(|m| m.spec.as_ref()));
    fn walk<'a>(steps: &'a [Step], refs: &mut Vec<&'a Spanned<String>>) {
        for step in steps {
            refs.extend(step.spec.as_ref());
            walk(&step.children, refs);
        }
    }
    walk(&doc.behavior, &mut refs);
    refs.extend(doc.invariants.iter().filter_map(|i| i.spec.as_ref()));
    refs
}

/// All source-code references in the document: the header `source.paths`
/// entries and each module's `@ path`. Whether a path resolves to a real file
/// needs a filesystem and is checked by fs-aware consumers (`lint`, the LSP).
pub fn collect_source_paths(doc: &Document) -> Vec<&Spanned<String>> {
    let mut paths = Vec::new();
    if let Some(source) = &doc.header.source {
        paths.extend(source.paths.iter());
    }
    paths.extend(doc.modules.iter().filter_map(|m| m.path.as_ref()));
    paths
}

fn check_spec_ref(spec: &Spanned<String>, diags: &mut Vec<Diagnostic>) {
    let raw = spec.value.as_str();
    if !crate::ast::is_valid_spec_ref(raw) {
        diags.push(Diagnostic::error(
            spec.span,
            code::MALFORMED_SPEC_REF,
            format!(
                "malformed spec reference `{raw}`; expected `system/unit` or `unit` (segments like `[A-Za-z][A-Za-z0-9._-]*`)"
            ),
        ));
    }
}

/// Whether `s` is a PascalCase type name (`[A-Z][A-Za-z0-9_]*`) — the rule for
/// a struct/enum name, and therefore for an imported type name.
fn is_pascal_type(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check_type(ty: &TypeExpr, known: &HashSet<&str>, diags: &mut Vec<Diagnostic>) {
    let ident = ty.ident.value.as_str();
    if !ident.is_empty() && !known.contains(ident) {
        diags.push(Diagnostic::error(
            ty.ident.span,
            code::UNDEFINED_TYPE,
            format!("undefined type `{ident}`; not a primitive, opaque type, container, or a type defined in `## data`"),
        ));
    }
    for arg in &ty.args {
        check_type(arg, known, diags);
    }
}
