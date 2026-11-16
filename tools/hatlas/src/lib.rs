//! `hatlas` — turns a Bevy workspace's rustdoc JSON into a set of linked local HTML
//! pages, and owns the order in which that happens.

pub mod atlas;
pub mod bevy;
pub mod cargo;
pub mod deps;
pub mod events;
pub mod rustdoc;
pub mod scip;
pub mod site;
pub mod source;
pub mod systems;
pub mod types;

use std::path::PathBuf;
use std::process::ExitStatus;

/// What a run was asked to do.
pub struct Options {
    /// The workspace to document; cargo runs with this as its working directory.
    pub repo: PathBuf,
    /// Where the site goes. `None` means `<target directory>/hatlas`.
    pub out: Option<PathBuf>,
}

/// What a successful run wrote.
pub struct Summary {
    pub index: PathBuf,
    pub crates: usize,
    pub modules: usize,
    pub plugins: usize,
    pub components: usize,
    pub resources: usize,
    pub systems: usize,
    pub observers: usize,
    pub events: usize,
    pub dependencies: usize,
    pub dependency_items: usize,
    pub undocumented: usize,
    pub unmapped: usize,
    pub unresolved: usize,
    pub pages: usize,
}

/// Every way a run fails. Each variant names the input that drifted, so the message
/// says what to look at without a second run.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not start `{step}`")]
    Spawn {
        step: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`{step}` failed ({status})")]
    Cargo { step: String, status: ExitStatus },
    #[error("cargo metadata is not JSON")]
    MetadataJson(#[source] serde_json::Error),
    #[error("cargo metadata has no `{0}`")]
    Metadata(&'static str),
    #[error("{}: not rustdoc JSON", path.display())]
    RustdocJson {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{}: not a SCIP index", path.display())]
    Scip {
        path: PathBuf,
        #[source]
        source: protobuf::Error,
    },
    #[error("{}: rustdoc JSON format_version {found} is not supported (supported: {supported:?})", file.display())]
    UnsupportedFormat {
        file: PathBuf,
        found: u64,
        supported: Vec<u64>,
    },
    #[error("{}: rustdoc JSON (format_version {}) has no `{field}`", file.display(), version.map_or_else(|| "unknown".to_string(), |v| v.to_string()))]
    MissingField {
        file: PathBuf,
        version: Option<u64>,
        field: String,
    },
    #[error("{} links to `{link}`, which the site does not contain", page.display())]
    DanglingLink { page: PathBuf, link: String },
    #[error("{} exists and is not a hatlas site (no {} marker); refusing to replace it", .0.display(), site::swap::MARKER)]
    NotASite(PathBuf),
    #[error("{}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Documents the workspace, renders the site, checks every link, and replaces the
/// previous site only once the new one has passed.
///
/// A dependency that fails to document is listed on the index and the run goes on.
/// Fails with `NotASite` before any compiling when the output directory exists
/// without the marker file. Runs `cargo`, which executes the workspace's build
/// scripts and proc macros. On any error the previous site is left as it was.
pub fn run(options: &Options) -> Result<Summary, Error> {
    let repo = options.repo.canonicalize().map_err(|source| Error::Io {
        path: options.repo.clone(),
        source,
    })?;
    let workspace = cargo::Workspace::load(&repo)?;
    let out = options
        .out
        .clone()
        .unwrap_or_else(|| workspace.target_directory.join("hatlas"));
    site::swap::guard(&out)?;

    let mut crates = Vec::new();
    let mut docs = Vec::new();
    let mut names: Vec<String> = Vec::new();
    for target in &workspace.targets {
        let doc = workspace.document(&repo, target)?;
        let mut name = doc.name().to_string();
        if names.contains(&name) {
            name = format!("{name}_bin");
        }
        names.push(name.clone());
        crates.push(atlas::CrateAtlas::build(
            &doc,
            name,
            target.package.clone(),
        )?);
        docs.push(doc);
    }
    let mut atlas = atlas::Atlas { crates };

    let scip = scip::ScipIndex::load(&workspace.scip(&repo)?)?;
    let (calls, notes) = source::read(&workspace.workspace_root, &workspace.source_files());
    let mut registry = systems::Registry::build(&mut atlas, &docs, &scip, &calls)?;
    registry.unresolved.extend(notes);
    let flows = events::build(&mut atlas, &docs, &scip, &calls, &mut registry)?;
    registry.unresolved.extend(flows.unresolved.iter().cloned());
    registry.unresolved.sort();
    registry.unresolved.dedup();

    let referenced = deps::referenced(&scip, &workspace.members);
    let dep_docs = workspace.document_dependencies(&repo, &referenced.wanted)?;
    let dependencies = deps::build(&scip, dep_docs, referenced)?;

    let tmp = site::swap::tmp_dir(&out);
    let result = site::render(&atlas, &registry, &flows, &dependencies, &tmp)
        .and_then(|pages| site::check::check(&tmp).map(|()| pages))
        .and_then(|pages| site::swap::swap(&tmp, &out).map(|()| pages));
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    let pages = result?;

    let count = |role| atlas.count_role(role);
    Ok(Summary {
        index: out.join("index.html"),
        crates: atlas.crates.len(),
        modules: atlas.crates.iter().map(|c| c.root.count_modules()).sum(),
        plugins: count(bevy::Role::Plugin),
        components: count(bevy::Role::Component),
        resources: count(bevy::Role::Resource),
        systems: count(bevy::Role::System),
        observers: count(bevy::Role::Observer),
        events: flows.flows.len(),
        dependencies: dependencies.crates.len(),
        dependency_items: dependencies.crates.iter().map(|c| c.items.len()).sum(),
        undocumented: dependencies.undocumented.len(),
        unmapped: dependencies.crates.iter().map(|c| c.unmapped.len()).sum(),
        unresolved: registry.unresolved.len(),
        pages,
    })
}
