//! A goal file — the UX spec a run is judged against — read into its screens,
//! scenarios and widgets, and the checks that hold between them.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use crate::model::Coordinator;

/// One `### <name> (<qualifier>)` under `## Screens`, with the heading and everything
/// beneath it kept verbatim so a brief can quote the drawing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Screen {
    pub name: String,
    pub qualifier: String,
    pub line: usize,
    pub text: String,
}

/// One `- **<handle>** …` bullet under `## Decisions`, with its indented continuation
/// lines, kept verbatim.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub handle: String,
    pub text: String,
    pub line: usize,
}

impl Decision {
    /// Whether `id` — a scenario id or a task name — occurs in the block as a whole
    /// word, so `S1` is not found in `S10` and a task name is not found in a longer one.
    pub fn names(&self, id: &str) -> bool {
        let word = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        self.text.match_indices(id).any(|(at, _)| {
            let before = self.text[..at].chars().next_back();
            let after = self.text[at + id.len()..].chars().next();
            !before.is_some_and(word) && !after.is_some_and(word)
        })
    }
}

/// One `### S<n> <title>    screen: <name>` under `## Scenarios`, with its body
/// kept verbatim so a brief can quote it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub screen: Option<String>,
    pub line: usize,
    pub steps: usize,
    pub arrows: usize,
    pub hand: bool,
    pub widgets: Vec<String>,
    pub text: String,
}

/// One row of the `## Widgets` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Widget {
    pub name: String,
    pub exists: String,
    pub provided_by: String,
    pub line: usize,
}

/// The parsed file. Sections the format does not name are kept out; they are
/// neither checked nor an error.
#[derive(Clone, Debug, Default)]
pub struct Goal {
    pub sections: BTreeSet<String>,
    pub intent: String,
    pub decisions: Vec<Decision>,
    pub screens: Vec<Screen>,
    pub scenarios: Vec<Scenario>,
    pub widgets: Vec<Widget>,
    pub open_markers: Vec<usize>,
}

/// One lint finding, anchored to a line of the goal file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub message: String,
}

const REQUIRED_SECTIONS: [&str; 5] = ["Intent", "Decisions", "Screens", "Scenarios", "Widgets"];

