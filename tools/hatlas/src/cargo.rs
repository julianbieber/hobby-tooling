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
}

/// A cargo workspace: its documentable targets and where its build output goes.
pub struct Workspace {
    pub target_directory: PathBuf,
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
        let (target_directory, targets) = parse_metadata(&json)?;
        Ok(Workspace { target_directory, targets, toolchain })
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

    fn doc_target_dir(&self) -> PathBuf {
        match self.toolchain {
            Toolchain::Repo => self.target_directory.clone(),
            Toolchain::Nightly => self.target_directory.join("hatlas-nightly"),
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

/// Members' lib, proc-macro and bin targets, and the target directory.
pub fn parse_metadata(json: &Value) -> Result<(PathBuf, Vec<Target>), Error> {
    let bad = |what: &str| Error::Metadata(format!("no `{what}`"));
    let target_directory = json["target_directory"].as_str().ok_or_else(|| bad("target_directory"))?;
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
            });
        }
    }
    Ok((PathBuf::from(target_directory), targets))
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
            "workspace_members": ["p 0.1.0"],
            "packages": [
                {"id": "p 0.1.0", "name": "p", "targets": [
                    {"name": "p", "kind": ["lib"]},
                    {"name": "p", "kind": ["bin"]},
                    {"name": "ex", "kind": ["example"]},
                    {"name": "build-script-build", "kind": ["custom-build"]}]},
                {"id": "dep 1.0.0", "name": "dep", "targets": [{"name": "dep", "kind": ["lib"]}]}
            ]
        });
        let (dir, targets) = parse_metadata(&json).unwrap();
        assert_eq!(dir, PathBuf::from("/t"));
        assert_eq!(targets.len(), 2);
    }
}
