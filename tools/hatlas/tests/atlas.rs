//! End-to-end checks of the atlas over the committed fixture rustdoc JSON, one run
//! per supported format version.

use hatlas::Error;
use hatlas::events::{EventKey, Flows};
use hatlas::atlas::{Atlas, CrateAtlas, Item, Module};
use hatlas::scip::ScipIndex;
use hatlas::source;
use hatlas::systems::{Registry, SystemView};
use hatlas::bevy::Role;
use hatlas::rustdoc::Crate;
use std::path::{Path, PathBuf};

const FIXTURES: &[&str] = &["tests/data/fixture-v57.json", "tests/data/fixture-v61.json"];

fn load(file: &str) -> Crate {
    Crate::load(Path::new(file)).unwrap()
}

fn atlas(file: &str) -> CrateAtlas {
    CrateAtlas::build(&load(file), "hatlas_fixture".into(), "hatlas-fixture".into()).unwrap()
}

fn registry(file: &str) -> (Atlas, Registry) {
    let (atlas, registry, _) = flows(file);
    (atlas, registry)
}

fn flows(file: &str) -> (Atlas, Registry, Flows) {
    let mut atlas = Atlas { crates: vec![atlas(file)] };
    let docs = [load(file)];
    let scip = ScipIndex::load(Path::new("tests/data/fixture.scip")).unwrap();
    let (calls, notes) = source::read(Path::new("fixture"), &["src/lib.rs".to_string()]);
    assert!(notes.is_empty(), "{notes:?}");
    let mut registry = Registry::build(&mut atlas, &docs, &scip, &calls).unwrap();
    let flows = hatlas::events::build(&mut atlas, &docs, &scip, &calls, &mut registry).unwrap();
    (atlas, registry, flows)
}

fn system<'a>(atlas: &'a Atlas, registry: &'a Registry, name: &str) -> &'a SystemView {
    let item = find(&atlas.crates[0].root, name).unwrap_or_else(|| panic!("no {name}"));
    registry.systems.get(&(0, item.id)).unwrap_or_else(|| panic!("{name} has no registration"))
}

fn find<'a>(module: &'a Module, name: &str) -> Option<&'a Item> {
    module
        .items
        .iter()
        .find(|i| i.name == name)
        .or_else(|| module.children.iter().find_map(|c| find(c, name)))
}

fn site(file: &str, tag: &str) -> PathBuf {
    let (atlas, registry, flows) = flows(file);
    let dir = std::env::temp_dir().join(format!("hatlas-it-{tag}-{}", std::process::id()));
    hatlas::site::render(&atlas, &registry, &flows, &dir).unwrap();
    dir
}

fn read_all(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            out.extend(read_all(&path));
        } else if path.extension().is_some_and(|e| e == "html") {
            let text = std::fs::read_to_string(&path).unwrap();
            out.push((path, text));
        }
    }
    out
}

// Pins the role rules: a resource is not reported as the component it also is, and a trait merely named Component is not Bevy's.
#[test]
fn roles_follow_the_bevy_traits() {
    for file in FIXTURES {
        let a = atlas(file);
        let role = |n: &str| find(&a.root, n).unwrap_or_else(|| panic!("{file}: no {n}")).role;
        assert_eq!(role("ScorePlugin"), Role::Plugin, "{file}");
        assert_eq!(role("HiddenPlugin"), Role::Plugin, "{file}");
        assert_eq!(role("Score"), Role::Resource, "{file}");
        assert_eq!(role("Progress"), Role::Component, "{file}");
        assert_eq!(role("Scored"), Role::Event, "{file}");
        assert_eq!(role("NotBevy"), Role::Other, "{file}");
    }
}

// Pins that a document with no format_version fails naming the field, instead of being read as some default version.
#[test]
fn missing_format_version_is_named() {
    for file in FIXTURES {
        let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        json.as_object_mut().unwrap().remove("format_version");
        match Crate::from_value(file.into(), json) {
            Err(Error::MissingField { field, .. }) => assert_eq!(field, "format_version"),
            _ => panic!("expected MissingField"),
        }
    }
}

// Pins that a format nobody has a fixture for is refused with the versions that are supported.
#[test]
fn unsupported_version_names_found_and_supported() {
    let file = FIXTURES[0];
    let mut json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
    json["format_version"] = 1.into();
    let Err(err) = Crate::from_value(file.into(), json) else {
        panic!("expected an error");
    };
    let text = err.to_string();
    assert!(text.contains("format_version 1") && text.contains("[57, 61]"), "{text}");
}

// Pins that a rendered fixture site has no dangling link, which is the check a real run relies on.
#[test]
fn rendered_site_passes_the_link_check() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let dir = site(file, &format!("links{n}"));
        hatlas::site::check::check(&dir).unwrap();
    }
}

