//! The pane beside the coordinator's Claude: `tasks.json`, redrawn whenever the file
//! changes, narrow enough for a 15% split.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::model::{BuildState, Coordinator, LoadError, Status, TASKS_FILE};

const POLL: Duration = Duration::from_millis(500);

struct Loaded {
    coordinator: Coordinator,
    prs: Vec<Vec<String>>,
    acceptance: Vec<Option<(String, bool)>>,
    merge: Vec<Option<(String, bool)>>,
}

struct Row {
    line: Line<'static>,
    url: Option<String>,
}

struct App {
    dir: PathBuf,
    loaded: Result<Loaded, LoadError>,
    seen: Vec<Option<SystemTime>>,
    scroll: u16,
    show_description: bool,
    body: Rect,
}

impl App {
    fn new(dir: &Path) -> Self {
        let mut app = Self {
            dir: dir.to_path_buf(),
            loaded: load(dir),
            seen: Vec::new(),
            scroll: 0,
            show_description: false,
            body: Rect::default(),
        };
        app.seen = app.mtimes();
        app
    }

    fn watched(&self) -> Vec<PathBuf> {
        match &self.loaded {
            Ok(l) => l.coordinator.watched_files(&self.dir),
            Err(_) => vec![Coordinator::tasks_path(&self.dir)],
        }
    }

    fn mtimes(&self) -> Vec<Option<SystemTime>> {
        self.watched()
            .iter()
            .map(|p| std::fs::metadata(p).ok().and_then(|m| m.modified().ok()))
            .collect()
    }

    fn reload(&mut self) {
        self.loaded = load(&self.dir);
        self.seen = self.mtimes();
    }

    fn reload_if_changed(&mut self) {
        if self.mtimes() != self.seen {
            self.reload();
        }
    }

    fn rows(&self) -> Vec<Row> {
        let Ok(loaded) = &self.loaded else {
            return Vec::new();
        };
        let plain = |line: Line<'static>| Row { line, url: None };
        let mut rows = Vec::new();
        if self.show_description {
            for l in loaded.coordinator.description.lines() {
                rows.push(plain(Line::from(l.to_owned().dim())));
            }
            rows.push(plain(Line::from("")));
        }
        for (i, t) in loaded.coordinator.tasks.iter().enumerate() {
            rows.push(plain(task_line(i + 1, t.status, &t.title)));
            if let Some(note) = &t.note {
                rows.push(plain(Line::from(format!("    {note}").dim())));
            }
            if let Some((summary, passed)) = loaded.acceptance.get(i).and_then(Option::as_ref) {
                rows.push(plain(summary_line(summary, *passed)));
            }
            if let Some((summary, merged)) = loaded.merge.get(i).and_then(Option::as_ref) {
                rows.push(plain(summary_line(summary, *merged)));
            }
            for url in loaded.prs.get(i).into_iter().flatten() {
                rows.push(Row {
                    line: pr_line(url),
                    url: Some(url.clone()),
                });
            }
        }
        if loaded.coordinator.tasks.is_empty() {
            rows.push(plain(Line::from("no tasks yet".dim())));
        }
        rows
    }

    fn lines(&self) -> Vec<Line<'static>> {
        self.rows().into_iter().map(|r| r.line).collect()
    }

    fn url_at(&self, column: u16, row: u16) -> Option<String> {
        let inner = Rect {
            y: self.body.y.saturating_add(1),
            height: self.body.height.saturating_sub(1),
            ..self.body
        };
        if !inner.contains(Position { x: column, y: row }) {
            return None;
        }
        let target = usize::from(row - inner.y) + usize::from(self.scroll);
        let mut top = 0;
        for r in self.rows() {
            let height = Paragraph::new(r.line)
                .wrap(Wrap { trim: false })
                .line_count(inner.width);
            if (top..top + height).contains(&target) {
                return r.url;
            }
            top += height;
        }
        None
    }
}

fn load(dir: &Path) -> Result<Loaded, LoadError> {
    Coordinator::load(dir).map(|coordinator| {
        let prs = coordinator
            .tasks
            .iter()
            .map(|t| coordinator.pr_urls(dir, t))
            .collect();
        let acceptance = coordinator
            .tasks
            .iter()
            .map(|t| {
                BuildState::load(&coordinator.task_notes_dir(dir, t))
                    .ok()
                    .flatten()
                    .and_then(|s| s.acceptance_result)
                    .map(|a| (a.summary(), a.passed))
            })
            .collect();
        let merge = coordinator
            .tasks
            .iter()
            .map(|t| {
                BuildState::load(&coordinator.task_notes_dir(dir, t))
                    .ok()
                    .flatten()
                    .and_then(|s| s.merge_result)
                    .map(|m| (m.summary(), m.merged()))
            })
            .collect();
        Loaded {
            coordinator,
            prs,
            acceptance,
            merge,
        }
    })
}

