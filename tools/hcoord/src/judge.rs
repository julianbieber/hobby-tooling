//! The judgement of a finished run: the order its scenarios are judged in, what is shown
//! for each, and the file the verdicts are recorded in.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::capture;
use crate::clock;
use crate::goal::{self, Goal, Scenario};
use crate::model::{BuildState, Coordinator, Status, TaskEntry};

const JUDGEMENT_FILE: &str = "judgement.md";
const FOLLOW_UPS: &str = "Follow-ups";

/// What the person who judged said about one scenario.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The scenario did what it said.
    Ok,
    /// The scenario did something other than what it said.
    Wrong,
    /// The scenario did the right thing with the wrong default.
    Tune,
}

impl Verdict {
    /// The verdict spelled `ok`, `wrong` or `tune`, lowercase only.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "ok" => Some(Verdict::Ok),
            "wrong" => Some(Verdict::Wrong),
            "tune" => Some(Verdict::Tune),
            _ => None,
        }
    }

    /// The spelling `parse` reads back.
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::Wrong => "wrong",
            Verdict::Tune => "tune",
        }
    }
}

/// One recorded judgement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The goal's scenario id.
    pub id: String,
    /// The scenario's title as the file recorded it.
    pub title: String,
    pub verdict: Verdict,
    /// The answer as given, without the `> ` quoting the file puts in front of each line.
    /// Empty for an `Ok` that came without words.
    pub words: String,
}

/// A parsed `judgement.md`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Judgement {
    pub blocks: Vec<Block>,
    /// The lines under `## Follow-ups`, each without its `- ` marker and shaped
    /// `<scenario id>: <text>`.
    pub follow_ups: Vec<String>,
}

/// One answer to be written into the file.
pub struct Entry<'a> {
    pub id: &'a str,
    pub verdict: Verdict,
    /// Kept verbatim except for trailing newlines.
    pub words: &'a str,
    /// One line proposing the task this answer implies; stored as `<id>: <line>`.
    pub follow_up: Option<&'a str>,
}

/// Read a `judgement.md`. Never fails: a heading it cannot read is skipped, along with
/// what is quoted under it, so a file edited by hand stays writable.
pub fn parse(text: &str) -> Judgement {
    #[derive(PartialEq)]
    enum Mode {
        Outside,
        Block,
        FollowUps,
    }

    fn flush(judgement: &mut Judgement, quoted: &mut Vec<&str>) {
        if let Some(block) = judgement.blocks.last_mut()
            && block.words.is_empty()
        {
            block.words = quoted.join("\n");
        }
        quoted.clear();
    }

    let mut judgement = Judgement::default();
    let mut quoted: Vec<&str> = Vec::new();
    let mut mode = Mode::Outside;
    for line in body(text).lines() {
        if let Some(head) = line.strip_prefix("## ") {
            flush(&mut judgement, &mut quoted);
            if head.trim() == FOLLOW_UPS {
                mode = Mode::FollowUps;
            } else if let Some(block) = block_from_heading(head) {
                judgement.blocks.push(block);
                mode = Mode::Block;
            } else {
                mode = Mode::Outside;
            }
            continue;
        }
        match mode {
            Mode::Block => {
                if line == ">" {
                    quoted.push("");
                } else if let Some(words) = line.strip_prefix("> ") {
                    quoted.push(words);
                }
            }
            Mode::FollowUps => {
                if let Some(item) = line.strip_prefix("- ").map(str::trim_end)
                    && !item.is_empty()
                {
                    judgement.follow_ups.push(item.to_owned());
                }
            }
            Mode::Outside => {}
        }
    }
    flush(&mut judgement, &mut quoted);
    judgement
}

