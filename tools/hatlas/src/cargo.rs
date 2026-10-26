//! Everything that runs `cargo` or `rustc`: which toolchain, which targets, and the
//! rustdoc JSON each target produces.

use crate::Error;
use crate::rustdoc::Crate;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Which toolchain the rustdoc calls use.
#[derive(Debug, PartialEq, Eq)]
pub enum Toolchain {
    /// The repo's own, because it is already a nightly.
    Repo,
    /// `+nightly`, with its own target directory.
    Nightly,
}

/// Decides from the text of `rustc -vV` run in the repo, so a `rust-toolchain.toml` applies.
pub fn toolchain_from(rustc_vv: &str) -> Toolchain {
    let nightly = rustc_vv
        .lines()
        .any(|l| l.starts_with("release:") && l.contains("nightly"));
    if nightly { Toolchain::Repo } else { Toolchain::Nightly }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Lib,
    Bin,
}

/// One documentable target of a workspace member.
pub struct Target {
    pub package: String,
    pub name: String,
    pub kind: TargetKind,
    pub src_path: PathBuf,
}

/// One package of the resolved dependency graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub id: String,
    pub name: String,
    pub version: String,
}

/// A package's `(name, version)`.
pub type Wanted = (String, String);

/// What one dependency documentation run produced, per package.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DepDocs {
    /// The rustdoc JSON file of each package cargo documented.
    pub documented: Vec<(Wanted, PathBuf)>,
    /// The first line of cargo's error for each package that did not document.
    pub failed: Vec<(Wanted, String)>,
}

/// A cargo workspace: its documentable targets, its resolved packages and where its
/// build output goes.
pub struct Workspace {
    pub target_directory: PathBuf,
    pub workspace_root: PathBuf,
    pub targets: Vec<Target>,
    /// Every package in the resolved graph, workspace members included.
    pub packages: Vec<Package>,
    /// The names of the workspace's own packages.
    pub members: Vec<String>,
    toolchain: Toolchain,
}

impl Workspace {
    /// Runs `rustc -vV` and `cargo metadata --locked`.
    pub fn load(repo: &Path) -> Result<Workspace, Error> {
        let vv = run_capture(Command::new("rustc").arg("-vV").current_dir(repo), "rustc -vV")?;
        let toolchain = toolchain_from(&vv);
        let mut cmd = cargo(&toolchain);
        cmd.args(["metadata", "--format-version", "1", "--locked"])
            .current_dir(repo);
        let text = run_capture(&mut cmd, "cargo metadata")?;
        let json: Value = serde_json::from_str(&text).map_err(|e| Error::Metadata(e.to_string()))?;
        let (target_directory, workspace_root, targets) = parse_metadata(&json)?;
        let (packages, members) = parse_packages(&json);
        Ok(Workspace { target_directory, workspace_root, targets, packages, members, toolchain })
    }

    /// Documents the given dependency packages in one keep-going `cargo doc --no-deps`,
    /// public items only, and reports per package what happened.
    ///
    /// A package that fails to document is in `failed`; the call is an error only when
    /// cargo cannot be started. Empty `wanted` runs nothing. Re-documents on every run
    /// when the toolchain's cargo has no `doc --output-format`.
    pub fn document_dependencies(&self, repo: &Path, wanted: &[Wanted]) -> Result<DepDocs, Error> {
        if wanted.is_empty() {
            return Ok(DepDocs::default());
        }
        let dir = self.doc_target_dir();
        let native = run_capture(cargo(&self.toolchain).args(["doc", "--help"]).current_dir(repo), "cargo doc --help")
            .is_ok_and(|help| help.contains("--output-format"));
        let mut cmd = cargo(&self.toolchain);
        if native {
            cmd.arg("-Zunstable-options");
        }
        cmd.args(["doc", "--no-deps", "--keep-going", "--locked", "--message-format", "json"]);
        if native {
            cmd.args(["--output-format", "json"]);
        } else {
            cmd.env("RUSTDOCFLAGS", "-Zunstable-options --output-format json");
        }
        for (name, version) in wanted {
            cmd.arg("-p").arg(format!("{name}@{version}"));
        }
        if self.toolchain == Toolchain::Nightly {
            cmd.arg("--target-dir").arg(&dir);
        }
        cmd.current_dir(repo);
        let out = cmd
            .stderr(std::process::Stdio::inherit())
            .output()
            .map_err(|e| Error::Cargo { step: "cargo doc (dependencies)".to_string(), status: e.to_string() })?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut docs = parse_doc_messages(&stdout, &self.packages, &dir.join("doc"), wanted);
        if !out.status.success() && docs.documented.is_empty() && docs.failed.iter().all(|(_, why)| why == NOTHING_REPORTED) {
            let why = format!("cargo doc exited with {}", out.status);
            for (_, reason) in &mut docs.failed {
                *reason = why.clone();
            }
        }
        Ok(docs)
    }