// Pins the privacy rule: the overview lists public things only, and a private item sits on its own module's page.
#[test]
fn private_items_stay_off_the_index() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let dir = site(file, &format!("priv{n}"));
        let top = std::fs::read_to_string(dir.join("index.html")).unwrap();
        assert!(!top.contains("Ledger") && !top.contains("HiddenPlugin"), "{file}");
        let hidden = std::fs::read_to_string(dir.join("hatlas_fixture/hidden/index.html")).unwrap();
        assert!(hidden.contains("Ledger") && hidden.contains("Kept inside its own module"), "{file}");
    }
}

// Pins that hostile doc text cannot become markup, and that an intra-doc link reaches the resource's page.
#[test]
fn plugin_page_is_escaped_and_linked() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let dir = site(file, &format!("esc{n}"));
        let (path, text) = read_all(&dir)
            .into_iter()
            .find(|(p, _)| p.file_name().is_some_and(|f| f == "struct.ScorePlugin.html"))
            .unwrap();
        let _ = path;
        assert!(text.contains("&lt;script&gt;") && !text.contains("<script"), "{file}");
        assert!(!text.contains("href=\"javascript:"), "{file}");
        assert!(text.contains("href=\"struct.Score.html\""), "{file}");
        assert!(text.contains("href=\"https://example.com/book\""), "{file}");
    }
}

// Pins the acceptance in miniature: a system in a set inherits the run condition configured on that set, and says which set it came through.
#[test]
fn system_inherits_its_sets_condition() {
    for file in FIXTURES {
        let (atlas, registry) = registry(file);
        let reg = &system(&atlas, &registry, "report_score").registrations[0];
        assert_eq!(reg.schedule, "Update", "{file}");
        assert_eq!(reg.inherited, [("resource_exists::<Score>".to_string(), "ScoreSystems::Report".to_string())], "{file}");
        assert_eq!(reg.sets, ["ScoreSystems::Report"], "{file}");
    }
}

// Pins that a query's parameter is split into data and filters by position, and that Option is marked.
#[test]
fn query_parameters_split_data_and_filters() {
    for file in FIXTURES {
        let (atlas, registry) = registry(file);
        let view = system(&atlas, &registry, "count_progress");
        let query = &view.params[0];
        assert_eq!(query.data, ["&Progress", "Option<&Name>"], "{file}");
        assert_eq!(query.filters, ["With<Progress>", "Without<Marker>"], "{file}");
        assert_eq!(view.params[1].ty, "ResMut<Score>", "{file}");
        let report = system(&atlas, &registry, "report_score");
        assert_eq!(report.params[0].data, ["&Progress"], "{file}");
        assert_eq!(report.params[0].filters, ["Changed<Progress>"], "{file}");
        assert!(report.params[1].optional, "{file}");
        assert!(report.params[2].nested.len() == 2, "{file}");
    }
}

// Pins that a chain gives the later system an ordering after the earlier one.
#[test]
fn chain_orders_systems() {
    for file in FIXTURES {
        let (atlas, registry) = registry(file);
        let reg = &system(&atlas, &registry, "report_score").registrations[0];
        assert_eq!(reg.ordering, ["after count_progress"], "{file}");
    }
}

// Pins that a ParamSet's members are listed as nested parameters.
#[test]
fn param_set_members_are_nested() {
    for file in FIXTURES {
        let (atlas, registry) = registry(file);
        let view = system(&atlas, &registry, "mixed");
        assert_eq!(view.params[0].nested.len(), 2, "{file}");
        assert_eq!(view.params[0].nested[0].data, ["&Progress"], "{file}");
    }
}

// Pins that a closure registered as a system is kept with an anchor on its module page, and that the link check still passes.
#[test]
fn closure_is_an_anonymous_system_with_an_anchor() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let (_, registry) = registry(file);
        assert_eq!(registry.anonymous.len(), 1, "{file}");
        let dir = site(file, &format!("anon{n}"));
        let root = std::fs::read_to_string(dir.join("hatlas_fixture/index.html")).unwrap();
        assert!(root.contains(&format!("id=\"{}\"", registry.anonymous[0].anchor)), "{file}");
    }
}

// Pins that observers and function plugins get their roles from the calls that register them, and that an observer names its event.
#[test]
fn observer_and_function_plugin_get_roles() {
    for file in FIXTURES {
        let (atlas, registry) = registry(file);
        let role = |n: &str| find(&atlas.crates[0].root, n).unwrap().role;
        assert_eq!(role("on_scored"), Role::Observer, "{file}");
        assert_eq!(role("extra_plugin"), Role::Plugin, "{file}");
        assert_eq!(role("count_progress"), Role::System, "{file}");
        assert_eq!(system(&atlas, &registry, "on_scored").observes.as_deref(), Some("Scored"), "{file}");
    }
}

// Pins that a registration through a macro is listed as unresolved instead of being guessed at or dropped.
#[test]
fn macro_registration_is_unresolved() {
    for file in FIXTURES {
        let (_, registry) = registry(file);
        assert!(registry.unresolved.iter().any(|u| u.contains("score_macro!()")), "{file}: {:?}", registry.unresolved);
    }
}

