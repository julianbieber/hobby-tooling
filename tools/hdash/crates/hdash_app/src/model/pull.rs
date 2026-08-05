//! `git pull` for a project's main checkout — the one action in this app that changes a
//! repository rather than reading one.
//!
//! Three things make it safe to run from a background thread with nobody watching:
//!
//! - **`--ff-only`.** A diverged or dirty checkout fails with a message instead of writing a
//!   merge commit into a repo the user was not looking at, or dropping them into an editor
//!   that has no terminal attached.
//! - **No prompting.** `GIT_TERMINAL_PROMPT=0` and ssh's `BatchMode` turn a credential
//!   prompt into an error. `hdash` is usually launched from a terminal, so it *has* a tty:
//!   without these, an ssh passphrase prompt would write into that terminal invisibly and
//!   hang an [`IoTaskPool`](bevy::tasks::IoTaskPool) thread for the rest of the session.
//! - **HEAD is compared, not parsed.** `git pull`'s human output differs across versions and
//!   configurations; the before/after SHA and a `rev-list --count` between them do not.

use std::path::Path;
use std::process::Command;

use hwt_core::{first_stderr_line, git_capture};

/// What a successful pull did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullOutcome {
    /// The checkout already matched the remote — nothing moved.
    UpToDate,
    /// Fast-forwarded by this many commits. `0` means HEAD moved but the count could not be
    /// read, which is reported as a bare "pulled" rather than a misleading "0 commits".
    Advanced(usize),
}

impl PullOutcome {
    /// One-line summary for the status strip.
    pub fn summary(self) -> String {
        match self {
            PullOutcome::UpToDate => "already up to date".to_owned(),
            PullOutcome::Advanced(0) => "pulled".to_owned(),
            PullOutcome::Advanced(1) => "pulled 1 commit".to_owned(),
            PullOutcome::Advanced(n) => format!("pulled {n} commits"),
        }
    }
}

/// A pull result for one project, delivered from the async job over a channel. Mirrors
/// [`super::issue::IssueMsg`].
pub struct PullMsg {
    /// Project name the result belongs to.
    pub project: String,
    /// `Ok(outcome)` or `Err(one-line reason)`.
    pub result: Result<PullOutcome, String>,
}

/// Fast-forward a checkout to its upstream. Blocking; call off the main thread.
///
/// Every failure — no upstream, no remote, diverged history, local changes in the way, a
/// credential prompt refused — comes back as `Err(reason)` for the project's status line.
/// Nothing here can abort the app.
pub fn pull(path: &Path) -> Result<PullOutcome, String> {
    let before = git_capture(path, &["rev-parse", "HEAD"]);

    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["pull", "--ff-only"])
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_SSH_COMMAND", "ssh -o BatchMode=yes")
        .output()
        .map_err(|e| format!("git not runnable: {e}"))?;
    if !out.status.success() {
        return Err(first_stderr_line(&String::from_utf8_lossy(&out.stderr)));
    }

    let after = git_capture(path, &["rev-parse", "HEAD"]);
    match (before, after) {
        (Some(before), Some(after)) if before != after => {
            let range = format!("{before}..{after}");
            let count = git_capture(path, &["rev-list", "--count", &range])
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            Ok(PullOutcome::Advanced(count))
        }
        // Same SHA, or an unreadable one (an empty repo has no HEAD): nothing to report.
        _ => Ok(PullOutcome::UpToDate),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// The wording every outcome gets in the status strip, including the two shapes that are
    /// not a plain count: nothing fetched, and a moved HEAD with no readable count.
    #[test]
    fn outcomes_read_as_sentences() {
        assert_eq!(PullOutcome::UpToDate.summary(), "already up to date");
        assert_eq!(PullOutcome::Advanced(0).summary(), "pulled");
        assert_eq!(PullOutcome::Advanced(1).summary(), "pulled 1 commit");
        assert_eq!(PullOutcome::Advanced(3).summary(), "pulled 3 commits");
    }

    /// Run git in `dir` with an identity of our own, so the test does not depend on (or
    /// disturb) whatever the machine's global config says.
    fn git(dir: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args([
                "-c",
                "user.name=hdash test",
                "-c",
                "user.email=hdash@example.invalid",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .output()
            .expect("git runnable");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(dir: &Path, file: &str) {
        std::fs::write(dir.join(file), file).unwrap();
        git(dir, &["add", file]);
        git(dir, &["commit", "-m", file]);
    }

    /// The whole pull path against a real local remote: a fast-forward reports how many
    /// commits arrived, a second pull reports nothing moved, and a diverged checkout is
    /// **refused** with git's own one-line reason rather than merged behind the user's back.
    ///
    /// Uses a `file://`-style local path, so it needs git but no network.
    #[test]
    fn pull_fast_forwards_and_refuses_to_merge() {
        let root: PathBuf = std::env::temp_dir().join("hdash_pull_fixture");
        let _ = std::fs::remove_dir_all(&root);
        let origin = root.join("origin");
        let clone = root.join("clone");
        std::fs::create_dir_all(&origin).unwrap();

        git(&origin, &["init", "--initial-branch=main"]);
        commit(&origin, "one");
        git(
            &root,
            &["clone", &origin.to_string_lossy(), &clone.to_string_lossy()],
        );

        // Nothing new upstream yet.
        assert_eq!(pull(&clone), Ok(PullOutcome::UpToDate));

        // Two new commits upstream arrive as a fast-forward, counted.
        commit(&origin, "two");
        commit(&origin, "three");
        assert_eq!(pull(&clone), Ok(PullOutcome::Advanced(2)));
        assert!(clone.join("three").exists(), "the working tree moved too");
        assert_eq!(pull(&clone), Ok(PullOutcome::UpToDate), "and stays put");

        // Diverged: a local commit here, another one upstream. `--ff-only` is what keeps this
        // from silently becoming a merge commit in a repo nobody is looking at.
        commit(&clone, "local");
        commit(&origin, "four");
        let err = pull(&clone).expect_err("a diverged checkout must not be merged");
        assert!(
            err.contains("fast-forward"),
            "git's own reason, one line: {err}"
        );
        assert!(!err.contains('\n'), "one line only: {err}");

        // A directory that is not a repo at all fails the same way — a line, never a panic.
        assert!(pull(&root).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
