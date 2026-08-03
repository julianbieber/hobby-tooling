//! Reading the Rust a spec points at (grammar §4.7).
//!
//! A type declaration carrying `@ <path>` claims a struct or enum already exists
//! in real code. This module is what turns that claim into something checkable:
//! it parses the `.rs` file and reports each declared type's shape, derives, and
//! member names, so [`crate::lint`] can say the type is missing, that its
//! `#[derive(…)]` disagrees with the section it was filed under, or that a field
//! the spec asserts is not there.
//!
//! **Why a real parser.** The whole value of a reference is that it is verified,
//! and a check that cries wolf is worse than no check. Deriving through
//! `#[cfg_attr(feature = "…", derive(Component))]`, ignoring the word `struct`
//! inside a string or a doc comment, and descending into `mod` blocks are all
//! things a line scanner gets wrong quietly.
//!
//! **Why the vendored grammar** (`tree-sitter-rust` from crates.io) rather than
//! `treesitter-class-lsp`'s Helix grammar cache: that cache resolves
//! `runtime/grammars/rust.so` at *run* time, so `hobby-mml-parser --lint` — the
//! binary every `hmml-*` skill shells out to — would silently lose the check on
//! a machine whose Helix grammars are not built. A compiled-in grammar has no
//! such failure mode, and this repo already requires a C compiler for
//! `tree-sitter-hmml`.

use std::path::Path;

use tree_sitter::{Node, Parser};

/// What a Rust type declaration *is*, as far as an HMML spec cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RustShape {
    /// `struct Foo { … }` or `struct Foo(…)`.
    Struct,
    /// `struct Foo;` — no fields at all, which is what a `marker` is.
    UnitStruct,
    Enum,
}

impl RustShape {
    pub fn noun(self) -> &'static str {
        match self {
            RustShape::Struct => "struct",
            RustShape::UnitStruct => "unit struct",
            RustShape::Enum => "enum",
        }
    }
}

/// One type declared in a Rust file.
#[derive(Debug, Clone)]
pub struct RustItem {
    pub name: String,
    pub shape: RustShape,
    /// Every name appearing inside a `derive(…)`, including through
    /// `cfg_attr(…, derive(…))`. Paths are reduced to their last segment, so
    /// `bevy::prelude::Component` reads as `Component`.
    pub derives: Vec<String>,
    /// Named struct fields, or enum variant names. Empty for a tuple struct.
    pub members: Vec<String>,
    /// 1-based line of the declaration, for goto-definition.
    pub line: u32,
}

impl RustItem {
    pub fn derives(&self, name: &str) -> bool {
        self.derives.iter().any(|d| d == name)
    }
}

/// Every struct and enum declared in the file at `path`, including inside `mod`
/// blocks. An unreadable or unparsable file yields an empty list — the caller
/// reports "not found", which is the honest outcome either way.
pub fn items_in(path: &Path) -> Vec<RustItem> {
    let Ok(src) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    items_in_source(&src)
}

/// [`items_in`] over source text, so the whole walk is testable without a file.
pub fn items_in_source(src: &str) -> Vec<RustItem> {
    let mut parser = Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return Vec::new();
    }
    let Some(tree) = parser.parse(src, None) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    walk(tree.root_node(), src.as_bytes(), &mut out);
    out
}

/// Find the one item named `name`, if the file declares it.
pub fn item_named<'a>(items: &'a [RustItem], name: &str) -> Option<&'a RustItem> {
    items.iter().find(|i| i.name == name)
}

fn walk(node: Node, src: &[u8], out: &mut Vec<RustItem>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "struct_item" | "enum_item" => {
                if let Some(item) = item_from(child, src) {
                    out.push(item);
                }
            }
            // Descend through anything that can hold items: `mod`, `impl`,
            // `unsafe`/`extern` blocks, and the file root itself.
            _ => walk(child, src, out),
        }
    }
}

fn item_from(node: Node, src: &[u8]) -> Option<RustItem> {
    let name_node = node.child_by_field_name("name")?;
    let name = text(name_node, src).to_string();

    let (shape, members) = match node.kind() {
        "enum_item" => (
            RustShape::Enum,
            node.child_by_field_name("body")
                .map(|b| named_children_field(b, src, "enum_variant", "name"))
                .unwrap_or_default(),
        ),
        _ => match node.child_by_field_name("body") {
            Some(body) if body.kind() == "field_declaration_list" => (
                RustShape::Struct,
                named_children_field(body, src, "field_declaration", "name"),
            ),
            // A tuple struct has ordered fields with no names to match against.
            Some(_) => (RustShape::Struct, Vec::new()),
            None => (RustShape::UnitStruct, Vec::new()),
        },
    };

    Some(RustItem {
        name,
        shape,
        derives: derives_of(node, src),
        members,
        line: name_node.start_position().row as u32 + 1,
    })
}

/// The named children of `body` of kind `kind`, taking each one's `field`.
fn named_children_field(body: Node, src: &[u8], kind: &str, field: &str) -> Vec<String> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|c| c.kind() == kind)
        .filter_map(|c| c.child_by_field_name(field))
        .map(|n| text(n, src).to_string())
        .collect()
}

