//! What keys and clicks do to the view, and the [`Action`] each hands back when the shell has to
//! run it.

use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Position;

use super::{CardButton, Control, DashView};
use crate::app::Action;
use crate::model::item::{Item, Stage, rank_projects};
use crate::model::task::{Task, TaskId};
use crate::widgets::{WHEEL_LINES, is_hidden};

/// The buttons a task's card carries, in focus order: Done and Backlog for a recorded task,
/// Terminal for one with a worktree, then Notes, Hide and the main button.
pub(super) fn card_buttons(task: &Task) -> Vec<CardButton> {
    let mut out = Vec::new();
    if task.adopted {
        out.extend([CardButton::Done, CardButton::Backlog]);
    }
    if task.present {
        out.push(CardButton::Terminal);
    }
    out.extend([CardButton::Notes, CardButton::Hide, CardButton::Main]);
    out
}

impl DashView {
    /// Handle a key the shell did not keep for itself.
    pub fn key(&mut self, key: KeyEvent) -> Option<Action> {
        if self.field_focused {
            match key.code {
                KeyCode::Esc => {
                    self.ui.query.clear();
                    self.field_focused = false;
                }
                KeyCode::Enter => {
                    let best =
                        rank_projects(&self.model.projects, &self.model.tasks, &self.ui.query)
                            .first()
                            .map(|p| p.name.clone());
                    if let Some(best) = best {
                        self.ui.selected_project = Some(best);
                        self.reset_selection = true;
                    }
                    self.ui.query.clear();
                }
                KeyCode::Backspace => {
                    self.ui.query.pop();
                }
                KeyCode::Char(c)
                    if !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                {
                    self.type_text(&c.to_string());
                }
                _ => {}
            }
            return None;
        }
        if !self.drawn {
            return None;
        }
        let ids = self.listed_ids();
        self.sync_selection(&ids);
        match key.code {
            KeyCode::Char('/') => self.field_focused = true,
            KeyCode::Up | KeyCode::Char('ü') => self.move_selection(&ids, -1),
            KeyCode::Down | KeyCode::Char('ä') => self.move_selection(&ids, 1),
            KeyCode::PageUp => self.move_selection(&ids, -(self.cards_per_screen as isize)),
            KeyCode::PageDown => self.move_selection(&ids, self.cards_per_screen as isize),
            KeyCode::Left | KeyCode::Char('ö') => self.move_focus(-1),
            KeyCode::Right | KeyCode::Char('#') => self.move_focus(1),
            KeyCode::Enter | KeyCode::Char(' ') => {
                return self.focus.clone().and_then(|c| self.activate(c));
            }
            _ => {}
        }
        None
    }

    /// Pasted text goes into the filter field, focusing it.
    pub fn paste(&mut self, text: &str) {
        self.field_focused = true;
        self.type_text(text);
    }

    fn type_text(&mut self, text: &str) {
        self.ui
            .query
            .extend(text.chars().filter(|c| !is_hidden(*c)));
    }

