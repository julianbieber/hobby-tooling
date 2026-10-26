//! Writing the site: an index with an overview graph, one page per module, and one per plugin,
//! component, resource, system and observer, all as static HTML that links by relative path.

mod deps;
mod graph;
pub mod check;
pub mod markdown;
pub mod names;
pub mod swap;

use crate::Error;
use crate::atlas::{Atlas, Item, Module};
use crate::bevy::Role;
use crate::deps::Dependencies;
use crate::events::{Endpoint, EventFlow, EventKey, Flows, Party};
use crate::rustdoc::Id;
use crate::systems::{Param, Registered, Registrant, Registration, Registry};
use markdown::{escape, render_docs, summary};
use names::{PageNames, relative};
use std::collections::HashMap;
use std::path::Path;

const CSS: &str = "body{font:15px/1.5 system-ui,sans-serif;max-width:56rem;margin:2rem auto;padding:0 1rem}\
code,pre{font-family:ui-monospace,monospace}pre{background:#f4f4f4;padding:.6rem;overflow:auto}\
.summary,.where,.vis,.crumbs{color:#666}.no-docs{color:#888;font-style:italic}ul{padding-left:1.2rem}#graph{height:70vh;border:1px solid #ccc;margin:1rem 0}\
@media(prefers-color-scheme:dark){body{background:#181818;color:#ddd}a{color:#8ab4f8}pre{background:#252525}#graph{border-color:#444}.summary,.where,.vis,.crumbs{color:#999}}";

fn has_page(role: Role) -> bool {
    matches!(role, Role::Plugin | Role::Component | Role::Resource | Role::Event | Role::System | Role::Observer)
}

struct Site<'a> {
    atlas: &'a Atlas,
    registry: &'a Registry,
    flows: &'a Flows,
    deps: &'a Dependencies,
    dep_crate_pages: Vec<String>,
    dep_item_pages: HashMap<(usize, usize), String>,
    dep_lookup: HashMap<(String, Vec<String>), (usize, usize)>,
    event_pages: HashMap<EventKey, String>,
    module_pages: HashMap<Vec<String>, String>,
    item_pages: HashMap<(usize, Id), String>,
    anchors: HashMap<(usize, Id), (Vec<String>, String)>,
}

