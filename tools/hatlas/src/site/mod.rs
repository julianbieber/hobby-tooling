//! Writing the site: one index, one page per module, and one per plugin, component
//! and resource, all as static HTML that links by relative path.

pub mod check;
pub mod markdown;
pub mod names;
pub mod swap;

use crate::Error;
use crate::atlas::{Atlas, Item, Module};
use crate::bevy::Role;
use crate::rustdoc::Id;
use markdown::{escape, render_docs, summary};
use names::{PageNames, relative};
use std::collections::HashMap;
use std::path::Path;

const CSS: &str = "body{font:15px/1.5 system-ui,sans-serif;max-width:56rem;margin:2rem auto;padding:0 1rem}\
code,pre{font-family:ui-monospace,monospace}pre{background:#f4f4f4;padding:.6rem;overflow:auto}\
.summary,.where,.vis,.crumbs{color:#666}.no-docs{color:#888;font-style:italic}ul{padding-left:1.2rem}\
@media(prefers-color-scheme:dark){body{background:#181818;color:#ddd}a{color:#8ab4f8}pre{background:#252525}.summary,.where,.vis,.crumbs{color:#999}}";

fn has_page(role: Role) -> bool {
    matches!(role, Role::Plugin | Role::Component | Role::Resource)
}

struct Site<'a> {
    atlas: &'a Atlas,
    module_pages: HashMap<Vec<String>, String>,
    item_pages: HashMap<(usize, Id), String>,
    anchors: HashMap<(usize, Id), (Vec<String>, String)>,
}

/// Writes the whole site into `root`, marker first, and returns how many pages it holds.
pub fn render(atlas: &Atlas, root: &Path) -> Result<usize, Error> {
    let _ = std::fs::remove_dir_all(root);
    write(root, swap::MARKER, "")?;
    let mut site = Site {
        atlas,
        module_pages: HashMap::new(),
        item_pages: HashMap::new(),
        anchors: HashMap::new(),
    };
    let mut names = PageNames::default();
    for (ci, krate) in atlas.crates.iter().enumerate() {
        site.register(ci, &krate.root, &mut names);
    }
    let mut pages = 0;
    write(root, "index.html", &site.index())?;
    pages += 1;
    for (ci, krate) in atlas.crates.iter().enumerate() {
        pages += site.write_module(root, ci, &krate.root)?;
    }
    Ok(pages)
}

fn write(root: &Path, rel: &str, content: &str) -> Result<(), Error> {
    let path = root.join(rel);
    let io = |source| Error::Io { path: path.clone(), source };
    std::fs::create_dir_all(path.parent().unwrap_or(root)).map_err(io)?;
    std::fs::write(&path, content).map_err(io)
}

fn page(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{}</title><style>{CSS}</style></head><body>\n{body}\n</body></html>\n",
        escape(title)
    )
}

impl<'a> Site<'a> {
    fn register(&mut self, ci: usize, module: &Module, names: &mut PageNames) {
        let page = names.module(&module.path);
        self.module_pages.insert(module.path.clone(), page);
        for item in &module.items {
            if has_page(item.role) {
                let page = names.item(&module.path, &item.kind, &item.name);
                self.item_pages.insert((ci, item.id), page);
            }
            self.anchors
                .insert((ci, item.id), (module.path.clone(), format!("{}.{}", item.kind, item.name)));
        }
        for child in &module.children {
            self.register(ci, child, names);
        }
    }

    fn href(&self, ci: usize, id: Id, from: &str) -> Option<String> {
        if let Some(page) = self.item_pages.get(&(ci, id)) {
            return Some(relative(from, page));
        }
        let (module, anchor) = self.anchors.get(&(ci, id))?;
        Some(format!("{}#{}", relative(from, &self.module_pages[module]), anchor))
    }

    fn docs_html(&self, ci: usize, from: &str, docs: Option<&str>, links: &[(String, Id)]) -> String {
        match docs {
            Some(d) if !d.trim().is_empty() => {
                let resolve = |key: &str| {
                    let id = links.iter().find(|(k, _)| k == key)?.1;
                    self.href(ci, id, from)
                };
                format!("<section class=\"docs\">{}</section>", render_docs(d, &resolve))
            }
            _ => "<p class=\"no-docs\">no rustdoc</p>".to_string(),
        }
    }

    fn crumbs(&self, from: &str, module_path: &[String], last: Option<&str>) -> String {
        let mut parts = Vec::new();
        for n in 1..=module_path.len() {
            let target = &self.module_pages[&module_path[..n].to_vec()];
            let label = &module_path[n - 1];
            if n == module_path.len() && last.is_none() {
                parts.push(escape(label));
            } else {
                parts.push(format!("<a href=\"{}\">{}</a>", escape(&relative(from, target)), escape(label)));
            }
        }
        if let Some(last) = last {
            parts.push(escape(last));
        }
        format!("<p class=\"crumbs\">{}</p>", parts.join(" :: "))
    }

