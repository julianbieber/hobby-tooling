//! Reading the facts about a Rust file that assertions are made about.
//!
//! Parsing is done with `tree-sitter-rust` rather than by matching text,
//! because Rust's import syntax has shapes text matching gets wrong: a grouped
//! `use crate::gameplay::{city, growth};` names two modules, and reading it as
//! one is how a hand-rolled survey came to believe seven modules depended on a
//! module called `gameplay`.

use std::collections::BTreeSet;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// What one Rust file says, reduced to what an assertion can ask about.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileFacts {
    pub lines: usize,
    /// Every path this file names, as its full segment list, with the leading
    /// `crate` / `super` / `self` or crate name kept.
    ///
    /// Whole paths rather than loose segments, because a leaf segment does not
    /// identify a module: `use crate::terrain::brush::Brush` and
    /// `use crate::brush::Brush` share the leaf `brush` and mean different
    /// modules. Resolving the prefix is the caller's job — it is the only party
    /// that knows which files each module owns.
    pub use_paths: Vec<Vec<String>>,
    /// The leading segment of every path that does not start at `crate`,
    /// `super` or `self` — that is, the external crates this file names.
    pub crates: BTreeSet<String>,
    /// Whether the file contains an `impl Plugin for ...` block.
    pub has_plugin_impl: bool,
    /// Whether the file declares a free function. Functions inside an `impl` do
    /// not count: a `mod.rs` asked to hold no functions is being asked to hold
    /// no systems, and its plugin's `build` is not one.
    pub declares_free_functions: bool,
    /// `Query` parameters carrying no `With` / `Without` / `Changed` / `Added`.
    ///
    /// A rough proxy for a system doing work proportional to the world rather
    /// than to what changed. Rough is tolerable here because the count is
    /// compared against a budget rather than adjudicated case by case.
    pub unfiltered_queries: usize,
    /// Functions that take a system parameter and also allocate.
    pub allocating_systems: usize,
    /// Every type this file takes mutable access to through a system parameter:
    /// the `T` of a `ResMut<T>`, and of a `&mut T` inside a `Query` or `Single`.
    ///
    /// Mutable access is what "writes" means here. Nothing tries to prove a
    /// write happens — asking for `&mut` is the declaration of intent an ECS
    /// schedules on, and it is the thing another module has to reason about.
    pub writes: BTreeSet<String>,
}

/// The system parameters a mutable borrow inside is a write to the borrowed
/// type. `ResMut` is handled separately: its type argument is the write.
const QUERY_PARAMS: &[&str] = &["Query", "Single", "Populated"];

/// Read a Rust file. A file that cannot be read yields empty facts rather than
/// an error, so one unreadable file does not abort a whole repo scan.
pub fn facts_in(path: &Path) -> FileFacts {
    match std::fs::read_to_string(path) {
        Ok(src) => facts_in_source(&src),
        Err(_) => FileFacts::default(),
    }
}

/// Read Rust source held in memory.
pub fn facts_in_source(src: &str) -> FileFacts {
    let mut facts = FileFacts {
        lines: src.lines().count(),
        ..FileFacts::default()
    };

    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return facts;
    }
    let Some(tree) = parser.parse(src, None) else {
        return facts;
    };

    let root = tree.root_node();
    collect(root, src, &mut facts);

    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "function_item" {
            facts.declares_free_functions = true;
        }
    }

    facts
}

