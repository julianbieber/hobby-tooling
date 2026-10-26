//! Which items of the packages a workspace depends on the workspace actually uses, and
//! where: the join from a rust-analyzer SCIP symbol to the dependency's rustdoc item.

use crate::Error;
use crate::cargo::{DepDocs, Wanted};
use crate::rustdoc::{Crate, Id};
use crate::scip::{ScipIndex, Symbol, parse_symbol};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One place in the workspace that mentions a dependency item.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct UseSite {
    /// One-based.
    pub line: usize,
    /// A field, variant or foreign-trait method attached to the item rather than its own page.
    pub member: Option<String>,
}

/// An intra-doc link as written, and the crate name and path it points at when known.
pub type DocLink = (String, Option<(String, Vec<String>)>);

/// A dependency item with its rustdoc and every workspace file that uses it.
#[derive(Debug, Clone, Default)]
pub struct DependencyUse {
    pub kind: String,
    /// The module path inside the dependency crate, without the crate name or the item.
    pub module: Vec<String>,
    /// `Query`, or `World::get_resource` for a method.
    pub name: String,
    pub docs: Option<String>,
    pub links: Vec<DocLink>,
    /// Workspace-relative file to its sites, sorted.
    pub uses: BTreeMap<String, Vec<UseSite>>,
}

/// A documented dependency crate and what the workspace uses of it.
#[derive(Debug, Clone, Default)]
pub struct DepCrate {
    pub name: String,
    pub version: String,
    /// The crate's own name as rustdoc spells it, which differs from a hyphenated package name.
    pub crate_name: String,
    pub items: Vec<DependencyUse>,
    /// Descriptor text of the symbols that matched no rustdoc item.
    pub unmapped: Vec<String>,
}

/// A referenced package that has no page, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undocumented {
    pub name: String,
    pub version: String,
    pub reason: String,
}

/// Everything the dependency pages are rendered from.
#[derive(Debug, Clone, Default)]
pub struct Dependencies {
    pub crates: Vec<DepCrate>,
    pub undocumented: Vec<Undocumented>,
    /// Names of std, core and alloc as the workspace references them; they stay plain text.
    pub std: Vec<String>,
}

/// The packages SCIP shows the workspace referencing.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Referenced {
    /// The packages to document, sorted.
    pub wanted: Vec<Wanted>,
    /// A second version of a documented crate.
    pub undocumented: Vec<Undocumented>,
    pub std: Vec<String>,
}

/// Counts occurrences per package, leaving out workspace members and std, core and alloc
/// (whose SCIP version is a URL). Of several versions of one crate the most referenced
/// is wanted, a tie going to the higher version string.
pub fn referenced(scip: &ScipIndex, members: &[String]) -> Referenced {
    let mut counts: BTreeMap<Wanted, usize> = BTreeMap::new();
    let mut std = BTreeSet::new();
    for (_, _, symbol) in scip.occurrences() {
        let Some(sym) = parse_symbol(symbol) else { continue };
        if sym.version.contains("://") {
            std.insert(sym.package);
        } else if !members.contains(&sym.package) {
            *counts.entry((sym.package, sym.version)).or_default() += 1;
        }
    }
    let mut by_name: BTreeMap<String, Vec<(String, usize)>> = BTreeMap::new();
    for ((name, version), n) in counts {
        by_name.entry(name).or_default().push((version, n));
    }
    let mut out = Referenced { std: std.into_iter().collect(), ..Referenced::default() };
    for (name, mut versions) in by_name {
        versions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(&a.0)));
        let kept = versions[0].0.clone();
        for (version, _) in versions.into_iter().skip(1) {
            out.undocumented.push(Undocumented {
                name: name.clone(),
                version,
                reason: format!("another version ({kept}) is documented"),
            });
        }
        out.wanted.push((name, kept));
    }
    out
}

/// What a symbol resolved to inside one dependency crate.
#[derive(Debug, PartialEq, Eq)]
pub enum Resolved {
    /// An item with a page of its own, or a use of its owner with `member` set.
    Item { id: Id, module: Vec<String>, name: String, kind: String, member: Option<String> },
    /// A module or the crate root: referenced everywhere, informative nowhere.
    Skipped,
    /// No rustdoc item found.
    Unmapped,
}

