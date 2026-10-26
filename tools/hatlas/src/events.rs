//! Deciding which event types a workspace produces and listens to, and which of its
//! items does each.

use crate::Error;
use crate::atlas::{Atlas, Module};
use crate::bevy::{BevyNames, Role};
use crate::rustdoc::Crate;
use crate::scip::{ScipIndex, parse_symbol};
use crate::source::{Call, CallKind};
use crate::systems::{ItemRef, Registrant, Registry, Resolver, SystemView, params_of};
use crate::types;
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};

/// An event type: the crate that defines it, with `-` written as `_`, and its name
/// without generic arguments. `Pointer<Click>` and `Pointer<Over>` share one key.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EventKey {
    pub crate_name: String,
    pub name: String,
}

/// Who produces or listens to an event.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Party {
    Item(ItemRef),
    /// An observer closure, shown on the page of the module it is written in.
    Anonymous { crate_index: usize, module: Vec<String>, anchor: String },
    Text(String),
}

/// One producer or listener of an event.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Endpoint {
    pub party: Party,
    /// How it takes part: `observer On<X>`, `reader MessageReader<X>`, or the name of the call.
    pub via: String,
    /// The item's own location for a signature, the call's location for a call.
    pub site: Option<(String, usize)>,
}

/// Everything known about one event type.
#[derive(Clone, Debug)]
pub struct EventFlow {
    pub key: EventKey,
    /// The workspace item defining it; `None` for an event a dependency defines.
    pub item: Option<ItemRef>,
    pub producers: Vec<Endpoint>,
    pub listeners: Vec<Endpoint>,
}

impl EventFlow {
    /// The crate that defines the event.
    pub fn defined_in(&self) -> &str {
        &self.key.crate_name
    }
}

/// Every event type the workspace reads, writes, triggers or observes, sorted by key.
#[derive(Clone, Debug, Default)]
pub struct Flows {
    pub flows: Vec<EventFlow>,
    /// `file:line — text` for each producer or listener whose event type could not be named.
    pub unresolved: Vec<String>,
}

impl Flows {
    /// The flow of this event type, if the workspace touches it.
    pub fn get(&self, key: &EventKey) -> Option<&EventFlow> {
        self.flows.binary_search_by(|f| f.key.cmp(key)).ok().map(|i| &self.flows[i])
    }

    /// The events this item is a listener of.
    pub fn listened_by(&self, r: ItemRef) -> impl Iterator<Item = &EventFlow> {
        self.flows.iter().filter(move |f| f.listeners.iter().any(|e| e.party == Party::Item(r)))
    }

    /// The events this item is a producer of.
    pub fn produced_by(&self, r: ItemRef) -> impl Iterator<Item = &EventFlow> {
        self.flows.iter().filter(move |f| f.producers.iter().any(|e| e.party == Party::Item(r)))
    }
}