    fn index(&self) -> String {
        let from = "index.html";
        let mut body = String::from("<h1>Atlas</h1>\n");
        for (ci, krate) in self.atlas.crates.iter().enumerate() {
            body.push_str(&format!("<h2>{}</h2>\n", escape(&krate.name)));
            let mut modules = Vec::new();
            collect_public_modules(&krate.root, &mut modules);
            body.push_str("<h3>Modules</h3>\n<ul>\n");
            for m in modules {
                let target = relative(from, &self.module_pages[&m.path]);
                body.push_str(&format!(
                    "<li><a href=\"{}\">{}</a> <span class=\"summary\">{}</span></li>\n",
                    escape(&target),
                    escape(&m.path.join("::")),
                    summary(m.docs.as_deref())
                ));
            }
            body.push_str("</ul>\n");
            for role in [Role::Plugin, Role::Component, Role::Resource] {
                let mut items = Vec::new();
                collect_public_items(&krate.root, role, &mut items);
                if items.is_empty() {
                    continue;
                }
                body.push_str(&format!("<h3>{}s</h3>\n<ul>\n", role.label()));
                for (module, item) in items {
                    let target = relative(from, &self.item_pages[&(ci, item.id)]);
                    body.push_str(&format!(
                        "<li><a href=\"{}\">{}</a> <span class=\"where\">{}</span> <span class=\"summary\">{}</span></li>\n",
                        escape(&target),
                        escape(&item.name),
                        escape(&module.path.join("::")),
                        summary(item.docs.as_deref())
                    ));
                }
                body.push_str("</ul>\n");
            }
        }
        page("Atlas", &body)
    }

    fn write_module(&self, root: &Path, ci: usize, module: &Module) -> Result<usize, Error> {
        let from = self.module_pages[&module.path].as_str();
        let mut body = self.crumbs(from, &module.path, None);
        body.push_str(&format!("\n<h1>Module {}</h1>\n", escape(&module.path.join("::"))));
        body.push_str(&self.docs_html(ci, from, module.docs.as_deref(), &module.links));
        if !module.children.is_empty() {
            body.push_str("\n<h2>Modules</h2>\n<ul>\n");
            for child in &module.children {
                let target = relative(from, &self.module_pages[&child.path]);
                body.push_str(&format!(
                    "<li><a href=\"{}\">{}</a>{} <span class=\"summary\">{}</span></li>\n",
                    escape(&target),
                    escape(child.path.last().map_or("", String::as_str)),
                    vis_tag(child.public),
                    summary(child.docs.as_deref())
                ));
            }
            body.push_str("</ul>");
        }
        if !module.items.is_empty() {
            body.push_str("\n<h2>Items</h2>\n<ul>\n");
            for item in &module.items {
                let anchor = format!("{}.{}", item.kind, item.name);
                let label = match self.item_pages.get(&(ci, item.id)) {
                    Some(p) => format!("<a href=\"{}\">{}</a>", escape(&relative(from, p)), escape(&item.name)),
                    None => escape(&item.name),
                };
                body.push_str(&format!(
                    "<li id=\"{}\">{} {}{} <span class=\"summary\">{}</span></li>\n",
                    escape(&anchor),
                    escape(role_or_kind(item)),
                    label,
                    vis_tag(item.public),
                    summary(item.docs.as_deref())
                ));
            }
            body.push_str("</ul>");
        }
        write(root, from, &page(&module.path.join("::"), &body))?;
        let mut pages = 1;
        for item in module.items.iter().filter(|i| has_page(i.role)) {
            self.write_item(root, ci, module, item)?;
            pages += 1;
        }
        for child in &module.children {
            pages += self.write_module(root, ci, child)?;
        }
        Ok(pages)
    }

    fn write_item(&self, root: &Path, ci: usize, module: &Module, item: &Item) -> Result<(), Error> {
        let from = self.item_pages[&(ci, item.id)].as_str();
        let mut body = self.crumbs(from, &module.path, Some(&item.name));
        body.push_str(&format!("\n<h1>{} {}</h1>\n", item.role.label(), escape(&item.name)));
        let location = item
            .location
            .as_ref()
            .map_or(String::new(), |(file, line)| format!(" · {}:{line}", escape(file)));
        body.push_str(&format!(
            "<p class=\"crumbs\">{}{} · {}</p>\n",
            escape(&item.crate_name),
            location,
            if item.public { "public" } else { "private" }
        ));
        if !item.traits.is_empty() {
            body.push_str(&format!("<p>Implements: {}</p>\n", escape(&item.traits.join(", "))));
        }
        body.push_str(&self.docs_html(ci, from, item.docs.as_deref(), &item.links));
        write(root, from, &page(&format!("{} {}", item.role.label(), item.name), &body))
    }
}

fn vis_tag(public: bool) -> &'static str {
    if public { "" } else { " <span class=\"vis\">private</span>" }
}

fn role_or_kind(item: &Item) -> &str {
    if item.role == Role::Other { &item.kind } else { item.role.label() }
}

fn collect_public_modules<'a>(module: &'a Module, out: &mut Vec<&'a Module>) {
    out.push(module);
    for child in module.children.iter().filter(|c| c.public) {
        collect_public_modules(child, out);
    }
}

fn collect_public_items<'a>(module: &'a Module, role: Role, out: &mut Vec<(&'a Module, &'a Item)>) {
    out.extend(module.items.iter().filter(|i| i.public && i.role == role).map(|i| (module, i)));
    for child in &module.children {
        collect_public_items(child, role, out);
    }
}
