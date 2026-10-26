//! The hard check that every link in a written site lands on a page that exists.

use crate::Error;
use std::path::{Component, Path, PathBuf};

/// Scans every `.html` under `root` for `href="…"` and `src="…"`, and `graph-data.js`, when
/// present, for each node's `data.href`.
///
/// Skips `http(s)` links. A relative link must resolve to an existing file inside
/// `root`, and a `#fragment` needs a matching `id="…"` in that file. A `graph-data.js` that
/// does not parse, or a node without an `href`, is a `DanglingLink` too. The first miss
/// is a `DanglingLink`.
pub fn check(root: &Path) -> Result<(), Error> {
    let mut pages = Vec::new();
    collect(root, &mut pages)?;
    for page in &pages {
        let text = read(page)?;
        for link in attrs(&text, "href=\"").into_iter().chain(attrs(&text, "src=\"")) {
            resolve(root, page, link)?;
        }
    }
    let graph = root.join("graph-data.js");
    if graph.is_file() {
        let index = root.join("index.html");
        for link in graph_hrefs(&graph)? {
            resolve(root, &index, &link)?;
        }
    }
    Ok(())
}

fn graph_hrefs(graph: &Path) -> Result<Vec<String>, Error> {
    let bad = |what: &str| Error::DanglingLink { page: graph.to_path_buf(), link: what.to_string() };
    let text = read(graph)?;
    let json = text
        .trim()
        .strip_prefix("window.HATLAS_GRAPH = ")
        .map(|t| t.trim_end_matches(';'))
        .ok_or_else(|| bad("graph-data.js does not start with window.HATLAS_GRAPH ="))?;
    let value: serde_json::Value = serde_json::from_str(json).map_err(|_| bad("graph-data.js is not valid JSON"))?;
    let nodes = value["nodes"].as_array().ok_or_else(|| bad("graph-data.js has no nodes"))?;
    nodes
        .iter()
        .map(|n| n["data"]["href"].as_str().map(str::to_string).ok_or_else(|| bad("a node has no href")))
        .collect()
}

fn resolve(root: &Path, page: &Path, link: &str) -> Result<(), Error> {
    if link.starts_with("http://") || link.starts_with("https://") {
        return Ok(());
    }
    let (file, frag) = match link.split_once('#') {
        Some((f, frag)) => (f, Some(frag)),
        None => (link, None),
    };
    let target = if file.is_empty() {
        page.to_path_buf()
    } else {
        normalize(&page.parent().unwrap_or(root).join(file))
    };
    let dangling = || Error::DanglingLink { page: page.to_path_buf(), link: link.to_string() };
    if !target.starts_with(root) || !target.is_file() {
        return Err(dangling());
    }
    if let Some(frag) = frag.filter(|f| !f.is_empty())
        && !read(&target)?.contains(&format!("id=\"{frag}\""))
    {
        return Err(dangling());
    }
    Ok(())
}

fn read(path: &Path) -> Result<String, Error> {
    std::fs::read_to_string(path).map_err(|source| Error::Io { path: path.to_path_buf(), source })
}

fn attrs<'t>(text: &'t str, prefix: &str) -> Vec<&'t str> {
    text.split(prefix)
        .skip(1)
        .filter_map(|rest| rest.split_once('"').map(|(link, _)| link))
        .collect()
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Error> {
    let io = |source| Error::Io { path: dir.to_path_buf(), source };
    for entry in std::fs::read_dir(dir).map_err(io)? {
        let path = entry.map_err(io)?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "html") {
            out.push(path);
        }
    }
    Ok(())
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn site(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("hatlas-check-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (rel, body) in files {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, body).unwrap();
        }
        root
    }

    // Pins that a link to a missing page fails the run instead of shipping a dead link.
    #[test]
    fn missing_page_is_dangling() {
        let root = site("page", &[("index.html", "<a href=\"gone.html\">x</a>")]);
        assert!(matches!(check(&root), Err(Error::DanglingLink { .. })));
    }

    // Pins that a fragment must exist in its target, not only the file.
    #[test]
    fn missing_anchor_is_dangling() {
        let root = site(
            "anchor",
            &[("index.html", "<a href=\"a/index.html#x\">x</a>"), ("a/index.html", "<p id=\"y\">")],
        );
        assert!(matches!(check(&root), Err(Error::DanglingLink { .. })));
    }

    // Pins that links across directories and to anchors that exist pass, and http links are ignored.
    #[test]
    fn valid_links_pass() {
        let root = site(
            "ok",
            &[
                ("index.html", "<a href=\"a/index.html#x\">x</a><a href=\"https://e.com\">e</a>"),
                ("a/index.html", "<p id=\"x\"><a href=\"../index.html\">up</a><a href=\"#x\">me</a>"),
            ],
        );
        assert!(check(&root).is_ok());
    }

    // Pins that a link climbing out of the site is rejected even when the target exists.
    #[test]
    fn escaping_the_site_is_dangling() {
        let root = site("escape", &[("index.html", "<a href=\"../../../../etc/passwd\">x</a>")]);
        assert!(matches!(check(&root), Err(Error::DanglingLink { .. })));
    }

    // Pins that a graph node whose page is missing fails the run, so the overview never ships a dead node.
    #[test]
    fn graph_node_to_missing_page_is_dangling() {
        let root = site(
            "graph-missing",
            &[
                ("index.html", "<p>"),
                ("graph-data.js", "window.HATLAS_GRAPH = {\"nodes\":[{\"data\":{\"href\":\"gone.html\"}}]};\n"),
            ],
        );
        assert!(matches!(check(&root), Err(Error::DanglingLink { .. })));
    }

    // Pins that graph nodes linking to existing pages pass.
    #[test]
    fn graph_node_to_existing_page_passes() {
        let root = site(
            "graph-ok",
            &[
                ("index.html", "<p>"),
                ("a/p.html", "<p>"),
                ("graph-data.js", "window.HATLAS_GRAPH = {\"nodes\":[{\"data\":{\"href\":\"a/p.html\"}}]};\n"),
            ],
        );
        assert!(check(&root).is_ok());
    }

    // Pins that a script whose src is missing fails, since the graph would silently not load.
    #[test]
    fn missing_script_is_dangling() {
        let root = site("script", &[("index.html", "<script src=\"assets/x.js\"></script>")]);
        assert!(matches!(check(&root), Err(Error::DanglingLink { .. })));
    }
}
