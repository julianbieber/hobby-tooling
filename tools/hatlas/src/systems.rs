//! Joining the registration calls read from source with the compiler's view of the
//! names in them: which item each call registers, from where, under which schedule,
//! conditions and sets, and with which parameters.

use crate::Error;
use crate::atlas::Atlas;
use crate::bevy::{BevyNames, Role};
use crate::rustdoc::{Crate, Id};
use crate::scip::{ScipIndex, parse_symbol};
use crate::source::{Call, CallKind, Entry, Ident, Owner, Target};
use crate::types;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// An item as the atlas addresses it: the crate's index and its rustdoc id.
pub type ItemRef = (usize, Id);

/// What made a registration: an item of the workspace, or text for anything else.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Registrant {
    Item(ItemRef),
    Text(String),
}

/// One place a system or observer is registered.
#[derive(Clone, Debug)]
pub struct Registration {
    pub registered_by: Option<Registrant>,
    pub schedule: String,
    pub conditions: Vec<String>,
    /// A condition and the set it came through.
    pub inherited: Vec<(String, String)>,
    pub sets: Vec<String>,
    pub ordering: Vec<String>,
    pub site: (String, usize),
}

/// A parameter of a system, with the parts a reader looks at.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub ty: String,
    pub optional: bool,
    pub data: Vec<String>,
    pub filters: Vec<String>,
    pub nested: Vec<Param>,
}

/// Everything known about one system or observer function.
#[derive(Clone, Debug, Default)]
pub struct SystemView {
    pub params: Vec<Param>,
    pub observes: Option<String>,
    pub registrations: Vec<Registration>,
}

/// A closure registered as a system, shown on the page of the module it is written in.
#[derive(Clone, Debug)]
pub struct AnonymousSystem {
    pub crate_index: usize,
    pub module: Vec<String>,
    pub anchor: String,
    pub registration: Registration,
}

/// What a plugin or function registers, one entry per thing.
#[derive(Clone, Debug)]
pub struct Registered {
    pub what: &'static str,
    pub text: String,
    pub target: Option<ItemRef>,
    pub schedule: String,
    pub site: (String, usize),
}

/// Every registration of the workspace, resolved against the atlas.
#[derive(Default)]
pub struct Registry {
    pub systems: HashMap<ItemRef, SystemView>,
    pub anonymous: Vec<AnonymousSystem>,
    pub registers: HashMap<ItemRef, Vec<Registered>>,
    pub resource_by: HashMap<ItemRef, Vec<Registrant>>,
    /// `file:line — text` for each registration the join could not attribute.
    pub unresolved: Vec<String>,
}

enum Res {
    Item(ItemRef),
    Workspace,
    External,
    Unknown,
}

struct SetConfig {
    key: String,
    text: String,
    schedule: String,
    conditions: Vec<String>,
    parents: Vec<String>,
}