fn collect(node: Node, src: &str, facts: &mut FileFacts) {
    match node.kind() {
        "use_declaration" => {
            if let Some(arg) = node.child_by_field_name("argument") {
                expand_use(arg, &[], src, facts);
            }
            return;
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            if let Some(path) = path_of(node, src) {
                record_path(path, facts);
            }
        }
        "generic_type" => {
            if let Some(name) = node.child_by_field_name("type").map(|t| text(t, src)) {
                if name == "ResMut"
                    && let Some(args) = node.child_by_field_name("type_arguments")
                    && let Some(first) = base_name(args, src)
                {
                    facts.writes.insert(first);
                } else if QUERY_PARAMS.contains(&name)
                    && let Some(args) = node.child_by_field_name("type_arguments")
                {
                    collect_mutable_borrows(args, src, facts);
                    if name == "Query" && !has_filter(args, src) {
                        facts.unfiltered_queries += 1;
                    }
                }
            }
        }
        "function_item" => {
            if let Some(params) = node.child_by_field_name("parameters")
                && takes_system_param(params, src)
                && body_allocates(node, src)
            {
                facts.allocating_systems += 1;
            }
        }
        "impl_item" => {
            if let Some(t) = node.child_by_field_name("trait")
                && text(t, src) == "Plugin"
            {
                facts.has_plugin_impl = true;
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, src, facts);
    }
}

fn record_path(path: Vec<String>, facts: &mut FileFacts) {
    if let Some(first) = path.first()
        && !matches!(first.as_str(), "crate" | "super" | "self")
    {
        facts.crates.insert(first.clone());
    }
    facts.use_paths.push(path);
}

/// Expand one `use` tree into the paths it names.
///
/// A grouped `use crate::a::{b, c::d}` names two paths, and a nested group
/// names more; flattening the tree into its identifiers loses which segments
/// belonged together, which is the whole point of resolving a path.
fn expand_use(node: Node, prefix: &[String], src: &str, facts: &mut FileFacts) {
    match node.kind() {
        "scoped_use_list" => {
            let mut base = prefix.to_vec();
            if let Some(path) = node.child_by_field_name("path")
                && let Some(segments) = path_of(path, src)
            {
                base.extend(segments);
            }
            if let Some(list) = node.child_by_field_name("list") {
                expand_use(list, &base, src, facts);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                expand_use(child, prefix, src, facts);
            }
        }
        "use_as_clause" => {
            if let Some(path) = node.child_by_field_name("path") {
                expand_use(path, prefix, src, facts);
            }
        }
        "use_wildcard" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                expand_use(child, prefix, src, facts);
            }
        }
        _ => {
            if let Some(segments) = path_of(node, src) {
                let mut full = prefix.to_vec();
                full.extend(segments);
                record_path(full, facts);
            }
        }
    }
}

/// A path node's segments, outermost first.
fn path_of(node: Node, src: &str) -> Option<Vec<String>> {
    match node.kind() {
        "identifier" | "type_identifier" | "crate" | "super" | "self" | "primitive_type" => {
            Some(vec![text(node, src).to_owned()])
        }
        "scoped_identifier" | "scoped_type_identifier" => {
            let mut segments = match node.child_by_field_name("path") {
                Some(path) => path_of(path, src)?,
                None => Vec::new(),
            };
            let name = node.child_by_field_name("name")?;
            segments.push(text(name, src).to_owned());
            Some(segments)
        }
        "generic_type" => path_of(node.child_by_field_name("type")?, src),
        _ => None,
    }
}

/// Whether a `Query`'s type arguments carry a filter.
fn has_filter(type_arguments: Node, src: &str) -> bool {
    const FILTERS: &[&str] = &["With", "Without", "Changed", "Added", "Or"];
    let text_of = &src[type_arguments.byte_range()];
    FILTERS.iter().any(|f| text_of.contains(&format!("{f}<")))
}

/// Whether a parameter list looks like a system's: it takes at least one of the
/// wrappers Bevy only injects into a scheduled function.
fn takes_system_param(params: Node, src: &str) -> bool {
    const SYSTEM_PARAMS: &[&str] = &[
        "Query",
        "Res",
        "ResMut",
        "Commands",
        "Single",
        "EventReader",
        "EventWriter",
        "Local",
    ];
    let text_of = &src[params.byte_range()];
    SYSTEM_PARAMS
        .iter()
        .any(|p| text_of.contains(&format!("{p}<")) || text_of.contains(&format!(": {p}")))
}

/// Whether a function body contains an allocating call.
///
/// Names, not types, because the type of an expression is not available here.
/// It over-counts a `Vec` built once at startup and under-counts an allocation
/// behind a helper — which a budget absorbs and a per-instance waiver would not.
fn body_allocates(function: Node, src: &str) -> bool {
    const ALLOCATORS: &[&str] = &[
        "vec!",
        "Vec::new",
        "Vec::with_capacity",
        ".to_vec()",
        ".collect::<Vec",
        "String::from",
        ".to_string()",
        ".clone()",
    ];
    let Some(body) = function.child_by_field_name("body") else {
        return false;
    };
    let text_of = &src[body.byte_range()];
    ALLOCATORS.iter().any(|a| text_of.contains(a))
}

/// The first type argument's own name, ignoring any generics it carries.
fn base_name(type_arguments: Node, src: &str) -> Option<String> {
    let mut cursor = type_arguments.walk();
    for child in type_arguments.children(&mut cursor) {
        match child.kind() {
            "type_identifier" => return Some(text(child, src).to_owned()),
            "generic_type" => {
                return child
                    .child_by_field_name("type")
                    .map(|t| text(t, src).to_owned());
            }
            _ => {}
        }
    }
    None
}