/// Run the view on the coordinator notes directory until `q` or Esc. A left click on a
/// `PR #n` line opens that PR in the browser. The terminal is restored on every exit
/// path, including a draw error.
pub fn run(dir: &Path) -> std::io::Result<()> {
    let mut terminal = ratatui::init();
    let mouse = execute!(std::io::stdout(), EnableMouseCapture).is_ok();
    let result = event_loop(&mut terminal, App::new(dir));
    if mouse {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
    }
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, mut app: App) -> std::io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, &mut app))?;
        if event::poll(POLL)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('j') | KeyCode::Down => scroll_down(&mut app),
                    KeyCode::Char('k') | KeyCode::Up => scroll_up(&mut app),
                    KeyCode::Char('d') => app.show_description = !app.show_description,
                    KeyCode::Char('r') => app.reload(),
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(url) = app.url_at(mouse.column, mouse.row) {
                            open_url(&url);
                        }
                    }
                    MouseEventKind::ScrollDown => scroll_down(&mut app),
                    MouseEventKind::ScrollUp => scroll_up(&mut app),
                    _ => {}
                },
                _ => {}
            }
        }
        app.reload_if_changed();
    }
}

fn scroll_down(app: &mut App) {
    app.scroll = app.scroll.saturating_add(1);
}

fn scroll_up(app: &mut App) {
    app.scroll = app.scroll.saturating_sub(1);
}

fn open_url(url: &str) {
    let _ = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn draw(frame: &mut Frame, app: &mut App) {
    let coordinator = match &app.loaded {
        Ok(l) => &l.coordinator,
        Err(e) => {
            let msg = Paragraph::new(vec![
                Line::from(TASKS_FILE.bold()),
                Line::from(""),
                Line::from(e.to_string().fg(Color::Red)),
                Line::from(""),
                Line::from("q quit · r reload".dim()),
            ])
            .wrap(Wrap { trim: false });
            frame.render_widget(msg, frame.area());
            return;
        }
    };

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    app.body = body;

    let head = Paragraph::new(vec![
        Line::from(coordinator.project.as_str().bold()),
        Line::from(coordinator.blurb.as_str().fg(Color::Cyan)),
        Line::from(coordinator.counts().summary().dim()),
    ]);
    frame.render_widget(head, header);

    let list = Paragraph::new(app.lines())
        .block(Block::default().borders(Borders::TOP))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    frame.render_widget(list, body);

    let keys = Paragraph::new(Line::from("q j k d r · click a PR".dim()));
    frame.render_widget(keys, footer);
}

fn task_line(index: usize, status: Status, title: &str) -> Line<'static> {
    let style = match status {
        Status::Todo => Style::default().fg(Color::Gray),
        Status::Running => Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
        Status::AwaitingHuman => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        Status::AwaitingMerge => Style::default().fg(Color::Cyan),
        Status::Done => Style::default().fg(Color::Green),
        Status::Blocked => Style::default().fg(Color::Red),
    };
    Line::from(vec![
        Span::styled(format!("{} ", status.glyph()), style),
        Span::styled(format!("{index}. "), style.remove_modifier(Modifier::BOLD)),
        Span::styled(title.to_owned(), style),
    ])
}

fn summary_line(summary: &str, ok: bool) -> Line<'static> {
    let colour = if ok { Color::Green } else { Color::Red };
    Line::from(vec![
        Span::raw("    "),
        Span::styled(summary.to_owned(), Style::new().fg(colour)),
    ])
}

fn pr_line(url: &str) -> Line<'static> {
    let label = match pr_number(url) {
        Some(n) => format!("PR #{n}"),
        None => url.to_owned(),
    };
    Line::from(vec![
        Span::raw("    "),
        Span::styled(label, Style::new().fg(Color::Magenta).underlined()),
    ])
}