impl Registry {
    /// Resolves every call's names and assigns the `System`, `Observer` and function
    /// `Plugin` roles it finds; a struct's role from classification is never replaced.
    ///
    /// `docs[i]` must be the rustdoc of `atlas.crates[i]`. Fails with `MissingField` when a
    /// system's signature contains a type the printer cannot render.
    pub fn build(atlas: &mut Atlas, docs: &[Crate], scip: &ScipIndex, calls: &[Call]) -> Result<Registry, Error> {
        let mut registry = Registry::default();
        let mut roles: Vec<(ItemRef, Role)> = Vec::new();
        let mut sets: Vec<SetConfig> = Vec::new();
        let mut pending: Vec<(ItemRef, Role, Registration, &Entry)> = Vec::new();
        let mut anon_keys: Vec<Vec<String>> = Vec::new();
        let packages: HashSet<String> = atlas.crates.iter().map(|c| c.package.clone()).collect();
        let resolver = Resolver { atlas: &*atlas, scip, packages: &packages };

        for call in calls {
            let owner = call.owner.as_ref().map(|o| resolver.owner(o));
            let owner_item = match &owner {
                Some(Registrant::Item(i)) => Some(*i),
                _ => None,
            };
            let site = (call.file.clone(), call.line);
            let note = |registry: &mut Registry, text: &str| {
                registry.unresolved.push(format!("{}:{} — {}", call.file, call.line, text));
            };
            match &call.kind {
                CallKind::Sets { schedule, entries } => {
                    for e in entries {
                        let Target::Named(id) = &e.target else { continue };
                        sets.push(SetConfig {
                            key: resolver.set_key(id),
                            text: id.text.clone(),
                            schedule: schedule.clone(),
                            conditions: e.conditions.clone(),
                            parents: e.sets.iter().map(|s| resolver.set_key(s)).collect(),
                        });
                        record(&mut registry, owner_item, "set", &id.text, None, schedule, &site);
                    }
                }
                CallKind::Systems { schedule, entries } => {
                    for e in entries {
                        let registration = Registration {
                            registered_by: owner.clone(),
                            schedule: schedule.clone(),
                            conditions: e.conditions.clone(),
                            inherited: Vec::new(),
                            sets: e.sets.iter().map(|s| s.text.clone()).collect(),
                            ordering: e.ordering.clone(),
                            site: site.clone(),
                        };
                        match &e.target {
                            Target::Closure { line, col, .. } => match owner_item.and_then(|o| resolver.atlas.item(o).map(|i| (o, i))) {
                                Some((o, item)) => {
                                    anon_keys.push(e.sets.iter().map(|s| resolver.set_key(s)).collect());
                                    registry.anonymous.push(AnonymousSystem {
                                        crate_index: o.0,
                                        module: item.path[..item.path.len() - 1].to_vec(),
                                        anchor: format!("closure.L{line}C{col}"),
                                        registration,
                                    })
                                }
                                None => note(&mut registry, "closure with an owner outside the atlas"),
                            },
                            Target::Unparsed(text) => note(&mut registry, text),
                            Target::Named(id) => match resolver.resolve(id) {
                                Res::Item(r) => {
                                    record(&mut registry, owner_item, "system", &id.text, Some(r), schedule, &site);
                                    roles.push((r, Role::System));
                                    pending.push((r, Role::System, registration, e));
                                }
                                Res::External => record(&mut registry, owner_item, "system", &id.text, None, schedule, &site),
                                Res::Workspace | Res::Unknown => note(&mut registry, &id.text),
                            },
                        }
                    }
                }
                CallKind::Observer(entries) => {
                    for e in entries {
                        let registration = Registration {
                            registered_by: owner.clone(),
                            schedule: String::new(),
                            conditions: e.conditions.clone(),
                            inherited: Vec::new(),
                            sets: Vec::new(),
                            ordering: Vec::new(),
                            site: site.clone(),
                        };
                        match &e.target {
                            Target::Named(id) => match resolver.resolve(id) {
                                Res::Item(r) => {
                                    record(&mut registry, owner_item, "observer", &id.text, Some(r), "", &site);
                                    roles.push((r, Role::Observer));
                                    pending.push((r, Role::Observer, registration, e));
                                }
                                Res::External => record(&mut registry, owner_item, "observer", &id.text, None, "", &site),
                                Res::Workspace | Res::Unknown => note(&mut registry, &id.text),
                            },
                            Target::Closure { line, col, .. } => match owner_item.and_then(|o| resolver.atlas.item(o).map(|i| (o, i))) {
                                Some((o, item)) => {
                                    anon_keys.push(e.sets.iter().map(|s| resolver.set_key(s)).collect());
                                    registry.anonymous.push(AnonymousSystem {
                                        crate_index: o.0,
                                        module: item.path[..item.path.len() - 1].to_vec(),
                                        anchor: format!("closure.L{line}C{col}"),
                                        registration,
                                    })
                                }
                                None => note(&mut registry, "closure with an owner outside the atlas"),
                            },
                            Target::Unparsed(text) => note(&mut registry, text),
                        }
                    }
                }
                CallKind::Plugins(entries) => {
                    for e in entries {
                        match &e.target {
                            Target::Named(id) => match resolver.resolve(id) {
                                Res::Item(r) => {
                                    if resolver.atlas.item(r).is_some_and(|i| i.kind == "function") {
                                        roles.push((r, Role::Plugin));
                                    }
                                    record(&mut registry, owner_item, "plugin", &id.text, Some(r), "", &site);
                                }
                                Res::External => record(&mut registry, owner_item, "plugin", &id.text, None, "", &site),
                                Res::Workspace | Res::Unknown => note(&mut registry, &id.text),
                            },
                            Target::Unparsed(text) => note(&mut registry, text),
                            Target::Closure { .. } => note(&mut registry, "closure as a plugin"),
                        }
                    }
                }
                CallKind::Resource(e) => {
                    if let Target::Named(id) = &e.target {
                        let found = match resolver.resolve(id) {
                            Res::Item(r) => Some(r),
                            _ => None,
                        };
                        record(&mut registry, owner_item, "resource", &id.text, found, "", &site);
                        if let (Some(r), Some(by)) = (found, owner.clone()) {
                            registry.resource_by.entry(r).or_default().push(by);
                        }
                    }
                }
            }
        }

        let keys_of = |e: &Entry| e.sets.iter().map(|s| resolver.set_key(s)).collect::<Vec<_>>();
        let inherited: Vec<Vec<(String, String)>> = pending
            .iter()
            .map(|(_, _, reg, e)| inherit(&sets, &keys_of(e), &reg.schedule))
            .collect();
        for (anon, keys) in registry.anonymous.iter_mut().zip(anon_keys) {
            anon.registration.inherited = inherit(&sets, &keys, &anon.registration.schedule);
        }
        for ((r, _, mut registration, _), inh) in pending.into_iter().zip(inherited) {
            registration.inherited = inh;
            registry.systems.entry(r).or_default().registrations.push(registration);
        }

        for (r, role) in roles {
            let Some(item) = atlas.item_mut(r) else { continue };
            if item.kind != "function" {
                continue;
            }
            if rank(role) > rank(item.role) {
                item.role = role;
            }
        }

        let keys: Vec<ItemRef> = registry.systems.keys().copied().collect();
        for r in keys {
            let role = atlas.item(r).map_or(Role::Other, |i| i.role);
            if !matches!(role, Role::System | Role::Observer) {
                registry.systems.remove(&r);
                continue;
            }
            let (view_params, observes) = params_of(atlas, docs, r)?;
            let view = registry.systems.get_mut(&r).expect("key was listed above");
            view.params = view_params;
            view.observes = if role == Role::Observer { observes } else { None };
        }
        registry.unresolved.sort();
        registry.unresolved.dedup();
        Ok(registry)
    }
}

