//! The skill / command index: a snapshot of every invocable Claude Code
//! slash-command and skill, mapping its name to the source file, description,
//! tier, and kind.
//!
//! Three tiers are merged into one namespace (lower `Tier` ordinal wins a name
//! collision):
//!   * **Project** — `<workspace-root>/.claude/{commands,skills}`
//!   * **User**    — `$CLAUDE_CONFIG_DIR/{commands,skills}`, default `~/.claude`
//!   * **Plugin**  — enabled plugins under the same config directory
//!
//! The user tier follows `CLAUDE_CONFIG_DIR` because this machine runs two Claude
//! accounts out of two config directories — `~/.claude` for work, `~/.claude-hobby`
//! for hobby projects. Anchoring on `$HOME/.claude` unconditionally would offer the
//! work skills in a hobby session and the hobby skills nowhere. The variable is
//! exported by `hcdev` before it launches zellij, so the Helix pane — and this
//! server under it — inherits the right one.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use serde::Deserialize;

/// Generous cap on walk depth; command namespaces are shallow but we allow
/// nesting without pulling in unbounded trees.
const MAX_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Command,
    Skill,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Command => "command",
            Kind::Skill => "skill",
        }
    }
}

/// Precedence for name collisions — the numeric order is meaningful: a lower
/// value shadows a higher one when two tiers define the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    Project = 0,
    User = 1,
    Plugin = 2,
}

impl Tier {
    pub fn label(self) -> &'static str {
        match self {
            Tier::Project => "project",
            Tier::User => "user",
            Tier::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    /// Invocable name, e.g. `julian:mml-review`.
    pub name: String,
    /// Symlink-resolved, canonicalized source file.
    pub source: PathBuf,
    /// First line of the frontmatter `description:` (may be empty).
    pub description: String,
    pub tier: Tier,
    pub kind: Kind,
    /// Owning plugin's display name, when `tier == Plugin`.
    pub plugin: Option<String>,
}

#[derive(Debug, Default)]
pub struct Index {
    entries: Vec<Entry>,
    /// name → index into `entries` of the precedence winner.
    by_name: HashMap<String, usize>,
    /// Winner names in stable (sorted) order — borrowable for fuzzy ranking.
    names: Vec<String>,
}

impl Index {
    /// Build the index from `config_dir` — the Claude config directory backing the
    /// user + plugin tiers, i.e. `~/.claude` or `~/.claude-hobby`, resolved by the
    /// caller — and the editor's `workspace_roots` (the project tier). Missing
    /// directories are silently skipped.
    ///
    /// Takes the resolved directory rather than `$HOME` so that the choice between
    /// accounts is made once, in the server, and this stays a pure function of its
    /// arguments; reading the environment here would make every test depend on the
    /// account the test runner happens to be running under.
    pub fn build(config_dir: Option<&Path>, workspace_roots: &[PathBuf]) -> Index {
        let mut raw: Vec<Entry> = Vec::new();

        // Project tier first so it wins collisions after the stable sort.
        for root in workspace_roots {
            let claude = root.join(".claude");
            collect_commands(&claude.join("commands"), Tier::Project, None, &mut raw);
            collect_skills(&claude.join("skills"), Tier::Project, None, &mut raw);
        }

        if let Some(claude) = config_dir {
            collect_commands(&claude.join("commands"), Tier::User, None, &mut raw);
            collect_skills(&claude.join("skills"), Tier::User, None, &mut raw);
            collect_plugins(claude, &mut raw);
        }

        Index::from_entries(raw)
    }

    fn from_entries(mut raw: Vec<Entry>) -> Index {
        // Stable order: by name, then tier (Project < User < Plugin), so the
        // first entry seen per name is the precedence winner.
        raw.sort_by(|a, b| a.name.cmp(&b.name).then(a.tier.cmp(&b.tier)));

        let mut by_name: HashMap<String, usize> = HashMap::new();
        let mut names: Vec<String> = Vec::new();
        for (i, e) in raw.iter().enumerate() {
            if !by_name.contains_key(&e.name) {
                by_name.insert(e.name.clone(), i);
                names.push(e.name.clone());
            }
        }

        Index {
            entries: raw,
            by_name,
            names,
        }
    }

    /// Winner names, sorted, for fuzzy ranking.
    pub fn names(&self) -> &[String] {
        &self.names
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    /// The precedence-winning entry for `name`.
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.by_name.get(name).and_then(|&i| self.entries.get(i))
    }