/// Parse the goal's text. Never fails: a malformed file parses to a `Goal` whose
/// `lint` names what is wrong, so one pass reports everything at once.
pub fn parse(text: &str) -> Goal {
    let mut goal = Goal::default();
    let mut section = String::new();
    let mut current: Option<Scenario> = None;
    let mut in_table_header = false;
    let mut in_screen = false;
    let mut in_decision = false;
    let mut intent = String::new();

    for (index, raw) in text.lines().enumerate() {
        let line = index + 1;
        let trimmed = raw.trim();

        if let Some(name) = raw.strip_prefix("## ") {
            if let Some(s) = current.take() {
                goal.scenarios.push(s);
            }
            in_screen = false;
            in_decision = false;
            section = name.split('(').next().unwrap_or("").trim().to_string();
            goal.sections.insert(section.clone());
            in_table_header = section == "Widgets";
            continue;
        }

        if has_open_marker(raw) {
            goal.open_markers.push(line);
        }

        match section.as_str() {
            "Intent" => {
                intent.push_str(raw);
                intent.push('\n');
            }
            "Decisions" => {
                if let Some(rest) = raw.strip_prefix("- **") {
                    let handle = rest.split("**").next().unwrap_or("").to_string();
                    goal.decisions.push(Decision {
                        handle,
                        text: format!("{raw}\n"),
                        line,
                    });
                    in_decision = true;
                } else if trimmed.is_empty() || raw.starts_with("- ") {
                    in_decision = false;
                } else if in_decision && let Some(d) = goal.decisions.last_mut() {
                    d.text.push_str(raw);
                    d.text.push('\n');
                }
            }
            "Screens" => {
                if let Some(head) = raw.strip_prefix("### ") {
                    let (name, qualifier) = split_qualifier(head);
                    goal.screens.push(Screen {
                        name,
                        qualifier,
                        line,
                        text: format!("{raw}\n"),
                    });
                    in_screen = true;
                } else if in_screen && let Some(s) = goal.screens.last_mut() {
                    s.text.push_str(raw);
                    s.text.push('\n');
                }
            }
            "Scenarios" => {
                if let Some(head) = raw.strip_prefix("### ") {
                    if let Some(s) = current.take() {
                        goal.scenarios.push(s);
                    }
                    current = Some(scenario_head(head, line));
                } else if let Some(s) = current.as_mut() {
                    s.text.push_str(raw);
                    s.text.push('\n');
                    if is_step(trimmed) {
                        s.steps += 1;
                    } else if trimmed.starts_with("->") {
                        s.arrows += 1;
                    } else if is_hand(trimmed) {
                        s.hand = true;
                    } else if let Some(list) = trimmed.strip_prefix("widgets:") {
                        s.widgets = widget_names(list);
                    }
                }
            }
            "Widgets" if trimmed.starts_with('|') => {
                let cells: Vec<&str> = trimmed
                    .trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .collect();
                if in_table_header {
                    in_table_header = false;
                    continue;
                }
                if cells
                    .iter()
                    .all(|c| c.chars().all(|ch| ch == '-' || ch == ':'))
                {
                    continue;
                }
                if cells.len() >= 3 {
                    goal.widgets.push(Widget {
                        name: cells[0].to_lowercase(),
                        exists: cells[1].to_lowercase(),
                        provided_by: cells[2].to_string(),
                        line,
                    });
                }
            }
            _ => {}
        }
    }
    if let Some(s) = current.take() {
        goal.scenarios.push(s);
    }
    for screen in &mut goal.screens {
        screen.text = trim_blank_lines(&screen.text);
    }
    goal.intent = trim_blank_lines(&intent);
    goal
}

fn trim_blank_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|l| !l.trim().is_empty());
    let end = lines.iter().rposition(|l| !l.trim().is_empty());
    match (start, end) {
        (Some(a), Some(b)) => lines[a..=b].iter().map(|l| format!("{l}\n")).collect(),
        _ => String::new(),
    }
}

/// Every check that holds within one goal file. Empty means clean.
///
/// A section or screen heading may carry a parenthetical after its name —
/// `## Decisions (my words, verbatim)`, `### editor.main (target)` — which is not
/// part of the name. The five required sections are present; every screen has a qualifier; every
/// scenario names a screen that exists, has at least one step and exactly one
/// `->`, and names only widgets the table lists; a widget that does not exist
/// names what provides it; no `?` is left anywhere outside backticks.
pub fn lint(goal: &Goal) -> Vec<Finding> {
    let mut out = Vec::new();
    for name in REQUIRED_SECTIONS {
        if !goal.sections.contains(name) {
            out.push(Finding {
                line: 0,
                message: format!("missing section `## {name}`"),
            });
        }
    }
    for line in &goal.open_markers {
        out.push(Finding {
            line: *line,
            message: "open question: a `?` is still here".into(),
        });
    }

    let mut screen_names = BTreeSet::new();
    for screen in &goal.screens {
        screen_names.insert(screen.name.clone());
        if screen.qualifier.is_empty() {
            out.push(Finding {
                line: screen.line,
                message: format!(
                    "screen `{}` says neither (exists) nor (target)",
                    screen.name
                ),
            });
        }
    }

    let widget_names: BTreeSet<&str> = goal.widgets.iter().map(|w| w.name.as_str()).collect();
    let mut seen_ids = BTreeSet::new();
    for s in &goal.scenarios {
        if !seen_ids.insert(s.id.clone()) {
            out.push(Finding {
                line: s.line,
                message: format!("scenario id `{}` is used twice", s.id),
            });
        }
        match &s.screen {
            None => out.push(Finding {
                line: s.line,
                message: format!("{} names no `screen:`", s.id),
            }),
            Some(name) if !screen_names.contains(name) => out.push(Finding {
                line: s.line,
                message: format!(
                    "{} names screen `{name}`, which `## Screens` does not have",
                    s.id
                ),
            }),
            _ => {}
        }
        if s.steps == 0 {
            out.push(Finding {
                line: s.line,
                message: format!("{} has no numbered step", s.id),
            });
        }
        if s.arrows != 1 {
            out.push(Finding {
                line: s.line,
                message: format!(
                    "{} has {} `->` lines; exactly one is the observation",
                    s.id, s.arrows
                ),
            });
        }
        for w in &s.widgets {
            if !widget_names.contains(w.as_str()) {
                out.push(Finding {
                    line: s.line,
                    message: format!(
                        "{} names widget `{w}`, which `## Widgets` does not list",
                        s.id
                    ),
                });
            }
        }
    }

    for w in &goal.widgets {
        let exists = w.exists.starts_with("yes");
        let provided = !w.provided_by.is_empty() && w.provided_by != "—" && w.provided_by != "-";
        if !exists && !provided {
            out.push(Finding {
                line: w.line,
                message: format!(
                    "widget `{}` does not exist and names nothing that provides it",
                    w.name
                ),
            });
        }
    }

    out.sort_by_key(|f| f.line);
    out
}

