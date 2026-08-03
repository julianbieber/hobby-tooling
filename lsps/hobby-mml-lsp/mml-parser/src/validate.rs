//! Semantic validation over a parsed [`Document`].
//!
//! These are the reliable "hard" diagnostics from the structured `## data`
//! section (grammar §4.3) plus duplicate-definition checks. Behavior cross-refs
//! are intentionally *not* validated: grammar §5.2 makes them link-if-known,
//! silent-if-unknown, so a mistyped module name in prose is a missing link, not
//! an error.

use crate::ast::{Document, Kind, Role, Step, TypeDef, TypeExpr};
use crate::diagnostic::{Diagnostic, code};
use crate::span::Spanned;
use std::collections::{HashMap, HashSet};

/// Valid HTTP methods for the `method:` header key (grammar §2).
const HTTP_METHODS: &[&str] = &["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/// Type idents that need no definition.
pub const PRIMITIVES: &[&str] = &[
    "String", "bool", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64",
];
pub const OPAQUE: &[&str] = &[
    "Uuid", "Money", "DateTime", "Entity", "Duration", "Vec2", "Vec3", "Vec4", "Quat", "Color",
    "Timer", "Rect",
];
pub const CONTAINERS: &[&str] = &["Option", "Vec", "Map", "Queue", "Stream", "Handle"];

/// Bevy components a spec may name without declaring them — the engine already
/// defines them, and restating `Transform` in every plugin spec would be noise.
pub const BUILTIN_COMPONENTS: &[&str] = &[
    "Entity",
    "Transform",
    "GlobalTransform",
    "Visibility",
    "InheritedVisibility",
    "ViewVisibility",
    "Name",
    "Children",
    "ChildOf",
    "Parent",
    "Sprite",
    "Mesh2d",
    "Mesh3d",
    "MeshMaterial2d",
    "MeshMaterial3d",
    "Camera",
    "Camera2d",
    "Camera3d",
    "PointLight",
    "DirectionalLight",
    "SpotLight",
    "Text",
    "Text2d",
    "Node",
    "Button",
    "Window",
    "AudioPlayer",
    "AnimationPlayer",
    "Aabb",
];

/// Bevy resources (and system params close enough to be spelled like one) a
/// spec may name without declaring them.
pub const BUILTIN_RESOURCES: &[&str] = &[
    "Time",
    "AssetServer",
    "Assets",
    "Commands",
    "ButtonInput",
    "Gamepads",
    "Windows",
    "State",
    "NextState",
    "Gizmos",
    "ClearColor",
    "AmbientLight",
    "Msaa",
    "Diagnostics",
    "Fixed",
];

/// Bevy events a spec may name without declaring them.
pub const BUILTIN_EVENTS: &[&str] = &[
    "AppExit",
    "AssetEvent",
    "WindowResized",
    "WindowCloseRequested",
    "CursorMoved",
    "KeyboardInput",
    "MouseButtonInput",
    "MouseMotion",
    "MouseWheel",
    "TouchInput",
];

/// Bevy's built-in schedule labels. A `schedule:` outside this set is a warning,
/// not an error — custom schedules and `OnEnter(…)`/`OnExit(…)` state schedules
/// are both legitimate (the latter are recognised by shape, see
/// [`is_known_schedule`]).
pub const SCHEDULES: &[&str] = &[
    "PreStartup",
    "Startup",
    "PostStartup",
    "First",
    "PreUpdate",
    "StateTransition",
    "RunFixedMainLoop",
    "FixedFirst",
    "FixedPreUpdate",
    "FixedUpdate",
    "FixedPostUpdate",
    "FixedLast",
    "Update",
    "PostUpdate",
    "Last",
    "Main",
    "SpawnScene",
];

/// Whether `s` names a schedule the grammar recognises: a built-in label, or a
/// state-transition schedule `OnEnter(X)` / `OnExit(X)` / `OnTransition(…)`.
pub fn is_known_schedule(s: &str) -> bool {
    if SCHEDULES.contains(&s) {
        return true;
    }
    match s.split_once('(') {
        Some((head, rest)) => {
            matches!(head, "OnEnter" | "OnExit" | "OnTransition") && rest.ends_with(')')
        }
        None => false,
    }
}

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

    // Defined type names (also drives duplicate-type detection). One namespace
    // across `## data` and the three world sections, so declaring `Wave` as
    // both a component and a resource is caught here rather than producing an
    // ambiguous role later.
    let mut defined: HashSet<&str> = HashSet::new();
    for def in doc.all_type_defs() {
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
    for def in doc.all_type_defs() {
        if let TypeDef::Struct {
            fields: Some(fields),
            ..
        } = def
        {
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
    for import in doc.all_imports() {
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
                    format!("`{name}` is imported but also defined locally in this spec"),
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

    // External-crate imports (grammar §4.8). Same scope and duplicate rules as
    // a spec import — the section still assigns the role — but the target is a
    // crate path with nothing to resolve it against, so only its shape is
    // checkable, here and nowhere else.
    for import in doc.all_extern_imports() {
        if !crate::ast::is_valid_crate_path(import.path.value.as_str()) {
            diags.push(Diagnostic::error(
                import.path.span,
                code::MALFORMED_EXTERN_IMPORT,
                format!(
                    "`{}` is not a Rust path; expected `crate` or `crate::module::path`",
                    import.path.value
                ),
            ));
        }
        for ty in &import.types {
            let name = ty.value.as_str();
            if !is_pascal_type(name) {
                diags.push(Diagnostic::error(
                    ty.span,
                    code::MALFORMED_EXTERN_IMPORT,
                    format!("imported type `{name}` must be a PascalCase type name"),
                ));
                continue;
            }
            if defined.contains(name) {
                diags.push(Diagnostic::error(
                    ty.span,
                    code::DUPLICATE_IMPORT,
                    format!("`{name}` is imported but also defined locally in this spec"),
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

    for def in doc.all_type_defs() {
        if let TypeDef::Struct {
            fields: Some(fields),
            ..
        } = def
        {
            for field in fields.iter().filter_map(|f| f.ty.as_ref()) {
                check_type(field, &known, &mut diags);
            }
        }
    }
    if let Some(ty) = &doc.data.input {
        check_type(ty, &known, &mut diags);
    }
    if let Some(ty) = &doc.data.output {
        check_type(ty, &known, &mut diags);
    }

    // Bevy dialect: systems and their access blocks.
    validate_bevy(doc, &mut diags);

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

/// Semantic checks for the Bevy dialect: duplicate system names, and every
/// access block's schedule, ordering refs, and world-item lists.
///
/// This is what replaces the `in:`/`out:` type check for `kind: plugin` and
/// `kind: system` — because data flows through the World, a system's contract
/// is the set of components, resources, and events it touches, and each of
/// those names must resolve to something declared, imported, or built into
/// Bevy, in a role that fits the key it appears under.
fn validate_bevy(doc: &Document, diags: &mut Vec<Diagnostic>) {
    // Duplicate system names.
    let mut seen: HashSet<&str> = HashSet::new();
    for system in &doc.systems {
        if !seen.insert(system.name.value.as_str()) {
            diags.push(Diagnostic::error(
                system.name.span,
                code::DUPLICATE_SYSTEM,
                format!("duplicate system `{}`", system.name.value),
            ));
        }
    }

    let roles = doc.roles();
    // Names `## data` brought into scope. They resolve as types but carry no ECS
    // role (grammar §4.6), so naming one in an access list is a role mismatch
    // rather than an unknown name — worth saying differently, because the fix is
    // to move the declaration, not to write it.
    let plain: HashSet<&str> = doc.data_names().map(|n| n.value.as_str()).collect();
    // Ordering targets declared in this file: the systems plus any set named in
    // a `set:` line.
    let mut orderable: HashSet<&str> = doc.systems.iter().map(|s| s.name.value.as_str()).collect();
    for (_, block) in doc.access_blocks() {
        orderable.extend(block.sets.iter().map(|s| s.value.as_str()));
    }
    // Only a plugin declares the systems it orders against. A `kind: system`
    // file legitimately orders against systems specced elsewhere, so checking
    // its refs would warn on every correct file.
    let check_ordering = doc.header.kind.as_ref().map(|k| k.value) == Some(Kind::Plugin);

    for (name, block) in doc.access_blocks() {
        let label = match name {
            Some(n) => format!("system `{}`", n.value),
            None => "`## access`".to_string(),
        };

        match &block.schedule {
            None => diags.push(Diagnostic::error(
                block.anchor,
                code::MISSING_SCHEDULE,
                format!("{label} has no `schedule:`; every system runs in some schedule"),
            )),
            Some(schedule) if !is_known_schedule(&schedule.value) => {
                diags.push(Diagnostic::warning(
                    schedule.span,
                    code::UNKNOWN_SCHEDULE,
                    format!(
                        "`{}` is not a built-in Bevy schedule; expected one of {SCHEDULES:?}, \
                         `OnEnter(..)`/`OnExit(..)`, or a custom schedule you register",
                        schedule.value
                    ),
                ));
            }
            Some(_) => {}
        }

        if check_ordering {
            for r in block.ordering_refs() {
                if !orderable.contains(r.value.as_str()) {
                    diags.push(Diagnostic::warning(
                        r.span,
                        code::UNRESOLVED_SYSTEM_REF,
                        format!(
                            "`{}` names no system or set in this spec; ordering against another \
                             plugin's system is fine, but check the spelling",
                            r.value
                        ),
                    ));
                }
            }
        }

        for (key, accepted, items) in block.world_lists() {
            for item in items {
                check_access_item(key, accepted, item, &roles, &plain, diags);
            }
        }
    }
}

/// Resolve one access-list name and check its role against the key it sits
/// under. Declarations win over Bevy built-ins, so a spec may redefine `Timer`.
/// `plain` holds the `## data` names, which resolve but have no role.
fn check_access_item(
    key: &str,
    accepted: &[Role],
    item: &Spanned<String>,
    roles: &HashMap<&str, Role>,
    plain: &HashSet<&str>,
    diags: &mut Vec<Diagnostic>,
) {
    let name = item.value.as_str();
    let role = roles.get(name).copied().or_else(|| builtin_role(name));
    let Some(role) = role else {
        // The sections that would give the name a role fitting this key.
        let want = accepted
            .iter()
            .map(|r| format!("`## {}`", r.section()))
            .collect::<Vec<_>>()
            .join(" or ");
        let (code, message) = if plain.contains(name) {
            (
                code::ACCESS_ROLE_MISMATCH,
                format!(
                    "`{name}` is declared in `## data`, which gives it no ECS role; move it to \
                     {want} if a system really accesses it"
                ),
            )
        } else {
            (
                code::UNDEFINED_ACCESS_ITEM,
                format!(
                    "`{name}` is not declared, imported, or a Bevy built-in; declare it in {want}"
                ),
            )
        };
        diags.push(Diagnostic::error(item.span, code, message));
        return;
    };
    if !accepted.contains(&role) {
        let want = accepted
            .iter()
            .map(|r| r.noun())
            .collect::<Vec<_>>()
            .join(" or a ");
        diags.push(Diagnostic::error(
            item.span,
            code::ACCESS_ROLE_MISMATCH,
            format!("`{name}` is a {}, but `{key}:` takes a {want}", role.noun()),
        ));
    }
}

/// The role of a Bevy built-in name, if it is one.
pub fn builtin_role(name: &str) -> Option<Role> {
    if BUILTIN_COMPONENTS.contains(&name) {
        Some(Role::Component)
    } else if BUILTIN_RESOURCES.contains(&name) {
        Some(Role::Resource)
    } else if BUILTIN_EVENTS.contains(&name) {
        Some(Role::Event)
    } else {
        None
    }
}

/// All `@ spec` refs in the document: modules, system entries, behavior steps
/// (recursively), and invariants.
pub fn collect_spec_refs(doc: &Document) -> Vec<&Spanned<String>> {
    let mut refs = Vec::new();
    refs.extend(doc.modules.iter().filter_map(|m| m.spec.as_ref()));
    refs.extend(doc.systems.iter().filter_map(|s| s.spec.as_ref()));
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
/// entries, each module's `@ path`, each system entry's `@ path`, and each
/// type declaration's `@ path` (grammar §4.7). Whether a path resolves to a
/// real file needs a filesystem and is checked by fs-aware consumers (`lint`,
/// the LSP).
pub fn collect_source_paths(doc: &Document) -> Vec<&Spanned<String>> {
    let mut paths = Vec::new();
    if let Some(source) = &doc.header.source {
        paths.extend(source.paths.iter());
    }
    paths.extend(doc.modules.iter().filter_map(|m| m.path.as_ref()));
    paths.extend(doc.systems.iter().filter_map(|s| s.path.as_ref()));
    paths.extend(doc.all_type_defs().filter_map(TypeDef::path));
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
            format!("undefined type `{ident}`; not a primitive, opaque type, container, or a type declared or imported in this spec"),
        ));
    }
    for arg in &ty.args {
        check_type(arg, known, diags);
    }
}
