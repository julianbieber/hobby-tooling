//! The project list: which projects the sidebar offers, and where each one's main checkout is.

use std::path::{Path, PathBuf};

pub use hwt_core::projects_root;

/// A project that has a directory in the notes repo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// Directory name, which is also the first level of a worktree path.
    pub name: String,
    /// Where the main checkout would be (`~/fun_repos/<name>`).
    pub path: PathBuf,
    /// Whether that path holds a git checkout — what a **Pull** needs.
    pub has_checkout: bool,
}

/// Every project with a visible directory under the notes root, by name. A clone under
/// `~/fun_repos` with no notes is not listed.
pub fn scan_projects() -> Vec<Project> {
    scan_projects_at(&hwt_core::notes_root(), &projects_root())
}

fn scan_projects_at(notes: &Path, repos: &Path) -> Vec<Project> {
    let Ok(entries) = std::fs::read_dir(notes) else {
        return Vec::new();
    };
    let mut projects: Vec<Project> = entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.'))
        .map(|name| {
            let path = repos.join(&name);
            Project {
                has_checkout: path.join(".git").exists(),
                name,
                path,
            }
        })
        .collect();
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
}

#[cfg(test)]
mod tests {
    use super::*;

    // Pins that the sidebar is the notes tree, not `~/fun_repos`: an upstream clone with no
    // notes gets no row, and a notes-only project gets one without a pull target.
    #[test]
    fn projects_are_the_notes_directories() {
        let root = std::env::temp_dir().join("hdash_projects_fixture");
        let _ = std::fs::remove_dir_all(&root);
        let (notes, repos) = (root.join("notes"), root.join("repos"));
        for dir in ["wusel", "notes-only", ".git"] {
            std::fs::create_dir_all(notes.join(dir)).unwrap();
        }
        std::fs::write(notes.join("README.md"), "x").unwrap();
        for repo in ["wusel", "bevy"] {
            std::fs::create_dir_all(repos.join(repo).join(".git")).unwrap();
        }

        let projects = scan_projects_at(&notes, &repos);
        let rows: Vec<(&str, bool)> = projects
            .iter()
            .map(|p| (p.name.as_str(), p.has_checkout))
            .collect();
        assert_eq!(rows, vec![("notes-only", false), ("wusel", true)]);

        let _ = std::fs::remove_dir_all(&root);
    }
}