/// The checks between a goal and the `tasks.json` split from it: every scenario is
/// owned by exactly one task, and every task owns at least one scenario.
pub fn lint_tasks(goal: &Goal, tasks: &Coordinator) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut owners: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for task in &tasks.tasks {
        if task.scenarios.is_empty() {
            out.push(Finding {
                line: 0,
                message: format!("task `{}` owns no scenario", task.task),
            });
        }
        for id in &task.scenarios {
            owners
                .entry(id.as_str())
                .or_default()
                .push(task.task.as_str());
        }
    }
    for s in &goal.scenarios {
        match owners.get(s.id.as_str()).map(Vec::as_slice) {
            None => out.push(Finding {
                line: s.line,
                message: format!("{} is owned by no task", s.id),
            }),
            Some([_]) => {}
            Some(many) => out.push(Finding {
                line: s.line,
                message: format!(
                    "{} is owned by {} tasks: {}",
                    s.id,
                    many.len(),
                    many.join(", ")
                ),
            }),
        }
    }
    let ids: BTreeSet<&str> = goal.scenarios.iter().map(|s| s.id.as_str()).collect();
    for (id, tasks) in &owners {
        if !ids.contains(id) {
            out.push(Finding {
                line: 0,
                message: format!(
                    "task `{}` owns `{id}`, which the goal does not have",
                    tasks[0]
                ),
            });
        }
    }
    out
}

/// `hcoord goal lint <goal.md> [--tasks <tasks.json>]`: print each finding as
/// `<file>:<line>: <message>` and exit 1 when there is one; exit 0 with a one-line
/// tally when clean; exit 1 naming the file when either cannot be read.
pub fn run_lint(goal_path: &Path, tasks_path: Option<&Path>) -> ExitCode {
    let text = match std::fs::read_to_string(goal_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {e}", goal_path.display());
            return ExitCode::from(1);
        }
    };
    let goal = parse(&text);
    let mut findings = lint(&goal);
    if let Some(path) = tasks_path {
        match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Coordinator>(&t).map_err(|e| e.to_string()))
        {
            Ok(tasks) => findings.extend(lint_tasks(&goal, &tasks)),
            Err(e) => {
                eprintln!("cannot read {}: {e}", path.display());
                return ExitCode::from(1);
            }
        }
    }
    if findings.is_empty() {
        let hand = goal.scenarios.iter().filter(|s| s.hand).count();
        println!(
            "ok: {} screens, {} scenarios ({hand} hand), {} widgets",
            goal.screens.len(),
            goal.scenarios.len(),
            goal.widgets.len()
        );
        return ExitCode::SUCCESS;
    }
    for f in &findings {
        if f.line == 0 {
            println!("{}: {}", goal_path.display(), f.message);
        } else {
            println!("{}:{}: {}", goal_path.display(), f.line, f.message);
        }
    }
    ExitCode::from(1)
}