/// Finds the event flows of the workspace. A function whose first parameter is `On<E>`
/// or `Trigger<E>` and that is not registered anywhere becomes an observer in the
/// atlas and the registry.
///
/// `docs[i]` must be the rustdoc of `atlas.crates[i]`. Fails with `MissingField` when a
/// signature contains a type the printer cannot render.
pub fn build(
    atlas: &mut Atlas,
    docs: &[Crate],
    scip: &ScipIndex,
    calls: &[Call],
    registry: &mut Registry,
) -> Result<Flows, Error> {
    let mut flows: BTreeMap<EventKey, EventFlow> = BTreeMap::new();
    let mut unresolved = Vec::new();
    let mut new_observers: Vec<(ItemRef, SystemView)> = Vec::new();

    for (ci, krate) in atlas.crates.iter().enumerate() {
        let mut stack = vec![&krate.root];
        while let Some(module) = stack.pop() {
            for item in module.items.iter().filter(|i| i.role == Role::Event) {
                let key = EventKey { crate_name: krate.package.replace('-', "_"), name: item.name.clone() };
                flows.insert(key.clone(), EventFlow { key, item: Some((ci, item.id)), producers: vec![], listeners: vec![] });
            }
            stack.extend(&module.children);
        }
    }

    for r in functions(atlas) {
        let krate = &docs[r.0];
        let Some(item) = atlas.item(r) else { continue };
        let json = krate.item(r.1)?;
        let inputs = krate.fn_inputs(json)?;
        let site = item.location.as_ref().map(|(f, l)| (f.clone(), *l as usize));
        let party = Party::Item(r);
        let note = |unresolved: &mut Vec<String>, shown: &str| {
            let (file, line) = site.clone().unwrap_or_default();
            unresolved.push(format!("{file}:{line} — {}: defining crate of {shown} unknown", item.name));
        };

        if let Some((_, ty)) = inputs.first()
            && types::last_segment(ty).is_some_and(|s| BevyNames::OBSERVER_PARAMS.contains(&s))
            && let Some(arg) = types::type_args(ty).first()
        {
            let shown = show(krate, ty)?;
            match key_of(atlas, docs, r.0, arg) {
                Some(key) => {
                    flows.entry(key.clone()).or_insert_with(|| fresh(key)).listeners.push(Endpoint {
                        party: party.clone(),
                        via: format!("observer {shown}"),
                        site: site.clone(),
                    });
                    if item.role == Role::Other {
                        let (params, observes) = params_of(atlas, docs, r)?;
                        new_observers.push((r, SystemView { params, observes, registrations: Vec::new() }));
                    }
                }
                None if types::last_segment(arg).is_none() && arg.get("generic").is_some() => {}
                None => note(&mut unresolved, &shown),
            }
        }

        let mut found = Vec::new();
        for (_, ty) in &inputs {
            io_types(atlas, krate, r.0, ty, 0, &mut found)?;
        }
        for (producer, ty) in found {
            let shown = show(krate, ty)?;
            let Some(arg) = types::type_args(ty).first().copied() else { continue };
            match key_of(atlas, docs, r.0, arg) {
                Some(key) => {
                    let end = Endpoint {
                        party: party.clone(),
                        via: format!("{} {shown}", if producer { "writer" } else { "reader" }),
                        site: site.clone(),
                    };
                    let flow = flows.entry(key.clone()).or_insert_with(|| fresh(key));
                    if producer { flow.producers.push(end) } else { flow.listeners.push(end) }
                }
                None if arg.get("generic").is_some() => {}
                None => note(&mut unresolved, &shown),
            }
        }
    }

    for (r, view) in new_observers {
        if let Some(item) = atlas.item_mut(r) {
            item.role = Role::Observer;
        }
        registry.systems.entry(r).or_insert(view);
    }

    let packages: HashSet<String> = atlas.crates.iter().map(|c| c.package.clone()).collect();
    let resolver = Resolver::new(atlas, scip, &packages);
    for call in calls {
        let CallKind::Produce { method, segments, text } = &call.kind else { continue };
        let site = Some((call.file.clone(), call.line));
        let symbols: Vec<Option<&str>> = segments.iter().map(|s| resolver.symbol(s)).collect();
        let key = type_segment(&symbols).and_then(|i| symbols[i]).and_then(parse_symbol).and_then(|s| {
            Some(EventKey { crate_name: s.package.replace('-', "_"), name: s.path.last()?.clone() })
        });
        let unambiguous = BevyNames::UNAMBIGUOUS_PRODUCING.contains(&method.as_str());
        let Some(key) = key else {
            if unambiguous {
                unresolved.push(format!("{}:{} — {method}({text}): event type not resolved", call.file, call.line));
            }
            continue;
        };
        if !unambiguous && !flows.contains_key(&key) {
            continue;
        }
        let party = match call.owner.as_ref().map(|o| resolver.owner(o)) {
            Some(Registrant::Item(r)) => Party::Item(r),
            Some(Registrant::Text(t)) => Party::Text(t),
            None => Party::Text("top level".into()),
        };
        flows
            .entry(key.clone())
            .or_insert_with(|| fresh(key))
            .producers
            .push(Endpoint { party, via: method.clone(), site });
    }

    for anon in &registry.anonymous {
        let Some(id) = &anon.observes else { continue };
        let key = resolver.symbol(id).and_then(parse_symbol).and_then(|s| {
            Some(EventKey { crate_name: s.package.replace('-', "_"), name: s.path.last()?.clone() })
        });
        let (file, line) = &anon.registration.site;
        match key {
            Some(key) => flows.entry(key.clone()).or_insert_with(|| fresh(key)).listeners.push(Endpoint {
                party: Party::Anonymous {
                    crate_index: anon.crate_index,
                    module: anon.module.clone(),
                    anchor: anon.anchor.clone(),
                },
                via: "observer closure".into(),
                site: Some((file.clone(), *line)),
            }),
            None => unresolved.push(format!("{file}:{line} — observer closure On<{}>: event type not resolved", id.name)),
        }
    }

    let mut flows: Vec<EventFlow> = flows.into_values().collect();
    for flow in &mut flows {
        flow.producers.sort();
        flow.producers.dedup();
        flow.listeners.sort();
        flow.listeners.dedup();
    }
    unresolved.sort();
    unresolved.dedup();
    Ok(Flows { flows, unresolved })
}

