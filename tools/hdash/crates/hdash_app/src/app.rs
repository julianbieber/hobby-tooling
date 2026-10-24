//! The terminal shell around the view: owning the terminal for the life of the process, and
//! running what the view asks for outside itself.

use std::io::{self, IsTerminal, Stdout};
use std::process::{Command, ExitCode, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use hwt_core::SPAWNED_ENV;
use log::{error, warn};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use ratatui::crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::crossterm::{cursor, execute};
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};

use crate::focus::{Launches, activate, find_window};
use crate::model::launch::{
    launch_coordinator, launch_hcdev, launch_restore, launch_terminal, open_notes, open_url,
};
use crate::model::task::Task;
use crate::session::now_secs;
use crate::view::DashView;

const EVENT_WAIT_MS: u64 = 100;
const KEY_HINT: &str = "q quit · / filter · ↑↓ ü ä select · ←→ ö # button · ⏎ run";

/// What a key or click asks the shell to do outside the view's own state.
#[derive(Clone, PartialEq, Debug)]
pub enum Action {
    OpenUrl(String),
    Hcdev(Task),
    Restore(Task),
    Coordinator(Task),
    Terminal(Task),
    Notes(Task),
}

/// Run the dashboard: in this terminal when there is one, else re-run in a fullscreen Alacritty.
pub fn run() -> ExitCode {
    hwt_core::log_to_file("hdash", "hdash.log");
    if !(io::stdin().is_terminal() && io::stdout().is_terminal()) {
        return relaunch();
    }
    // SAFETY: no other thread runs yet, so nothing reads the environment concurrently.
    unsafe { std::env::remove_var(SPAWNED_ENV) };
    let quit = Arc::new(AtomicBool::new(false));
    for signal in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGHUP] {
        if let Err(e) = signal_hook::flag::register(signal, quit.clone()) {
            warn!("cannot watch signal {signal}: {e}");
        }
    }
    install_panic_hook();
    let mut terminal = match enter_terminal() {
        Ok(t) => t,
        Err(e) => {
            restore_terminal();
            error!("cannot set up the terminal: {e}");
            return ExitCode::FAILURE;
        }
    };
    let result = event_loop(&mut terminal, &quit);
    restore_terminal();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            error!("terminal error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn relaunch() -> ExitCode {
    if std::env::var_os(SPAWNED_ENV).is_some() {
        error!("re-run in Alacritty and still found no terminal to run in");
        return ExitCode::FAILURE;
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            error!("cannot find the hdash executable to re-run: {e}");
            return ExitCode::FAILURE;
        }
    };
    let spawned = Command::new("setsid")
        .args(["-f", "alacritty", "--title", "hdash", "--class", "hdash"])
        .args(["-o", "window.startup_mode=\"Fullscreen\"", "-e"])
        .arg(exe)
        .args(std::env::args_os().skip(1))
        .env(SPAWNED_ENV, "1")
        .env_remove("ZELLIJ")
        .env_remove("ZELLIJ_SESSION_NAME")
        .env_remove("ZELLIJ_PANE_ID")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match spawned {
        Ok(status) if status.success() => ExitCode::SUCCESS,
        Ok(status) => {
            error!("cannot open an Alacritty window for hdash: setsid exited {status}");
            ExitCode::FAILURE
        }
        Err(e) => {
            error!("cannot open an Alacritty window for hdash: {e}");
            ExitCode::FAILURE
        }
    }
}

fn install_panic_hook() {
    let main_thread = std::thread::current().id();
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == main_thread {
            error!("hdash panicked: {info}");
            restore_terminal();
            previous(info);
        } else {
            error!("background job panicked: {info}");
        }
    }));
}

fn enter_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    execute!(
        io::stdout(),
        EnterAlternateScreen,
        EnableMouseCapture,
        EnableBracketedPaste,
        cursor::Hide
    )?;
    Terminal::new(CrosstermBackend::new(io::stdout()))
}