    /// Builds one target's rustdoc JSON, including private items, and loads it at once:
    /// a library and a binary of one name write the same file.
    pub fn document(&self, repo: &Path, target: &Target) -> Result<Crate, Error> {
        let mut cmd = cargo(&self.toolchain);
        cmd.args(["-Zunstable-options", "rustdoc", "-p", &target.package]);
        match target.kind {
            TargetKind::Lib => cmd.arg("--lib"),
            TargetKind::Bin => cmd.args(["--bin", &target.name]),
        };
        cmd.args(["--locked", "--output-format", "json"]);
        let dir = self.doc_target_dir();
        if self.toolchain == Toolchain::Nightly {
            cmd.arg("--target-dir").arg(&dir);
        }
        cmd.args(["--", "--document-private-items"]).current_dir(repo);
        let step = format!("cargo rustdoc -p {} ({})", target.package, target.name);
        let status = cmd.status().map_err(|e| Error::Cargo {
            step: step.clone(),
            status: e.to_string(),
        })?;
        if !status.success() {
            return Err(Error::Cargo { step, status: status.to_string() });
        }
        let file = dir.join("doc").join(format!("{}.json", target.name.replace('-', "_")));
        Crate::load(&file)
    }

    /// Indexes the workspace with `rust-analyzer scip` and returns the index file.
    ///
    /// Needs `rust-analyzer` on the toolchain that applies, and builds the workspace's
    /// build scripts and proc macros; memory use is in the gigabytes for a large game.
    /// On a stable repo it runs under `+nightly` with its own target directory.
    pub fn scip(&self, repo: &Path) -> Result<PathBuf, Error> {
        let out = self.doc_target_dir().join("hatlas.scip");
        let mut cmd = Command::new("rust-analyzer");
        if self.toolchain == Toolchain::Nightly {
            cmd.arg("+nightly");
            cmd.env("CARGO_TARGET_DIR", self.target_directory.join("hatlas-nightly"));
        }
        cmd.arg("scip").arg(repo).arg("--output").arg(&out);
        let step = "rust-analyzer scip".to_string();
        let status = cmd
            .stdout(std::process::Stdio::null())
            .status()
            .map_err(|e| Error::Cargo { step: step.clone(), status: e.to_string() })?;
        if !status.success() {
            return Err(Error::Cargo { step, status: status.to_string() });
        }
        Ok(out)
    }

    /// Every `.rs` file under the directory of each target's entry point, deduplicated and
    /// sorted, as paths relative to the workspace root. These are the paths SCIP uses.
    pub fn source_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        let mut dirs: Vec<PathBuf> = self
            .targets
            .iter()
            .filter_map(|t| t.src_path.parent().map(Path::to_path_buf))
            .collect();
        dirs.sort();
        dirs.dedup();
        for dir in &dirs {
            collect_rs(dir, &mut files);
        }
        let mut rel: Vec<String> = files
            .iter()
            .filter_map(|f| f.strip_prefix(&self.workspace_root).ok())
            .map(|f| f.to_string_lossy().into_owned())
            .collect();
        rel.sort();
        rel.dedup();
        rel
    }

    fn doc_target_dir(&self) -> PathBuf {
        match self.toolchain {
            Toolchain::Repo => self.target_directory.clone(),
            Toolchain::Nightly => self.target_directory.join("hatlas-nightly"),
        }
    }
}

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn cargo(toolchain: &Toolchain) -> Command {
    let mut cmd = Command::new("cargo");
    if *toolchain == Toolchain::Nightly {
        cmd.arg("+nightly");
    }
    cmd
}