fn fresh(key: EventKey) -> EventFlow {
    EventFlow { key, item: None, producers: Vec::new(), listeners: Vec::new() }
}

fn show(krate: &Crate, ty: &Value) -> Result<String, Error> {
    types::display(ty).map_err(|v| krate.unknown_type(&v))
}

fn functions(atlas: &Atlas) -> Vec<ItemRef> {
    fn walk(ci: usize, module: &Module, out: &mut Vec<ItemRef>) {
        out.extend(module.items.iter().filter(|i| i.kind == "function").map(|i| (ci, i.id)));
        for child in &module.children {
            walk(ci, child, out);
        }
    }
    let mut out = Vec::new();
    for (ci, krate) in atlas.crates.iter().enumerate() {
        walk(ci, &krate.root, &mut out);
    }
    out
}

fn key_of(atlas: &Atlas, docs: &[Crate], ci: usize, arg: &Value) -> Option<EventKey> {
    let name = types::last_segment(arg)?.to_string();
    let id = arg["resolved_path"]["id"].as_u64()?;
    let defined = docs[ci].defining_crate(id)?;
    let crate_name = if defined == docs[ci].name() { atlas.crates[ci].package.clone() } else { defined };
    Some(EventKey { crate_name: crate_name.replace('-', "_"), name })
}

fn io_types<'a>(
    atlas: &Atlas,
    krate: &'a Crate,
    ci: usize,
    ty: &'a Value,
    depth: usize,
    out: &mut Vec<(bool, &'a Value)>,
) -> Result<(), Error> {
    let mut inner = ty;
    if types::last_segment(inner) == Some("Option")
        && let Some(arg) = types::type_args(inner).first()
    {
        inner = arg;
    }
    let Some(last) = types::last_segment(inner) else { return Ok(()) };
    if BevyNames::READERS.contains(&last) {
        out.push((false, inner));
    } else if BevyNames::WRITERS.contains(&last) {
        out.push((true, inner));
    } else if BevyNames::PARAM_SETS.contains(&last) {
        if let Some(set) = types::type_args(inner).first() {
            for member in types::members(set) {
                io_types(atlas, krate, ci, member, depth + 1, out)?;
            }
        }
    } else if depth < 4
        && let Some(id) = inner["resolved_path"]["id"].as_u64()
        && krate.item(id).is_ok()
        && atlas.item((ci, id)).is_some_and(|i| i.system_param)
    {
        for (_, field) in krate.struct_field_types(id)? {
            io_types(atlas, krate, ci, field, depth + 1, out)?;
        }
    }
    Ok(())
}

fn type_segment(symbols: &[Option<&str>]) -> Option<usize> {
    symbols.iter().position(|s| s.is_some_and(|s| s.ends_with('#')))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins that the event type is the first segment whose symbol is a type, so `AppExit::Success` names `AppExit` and a leading module is skipped.
    #[test]
    fn type_segment_skips_modules_and_prefers_the_enum() {
        let module = Some("rust-analyzer cargo p 1.0.0 ev/");
        let ty = Some("rust-analyzer cargo bevy_app 0.19.0 AppExit#");
        let variant = Some("rust-analyzer cargo bevy_app 0.19.0 AppExit#Success#");
        assert_eq!(type_segment(&[module, ty, variant]), Some(1));
        assert_eq!(type_segment(&[module, None]), None);
    }
}
