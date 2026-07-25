//! `hwt_core` — shared non-UI helpers for the hobby worktree dashboard.
//!
//! Holds everything that is not Bevy-specific: well-known paths (`~/fun_worktrees`,
//! the hobby notes repo, XDG config/cache files),
//! detached process spawning (terminals, browsers, notifications), and human-friendly time
//! formatting. Logging goes through the `log` facade, which Bevy's `LogPlugin` bridges into
//! its tracing output.

use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use log::warn;

// ---------------------------------------------------------------------------
// Paths
// ---------------------------------------------------------------------------

/// The user's home directory (`$HOME`, falling back to `/`).
pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

/// `~/fun_worktrees` — hobby task worktrees, laid out `<project>/<topic>/<task>`.
/// The task directory is itself the git worktree; there is no repo level below it.
pub fn worktrees_root() -> PathBuf {
    home().join("fun_worktrees")
}

/// `~/fun_repos` — the main repo checkouts that worktrees are derived from.
pub fn projects_root() -> PathBuf {
    home().join("fun_repos")
}

/// `~/fun_repos/hobby-mimisbrunnr/notes`, laid out `<project>/<topic>/<task>`.
pub fn notes_root() -> PathBuf {
    home().join("fun_repos/hobby-mimisbrunnr/notes")
}

/// `$XDG_CACHE_HOME/<app>/<file>` (default `~/.cache/<app>/<file>`).
pub fn xdg_cache_path(app: &str, file: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".cache"));
    base.join(app).join(file)
}

/// `$XDG_CONFIG_HOME/<app>/<file>` (default `~/.config/<app>/<file>`).
pub fn xdg_config_path(app: &str, file: &str) -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"));
    base.join(app).join(file)
}

// ---------------------------------------------------------------------------
// Git
// ---------------------------------------------------------------------------

/// Run `git -C <path> <args>` and capture trimmed stdout. `None` on spawn failure, non-zero exit,
/// or empty output. Blocking; call off the main thread for anything latency-sensitive.
pub fn git_capture(path: &std::path::Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if s.is_empty() { None } else { Some(s) }
}

// ---------------------------------------------------------------------------
// Detached processes
// ---------------------------------------------------------------------------

/// Spawn a command detached from this process (via `setsid`), so terminals/browsers opened from
/// the dashboard outlive it. Failures are logged, never fatal.
pub fn spawn_detached(args: &[&str]) {
    if let Err(e) = Command::new("setsid").args(args).spawn() {
        warn!("failed to spawn `{}`: {e}", args.join(" "));
    }
}

/// Open a URL (e.g. a PR) in Firefox, detached.
pub fn open_url(url: &str) {
    spawn_detached(&["-f", "firefox", url]);
}

/// Fire a desktop notification (via `notify-send`), detached. Best-effort.
pub fn notify(summary: &str, body: &str) {
    spawn_detached(&["-f", "notify-send", summary, body]);
}

// ---------------------------------------------------------------------------
// Time formatting
// ---------------------------------------------------------------------------

/// Human-friendly "time since" string for a [`SystemTime`].
pub fn rel_time(t: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(t)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = secs / 86_400;
    match days {
        0 => "today".into(),
        1 => "yesterday".into(),
        d if d < 30 => format!("{d}d ago"),
        d if d < 365 => format!("{}mo ago", d / 30),
        d => format!("{}y ago", d / 365),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// `rel_time` buckets ages into today / yesterday / days / months / years.
    #[test]
    fn rel_time_buckets() {
        let now = SystemTime::now();
        let ago = |days: u64| now - Duration::from_secs(days * 86_400 + 60);
        assert_eq!(rel_time(now), "today");
        assert_eq!(rel_time(ago(1)), "yesterday");
        assert_eq!(rel_time(ago(5)), "5d ago");
        assert_eq!(rel_time(ago(65)), "2mo ago");
        assert_eq!(rel_time(ago(800)), "2y ago");
        // A future timestamp degrades to "today", never panics.
        assert_eq!(rel_time(now + Duration::from_secs(3600)), "today");
    }

    /// The XDG helpers honor the override env vars and fall back to `~/.cache` / `~/.config`.
    /// One test because env vars are process-global.
    #[test]
    fn xdg_paths_honor_env() {
        // SAFETY: no other test in this crate reads these vars concurrently.
        unsafe {
            std::env::set_var("XDG_CACHE_HOME", "/tmp/xdg-cache-test");
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-config-test");
        }
        assert_eq!(
            xdg_cache_path("app", "f.json"),
            PathBuf::from("/tmp/xdg-cache-test/app/f.json")
        );
        assert_eq!(
            xdg_config_path("app", "f.json"),
            PathBuf::from("/tmp/xdg-config-test/app/f.json")
        );
        unsafe {
            std::env::remove_var("XDG_CACHE_HOME");
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        assert_eq!(
            xdg_cache_path("app", "f.json"),
            home().join(".cache/app/f.json")
        );
        assert_eq!(
            xdg_config_path("app", "f.json"),
            home().join(".config/app/f.json")
        );
    }
}
