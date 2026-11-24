//! The view drawn into a test buffer and driven by synthetic keys and clicks.

use std::path::PathBuf;

use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

use super::*;
use crate::app::Action;
use crate::model::launch::Mode;
use crate::model::project::Project;
use crate::model::task::{AspectRef, GitInfo, OpenItem, Pr, PrStatus, Repo, Todo, TodoStatus};
use crate::session::Session;
use crate::testkit::{find, row, screen};
use crate::widgets::{ACCENT, SIDEBAR_COLS};

/// A present, adopted task named `name` in project `wusel`, topic `alpha`, with nothing loaded.
pub(crate) fn task(name: &str) -> Task {
    let id = TaskId {
        project: "wusel".into(),
        topic: "alpha".into(),
        name: name.into(),
    };
    let dir = PathBuf::from("/nonexistent").join(id.rel());
    Task {
        repos: vec![Repo {
            path: dir.clone(),
            git: GitInfo::default(),
        }],
        id,
        dir,
        adopted: true,
        present: true,
        done: false,
        block: None,
        backlog: None,
        title: None,
        has_branch: true,
        prs: Vec::new(),
        aspects: Vec::new(),
        last_active: None,
        phase: None,
        todo: Vec::new(),
        waiting_on: None,
        session_name: String::new(),
    }
}

/// A view over `tasks` and the one project `wusel`, with a checkout; nothing runs in the background.
pub(crate) fn view(tasks: Vec<Task>) -> DashView {
    DashView::with_model(Model {
        tasks,
        projects: vec![Project {
            name: "wusel".into(),
            path: PathBuf::from("/nonexistent/wusel"),
            has_checkout: true,
        }],
    })
}

fn frame(view: &mut DashView) -> Buffer {
    let area = Rect::new(0, 0, 160, 50);
    let mut buf = Buffer::empty(area);
    view.draw(area, &mut buf);
    buf
}

fn click(view: &mut DashView, (x, y): (u16, u16)) -> Option<Action> {
    let at = |kind| MouseEvent {
        kind,
        column: x,
        row: y,
        modifiers: KeyModifiers::NONE,
    };
    view.mouse(at(MouseEventKind::Down(MouseButton::Left)));
    view.mouse(at(MouseEventKind::Up(MouseButton::Left))).1
}

fn press(view: &mut DashView, code: KeyCode) -> Option<Action> {
    let mut key = KeyEvent::new(code, KeyModifiers::NONE);
    key.kind = KeyEventKind::Press;
    view.key(key)
}

fn id(name: &str) -> TaskId {
    task(name).id
}

fn summary(buf: &Buffer) -> String {
    let (_, y) = find(buf, "Work", None, SIDEBAR_COLS);
    row(buf, y)
}

#[test]
fn todo_line_shows_progress_and_expands() {
    let mut a = task("task-a");
    a.waiting_on = Some("answer GATE C".into());
    a.todo = [
        ("write the test", TodoStatus::Done),
        ("fix it", TodoStatus::Active),
        ("ship it", TodoStatus::Pending),
    ]
    .into_iter()
    .map(|(text, status)| Todo {
        text: text.into(),
        status,
    })
    .collect();
    let mut view = view(vec![a]);

    let buf = frame(&mut view);
    let shown = screen(&buf);
    assert!(shown.contains("▶ fix it"), "{shown}");
    assert!(shown.contains("waiting: answer GATE C"), "{shown}");
    assert!(!shown.contains("ship it"), "collapsed by default: {shown}");

    let toggle = find(&buf, "[▸ Todo (1/3)]", None, 0);
    assert_eq!(click(&mut view, toggle), None);
    let shown = screen(&frame(&mut view));
    assert!(shown.contains("▾ Todo (1/3)"), "{shown}");
    assert!(shown.contains("✓ write the test"), "{shown}");
    assert!(shown.contains("○ ship it"), "{shown}");
}

