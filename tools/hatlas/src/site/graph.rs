//! The overview graph on the index: the public Bevy items and the dependency crates as
//! nodes, how they relate as edges, and the script data file the browser draws it from.

use super::{Site, has_page, write};
use crate::Error;
use crate::atlas::Module;
use crate::events::Party;
use crate::site::names::relative;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::path::Path;

const CYTOSCAPE: &str = include_str!("../../assets/cytoscape.min.js");

const GRAPH_INIT: &str = "(function(){var g=window.HATLAS_GRAPH,el=document.getElementById('graph');\
var cy=cytoscape({container:el,elements:{nodes:g.nodes,edges:g.edges},\
layout:{name:'breadthfirst',directed:true,roots:g.roots.length?g.roots:undefined,spacingFactor:1.2,animate:false},\
wheelSensitivity:0.3,\
style:[{selector:'node',style:{'label':'data(label)','font-size':10,'text-valign':'bottom','width':14,'height':14,'background-color':'#888'}},\
{selector:'node[kind=\"plugin\"]',style:{'background-color':'#c0392b','shape':'diamond','width':20,'height':20}},\
{selector:'node[kind=\"component\"]',style:{'background-color':'#2980b9'}},\
{selector:'node[kind=\"resource\"]',style:{'background-color':'#27ae60'}},\
{selector:'node[kind=\"event\"]',style:{'background-color':'#f39c12','shape':'triangle'}},\
{selector:'node[kind=\"system\"]',style:{'background-color':'#8e44ad','shape':'round-rectangle'}},\
{selector:'node[kind=\"observer\"]',style:{'background-color':'#16a085','shape':'round-rectangle'}},\
{selector:'node[kind=\"crate\"]',style:{'background-color':'#555','shape':'rectangle','width':18,'height':18}},\
{selector:'edge',style:{'width':1,'line-color':'#bbb','target-arrow-shape':'triangle','target-arrow-color':'#bbb','curve-style':'bezier'}}]});\
cy.on('tap','node',function(e){window.location.href=e.target.data('href');});\
el.setAttribute('data-nodes',String(cy.nodes().length));})();";

const LEGEND: &str = "<p class=\"summary\">plugin · component · resource · event · system · observer · crate. \
Scroll to zoom, drag to pan, click a node to open its page.</p>";

fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}

fn node_items(module: &Module, ci: usize, out: &mut Vec<(usize, Vec<String>, crate::rustdoc::Id)>) {
    for item in &module.items {
        if item.public && has_page(item.role) {
            out.push((ci, module.path.clone(), item.id));
        }
    }
    for child in &module.children {
        node_items(child, ci, out);
    }
}

impl Site<'_> {
    pub(super) fn write_graph(&self, root: &Path) -> Result<(), Error> {
        write(root, "assets/cytoscape.min.js", CYTOSCAPE)?;
        let json = serde_json::to_string(&self.graph_json()).unwrap_or_else(|_| "{}".to_string());
        write(root, "graph-data.js", &format!("window.HATLAS_GRAPH = {};\n", script_safe(&json)))
    }

    fn graph_json(&self) -> Value {
        let mut nodes = Vec::new();
        let mut roots = Vec::new();
        let mut ids: BTreeSet<String> = BTreeSet::new();
        let mut items = Vec::new();
        for (ci, krate) in self.atlas.crates.iter().enumerate() {
            node_items(&krate.root, ci, &mut items);
        }
        for (ci, module_path, id) in &items {
            let Some(item) = self.atlas.item((*ci, *id)) else { continue };
            let Some(page) = self.item_pages.get(&(*ci, *id)) else { continue };
            let node = format!("i{ci}-{id}");
            if item.role == crate::bevy::Role::Plugin {
                roots.push(node.clone());
            }
            ids.insert(node.clone());
            nodes.push(json!({"data": {
                "id": node,
                "label": item.name,
                "kind": item.role.label().to_lowercase(),
                "module": module_path.join("::"),
                "href": relative("index.html", page),
            }}));
        }
        for (ci, krate) in self.deps.crates.iter().enumerate() {
            let node = format!("d{ci}");
            ids.insert(node.clone());
            nodes.push(json!({"data": {
                "id": node,
                "label": format!("{}@{}", krate.name, krate.version),
                "kind": "crate",
                "href": relative("index.html", &self.dep_crate_pages[ci]),
            }}));
        }

        let item_node = |r: &(usize, crate::rustdoc::Id)| format!("i{}-{}", r.0, r.1);
        let mut edges: BTreeSet<(String, String, &str)> = BTreeSet::new();
        for (owner, list) in &self.registry.registers {
            for target in list.iter().filter_map(|r| r.target) {
                edges.insert((item_node(owner), item_node(&target), "registers"));
            }
        }
        let crate_node: HashMap<&str, String> = self
            .deps
            .crates
            .iter()
            .enumerate()
            .map(|(ci, k)| (k.crate_name.as_str(), format!("d{ci}")))
            .collect();
        for flow in &self.flows.flows {
            let event = match flow.item {
                Some(r) => Some(item_node(&r)),
                None => crate_node.get(flow.key.crate_name.as_str()).cloned(),
            };
            let Some(event) = event else { continue };
            for p in &flow.producers {
                if let Party::Item(r) = &p.party {
                    edges.insert((item_node(r), event.clone(), "produces"));
                }
            }
            for l in &flow.listeners {
                if let Party::Item(r) = &l.party {
                    edges.insert((event.clone(), item_node(r), "listens"));
                }
            }
        }
        for (dci, krate) in self.deps.crates.iter().enumerate() {
            for used in &krate.items {
                for file in used.uses.keys() {
                    for (ci, c) in self.atlas.crates.iter().enumerate() {
                        let Some(module) = c.root.module_of_file(file) else { continue };
                        for item in module.items.iter().filter(|i| i.public && has_page(i.role)) {
                            edges.insert((format!("i{ci}-{}", item.id), format!("d{dci}"), "uses"));
                        }
                        break;
                    }
                }
            }
        }
        let edges: Vec<Value> = edges
            .into_iter()
            .filter(|(s, t, _)| ids.contains(s) && ids.contains(t))
            .enumerate()
            .map(|(n, (s, t, kind))| json!({"data": {"id": format!("e{n}"), "source": s, "target": t, "kind": kind}}))
            .collect();
        json!({"nodes": nodes, "edges": edges, "roots": roots})
    }

    pub(super) fn graph_html(&self) -> String {
        format!(
            "<h2>Overview</h2>\n<div id=\"graph\"></div>\n{LEGEND}\n<script src=\"assets/cytoscape.min.js\"></script>\n\
<script src=\"graph-data.js\"></script>\n<script>{GRAPH_INIT}</script>\n"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins that text which could end a script element or a JS string never survives into graph-data.js, and that the data is unchanged once parsed.
    #[test]
    fn script_safe_escapes_and_round_trips() {
        let original = "</script>&\u{2028}\u{2029}x";
        let json = serde_json::to_string(original).unwrap();
        let safe = script_safe(&json);
        for c in ['<', '>', '&', '\u{2028}', '\u{2029}'] {
            assert!(!safe.contains(c), "{c:?} survived");
        }
        assert_eq!(serde_json::from_str::<String>(&safe).unwrap(), original);
    }
}