// Pins that a plugin's page lists what it registers and the resource's page names its plugin.
#[test]
fn plugin_and_resource_pages_cross_reference() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let dir = site(file, &format!("reg{n}"));
        let plugin = std::fs::read_to_string(dir.join("hatlas_fixture/struct.ScorePlugin.html")).unwrap();
        assert!(plugin.contains("Registers") && plugin.contains("function.count_progress.html"), "{file}");
        let score = std::fs::read_to_string(dir.join("hatlas_fixture/struct.Score.html")).unwrap();
        assert!(score.contains("Registered by") && score.contains("struct.ScorePlugin.html"), "{file}");
        let page = std::fs::read_to_string(dir.join("hatlas_fixture/function.report_score.html")).unwrap();
        assert!(page.contains("resource_exists::&lt;Score&gt;") && page.contains("ScoreSystems::Report"), "{file}");
    }
}

fn key(name: &str) -> EventKey {
    EventKey { crate_name: "hatlas_fixture".into(), name: name.into() }
}

fn name_of(atlas: &Atlas, party: &hatlas::events::Party) -> String {
    match party {
        hatlas::events::Party::Item(r) => atlas.item(*r).unwrap().name.clone(),
        other => format!("{other:?}"),
    }
}

// Pins the trigger half and the observer half of an event: a `trigger` call in a body is a producer with its line, and both a registered and an unregistered `On<E>` function are listeners.
#[test]
fn trigger_and_observers_make_a_flow() {
    for file in FIXTURES {
        let (atlas, _, flows) = flows(file);
        let flow = flows.get(&key("Scored")).unwrap_or_else(|| panic!("{file}: no Scored flow"));
        assert_eq!(flow.defined_in(), "hatlas_fixture", "{file}");
        let listeners: Vec<_> = flow.listeners.iter().map(|e| name_of(&atlas, &e.party)).collect();
        assert!(listeners.contains(&"on_scored".to_string()) && listeners.contains(&"heard_elsewhere".to_string()), "{file}: {listeners:?}");
        let fire = flow.producers.iter().find(|e| name_of(&atlas, &e.party) == "fire").unwrap_or_else(|| panic!("{file}: no producer"));
        assert_eq!(fire.via, "trigger", "{file}");
        assert!(fire.site.as_ref().is_some_and(|(f, _)| f == "src/lib.rs"), "{file}");
    }
}

// Pins that a message writer parameter is a producer and a reader parameter a listener, and that the `write` call in the writer's body is listed beside it, by its call site, because the type is already a known event.
#[test]
fn writer_and_reader_parameters_make_a_flow() {
    for file in FIXTURES {
        let (atlas, _, flows) = flows(file);
        let flow = flows.get(&key("Tallied")).unwrap_or_else(|| panic!("{file}: no Tallied flow"));
        let producers: Vec<_> = flow.producers.iter().map(|e| (name_of(&atlas, &e.party), e.via.clone())).collect();
        assert_eq!(
            producers,
            [("announce".to_string(), "write".to_string()), ("announce".to_string(), "writer MessageWriter<Tallied>".to_string())],
            "{file}"
        );
        let listeners: Vec<_> = flow.listeners.iter().map(|e| name_of(&atlas, &e.party)).collect();
        assert_eq!(listeners, ["hear"], "{file}");
    }
}

// Pins that an `On<E>` function registered nowhere is still an observer with a page that says no registration was found.
#[test]
fn unregistered_observer_is_found_by_signature() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let (atlas, registry, _) = flows(file);
        let item = find(&atlas.crates[0].root, "heard_elsewhere").unwrap();
        assert_eq!(item.role, Role::Observer, "{file}");
        assert!(registry.systems[&(0, item.id)].registrations.is_empty(), "{file}");
        let dir = site(file, &format!("unreg{n}"));
        let page = std::fs::read_to_string(dir.join("hatlas_fixture/function.heard_elsewhere.html")).unwrap();
        assert!(page.contains("no registration found"), "{file}");
    }
}

// Pins the cross-links: the event's page lists its producer and listeners, and a listener's page links back to the event and the producers.
#[test]
fn event_and_listener_pages_link_each_other() {
    for (n, file) in FIXTURES.iter().enumerate() {
        let dir = site(file, &format!("flow{n}"));
        let read = |p: &str| std::fs::read_to_string(dir.join("hatlas_fixture").join(p)).unwrap();
        let event = read("struct.Scored.html");
        assert!(event.contains("function.fire.html") && event.contains("function.on_scored.html"), "{file}");
        assert!(event.contains("Defined in <code>hatlas_fixture</code>"), "{file}");
        let listener = read("function.on_scored.html");
        assert!(listener.contains("Listens to") && listener.contains("struct.Scored.html") && listener.contains("function.fire.html"), "{file}");
        assert!(read("function.fire.html").contains("Produces"), "{file}");
    }
}
