//! What a tab's piece of work is for and how far it has come, read from the files its pipeline
//! leaves in the notes: `aspect.json`, `brief.md`, `build-task.json`, `issue.md`, `prs.json` and
//! the plan files. Nothing here asks a model or the network.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use hwt_core::Roots;
use serde::Deserialize;

use crate::scope::Scope;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub heading: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Todo {
    pub text: String,
    #[serde(default)]
    pub status: String,
}

impl Todo {
    /// Whether the step is behind the work: done or dropped.
    pub fn is_closed(&self) -> bool {
        matches!(self.status.as_str(), "done" | "dropped")
    }
}

/// Something left over when an aspect finished; `issue` names the task that took it on.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct OpenItem {
    pub text: String,
    #[serde(default)]
    pub issue: Option<String>,
}

/// Everything the pane shows about one scope. A file that is missing leaves its fields empty; a
/// file that does not parse adds a line to `warnings`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overview {
    pub name: String,
    pub kind: Option<String>,
    pub state: Option<String>,
    pub phase: Option<String>,
    pub phase_since: Option<SystemTime>,
    pub waiting_on: Option<String>,
    pub goal: Option<String>,
    pub sections: Vec<Section>,
    pub todo: Vec<Todo>,
    pub outcome: Option<String>,
    pub open_items: Vec<OpenItem>,
    pub prs: Vec<String>,
    pub plan: Option<PathBuf>,
    pub warnings: Vec<String>,
}

impl Overview {
    pub fn load(roots: &Roots, scope: &Scope) -> Self {
        let dir = scope.notes(roots);
        let mut o = Self {
            name: scope.name().to_owned(),
            ..Self::default()
        };
        let build: Option<BuildState> = o.json(&dir.join("build-task.json"));
        if scope.aspect.is_some() {
            if let Some(a) = o.json::<AspectRecord>(&dir.join("aspect.json")) {
                o.kind = a.kind;
                o.state = a.state;
                o.goal = a.goal;
                o.outcome = a.outcome;
                o.open_items = a.open_items;
                o.prs.extend(a.pr);
            }
            if let Some(brief) = read(&dir.join("brief.md")) {
                o.sections = brief_sections(&brief);
            }
        } else {
            if let Some(issue) = read(&dir.join("issue.md")) {
                let (title, body) = issue_parts(&issue);
                o.goal = title;
                if !body.is_empty() {
                    o.sections.push(Section {
                        heading: "Issue".into(),
                        body,
                    });
                }
            }
            if let Some(prs) = o.json::<Vec<PrRecord>>(&dir.join("prs.json")) {
                o.prs.extend(prs.into_iter().map(|p| p.url));
            }
        }
        if let Some(b) = build {
            o.phase = b.phase;
            o.phase_since = b
                .phase_since
                .as_deref()
                .and_then(hwt_core::parse_rfc3339_utc);
            o.waiting_on = b.waiting_on.filter(|w| !w.trim().is_empty());
            o.todo = b.todo;
            if o.sections.is_empty()
                && let Some(brief) = b.brief.filter(|b| !b.trim().is_empty())
            {
                o.sections.push(Section {
                    heading: "Brief".into(),
                    body: brief,
                });
            }
            o.plan = b
                .plan
                .filter(|p| !p.is_empty())
                .map(|p| dir.join(p))
                .filter(|p| p.is_file());
        }
        if o.plan.is_none() {
            o.plan = newest_plan(&dir);
        }
        o
    }

    /// What the tab is called: its name, and the phase (or, without a pipeline run, the aspect's
    /// state) after ` · `.
    pub fn tab_label(&self) -> String {
        match self.phase.as_deref().or(self.state.as_deref()) {
            Some(p) => format!("{} · {p}", self.name),
            None => self.name.clone(),
        }
    }

    fn json<T: for<'de> Deserialize<'de>>(&mut self, path: &Path) -> Option<T> {
        let text = read(path)?;
        match serde_json::from_str(&text) {
            Ok(v) => Some(v),
            Err(e) => {
                let file = path
                    .file_name()
                    .map_or_else(String::new, |f| f.to_string_lossy().into_owned());
                self.warnings.push(format!("{file}: {e}"));
                None
            }
        }
    }
}

#[derive(Deserialize)]
struct AspectRecord {
    kind: Option<String>,
    state: Option<String>,
    goal: Option<String>,
    pr: Option<String>,
    outcome: Option<String>,
    #[serde(default)]
    open_items: Vec<OpenItem>,
}

#[derive(Deserialize)]
struct BuildState {
    phase: Option<String>,
    phase_since: Option<String>,
    waiting_on: Option<String>,
    brief: Option<String>,
    plan: Option<String>,
    #[serde(default)]
    todo: Vec<Todo>,
}

#[derive(Deserialize)]
struct PrRecord {
    url: String,
}

const BRIEF_SECTIONS: [(&str, &str); 5] = [
    ("Why the task needs it", "Why"),
    ("Goal", "What"),
    ("Question", "What"),
    ("Done when", "Done when"),
    ("Out of scope", "Out of scope"),
];

fn brief_sections(brief: &str) -> Vec<Section> {
    let found = markdown_sections(strip_front_matter(brief));
    BRIEF_SECTIONS
        .iter()
        .filter_map(|(source, shown)| {
            found
                .iter()
                .find(|s| s.heading == *source)
                .map(|s| Section {
                    heading: (*shown).to_owned(),
                    body: s.body.clone(),
                })
        })
        .collect()
}