#[test]
fn clicks_run_the_control_drawn_under_the_pointer() {
    let mut a = task("task-a");
    a.prs = vec![Pr {
        url: "https://github.com/o/r/pull/1".into(),
        repo: None,
        title: None,
        opened: None,
        status: PrStatus::Open,
        number: Some(1),
        live_title: Some("Fix it".into()),
    }];
    let mut b = task("task-b");
    b.present = false;
    let mut view = view(vec![a.clone(), b.clone()]);

    let buf = frame(&mut view);
    let (_, body_y) = find(&buf, "task-b", None, SIDEBAR_COLS);
    assert_eq!(click(&mut view, (150, body_y + 1)), None);
    let buf = frame(&mut view);
    let (_, by) = find(&buf, "task-b", None, SIDEBAR_COLS);
    assert_eq!(
        buf[(SIDEBAR_COLS + 1, by - 1)].fg,
        ACCENT,
        "the clicked card is selected"
    );

    let toggle = find(&buf, "[▸ PRs (1)]", None, 0);
    assert_eq!(click(&mut view, toggle), None);
    let buf = frame(&mut view);
    assert!(screen(&buf).contains("▾ PRs (1)"));
    let pr = find(&buf, "#1 Fix it", None, 0);
    assert_eq!(
        click(&mut view, pr),
        Some(Action::OpenUrl("https://github.com/o/r/pull/1".into()))
    );

    let buf = frame(&mut view);
    let open = find(&buf, "[Open hcdev]", Some("task-a"), 0);
    assert_eq!(
        click(&mut view, open),
        Some(Action::Hcdev(a.clone(), Mode::Editor))
    );
    let buf = frame(&mut view);
    let restore = find(&buf, "[Restore]", Some("task-b"), 0);
    assert_eq!(
        click(&mut view, restore),
        Some(Action::Restore(b.clone(), Mode::Editor))
    );
    let buf = frame(&mut view);
    let claude = find(&buf, "[Claude mode]", Some("task-b"), 0);
    assert_eq!(
        click(&mut view, claude),
        Some(Action::Restore(b.clone(), Mode::Claude))
    );
    let buf = frame(&mut view);
    let claude = find(&buf, "[Claude mode]", Some("task-a"), 0);
    assert_eq!(
        click(&mut view, claude),
        Some(Action::Hcdev(a.clone(), Mode::Claude))
    );

    let buf = frame(&mut view);
    let hide = find(&buf, "[Hide]", Some("task-a"), 0);
    assert_eq!(click(&mut view, hide), None);
    let buf = frame(&mut view);
    assert!(!screen(&buf).contains("task-a"));
    assert!(summary(&buf).contains("· 1 dismissed"));

    let project = find(&buf, "wusel", None, 1);
    assert!(project.0 < SIDEBAR_COLS);
    click(&mut view, project);
    assert!(summary(&frame(&mut view)).contains("wusel  ·"));

    let buf = frame(&mut view);
    let chip = find(&buf, "[removed 1]", None, SIDEBAR_COLS);
    click(&mut view, chip);
    assert!(summary(&frame(&mut view)).contains("· 1 shown"));
}

#[test]
fn navigation_keys_move_selection_and_button_focus() {
    let mut view = view(vec![task("n1"), task("n2"), task("n3")]);
    frame(&mut view);
    press(&mut view, KeyCode::Char('ä'));
    press(&mut view, KeyCode::Char('ä'));
    press(&mut view, KeyCode::Char('ü'));
    press(&mut view, KeyCode::Char('ö'));
    press(&mut view, KeyCode::Char('ö'));
    let n2 = view.task(&id("n2")).unwrap().clone();
    assert_eq!(press(&mut view, KeyCode::Enter), Some(Action::Notes(n2)));
}

#[test]
fn selection_follows_its_task_across_a_resort() {
    let mut view = view(vec![task("n1"), task("n2"), task("n3"), task("n4")]);
    frame(&mut view);
    press(&mut view, KeyCode::Char('ä'));
    press(&mut view, KeyCode::Char('ä'));
    let hcdev = |view: &DashView, name: &str| {
        Some(Action::Hcdev(
            view.task(&id(name)).unwrap().clone(),
            Mode::Editor,
        ))
    };

    view.sessions.tasks.insert(
        id("n3"),
        Session {
            state: SessionState::Idle,
            since: 0,
        },
    );
    let buf = frame(&mut view);
    let (_, top) = find(&buf, "n3", None, SIDEBAR_COLS);
    let (_, next) = find(&buf, "n1", None, SIDEBAR_COLS);
    assert!(top < next, "the idle task sorts first:\n{}", screen(&buf));
    assert!(screen(&buf).contains("idle "));
    assert!(summary(&buf).contains("· 1 waiting on you"));
    assert_eq!(press(&mut view, KeyCode::Enter), hcdev(&view, "n3"));

    press(&mut view, KeyCode::Char('ä'));
    view.ui.hidden.insert(id("n1"));
    frame(&mut view);
    assert_eq!(press(&mut view, KeyCode::Enter), hcdev(&view, "n2"));
}

