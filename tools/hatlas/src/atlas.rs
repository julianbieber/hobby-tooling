//! The model the site is rendered from: crates, their module trees and the items in
//! them, each with the Bevy role it plays.

use crate::Error;
use crate::bevy::{BevyClassifier, Role};
use crate::rustdoc::{Crate, Id};

/// Every documented target of the workspace, in cargo metadata order.
pub struct Atlas {
    pub crates: Vec<CrateAtlas>,
}

/// One library or binary target; `name` is unique within the atlas.
pub struct CrateAtlas {
    pub name: String,
    pub package: String,
    pub root: Module,
}

/// A module with the items it defines. `path` starts with the crate's display name;
/// `public` is the module's own visibility, not its ancestors'.
pub struct Module {
    pub path: Vec<String>,
    pub public: bool,
    /// The workspace-relative source file the module lives in, as rustdoc spans it.
    pub file: Option<String>,
    pub docs: Option<String>,
    pub links: Vec<(String, Id)>,
    pub items: Vec<Item>,
    pub children: Vec<Module>,
}

/// A named item a module defines. `path` is the module path plus the name; `links`
/// is the item's intra-doc table (text as written, target id within the same crate).
pub struct Item {
    pub id: Id,
    pub path: Vec<String>,
    pub name: String,
    pub crate_name: String,
    pub kind: String,
    pub role: Role,
    pub public: bool,
    pub docs: Option<String>,
    pub location: Option<(String, u64)>,
    pub links: Vec<(String, Id)>,
    pub traits: Vec<&'static str>,
    pub system_param: bool,
}

impl Atlas {
    /// The item at this path in a crate of this package. A package with a library and
    /// binaries has one crate each, so every one is tried and the first match wins.
    /// `path` is module names then the item name, without the crate.
    pub fn find(&self, package: &str, path: &[String]) -> Option<(usize, Id)> {
        self.crates.iter().enumerate().filter(|(_, c)| c.package == package).find_map(|(ci, c)| {
            c.root.find(path).map(|i| (ci, i.id))
        })
    }

    /// The item behind an id from [`Atlas::find`].
    pub fn item(&self, (ci, id): (usize, Id)) -> Option<&Item> {
        self.crates[ci].root.find_id(id)
    }

    /// Mutable access to an item, for assigning a role found after classification.
    pub fn item_mut(&mut self, (ci, id): (usize, Id)) -> Option<&mut Item> {
        self.crates[ci].root.find_id_mut(id)
    }

    /// The shallowest module whose source file is `file`; the first crate wins a tie.
    pub fn module_of_file(&self, file: &str) -> Option<&Module> {
        self.crates.iter().filter_map(|c| c.root.module_of_file(file)).next()
    }

    /// How many items in the whole atlas play this role.
    pub fn count_role(&self, role: Role) -> usize {
        self.crates.iter().map(|c| c.root.count_role(role)).sum()
    }
}

impl CrateAtlas {
    /// Walks the module tree from the crate root, so only reachable items appear.
    /// `name` is the display name, which differs from the crate's when a library and
    /// a binary share one.
    pub fn build(krate: &Crate, name: String, package: String) -> Result<CrateAtlas, Error> {
        let classifier = BevyClassifier::new(krate);
        let root = krate.item(krate.root())?;
        let module = build_module(krate, &classifier, &name, root, vec![name.clone()], true)?;
        Ok(CrateAtlas { name, package, root: module })
    }
}

impl Module {
    fn find(&self, path: &[String]) -> Option<&Item> {
        let (name, dirs) = path.split_last()?;
        let mut module = self;
        for dir in dirs {
            module = module.children.iter().find(|c| c.path.last() == Some(dir))?;
        }
        module.items.iter().find(|i| &i.name == name)
    }

    pub(crate) fn module_of_file(&self, file: &str) -> Option<&Module> {
        if self.file.as_deref() == Some(file) {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.module_of_file(file))
    }

    fn find_id(&self, id: Id) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id).or_else(|| self.children.iter().find_map(|c| c.find_id(id)))
    }

    fn find_id_mut(&mut self, id: Id) -> Option<&mut Item> {
        if let Some(pos) = self.items.iter().position(|i| i.id == id) {
            return self.items.get_mut(pos);
        }
        self.children.iter_mut().find_map(|c| c.find_id_mut(id))
    }

    pub fn count_modules(&self) -> usize {
        1 + self.children.iter().map(Module::count_modules).sum::<usize>()
    }

    pub fn count_role(&self, role: Role) -> usize {
        self.items.iter().filter(|i| i.role == role).count()
            + self.children.iter().map(|c| c.count_role(role)).sum::<usize>()
    }
}

const SKIPPED_KINDS: &[&str] = &["use", "impl", "module", "extern_crate", "extern_type"];

fn build_module(
    krate: &Crate,
    classifier: &BevyClassifier,
    crate_name: &str,
    item: &serde_json::Value,
    path: Vec<String>,
    public: bool,
) -> Result<Module, Error> {
    let mut module = Module {
        path: path.clone(),
        public,
        file: Crate::span(item).map(|(file, _)| file),
        docs: Crate::docs(item).map(str::to_string),
        links: Crate::links(item),
        items: Vec::new(),
        children: Vec::new(),
    };
    for id in krate.module_items(item)? {
        let child = krate.item(id)?;
        let Some(name) = Crate::item_name(child) else {
            continue;
        };
        let kind = krate.kind(child)?;
        let child_public = krate.is_public(child)?;
        let mut child_path = path.clone();
        child_path.push(name.to_string());
        if kind == "module" {
            module.children.push(build_module(
                krate,
                classifier,
                crate_name,
                child,
                child_path,
                child_public,
            )?);
            continue;
        }
        if SKIPPED_KINDS.contains(&kind) {
            continue;
        }
        let role = if matches!(kind, "struct" | "enum" | "union") {
            classifier.classify(id)
        } else {
            Role::Other
        };
        module.items.push(Item {
            id,
            path: child_path,
            name: name.to_string(),
            crate_name: crate_name.to_string(),
            kind: kind.to_string(),
            role,
            public: child_public,
            docs: Crate::docs(child).map(str::to_string),
            location: Crate::span(child),
            links: Crate::links(child),
            traits: classifier.traits_of(id).to_vec(),
            system_param: classifier.is_system_param(id),
        });
    }
    Ok(module)
}