/// Writes the whole site into `root`, marker first, and returns how many pages it holds.
pub fn render(atlas: &Atlas, registry: &Registry, flows: &Flows, deps: &Dependencies, root: &Path) -> Result<usize, Error> {
    let _ = std::fs::remove_dir_all(root);
    write(root, swap::MARKER, "")?;
    let mut site = Site {
        atlas,
        registry,
        flows,
        deps,
        dep_crate_pages: Vec::new(),
        dep_item_pages: HashMap::new(),
        dep_lookup: HashMap::new(),
        event_pages: HashMap::new(),
        module_pages: HashMap::new(),
        item_pages: HashMap::new(),
        anchors: HashMap::new(),
    };
    let mut names = PageNames::default();
    for (ci, krate) in atlas.crates.iter().enumerate() {
        site.register(ci, &krate.root, &mut names);
    }
    for flow in &flows.flows {
        if flow.item.is_none() {
            let page = names.item(&["events".to_string(), flow.key.crate_name.clone()], "event", &flow.key.name);
            site.event_pages.insert(flow.key.clone(), page);
        }
    }
    site.register_deps(&mut names);
    site.write_graph(root)?;
    let mut pages = 0;
    write(root, "index.html", &site.index())?;
    pages += 1;
    for (ci, krate) in atlas.crates.iter().enumerate() {
        pages += site.write_module(root, ci, &krate.root)?;
    }
    for flow in flows.flows.iter().filter(|f| f.item.is_none()) {
        site.write_event(root, flow)?;
        pages += 1;
    }
    pages += site.write_deps(root)?;
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
        body.push_str(&self.graph_html());
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
            for role in [Role::Plugin, Role::Component, Role::Resource, Role::Event, Role::System, Role::Observer] {
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
        let external: Vec<_> = self.flows.flows.iter().filter(|f| f.item.is_none()).collect();
        if !external.is_empty() {
            body.push_str("<h2>Events from dependencies</h2>\n<ul>\n");
            for flow in external {
                body.push_str(&format!(
                    "<li><a href=\"{}\">{}</a> <span class=\"where\">{}</span></li>\n",
                    escape(&relative(from, &self.event_pages[&flow.key])),
                    escape(&flow.key.name),
                    escape(flow.defined_in())
                ));
            }
            body.push_str("</ul>\n");
        }
        if !self.registry.unresolved.is_empty() {
            body.push_str("<h2>Unresolved registrations</h2>\n<ul>\n");
            for note in &self.registry.unresolved {
                body.push_str(&format!("<li><code>{}</code></li>\n", escape(note)));
            }
            body.push_str("</ul>\n");
        }
        body.push_str(&self.deps_index_html(from));
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
        let anonymous: Vec<_> = self
            .registry
            .anonymous
            .iter()
            .filter(|a| a.crate_index == ci && a.module == module.path)
            .collect();
        if !anonymous.is_empty() {
            body.push_str("\n<h2>Anonymous systems</h2>\n<ul>\n");
            for a in anonymous {
                body.push_str(&format!(
                    "<li id=\"{}\">closure · {}</li>\n",
                    escape(&a.anchor),
                    self.registration_html(from, &a.registration)
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
        if let Some(flow) = self.flows.flows.iter().find(|f| f.item == Some((ci, item.id))) {
            body.push_str(&self.flow_html(from, flow));
        }
        if let Some(view) = self.registry.systems.get(&(ci, item.id)) {
            body.push_str(&self.system_html(from, view));
        }
        for flow in self.flows.listened_by((ci, item.id)) {
            body.push_str(&format!("\n<h2>Listens to {}</h2>", self.event_link(from, flow)));
            body.push_str(&self.producers_html(from, flow));
        }
        for flow in self.flows.produced_by((ci, item.id)) {
            body.push_str(&format!("\n<h2>Produces {}</h2>", self.event_link(from, flow)));
        }
        if let Some(registered) = self.registry.registers.get(&(ci, item.id)) {
            body.push_str(&self.registers_html(from, registered));
        }
        if let Some(by) = self.registry.resource_by.get(&(ci, item.id)) {
            body.push_str("\n<h2>Registered by</h2>\n<ul>\n");
            for r in by {
                body.push_str(&format!("<li>{}</li>\n", self.registrant_html(from, r)));
            }
            body.push_str("</ul>");
        }
        write(root, from, &page(&format!("{} {}", item.role.label(), item.name), &body))
    }

    fn event_page(&self, flow: &EventFlow) -> &str {
        match flow.item {
            Some(r) => &self.item_pages[&r],
            None => &self.event_pages[&flow.key],
        }
    }

    fn event_link(&self, from: &str, flow: &EventFlow) -> String {
        format!("<a href=\"{}\">{}</a>", escape(&relative(from, self.event_page(flow))), escape(&flow.key.name))
    }

    fn endpoint_html(&self, from: &str, e: &Endpoint) -> String {
        let mut out = match &e.party {
            Party::Item(r) => {
                let name = self.atlas.item(*r).map_or("", |i| i.name.as_str());
                match self.href(r.0, r.1, from) {
                    Some(h) => format!("<a href=\"{}\">{}</a>", escape(&h), escape(name)),
                    None => format!("<code>{}</code>", escape(name)),
                }
            }
            Party::Anonymous { module, anchor, .. } => format!(
                "<a href=\"{}#{}\">closure</a>",
                escape(&relative(from, &self.module_pages[module])),
                escape(anchor)
            ),
            Party::Text(t) => format!("<code>{}</code>", escape(t)),
        };
        out.push_str(&format!(" · <code>{}</code>", escape(&e.via)));
        if let Some((file, line)) = &e.site {
            out.push_str(&format!(" · <span class=\"where\">{}:{line}</span>", escape(file)));
        }
        out
    }

    fn endpoints_html(&self, from: &str, heading: &str, none: &str, endpoints: &[Endpoint]) -> String {
        let mut out = format!("\n<h2>{heading}</h2>\n");
        if endpoints.is_empty() {
            out.push_str(&format!("<p class=\"no-docs\">{none}</p>"));
            return out;
        }
        out.push_str("<ul>\n");
        for e in endpoints {
            out.push_str(&format!("<li>{}</li>\n", self.endpoint_html(from, e)));
        }
        out.push_str("</ul>");
        out
    }

    fn producers_html(&self, from: &str, flow: &EventFlow) -> String {
        self.endpoints_html(from, "Produced by", "no producer found in the workspace", &flow.producers)
    }

    fn flow_html(&self, from: &str, flow: &EventFlow) -> String {
        let mut out = format!("\n<p>Defined in <code>{}</code></p>", escape(flow.defined_in()));
        out.push_str(&self.producers_html(from, flow));
        out.push_str(&self.endpoints_html(from, "Listened to by", "no listener found in the workspace", &flow.listeners));
        out
    }

    fn write_event(&self, root: &Path, flow: &EventFlow) -> Result<(), Error> {
        let from = self.event_page(flow);
        let mut body = format!(
            "<p class=\"crumbs\"><a href=\"{}\">Atlas</a> :: events :: {}</p>\n<h1>Event {}</h1>",
            escape(&relative(from, "index.html")),
            escape(flow.defined_in()),
            escape(&flow.key.name)
        );
        body.push_str(&self.flow_html(from, flow));
        write(root, from, &page(&format!("Event {}", flow.key.name), &body))
    }

    fn registrant_html(&self, from: &str, by: &Registrant) -> String {
        match by {
            Registrant::Item(r) => {
                let name = self.atlas.item(*r).map_or("", |i| i.name.as_str());
                match self.href(r.0, r.1, from) {
                    Some(h) => format!("<a href=\"{}\">{}</a>", escape(&h), escape(name)),
                    None => format!("<code>{}</code>", escape(name)),
                }
            }
            Registrant::Text(t) => {
                format!("<code>{}</code>", escape(t))
            }
        }
    }

    fn registration_html(&self, from: &str, reg: &Registration) -> String {
        let mut out = String::new();
        if !reg.schedule.is_empty() {
            out.push_str(&format!("schedule <code>{}</code>", escape(&reg.schedule)));
        }
        if let Some(by) = &reg.registered_by {
            out.push_str(&format!(" · registered by {}", self.registrant_html(from, by)));
        }
        out.push_str(&format!(" · <span class=\"where\">{}:{}</span>", escape(&reg.site.0), reg.site.1));
        let mut lines = Vec::new();
        for c in &reg.conditions {
            lines.push(format!("run if <code>{}</code>", escape(c)));
        }
        for (c, via) in &reg.inherited {
            lines.push(format!("run if <code>{}</code> via set <code>{}</code>", escape(c), escape(via)));
        }
        for s in &reg.sets {
            lines.push(format!("in set <code>{}</code>", escape(s)));
        }
        for o in &reg.ordering {
            lines.push(format!("ordered <code>{}</code>", escape(o)));
        }
        if !lines.is_empty() {
            out.push_str("<ul>");
            for l in lines {
                out.push_str(&format!("<li>{l}</li>"));
            }
            out.push_str("</ul>");
        }
        out
    }

    fn system_html(&self, from: &str, view: &crate::systems::SystemView) -> String {
        let mut out = String::new();
        if let Some(observes) = &view.observes {
            out.push_str(&format!("\n<h2>Observes</h2>\n<p><code>{}</code></p>", escape(observes)));
        }
        out.push_str("\n<h2>Parameters</h2>\n");
        if view.params.is_empty() {
            out.push_str("<p class=\"no-docs\">none</p>");
        } else {
            out.push_str(&params_html(&view.params));
        }
        out.push_str("\n<h2>Registrations</h2>\n");
        if view.registrations.is_empty() {
            out.push_str("<p class=\"no-docs\">no registration found in source (one inside a macro is not read)</p>");
            return out;
        }
        out.push_str("<ul>\n");
        for reg in &view.registrations {
            out.push_str(&format!("<li>{}</li>\n", self.registration_html(from, reg)));
        }
        out.push_str("</ul>");
        out
    }

    fn registers_html(&self, from: &str, registered: &[Registered]) -> String {
        let mut out = String::from("\n<h2>Registers</h2>\n<ul>\n");
        for r in registered {
            let label = match r.target.and_then(|t| self.href(t.0, t.1, from)) {
                Some(h) => format!("<a href=\"{}\">{}</a>", escape(&h), escape(&r.text)),
                None => format!("<code>{}</code>", escape(&r.text)),
            };
            let schedule = if r.schedule.is_empty() {
                String::new()
            } else {
                format!(" in <code>{}</code>", escape(&r.schedule))
            };
            out.push_str(&format!(
                "<li>{} {label}{schedule} · <span class=\"where\">{}:{}</span></li>\n",
                r.what,
                escape(&r.site.0),
                r.site.1
            ));
        }
        out.push_str("</ul>");
        out
    }
}

fn params_html(params: &[Param]) -> String {
    let mut out = String::from("<ul>");
    for p in params {
        out.push_str(&format!(
            "<li><code>{}: {}</code>{}",
            escape(&p.name),
            escape(&p.ty),
            if p.optional { " <span class=\"vis\">optional</span>" } else { "" }
        ));
        if !p.data.is_empty() {
            out.push_str(&format!("<br>data: <code>{}</code>", escape(&p.data.join(", "))));
        }
        if !p.filters.is_empty() {
            out.push_str(&format!("<br>filters: <code>{}</code>", escape(&p.filters.join(", "))));
        }
        if !p.nested.is_empty() {
            out.push_str(&params_html(&p.nested));
        }
        out.push_str("</li>");
    }
    out.push_str("</ul>");
    out
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