#[test]
fn too_small_draws_no_controls() {
    let mut view = view(vec![task("n1")]);
    frame(&mut view);
    let area = Rect::new(0, 0, 50, 8);
    let mut buf = Buffer::empty(area);
    view.draw(area, &mut buf);
    assert!(screen(&buf).contains("terminal too small"));
    assert_eq!(click(&mut view, (1, 1)), None);
    assert_eq!(press(&mut view, KeyCode::Enter), None);
}

#[test]
fn the_primary_button_follows_the_stage() {
    let mut orphan = task("stray");
    orphan.adopted = false;
    let mut fresh = task("fresh");
    fresh.present = false;
    fresh.has_branch = false;
    let mut view = view(vec![orphan, fresh.clone()]);
    let buf = frame(&mut view);
    find(&buf, "[Adopt]", Some("stray"), 0);
    let start = find(&buf, "[Start task]", Some("fresh"), 0);
    assert_eq!(
        click(&mut view, start),
        Some(Action::Restore(fresh, Mode::Editor))
    );
}

#[test]
fn aspect_lines_show_state_and_open_their_tab() {
    let mut a = task("task-a");
    a.aspects = vec![AspectRef {
        name: "wgpu-limits".into(),
        kind: "research".into(),
        state: "reported".into(),
        goal: Some("which limits apply".into()),
        ..AspectRef::default()
    }];
    let mut view = view(vec![a.clone()]);
    view.sessions.aspects.insert(
        (a.id.clone(), "wgpu-limits".into()),
        Session {
            state: SessionState::Idle,
            since: 0,
        },
    );

    let buf = frame(&mut view);
    assert!(screen(&buf).contains("Aspects (1)"), "{}", screen(&buf));
    let (_, line_y) = find(&buf, "⌕ wgpu-limits", None, 0);
    assert!(
        row(&buf, line_y).contains("idle "),
        "the aspect line carries its own badge: {}",
        row(&buf, line_y)
    );
    assert!(
        !screen(&buf).contains("task-a  idle"),
        "the task's own badge stays empty"
    );
    let line = find(
        &buf,
        "⌕ wgpu-limits · research · reported · which limits apply",
        None,
        0,
    );
    assert_eq!(
        click(&mut view, line),
        Some(Action::Tab(a, "wgpu-limits".into()))
    );
}

#[test]
fn an_aspect_shows_its_phase_pr_and_expands_to_plan_outcome_and_open_items() {
    let mut a = task("task-a");
    let url = "https://github.com/o/r/pull/11";
    a.aspects = vec![AspectRef {
        name: "export".into(),
        kind: "code".into(),
        state: "pr".into(),
        created: Some("2026-11-08".into()),
        pr: Some(Pr {
            status: PrStatus::Merged,
            ..Pr::from_url(url.into())
        }),
        outcome: Some("merged the export".into()),
        open_items: vec![
            OpenItem {
                text: "web loading".into(),
                issue: None,
            },
            OpenItem {
                text: "tiff".into(),
                issue: Some("tiff-export".into()),
            },
        ],
        phase: Some("await-fix".into()),
        todo: vec![
            Todo {
                text: "plan it".into(),
                status: TodoStatus::Done,
            },
            Todo {
                text: "build it".into(),
                status: TodoStatus::Active,
            },
        ],
        waiting_on: Some("red check".into()),
        ..AspectRef::default()
    }];
    let mut view = view(vec![a]);

    let buf = frame(&mut view);
    let (_, y) = find(&buf, "⌕ export", None, 0);
    let line = row(&buf, y);
    for part in ["· await-fix", "#11 merged", "waiting: red check"] {
        assert!(line.contains(part), "{part} missing: {line}");
    }
    assert!(!screen(&buf).contains("outcome:"), "collapsed by default");

    let chip = find(&buf, "#11 merged", None, 0);
    assert_eq!(click(&mut view, chip), Some(Action::OpenUrl(url.into())));

    let buf = frame(&mut view);
    let toggle = find(&buf, "[▸ todo 1/2 · 1 open item]", None, 0);
    assert_eq!(click(&mut view, toggle), None);
    let shown = screen(&frame(&mut view));
    for part in [
        "created 2026-11-08 · base default branch",
        "▶ build it",
        "outcome: merged the export",
        "○ web loading",
        "→ tiff-export: tiff",
    ] {
        assert!(shown.contains(part), "{part} missing:\n{shown}");
    }
}