    /// Handle a mouse event. Answers whether anything shown changed, and the action a click asks
    /// for. A click counts only when press and release land on the same control.
    pub fn mouse(&mut self, ev: MouseEvent) -> (bool, Option<Action>) {
        let (x, y) = (ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.pressed = Some(self.hits.at(x, y));
                (false, None)
            }
            MouseEventKind::Up(MouseButton::Left) => {
                let target = self.hits.at(x, y);
                let Some(pressed) = self.pressed.take() else {
                    return (false, None);
                };
                if pressed != target {
                    return (false, None);
                }
                (true, self.click(target))
            }
            MouseEventKind::Moved | MouseEventKind::Drag(_) => {
                let target = self.hits.at(x, y);
                let changed = target != self.hovered;
                self.hovered = target;
                (changed, None)
            }
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = ev.kind == MouseEventKind::ScrollUp;
                let scroll = if self.sidebar_area.contains(Position::new(x, y)) {
                    &mut self.scroll_sidebar
                } else if self.list_area.contains(Position::new(x, y)) {
                    &mut self.scroll_list
                } else {
                    return (false, None);
                };
                *scroll = if up {
                    scroll.saturating_sub(WHEEL_LINES)
                } else {
                    scroll.saturating_add(WHEEL_LINES)
                };
                (true, None)
            }
            _ => (false, None),
        }
    }

    fn click(&mut self, target: Option<Control>) -> Option<Action> {
        if target != Some(Control::FilterField) {
            self.field_focused = false;
        }
        let control = target?;
        if let Some(id) = control.card().cloned() {
            self.selected = Some(id.clone());
            self.scroll_to_selected = false;
            self.focus = Some(match &control {
                Control::CardBody(_) => Control::Card(id, CardButton::Main),
                other => other.clone(),
            });
        }
        self.activate(control)
    }

    fn activate(&mut self, control: Control) -> Option<Action> {
        match control {
            Control::ProjectRow(project) => {
                self.ui.selected_project = project;
                self.reset_selection = true;
            }
            Control::PullButton(project) => self.start_pull(&project),
            Control::FilterField => self.field_focused = true,
            Control::ClearFilter => self.ui.query.clear(),
            Control::Chip(chip) => {
                self.ui.chips.toggle(chip);
                self.reset_selection = true;
            }
            Control::ClearChips => {
                self.ui.chips = Default::default();
                self.reset_selection = true;
            }
            Control::ShowDone => self.ui.show_done = !self.ui.show_done,
            Control::ShowBacklog => {
                self.ui.backlog_mode = !self.ui.backlog_mode;
                self.reset_selection = true;
            }
            Control::Refresh => self.refresh(),
            Control::ClearCache => self.clear_cache(),
            Control::CardBody(_) => {}
            Control::Card(id, button) => return self.card_button(id, button),
            Control::PrsToggle(id) => {
                if !self.ui.pr_expanded.remove(&id) {
                    self.ui.pr_expanded.insert(id);
                }
            }
            Control::PrLine(_, url) => return Some(Action::OpenUrl(url)),
            Control::Subtask(_, child) => {
                self.ui.selected_project = Some(child.project);
                self.reset_selection = true;
            }
        }
        None
    }

    fn card_button(&mut self, id: TaskId, button: CardButton) -> Option<Action> {
        let task = self.task(&id)?.clone();
        match button {
            CardButton::Done => self.toggle_done(&id),
            CardButton::Backlog => self.toggle_backlog(&id),
            CardButton::Terminal => return Some(Action::Terminal(task)),
            CardButton::Notes => return Some(Action::Notes(task)),
            CardButton::Hide => {
                self.ui.hidden.insert(id);
            }
            CardButton::Main => {
                let stage = Item { task: &task }.stage();
                return match stage {
                    Stage::Orphan => {
                        self.adopt(&id);
                        None
                    }
                    Stage::NotStarted | Stage::Removed => Some(Action::Restore(task)),
                    Stage::Coordinator => Some(Action::Coordinator(task)),
                    Stage::Done if task.is_coordinator() => Some(Action::Coordinator(task)),
                    Stage::Active | Stage::Done => Some(Action::Hcdev(task)),
                };
            }
        }
        None
    }

    /// Keep the selection on its task, or move it to the card at its position, the first card after
    /// a project, chip or backlog change, or nothing in an empty list.
    pub(super) fn sync_selection(&mut self, ids: &[TaskId]) {
        if std::mem::take(&mut self.reset_selection) {
            self.sel_pos = 0;
            self.selected = None;
        }
        match self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|i| i == id))
        {
            Some(pos) => self.sel_pos = pos,
            None if ids.is_empty() => {
                self.selected = None;
                self.focus = None;
            }
            None => {
                let pos = self.sel_pos.min(ids.len() - 1);
                self.select(ids[pos].clone(), pos);
            }
        }
        if let Some(id) = self.selected.clone() {
            let controls = self.card_controls(&id);
            if !self.focus.as_ref().is_some_and(|f| controls.contains(f)) {
                self.focus = Some(Control::Card(id, CardButton::Main));
            }
        }
    }

    fn select(&mut self, id: TaskId, pos: usize) {
        self.focus = Some(Control::Card(id.clone(), CardButton::Main));
        self.selected = Some(id);
        self.sel_pos = pos;
        self.scroll_to_selected = true;
    }

    fn move_selection(&mut self, ids: &[TaskId], by: isize) {
        if ids.is_empty() {
            return;
        }
        let pos = (self.sel_pos as isize + by).clamp(0, ids.len() as isize - 1) as usize;
        if self.selected.as_ref() != Some(&ids[pos]) {
            self.select(ids[pos].clone(), pos);
        }
    }

    fn move_focus(&mut self, by: isize) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let controls = self.card_controls(&id);
        let Some(at) = self
            .focus
            .as_ref()
            .and_then(|f| controls.iter().position(|c| c == f))
        else {
            return;
        };
        let next = (at as isize + by).clamp(0, controls.len() as isize - 1) as usize;
        self.focus = Some(controls[next].clone());
    }

    fn card_controls(&self, id: &TaskId) -> Vec<Control> {
        let Some(task) = self.task(id) else {
            return Vec::new();
        };
        let mut out: Vec<Control> = card_buttons(task)
            .into_iter()
            .map(|b| Control::Card(id.clone(), b))
            .collect();
        if !task.prs.is_empty() {
            out.push(Control::PrsToggle(id.clone()));
            if self.ui.pr_expanded.contains(id) {
                out.extend(
                    task.prs
                        .iter()
                        .map(|p| Control::PrLine(id.clone(), p.url.clone())),
                );
            }
        }
        out.extend(
            task.subtasks
                .iter()
                .map(|s| Control::Subtask(id.clone(), s.resolved_id(&id.project, &id.topic))),
        );
        out
    }
}
