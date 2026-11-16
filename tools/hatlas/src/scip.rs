//! Reading a rust-analyzer SCIP index: which symbol sits at a source position, and
//! what a symbol's package and path are.

use crate::Error;
use protobuf::Message;
use scip::types::Index;
use std::collections::HashMap;
use std::path::Path;

/// The parts of a SCIP symbol the atlas joins on.
#[derive(Debug, PartialEq, Eq)]
pub struct Symbol {
    pub package: String,
    pub version: String,
    pub path: Vec<String>,
    /// For a member of an `impl` block: the implemented type's name, without generics or path.
    pub impl_type: Option<String>,
    /// For a member of a trait `impl`: the trait's name, without generics or path.
    pub impl_trait: Option<String>,
}

struct Occurrence {
    start: usize,
    symbol: String,
}

/// Every symbol occurrence of an index, keyed by file and line.
pub struct ScipIndex {
    lines: HashMap<(String, usize), Vec<Occurrence>>,
}

impl ScipIndex {
    /// Fails with `Error::Io` when the file cannot be read and `Error::Scip` when it
    /// does not decode. Local
    /// symbols are dropped.
    pub fn load(path: &Path) -> Result<ScipIndex, Error> {
        let bytes = std::fs::read(path).map_err(|source| Error::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let index = Index::parse_from_bytes(&bytes).map_err(|source| Error::Scip {
            path: path.to_path_buf(),
            source,
        })?;
        Ok(ScipIndex::from_index(&index))
    }

    /// Builds the lookup from an already decoded index. Local symbols are dropped.
    pub fn from_index(index: &Index) -> ScipIndex {
        let mut lines: HashMap<(String, usize), Vec<Occurrence>> = HashMap::new();
        for doc in &index.documents {
            for occ in &doc.occurrences {
                if occ.symbol.is_empty() || occ.symbol.starts_with("local ") {
                    continue;
                }
                let &[line, start, _, ..] = occ.range.as_slice() else {
                    continue;
                };
                let (line, start) = (
                    usize::try_from(line).unwrap_or(0),
                    usize::try_from(start).unwrap_or(0),
                );
                lines
                    .entry((doc.relative_path.clone(), line))
                    .or_default()
                    .push(Occurrence {
                        start,
                        symbol: occ.symbol.clone(),
                    });
            }
        }
        ScipIndex { lines }
    }

    /// Every kept occurrence as `(file, zero-based line, symbol)`, in no particular order.
    pub fn occurrences(&self) -> impl Iterator<Item = (&str, usize, &str)> {
        self.lines.iter().flat_map(|((file, line), occs)| {
            occs.iter()
                .map(move |o| (file.as_str(), *line, o.symbol.as_str()))
        })
    }

    /// The symbol whose occurrence starts at exactly this zero-based line and column.
    pub fn symbol_at(&self, file: &str, line: usize, col: usize) -> Option<&str> {
        self.lines
            .get(&(file.to_string(), line))?
            .iter()
            .find(|o| o.start == col)
            .map(|o| o.symbol.as_str())
    }

    /// The only symbol on this line whose last descriptor is `name`; `None` when none
    /// or several are.
    pub fn symbol_named(&self, file: &str, line: usize, name: &str) -> Option<&str> {
        let mut found = self
            .lines
            .get(&(file.to_string(), line))?
            .iter()
            .filter(|o| {
                parse_symbol(&o.symbol).is_some_and(|s| s.path.last().is_some_and(|l| l == name))
            })
            .map(|o| o.symbol.as_str());
        let first = found.next()?;
        found.all(|s| s == first).then_some(first)
    }
}

/// Splits `<scheme> <manager> <package> <version> <descriptors>` and keeps the names of
/// namespace, type, term and method descriptors; `None` for a symbol of another shape.
pub fn parse_symbol(symbol: &str) -> Option<Symbol> {
    let mut parts = symbol.splitn(5, ' ');
    let _scheme = parts.next()?;
    let _manager = parts.next()?;
    let package = parts.next()?;
    let version = parts.next()?;
    let descriptors = parts.next()?;
    let mut path = Vec::new();
    let mut after_impl = false;
    let mut impl_parts: Vec<String> = Vec::new();
    let mut chars = descriptors.chars().peekable();
    while chars.peek().is_some() {
        let mut name = String::new();
        if chars.peek() == Some(&'`') {
            chars.next();
            for c in chars.by_ref() {
                if c == '`' {
                    break;
                }
                name.push(c);
            }
        } else if chars.peek() == Some(&'[') || chars.peek() == Some(&'(') {
            let close = if chars.next() == Some('[') { ']' } else { ')' };
            let mut inner = String::new();
            let mut quoted = false;
            for c in chars.by_ref() {
                if c == '`' {
                    quoted = !quoted;
                } else if c == close && !quoted {
                    break;
                } else {
                    inner.push(c);
                }
            }
            if after_impl && close == ']' {
                impl_parts.push(bare_type_name(&inner));
            }
            continue;
        } else {
            while let Some(&c) = chars.peek() {
                if matches!(c, '/' | '#' | '.' | '(' | '!' | ':' | '[') {
                    break;
                }
                name.push(c);
                chars.next();
            }
        }
        let suffix = chars.next()?;
        match suffix {
            '/' | '#' | '.' | '!' => {}
            '(' => {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
                if chars.peek() == Some(&'.') {
                    chars.next();
                }
            }
            ':' => continue,
            _ => return None,
        }
        after_impl = name == "impl" && suffix == '#';
        if !name.is_empty() {
            path.push(name);
        }
    }
    let mut parts = impl_parts.into_iter();
    Some(Symbol {
        package: package.to_string(),
        version: version.to_string(),
        path,
        impl_type: parts.next(),
        impl_trait: parts.next(),
    })
}

fn bare_type_name(text: &str) -> String {
    let head = text.split('<').next().unwrap_or(text).trim();
    head.rsplit("::").next().unwrap_or(head).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(sym: &str) -> Vec<String> {
        parse_symbol(sym).unwrap().path
    }

    // Pins that a free function inside nested modules yields its module path and name, which is what an atlas lookup needs.
    #[test]
    fn function_symbol_gives_module_path() {
        let s =
            parse_symbol("rust-analyzer cargo wusel 0.1.0 gameplay/world/refresh_edited_chunks().")
                .unwrap();
        assert_eq!((s.package.as_str(), s.version.as_str()), ("wusel", "0.1.0"));
        assert_eq!(s.path, ["gameplay", "world", "refresh_edited_chunks"]);
    }

    // Pins that an enum variant keeps both the enum and the variant, so `WorldSystems::Refresh` resolves to the set.
    #[test]
    fn variant_symbol_keeps_enum_and_variant() {
        assert_eq!(
            path("rust-analyzer cargo wusel 0.1.0 gameplay/world/WorldSystems#Refresh#"),
            ["gameplay", "world", "WorldSystems", "Refresh"]
        );
    }

    // Pins that a unit struct is its own type descriptor.
    #[test]
    fn type_symbol_ends_at_the_type() {
        assert_eq!(
            path("rust-analyzer cargo wusel 0.1.0 gameplay/city_panel/CityPanelSystems#"),
            ["gameplay", "city_panel", "CityPanelSystems"]
        );
    }

    // Pins that a symbol of another package keeps that package, so a dependency is told apart from workspace code.
    #[test]
    fn external_symbol_names_its_package() {
        let s =
            parse_symbol("rust-analyzer cargo bevy_state 0.19.0 condition/in_state().").unwrap();
        assert_eq!(s.package, "bevy_state");
        assert_eq!(s.path, ["condition", "in_state"]);
    }

    // Pins that the type and trait of an impl block's member are kept bare, because the dependency join looks them up by name.
    #[test]
    fn impl_members_carry_type_and_trait() {
        let s = parse_symbol("rust-analyzer cargo bevy_ecs 0.19.0 lib/system/commands/impl#[`Commands<'w, 's>`]spawn().").unwrap();
        assert_eq!(
            (s.impl_type.as_deref(), s.impl_trait),
            (Some("Commands"), None)
        );
        let s = parse_symbol(
            "rust-analyzer cargo bevy_ecs 0.19.0 lib/entity/impl#[Entity][`PartialEq<Self>`]eq().",
        )
        .unwrap();
        assert_eq!(
            (s.impl_type.as_deref(), s.impl_trait.as_deref()),
            (Some("Entity"), Some("PartialEq"))
        );
        assert_eq!(s.path, ["lib", "entity", "impl", "eq"]);
    }

    // Pins that a generic parameter and a backtick-quoted name do not corrupt the path.
    #[test]
    fn generics_and_quoted_names_are_handled() {
        assert_eq!(
            path("rust-analyzer cargo p 1.0.0 m/Wrapper#new().[T]"),
            ["m", "Wrapper", "new"]
        );
        assert_eq!(
            path("rust-analyzer cargo p 1.0.0 `my-mod`/f()."),
            ["my-mod", "f"]
        );
    }
}