/// The text of `judgement.md` after recording `entry` in `existing`, which is `None` for
/// a run not yet judged. Blocks are laid out in the goal's scenario order however they
/// were judged, `## Follow-ups` comes last and only when there is one, and `created` is
/// kept from `existing` and is `today` otherwise.
///
/// Recording a scenario again replaces its block and drops the follow-up its earlier
/// answer had; `entry.follow_up` then supplies the new one. Refuses, changing nothing,
/// when the goal has no scenario `entry.id`, when a `Wrong` or a `Tune` comes without
/// words — the follow-up and the tuning commit are both made from them — or when the
/// follow-up is empty or spans lines.
pub fn record(
    existing: Option<&str>,
    blurb: &str,
    today: &str,
    goal: &Goal,
    entry: &Entry,
) -> Result<String, String> {
    if !goal.scenarios.iter().any(|s| s.id == entry.id) {
        return Err(format!("no scenario {} in the goal", entry.id));
    }
    let words = entry.words.trim_end_matches(['\n', '\r']);
    if entry.verdict != Verdict::Ok && words.trim().is_empty() {
        return Err(format!(
            "a {} needs your words: what you saw, or the default you want",
            entry.verdict.as_str()
        ));
    }
    if let Some(line) = entry.follow_up
        && (line.trim().is_empty() || line.contains('\n'))
    {
        return Err("a follow-up is one non-empty line".to_owned());
    }

    let mut judgement = existing.map(parse).unwrap_or_default();
    let created = existing.and_then(created_of).unwrap_or(today);
    let prefix = format!("{}: ", entry.id);
    judgement.blocks.retain(|b| b.id != entry.id);
    judgement.follow_ups.retain(|f| !f.starts_with(&prefix));
    judgement.blocks.push(Block {
        id: entry.id.to_owned(),
        title: String::new(),
        verdict: entry.verdict,
        words: words.to_owned(),
    });
    if let Some(line) = entry.follow_up {
        judgement
            .follow_ups
            .push(format!("{prefix}{}", line.trim()));
    }
    Ok(render(blurb, created, goal, &judgement))
}

/// `hcoord judge list <coordinator notes dir>`: print the scenarios to judge, hand ones
/// first and each group in the goal's order, one line each with the verdict already
/// recorded, the owning task, the screen the scenario names and where its capture is.
/// Above them: the goal's path, the path of `judgement.md`, and the tasks not yet done.
/// Exits 0 whether or not tasks are unfinished — whether to judge such a run is the
/// caller's to decide.
///
/// Exits 1 with `hcoord judge: <reason>` on stderr when `tasks.json` is unreadable,
/// names no `goal`, or the goal cannot be read.
pub fn run_list(dir: &Path) -> ExitCode {
    report(load(dir).map(|l| {
        list_text(
            &l.dir,
            &l.coord,
            &l.goal_path,
            &l.goal,
            &parse(l.existing.as_deref().unwrap_or("")),
        )
    }))
}

/// `hcoord judge show <coordinator notes dir> <scenario>`: print the scenario exactly as
/// `hcoord goal show` does, then every screen it names in the goal's words, then one line
/// saying where its captured screen is or why there is none.
///
/// Exits 1 with `hcoord judge: <reason>` on stderr when the goal has no such scenario, or
/// for the reasons `run_list` does.
pub fn run_show(dir: &Path, id: &str) -> ExitCode {
    report(load(dir).and_then(|l| show_text(&l.dir, &l.coord, &l.goal, id)))
}

/// `hcoord judge record <coordinator notes dir> <scenario> <ok|wrong|tune>`: write the
/// verdict into `judgement.md` in that directory, creating the file when it is absent,
/// and print one line saying what was recorded. `words_from` is a file, or `-` for
/// standard input, holding the answer; `follow_up` is the one line a `wrong` proposes.
///
/// Exits 1 with `hcoord judge: <reason>` on stderr, leaving the file as it was, when the
/// verdict is not one of the three, when `record` refuses the entry, when the words
/// cannot be read, or for the reasons `run_list` does.
pub fn run_record(
    dir: &Path,
    id: &str,
    verdict: &str,
    words_from: Option<&Path>,
    follow_up: Option<&str>,
) -> ExitCode {
    report(try_record(dir, id, verdict, words_from, follow_up))
}