fn restore_terminal() {
    let _ = disable_raw_mode();
    let _ = execute!(
        io::stdout(),
        DisableBracketedPaste,
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    );
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    quit: &AtomicBool,
) -> io::Result<()> {
    terminal.draw(|f| {
        let area = f.area();
        f.buffer_mut().set_string(
            area.x,
            area.y,
            "hdash — scanning…",
            Style::default().fg(Color::DarkGray),
        );
    })?;
    let mut app = App::new(DashView::new());
    let mut last = Instant::now();
    let mut second = now_secs();
    let mut dirty = true;
    while !app.quit && !quit.load(Ordering::Relaxed) {
        if event::poll(Duration::from_millis(EVENT_WAIT_MS))? {
            loop {
                dirty |= app.event(event::read()?);
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        let now = Instant::now();
        dirty |= app.view.tick(now - last);
        last = now;
        let this_second = now_secs();
        if this_second != second {
            second = this_second;
            dirty = true;
        }
        if dirty {
            terminal.draw(|f| {
                let area = f.area();
                app.draw(area, f.buffer_mut());
            })?;
            dirty = false;
        }
    }
    Ok(())
}

/// The shell's state: the view, the launch grace period, and whether to quit.
pub struct App {
    view: DashView,
    launches: Launches,
    quit: bool,
}

impl App {
    pub fn new(view: DashView) -> Self {
        Self {
            view,
            launches: Launches::default(),
            quit: false,
        }
    }

    fn event(&mut self, ev: Event) -> bool {
        match ev {
            Event::Key(key) if key.kind != KeyEventKind::Release => {
                if let Some(action) = self.key(key) {
                    self.run(action);
                }
                true
            }
            Event::Mouse(mouse) => {
                let (changed, action) = self.view.mouse(mouse);
                if let Some(action) = action {
                    self.run(action);
                }
                changed
            }
            Event::Paste(text) => {
                self.view.paste(&text);
                true
            }
            Event::Resize(..) => true,
            _ => false,
        }
    }

    fn key(&mut self, key: KeyEvent) -> Option<Action> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.quit = true;
            return None;
        }
        if !self.view.captures_keys() && key.code == KeyCode::Char('q') {
            self.quit = true;
            return None;
        }
        self.view.key(key)
    }

    fn run(&mut self, action: Action) {
        match action {
            Action::OpenUrl(url) => open_url(&url),
            Action::Hcdev(task) | Action::Coordinator(task) | Action::Restore(task)
                if self.launches.pending(&task.id) => {}
            Action::Hcdev(task) => {
                if !find_window(&task).is_some_and(activate) {
                    launch_hcdev(&task.dir, &task_spec(&task));
                    self.launches.record(task.id.clone());
                }
            }
            Action::Coordinator(task) => {
                if !find_window(&task).is_some_and(activate) {
                    launch_coordinator(&task_spec(&task));
                    self.launches.record(task.id.clone());
                }
            }
            Action::Restore(task) => {
                launch_restore(&task_spec(&task));
                self.launches.record(task.id.clone());
            }
            Action::Terminal(task) => {
                launch_terminal(&task.dir, &format!("term-{}", task.id.name));
            }
            Action::Notes(task) => open_notes(&task),
        }
    }

    fn draw(&mut self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 {
            return;
        }
        buf.set_stringn(
            area.x,
            area.y,
            KEY_HINT,
            area.width as usize,
            Style::default().fg(Color::DarkGray),
        );
        let body = Rect {
            y: area.y + 1,
            height: area.height.saturating_sub(1),
            ..area
        };
        self.view.draw(body, buf);
    }
}

fn task_spec(task: &Task) -> crate::model::launch::LaunchSpec {
    crate::model::item::Item { task }.launch_spec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::find;
    use crate::view::tests::{task, view};

    fn press(app: &mut App, code: KeyCode) {
        let _ = app.key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    // Pins that while the filter field has focus, the bound keys — q included — are typed into it
    // rather than acting, and that Esc gives the bindings back.
    #[test]
    fn keys_in_the_filter_field_are_typed_not_bound() {
        let mut app = App::new(view(vec![task("n1"), task("n2")]));
        let mut buf = Buffer::empty(Rect::new(0, 0, 160, 50));
        app.draw(buf.area, &mut buf);
        press(&mut app, KeyCode::Char('ä'));
        let before = app.view.selection();
        press(&mut app, KeyCode::Char('/'));
        for c in ['q', 'ö', 'ä', '#'] {
            press(&mut app, KeyCode::Char(c));
        }
        press(&mut app, KeyCode::Backspace);
        assert_eq!(app.view.query(), "qöä");
        assert_eq!(app.view.selection(), before);
        assert!(!app.quit);
        let mut buf = Buffer::empty(Rect::new(0, 0, 160, 50));
        app.draw(buf.area, &mut buf);
        find(&buf, "/qöä", None, 0);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('q'));
        assert!(app.quit);
    }

    // Pins the point of the port: the dashboard builds without any Bevy crate.
    #[test]
    fn workspace_has_no_bevy() {
        let lock =
            std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock"))
                .expect("the workspace Cargo.lock");
        let bevy: Vec<&str> = lock
            .lines()
            .filter_map(|l| l.strip_prefix("name = \""))
            .filter(|name| name.starts_with("bevy"))
            .collect();
        assert!(bevy.is_empty(), "bevy packages in Cargo.lock: {bevy:?}");
    }
}
