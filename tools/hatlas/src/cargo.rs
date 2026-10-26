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

/// A cargo workspace: its documentable targets and where its build output goes.
pub struct Workspace {
    pub target_directory: PathBuf,
    pub workspace_root: PathBuf,
    pub targets: Vec<Target>,
    toolchain: Toolchain,
}

impl Workspace {
    /// Runs `rustc -vV` and `cargo metadata --no-deps --locked`.
    pub fn load(repo: &Path) -> Result<Workspace, Error> {
        let vv = run_capture(Command::new("rustc").arg("-vV").current_dir(repo), "rustc -vV")?;
        let toolchain = toolchain_from(&vv);
        let mut cmd = cargo(&toolchain);
        cmd.args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
            .current_dir(repo);
        let text = run_capture(&mut cmd, "cargo metadata")?;
        let json: Value = serde_json::from_str(&text).map_err(|e| Error::Metadata(e.to_string()))?;
        let (target_directory, workspace_root, targets) = parse_metadata(&json)?;
        Ok(Workspace { target_directory, workspace_root, targets, toolchain })
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
}
