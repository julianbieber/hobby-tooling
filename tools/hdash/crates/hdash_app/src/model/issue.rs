//! The issue half of the model: the project list, the `gh issue list` read, and the
//! `topic:` tag convention. No Bevy UI here, and — unlike the pre-merge `hgh-issues` —
//! **no filesystem access**: matching an issue to the worktree it spawned is
//! [`super::join`]'s job, because the task half already holds that tree.
//!
//! The launch story hangs off one convention: **an issue names its topic with a
//! `topic:<name>` label**. Topic is the middle level of
//! `~/fun_worktrees/PROJECT/TOPIC/TASK`, and it is the one piece of the worktree path
//! that cannot be derived from the issue itself — the project comes from the repo the
//! issue lives in and the task name from its number and title. So the label is what
//! makes "turn this issue into a dev session" a single click.

use std::path::PathBuf;
use std::process::Command;
use std::time::SystemTime;

use hwt_core::parse_rfc3339_utc;
use serde::Deserialize;

pub use hwt_core::projects_root;

/// The label prefix that encodes a task topic (`topic:features` → `features`).
///
/// Prefixed rather than bare: a repo's ordinary labels (`bug`, `enhancement`, `good
/// first issue`) must never be read as topics, and the prefix also lets an issue name
/// a topic whose directory does not exist yet — which is the normal case for a new
/// issue, since the topic dir only appears with the first worktree under it.
pub const TOPIC_PREFIX: &str = "topic:";

/// How many issues to ask `gh` for per project. Well past any hobby repo's open count;
/// the cap only exists so a mis-pointed remote cannot stall the fetch.
const ISSUE_LIMIT: &str = "200";

/// The `gh issue list --json` field set this app reads.
const GH_FIELDS: &str = "number,title,url,labels,updatedAt,author";

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// A hobby project: a git checkout directly under `~/fun_repos`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    /// Directory name, which is also the first level of a worktree path.
    pub name: String,
    /// Absolute path to the main checkout (`~/fun_repos/<name>`).
    pub path: PathBuf,
}

/// Where a project's issue fetch has got to. Separating "not queried yet" from
/// "queried and failed" is what lets the list say *why* a project shows nothing.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum FetchState {
    /// Not dispatched yet, or dispatched and still running.
    #[default]
    Loading,
    /// `gh` answered; `issues` is authoritative (possibly empty).
    Ready,
    /// `gh` failed — not a GitHub remote, issues disabled, not authenticated. Carries a
    /// one-line reason for the card, never a panic.
    Failed(String),
}

/// One open GitHub issue, plus everything derived from it that the UI needs.
#[derive(Clone, Debug)]
pub struct Issue {
    /// Issue number.
    pub number: u64,
    /// Issue title.
    pub title: String,
    /// Web URL, for the browser button.
    pub url: String,
    /// Every label name, in the order `gh` returned them.
    pub labels: Vec<String>,
    /// Last-updated timestamp, if it parsed.
    pub updated: Option<SystemTime>,
    /// Author login, if `gh` reported one.
    pub author: Option<String>,
    /// Topic from the `topic:` label, if the issue carries a usable one.
    pub topic: Option<String>,
    /// Task/branch name this issue would get: `gh-<number>-<slug>`.
    pub task: String,
}

impl Issue {
    /// Labels other than the topic tag, for the card's label line.
    pub fn other_labels(&self) -> impl Iterator<Item = &String> {
        self.labels.iter().filter(|l| !l.starts_with(TOPIC_PREFIX))
    }
}

/// A project and its issue fetch.
#[derive(Clone, Debug)]
pub struct ProjectIssues {
    /// The project.
    pub project: Project,
    /// Fetch progress/outcome.
    pub state: FetchState,
    /// Open issues. Empty while loading or on failure.
    pub issues: Vec<Issue>,
}

/// A fetch result for one project, delivered from the async query over a channel.
pub struct IssueMsg {
    /// Project name the result belongs to.
    pub project: String,
    /// `Ok(issues)` or `Err(one-line reason)`.
    pub result: Result<Vec<Issue>, String>,
}

// ---------------------------------------------------------------------------
// The tag convention
// ---------------------------------------------------------------------------