struct Loaded {
    dir: PathBuf,
    coord: Coordinator,
    goal_path: PathBuf,
    goal: Goal,
    existing: Option<String>,
}

struct Item<'a> {
    scenario: &'a Scenario,
    task: Option<&'a TaskEntry>,
    capture: Option<PathBuf>,
}

impl Item<'_> {
    fn captured(&self) -> Option<&Path> {
        self.capture.as_deref().filter(|p| p.exists())
    }
}

fn report(result: Result<String, String>) -> ExitCode {
    match result {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("hcoord judge: {e}");
            ExitCode::from(1)
        }
    }
}

fn load(dir: &Path) -> Result<Loaded, String> {
    let dir = dir
        .canonicalize()
        .map_err(|e| format!("cannot read {}: {e}", dir.display()))?;
    let coord = Coordinator::load(&dir)?;
    let goal_rel = coord
        .goal
        .as_deref()
        .ok_or("tasks.json names no goal; there is nothing to judge")?;
    let goal_path = dir
        .join(goal_rel)
        .canonicalize()
        .map_err(|e| format!("cannot read goal {}: {e}", dir.join(goal_rel).display()))?;
    let goal_text = std::fs::read_to_string(&goal_path)
        .map_err(|e| format!("cannot read {}: {e}", goal_path.display()))?;
    let path = dir.join(JUDGEMENT_FILE);
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    Ok(Loaded {
        goal: goal::parse(&goal_text),
        dir,
        coord,
        goal_path,
        existing,
    })
}

fn try_record(
    dir: &Path,
    id: &str,
    verdict: &str,
    words_from: Option<&Path>,
    follow_up: Option<&str>,
) -> Result<String, String> {
    let verdict = Verdict::parse(verdict)
        .ok_or_else(|| format!("the verdict is ok, wrong or tune, not {verdict:?}"))?;
    let words = match words_from {
        Some(path) => capture::read_text(path)?,
        None => String::new(),
    };
    let loaded = load(dir)?;
    let text = record(
        loaded.existing.as_deref(),
        &loaded.coord.blurb,
        &clock::rfc3339_utc(clock::now_unix())[..10],
        &loaded.goal,
        &Entry {
            id,
            verdict,
            words: &words,
            follow_up,
        },
    )?;
    let path = loaded.dir.join(JUDGEMENT_FILE);
    std::fs::write(&path, text).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let mut line = format!("recorded {id} {} in {}", verdict.as_str(), path.display());
    if follow_up.is_some() {
        line.push_str(", follow-up");
    }
    Ok(line)
}

fn items<'a>(dir: &Path, coord: &'a Coordinator, goal: &'a Goal) -> Vec<Item<'a>> {
    let mut items: Vec<Item<'a>> = goal
        .scenarios
        .iter()
        .map(|scenario| {
            let task = coord
                .tasks
                .iter()
                .find(|t| t.scenarios.contains(&scenario.id));
            let capture = task.map(|t| capture_path(&coord.task_notes_dir(dir, t), &scenario.id));
            Item {
                scenario,
                task,
                capture,
            }
        })
        .collect();
    items.sort_by_key(|i| !i.scenario.hand);
    items
}

fn capture_path(task_notes: &Path, id: &str) -> PathBuf {
    let recorded = BuildState::load(task_notes)
        .ok()
        .flatten()
        .and_then(|state| state.acceptance_result)
        .and_then(|result| {
            result
                .checks
                .into_iter()
                .find(|c| c.scenario.as_deref() == Some(id))
        })
        .and_then(|check| check.screen);
    match recorded {
        Some(screen) => task_notes.join(screen),
        None => task_notes.join("screens").join(format!("{id}.png")),
    }
}

fn status_name(status: Status) -> &'static str {
    match status {
        Status::Todo => "todo",
        Status::Running => "running",
        Status::AwaitingHuman => "awaiting-human",
        Status::AwaitingMerge => "awaiting-merge",
        Status::Done => "done",
        Status::Blocked => "blocked",
    }
}

