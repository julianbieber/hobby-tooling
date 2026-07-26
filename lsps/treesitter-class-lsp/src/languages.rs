use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Resolved helix runtime directory. Defaults to the path the user keeps their
/// helix checkout at; overridable via `HELIX_RUNTIME`.
pub fn helix_runtime() -> PathBuf {
    if let Ok(p) = std::env::var("HELIX_RUNTIME") {
        return PathBuf::from(p);
    }
    PathBuf::from("/home/jbieber/outside_repos/helix/runtime")
}

#[derive(Debug, Clone)]
pub struct LanguageInfo {
    /// Name as used by helix (also the grammar `.so` basename and the query
    /// directory name).
    pub name: String,
    /// Grammar shared library path (e.g. `runtime/grammars/rust.so`).
    pub grammar_path: PathBuf,
    /// Optional tags query path (`runtime/queries/<lang>/tags.scm`). Kept as a
    /// precomputed flag; the actual source is read (with inheritance expanded)
    /// via `load_tags_query`.
    #[allow(dead_code)]
    pub tags_query: Option<PathBuf>,
}

/// Registry built from helix's `languages.toml`. Maps file extensions and glob
/// patterns to the language metadata we need to parse a file.
#[derive(Debug, Clone)]
pub struct LanguageRegistry {
    by_ext: HashMap<String, LanguageInfo>,
    runtime: PathBuf,
}

impl LanguageRegistry {
    pub fn default_empty() -> Self {
        Self {
            by_ext: HashMap::new(),
            runtime: helix_runtime(),
        }
    }

    pub fn load() -> anyhow::Result<Self> {
        let runtime = helix_runtime();
        let languages_toml = runtime
            .parent()
            .map(|p| p.join("languages.toml"))
            .unwrap_or_else(|| runtime.join("../languages.toml"));
        let raw = std::fs::read_to_string(&languages_toml)
            .map_err(|e| anyhow::anyhow!("read {}: {}", languages_toml.display(), e))?;
        let doc: HelixLanguagesDoc = toml::from_str(&raw)?;

        let mut by_ext: HashMap<String, LanguageInfo> = HashMap::new();
        for lang in doc.language {
            let grammar_name = lang.grammar.as_deref().unwrap_or(&lang.name).to_string();
            let grammar_path = runtime.join("grammars").join(format!("{grammar_name}.so"));
            if !grammar_path.exists() {
                continue;
            }
            let tags_query = {
                let p = runtime.join("queries").join(&lang.name).join("tags.scm");
                if p.exists() { Some(p) } else { None }
            };
            let info = LanguageInfo {
                name: lang.name.clone(),
                grammar_path,
                tags_query,
            };
            for ft in &lang.file_types {
                if let Some(ext) = ft.as_extension() {
                    by_ext.entry(ext.to_ascii_lowercase()).or_insert_with(|| info.clone());
                }
            }
        }

        Ok(Self { by_ext, runtime })
    }

    pub fn runtime(&self) -> &Path {
        &self.runtime
    }

    pub fn resolve(&self, path: &Path) -> Option<&LanguageInfo> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        self.by_ext.get(&ext)
    }

    /// The tree-sitter symbol name for a grammar, e.g. `c-sharp` → `tree_sitter_c_sharp`.
    pub fn symbol_name(&self, lang: &LanguageInfo) -> String {
        let grammar = lang
            .grammar_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&lang.name);
        format!("tree_sitter_{}", grammar.replace('-', "_"))
    }
}

#[derive(Debug, Deserialize)]
struct HelixLanguagesDoc {
    #[serde(default)]
    language: Vec<HelixLanguage>,
}

#[derive(Debug, Deserialize)]
struct HelixLanguage {
    name: String,
    /// Optional override; when unset helix uses `name` as the grammar name.
    #[serde(default)]
    grammar: Option<String>,
    #[serde(rename = "file-types", default)]
    file_types: Vec<FileType>,
}

/// helix's `file-types` can be a bare string (`"rs"`) or an inline table
/// (`{ glob = "Makefile" }` / `{ suffix = ".mk" }`). We only care about
/// extensions.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum FileType {
    Ext(String),
    Obj(FileTypeObj),
}

#[derive(Debug, Deserialize)]
struct FileTypeObj {
    #[serde(default)]
    suffix: Option<String>,
    #[serde(default)]
    glob: Option<String>,
}

impl FileType {
    fn as_extension(&self) -> Option<String> {
        match self {
            FileType::Ext(s) => Some(s.trim_start_matches('.').to_string()),
            FileType::Obj(o) => {
                if let Some(suf) = &o.suffix {
                    let trimmed = suf.trim_start_matches('.');
                    if !trimmed.is_empty() {
                        return Some(trimmed.to_string());
                    }
                }
                // Glob-based matches (e.g. Makefile) are out of scope for MVP.
                if let Some(g) = &o.glob
                    && let Some(dot) = g.rfind('.')
                {
                    let ext = &g[dot + 1..];
                    if !ext.contains('*') && !ext.is_empty() {
                        return Some(ext.to_string());
                    }
                }
                None
            }
        }
    }
}

/// Built-in tags fallbacks for languages helix doesn't ship a `tags.scm` for
/// (notably Scala). Only covers class-like definitions.
fn builtin_tags(lang: &str) -> Option<&'static str> {
    match lang {
        "scala" => Some(include_str!("builtin_queries/scala_tags.scm")),
        _ => None,
    }
}

/// Load a tags query, resolving `; inherits: a,b,c` directives by concatenating
/// the referenced query files. Falls back to a bundled query for languages
/// helix doesn't ship tags for. Some helix queries (e.g. tsx) are pure inherit
/// stubs; we still expand those via inheritance.
pub fn load_tags_query(runtime: &Path, lang: &str) -> Option<String> {
    let path = runtime.join("queries").join(lang).join("tags.scm");
    let raw = std::fs::read_to_string(&path).ok();
    let raw = match raw {
        Some(r) => r,
        None => return builtin_tags(lang).map(str::to_string),
    };
    let mut visited = std::collections::HashSet::new();
    visited.insert(lang.to_string());
    let mut out = String::new();
    expand_inherits(&raw, runtime, &mut visited, &mut out);
    if out.trim().is_empty() {
        builtin_tags(lang).map(str::to_string)
    } else {
        Some(out)
    }
}

fn expand_inherits(
    source: &str,
    runtime: &Path,
    visited: &mut std::collections::HashSet<String>,
    out: &mut String,
) {
    // First line(s) may contain `; inherits: a,b,c`. Helix scans the whole file
    // for such directives, not just the first line.
    for line in source.lines() {
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("; inherits:") {
            for name in rest.split(',') {
                let name = name.trim();
                if name.is_empty() || !visited.insert(name.to_string()) {
                    continue;
                }
                let p = runtime.join("queries").join(name).join("tags.scm");
                if let Ok(inherited) = std::fs::read_to_string(&p) {
                    expand_inherits(&inherited, runtime, visited, out);
                }
            }
        }
    }
    out.push_str(source);
    if !source.ends_with('\n') {
        out.push('\n');
    }
}