/// Record every `&mut T` under this node. Descends through tuples and nested
/// generics, so `Query<(Entity, &mut City, &mut CityGrowth)>` yields both, and
/// a shared `&T` beside them is passed over.
fn collect_mutable_borrows(node: Node, src: &str, facts: &mut FileFacts) {
    if node.kind() == "reference_type" {
        let mutable = {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|c| c.kind() == "mutable_specifier")
        };
        if mutable && let Some(inner) = node.child_by_field_name("type") {
            match inner.kind() {
                "type_identifier" => {
                    facts.writes.insert(text(inner, src).to_owned());
                }
                "generic_type" => {
                    if let Some(name) = inner.child_by_field_name("type") {
                        facts.writes.insert(text(name, src).to_owned());
                    }
                }
                _ => {}
            }
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_mutable_borrows(child, src, facts);
    }
}

fn text<'a>(node: Node, src: &'a str) -> &'a str {
    &src[node.byte_range()]
}

#[cfg(test)]
mod tests {
    use super::*;

    // The grouped-import form is the one text matching gets wrong, and getting
    // it wrong is what motivated parsing Rust properly at all.
    #[test]
    fn grouped_use_names_every_path_in_the_group() {
        let f = facts_in_source("use crate::gameplay::{city, growth};");
        assert!(
            f.use_paths
                .contains(&vec!["crate".into(), "gameplay".into(), "city".into()])
        );
        assert!(
            f.use_paths
                .contains(&vec!["crate".into(), "gameplay".into(), "growth".into()])
        );
    }

    // A path is kept whole, because its leaf does not identify a module:
    // `crate::terrain::brush` and `crate::brush` share the leaf `brush` and are
    // different modules. Recording loose segments blamed the wrong one.
    #[test]
    fn a_path_keeps_the_segments_that_disambiguate_it() {
        let f = facts_in_source("use crate::terrain::brush::{Brush, BrushMode};");
        assert!(
            f.use_paths.iter().all(|p| p[1] == "terrain"),
            "{:?}",
            f.use_paths
        );
    }

    // `imports no <crate>` is checked against the leading segment, which must
    // not be confused with a `crate::`-rooted path to a local module.
    #[test]
    fn external_crates_are_distinguished_from_local_paths() {
        let f = facts_in_source("use bevy::prelude::*;\nuse crate::gameplay::terrain;");
        assert!(f.crates.contains("bevy"));
        assert!(!f.crates.contains("crate"));
        assert!(!f.crates.contains("gameplay"));
    }

    // A qualified path in a body is an import too; only counting `use` lines
    // would miss `bevy::log::info!()` and call a module pure that is not.
    #[test]
    fn qualified_paths_in_bodies_count() {
        let f = facts_in_source("fn go() { let _ = bevy::math::Vec2::ZERO; }");
        assert!(f.crates.contains("bevy"));
    }

    // `every module is a plugin` needs the impl, and the trait name is the only
    // thing that distinguishes it from any other impl block.
    #[test]
    fn plugin_impl_is_found_by_trait_name() {
        assert!(facts_in_source("impl Plugin for P { }").has_plugin_impl);
        assert!(!facts_in_source("impl Display for P { }").has_plugin_impl);
    }

    // `ResMut<T>` is the plain form of taking mutable access to a resource.
    #[test]
    fn res_mut_is_a_write() {
        let f = facts_in_source("fn s(mut m: ResMut<CityMap>) {}");
        assert!(f.writes.contains("CityMap"));
        assert!(
            !facts_in_source("fn s(m: Res<CityMap>) {}")
                .writes
                .contains("CityMap")
        );
    }

    // A component is written through a mutable borrow in a query, and a shared
    // borrow beside it in the same tuple must not be swept up with it.
    #[test]
    fn a_mutable_borrow_in_a_query_is_a_write() {
        let f = facts_in_source("fn s(q: Query<(Entity, &CityGrowth, &mut City)>) {}");
        assert!(f.writes.contains("City"));
        assert!(!f.writes.contains("CityGrowth"));
        assert!(
            facts_in_source("fn s(t: Single<&mut Transform>) {}")
                .writes
                .contains("Transform")
        );
    }

    // A type named in a doc comment is not a write. `city_panel` explains its
    // ordering by saying the simulation holds `&mut City`, and text matching
    // reads that as city_panel writing the component it only reads.
    #[test]
    fn a_type_named_in_a_comment_is_not_a_write() {
        let f = facts_in_source("/// the simulation holds `&mut City` here\nfn s() {}");
        assert!(f.writes.is_empty(), "got {:?}", f.writes);
    }

    // A mod.rs asked to hold no functions still holds its plugin's `build`, so
    // counting functions inside an impl would make the assertion unusable.
    #[test]
    fn functions_inside_an_impl_are_not_free_functions() {
        let f = facts_in_source("impl Plugin for P { fn build(&self, _: &mut App) {} }");
        assert!(!f.declares_free_functions);
        assert!(facts_in_source("fn spawn_stuff() {}").declares_free_functions);
    }
}
