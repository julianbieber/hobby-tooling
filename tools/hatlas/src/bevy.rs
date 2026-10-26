//! What counts as a Bevy thing: the trait names it recognises and the decision that
//! turns an item's trait impls into a role.

use crate::rustdoc::{Crate, Id};
use std::collections::HashMap;

/// Every era's names for the Bevy traits and calls the atlas reads.
/// Names are matched on the last path segment of a trait, never on the full path.
pub struct BevyNames;

impl BevyNames {
    pub const PLUGIN: &'static str = "Plugin";
    pub const COMPONENT: &'static str = "Component";
    pub const RESOURCE: &'static str = "Resource";
    pub const EVENT_TRAITS: &'static [&'static str] = &["Event", "EntityEvent", "Message"];
    pub const READERS: &'static [&'static str] = &["EventReader", "MessageReader"];
    pub const WRITERS: &'static [&'static str] = &["EventWriter", "MessageWriter"];
    pub const OBSERVER_PARAMS: &'static [&'static str] = &["On", "Trigger"];
    pub const PRODUCING_CALLS: &'static [&'static str] =
        &["trigger", "write_message", "send_event", "send", "write"];

    pub const SYSTEM_PARAM: &'static str = "SystemParam";
    pub const QUERIES: &'static [&'static str] = &["Query", "Single", "Populated"];
    pub const PARAM_SETS: &'static [&'static str] = &["ParamSet"];
    pub const ADD_SYSTEMS: &'static str = "add_systems";
    pub const CONFIGURE_SETS: &'static [&'static str] = &["configure_sets", "configure_set"];
    pub const ADD_OBSERVER: &'static [&'static str] = &["add_observer", "observe"];
    pub const ADD_PLUGINS: &'static [&'static str] = &["add_plugins", "add_plugin"];
    pub const RESOURCE_CALLS: &'static [&'static str] = &["init_resource", "insert_resource"];
    pub const CONDITIONS: &'static [&'static str] = &["run_if", "distributive_run_if"];
    pub const IN_SET: &'static str = "in_set";
    pub const CHAINS: &'static [&'static str] = &["chain", "chain_ignore_deferred"];
    pub const AFTER: &'static [&'static str] = &["after", "after_ignore_deferred"];
    pub const BEFORE: &'static [&'static str] = &["before", "before_ignore_deferred"];

    /// The trait names that count at all, in precedence order.
    pub fn traits() -> Vec<&'static str> {
        let mut all = vec![Self::PLUGIN, Self::RESOURCE, Self::COMPONENT];
        all.extend(Self::EVENT_TRAITS);
        all.push(Self::SYSTEM_PARAM);
        all
    }
}

/// What an item is to a Bevy reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Plugin,
    Component,
    Resource,
    Event,
    System,
    Observer,
    Other,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Plugin => "Plugin",
            Role::Component => "Component",
            Role::Resource => "Resource",
            Role::Event => "Event",
            Role::System => "System",
            Role::Observer => "Observer",
            Role::Other => "Item",
        }
    }
}

/// Roles for every item of one crate, decided once from its impls.
pub struct BevyClassifier {
    traits: HashMap<Id, Vec<&'static str>>,
}

impl BevyClassifier {
    /// A trait impl counts only when the trait's name matches [`BevyNames`] and the
    /// crate defining the trait is named `bevy_*`.
    pub fn new(krate: &Crate) -> BevyClassifier {
        let known = BevyNames::traits();
        let mut traits: HashMap<Id, Vec<&'static str>> = HashMap::new();
        for imp in krate.impls() {
            let Some(name) = known.iter().find(|n| **n == imp.trait_name) else {
                continue;
            };
            let from_bevy = krate
                .defining_crate(imp.trait_id)
                .is_some_and(|c| c.starts_with("bevy_"));
            if from_bevy {
                traits.entry(imp.for_id).or_default().push(name);
            }
        }
        BevyClassifier { traits }
    }

    /// Plugin, then resource (a resource is also a component), then component, then event.
    pub fn classify(&self, id: Id) -> Role {
        let Some(found) = self.traits.get(&id) else {
            return Role::Other;
        };
        let has = |n: &str| found.contains(&n);
        if has(BevyNames::PLUGIN) {
            Role::Plugin
        } else if has(BevyNames::RESOURCE) {
            Role::Resource
        } else if has(BevyNames::COMPONENT) {
            Role::Component
        } else if BevyNames::EVENT_TRAITS.iter().any(|n| has(n)) {
            Role::Event
        } else {
            Role::Other
        }
    }

    /// Whether the item implements Bevy's `SystemParam`, which is a capability and not a role.
    pub fn is_system_param(&self, id: Id) -> bool {
        self.traits_of(id).contains(&BevyNames::SYSTEM_PARAM)
    }

    /// The matching Bevy trait names an item implements, in the order they were found.
    pub fn traits_of(&self, id: Id) -> &[&'static str] {
        self.traits.get(&id).map_or(&[], Vec::as_slice)
    }
}
