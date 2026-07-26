//! Deriving a Claude Code invocable name from an on-disk path.
//!
//! Commands: the name is the path relative to the `commands/` root with the
//! `.md` extension stripped and each directory separator turned into `:`, so
//! `commands/julian/mml-extract.md` → `julian:mml-extract` and
//! `commands/review-rust.md` → `review-rust`.
//!
//! Skills: the name is the frontmatter `name:` field when present, otherwise
//! the skill's directory name.

use std::path::Path;

/// Derive a command's invocable name from its file `path` relative to
/// `commands_root`. Returns `None` when `path` is not `.md` or is not under
/// `commands_root`.
pub fn derive_command_name(path: &Path, commands_root: &Path) -> Option<String> {
    let rel = path.strip_prefix(commands_root).ok()?;
    let with_ext = rel.to_str()?;
    let stem = with_ext.strip_suffix(".md").or_else(|| {
        // Be tolerant of an uppercase extension.
        with_ext
            .strip_suffix(".MD")
            .or_else(|| with_ext.strip_suffix(".Md"))
    })?;
    if stem.is_empty() {
        return None;
    }
    // Normalise separators: on any platform `MAIN_SEPARATOR` plus the portable
    // `/` both map to the `:` namespace separator.
    let name: String = stem.replace([std::path::MAIN_SEPARATOR, '/'], ":");
    Some(name)
}

/// Derive a skill's invocable name: the frontmatter `name:` if present and
/// non-empty, otherwise the directory name.
pub fn derive_skill_name(skill_dir: &Path, frontmatter_name: Option<&str>) -> Option<String> {
    if let Some(n) = frontmatter_name {
        let n = n.trim();
        if !n.is_empty() {
            return Some(n.to_string());
        }
    }
    skill_dir
        .file_name()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn cmd(root: &str, path: &str) -> Option<String> {
        derive_command_name(&PathBuf::from(path), &PathBuf::from(root))
    }

    #[test]
    fn flat_command_is_bare_name() {
        assert_eq!(
            cmd(
                "/home/u/.claude/commands",
                "/home/u/.claude/commands/review-rust.md"
            ),
            Some("review-rust".to_string())
        );
    }

    #[test]
    fn one_subdir_is_namespaced() {
        assert_eq!(
            cmd(
                "/home/u/.claude/commands",
                "/home/u/.claude/commands/julian/mml-extract.md"
            ),
            Some("julian:mml-extract".to_string())
        );
    }

    #[test]
    fn tooling_install_namespaced() {
        assert_eq!(
            cmd(
                "/home/u/.claude/commands",
                "/home/u/.claude/commands/tooling/install.md"
            ),
            Some("tooling:install".to_string())
        );
    }

    #[test]
    fn nested_deeper_joins_all_segments() {
        assert_eq!(cmd("/c", "/c/a/b/c.md"), Some("a:b:c".to_string()));
    }

    #[test]
    fn non_md_is_rejected() {
        assert_eq!(cmd("/c", "/c/notes.txt"), None);
    }

    #[test]
    fn outside_root_is_rejected() {
        assert_eq!(cmd("/c", "/other/x.md"), None);
    }

    #[test]
    fn skill_name_prefers_frontmatter() {
        assert_eq!(
            derive_skill_name(
                &PathBuf::from("/s/code-quality-review"),
                Some("code-quality-review")
            ),
            Some("code-quality-review".to_string())
        );
    }

    #[test]
    fn skill_name_falls_back_to_dir() {
        assert_eq!(
            derive_skill_name(&PathBuf::from("/s/principal-review"), None),
            Some("principal-review".to_string())
        );
        // Empty / whitespace frontmatter name → dir name.
        assert_eq!(
            derive_skill_name(&PathBuf::from("/s/principal-review"), Some("   ")),
            Some("principal-review".to_string())
        );
    }
}