/// The text `hcoord goal show` prints for `ids`: per id, in the order asked, the
/// scenario's heading reconstructed as `### <id> <title>    screen: <name>` and its
/// body verbatim. `Err` names every id the goal lacks, and nothing is rendered then.
pub fn show_text(goal: &Goal, ids: &[String]) -> Result<String, Vec<String>> {
    let mut out = String::new();
    let mut missing = Vec::new();
    for id in ids {
        match goal.scenarios.iter().find(|s| &s.id == id) {
            Some(s) => {
                let screen = s.screen.as_deref().unwrap_or("");
                out.push_str(&format!("### {} {}    screen: {screen}\n", s.id, s.title));
                out.push_str(&s.text);
            }
            None => missing.push(id.clone()),
        }
    }
    if missing.is_empty() {
        Ok(out)
    } else {
        Err(missing)
    }
}

/// `hcoord goal show <goal.md> <id>...`: print the named scenarios verbatim,
/// heading and body, in the order asked; exit 1 naming any id the goal lacks.
pub fn run_show(goal_path: &Path, ids: &[String]) -> ExitCode {
    let text = match std::fs::read_to_string(goal_path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read {}: {e}", goal_path.display());
            return ExitCode::from(1);
        }
    };
    match show_text(&parse(&text), ids) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(missing) => {
            eprintln!("no such scenario: {}", missing.join(", "));
            ExitCode::from(1)
        }
    }
}

fn split_qualifier(head: &str) -> (String, String) {
    let head = head.trim();
    match head.find('(') {
        Some(open) => {
            let name = head[..open].trim().to_string();
            let rest = &head[open + 1..];
            let qualifier = rest.split(')').next().unwrap_or("").trim().to_string();
            (name, qualifier)
        }
        None => (head.to_string(), String::new()),
    }
}

fn scenario_head(head: &str, line: usize) -> Scenario {
    let (left, screen) = match head.find("screen:") {
        Some(at) => (
            &head[..at],
            Some(head[at + "screen:".len()..].trim().to_string()),
        ),
        None => (head, None),
    };
    let mut words = left.split_whitespace();
    let id = words.next().unwrap_or("").to_string();
    let title = words.collect::<Vec<_>>().join(" ");
    Scenario {
        id,
        title,
        screen: screen.filter(|s| !s.is_empty()),
        line,
        steps: 0,
        arrows: 0,
        hand: false,
        widgets: Vec::new(),
        text: String::new(),
    }
}

fn is_hand(trimmed: &str) -> bool {
    trimmed == "hand"
        || trimmed
            .strip_prefix("hand")
            .is_some_and(|rest| rest.trim_start().starts_with("<-"))
}

fn is_step(trimmed: &str) -> bool {
    let digits: String = trimmed.chars().take_while(char::is_ascii_digit).collect();
    !digits.is_empty() && trimmed[digits.len()..].starts_with(". ")
}

fn widget_names(list: &str) -> Vec<String> {
    let list = list.trim();
    if list.is_empty() || list.starts_with("none") {
        return Vec::new();
    }
    list.split(',')
        .map(|item| {
            let item = item.trim();
            let name = item.split('(').next().unwrap_or(item).trim();
            name.to_lowercase()
        })
        .filter(|n| !n.is_empty())
        .collect()
}