/// Looks symbols up in one dependency's rustdoc.
pub struct Resolver<'a> {
    krate: &'a Crate,
    paths: HashMap<Vec<String>, (Id, String)>,
    types: Vec<(Id, Vec<String>, String, String)>,
    impls: HashMap<Id, Vec<Id>>,
}

const TYPE_KINDS: &[&str] = &["struct", "enum", "union", "trait", "type_alias"];

impl<'a> Resolver<'a> {
    /// Indexes the crate's paths and impls once; every `resolve` after it is a lookup.
    pub fn new(krate: &'a Crate) -> Resolver<'a> {
        let mut paths = HashMap::new();
        let mut types = Vec::new();
        for (id, full, kind) in krate.local_paths() {
            let rel: Vec<String> = full.into_iter().skip(1).collect();
            if TYPE_KINDS.contains(&kind.as_str())
                && let Some((name, module)) = rel.split_last()
            {
                types.push((id, module.to_vec(), name.clone(), kind.clone()));
            }
            paths.entry(rel).or_insert((id, kind));
        }
        Resolver { krate, paths, types, impls: krate.inherent_impl_items() }
    }

    /// Resolves by exact path, then through the `impl` the symbol sits in, then through
    /// its parent: a trait's method gets its own item, anything else becomes a use of the
    /// parent with `member` set.
    pub fn resolve(&self, symbol: &Symbol) -> Resolved {
        let path: Vec<String> = match symbol.path.split_first() {
            Some((first, rest)) if first == "lib" || first == "proc-macro" => rest.to_vec(),
            _ => symbol.path.clone(),
        };
        if path.is_empty() {
            return Resolved::Skipped;
        }
        if let Some((id, kind)) = self.paths.get(&path) {
            return self.exact(*id, kind, &path);
        }
        if let (Some(ty), Some(at)) = (&symbol.impl_type, path.iter().position(|s| s == "impl")) {
            return self.in_impl(ty, symbol.impl_trait.as_deref(), &path[..at], &path[path.len() - 1]);
        }
        let Some((last, parent)) = path.split_last() else { return Resolved::Unmapped };
        let Some((pid, pkind)) = self.paths.get(parent) else { return Resolved::Unmapped };
        if pkind == "trait"
            && let Some(found) = self.trait_member(*pid, parent, last)
        {
            return found;
        }
        match self.exact(*pid, pkind, parent) {
            Resolved::Item { id, module, name, kind, .. } => {
                Resolved::Item { id, module, name, kind, member: Some(last.clone()) }
            }
            other => other,
        }
    }

    /// A macro defined by this crate, by name: how a `Name:` descriptor resolves.
    pub fn resolve_macro(&self, name: &str) -> Resolved {
        for kind in ["proc_derive", "proc_attribute", "macro"] {
            let found = self.paths.iter().find(|(path, (_, k))| k == kind && path.last().is_some_and(|l| l == name));
            if let Some((path, (id, _))) = found {
                return self.exact(*id, kind, path);
            }
        }
        Resolved::Unmapped
    }

    fn exact(&self, id: Id, kind: &str, path: &[String]) -> Resolved {
        if kind == "module" {
            return Resolved::Skipped;
        }
        let (name, module) = path.split_last().expect("a resolved path is not empty");
        Resolved::Item { id, module: module.to_vec(), name: name.clone(), kind: kind.to_string(), member: None }
    }

    fn trait_member(&self, trait_id: Id, trait_path: &[String], member: &str) -> Option<Resolved> {
        let (trait_name, module) = trait_path.split_last()?;
        self.named_member(self.krate.trait_items(trait_id), member, module, trait_name)
    }

    fn named_member(&self, items: Vec<Id>, member: &str, module: &[String], owner: &str) -> Option<Resolved> {
        let id = items.into_iter().find(|id| self.krate.item(*id).ok().and_then(Crate::item_name) == Some(member))?;
        let kind = match self.krate.kind(self.krate.item(id).ok()?).ok()? {
            "function" => "method",
            other => other,
        };
        Some(Resolved::Item {
            id,
            module: module.to_vec(),
            name: format!("{owner}::{member}"),
            kind: kind.to_string(),
            member: None,
        })
    }

    fn in_impl(&self, ty: &str, impl_trait: Option<&str>, module: &[String], member: &str) -> Resolved {
        let named: Vec<_> = self.types.iter().filter(|(_, _, name, _)| name == ty).collect();
        let Some((id, ty_module, _, kind)) = named.iter().find(|(_, m, _, _)| m == module).or(if named.len() == 1 { named.first() } else { None }) else {
            return Resolved::Unmapped;
        };
        let items = match impl_trait {
            None => self.impls.get(id).cloned().unwrap_or_default(),
            Some(tr) => self
                .types
                .iter()
                .find(|(_, _, name, k)| name == tr && k == "trait")
                .map(|(tid, _, _, _)| self.krate.trait_items(*tid))
                .unwrap_or_default(),
        };
        if let Some(found) = self.named_member(items, member, ty_module, ty) {
            return found;
        }
        Resolved::Item { id: *id, module: ty_module.clone(), name: ty.to_string(), kind: kind.clone(), member: Some(member.to_string()) }
    }
}

/// Resolves every occurrence of each documented package against its rustdoc, one crate
/// at a time so only one crate's JSON is in memory.
///
/// A package that failed to document is listed in `undocumented` with cargo's reason.
/// An unsupported rustdoc `format_version` stays a hard error.
pub fn build(scip: &ScipIndex, docs: DepDocs, referenced: Referenced) -> Result<Dependencies, Error> {
    let mut undocumented = referenced.undocumented;
    for ((name, version), reason) in docs.failed {
        undocumented.push(Undocumented { name, version, reason });
    }
    let mut by_package: HashMap<Wanted, Vec<(&str, usize, &str, Symbol)>> = HashMap::new();
    for (file, line, text) in scip.occurrences() {
        let Some(sym) = parse_symbol(text) else { continue };
        let key = (sym.package.clone(), sym.version.clone());
        if docs.documented.iter().any(|(k, _)| *k == key) {
            by_package.entry(key).or_default().push((file, line, text, sym));
        }
    }
    let mut crates = Vec::new();
    for ((name, version), file) in docs.documented {
        if !file.exists() {
            let reason = format!("no rustdoc JSON at {}", file.display());
            undocumented.push(Undocumented { name, version, reason });
            continue;
        }
        let krate = Crate::load(&file)?;
        let resolver = Resolver::new(&krate);
        let mut items: BTreeMap<Id, DependencyUse> = BTreeMap::new();
        let mut unmapped = BTreeSet::new();
        let mut occs = by_package.remove(&(name.clone(), version.clone())).unwrap_or_default();
        occs.sort_by(|a, b| (a.0, a.1).cmp(&(b.0, b.1)));
        for (file, line, text, sym) in occs {
            let descriptors = text.splitn(5, ' ').nth(4).unwrap_or(text);
            let resolved = match descriptors.strip_suffix(':') {
                Some(macro_path) => resolver.resolve_macro(macro_path.rsplit('/').next().unwrap_or(macro_path)),
                None => resolver.resolve(&sym),
            };
            match resolved {
                Resolved::Item { id, module, name, kind, member } => {
                    let entry = items.entry(id).or_insert_with(|| use_of(&krate, id, module, name, kind));
                    let sites = entry.uses.entry(file.to_string()).or_default();
                    sites.push(UseSite { line: line + 1, member });
                }
                Resolved::Skipped => {}
                Resolved::Unmapped => {
                    unmapped.insert(descriptors.to_string());
                }
            }
        }
        let mut items: Vec<DependencyUse> = items.into_values().collect();
        for item in &mut items {
            for sites in item.uses.values_mut() {
                sites.sort();
                sites.dedup();
            }
        }
        items.sort_by(|a, b| (&a.module, &a.name, &a.kind).cmp(&(&b.module, &b.name, &b.kind)));
        crates.push(DepCrate { name, version, crate_name: krate.name().to_string(), items, unmapped: unmapped.into_iter().collect() });
    }
    crates.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    undocumented.sort_by(|a, b| (&a.name, &a.version).cmp(&(&b.name, &b.version)));
    Ok(Dependencies { crates, undocumented, std: referenced.std })
}

fn use_of(krate: &Crate, id: Id, module: Vec<String>, name: String, kind: String) -> DependencyUse {
    let item = krate.item(id).ok();
    DependencyUse {
        kind,
        module,
        name,
        docs: item.and_then(Crate::docs).map(str::to_string),
        links: item
            .map(Crate::links)
            .unwrap_or_default()
            .into_iter()
            .map(|(text, target)| (text, krate.path_of(target)))
            .collect(),
        uses: BTreeMap::new(),
    }
}
