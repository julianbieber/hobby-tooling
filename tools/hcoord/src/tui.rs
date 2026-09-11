//! The pane beside the coordinator's Claude: `tasks.json`, redrawn whenever the file
//! changes, narrow enough for a 15% split.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{DefaultTerminal, Frame};

use crate::model::{Coordinator, Status, TASKS_FILE};

const POLL: Duration = Duration::from_millis(500);

struct App {
    dir: PathBuf,
    loaded: Result<Coordinator, String>,
    seen: Option<SystemTime>,
    scroll: u16,
    show_description: bool,
}

impl App {
    fn new(dir: &Path) -> Self {
        let mut app = Self {
            dir: dir.to_path_buf(),
            loaded: Err("not loaded".into()),
            seen: None,
            scroll: 0,
            show_description: false,
        };
        app.reload();
        app
    }

    fn mtime(&self) -> Option<SystemTime> {
        std::fs::metadata(Coordinator::tasks_path(&self.dir))
            .ok()
            .and_then(|m| m.modified().ok())
    }

    fn reload(&mut self) {
        self.seen = self.mtime();
        self.loaded = Coordinator::load(&self.dir);
    }

    fn reload_if_changed(&mut self) {
        if self.mtime() != self.seen {
            self.reload();
        }
    }
}

/// Run the view on the coordinator notes directory until `q` or Esc. The terminal is
/// restored on every exit path, including a draw error.
pub fn run(dir: &Path) -> Result<(), String> {
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, App::new(dir));
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal, mut app: App) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| draw(frame, &app))
            .map_err(|e| e.to_string())?;
        if event::poll(POLL).map_err(|e| e.to_string())?
            && let Event::Key(key) = event::read().map_err(|e| e.to_string())?
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                KeyCode::Char('j') | KeyCode::Down => app.scroll = app.scroll.saturating_add(1),
                KeyCode::Char('k') | KeyCode::Up => app.scroll = app.scroll.saturating_sub(1),
                KeyCode::Char('d') => app.show_description = !app.show_description,
                KeyCode::Char('r') => app.reload(),
                _ => {}
            }
        }
        app.reload_if_changed();
    }
}

fn draw(frame: &mut Frame, app: &App) {
    let coordinator = match &app.loaded {
        Ok(c) => c,
        Err(e) => {
            let msg = Paragraph::new(vec![
                Line::from(TASKS_FILE.bold()),
                Line::from(""),
                Line::from(e.as_str().fg(Color::Red)),
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

    let head = Paragraph::new(vec![
        Line::from(coordinator.project.as_str().bold()),
        Line::from(coordinator.blurb.as_str().fg(Color::Cyan)),
        Line::from(coordinator.counts().summary().dim()),
    ]);
    frame.render_widget(head, header);

    let mut lines = Vec::new();
    if app.show_description {
        for l in coordinator.description.lines() {
            lines.push(Line::from(l.to_owned().dim()));
        }
        lines.push(Line::from(""));
    }
    for (i, t) in coordinator.tasks.iter().enumerate() {
        lines.push(task_line(i + 1, t.status, &t.title));
        if let Some(note) = &t.note {
            lines.push(Line::from(format!("    {note}").dim()));
        }
        if let Some(pr) = t.pr.as_deref().and_then(pr_number) {
            lines.push(Line::from(format!("    PR #{pr}").fg(Color::Magenta)));
        }
    }
    if coordinator.tasks.is_empty() {
        lines.push(Line::from("no tasks yet".dim()));
    }
    let list = Paragraph::new(lines)
        .block(Block::default().borders(Borders::TOP))
        .wrap(Wrap { trim: false })
        .scroll((app.scroll, 0));
    frame.render_widget(list, body);

    let keys = Paragraph::new(Line::from("q j k d r".dim()));
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
        Status::Done => Style::default().fg(Color::Green),
        Status::Blocked => Style::default().fg(Color::Red),
    };
    Line::from(vec![
        Span::styled(format!("{} ", status.glyph()), style),
        Span::styled(format!("{index}. "), style.remove_modifier(Modifier::BOLD)),
        Span::styled(title.to_owned(), style),
    ])
}

fn pr_number(url: &str) -> Option<&str> {
    url.rsplit('/')
        .next()
        .filter(|n| n.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // The narrow pane has no room for a URL, so the number is what is shown; a URL
    // that does not end in one must not print a stray path segment.
    #[test]
    fn pr_number_is_the_last_segment_when_numeric() {
        assert_eq!(pr_number("https://github.com/o/r/pull/12"), Some("12"));
        assert_eq!(pr_number("https://github.com/o/r/pull/12/files"), None);
    }
}
