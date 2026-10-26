//! `hatlas` — turns a Bevy workspace's rustdoc JSON into a set of linked local HTML
//! pages, and owns the order in which that happens.

pub mod atlas;
pub mod bevy;
pub mod cargo;
pub mod rustdoc;
pub mod site;

use std::fmt;
use std::path::PathBuf;

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
    pub pages: usize,
}

/// Every way a run fails. Each variant names the input that drifted, so the message
/// says what to look at without a second run.
#[derive(Debug)]
pub enum Error {
    Cargo { step: String, status: String },
    Metadata(String),
    UnsupportedFormat { file: PathBuf, found: u32, supported: Vec<u32> },
    MissingField { file: PathBuf, version: Option<u32>, field: String },
    DanglingLink { page: PathBuf, link: String },
    NotASite(PathBuf),
    Io { path: PathBuf, source: std::io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cargo { step, status } => write!(f, "`{step}` failed ({status})"),
            Error::Metadata(why) => write!(f, "cargo metadata: {why}"),
            Error::UnsupportedFormat { file, found, supported } => write!(
                f,
                "{}: rustdoc JSON format_version {found} is not supported (supported: {supported:?})",
                file.display()
            ),
            Error::MissingField { file, version, field } => {
                let v = version.map_or("unknown".to_string(), |v| v.to_string());
                write!(f, "{}: rustdoc JSON (format_version {v}) has no `{field}`", file.display())
            }
            Error::DanglingLink { page, link } => {
                write!(f, "{} links to `{link}`, which the site does not contain", page.display())
            }
            Error::NotASite(path) => write!(
                f,
                "{} exists and is not a hatlas site (no {} marker); refusing to replace it",
                path.display(),
                site::swap::MARKER
            ),
            Error::Io { path, source } => write!(f, "{}: {source}", path.display()),
        }
    }
}

impl std::error::Error for Error {}

/// Documents the workspace, renders the site, checks every link, and replaces the
/// previous site only once the new one has passed.
///
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
    let mut names: Vec<String> = Vec::new();
    for target in &workspace.targets {
        let doc = workspace.document(&repo, target)?;
        let mut name = doc.name().to_string();
        if names.contains(&name) {
            name = format!("{name}_bin");
        }
        names.push(name.clone());
        crates.push(atlas::CrateAtlas::build(&doc, name)?);
    }
    let atlas = atlas::Atlas { crates };

    let tmp = site::swap::tmp_dir(&out);
    let result = site::render(&atlas, &tmp)
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
        pages,
    })
}