fn rank(role: Role) -> u8 {
    match role {
        Role::Plugin => 3,
        Role::Observer => 2,
        Role::System => 1,
        _ => 0,
    }
}

fn record(
    registry: &mut Registry,
    owner: Option<ItemRef>,
    what: &'static str,
    text: &str,
    target: Option<ItemRef>,
    schedule: &str,
    site: &(String, usize),
) {
    let Some(owner) = owner else { return };
    registry.registers.entry(owner).or_default().push(Registered {
        what,
        text: text.to_string(),
        target,
        schedule: schedule.to_string(),
        site: site.clone(),
    });
}

struct Resolver<'a> {
    atlas: &'a Atlas,
    scip: &'a ScipIndex,
    packages: &'a HashSet<String>,
}

impl Resolver<'_> {
    fn symbol(&self, id: &Ident) -> Option<&str> {
        let line = id.line.checked_sub(1)?;
        self.scip
            .symbol_at(&id.file, line, id.col)
            .or_else(|| self.scip.symbol_named(&id.file, line, &id.name))
    }

    fn resolve(&self, id: &Ident) -> Res {
        let Some(symbol) = self.symbol(id).and_then(parse_symbol) else {
            return Res::Unknown;
        };
        if !self.packages.contains(&symbol.package) {
            return Res::External;
        }
        let rooted = match symbol.path.as_slice() {
            [root, rest @ ..] if root == "lib" => Some(rest),
            [root, _name, rest @ ..] if root == "bin" => Some(rest),
            _ => None,
        };
        let found = self
            .atlas
            .find(&symbol.package, &symbol.path)
            .or_else(|| rooted.and_then(|rest| self.atlas.find(&symbol.package, rest)));
        match found {
            Some(found) => Res::Item(found),
            None => Res::Workspace,
        }
    }

    fn owner(&self, owner: &Owner) -> Registrant {
        match owner {
            Owner::Plugin(id) | Owner::Function(id) => match self.resolve(id) {
                Res::Item(r) => Registrant::Item(r),
                _ => Registrant::Text(id.name.clone()),
            },
            Owner::Other(text) => Registrant::Text(text.clone()),
        }
    }

    fn set_key(&self, id: &Ident) -> String {
        match self.symbol(id) {
            Some(s) => s.to_string(),
            None => format!("text:{}", id.text.split_whitespace().collect::<String>()),
        }
    }
}