    /// Every entry defining `name`, winner first (used for multi-location goto).
    pub fn locations(&self, name: &str) -> Vec<&Entry> {
        let winner = self.by_name.get(name).copied();
        let others = self
            .entries
            .iter()
            .enumerate()
            .filter(|&(i, e)| e.name == name && Some(i) != winner)
            .map(|(_, e)| e);
        winner
            .and_then(|w| self.entries.get(w))
            .into_iter()
            .chain(others)
            .collect()
    }
}

/// Fuzzy-rank `names` against `pattern`. An empty pattern returns every name in
/// its original order; otherwise only matches are kept, best score first.
pub fn fuzzy_rank<'a>(names: &'a [String], pattern: &str) -> Vec<&'a String> {
    if pattern.trim().is_empty() {
        return names.iter().collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pat = Pattern::parse(pattern, CaseMatching::Ignore, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(u32, &String)> = names
        .iter()
        .filter_map(|name| {
            let haystack = Utf32Str::new(name, &mut buf);
            pat.score(haystack, &mut matcher).map(|s| (s, name))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(b.1)));
    scored.into_iter().map(|(_, name)| name).collect()
}

/// The directory holding the user tier: `$CLAUDE_CONFIG_DIR` when set to something
/// non-empty, else `$HOME/.claude`.
///
/// `override_dir` is a parameter rather than an `env::var` call inside so the rule
/// stays pure and testable — the process environment is global, and a test that
/// mutated it would race every other test in the binary.
pub fn resolve_config_dir(home: &Path, override_dir: Option<&OsStr>) -> PathBuf {
    match override_dir {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => home.join(".claude"),
    }
}

/// Walk a `commands/` root, adding one `Entry` per `*.md` file.
fn collect_commands(root: &Path, tier: Tier, plugin: Option<&str>, out: &mut Vec<Entry>) {
    if !root.is_dir() {
        return;
    }
    for entry in WalkBuilder::new(root)
        .hidden(true)
        .follow_links(false)
        .max_depth(Some(MAX_DEPTH))
        .build()
    {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !is_markdown(path) || !path.is_file() {
            continue;
        }
        let Some(name) = crate::name::derive_command_name(path, root) else {
            continue;
        };
        push_entry(path, name, tier, Kind::Command, plugin, None, out);
    }
}

/// Walk a `skills/` root, adding one `Entry` per skill directory that contains
/// a skill file.
fn collect_skills(root: &Path, tier: Tier, plugin: Option<&str>, out: &mut Vec<Entry>) {
    let Ok(dirs) = std::fs::read_dir(root) else {
        return;
    };
    for dir in dirs.flatten() {
        let dir_path = dir.path();
        if !dir_path.is_dir() {
            continue;
        }
        let Some(skill_file) = find_skill_file(&dir_path) else {
            continue;
        };
        let fm = read_frontmatter(&skill_file);
        let Some(name) = crate::name::derive_skill_name(&dir_path, fm.name.as_deref()) else {
            continue;
        };
        let display = match name.split_once(':') {
            Some((_, bare)) => bare.to_string(),
            None => name.clone(),
        };
        let indexed = match plugin {
            Some(p) => format!("{p}:{display}"),
            None => name,
        };
        push_entry(
            &skill_file,
            indexed,
            tier,
            Kind::Skill,
            plugin,
            Some(fm),
            out,
        );
    }
}

#[derive(Debug, Deserialize)]
struct InstalledPlugins {
    #[serde(default)]
    plugins: HashMap<String, Vec<PluginInstall>>,
}

#[derive(Debug, Deserialize)]
struct PluginInstall {
    #[serde(rename = "installPath")]
    install_path: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PluginManifest {
    name: Option<String>,
}

/// Enumerate enabled plugins from `installed_plugins.json` and add their skills.
///
/// A plugin is treated as enabled unless `enabledPlugins` explicitly maps its
/// `<name>@<marketplace>` key to `false`. (Observed installs leave active local
/// plugins out of `enabledPlugins` entirely, so "absent" must mean enabled.)
fn collect_plugins(claude: &Path, out: &mut Vec<Entry>) {
    let registry_path = claude.join("plugins/installed_plugins.json");
    let Ok(text) = std::fs::read_to_string(&registry_path) else {
        return;
    };
    let Ok(registry) = serde_json::from_str::<InstalledPlugins>(&text) else {
        tracing::debug!(path = %registry_path.display(), "failed to parse installed_plugins.json");
        return;
    };

    let disabled = disabled_plugin_keys(claude);

    for (key, installs) in &registry.plugins {
        if disabled.contains(key.as_str()) {
            continue;
        }
        for install in installs {
            let Some(install_path) = &install.install_path else {
                continue;
            };
            let install_path = PathBuf::from(install_path);
            let plugin_name = plugin_display_name(&install_path, key);
            collect_skills(
                &install_path.join("skills"),
                Tier::Plugin,
                Some(&plugin_name),
                out,
            );
        }
    }
}

/// The set of `<name>@<marketplace>` keys explicitly disabled (`false`) in the
/// user's `settings.json` / `settings.local.json` `enabledPlugins` map.
fn disabled_plugin_keys(claude: &Path) -> std::collections::HashSet<String> {
    let mut disabled = std::collections::HashSet::new();
    for file in ["settings.json", "settings.local.json"] {
        let Ok(text) = std::fs::read_to_string(claude.join(file)) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if let Some(map) = value.get("enabledPlugins").and_then(|v| v.as_object()) {
            for (k, v) in map {
                if v.as_bool() == Some(false) {
                    disabled.insert(k.clone());
                }
            }
        }
    }
    disabled
}

/// Plugin display name = `.claude-plugin/plugin.json` `"name"`, else the part of
/// the registry key before `@`, else the whole key.
fn plugin_display_name(install_path: &Path, key: &str) -> String {
    let manifest = install_path.join(".claude-plugin/plugin.json");
    if let Ok(text) = std::fs::read_to_string(&manifest)
        && let Ok(m) = serde_json::from_str::<PluginManifest>(&text)
        && let Some(name) = m.name
    {
        let name = name.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    key.split('@').next().unwrap_or(key).to_string()
}

/// Canonicalize `path` (resolving symlinks) and push an `Entry`. Broken links —
/// where `canonicalize` fails — are skipped so goto never targets a dead path.
fn push_entry(
    path: &Path,
    name: String,
    tier: Tier,
    kind: Kind,
    plugin: Option<&str>,
    frontmatter: Option<crate::frontmatter::Frontmatter>,
    out: &mut Vec<Entry>,
) {
    let Ok(source) = std::fs::canonicalize(path) else {
        tracing::debug!(path = %path.display(), "skipping unresolvable (broken symlink?)");
        return;
    };
    let fm = frontmatter.unwrap_or_else(|| read_frontmatter(&source));
    out.push(Entry {
        name,
        source,
        description: fm.description.unwrap_or_default(),
        tier,
        kind,
        plugin: plugin.map(|s| s.to_string()),
    });
}

fn read_frontmatter(path: &Path) -> crate::frontmatter::Frontmatter {
    std::fs::read_to_string(path)
        .map(|s| crate::frontmatter::parse(&s))
        .unwrap_or_default()
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("md"))
        .unwrap_or(false)
}

/// Find a skill's content file in `dir`: a `skill.md` (any case), else a file
/// named after the directory (`<dir>.md`, the plugin convention).
fn find_skill_file(dir: &Path) -> Option<PathBuf> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut dir_named: Option<PathBuf> = None;
    let dir_stem = dir.file_name().and_then(|s| s.to_str());
    for e in entries.flatten() {
        let name = e.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.eq_ignore_ascii_case("skill.md") {
            return Some(e.path());
        }
        if let Some(stem) = dir_stem
            && name.eq_ignore_ascii_case(&format!("{stem}.md"))
        {
            dir_named = Some(e.path());
        }
    }
    dir_named
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn config_dir_defaults_to_dot_claude() {
        assert_eq!(
            resolve_config_dir(Path::new("/home/u"), None),
            PathBuf::from("/home/u/.claude")
        );
    }

    #[test]
    fn config_dir_follows_the_override() {
        assert_eq!(
            resolve_config_dir(
                Path::new("/home/u"),
                Some(OsStr::new("/home/u/.claude-hobby"))
            ),
            PathBuf::from("/home/u/.claude-hobby")
        );
    }

    /// An exported-but-empty variable is the shell's way of saying "unset", and
    /// treating it as a path would point the user tier at the process's cwd.
    #[test]
    fn config_dir_ignores_an_empty_override() {
        assert_eq!(
            resolve_config_dir(Path::new("/home/u"), Some(OsStr::new(""))),
            PathBuf::from("/home/u/.claude")
        );
    }

    #[test]
    fn fuzzy_empty_returns_all() {
        let names = vec!["a".to_string(), "b".to_string()];
        assert_eq!(fuzzy_rank(&names, "  "), names.iter().collect::<Vec<_>>());
    }

    #[test]
    fn fuzzy_filters_and_ranks() {
        let names = vec![
            "julian:mml-review".to_string(),
            "review-rust".to_string(),
            "unrelated".to_string(),
        ];
        let ranked = fuzzy_rank(&names, "mml");
        assert_eq!(
            ranked.first().map(|s| s.as_str()),
            Some("julian:mml-review")
        );
        assert!(ranked.iter().all(|s| s.as_str() != "unrelated"));
    }

    #[test]
    fn builds_user_commands_and_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        write(
            &home.join(".claude/commands/review-rust.md"),
            "---\ndescription: Review rust\n---\nbody",
        );
        write(
            &home.join(".claude/commands/julian/mml-extract.md"),
            "---\ndescription: Extract mml\n---\n",
        );
        write(
            &home.join(".claude/skills/principal-review/SKILL.md"),
            "---\nname: principal-review\ndescription: Principal review\n---\n",
        );

        let idx = Index::build(Some(&home.join(".claude")), &[]);
        assert_eq!(idx.get("review-rust").map(|e| e.kind), Some(Kind::Command));
        assert_eq!(
            idx.get("julian:mml-extract").map(|e| e.tier),
            Some(Tier::User)
        );
        let skill = idx.get("principal-review").unwrap();
        assert_eq!(skill.kind, Kind::Skill);
        assert_eq!(skill.description, "Principal review");
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_command_resolves_to_target() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        // Real source elsewhere (simulating the tooling repo).
        let real = home.join("repo/skills/spawn/skill.md");
        write(&real, "---\ndescription: Spawn a subtask\n---\n");
        // Symlinked into the commands dir.
        let link = home.join(".claude/commands/spawn.md");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(&real, &link).unwrap();

        let idx = Index::build(Some(&home.join(".claude")), &[]);
        let e = idx.get("spawn").unwrap();
        assert_eq!(e.source, fs::canonicalize(&real).unwrap());
        assert_eq!(e.description, "Spawn a subtask");
    }

    #[cfg(unix)]
    #[test]
    fn broken_symlink_is_skipped() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let link = home.join(".claude/commands/dead.md");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(home.join("does-not-exist.md"), &link).unwrap();

        let idx = Index::build(Some(&home.join(".claude")), &[]);
        assert!(idx.get("dead").is_none());
    }

    #[test]
    fn project_tier_shadows_user_tier() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let proj = tmp.path().join("proj");
        write(
            &home.join(".claude/commands/shared.md"),
            "---\ndescription: user version\n---\n",
        );
        write(
            &proj.join(".claude/commands/shared.md"),
            "---\ndescription: project version\n---\n",
        );

        let idx = Index::build(Some(&home.join(".claude")), std::slice::from_ref(&proj));
        let winner = idx.get("shared").unwrap();
        assert_eq!(winner.tier, Tier::Project);
        assert_eq!(winner.description, "project version");
        // Both tiers retained for multi-location goto.
        assert_eq!(idx.locations("shared").len(), 2);
    }

    #[test]
    fn case_insensitive_skill_file() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        // Lowercase skill.md (tooling-repo convention).
        write(
            &home.join(".claude/skills/my-skill/skill.md"),
            "---\ndescription: lower case\n---\n",
        );
        let idx = Index::build(Some(&home.join(".claude")), &[]);
        assert_eq!(idx.get("my-skill").map(|e| e.kind), Some(Kind::Skill));
    }

    #[test]
    fn enabled_plugin_skills_are_namespaced() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let plugin_path = home.join("plugins-src/my-plugin");
        write(
            &plugin_path.join(".claude-plugin/plugin.json"),
            r#"{"name":"scripter"}"#,
        );
        write(
            &plugin_path.join("skills/mcp-script/mcp-script.md"),
            "---\nname: mcp-script\ndescription: Run scripts\n---\n",
        );
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            &format!(
                r#"{{"version":2,"plugins":{{"my-plugin@local":[{{"installPath":"{}"}}]}}}}"#,
                plugin_path.display()
            ),
        );

        let idx = Index::build(Some(&home.join(".claude")), &[]);
        let e = idx.get("scripter:mcp-script").unwrap();
        assert_eq!(e.tier, Tier::Plugin);
        assert_eq!(e.plugin.as_deref(), Some("scripter"));
    }

    #[test]
    fn explicitly_disabled_plugin_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path();
        let plugin_path = home.join("plugins-src/off");
        write(
            &plugin_path.join(".claude-plugin/plugin.json"),
            r#"{"name":"off"}"#,
        );
        write(
            &plugin_path.join("skills/s/s.md"),
            "---\ndescription: x\n---\n",
        );
        write(
            &home.join(".claude/plugins/installed_plugins.json"),
            &format!(
                r#"{{"plugins":{{"off@local":[{{"installPath":"{}"}}]}}}}"#,
                plugin_path.display()
            ),
        );
        write(
            &home.join(".claude/settings.json"),
            r#"{"enabledPlugins":{"off@local":false}}"#,
        );

        let idx = Index::build(Some(&home.join(".claude")), &[]);
        assert!(idx.get("off:s").is_none());
    }
}
