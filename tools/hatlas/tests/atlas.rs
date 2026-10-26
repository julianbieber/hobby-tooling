//! End-to-end checks of the atlas over the committed fixture rustdoc JSON, one run
//! per supported format version.

use hatlas::Error;
use hatlas::atlas::{CrateAtlas, Item, Module};
use hatlas::bevy::Role;
use hatlas::rustdoc::Crate;
use std::path::{Path, PathBuf};

const FIXTURES: &[&str] = &["tests/data/fixture-v57.json", "tests/data/fixture-v61.json"];

fn load(file: &str) -> Crate {
    Crate::load(Path::new(file)).unwrap()
}

fn atlas(file: &str) -> CrateAtlas {
    CrateAtlas::build(&load(file), "hatlas_fixture".into()).unwrap()
}

fn find<'a>(module: &'a Module, name: &str) -> Option<&'a Item> {
    module
        .items
        .iter()
        .find(|i| i.name == name)
        .or_else(|| module.children.iter().find_map(|c| find(c, name)))
}

fn site(file: &str, tag: &str) -> PathBuf {
    let atlas = hatlas::atlas::Atlas { crates: vec![atlas(file)] };
    let dir = std::env::temp_dir().join(format!("hatlas-it-{tag}-{}", std::process::id()));
    hatlas::site::render(&atlas, &dir).unwrap();
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