fn run_capture(cmd: &mut Command, step: &str) -> Result<String, Error> {
    let out = cmd
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| Error::Cargo { step: step.to_string(), status: e.to_string() })?;
    if !out.status.success() {
        return Err(Error::Cargo { step: step.to_string(), status: out.status.to_string() });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

const NOTHING_REPORTED: &str = "cargo reported nothing for it";

/// Every package of the graph and the names of the workspace members.
pub fn parse_packages(json: &Value) -> (Vec<Package>, Vec<String>) {
    let member_ids: Vec<&str> = json["workspace_members"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut packages = Vec::new();
    let mut members = Vec::new();
    for pkg in json["packages"].as_array().into_iter().flatten() {
        let (Some(id), Some(name)) = (pkg["id"].as_str(), pkg["name"].as_str()) else { continue };
        if member_ids.contains(&id) {
            members.push(name.to_string());
        }
        packages.push(Package {
            id: id.to_string(),
            name: name.to_string(),
            version: pkg["version"].as_str().unwrap_or("").to_string(),
        });
    }
    (packages, members)
}

/// Reads `cargo doc --message-format json` output into per-package results.
///
/// A package is documented when cargo emitted a doc artifact for it (one without an
/// rmeta, rlib or shared object, and not its build script); a leftover `<crate>.json` on disk proves nothing.
/// `doc_dir` is where the JSON lands when the artifact does not name the file.
pub fn parse_doc_messages(stdout: &str, packages: &[Package], doc_dir: &Path, wanted: &[Wanted]) -> DepDocs {
    let by_id = |id: &str| packages.iter().find(|p| p.id == id).map(|p| (p.name.clone(), p.version.clone()));
    let mut documented: Vec<(Wanted, PathBuf)> = Vec::new();
    let mut errors: Vec<(Wanted, String)> = Vec::new();
    for line in stdout.lines() {
        let Ok(msg) = serde_json::from_str::<Value>(line) else { continue };
        let Some(key) = msg["package_id"].as_str().and_then(by_id) else { continue };
        match msg["reason"].as_str() {
            Some("compiler-artifact") => {
                let kinds = msg["target"]["kind"].as_array().into_iter().flatten().filter_map(Value::as_str);
                if kinds.clone().any(|k| k == "custom-build") {
                    continue;
                }
                let files: Vec<&str> = msg["filenames"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
                if files.iter().any(|f| [".rmeta", ".rlib", ".so"].iter().any(|e| f.ends_with(e))) {
                    continue;
                }
                let file = files.iter().find(|f| f.ends_with(".json")).map(PathBuf::from).unwrap_or_else(|| {
                    let target = msg["target"]["name"].as_str().unwrap_or(&key.0);
                    doc_dir.join(format!("{}.json", target.replace('-', "_")))
                });
                if !documented.iter().any(|(k, _)| *k == key) {
                    documented.push((key, file));
                }
            }
            Some("compiler-message") if msg["message"]["level"].as_str() == Some("error") => {
                let text = msg["message"]["rendered"].as_str().or(msg["message"]["message"].as_str()).unwrap_or("error");
                let first = text.lines().next().unwrap_or(text).trim().to_string();
                if !errors.iter().any(|(k, _)| *k == key) {
                    errors.push((key, first));
                }
            }
            _ => {}
        }
    }
    let mut failed = Vec::new();
    for key in wanted {
        if documented.iter().any(|(k, _)| k == key) {
            continue;
        }
        let why = errors.iter().find(|(k, _)| k == key).map_or(NOTHING_REPORTED.to_string(), |(_, w)| w.clone());
        failed.push((key.clone(), why));
    }
    documented.retain(|(k, _)| wanted.contains(k));
    DepDocs { documented, failed }
}

/// The target directory, the workspace root, and the members' lib, proc-macro and bin targets.
pub fn parse_metadata(json: &Value) -> Result<(PathBuf, PathBuf, Vec<Target>), Error> {
    let bad = |what: &str| Error::Metadata(format!("no `{what}`"));
    let target_directory = json["target_directory"].as_str().ok_or_else(|| bad("target_directory"))?;
    let workspace_root = json["workspace_root"].as_str().ok_or_else(|| bad("workspace_root"))?;
    let members: Vec<&str> = json["workspace_members"]
        .as_array()
        .ok_or_else(|| bad("workspace_members"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let mut targets = Vec::new();
    for pkg in json["packages"].as_array().ok_or_else(|| bad("packages"))? {
        if !pkg["id"].as_str().is_some_and(|id| members.contains(&id)) {
            continue;
        }
        let package = pkg["name"].as_str().ok_or_else(|| bad("packages[].name"))?;
        for t in pkg["targets"].as_array().into_iter().flatten() {
            let kinds: Vec<&str> = t["kind"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let kind = if kinds.iter().any(|k| ["lib", "rlib", "dylib", "proc-macro"].contains(k)) {
                TargetKind::Lib
            } else if kinds.contains(&"bin") {
                TargetKind::Bin
            } else {
                continue;
            };
            targets.push(Target {
                package: package.to_string(),
                name: t["name"].as_str().ok_or_else(|| bad("targets[].name"))?.to_string(),
                kind,
                src_path: PathBuf::from(t["src_path"].as_str().ok_or_else(|| bad("targets[].src_path"))?),
            });
        }
    }
    Ok((PathBuf::from(target_directory), PathBuf::from(workspace_root), targets))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins that a stable repo is documented through `+nightly`, since rustdoc JSON needs one.
    #[test]
    fn stable_repo_uses_nightly() {
        assert_eq!(toolchain_from("rustc 1.90.0\nrelease: 1.90.0\n"), Toolchain::Nightly);
    }

    // Pins that a repo already on a nightly keeps its own toolchain and target directory.
    #[test]
    fn nightly_repo_keeps_its_toolchain() {
        assert_eq!(toolchain_from("rustc 1.97.0-nightly\nrelease: 1.97.0-nightly\n"), Toolchain::Repo);
    }

    // Pins that examples and tests are not documented, only libs and bins of members.
    #[test]
    fn metadata_keeps_lib_and_bin_targets_only() {
        let json: Value = serde_json::json!({
            "target_directory": "/t",
            "workspace_root": "/w",
            "workspace_members": ["p 0.1.0"],
            "packages": [
                {"id": "p 0.1.0", "name": "p", "targets": [
                    {"name": "p", "kind": ["lib"], "src_path": "/w/src/lib.rs"},
                    {"name": "p", "kind": ["bin"], "src_path": "/w/src/main.rs"},
                    {"name": "ex", "kind": ["example"], "src_path": "/w/examples/ex.rs"},
                    {"name": "build-script-build", "kind": ["custom-build"], "src_path": "/w/build.rs"}]},
                {"id": "dep 1.0.0", "name": "dep", "targets": [{"name": "dep", "kind": ["lib"], "src_path": "/d/lib.rs"}]}
            ]
        });
        let (dir, root, targets) = parse_metadata(&json).unwrap();
        assert_eq!((dir, root), (PathBuf::from("/t"), PathBuf::from("/w")));
        assert_eq!(targets.len(), 2);
    }

    fn pk(id: &str, name: &str, version: &str) -> Package {
        Package { id: id.into(), name: name.into(), version: version.into() }
    }

    // Pins how cargo's JSON becomes per-package results: both artifact shapes document, an rmeta never does, an error is attributed, and silence is a failure.
    #[test]
    fn doc_messages_are_attributed_per_package() {
        let packages = [pk("a1", "a", "1.0.0"), pk("b1", "b-x", "2.0.0"), pk("c1", "c", "3.0.0"), pk("d1", "d", "4.0.0"), pk("e1", "e", "5.0.0")];
        let wanted: Vec<Wanted> = packages.iter().map(|p| (p.name.clone(), p.version.clone())).collect();
        let stdout = [
            r#"{"reason":"compiler-artifact","package_id":"a1","target":{"name":"build-script-build","kind":["custom-build"]},"filenames":[]}"#,
            r#"{"reason":"compiler-artifact","package_id":"a1","target":{"name":"a","kind":["lib"]},"filenames":[]}"#,
            r#"{"reason":"compiler-artifact","package_id":"b1","target":{"name":"b-x"},"filenames":["/t/doc/b_x.json"]}"#,
            r#"{"reason":"compiler-message","package_id":"c1","message":{"level":"error","rendered":"error: boom\n  more"}}"#,
            r#"{"reason":"compiler-artifact","package_id":"d1","target":{"name":"d"},"filenames":["/t/debug/libd.rmeta"]}"#,
            "not json",
        ]
        .join("\n");
        let docs = parse_doc_messages(&stdout, &packages, Path::new("/t/doc"), &wanted);
        assert_eq!(
            docs.documented,
            [(("a".into(), "1.0.0".into()), PathBuf::from("/t/doc/a.json")), (("b-x".into(), "2.0.0".into()), PathBuf::from("/t/doc/b_x.json"))]
        );
        let failed: Vec<(&str, &str)> = docs.failed.iter().map(|((n, _), w)| (n.as_str(), w.as_str())).collect();
        assert_eq!(failed, [("c", "error: boom"), ("d", NOTHING_REPORTED), ("e", NOTHING_REPORTED)]);
    }
}
