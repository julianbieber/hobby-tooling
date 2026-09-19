//! Seeding a task's pipeline state from its coordinator's list and goal file.

use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use crate::clock;
use crate::goal::{self, Decision, Goal};
use crate::model::{Coordinator, MergeBy, STATE_FILE, TaskEntry};

const CONTEXT_FILE: &str = "business-context.md";

/// The two files a seed writes, rendered and not yet on disk.
#[derive(Debug)]
pub struct Seeded {
    /// The task's `build-task.json`, with a trailing newline.
    pub state: String,
    /// The task's `business-context.md`; `None` when no decision concerns the task.
    pub context: Option<String>,
    /// How many of the goal's decisions concern the task.
    pub decisions: usize,
}

#[derive(Serialize)]
struct State<'a> {
    phase: &'a str,
    auto: bool,
    merge: MergeBy,
    #[serde(skip_serializing_if = "Option::is_none")]
    model: Option<&'a str>,
    coordinator: String,
    goal: String,
    scenarios: &'a [String],
    brief_source: &'a str,
    brief: String,
    acceptance: String,
    feedback: Vec<String>,
    rounds: u32,
    written: String,
    phase_since: String,
}

/// Render the seed for `entry` without touching disk beyond looking for a screen's
/// capture next to the goal. `dir` is the coordinator's notes directory and `goal_path`
/// the goal file, both as they should be recorded; `now` is seconds since the epoch.
///
/// Fails, writing nothing, when the task owns no scenario, the goal or its split fails
/// `lint` (an open `?` included — a goal with a question left must not seed a run), or an
/// owned scenario is not in the goal.
pub fn seed(
    dir: &Path,
    coord: &Coordinator,
    entry: &TaskEntry,
    goal_path: &Path,
    goal_text: &str,
    now: i64,
) -> Result<Seeded, String> {
    if entry.scenarios.is_empty() {
        return Err(format!("task {} owns no scenario", entry.task));
    }
    let parsed = goal::parse(goal_text);
    let mut findings = goal::lint(&parsed);
    findings.extend(goal::lint_tasks(&parsed, coord));
    if !findings.is_empty() {
        let lines: Vec<String> = findings
            .iter()
            .map(|f| {
                if f.line == 0 {
                    format!("{}: {}", goal_path.display(), f.message)
                } else {
                    format!("{}:{}: {}", goal_path.display(), f.line, f.message)
                }
            })
            .collect();
        return Err(format!(
            "the goal does not lint, so it cannot seed a run:\n{}",
            lines.join("\n")
        ));
    }
    let acceptance = goal::show_text(&parsed, &entry.scenarios)
        .map_err(|missing| format!("no such scenario: {}", missing.join(", ")))?;

    let selected: Vec<&Decision> = parsed
        .decisions
        .iter()
        .filter(|d| entry.scenarios.iter().any(|id| d.names(id)) || d.names(&entry.task))
        .collect();

    let written = clock::rfc3339_utc(now);
    let brief = brief(coord, entry, goal_path, &parsed, &selected, &acceptance);
    let state = State {
        phase: "acceptance",
        auto: true,
        merge: coord.merge,
        model: entry.model.as_deref().or(coord.model.as_deref()),
        coordinator: dir.display().to_string(),
        goal: goal_path.display().to_string(),
        scenarios: &entry.scenarios,
        brief_source: "coordinator",
        brief,
        acceptance,
        feedback: Vec::new(),
        rounds: 0,
        phase_since: written.clone(),
        written,
    };
    let mut state = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    state.push('\n');

    let context = (!selected.is_empty())
        .then(|| business_context(&entry.task, goal_path, &selected, &clock::rfc3339_utc(now)));
    Ok(Seeded {
        state,
        context,
        decisions: selected.len(),
    })
}

/// `hcoord seed <coordinator notes dir> <task> [--force]`: write the task's
/// `build-task.json` — `acceptance` being its scenarios exactly as `goal show` prints
/// them — and, when a decision in the goal names the task or one of its scenarios, its
/// `business-context.md`, into the task's notes directory; print one line saying what
/// was written and exit 0.
///
/// Exits 1 with `hcoord seed: <reason>` on stderr, having written nothing, when: the
/// task is not in `tasks.json`; `tasks.json` names no `goal`; the goal cannot be read,
/// or `seed` refuses it; or `build-task.json` — or `business-context.md`, when one
/// would be written — already exists and `force` is not set. `force` overwrites them.
pub fn run(dir: &Path, task: &str, force: bool) -> ExitCode {
    match try_run(dir, task, force) {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hcoord seed: {e}");
            ExitCode::from(1)
        }
    }
}