fn has_open_marker(raw: &str) -> bool {
    let mut in_code = false;
    for ch in raw.chars() {
        match ch {
            '`' => in_code = !in_code,
            '?' if !in_code => return true,
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLEAN: &str = "\
## Intent
> words
## Decisions
- **a** — \"said\"
## Screens
### editor.main (exists)
    +---+
### editor.main (target)
    +---+
    changes: x
## Scenarios
### S1 open it                          screen: editor.main
    1. `cd there && run`
    -> the list shows it
    widgets: none new
### S2 add one                          screen: editor.main
    1. click [+ add]
    2. type `x`, Enter
    hand
    -> a file exists
    widgets: text input (in feathers), button
## Widgets
| widget     | exists | provided by |
|------------|--------|-------------|
| text input | no     | S2 |
| button     | yes    | — |
";

    // The whole point of the lint is that a complete goal passes it; every
    // other test is a deviation from this file, so it has to be clean first.
    #[test]
    fn a_complete_goal_is_clean() {
        let goal = parse(CLEAN);
        assert_eq!(lint(&goal), Vec::new());
        assert_eq!(goal.screens.len(), 2);
        assert_eq!(goal.scenarios.len(), 2);
        assert!(goal.scenarios[1].hand);
        assert_eq!(goal.scenarios[1].steps, 2);
    }

    // A heading's parenthetical is annotation, not name: the real goal writes
    // `## Decisions (my words, verbatim)` and must still count as `Decisions`.
    #[test]
    fn a_heading_parenthetical_is_not_part_of_the_name() {
        let text = CLEAN.replace("## Decisions", "## Decisions (my words, verbatim)");
        assert_eq!(lint(&parse(&text)), Vec::new());
    }

    // `hand` is a marker on its own line, optionally followed by a `<-` aside; a
    // step or observation that happens to continue onto a line starting with the
    // word must not turn a scriptable scenario into one nobody runs.
    #[test]
    fn hand_is_only_the_marker_on_its_own_line() {
        let prose = CLEAN.replace("    hand\n", "    hand checks are counted\n");
        assert!(!parse(&prose).scenarios[1].hand);
        let aside = CLEAN.replace("    hand\n", "    hand      <- nobody can script it\n");
        assert!(parse(&aside).scenarios[1].hand);
    }

    // A `?` is how an unanswered question is marked while the goal is being
    // written; a goal with one left is not finished and must not seed a run.
    #[test]
    fn an_open_marker_fails() {
        let text = CLEAN.replace("-> the list shows it", "-> ? the list shows it");
        let f = lint(&parse(&text));
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("open question"));
    }

    // A `?` inside backticks is code, not a question — `[ -f x ] && echo ok?`
    // would otherwise be unwritable in a step.
    #[test]
    fn a_marker_inside_code_is_not_open() {
        let text = CLEAN.replace("`cd there && run`", "`run 'why?'`");
        assert_eq!(lint(&parse(&text)), Vec::new());
    }

    // The phantom-slider case: a scenario naming a widget the table does not
    // list is the one that was built twice against a control that did not exist.
    #[test]
    fn a_widget_missing_from_the_table_fails() {
        let text = CLEAN.replace("widgets: none new", "widgets: drag slider");
        let f = lint(&parse(&text));
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("drag slider"));
    }

    // A widget that does not exist and names no provider is work nobody owns.
    #[test]
    fn a_missing_widget_needs_a_provider() {
        let text = CLEAN.replace(
            "| text input | no     | S2 |",
            "| text input | no     | — |",
        );
        let f = lint(&parse(&text));
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("provides"));
    }

    // A scenario is one observation; two `->` lines are two scenarios, none is a
    // step list with nothing to check.
    #[test]
    fn exactly_one_arrow_per_scenario() {
        let none = CLEAN.replace("    -> the list shows it\n", "");
        assert_eq!(lint(&parse(&none)).len(), 1);
        let two = CLEAN.replace(
            "    -> a file exists\n",
            "    -> a file exists\n    -> twice\n",
        );
        assert_eq!(lint(&parse(&two)).len(), 1);
    }

    // A scenario on a screen the goal never drew is the panel-overlap case: the
    // build had nothing to place the control against.
    #[test]
    fn a_scenario_needs_a_drawn_screen() {
        let text = CLEAN.replace(
            "screen: editor.main\n    1. click",
            "screen: editor.other\n    1. click",
        );
        let f = lint(&parse(&text));
        assert_eq!(f.len(), 1);
        assert!(f[0].message.contains("editor.other"));
    }

    // The split is checked from both sides: an orphan scenario is work that will
    // not be built, a task owning none has no acceptance at all.
    #[test]
    fn every_scenario_has_exactly_one_owner() {
        let goal = parse(CLEAN);
        let tasks: Coordinator = serde_json::from_str(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"b-one","title":"t","scenarios":["S1","S2"]},
                {"task":"b-two","title":"t","scenarios":["S2"]},
                {"task":"b-three","title":"t"}]}"#,
        )
        .unwrap();
        let f = lint_tasks(&goal, &tasks);
        let messages: Vec<&str> = f.iter().map(|f| f.message.as_str()).collect();
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(messages.iter().any(|m| m.contains("S2 is owned by 2")));
        assert!(messages.iter().any(|m| m.contains("b-three")));
    }

    // `show` is what seeds a brief; the body has to come back byte for byte or the
    // brief paraphrases after all.
    #[test]
    fn a_scenario_body_is_kept_verbatim() {
        let goal = parse(CLEAN);
        assert_eq!(
            goal.scenarios[0].text,
            "    1. `cd there && run`\n    -> the list shows it\n    widgets: none new\n"
        );
    }

    // Pins `show_text` as the one renderer behind `run_show` and `seed`: order as asked,
    // heading reconstructed, and every missing id named.
    #[test]
    fn show_text_renders_in_the_order_asked_and_names_what_is_missing() {
        let goal = parse(CLEAN);
        let out = show_text(&goal, &["S2".into(), "S1".into()]).unwrap();
        assert!(out.starts_with("### S2 add one    screen: editor.main\n    1. click [+ add]\n"));
        assert!(out.contains("### S1 open it    screen: editor.main\n    1. `cd there && run`\n"));
        assert_eq!(
            show_text(&goal, &["S1".into(), "S9".into()]),
            Err(vec!["S9".to_string()])
        );
    }

    // Pins that a decision keeps its indented continuation lines and that a blank line
    // or the next bullet ends it.
    #[test]
    fn a_decision_keeps_its_continuation_lines_and_a_blank_line_ends_it() {
        let text = CLEAN.replace(
            "- **a** — \"said\"\n",
            "- **a** — \"said\n  and more\"\n- **b** — \"next\"\n\nstray\n",
        );
        let goal = parse(&text);
        assert_eq!(goal.decisions.len(), 2);
        assert_eq!(goal.decisions[0].handle, "a");
        assert_eq!(goal.decisions[0].text, "- **a** — \"said\n  and more\"\n");
        assert_eq!(goal.decisions[1].text, "- **b** — \"next\"\n");
    }

    // Pins `names` as whole-word: `S1` is not found in `S10`, nor `b-one` in `b-one-more`.
    #[test]
    fn names_matches_whole_words_only() {
        let d = Decision {
            handle: "h".into(),
            text: "- **h** — S10 and b-one-more, but also (S2).\n".into(),
            line: 1,
        };
        assert!(!d.names("S1"));
        assert!(!d.names("b-one"));
        assert!(d.names("S10"));
        assert!(d.names("S2"));
    }

    // Pins a screen's `text` as its heading and drawing up to the next heading, without
    // trailing blank lines, and the intent body likewise.
    #[test]
    fn a_screen_keeps_its_drawing_up_to_the_next_heading() {
        let goal = parse(CLEAN);
        assert_eq!(
            goal.screens[0].text,
            "### editor.main (exists)\n    +---+\n"
        );
        assert_eq!(
            goal.screens[1].text,
            "### editor.main (target)\n    +---+\n    changes: x\n"
        );
        assert_eq!(goal.intent, "> words\n");
    }
}