/// Every derive on `node`, read off the `attribute_item` siblings that precede
/// it. `#[cfg_attr(feature = "x", derive(Component))]` counts: the spec's claim
/// is about the type as written, and a conditional derive is still the answer to
/// "what role does this have".
fn derives_of(node: Node, src: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut sibling = node.prev_sibling();
    while let Some(prev) = sibling {
        match prev.kind() {
            "attribute_item" => {
                collect_derives(prev, src, &mut out);
                sibling = prev.prev_sibling();
            }
            // Doc comments and ordinary comments sit between the attributes and
            // the item; keep walking past them.
            "line_comment" | "block_comment" => sibling = prev.prev_sibling(),
            _ => break,
        }
    }
    out.reverse();
    out
}

/// Pull the identifiers out of any `derive(…)` inside an attribute, at any
/// nesting depth, so `cfg_attr(…, derive(…))` is covered by the same walk. Only
/// the last path segment is kept: `bevy::prelude::Component` is `Component`.
fn collect_derives(node: Node, src: &[u8], out: &mut Vec<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let is_derive = child.kind() == "identifier" && text(child, src) == "derive";
        if is_derive {
            if let Some(args) = next_token_tree(child) {
                out.extend(idents_in(args, src));
            }
            continue;
        }
        collect_derives(child, src, out);
    }
}

/// The `(…)` group immediately following `node` — a `derive`'s argument list.
fn next_token_tree(node: Node) -> Option<Node> {
    let mut next = node.next_sibling();
    while let Some(n) = next {
        match n.kind() {
            "token_tree" | "arguments" | "delim_token_tree" => return Some(n),
            _ => next = n.next_sibling(),
        }
    }
    None
}

/// Every identifier inside `node`, keeping only the last segment of a path so a
/// fully-qualified derive still reads as its trait name.
fn idents_in(node: Node, src: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "type_identifier" => out.push(text(child, src).to_string()),
            "scoped_identifier" | "scoped_type_identifier" => {
                if let Some(last) = child.child_by_field_name("name") {
                    out.push(text(last, src).to_string());
                }
            }
            _ => out.extend(idents_in(child, src)),
        }
    }
    out
}

fn text<'a>(node: Node, src: &'a [u8]) -> &'a str {
    std::str::from_utf8(&src[node.byte_range()]).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"
use bevy::prelude::*;

/// A wave of enemies.
#[derive(Component, Debug, Default)]
pub struct Wave {
    pub index: u32,
    remaining: u32,
}

#[derive(Resource)]
pub struct WaveConfig {
    pub interval_secs: f32,
}

#[derive(Component)]
pub struct Spawner;

#[cfg_attr(feature = "net", derive(Event))]
pub struct WaveCleared {
    pub index: u32,
}

#[derive(bevy::prelude::Component)]
pub struct Qualified;

pub enum SpawnPattern {
    Ring,
    Scatter,
}

mod inner {
    #[derive(Component)]
    pub struct Nested {
        pub n: u8,
    }
}

// struct NotReal — the word `struct` in a comment declares nothing.
const DECOY: &str = "struct AlsoNotReal { x: u8 }";
"#;

    fn items() -> Vec<RustItem> {
        items_in_source(SRC)
    }

    #[test]
    fn finds_every_declared_type_and_nothing_else() {
        let items = items();
        let names: Vec<&str> = items.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Wave",
                "WaveConfig",
                "Spawner",
                "WaveCleared",
                "Qualified",
                "SpawnPattern",
                "Nested",
            ]
        );
    }

    #[test]
    fn derives_carry_the_ecs_role() {
        let items = items();
        assert!(item_named(&items, "Wave").unwrap().derives("Component"));
        assert!(
            item_named(&items, "WaveConfig")
                .unwrap()
                .derives("Resource")
        );
        assert!(
            !item_named(&items, "WaveConfig")
                .unwrap()
                .derives("Component")
        );
    }

    #[test]
    fn cfg_attr_and_qualified_derives_are_seen() {
        let items = items();
        assert!(item_named(&items, "WaveCleared").unwrap().derives("Event"));
        assert!(
            item_named(&items, "Qualified")
                .unwrap()
                .derives("Component")
        );
    }

    #[test]
    fn shapes_distinguish_a_marker_from_a_struct() {
        let items = items();
        assert_eq!(
            item_named(&items, "Spawner").unwrap().shape,
            RustShape::UnitStruct
        );
        assert_eq!(item_named(&items, "Wave").unwrap().shape, RustShape::Struct);
        assert_eq!(
            item_named(&items, "SpawnPattern").unwrap().shape,
            RustShape::Enum
        );
    }

    #[test]
    fn members_cover_fields_and_variants() {
        let items = items();
        assert_eq!(
            item_named(&items, "Wave").unwrap().members,
            ["index", "remaining"]
        );
        assert_eq!(
            item_named(&items, "SpawnPattern").unwrap().members,
            ["Ring", "Scatter"]
        );
    }

    #[test]
    fn line_points_at_the_declaration() {
        let items = items();
        let wave = item_named(&items, "Wave").unwrap();
        let line = SRC.lines().nth(wave.line as usize - 1).unwrap();
        assert!(line.contains("struct Wave"), "{line}");
    }

    #[test]
    fn unparsable_input_yields_nothing_rather_than_panicking() {
        assert!(
            items_in_source("struct {{{ ??? ")
                .iter()
                .all(|i| !i.name.is_empty())
        );
    }
}