fn try_run(dir: &Path, task: &str, force: bool) -> Result<String, String> {
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let coord = Coordinator::load(&dir)?;
    let entry = coord.tasks.iter().find(|t| t.task == task).ok_or_else(|| {
        format!(
            "no task {task} in {}",
            Coordinator::tasks_path(&dir).display()
        )
    })?;
    let goal_rel = coord
        .goal
        .as_deref()
        .ok_or("tasks.json names no goal; seed this task by hand")?;
    let goal_path = dir
        .join(goal_rel)
        .canonicalize()
        .map_err(|e| format!("cannot read goal {}: {e}", dir.join(goal_rel).display()))?;
    let goal_text = std::fs::read_to_string(&goal_path)
        .map_err(|e| format!("cannot read {}: {e}", goal_path.display()))?;

    let seeded = seed(
        &dir,
        &coord,
        entry,
        &goal_path,
        &goal_text,
        clock::now_unix(),
    )?;

    let notes = coord.task_notes_dir(&dir, entry);
    let state_path = notes.join(STATE_FILE);
    let context_path = notes.join(CONTEXT_FILE);
    if !force {
        for path in [&state_path]
            .into_iter()
            .chain(seeded.context.is_some().then_some(&context_path))
        {
            if path.exists() {
                return Err(format!(
                    "{} exists; the task is already seeded (--force overwrites)",
                    path.display()
                ));
            }
        }
    }
    std::fs::create_dir_all(&notes)
        .map_err(|e| format!("cannot create {}: {e}", notes.display()))?;
    std::fs::write(&state_path, &seeded.state)
        .map_err(|e| format!("cannot write {}: {e}", state_path.display()))?;
    let mut line = format!(
        "seeded {}: {STATE_FILE} ({})",
        notes.display(),
        entry.scenarios.join(", ")
    );
    if let Some(context) = &seeded.context {
        std::fs::write(&context_path, context)
            .map_err(|e| format!("cannot write {}: {e}", context_path.display()))?;
        line.push_str(&format!(
            ", {CONTEXT_FILE} ({} decisions)",
            seeded.decisions
        ));
    }
    Ok(line)
}

fn brief(
    coord: &Coordinator,
    entry: &TaskEntry,
    goal_path: &Path,
    goal: &Goal,
    selected: &[&Decision],
    acceptance: &str,
) -> String {
    let mut sections: Vec<String> = Vec::new();

    if !coord.description.trim().is_empty() {
        let quoted: Vec<String> = coord
            .description
            .lines()
            .map(|l| {
                if l.is_empty() {
                    ">".to_owned()
                } else {
                    format!("> {l}")
                }
            })
            .collect();
        sections.push(format!(
            "## Coordinator's description (verbatim)\n\n{}",
            quoted.join("\n")
        ));
    }

    sections.push(format!(
        "## Intent (verbatim from the goal)\n\n{}",
        goal.intent.trim_end()
    ));

    let decisions = if selected.is_empty() {
        "None of the goal's decisions names this task or its scenarios.".to_owned()
    } else {
        selected
            .iter()
            .map(|d| d.text.trim_end())
            .collect::<Vec<_>>()
            .join("\n")
    };
    sections.push(format!(
        "## Decisions that concern this task (verbatim from the goal)\n\n{decisions}"
    ));

    let mut this = vec![
        format!("- task: `{}`", entry.task),
        format!("- title: {}", entry.title),
        format!("- goal: `{}`", goal_path.display()),
    ];
    if let Some(n) = issue_number(&entry.task) {
        this.push(format!(
            "- GitHub: issue #{n}; the PR body's first line is `Closes #{n}`"
        ));
    }
    if let Some(note) = &entry.note {
        this.push(format!("- note from the split: {note}"));
    }
    sections.push(format!("## This task\n\n{}", this.join("\n")));

    sections.push(format!("## Scenarios you own\n\n{}", acceptance.trim_end()));
    sections.push(format!(
        "## Screens they name\n\n{}",
        screens_named(goal, entry, goal_path)
    ));
    sections.push(format!("## Dependencies\n\n{}", dependencies(coord, entry)));

    sections.join("\n\n")
}

