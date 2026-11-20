//! `htab` — the overview pane of a Claude-mode `hcdev` tab. It shows one task or one aspect of
//! a task (the one whose worktree the pane runs in): why it exists, what it builds and where it
//! stops, the steps still open and those done, and its PRs and plan, which a click opens. Every
//! field is read from the notes on disk and re-read every second. It also keeps its own tab named
//! `<name> · <phase>`.

pub mod overview;
pub mod scope;
pub mod view;
pub mod zellij;

use std::io::{self, IsTerminal, Stdout, Write};
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime};

use hwt_core::Roots;
use log::error;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::crossterm::{cursor, execute};
use ratatui::layout::Rect;

use crate::overview::Overview;
use crate::scope::Scope;
use crate::view::{Link, Row};

const USAGE: &str = "usage: htab [--scope <project>/<topic>/<task>[/<aspect>]] [--print <width>]";
const RELOAD: Duration = Duration::from_secs(1);

/// Run the pane for the scope of the current directory, or of `--scope`. `--print <width>`
/// renders once to stdout instead.
pub fn run() -> ExitCode {
    hwt_core::log_to_file("htab", "htab.log");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut scope_arg = None;
    let mut print_width = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--scope" => scope_arg = it.next().cloned(),
            "--print" => print_width = it.next().and_then(|w| w.parse::<usize>().ok()),
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        }
    }
    let roots = Roots::of_user();
    let scope = match scope_arg {
        Some(s) => Scope::parse(&s),
        None => std::env::current_dir()
            .ok()
            .and_then(|d| Scope::of_dir(&roots, &d)),
    };
    let Some(scope) = scope else {
        eprintln!("htab: not in a task or aspect worktree\n{USAGE}");
        return ExitCode::FAILURE;
    };
    if let Some(width) = print_width {
        let o = Overview::load(&roots, &scope);
        let mut out = io::stdout().lock();
        for row in view::rows(&o, width, SystemTime::now()) {
            if writeln!(out, "{}", row.plain().trim_end()).is_err() {
                break;
            }
        }
        return ExitCode::SUCCESS;
    }
    if !io::stdout().is_terminal() {
        eprintln!("htab: needs a terminal (or --print <width>)");
        return ExitCode::FAILURE;
    }
    let mut terminal = match enter_terminal() {
        Ok(t) => t,
        Err(e) => {
            restore_terminal();
            error!("cannot set up the terminal: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = Pane::new(roots, scope).event_loop(&mut terminal);
    restore_terminal();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("terminal error: {e}");
            ExitCode::FAILURE
        }
    }
}

struct Pane {
    roots: Roots,
    scope: Scope,
    overview: Overview,
    loaded: Instant,
    scroll: usize,
    rows: Vec<Row>,
    area: Rect,
    tab: Option<u64>,
    label: Option<String>,
    quit: bool,
}

impl Pane {
    fn new(roots: Roots, scope: Scope) -> Self {
        let overview = Overview::load(&roots, &scope);
        Self {
            roots,
            scope,
            overview,
            loaded: Instant::now(),
            scroll: 0,
            rows: Vec::new(),
            area: Rect::default(),
            tab: None,
            label: None,
            quit: false,
        }
    }

    fn event_loop(&mut self, terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> io::Result<()> {
        self.name_tab();
        while !self.quit {
            terminal.draw(|f| {
                let area = f.area();
                self.draw(area, f.buffer_mut());
            })?;
            if event::poll(Duration::from_millis(200))? {
                self.event(event::read()?);
            }
            if self.loaded.elapsed() >= RELOAD {
                self.reload();
            }
        }
        Ok(())
    }

    fn reload(&mut self) {
        self.overview = Overview::load(&self.roots, &self.scope);
        self.loaded = Instant::now();
        self.name_tab();
    }

    fn name_tab(&mut self) {
        let label = self.overview.tab_label();
        if self.label.as_ref() == Some(&label) {
            return;
        }
        if self.tab.is_none() {
            self.tab = zellij::own_tab_id();
        }
        if let Some(tab) = self.tab {
            zellij::rename_tab(tab, &label);
            self.label = Some(label);
        }
    }

    fn draw(&mut self, area: Rect, buf: &mut Buffer) {
        self.area = area;
        self.rows = view::rows(&self.overview, usize::from(area.width), SystemTime::now());
        let height = usize::from(area.height);
        self.scroll = self.scroll.min(self.rows.len().saturating_sub(height));
        for (y, row) in (area.y..area.bottom()).zip(self.rows.iter().skip(self.scroll)) {
            let mut x = area.x;
            for (text, style) in &row.spans {
                let room = usize::from(area.right().saturating_sub(x));
                let (end, _) = buf.set_stringn(x, y, text, room, *style);
                x = end;
            }
        }
    }

    fn event(&mut self, ev: Event) {
        let page = usize::from(self.area.height).max(1);
        match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => match k.code {
                KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
                KeyCode::Char('j') | KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
                KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
                KeyCode::Char(' ') | KeyCode::PageDown => {
                    self.scroll = self.scroll.saturating_add(page);
                }
                KeyCode::Char('b') | KeyCode::PageUp => {
                    self.scroll = self.scroll.saturating_sub(page);
                }
                KeyCode::Char('g') | KeyCode::Home => self.scroll = 0,
                KeyCode::Char('G') | KeyCode::End => self.scroll = usize::MAX,
                KeyCode::Char('r') => self.reload(),
                _ => {}
            },
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_add(3),
                MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(3),
                MouseEventKind::Down(MouseButton::Left) => {
                    let Some(dy) = m.row.checked_sub(self.area.y) else {
                        return;
                    };
                    let hit = self
                        .rows
                        .get(self.scroll.saturating_add(usize::from(dy)))
                        .and_then(|r| r.link.clone());
                    match hit {
                        Some(Link::Url(url)) => hwt_core::open_url(&url),
                        Some(Link::File(path)) => zellij::open_file(&path),
                        None => {}
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}

fn enter_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        cursor::Hide
    )?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    );
}