/// The PR's number, taken from a GitHub PR URL's last path segment. `None` when that
/// segment is not purely digits, as on a URL with a trailing path (`/pull/12/files`).
pub(crate) fn pr_number(url: &str) -> Option<&str> {
    url.rsplit('/')
        .next()
        .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // A click has to land on the PR line as it is drawn, after wrapping and scrolling,
    // or it opens the wrong PR: a title wider than the pane takes two rows.
    #[test]
    fn click_finds_the_pr_under_a_wrapped_title() {
        let coordinator = Coordinator::parse(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"a","title":"a title long enough to wrap twice over"},
                {"task":"b","title":"B"}]}"#,
        )
        .unwrap();
        let mut app = App {
            dir: PathBuf::new(),
            loaded: Ok(Loaded {
                coordinator,
                prs: vec![
                    vec!["https://github.com/o/r/pull/1".into()],
                    vec!["https://github.com/o/r/pull/2".into()],
                ],
                acceptance: vec![None, None],
                merge: vec![None, None],
            }),
            seen: Vec::new(),
            scroll: 0,
            show_description: false,
            body: Rect::new(0, 3, 20, 10),
        };
        let title_rows = u16::try_from(
            Paragraph::new(task_line(
                1,
                Status::Todo,
                "a title long enough to wrap twice over",
            ))
            .wrap(Wrap { trim: false })
            .line_count(20),
        )
        .unwrap();
        assert!(title_rows > 1);
        let first_pr_row = 4 + title_rows;
        assert_eq!(app.url_at(5, first_pr_row - 1), None);
        assert_eq!(
            app.url_at(5, first_pr_row).as_deref(),
            Some("https://github.com/o/r/pull/1")
        );
        assert_eq!(
            app.url_at(5, first_pr_row + 2).as_deref(),
            Some("https://github.com/o/r/pull/2")
        );
        assert_eq!(app.url_at(5, 3), None);
        app.scroll = 1;
        assert_eq!(
            app.url_at(5, first_pr_row - 1).as_deref(),
            Some("https://github.com/o/r/pull/1")
        );
    }

    // The acceptance result is the only thing in this view that comes from a task's own
    // notes rather than from tasks.json, and it is what a coordinator has instead of the
    // task's pane — so it has to reach the list, above the PR and below the note.
    #[test]
    fn a_task_with_an_acceptance_shows_it_under_its_note() {
        let coordinator = Coordinator::parse(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"a","title":"A","note":"acceptance failing"},
                {"task":"b","title":"B"}]}"#,
        )
        .unwrap();
        let app = App {
            dir: PathBuf::new(),
            loaded: Ok(Loaded {
                coordinator,
                prs: vec![vec!["https://github.com/o/r/pull/1".into()], Vec::new()],
                acceptance: vec![Some(("acceptance 1/2 \u{2717} cli".into(), false)), None],
                merge: vec![None, None],
            }),
            seen: Vec::new(),
            scroll: 0,
            show_description: false,
            body: Rect::new(0, 3, 40, 10),
        };
        let rendered: Vec<String> = app
            .lines()
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "\u{b7} 1. A",
                "    acceptance failing",
                "    acceptance 1/2 \u{2717} cli",
                "    PR #1",
                "\u{b7} 2. B",
            ]
        );
    }

    // The hand count has to reach the list itself, not only the wait line, or a
    // coordinator watching the pane cannot tell a run that checked everything from one
    // that left a scenario to a person.
    #[test]
    fn a_hand_check_shows_in_the_tally_under_the_task() {
        let coordinator =
            Coordinator::parse(r#"{"project":"p","blurb":"b","tasks":[{"task":"a","title":"A"}]}"#)
                .unwrap();
        let result: crate::model::AcceptanceResult = serde_json::from_str(
            r#"{"passed":true,"checks":[
                {"scenario":"S1","state":"pass"},
                {"scenario":"S2","state":"pass"},
                {"scenario":"S3","state":"pass"},
                {"scenario":"S6","state":"hand","screen":"screens/S6.png"}]}"#,
        )
        .unwrap();
        let app = App {
            dir: PathBuf::new(),
            loaded: Ok(Loaded {
                coordinator,
                prs: vec![Vec::new()],
                acceptance: vec![Some((result.summary(), result.passed))],
                merge: vec![None],
            }),
            seen: Vec::new(),
            scroll: 0,
            show_description: false,
            body: Rect::new(0, 3, 40, 10),
        };
        let rendered: Vec<String> = app
            .lines()
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "\u{b7} 1. A",
                "    acceptance 3 pass \u{b7} 1 hand \u{2713}"
            ]
        );
    }

    // A blocked merge is what a coordinator has instead of the session's pane when a
    // green PR still could not land — it has to reach the list between the note and the
    // PR link, the same slot the acceptance result takes.
    #[test]
    fn a_task_with_a_blocked_merge_shows_it_between_its_note_and_its_pr() {
        let coordinator = Coordinator::parse(
            r#"{"project":"p","blurb":"b","tasks":[
                {"task":"a","title":"A","note":"checks failing: build"}]}"#,
        )
        .unwrap();
        let app = App {
            dir: PathBuf::new(),
            loaded: Ok(Loaded {
                coordinator,
                prs: vec![vec!["https://github.com/o/r/pull/9".into()]],
                acceptance: vec![None],
                merge: vec![Some(("merge \u{2717} checks failing: build".into(), false))],
            }),
            seen: Vec::new(),
            scroll: 0,
            show_description: false,
            body: Rect::new(0, 3, 40, 10),
        };
        let rendered: Vec<String> = app
            .lines()
            .iter()
            .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "\u{b7} 1. A",
                "    checks failing: build",
                "    merge \u{2717} checks failing: build",
                "    PR #9",
            ]
        );
    }

    // The narrow pane has no room for a URL, so the number is what is shown; a URL
    // that does not end in one must not print a stray path segment.
    #[test]
    fn pr_number_is_the_last_segment_when_numeric() {
        assert_eq!(pr_number("https://github.com/o/r/pull/12"), Some("12"));
        assert_eq!(pr_number("https://github.com/o/r/pull/12/files"), None);
    }
}