fn screens_named(goal: &Goal, entry: &TaskEntry, goal_path: &Path) -> String {
    let mut names: Vec<&str> = Vec::new();
    for id in &entry.scenarios {
        if let Some(name) = goal
            .scenarios
            .iter()
            .find(|s| &s.id == id)
            .and_then(|s| s.screen.as_deref())
            && !names.contains(&name)
        {
            names.push(name);
        }
    }
    let goal_dir = goal_path.parent().unwrap_or(Path::new("."));
    let mut blocks = Vec::new();
    for name in names {
        for screen in goal.screens.iter().filter(|s| s.name == name) {
            let mut block = screen.text.trim_end().to_owned();
            let png = goal_dir.join("screens").join(format!("{name}.png"));
            if screen.qualifier.starts_with("exists") && png.exists() {
                block.push_str(&format!("\npng: {}", png.display()));
            }
            blocks.push(block);
        }
    }
    if blocks.is_empty() {
        "None of the owned scenarios names a screen.".to_owned()
    } else {
        blocks.join("\n\n")
    }
}

fn dependencies(coord: &Coordinator, entry: &TaskEntry) -> String {
    let describe = |tasks: Vec<&TaskEntry>| {
        if tasks.is_empty() {
            "nothing".to_owned()
        } else {
            tasks
                .iter()
                .map(|t| format!("`{}` — {}", t.task, t.title))
                .collect::<Vec<_>>()
                .join("; ")
        }
    };
    let depends_on: Vec<&TaskEntry> = entry
        .depends_on
        .iter()
        .filter_map(|name| coord.tasks.iter().find(|t| &t.task == name))
        .collect();
    let depended_on_by: Vec<&TaskEntry> = coord
        .tasks
        .iter()
        .filter(|t| t.depends_on.contains(&entry.task))
        .collect();

    let mut lines = vec![format!("- depends on: {}", describe(depends_on.clone()))];
    lines.push(format!("- depended on by: {}", describe(depended_on_by)));
    let upstream: Vec<&str> = depends_on
        .iter()
        .flat_map(|t| t.scenarios.iter().map(String::as_str))
        .collect();
    if !upstream.is_empty() {
        lines.push(format!(
            "- must not break: the scenarios of the tasks it depends on: {}",
            upstream.join(", ")
        ));
    }
    lines.join("\n")
}

fn business_context(task: &str, goal_path: &Path, selected: &[&Decision], today: &str) -> String {
    let mut out = format!(
        "---\ntitle: \"{task} — business context\"\ncreated: {}\ntags:\n  - business-context\n---\n\n\
         Copied verbatim by `hcoord seed` from the goal `{}`: the decisions that name this task or its scenarios.\n",
        &today[..10],
        goal_path.display()
    );
    for d in selected {
        out.push_str(&format!("\n## {}\n\n{}", d.handle, d.text));
    }
    out
}

