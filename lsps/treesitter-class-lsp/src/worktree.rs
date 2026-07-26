use std::path::{Path, PathBuf};

use regex::Regex;

/// Given a starting path (usually the LSP's cwd or the client's workspace root),
/// walk upward and look for `.../worktrees/<topic>/<task>/...`. Return the
/// `<topic>/<task>` directory. If the path is outside `~/worktrees/`, fall back
/// to `start` itself.
pub fn detect_task_root(start: &Path) -> PathBuf {
    let s = start.to_string_lossy();
    let re = Regex::new(r"^(?P<root>.*/worktrees/[^/]+/[^/]+)(/|$)").expect("valid regex");
    if let Some(caps) = re.captures(&s) {
        return PathBuf::from(&caps["root"]);
    }
    start.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_task_root_from_nested_path() {
        let got = detect_task_root(Path::new(
            "/home/x/worktrees/topic-a/task-b/some-repo/src/main.rs",
        ));
        assert_eq!(
            got,
            PathBuf::from("/home/x/worktrees/topic-a/task-b")
        );
    }

    #[test]
    fn handles_task_root_as_start() {
        let got = detect_task_root(Path::new("/home/x/worktrees/topic/task"));
        assert_eq!(got, PathBuf::from("/home/x/worktrees/topic/task"));
    }

    #[test]
    fn falls_back_when_outside_worktrees() {
        let got = detect_task_root(Path::new("/tmp/somewhere"));
        assert_eq!(got, PathBuf::from("/tmp/somewhere"));
    }
}
