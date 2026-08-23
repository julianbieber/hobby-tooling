//! `hwt_core` — shared non-UI helpers for the hobby worktree dashboard.
//!
//! Holds everything that is not Bevy-specific: well-known paths (`~/fun_worktrees`,
//! the hobby notes repo, XDG config/cache files),
//! detached process spawning (terminals, browsers, notifications), and human-friendly time
//! formatting. Logging goes through the `log` facade, which Bevy's `LogPlugin` bridges into
//! its tracing output.

use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

/// One short line saying *why* a command failed, from its stderr — enough for a list row,
/// without a paragraph of configuration advice wrapped around it.
///
/// A line marked `error:`/`fatal:` wins **wherever it sits**, not just first. `git` narrates
/// its progress on stderr, so a refused `pull --ff-only` puts the reason under two lines of
/// `From <remote>`, and taking the first line would report the remote as the problem. Only
/// when nothing is marked — `gh`'s "none of the git remotes point to a known GitHub host" —
/// is the first line the reason.
///
/// The cap is what keeps a failure to one row at the window's default width; `gh` happily
/// emits a 200-character sentence telling you how to register a GitHub host, and `git`
/// follows its reason with a hint block.
pub fn first_stderr_line(stderr: &str) -> String {
    const MAX: usize = 80;
    const MARKERS: [&str; 2] = ["error: ", "fatal: "];

    let lines = || stderr.lines().map(str::trim).filter(|l| !l.is_empty());
    let line = lines()
        .find_map(|l| MARKERS.iter().find_map(|m| l.strip_prefix(m)))
        .or_else(|| lines().next())
        .unwrap_or("command failed");
    // Advice sentences follow the actual reason; drop them at the first sentence end.
    let line = match line.split_once(". ") {
        Some((first, _)) => first,
        None => line,
    };
    if line.chars().count() > MAX {
        format!("{}…", line.chars().take(MAX - 1).collect::<String>())
    } else {
        line.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Detached processes
// ---------------------------------------------------------------------------

/// Zellij advertises the pane it started a process in through these. A dashboard launched from
/// a zellij pane inherits them, and would pass them on to every terminal it opens.
const ZELLIJ_PANE_ENV: [&str; 3] = ["ZELLIJ", "ZELLIJ_SESSION_NAME", "ZELLIJ_PANE_ID"];

/// Spawn a command detached from this process (via `setsid`), so terminals/browsers opened from
/// the dashboard outlive it. Failures are logged, never fatal.
///
/// The child does not inherit [`ZELLIJ_PANE_ENV`]. Detaching from the process is not enough when
/// the process is itself running in a zellij pane: the terminal would start believing it is
/// already inside that session, and `hcdev` would then ask zellij to attach a session to itself —
/// which zellij answers with a panic, so the new terminal vanished the moment it appeared.
pub fn spawn_detached(args: &[&str]) {
    let mut cmd = Command::new("setsid");
    cmd.args(args);
    for var in ZELLIJ_PANE_ENV {
        cmd.env_remove(var);
    }
    if let Err(e) = cmd.spawn() {
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
// Dates
// ---------------------------------------------------------------------------

/// Parse a `YYYY-MM-DD` string into `(year, month, day)`. Strict: trailing text fails.
pub fn parse_ymd(s: &str) -> Option<(i64, i64, i64)> {
    let mut parts = s.trim().splitn(3, '-');
    let y = parts.next()?.parse().ok()?;
    let m = parts.next()?.parse().ok()?;
    let d = parts.next()?.parse().ok()?;
    Some((y, m, d))
}

/// Days since the Unix epoch (1970-01-01) for a civil date. Howard Hinnant's algorithm.
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Parse a UTC RFC 3339 timestamp of the shape `gh` emits (`2026-07-26T20:44:11Z`).
///
/// Deliberately loose about the tail: fractional seconds and a numeric offset are
/// accepted and then ignored, because every consumer of this feeds the result to
/// [`rel_time`] and a few hours' skew cannot change a "3d ago" label. Returns `None`
/// for a malformed date part or a pre-epoch timestamp.
pub fn parse_rfc3339_utc(s: &str) -> Option<SystemTime> {
    let (date, rest) = s.trim().split_once('T')?;
    let (y, m, d) = parse_ymd(date)?;
    // Strip the zone designator, then anything from a numeric offset onwards.
    let time = rest.trim_end_matches('Z');
    let time = time.split(['+', '-']).next().unwrap_or(time);
    let mut parts = time.split(':');
    let hour: i64 = parts.next()?.parse().ok()?;
    let min: i64 = parts.next().unwrap_or("0").parse().ok()?;
    // Seconds may carry a fraction (`11.523`); truncate it.
    let sec: i64 = parts
        .next()
        .unwrap_or("0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .ok()?;
    let secs = days_from_civil(y, m, d) * 86_400 + hour * 3_600 + min * 60 + sec;
    u64::try_from(secs)
        .ok()
        .map(|s| UNIX_EPOCH + Duration::from_secs(s))
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

    #[test]
    fn dates_parse_and_convert() {
        assert_eq!(parse_ymd("2026-07-26"), Some((2026, 7, 26)));
        assert_eq!(parse_ymd("  2026-07-26  "), Some((2026, 7, 26)));
        assert_eq!(
            parse_ymd("2026-07-26T20:44:11Z"),
            None,
            "strict: no trailing text"
        );
        assert_eq!(parse_ymd("nope"), None);

        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);

        let t = parse_rfc3339_utc("2026-07-26T20:44:11Z").expect("gh's shape parses");
        let secs = t.duration_since(UNIX_EPOCH).unwrap().as_secs();
        // `date -u -d '2026-07-26T20:44:11Z' +%s`
        assert_eq!(secs, 1_785_098_651);
        // The tail is ignored, so these all land on the same instant as the bare form.
        for s in [
            "2026-07-26T20:44:11.523Z",
            "2026-07-26T20:44:11+02:00",
            "2026-07-26T20:44:11",
        ] {
            assert_eq!(parse_rfc3339_utc(s), Some(t), "tolerates the tail of {s}");
        }
        assert_eq!(
            parse_rfc3339_utc("2026-07-26"),
            None,
            "date alone is not a timestamp"
        );
        assert_eq!(parse_rfc3339_utc("1969-01-01T00:00:00Z"), None, "pre-epoch");
    }

    #[test]
    fn stderr_shrinks_to_one_line() {
        assert_eq!(
            first_stderr_line("\nerror: could not determine base repo\nmore detail\n"),
            "could not determine base repo"
        );
        // The real codeberg case: reason, then two sentences of advice.
        assert_eq!(
            first_stderr_line(
                "none of the git remotes point to a known GitHub host. To tell gh about a \
                 new GitHub host, please use `gh auth login`"
            ),
            "none of the git remotes point to a known GitHub host"
        );
        // git's diverged-checkout answer to `pull --ff-only`: two lines of fetch progress
        // first, then the reason. Taking line one would have blamed the remote.
        assert_eq!(
            first_stderr_line(
                "From /tmp/origin\n * branch  HEAD -> FETCH_HEAD\n\
                 fatal: Not possible to fast-forward, aborting.\nhint: ...\n"
            ),
            "Not possible to fast-forward, aborting."
        );
        assert_eq!(first_stderr_line(""), "command failed");
        let long = first_stderr_line(&"x".repeat(300));
        assert_eq!(long.chars().count(), 80, "capped, with an ellipsis");
        assert!(long.ends_with('…'));
    }

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