fn issue_number(task: &str) -> Option<&str> {
    let rest = task.strip_prefix("gh-")?;
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let boundary = rest[digits..].is_empty() || rest[digits..].starts_with('-');
    (digits > 0 && boundary).then(|| &rest[..digits])
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOAL: &str = "\
## Intent
> the goal
## Decisions
- **about-s2** — \"S2 has to keep working\"
- **about-task** — \"b-one must be quick\"
- **elsewhere** — \"S10 only\"
## Screens
### main (exists)
    +---+
### main (target)
    +---+
## Scenarios
### S1 open it                          screen: main
    1. `run`
    -> it opens
    widgets: none new
### S2 add one                          screen: main
    1. click
    -> it adds
    widgets: none new
### S10 other                           screen: main
    1. click
    -> other
    widgets: none new
## Widgets
| widget | exists | provided by |
|--------|--------|-------------|
";

    fn list(tasks: &str) -> Coordinator {
        Coordinator::parse(&format!(
            r#"{{"project":"p","blurb":"b","description":"one\n\ntwo","model":"sonnet","goal":"g.md","tasks":{tasks}}}"#
        ))
        .unwrap()
    }

    fn two_tasks() -> Coordinator {
        list(
            r#"[{"task":"b-one","title":"One","scenarios":["S2","S1"],"note":"careful"},
                {"task":"gh-7-two","title":"Two","scenarios":["S10"],"depends_on":["b-one"],"model":"opus"}]"#,
        )
    }

    fn seeded(coord: &Coordinator, index: usize) -> Result<Seeded, String> {
        seed(
            Path::new("/n/p/coordinator/b"),
            coord,
            &coord.tasks[index],
            Path::new("/n/p/planning/b/goal.md"),
            GOAL,
            1_709_208_000,
        )
    }

    fn state(s: &Seeded) -> serde_json::Value {
        serde_json::from_str(&s.state).unwrap()
    }

    // Pins `acceptance` to the text `goal show` prints for the owned ids, in the order
    // the task lists them, and `written` to `phase_since` on a fresh seed.
    #[test]
    fn acceptance_is_show_text_for_the_owned_ids_in_the_tasks_order() {
        let coord = two_tasks();
        let s = seeded(&coord, 0).unwrap();
        let goal = goal::parse(GOAL);
        let expected = goal::show_text(&goal, &["S2".into(), "S1".into()]).unwrap();
        assert_eq!(state(&s)["acceptance"], expected);
        assert_eq!(state(&s)["scenarios"], serde_json::json!(["S2", "S1"]));
        assert_eq!(state(&s)["written"], "2024-02-29T12:00:00Z");
        assert_eq!(state(&s)["written"], state(&s)["phase_since"]);
    }

    // Pins that a decision naming only another scenario (`S10`) is left out of the
    // context of a task owning `S2`.
    #[test]
    fn a_decision_naming_another_scenario_is_not_selected() {
        let coord = two_tasks();
        let s = seeded(&coord, 0).unwrap();
        let context = s.context.unwrap();
        assert!(context.contains("## about-s2"));
        assert!(!context.contains("## elsewhere"));
    }

    // Pins that a decision naming the task, and no scenario, is selected for it.
    #[test]
    fn a_decision_naming_the_task_is_selected() {
        let coord = two_tasks();
        let s = seeded(&coord, 0).unwrap();
        assert_eq!(s.decisions, 2);
        assert!(
            s.context
                .unwrap()
                .contains("- **about-task** — \"b-one must be quick\"\n")
        );
    }

    // Pins that a task no decision names gets no business-context.md, and a brief that
    // says so.
    #[test]
    fn no_concerning_decision_means_no_context_file() {
        let coord = list(
            r#"[{"task":"b-three","title":"Three","scenarios":["S1"]},
                {"task":"b-four","title":"Four","scenarios":["S2","S10"]}]"#,
        );
        let s = seeded(&coord, 0).unwrap();
        assert!(s.context.is_none());
        assert!(
            state(&s)["brief"]
                .as_str()
                .unwrap()
                .contains("None of the goal's decisions names this task")
        );
    }

    // Pins the refusal of a task that owns no scenario.
    #[test]
    fn a_task_with_no_scenarios_is_refused() {
        let coord = list(r#"[{"task":"b-one","title":"One"}]"#);
        let e = seeded(&coord, 0).unwrap_err();
        assert!(e.contains("owns no scenario"), "{e}");
    }

    // Pins that a `?` left in the goal makes `seed` fail with the lint's finding.
    #[test]
    fn a_goal_with_an_open_question_is_refused() {
        let coord = two_tasks();
        let e = seed(
            Path::new("/n"),
            &coord,
            &coord.tasks[0],
            Path::new("/n/goal.md"),
            &GOAL.replace("-> it opens", "-> it opens?"),
            0,
        )
        .unwrap_err();
        assert!(e.contains("open question"), "{e}");
    }

    // Pins that the `Closes #<n>` line is derived from a `gh-<digits>` task name and
    // from no other.
    #[test]
    fn the_issue_line_appears_only_for_a_gh_task() {
        let coord = two_tasks();
        let plain = seeded(&coord, 0).unwrap();
        assert!(
            !state(&plain)["brief"]
                .as_str()
                .unwrap()
                .contains("Closes #")
        );
        let gh = seeded(&coord, 1).unwrap();
        assert!(
            state(&gh)["brief"]
                .as_str()
                .unwrap()
                .contains("issue #7; the PR body's first line is `Closes #7`")
        );
        assert_eq!(issue_number("gh-12"), Some("12"));
        assert_eq!(issue_number("gh-x-1"), None);
        assert_eq!(issue_number("gh-12x"), None);
    }

    // Pins `model` as the task's, else the list's, else omitted (`hauto` then passes no
    // `--model`).
    #[test]
    fn model_falls_back_from_the_task_to_the_list() {
        let coord = two_tasks();
        assert_eq!(state(&seeded(&coord, 0).unwrap())["model"], "sonnet");
        assert_eq!(state(&seeded(&coord, 1).unwrap())["model"], "opus");
        let bare = Coordinator::parse(
            r#"{"project":"p","blurb":"b","goal":"g.md","tasks":[{"task":"b-one","title":"One","scenarios":["S1","S2","S10"]}]}"#,
        )
        .unwrap();
        assert!(state(&seeded(&bare, 0).unwrap()).get("model").is_none());
    }

    // Pins the brief's dependency lines, both directions, and the description quoted
    // line by line.
    #[test]
    fn the_brief_names_what_depends_on_the_task_and_what_it_must_not_break() {
        let coord = two_tasks();
        let first = seeded(&coord, 0).unwrap();
        let brief = state(&first)["brief"].as_str().unwrap().to_owned();
        assert!(brief.contains("- depended on by: `gh-7-two` — Two"));
        assert!(brief.contains("- depends on: nothing"));
        assert!(!brief.contains("must not break"));
        let second = seeded(&coord, 1).unwrap();
        let brief = state(&second)["brief"].as_str().unwrap().to_owned();
        assert!(
            brief.contains("- must not break: the scenarios of the tasks it depends on: S2, S1")
        );
        assert!(brief.contains("> one\n>\n> two"));
    }

    // Pins that a second seed is refused with the file byte-identical, that `--force`
    // replaces it, and that an unknown task is named.
    #[test]
    fn run_refuses_a_seeded_task_and_leaves_it_untouched() {
        let root = std::env::temp_dir().join(format!("hcoord-seed-{}", std::process::id()));
        let coordinator = root.join("p/coordinator/b");
        std::fs::create_dir_all(&coordinator).unwrap();
        std::fs::create_dir_all(root.join("p/planning/b")).unwrap();
        std::fs::write(root.join("p/planning/b/goal.md"), GOAL).unwrap();
        std::fs::write(
            coordinator.join("tasks.json"),
            r#"{"project":"p","blurb":"b","goal":"../../planning/b/goal.md","tasks":[
                {"task":"b-one","title":"One","scenarios":["S1","S2","S10"]}]}"#,
        )
        .unwrap();
        let state_path = root.join("p/b/b-one/build-task.json");

        assert!(try_run(&coordinator, "b-one", false).is_ok());
        let first = std::fs::read(&state_path).unwrap();
        let e = try_run(&coordinator, "b-one", false).unwrap_err();
        assert!(e.contains("already seeded"), "{e}");
        assert_eq!(std::fs::read(&state_path).unwrap(), first);
        assert!(try_run(&coordinator, "b-one", true).is_ok());
        assert!(
            try_run(&coordinator, "b-nope", false)
                .unwrap_err()
                .contains("no task b-nope")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Pins that a list with no `goal` is refused and no notes directory is created.
    #[test]
    fn a_list_without_a_goal_is_refused() {
        let root = std::env::temp_dir().join(format!("hcoord-seed-nogoal-{}", std::process::id()));
        let coordinator = root.join("p/coordinator/b");
        std::fs::create_dir_all(&coordinator).unwrap();
        std::fs::write(
            coordinator.join("tasks.json"),
            r#"{"project":"p","blurb":"b","tasks":[{"task":"b-one","title":"One","scenarios":["S1"]}]}"#,
        )
        .unwrap();
        let e = try_run(&coordinator, "b-one", false).unwrap_err();
        assert!(e.contains("names no goal"), "{e}");
        assert!(!root.join("p/b").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }
}