fn issue_parts(issue: &str) -> (Option<String>, String) {
    let title = front_matter(issue).and_then(|fm| {
        fm.lines().find_map(|l| {
            l.strip_prefix("title:")
                .map(|t| t.trim().trim_matches('"').to_owned())
        })
    });
    let body = strip_front_matter(issue);
    let body = body
        .split("\n## Progress")
        .next()
        .unwrap_or(body)
        .trim()
        .to_owned();
    (title, body)
}

fn front_matter(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---\n")?;
    rest.split_once("\n---").map(|(fm, _)| fm)
}

fn strip_front_matter(text: &str) -> &str {
    text.strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---"))
        .map_or(text, |(_, body)| body.trim_start_matches('-'))
}

fn markdown_sections(text: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            out.push(Section {
                heading: h.trim().to_owned(),
                body: String::new(),
            });
        } else if let Some(s) = out.last_mut() {
            s.body.push_str(line);
            s.body.push('\n');
        }
    }
    for s in &mut out {
        s.body = s.body.trim().to_owned();
    }
    out
}

fn newest_plan(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("plan-") && n.ends_with(".md"))
        })
        .max()
}

fn read(path: &Path) -> Option<String> {
    match fs::read_to_string(path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => {
            log::warn!("cannot read {}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Home(PathBuf);

    impl Home {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("htab-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
        fn roots(&self) -> Roots {
            Roots::under(&self.0)
        }
        fn write(&self, scope: &Scope, file: &str, text: &str) {
            let dir = scope.notes(&self.roots());
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(file), text).unwrap();
        }
    }

    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn aspect() -> Scope {
        Scope::parse("p/t/task/persist").unwrap()
    }

    #[test]
    fn an_aspect_shows_its_brief_record_and_plan() {
        let home = Home::new("aspect");
        let s = aspect();
        home.write(
            &s,
            "aspect.json",
            r#"{"kind":"code","state":"pr","goal":"Save the game","pr":"https://x/pull/3",
               "open_items":[{"text":"tune","issue":null}]}"#,
        );
        home.write(
            &s,
            "brief.md",
            "---\ntitle: x\n---\n\n## Goal\n\nSave it.\n\n## Why the task needs it\n\nLosing runs.\n\n\
             ## Where to look\n\nsrc/\n\n## Out of scope\n\nCloud.\n",
        );
        home.write(
            &s,
            "build-task.json",
            r#"{"phase":"review","waiting_on":"read it","plan":"plan-2026-01-02-b.md",
               "todo":[{"text":"a","status":"done"},{"text":"b","status":"active"}]}"#,
        );
        home.write(&s, "plan-2026-01-02-b.md", "plan");
        let o = Overview::load(&home.roots(), &s);
        let headings: Vec<&str> = o.sections.iter().map(|s| s.heading.as_str()).collect();
        assert_eq!(headings, ["Why", "What", "Out of scope"]);
        assert_eq!(o.sections[0].body, "Losing runs.");
        assert_eq!(o.goal.as_deref(), Some("Save the game"));
        assert_eq!(o.prs, ["https://x/pull/3"]);
        assert_eq!(o.waiting_on.as_deref(), Some("read it"));
        assert_eq!(o.todo.iter().filter(|t| t.is_closed()).count(), 1);
        assert_eq!(o.open_items.len(), 1);
        assert!(o.plan.as_ref().unwrap().ends_with("plan-2026-01-02-b.md"));
        assert_eq!(o.tab_label(), "persist · review");
        assert!(o.warnings.is_empty());
    }

    #[test]
    fn a_research_aspect_without_a_pipeline_is_labelled_by_its_state() {
        let home = Home::new("research");
        let s = aspect();
        home.write(&s, "aspect.json", r#"{"kind":"research","state":"active"}"#);
        let o = Overview::load(&home.roots(), &s);
        assert_eq!(o.tab_label(), "persist · active");
        assert_eq!(o.plan, None);
    }

    #[test]
    fn a_task_shows_its_issue_without_progress_and_its_prs() {
        let home = Home::new("task");
        let s = Scope::parse("p/t/task").unwrap();
        home.write(
            &s,
            "issue.md",
            "---\ntitle: \"Make it fast\"\ncreated: 2026-01-01\n---\n\nIt is slow.\n\n## Progress\n\n- did x\n",
        );
        home.write(
            &s,
            "prs.json",
            r#"[{"url":"https://x/pull/1","title":"t"}]"#,
        );
        home.write(&s, "plan-2026-01-01-a.md", "");
        home.write(&s, "plan-2026-02-01-b.md", "");
        let o = Overview::load(&home.roots(), &s);
        assert_eq!(o.goal.as_deref(), Some("Make it fast"));
        assert_eq!(o.sections[0].body, "It is slow.");
        assert_eq!(o.prs, ["https://x/pull/1"]);
        assert!(o.plan.as_ref().unwrap().ends_with("plan-2026-02-01-b.md"));
        assert_eq!(o.tab_label(), "task");
    }

    #[test]
    fn a_task_without_an_issue_shows_its_brief() {
        let home = Home::new("brief");
        let s = Scope::parse("p/t/task").unwrap();
        home.write(
            &s,
            "build-task.json",
            r#"{"phase":"implement","brief":"Do the thing."}"#,
        );
        let o = Overview::load(&home.roots(), &s);
        assert_eq!(o.sections[0].heading, "Brief");
        assert_eq!(o.tab_label(), "task · implement");
    }

    #[test]
    fn a_broken_file_is_a_warning() {
        let home = Home::new("broken");
        let s = aspect();
        home.write(&s, "aspect.json", "{");
        let o = Overview::load(&home.roots(), &s);
        assert_eq!(o.warnings.len(), 1);
        assert!(o.warnings[0].starts_with("aspect.json:"));
    }
}
