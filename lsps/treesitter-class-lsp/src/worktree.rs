//! Widening a client's workspace root to the task directory that holds it.

use std::path::{Component, Path, PathBuf};

/// Given a starting path (usually the LSP's cwd or the client's workspace root),
/// walk upward and look for `.../worktrees/<topic>/<task>/...`. Return the
/// `<topic>/<task>` directory. If the path is outside `~/worktrees/`, fall back
/// to `start` itself.
pub fn detect_task_root(start: &Path) -> PathBuf {
    let components: Vec<Component> = start.components().collect();
    let worktrees = components.windows(3).rposition(|window| {
        matches!(
            window,
            [Component::Normal(dir), Component::Normal(_), Component::Normal(_)] if *dir == "worktrees"
        )
    });
    match worktrees {
        Some(at) => components.iter().take(at + 3).collect(),
        None => start.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_task_root_from_nested_path() {
        let got = detect_task_root(Path::new(
            "/home/x/worktrees/topic-a/task-b/some-repo/src/main.rs",
        ));
        assert_eq!(got, PathBuf::from("/home/x/worktrees/topic-a/task-b"));
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