/// The topic a set of labels declares, or `None`.
///
/// The first `topic:`-prefixed label wins. The value must look like a directory name
/// (`[A-Za-z0-9._-]+`, no leading dot): it becomes a path component and is interpolated
/// into a shell command, so anything else is rejected outright rather than escaped —
/// which also keeps a hand-typed `topic: features` (with a space) from creating a
/// directory nobody wanted.
pub fn topic_of<'a>(labels: impl IntoIterator<Item = &'a String>) -> Option<String> {
    labels.into_iter().find_map(|l| {
        let topic = l.strip_prefix(TOPIC_PREFIX)?;
        is_safe_topic(topic).then(|| topic.to_owned())
    })
}

/// Whether a topic value is usable as a directory name and as a shell argument.
fn is_safe_topic(topic: &str) -> bool {
    !topic.is_empty()
        && !topic.starts_with('.')
        && topic
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The task/branch name for an issue: `gh-<number>-<slug>`.
///
/// Matches what `/gh-issue-create` suggests, so an issue opened through the skill and a
/// task started from this app agree on the name. The slug lowercases the title, turns
/// every run of non-alphanumerics into a single `-`, and truncates at a `-` boundary so
/// a long title cannot produce an unwieldy branch.
pub fn task_name(number: u64, title: &str) -> String {
    const MAX_SLUG: usize = 40;

    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in title.chars() {
        if ch.is_ascii_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    if slug.len() > MAX_SLUG {
        // Cut at the last dash inside the budget; fall back to a hard cut for a title
        // that is one very long word.
        let cut = slug[..MAX_SLUG].rfind('-').unwrap_or(MAX_SLUG);
        slug.truncate(cut);
    }

    if slug.is_empty() {
        format!("gh-{number}")
    } else {
        format!("gh-{number}-{slug}")
    }
}

/// Whether a task directory name belongs to the given issue number.
///
/// Matching on the **number**, not on [`task_name`]'s spelling, is deliberate: tasks
/// predating this convention are named differently (`issue-9_dotfile_repo_retire`), and
/// a slug also changes whenever the issue title is edited. So the name is tokenised and
/// accepted when it opens with `gh`/`issue`/`issues` followed by exactly this number:
/// `gh-42-x`, `gh42-x`, `issue-42_x`, and bare `gh-42` all match issue 42, while
/// `gh-421-x` and `some-42` do not.
pub fn matches_issue(task: &str, number: u64) -> bool {
    let lower = task.to_ascii_lowercase();
    // A bare number prefix is too loose to accept: `2-fix-thing` is a task name, not a
    // reference to issue 2, so one of the keywords has to be there.
    let Some(rest) = ["gh", "issues", "issue"]
        .iter()
        .find_map(|p| lower.strip_prefix(*p))
        .map(|r| r.trim_start_matches(['-', '_']))
    else {
        return false;
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    // The digits must be the whole number, not a prefix of a longer one.
    digits.parse::<u64>().ok() == Some(number)
        && rest[digits.len()..]
            .chars()
            .next()
            .is_none_or(|c| c == '-' || c == '_')
}

// ---------------------------------------------------------------------------
// Scan
// ---------------------------------------------------------------------------

/// Every project: a visible directory under `~/fun_repos` holding a `.git` entry.
///
/// Not filtered to GitHub remotes here — that would cost a `git config` call per
/// project on the main thread, and a non-GitHub project (a codeberg repo, a fork with
/// issues disabled) already renders correctly as a [`FetchState::Failed`] line once its
/// `gh` call comes back.
pub fn scan_projects() -> Vec<ProjectIssues> {
    let mut projects: Vec<Project> = Vec::new();
    let Ok(entries) = std::fs::read_dir(projects_root()) else {
        return Vec::new();
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || !entry.path().is_dir() || !entry.path().join(".git").exists() {
            continue;
        }
        projects.push(Project {
            name,
            path: entry.path(),
        });
    }
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    projects
        .into_iter()
        .map(|project| ProjectIssues {
            project,
            state: FetchState::Loading,
            issues: Vec::new(),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// gh
// ---------------------------------------------------------------------------

/// One issue as `gh issue list --json` emits it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIssue {
    number: u64,
    title: String,
    url: String,
    #[serde(default)]
    labels: Vec<RawLabel>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    author: Option<RawAuthor>,
}

#[derive(Deserialize)]
struct RawLabel {
    name: String,
}

#[derive(Deserialize)]
struct RawAuthor {
    #[serde(default)]
    login: Option<String>,
}

/// Read a project's open issues via `gh`, run from the project's checkout so `gh`
/// resolves the repo from its own remote. Blocking; call off the main thread.
///
/// Any failure — `gh` missing, no GitHub remote, issues disabled, not authenticated —
/// comes back as `Err(reason)` for the project's line. Nothing here can abort the app.
pub fn fetch_issues(project: &Project) -> Result<Vec<Issue>, String> {
    let out = Command::new("gh")
        .current_dir(&project.path)
        .args([
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            ISSUE_LIMIT,
            "--json",
            GH_FIELDS,
        ])
        .output()
        .map_err(|e| format!("gh not runnable: {e}"))?;
    if !out.status.success() {
        return Err(first_line(&String::from_utf8_lossy(&out.stderr)));
    }
    parse_issues(&out.stdout)
}

/// The first non-empty line of `gh`'s stderr, capped — enough to say *why* a project has
/// no issues without wrapping the list in a paragraph of remote-configuration advice.
///
/// The cap is what keeps the project line to one row at the window's default width; `gh`
/// happily emits a 200-character sentence telling you how to register a GitHub host.
fn first_line(stderr: &str) -> String {
    const MAX: usize = 80;

    let line = stderr
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("gh failed");
    let line = line.strip_prefix("error: ").unwrap_or(line);
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

/// Map `gh`'s JSON into [`Issue`]s, resolving the topic tag and task name. Split from
/// [`fetch_issues`] so the mapping is testable without `gh` — and, since the worktree
/// lookup moved to [`super::join`], without a filesystem fixture either.
///
/// The order `gh` returned is preserved; display order is the merged sort's job.
pub fn parse_issues(json: &[u8]) -> Result<Vec<Issue>, String> {
    let raw: Vec<RawIssue> =
        serde_json::from_slice(json).map_err(|e| format!("unparseable gh output: {e}"))?;
    Ok(raw
        .into_iter()
        .map(|r| {
            let labels: Vec<String> = r.labels.into_iter().map(|l| l.name).collect();
            Issue {
                topic: topic_of(&labels),
                task: task_name(r.number, &r.title),
                number: r.number,
                title: r.title,
                url: r.url,
                labels,
                updated: r.updated_at.as_deref().and_then(parse_rfc3339_utc),
                author: r.author.and_then(|a| a.login).filter(|l| !l.is_empty()),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The topic tag: prefixed labels only, first one wins, and a value that could not be
    /// a directory name (or would need shell escaping) is rejected rather than sanitized.
    #[test]
    fn topic_comes_only_from_a_prefixed_label() {
        let labels = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();

        assert_eq!(
            topic_of(&labels(&["bug", "topic:features", "enhancement"])),
            Some("features".to_owned())
        );
        // Bare labels are never topics, however plausible they look.
        assert_eq!(topic_of(&labels(&["features", "bugs", "perf"])), None);
        assert_eq!(topic_of(&labels(&[])), None);
        // First prefixed label wins.
        assert_eq!(
            topic_of(&labels(&["topic:perf", "topic:features"])),
            Some("perf".to_owned())
        );
        // Values that cannot be a directory component / would need escaping.
        for bad in [
            "topic:",
            "topic: features",
            "topic:has space",
            "topic:a/b",
            "topic:..",
            "topic:.hidden",
            "topic:it's",
            "topic:x;rm -rf /",
        ] {
            assert_eq!(
                topic_of(&labels(&[bad])),
                None,
                "{bad} must not yield a topic"
            );
        }
        // Everything a directory name legitimately uses is fine.
        assert_eq!(
            topic_of(&labels(&["topic:ui_github-issues.v2"])),
            Some("ui_github-issues.v2".to_owned())
        );
    }

    /// Task names match what `/gh-issue-create` suggests: `gh-<n>-<slug>`, truncated at a
    /// dash boundary, and degrading to `gh-<n>` when a title has nothing sluggable.
    #[test]
    fn task_names_are_slugged_and_bounded() {
        assert_eq!(
            task_name(42, "Add a create issue command"),
            "gh-42-add-a-create-issue-command"
        );
        // Punctuation, case, and runs of separators all collapse to single dashes.
        assert_eq!(
            task_name(7, "  Fix: the *UI* — really?!  "),
            "gh-7-fix-the-ui-really"
        );
        assert_eq!(task_name(9, "CamelCase123"), "gh-9-camelcase123");
        // Truncated at a dash boundary inside the 40-char budget, never mid-word.
        let long = task_name(
            9,
            "Retire the home dotfiles repo: vendor its contents here with per-machine variants",
        );
        assert_eq!(long, "gh-9-retire-the-home-dotfiles-repo-vendor");
        assert!(!long.ends_with('-'));
        // A single very long word has no dash to cut at, so it gets a hard cut.
        assert_eq!(
            task_name(1, &"z".repeat(60)),
            format!("gh-1-{}", "z".repeat(40))
        );
        // Nothing sluggable at all.
        assert_eq!(task_name(5, "🎉 !!! ???"), "gh-5");
        assert_eq!(task_name(5, ""), "gh-5");
    }

    /// A task name is claimed by issue number, across every naming convention this repo
    /// has used — and emphatically not by a number that merely appears in the name.
    #[test]
    fn worktrees_are_matched_by_issue_number() {
        // The current convention, plus the pre-convention spellings.
        for name in [
            "gh-42-add-a-thing",
            "gh42-add-a-thing",
            "gh_42_add",
            "gh-42",
            "issue-42_dotfile_repo_retire",
            "issue42-x",
            "issues-42-x",
            "GH-42-Add",
        ] {
            assert!(matches_issue(name, 42), "{name} should match issue 42");
        }
        // A different issue, a longer number, or a number that is just part of the name.
        for name in [
            "gh-421-add",
            "gh-4-add",
            "42-add-a-thing",
            "some-task-42",
            "fix-gh-42",
            "ghost-42",
            "gh-",
            "gh",
            "",
        ] {
            assert!(!matches_issue(name, 42), "{name} must not match issue 42");
        }
    }

    /// `gh`'s JSON maps to the model: labels flatten to names, the topic tag is split out
    /// of the label list, and timestamps parse. No filesystem fixture — the worktree
    /// lookup lives in `join` now, which is what frees this test from `HOME`.
    #[test]
    fn gh_json_maps_to_issues() {
        let json = br#"[
          {"number":3,"title":"Untagged thing","url":"https://gh/3",
           "labels":[{"name":"bug"}],"updatedAt":"2026-07-20T10:00:00Z",
           "author":{"login":"julianbieber"}},
          {"number":5,"title":"Tagged, no worktree","url":"https://gh/5",
           "labels":[{"name":"topic:features"},{"name":"enhancement"}],
           "updatedAt":"2026-07-28T10:00:00Z","author":{"login":"julianbieber"}},
          {"number":7,"title":"Chunk loading","url":"https://gh/7",
           "labels":[{"name":"topic:perf"}],"updatedAt":"2026-07-01T10:00:00Z","author":null}
        ]"#;

        let issues = parse_issues(json).expect("valid gh output");
        let numbers: Vec<u64> = issues.iter().map(|i| i.number).collect();
        assert_eq!(numbers, vec![3, 5, 7], "gh's order is preserved");

        let tagged = &issues[1];
        assert_eq!(tagged.topic.as_deref(), Some("features"));
        assert_eq!(tagged.task, "gh-5-tagged-no-worktree");
        assert_eq!(
            tagged.other_labels().collect::<Vec<_>>(),
            vec!["enhancement"],
            "the topic tag is not repeated in the label line"
        );
        assert!(tagged.updated.is_some(), "gh's timestamp parsed");

        let perf = &issues[2];
        assert_eq!(perf.topic.as_deref(), Some("perf"));
        assert_eq!(perf.task, "gh-7-chunk-loading");
        assert_eq!(perf.author, None, "a null author stays None");

        assert_eq!(issues[0].topic, None, "a bare `bug` label is not a topic");

        // Malformed output is an error for one project's line, never a panic.
        assert!(parse_issues(b"{ not json").is_err());
        assert!(parse_issues(b"[]").unwrap().is_empty());
    }

    /// `gh`'s stderr becomes one short list line: first line only, `error:` stripped,
    /// trailing advice sentence dropped, and hard-capped so it cannot wrap.
    #[test]
    fn gh_errors_shrink_to_one_line() {
        assert_eq!(
            first_line("\nerror: could not determine base repo\nmore detail\n"),
            "could not determine base repo"
        );
        // The real codeberg case: reason, then two sentences of advice.
        assert_eq!(
            first_line(
                "none of the git remotes point to a known GitHub host. To tell gh about a \
                 new GitHub host, please use `gh auth login`"
            ),
            "none of the git remotes point to a known GitHub host"
        );
        assert_eq!(first_line(""), "gh failed");
        let long = first_line(&"x".repeat(300));
        assert_eq!(long.chars().count(), 80, "capped, with an ellipsis");
        assert!(long.ends_with('…'));
    }
}