fn list_text(
    dir: &Path,
    coord: &Coordinator,
    goal_path: &Path,
    goal: &Goal,
    judged: &Judgement,
) -> String {
    let items = items(dir, coord, goal);
    let hand = items.iter().filter(|i| i.scenario.hand).count();
    let unfinished: Vec<String> = coord
        .tasks
        .iter()
        .filter(|t| t.status != Status::Done)
        .map(|t| format!("{} ({})", t.task, status_name(t.status)))
        .collect();

    let rows: Vec<[String; 6]> = items
        .iter()
        .map(|item| {
            let verdict = judged
                .blocks
                .iter()
                .find(|b| b.id == item.scenario.id)
                .map_or("—", |b| b.verdict.as_str());
            [
                item.scenario.id.clone(),
                if item.scenario.hand { "hand" } else { "" }.to_owned(),
                format!("judged={verdict}"),
                format!("task={}", item.task.map_or("—", |t| t.task.as_str())),
                format!("screen={}", item.scenario.screen.as_deref().unwrap_or("—")),
                format!(
                    "capture={}",
                    item.captured()
                        .map_or("missing".to_owned(), |p| p.display().to_string())
                ),
            ]
        })
        .collect();
    let widths: Vec<usize> = (0..5)
        .map(|col| {
            rows.iter()
                .map(|r| r[col].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let mut out = format!(
        "{} ({}) — {} scenarios, {hand} hand\n",
        coord.blurb,
        coord.project,
        items.len()
    );
    out.push_str(&format!("goal:       {}\n", goal_path.display()));
    out.push_str(&format!(
        "judgement:  {}\n",
        dir.join(JUDGEMENT_FILE).display()
    ));
    out.push_str(&format!(
        "unfinished: {}",
        if unfinished.is_empty() {
            "none".to_owned()
        } else {
            unfinished.join(", ")
        }
    ));
    for row in &rows {
        let mut line = String::new();
        for (col, cell) in row.iter().enumerate() {
            line.push_str(cell);
            if col < 5 {
                line.push_str(&" ".repeat(widths[col] - cell.chars().count() + 2));
            }
        }
        out.push('\n');
        out.push_str(&line);
    }
    out
}

fn show_text(dir: &Path, coord: &Coordinator, goal: &Goal, id: &str) -> Result<String, String> {
    let scenario = goal::show_text(goal, &[id.to_owned()])
        .map_err(|_| format!("no scenario {id} in the goal"))?;
    let mut out = scenario.trim_end().to_owned();
    let scenario = goal
        .scenarios
        .iter()
        .find(|s| s.id == id)
        .ok_or_else(|| format!("no scenario {id} in the goal"))?;
    if let Some(name) = scenario.screen.as_deref() {
        for screen in goal.screens.iter().filter(|s| s.name == name) {
            out.push_str("\n\n");
            out.push_str(screen.text.trim_end());
        }
    }
    let item = items(dir, coord, goal)
        .into_iter()
        .find(|i| i.scenario.id == id);
    let capture = match item.as_ref() {
        Some(i) => match (i.captured(), i.task) {
            (Some(path), _) => format!("capture: {}", path.display()),
            (None, Some(task)) => {
                format!("capture: none — {} left no capture for it", task.task)
            }
            (None, None) => "capture: none — no task owns this scenario".to_owned(),
        },
        None => "capture: none — no task owns this scenario".to_owned(),
    };
    out.push_str("\n\n");
    out.push_str(&capture);
    Ok(out)
}

fn render(blurb: &str, created: &str, goal: &Goal, judgement: &Judgement) -> String {
    let mut out = format!(
        "---\ntitle: \"{blurb} — judgement\"\ncreated: {created}\ntags: [judgement]\n---\n"
    );
    let in_goal = goal
        .scenarios
        .iter()
        .filter_map(|s| judgement.blocks.iter().find(|b| b.id == s.id));
    let outside_goal = judgement
        .blocks
        .iter()
        .filter(|b| !goal.scenarios.iter().any(|s| s.id == b.id));
    for block in in_goal.chain(outside_goal) {
        let title = goal
            .scenarios
            .iter()
            .find(|s| s.id == block.id)
            .map_or(block.title.as_str(), |s| s.title.as_str());
        let head = if title.is_empty() {
            block.id.clone()
        } else {
            format!("{} {title}", block.id)
        };
        out.push_str(&format!("\n## {head} — {}\n", block.verdict.as_str()));
        if !block.words.is_empty() {
            out.push('\n');
            for line in block.words.lines() {
                if line.is_empty() {
                    out.push_str(">\n");
                } else {
                    out.push_str(&format!("> {line}\n"));
                }
            }
        }
    }
    if !judgement.follow_ups.is_empty() {
        out.push_str(&format!("\n## {FOLLOW_UPS}\n\n"));
        for line in &judgement.follow_ups {
            out.push_str(&format!("- {line}\n"));
        }
    }
    out
}

fn body(text: &str) -> &str {
    let Some(rest) = text.strip_prefix("---\n") else {
        return text;
    };
    match rest.find("\n---\n") {
        Some(end) => &rest[end + "\n---\n".len()..],
        None => rest.strip_suffix("\n---").map_or(text, |_| ""),
    }
}

fn created_of(text: &str) -> Option<&str> {
    let front = text.strip_prefix("---\n")?;
    let front = &front[..front.find("\n---")?];
    front
        .lines()
        .find_map(|l| l.strip_prefix("created:"))
        .map(str::trim)
        .filter(|d| !d.is_empty())
}

fn block_from_heading(head: &str) -> Option<Block> {
    let (name, verdict) = head.trim().rsplit_once(" — ")?;
    let verdict = Verdict::parse(verdict.trim())?;
    let (id, title) = name.trim().split_once(' ').unwrap_or((name.trim(), ""));
    if id.is_empty() {
        return None;
    }
    Some(Block {
        id: id.to_owned(),
        title: title.trim().to_owned(),
        verdict,
        words: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOAL: &str = "\
## Intent
> the goal
## Screens
### main (exists)
    +---+
### main (target)
    +---+ target
### note.md (target)
    ---
    title: sketch
## Scenarios
### S1 open it                          screen: main
    1. `run`
    -> it opens
    widgets: none new
### S2 add one                          screen: main
    1. click
    -> it adds
    hand
    widgets: none new
### S3 rename it                        screen: note.md
    1. click
    -> it renames
    widgets: none new
### S4 keep it                          screen: main
    1. restart
    -> it is kept
    hand
    widgets: none new
### S5 leave it
    1. quit
    -> it quits
    widgets: none new
## Widgets
| widget | exists | provided by |
|--------|--------|-------------|
";

    fn coordinator(tasks: &str) -> Coordinator {
        Coordinator::parse(&format!(
            r#"{{"project":"p","blurb":"b","goal":"g.md","tasks":{tasks}}}"#
        ))
        .unwrap()
    }

    fn two_tasks() -> Coordinator {
        coordinator(
            r#"[{"task":"b-one","title":"One","scenarios":["S1","S2","S3"],"status":"done"},
                {"task":"b-two","title":"Two","scenarios":["S4"],"status":"todo"}]"#,
        )
    }

    fn goal() -> Goal {
        goal::parse(GOAL)
    }

    fn entry<'a>(id: &'a str, verdict: Verdict, words: &'a str) -> Entry<'a> {
        Entry {
            id,
            verdict,
            words,
            follow_up: None,
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("hcoord-judge-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("p/coordinator/b")).unwrap();
        root
    }

    fn ids<'a>(items: &'a [Item]) -> Vec<&'a str> {
        items.iter().map(|i| i.scenario.id.as_str()).collect()
    }

    // Pins that the scenarios only a person can settle come first, so the running app is
    // spent on them, and that neither group is reordered.
    #[test]
    fn hand_scenarios_are_judged_first_and_then_the_goals_own_order() {
        let (coord, goal) = (two_tasks(), goal());
        let items = items(Path::new("/n/p/coordinator/b"), &coord, &goal);
        assert_eq!(ids(&items), ["S2", "S4", "S1", "S3", "S5"]);
    }

    // Pins that the capture a run recorded beats the derived path, and that a task with
    // no state file still resolves to the convention.
    #[test]
    fn a_captured_screen_is_taken_from_the_state_file_before_the_convention() {
        let root = scratch("capture");
        let dir = root.join("p/coordinator/b");
        let one = root.join("p/b/b-one");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::write(
            one.join("build-task.json"),
            r#"{"phase":"done","acceptance_result":{"passed":true,"checks":[
                {"scenario":"S2","state":"hand","screen":"screens/other.png"}]}}"#,
        )
        .unwrap();
        let (coord, goal) = (two_tasks(), goal());
        let items = items(&dir, &coord, &goal);
        let capture = |id: &str| {
            items
                .iter()
                .find(|i| i.scenario.id == id)
                .and_then(|i| i.capture.clone())
        };
        assert_eq!(capture("S2"), Some(one.join("screens/other.png")));
        assert_eq!(capture("S1"), Some(one.join("screens/S1.png")));
        assert_eq!(
            capture("S4"),
            Some(root.join("p/b/b-two/screens/S4.png")),
            "no state file at all"
        );
        assert_eq!(capture("S5"), None, "no owner");
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Pins the two things the skill decides from the listing: which tasks have not
    // finished, and which scenarios already carry a verdict.
    #[test]
    fn the_list_names_the_unfinished_tasks_and_the_verdicts_already_recorded() {
        let (coord, goal) = (two_tasks(), goal());
        let judged = parse(
            &record(
                None,
                "b",
                "2026-09-21",
                &goal,
                &entry("S1", Verdict::Ok, ""),
            )
            .unwrap(),
        );
        let text = list_text(
            Path::new("/n/p/coordinator/b"),
            &coord,
            Path::new("/n/goal.md"),
            &goal,
            &judged,
        );
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "b (p) — 5 scenarios, 2 hand");
        assert_eq!(lines[1], "goal:       /n/goal.md");
        assert_eq!(lines[3], "unfinished: b-two (todo)");
        assert!(
            lines[4].starts_with("S2  hand  judged=—   task=b-one"),
            "{}",
            lines[4]
        );
        let s1 = lines.iter().find(|l| l.starts_with("S1 ")).unwrap();
        assert!(s1.contains("judged=ok"), "{s1}");
        let s5 = lines.iter().find(|l| l.starts_with("S5 ")).unwrap();
        assert!(s5.contains("task=—") && s5.contains("screen=—"), "{s5}");
    }

    // Pins that show prints the scenario, both drawings of its screen in the goal's words
    // and one capture line, and says why when the capture is not there.
    #[test]
    fn show_prints_the_scenario_then_its_screens_then_where_the_picture_is() {
        let root = scratch("show");
        let dir = root.join("p/coordinator/b");
        let one = root.join("p/b/b-one/screens");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::write(one.join("S2.png"), b"png").unwrap();
        let (coord, goal) = (two_tasks(), goal());

        let text = show_text(&dir, &coord, &goal, "S2").unwrap();
        let heading = text.find("### S2 add one    screen: main").unwrap();
        let exists = text.find("### main (exists)").unwrap();
        let target = text.find("### main (target)\n    +---+ target").unwrap();
        assert!(heading < exists && exists < target, "{text}");
        assert!(text.contains("    -> it adds"));
        assert!(
            text.ends_with(&format!("capture: {}", one.join("S2.png").display())),
            "{text}"
        );

        let missing = show_text(&dir, &coord, &goal, "S4").unwrap();
        assert!(
            missing.ends_with("capture: none — b-two left no capture for it"),
            "{missing}"
        );
        let unowned = show_text(&dir, &coord, &goal, "S5").unwrap();
        assert!(
            unowned.ends_with("capture: none — no task owns this scenario"),
            "{unowned}"
        );
        assert!(
            show_text(&dir, &coord, &goal, "S99")
                .unwrap_err()
                .contains("S99")
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Pins the file's format end to end: what `record` writes, `parse` reads back with
    // the ids, verdicts and words unchanged, a two-paragraph answer included.
    #[test]
    fn a_recorded_verdict_round_trips_through_the_file() {
        let goal = goal();
        let long =
            "the new one lands last\n\nI expected it beside the selected one\n  indented > quoted";
        let mut file = record(
            None,
            "b",
            "2026-09-21",
            &goal,
            &entry("S1", Verdict::Ok, ""),
        )
        .unwrap();
        file = record(
            Some(&file),
            "b",
            "2026-09-22",
            &goal,
            &Entry {
                follow_up: Some("the new one lands beside the selected one"),
                ..entry("S3", Verdict::Wrong, long)
            },
        )
        .unwrap();
        file = record(
            Some(&file),
            "b",
            "2026-09-22",
            &goal,
            &entry("S4", Verdict::Tune, "scale default 6"),
        )
        .unwrap();

        assert!(
            file.starts_with(
                "---\ntitle: \"b — judgement\"\ncreated: 2026-09-21\ntags: [judgement]\n---\n"
            ),
            "{file}"
        );
        let parsed = parse(&file);
        let seen: Vec<(&str, Verdict, &str)> = parsed
            .blocks
            .iter()
            .map(|b| (b.id.as_str(), b.verdict, b.words.as_str()))
            .collect();
        assert_eq!(
            seen,
            [
                ("S1", Verdict::Ok, ""),
                ("S3", Verdict::Wrong, long),
                ("S4", Verdict::Tune, "scale default 6"),
            ]
        );
        assert_eq!(parsed.blocks[1].title, "rename it");
        assert_eq!(
            parsed.follow_ups,
            ["S3: the new one lands beside the selected one"]
        );
    }

    // Pins that answering a scenario twice leaves one block, and that the follow-up its
    // first answer proposed goes with it instead of piling up.
    #[test]
    fn re_recording_a_scenario_replaces_its_block() {
        let goal = goal();
        let first = record(
            None,
            "b",
            "d",
            &goal,
            &Entry {
                follow_up: Some("one"),
                ..entry("S1", Verdict::Wrong, "bad")
            },
        )
        .unwrap();
        let again = record(
            Some(&first),
            "b",
            "d",
            &goal,
            &Entry {
                follow_up: Some("two"),
                ..entry("S1", Verdict::Wrong, "still bad")
            },
        )
        .unwrap();
        let parsed = parse(&again);
        assert_eq!(parsed.blocks.len(), 1);
        assert_eq!(parsed.blocks[0].words, "still bad");
        assert_eq!(parsed.follow_ups, ["S1: two"]);

        let settled = record(Some(&again), "b", "d", &goal, &entry("S1", Verdict::Ok, "")).unwrap();
        let parsed = parse(&settled);
        assert_eq!(parsed.blocks.len(), 1);
        assert!(parsed.follow_ups.is_empty());
        assert!(!settled.contains("Follow-ups"));
    }

    // Pins that the file reads as a report on the goal, so blocks follow the goal's
    // order even though hand scenarios are judged first.
    #[test]
    fn blocks_are_written_in_the_goals_order_whatever_order_they_were_judged_in() {
        let goal = goal();
        let mut file = None;
        for id in ["S4", "S1", "S3"] {
            file = Some(
                record(
                    file.as_deref(),
                    "b",
                    "d",
                    &goal,
                    &entry(id, Verdict::Ok, ""),
                )
                .unwrap(),
            );
        }
        let file = file.unwrap();
        let headings: Vec<&str> = file.lines().filter(|l| l.starts_with("## ")).collect();
        assert_eq!(
            headings,
            [
                "## S1 open it — ok",
                "## S3 rename it — ok",
                "## S4 keep it — ok"
            ]
        );
    }

    // Pins the refusals: a verdict whose words are missing, an id the goal lacks and a
    // follow-up that is not one line each fail without producing any text to write.
    #[test]
    fn a_verdict_that_needs_words_and_has_none_is_refused() {
        let goal = goal();
        for verdict in [Verdict::Wrong, Verdict::Tune] {
            let e = record(None, "b", "d", &goal, &entry("S1", verdict, " \n")).unwrap_err();
            assert!(e.contains("needs your words"), "{e}");
        }
        assert!(
            record(None, "b", "d", &goal, &entry("S99", Verdict::Ok, ""))
                .unwrap_err()
                .contains("S99")
        );
        let two_lines = Entry {
            follow_up: Some("a\nb"),
            ..entry("S1", Verdict::Wrong, "bad")
        };
        assert!(record(None, "b", "d", &goal, &two_lines).is_err());
    }

    // Pins that a file edited by hand stays writable: an unreadable heading and its quote
    // are skipped, the readable blocks survive and the original `created` is kept.
    #[test]
    fn a_hand_edited_file_that_no_longer_parses_is_read_as_far_as_it_goes() {
        let goal = goal();
        let edited = "---\ntitle: \"b — judgement\"\ncreated: 2026-01-02\ntags: [judgement]\n---\n\n\
                      ## S1 open it — ok\n\n## S2 my own heading\n\n> lost\n\n\
                      ## S3 rename it — wrong\n> kept\n>\n> both\n\n## Follow-ups\n\n- S3: x\n- \n";
        let parsed = parse(edited);
        let ids: Vec<&str> = parsed.blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["S1", "S3"]);
        assert_eq!(parsed.blocks[1].words, "kept\n\nboth");
        let next = record(
            Some(edited),
            "b",
            "2026-09-21",
            &goal,
            &entry("S4", Verdict::Ok, ""),
        )
        .unwrap();
        assert!(next.contains("created: 2026-01-02"), "{next}");
        assert!(!next.contains("lost"));
        assert_eq!(parse(&next).blocks.len(), 3);
    }

    // Pins the command as the skill drives it: a refused answer leaves the file byte for
    // byte as it was, and a list with no goal is named as the reason nothing is judged.
    #[test]
    fn the_command_leaves_the_file_alone_when_it_refuses() {
        let root = scratch("run");
        let dir = root.join("p/coordinator/b");
        std::fs::create_dir_all(root.join("p/planning/b")).unwrap();
        std::fs::write(root.join("p/planning/b/goal.md"), GOAL).unwrap();
        std::fs::write(
            dir.join("tasks.json"),
            r#"{"project":"p","blurb":"b","goal":"../../planning/b/goal.md","tasks":[
                {"task":"b-one","title":"One","scenarios":["S1"],"status":"done"}]}"#,
        )
        .unwrap();
        let words = root.join("words.txt");
        std::fs::write(&words, "too slow\n").unwrap();

        let line = try_record(&dir, "S1", "wrong", Some(&words), Some("make it fast")).unwrap();
        assert!(
            line.starts_with("recorded S1 wrong in ") && line.ends_with(", follow-up"),
            "{line}"
        );
        let before = std::fs::read(dir.join("judgement.md")).unwrap();
        assert!(try_record(&dir, "S1", "tune", None, None).is_err());
        assert!(try_record(&dir, "S1", "bad", None, None).is_err());
        assert!(try_record(&dir, "S99", "ok", None, None).is_err());
        assert_eq!(std::fs::read(dir.join("judgement.md")).unwrap(), before);

        std::fs::write(
            dir.join("tasks.json"),
            r#"{"project":"p","blurb":"b","tasks":[]}"#,
        )
        .unwrap();
        assert!(load(&dir).err().unwrap().contains("names no goal"));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