fn inherit(sets: &[SetConfig], keys: &[String], schedule: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut stack: Vec<&str> = keys.iter().map(String::as_str).collect();
    while let Some(key) = stack.pop() {
        if !seen.insert(key) {
            continue;
        }
        for cfg in sets.iter().filter(|c| c.key == key) {
            if !cfg.schedule.is_empty() && !schedule.is_empty() && cfg.schedule != schedule {
                continue;
            }
            for c in &cfg.conditions {
                let pair = (c.clone(), cfg.text.clone());
                if !out.contains(&pair) {
                    out.push(pair);
                }
            }
            stack.extend(cfg.parents.iter().map(String::as_str));
        }
    }
    out
}

fn params_of(atlas: &Atlas, docs: &[Crate], r: ItemRef) -> Result<(Vec<Param>, Option<String>), Error> {
    let krate = &docs[r.0];
    let item = krate.item(r.1)?;
    let inputs = krate.fn_inputs(item)?;
    let mut params = Vec::new();
    let mut observes = None;
    for (name, ty) in inputs {
        if observes.is_none()
            && types::last_segment(ty).is_some_and(|s| BevyNames::OBSERVER_PARAMS.contains(&s))
            && let Some(arg) = types::type_args(ty).first()
        {
            observes = Some(show(krate, arg)?);
        }
        params.push(param(atlas, krate, r.0, name, ty, 0)?);
    }
    Ok((params, observes))
}

fn show(krate: &Crate, ty: &Value) -> Result<String, Error> {
    types::display(ty).map_err(|v| krate.unknown_type(&v))
}

fn param(atlas: &Atlas, krate: &Crate, ci: usize, name: String, ty: &Value, depth: usize) -> Result<Param, Error> {
    let shown = show(krate, ty)?;
    let mut p = Param { name, ty: shown, optional: false, data: Vec::new(), filters: Vec::new(), nested: Vec::new() };
    let mut inner = ty;
    if types::last_segment(inner) == Some("Option")
        && let Some(arg) = types::type_args(inner).first()
    {
        p.optional = true;
        inner = arg;
    }
    let Some(last) = types::last_segment(inner) else { return Ok(p) };
    let args = types::type_args(inner);
    if BevyNames::QUERIES.contains(&last) {
        if let Some(data) = args.first() {
            p.data = types::members(data).into_iter().map(|t| show(krate, t)).collect::<Result<_, _>>()?;
        }
        if let Some(filter) = args.get(1) {
            p.filters = types::members(filter).into_iter().map(|t| show(krate, t)).collect::<Result<_, _>>()?;
        }
    } else if BevyNames::PARAM_SETS.contains(&last) {
        if let Some(set) = args.first() {
            for (i, member) in types::members(set).into_iter().enumerate() {
                p.nested.push(param(atlas, krate, ci, format!("p{i}"), member, depth + 1)?);
            }
        }
    } else if depth < 4 {
        let id = inner["resolved_path"]["id"].as_u64();
        if let Some(id) = id {
            let local_param = krate.item(id).is_ok() && atlas.item((ci, id)).is_some_and(|i| i.system_param);
            if local_param {
                for (field, fty) in krate.struct_field_types(id)? {
                    p.nested.push(param(atlas, krate, ci, field, fty, depth + 1)?);
                }
            }
        }
    }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(key: &str, schedule: &str, conditions: &[&str], parents: &[&str]) -> SetConfig {
        SetConfig {
            key: key.into(),
            text: key.into(),
            schedule: schedule.into(),
            conditions: conditions.iter().map(|s| s.to_string()).collect(),
            parents: parents.iter().map(|s| s.to_string()).collect(),
        }
    }

    // Pins that a condition on an outer set reaches a system that is only in the inner set, naming the set it came through.
    #[test]
    fn conditions_inherit_through_nested_sets() {
        let sets = [cfg("inner", "Update", &[], &["outer"]), cfg("outer", "Update", &["in_state(X)"], &[])];
        assert_eq!(inherit(&sets, &["inner".into()], "Update"), [("in_state(X)".to_string(), "outer".to_string())]);
    }

    // Pins that a set configured for another schedule gives its conditions to nothing here.
    #[test]
    fn other_schedule_does_not_inherit() {
        let sets = [cfg("s", "PostUpdate", &["c"], &[])];
        assert!(inherit(&sets, &["s".into()], "Update").is_empty());
        assert_eq!(inherit(&sets, &["s".into()], "").len(), 1);
    }

    // Pins that sets which contain each other terminate instead of looping.
    #[test]
    fn cyclic_sets_terminate() {
        let sets = [cfg("a", "", &["c"], &["b"]), cfg("b", "", &[], &["a"])];
        assert_eq!(inherit(&sets, &["a".into()], "").len(), 1);
    }
}
