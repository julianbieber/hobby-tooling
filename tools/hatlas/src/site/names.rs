//! Where each page lives in the site, and how one page names another.

use std::collections::HashSet;

const MAX_PATH: usize = 120;
const TRUNCATED: usize = 40;

/// Hands out site-relative page paths and keeps them unique, case-insensitively.
#[derive(Default)]
pub struct PageNames {
    used: HashSet<String>,
}

impl PageNames {
    /// `<crate>/<mod>/<sub>/index.html`
    pub fn module(&mut self, path: &[String]) -> String {
        self.assign(path, "index", ".html", false)
    }

    /// `<crate>/<mod>/<kind>.<Name>.html`, where `module` is the item's module path.
    pub fn item(&mut self, module: &[String], kind: &str, name: &str) -> String {
        let shown = name.strip_prefix("r#").unwrap_or(name);
        self.assign(module, &format!("{kind}.{shown}"), ".html", shown != name)
    }

    fn assign(&mut self, dirs: &[String], stem: &str, ext: &str, renamed: bool) -> String {
        let raw = format!("{}/{stem}{ext}", dirs.join("/"));
        let mut dirs: Vec<String> = dirs.iter().map(|d| sanitize(d)).collect();
        let mut stem_clean = sanitize(stem);
        let changed = dirs.iter().zip(&raw_segments(&raw)).any(|(c, r)| c != r)
            || stem_clean != stem
            || renamed;
        let join = |dirs: &[String], stem: &str| format!("{}/{stem}{ext}", dirs.join("/"));
        let mut path = join(&dirs, &stem_clean);
        if changed || path.len() > MAX_PATH || self.used.contains(&path.to_lowercase()) {
            let hash = format!("{:016x}", fnv1a(&raw));
            if path.len() > MAX_PATH {
                for d in dirs.iter_mut() {
                    d.truncate(TRUNCATED);
                }
                stem_clean.truncate(TRUNCATED);
            }
            match dirs.last_mut() {
                Some(last) if stem == "index" => last.push_str(&format!("-{hash}")),
                _ => stem_clean.push_str(&format!("-{hash}")),
            }
            path = join(&dirs, &stem_clean);
        }
        self.used.insert(path.to_lowercase());
        path
    }
}

fn raw_segments(raw: &str) -> Vec<&str> {
    raw.split('/').collect()
}

fn sanitize(segment: &str) -> String {
    segment
        .strip_prefix("r#")
        .unwrap_or(segment)
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100000001b3)
    })
}

/// The href that goes from page `from` to page `to`, both site-relative file paths.
pub fn relative(from: &str, to: &str) -> String {
    let from_dirs = dirs_of(from);
    let to_dirs = dirs_of(to);
    let common = from_dirs
        .iter()
        .zip(&to_dirs)
        .take_while(|(a, b)| a == b)
        .count();
    let mut out: Vec<&str> = vec![".."; from_dirs.len() - common];
    out.extend(to.split('/').skip(common));
    out.join("/")
}

fn dirs_of(path: &str) -> Vec<&str> {
    path.rsplit_once('/')
        .map_or_else(Vec::new, |(dirs, _)| dirs.split('/').collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    // Pins that an ordinary name is used as written, so pages are readable by path.
    #[test]
    fn clean_names_are_unchanged() {
        let mut n = PageNames::default();
        assert_eq!(
            n.item(&p(&["wusel", "trade"]), "struct", "Sun"),
            "wusel/trade/struct.Sun.html"
        );
        assert_eq!(n.module(&p(&["wusel", "trade"])), "wusel/trade/index.html");
    }

    // Pins that a raw identifier loses `r#` and gets a hash, so it cannot collide with its plain twin.
    #[test]
    fn sanitized_names_get_a_hash() {
        let mut n = PageNames::default();
        let page = n.item(&p(&["c"]), "struct", "r#type");
        assert!(
            page.starts_with("c/struct.type-") && page.ends_with(".html"),
            "{page}"
        );
    }

    // Pins that two names differing only in case do not overwrite each other on a case-insensitive disk.
    #[test]
    fn case_collision_is_disambiguated() {
        let mut n = PageNames::default();
        let a = n.item(&p(&["c"]), "struct", "Foo");
        let b = n.item(&p(&["c"]), "struct", "FOO");
        assert_ne!(a.to_lowercase(), b.to_lowercase());
    }

    // Pins the length cap, because very deep module paths otherwise exceed filesystem limits.
    #[test]
    fn long_paths_are_capped() {
        let mut n = PageNames::default();
        let deep = p(&["c", &"m".repeat(60), &"n".repeat(60)]);
        assert!(n.item(&deep, "struct", "X").len() <= 200);
    }

    // Pins relative hrefs between pages in different directories.
    #[test]
    fn relative_climbs_and_descends() {
        assert_eq!(
            relative("c/a/index.html", "c/b/struct.X.html"),
            "../b/struct.X.html"
        );
        assert_eq!(relative("index.html", "c/index.html"), "c/index.html");
        assert_eq!(
            relative("c/a/struct.X.html", "c/a/index.html"),
            "index.html"
        );
    }
}
