//! The pages for the dependencies a workspace uses: one per documented crate and one per
//! item of it that the workspace references, each listing the files that use it.

use super::{Site, page, write};
use crate::Error;
use crate::deps::{DepCrate, DependencyUse};
use crate::site::markdown::{escape, render_docs, summary};
use crate::site::names::{PageNames, relative};
use std::path::Path;

fn crate_dir(krate: &DepCrate) -> String {
    format!("{}-{}", krate.name, krate.version)
}

fn kind_label(kind: &str) -> String {
    let spaced = kind.replace('_', " ");
    let mut chars = spaced.chars();
    chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

fn full_path(krate: &DepCrate, item: &DependencyUse) -> Vec<String> {
    let mut path = vec![krate.crate_name.clone()];
    path.extend(item.module.iter().cloned());
    path.push(item.name.clone());
    path
}

impl Site<'_> {
    pub(super) fn register_deps(&mut self, names: &mut PageNames) {
        for (ci, krate) in self.deps.crates.iter().enumerate() {
            let dir = crate_dir(krate);
            self.dep_crate_pages.push(names.module(&["deps".to_string(), dir.clone()]));
            for (ii, item) in krate.items.iter().enumerate() {
                let mut module = vec!["deps".to_string(), dir.clone()];
                module.extend(item.module.iter().cloned());
                let page = names.item(&module, &item.kind, &item.name.replace("::", "."));
                self.dep_item_pages.insert((ci, ii), page);
                if !item.name.contains("::") {
                    self.dep_lookup.insert((krate.crate_name.clone(), full_path(krate, item)), (ci, ii));
                }
            }
        }
    }

    pub(super) fn deps_index_html(&self, from: &str) -> String {
        let mut body = String::new();
        if !self.deps.crates.is_empty() {
            body.push_str("<h2>Dependencies</h2>\n<ul>\n");
            for (ci, krate) in self.deps.crates.iter().enumerate() {
                body.push_str(&format!(
                    "<li><a href=\"{}\">{}@{}</a> <span class=\"where\">{} items used</span></li>\n",
                    escape(&relative(from, &self.dep_crate_pages[ci])),
                    escape(&krate.name),
                    escape(&krate.version),
                    krate.items.len()
                ));
            }
            body.push_str("</ul>\n");
        }
        if !self.deps.undocumented.is_empty() {
            body.push_str("<h2>Undocumented crates</h2>\n<ul>\n");
            for u in &self.deps.undocumented {
                body.push_str(&format!(
                    "<li><code>{}@{}</code> — {}</li>\n",
                    escape(&u.name),
                    escape(&u.version),
                    escape(&u.reason)
                ));
            }
            body.push_str("</ul>\n");
        }
        if self.deps.crates.iter().any(|c| !c.unmapped.is_empty()) {
            body.push_str("<h2>Unmapped symbols</h2>\n");
            for krate in self.deps.crates.iter().filter(|c| !c.unmapped.is_empty()) {
                body.push_str(&format!(
                    "<details><summary>{}@{} ({})</summary>\n{}</details>\n",
                    escape(&krate.name),
                    escape(&krate.version),
                    krate.unmapped.len(),
                    unmapped_html(krate)
                ));
            }
        }
        if !self.deps.std.is_empty() {
            let names: Vec<String> = self.deps.std.iter().map(|n| format!("<code>{}</code>", escape(n))).collect();
            body.push_str(&format!("<p class=\"where\">Not documented, plain text: {}</p>\n", names.join(", ")));
        }
        body
    }

    pub(super) fn write_deps(&self, root: &Path) -> Result<usize, Error> {
        let mut pages = 0;
        for (ci, krate) in self.deps.crates.iter().enumerate() {
            self.write_dep_crate(root, ci, krate)?;
            pages += 1;
            for (ii, item) in krate.items.iter().enumerate() {
                self.write_dep_item(root, ci, ii, krate, item)?;
                pages += 1;
            }
        }
        Ok(pages)
    }

    fn write_dep_crate(&self, root: &Path, ci: usize, krate: &DepCrate) -> Result<(), Error> {
        let from = self.dep_crate_pages[ci].as_str();
        let mut body = format!(
            "<p class=\"crumbs\"><a href=\"{}\">Atlas</a> :: dependencies :: {}</p>\n<h1>Crate {}</h1>\n<p class=\"crumbs\">{}@{}</p>\n",
            escape(&relative(from, "index.html")),
            escape(&krate.crate_name),
            escape(&krate.crate_name),
            escape(&krate.name),
            escape(&krate.version)
        );
        body.push_str("<h2>Used items</h2>\n");
        if krate.items.is_empty() {
            body.push_str("<p class=\"no-docs\">no item of this crate was mapped</p>");
        } else {
            body.push_str("<ul>\n");
            for (ii, item) in krate.items.iter().enumerate() {
                let sites: usize = item.uses.values().map(Vec::len).sum();
                let module: Vec<&str> = item.module.iter().map(String::as_str).collect();
                body.push_str(&format!(
                    "<li>{} <a href=\"{}\">{}</a> <span class=\"where\">{}</span> <span class=\"where\">{} files, {} sites</span> <span class=\"summary\">{}</span></li>\n",
                    escape(&item.kind.replace('_', " ")),
                    escape(&relative(from, &self.dep_item_pages[&(ci, ii)])),
                    escape(&item.name),
                    escape(&module.join("::")),
                    item.uses.len(),
                    sites,
                    summary(item.docs.as_deref())
                ));
            }
            body.push_str("</ul>");
        }
        if !krate.unmapped.is_empty() {
            body.push_str(&format!("\n<h2>Unmapped symbols</h2>\n{}", unmapped_html(krate)));
        }
        write(root, from, &page(&format!("Crate {}", krate.crate_name), &body))
    }

    fn write_dep_item(&self, root: &Path, ci: usize, ii: usize, krate: &DepCrate, item: &DependencyUse) -> Result<(), Error> {
        let from = self.dep_item_pages[&(ci, ii)].as_str();
        let mut path = vec![krate.crate_name.clone()];
        path.extend(item.module.iter().cloned());
        let mut crumbs = vec![
            format!("<a href=\"{}\">Atlas</a>", escape(&relative(from, "index.html"))),
            format!("<a href=\"{}\">{}</a>", escape(&relative(from, &self.dep_crate_pages[ci])), escape(&krate.crate_name)),
        ];
        crumbs.extend(path.iter().skip(1).map(|s| escape(s)));
        let mut body = format!("<p class=\"crumbs\">{}</p>\n", crumbs.join(" :: "));
        body.push_str(&format!("<h1>{} {}</h1>\n", escape(&kind_label(&item.kind)), escape(&item.name)));
        body.push_str(&format!("<p class=\"crumbs\">{}@{}</p>\n", escape(&krate.name), escape(&krate.version)));
        body.push_str(&self.dep_docs_html(from, item));
        body.push_str("\n<h2>Used in</h2>\n");
        for (file, sites) in &item.uses {
            let label = match self.atlas.module_of_file(file) {
                Some(m) => format!("<a href=\"{}\">{}</a>", escape(&relative(from, &self.module_pages[&m.path])), escape(file)),
                None => escape(file),
            };
            body.push_str(&format!("<h3>{label}</h3>\n<ul>\n"));
            for site in sites {
                let member = site.member.as_ref().map_or(String::new(), |m| format!(" · <code>{}</code>", escape(m)));
                body.push_str(&format!("<li>line {}{member}</li>\n", site.line));
            }
            body.push_str("</ul>\n");
        }
        write(root, from, &page(&format!("{} {}", kind_label(&item.kind), item.name), &body))
    }

    fn dep_docs_html(&self, from: &str, item: &DependencyUse) -> String {
        match item.docs.as_deref() {
            Some(d) if !d.trim().is_empty() => {
                let resolve = |key: &str| {
                    let target = item.links.iter().find(|(k, _)| k == key)?.1.as_ref()?;
                    let (ci, ii) = self.dep_lookup.get(&(target.0.clone(), target.1.clone()))?;
                    Some(relative(from, &self.dep_item_pages[&(*ci, *ii)]))
                };
                format!("<section class=\"docs\">{}</section>", render_docs(d, &resolve))
            }
            _ => "<p class=\"no-docs\">no rustdoc</p>".to_string(),
        }
    }
}

fn unmapped_html(krate: &DepCrate) -> String {
    let mut out = String::from("<ul>\n");
    for symbol in &krate.unmapped {
        out.push_str(&format!("<li><code>{}</code></li>\n", escape(symbol)));
    }
    out.push_str("</ul>\n");
    out
}
