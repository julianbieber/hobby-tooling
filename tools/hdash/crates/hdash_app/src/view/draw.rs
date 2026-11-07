//! Drawing the view into a terminal buffer, recording a hit region for every control as it is
//! drawn.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::time::SystemTime;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

use super::input::card_buttons;
use super::{CardButton, Control, DashView, PullState};
use crate::facets::{Chip, Facts, Group, candidates, count, facts, passes};
use crate::focus::Presence;
use crate::model::item::{Item, Stage, backlog_visible, rank_projects};
use crate::model::project::Project;
use crate::model::task::{AspectRef, Pr, PrStatus, Repo, Task, TaskId, TodoCount, TodoStatus};
use crate::session::{Session, SessionState, now_secs};
use crate::widgets::{
    self, ACCENT, AMBER, CLOSED, DIM, DONE, Lines, MERGED, SIDEBAR_COLS, badge, button, clean,
    clip_rect, draw_field, draw_item, flow, text, too_small, truncate,
};

type Item_ = widgets::Item<Control>;

const SIDEBAR_HINT: &str = "click to scope  ·  / to filter";

impl DashView {
    /// Draw the view into `area` of `buf`, recording the hit region of everything clickable. A
    /// terminal too small to hold it gets one line saying so and no controls at all.
    pub fn draw(&mut self, area: Rect, buf: &mut Buffer) {
        self.hits.clear();
        if too_small(area, buf) {
            self.list_area = Rect::default();
            self.sidebar_area = Rect::default();
            self.drawn = false;
            return;
        }
        self.drawn = true;
        let ids = self.listed_ids();
        self.sync_selection(&ids);
        let sessions = self.sessions.states();
        let now = SystemTime::now();
        let all_facts: HashMap<TaskId, Facts> = self
            .model
            .tasks
            .iter()
            .map(|task| {
                let f = facts(&Item { task }, sessions.get(&task.id).copied(), now);
                (task.id.clone(), f)
            })
            .collect();
        let sidebar = Rect {
            width: SIDEBAR_COLS,
            ..area
        };
        let main = Rect {
            x: area.x + SIDEBAR_COLS,
            width: area.width - SIDEBAR_COLS,
            ..area
        };
        self.draw_sidebar(sidebar, buf, &all_facts);
        self.draw_main(main, buf, &all_facts);
    }

    fn counted(&self, item: &Item<'_>, all_facts: &HashMap<TaskId, Facts>) -> bool {
        (self.ui.show_done || item.stage() != Stage::Done)
            && backlog_visible(item, &self.ui)
            && (self.ui.chips.is_empty() || passes(&all_facts[&item.task.id], &self.ui.chips))
    }

    fn sidebar_projects(&self) -> Vec<Project> {
        let mut projects = self.model.projects.clone();
        for task in &self.model.tasks {
            if !projects.iter().any(|p| p.name == task.id.project) {
                projects.push(Project {
                    name: task.id.project.clone(),
                    path: PathBuf::new(),
                    has_checkout: false,
                });
            }
        }
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        projects
    }

    fn draw_sidebar(&mut self, area: Rect, buf: &mut Buffer, all_facts: &HashMap<TaskId, Facts>) {
        for y in area.y..area.y + area.height {
            buf.set_string(area.x + area.width - 1, y, "│", Style::default().fg(DIM));
        }
        let inner = Rect {
            x: area.x + 1,
            width: area.width - 3,
            ..area
        };
        buf.set_string(
            inner.x,
            inner.y,
            "Projects",
            Style::default().add_modifier(Modifier::BOLD),
        );

        let projects = self.sidebar_projects();
        let mut per_project: BTreeMap<String, usize> = BTreeMap::new();
        for task in &self.model.tasks {
            if self.counted(&Item { task }, all_facts) {
                *per_project.entry(task.id.project.clone()).or_default() += 1;
            }
        }
        let filtering = !self.ui.query.trim().is_empty();
        let ranked = rank_projects(&projects, &self.model.tasks, &self.ui.query);
        let mut listed: Vec<&Project> = ranked
            .into_iter()
            .filter(|p| filtering || per_project.get(p.name.as_str()).is_some_and(|n| *n > 0))
            .collect();
        if let Some(sel) = &self.ui.selected_project
            && !listed.iter().any(|p| &p.name == sel)
            && let Some(p) = projects.iter().find(|p| &p.name == sel)
        {
            listed.push(p);
        }
        let hint = if filtering {
            format!(
                "{} of {} projects · esc clears",
                listed.len(),
                projects.len()
            )
        } else {
            SIDEBAR_HINT.to_owned()
        };
        buf.set_stringn(
            inner.x,
            inner.y + 1,
            hint,
            inner.width as usize,
            Style::default().fg(DIM),
        );

        let field_y = inner.y + 2;
        let field_w = inner.width.saturating_sub("[clear]".len() as u16 + 1);
        let field = Rect::new(inner.x, field_y, field_w, 1);
        draw_field(
            buf,
            field,
            &format!("/{}", self.ui.query),
            self.field_focused,
        );
        self.hits.record(field, area, Control::FilterField);
        let clear = button("clear", Control::ClearFilter, false);
        self.draw_item(buf, inner.x + field_w + 1, field_y, &clear, area);

        let list = Rect {
            y: inner.y + 4,
            height: area.height.saturating_sub(4),
            ..inner
        };
        self.sidebar_area = Rect {
            x: area.x,
            width: area.width,
            ..list
        };
        let total: usize = per_project.values().sum();
        let mut rows: Vec<(Option<&Project>, usize)> = vec![(None, total)];
        rows.extend(listed.iter().map(|p| {
            (
                Some(*p),
                per_project.get(p.name.as_str()).copied().unwrap_or(0),
            )
        }));
        let max_scroll = (rows.len() as u16).saturating_sub(list.height);
        self.scroll_sidebar = self.scroll_sidebar.min(max_scroll);
        if listed.is_empty() {
            let msg = if projects.is_empty() {
                "No projects in the notes repo.".to_owned()
            } else if filtering {
                format!("No project matches \"{}\".", self.ui.query.trim())
            } else {
                "No project has items to show.".to_owned()
            };
            buf.set_stringn(
                list.x,
                list.y + 1,
                clean(&msg),
                list.width as usize,
                Style::default().fg(DIM),
            );
        }
        for (i, (project, n)) in rows
            .into_iter()
            .enumerate()
            .skip(self.scroll_sidebar as usize)
        {
            let y = list.y + (i as u16 - self.scroll_sidebar);
            if y >= list.y + list.height {
                break;
            }
            let name = project.map(|p| p.name.clone());
            let control = Control::ProjectRow(name.clone());
            let selected = self.ui.selected_project == name;
            let mut style = if selected {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            if self.hovered.as_ref() == Some(&control) {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let row = Rect::new(list.x, y, list.width, 1);
            buf.set_style(row, style);
            let count = n.to_string();
            let pull_w: u16 = 7;
            let name_w = list.width.saturating_sub(count.len() as u16 + pull_w + 1);
            let label = name.clone().unwrap_or_else(|| "All".to_owned());
            buf.set_stringn(
                row.x,
                y,
                truncate(&clean(&label), name_w),
                name_w as usize,
                style,
            );
            self.hits.record(row, list, control);
            if let Some(p) = project.filter(|p| p.has_checkout) {
                let px = row.x + name_w + 1;
                if matches!(
                    self.pulls.get(&p.name).map(|p| &p.state),
                    Some(PullState::Running)
                ) {
                    buf.set_string(px + 2, y, "…", style.fg(DIM));
                } else {
                    let pull = button("Pull", Control::PullButton(p.name.clone()), false);
                    self.draw_item(buf, px, y, &pull, list);
                }
            }
            buf.set_string(
                row.x + list.width - count.len() as u16,
                y,
                &count,
                style.fg(DIM),
            );
        }
    }

    fn draw_main(&mut self, area: Rect, buf: &mut Buffer, all_facts: &HashMap<TaskId, Facts>) {
        let inner = Rect {
            x: area.x + 1,
            width: area.width - 2,
            ..area
        };
        let mut y = inner.y;
        let mut bars = vec![self.header_items(), self.chip_items(all_facts)];
        bars.extend(self.pull_items().into_iter().map(|line| vec![line]));
        for bar in bars {
            for line in flow(bar, inner.width, 0) {
                for (x, item) in &line {
                    self.draw_item(buf, inner.x + x, y, item, area);
                }
                y += 1;
            }
        }
        buf.set_string(
            inner.x,
            y,
            "─".repeat(inner.width as usize),
            Style::default().fg(DIM),
        );
        y += 1;
        let list = Rect {
            y,
            height: (area.y + area.height).saturating_sub(y),
            ..inner
        };
        self.list_area = list;
        self.draw_cards(list, buf);
    }

    fn header_items(&self) -> Vec<Item_> {
        let (mut active, mut unstarted, mut done, mut orphans, mut parked) = (0, 0, 0, 0, 0);
        for task in &self.model.tasks {
            let item = Item { task };
            if let Some(sel) = &self.ui.selected_project
                && item.project() != sel
            {
                continue;
            }
            if !backlog_visible(&item, &self.ui) {
                parked += 1;
                continue;
            }
            match item.stage() {
                Stage::Done => done += 1,
                Stage::NotStarted => unstarted += 1,
                Stage::Active | Stage::Removed | Stage::Coordinator => active += 1,
                Stage::Orphan => orphans += 1,
            }
        }
        let scope = self
            .ui
            .selected_project
            .as_deref()
            .unwrap_or("all projects");
        let mut summary = if self.ui.backlog_mode {
            format!(
                "{scope} backlog  ·  {} backlogged tasks",
                active + unstarted
            )
        } else {
            format!("{scope}  ·  {active} active · {unstarted} not started")
        };
        if orphans > 0 {
            summary.push_str(&format!(" · {orphans} orphan worktrees"));
        }
        if !self.ui.backlog_mode {
            if self.ui.show_done {
                summary.push_str(&format!(" · {done} done"));
            } else if done > 0 {
                summary.push_str(&format!(" · {done} done hidden"));
            }
            if parked > 0 {
                summary.push_str(&format!(" · {parked} backlogged hidden"));
            }
        }
        if !self.ui.hidden.is_empty() {
            summary.push_str(&format!(" · {} dismissed", self.ui.hidden.len()));
        }
        let waiting = self
            .model
            .tasks
            .iter()
            .filter(|t| !t.done && self.sessions.state_of(&t.id) == Some(SessionState::Idle))
            .count();
        if waiting > 0 {
            summary.push_str(&format!(" · {waiting} waiting on you"));
        }
        if !self.ui.chips.is_empty() {
            summary.push_str(&format!(" · {} shown", self.listed().len()));
        }
        vec![
            text("Work", Style::default().add_modifier(Modifier::BOLD)),
            text(&summary, Style::default().fg(DIM)),
            button("Show done", Control::ShowDone, self.ui.show_done).gap(3),
            button("Show backlog", Control::ShowBacklog, self.ui.backlog_mode),
            button("Refresh", Control::Refresh, self.poller.refreshing()),
            button("Clear cache", Control::ClearCache, false),
        ]
    }

    fn chip_items(&self, all_facts: &HashMap<TaskId, Facts>) -> Vec<Item_> {
        let base: Vec<&Facts> = self
            .model
            .tasks
            .iter()
            .filter(|task| {
                let item = Item { task };
                crate::model::item::in_pr_scope(&item, &self.ui)
            })
            .map(|task| &all_facts[&task.id])
            .collect();
        let owned: Vec<Facts> = base.iter().map(|f| (*f).clone()).collect();
        let prs_loading = self
            .model
            .tasks
            .iter()
            .filter(|task| crate::model::item::in_pr_scope(&Item { task }, &self.ui))
            .flat_map(|t| t.prs.iter())
            .any(|p| p.status == PrStatus::Loading);
        let mut items = Vec::new();
        for group in Group::ALL {
            let shown: Vec<(Chip, usize)> = candidates(group, &owned, &self.ui.chips)
                .into_iter()
                .map(|chip| {
                    let n = count(base.iter().copied(), &self.ui.chips, &chip);
                    (chip, n)
                })
                .filter(|(chip, n)| {
                    *n > 0
                        || self.ui.chips.is_on(chip)
                        || (*chip == Chip::AwaitingReview && prs_loading)
                })
                .collect();
            if shown.is_empty() {
                continue;
            }
            items.push(text(&format!("{}:", group.label()), Style::default().fg(DIM)).gap(3));
            for (chip, n) in shown {
                let n = if chip == Chip::AwaitingReview && prs_loading {
                    "...".to_owned()
                } else {
                    n.to_string()
                };
                let on = self.ui.chips.is_on(&chip);
                items.push(button(
                    &format!("{} {n}", chip.label()),
                    Control::Chip(chip),
                    on,
                ));
            }
        }
        if !self.ui.chips.is_empty() {
            items.push(button("clear", Control::ClearChips, false).gap(3));
        }
        if let Some(first) = items.first_mut() {
            first.gap = 0;
        }
        items
    }

    fn pull_items(&self) -> Vec<Item_> {
        let mut lines: Vec<(&String, &super::Pull)> = self.pulls.iter().collect();
        lines.sort_by(|a, b| a.0.cmp(b.0));
        lines
            .into_iter()
            .map(|(name, pull)| match &pull.state {
                PullState::Running => text(&format!("{name} · pulling…"), Style::default().fg(DIM)),
                PullState::Done(outcome) => text(
                    &format!("{name} · {}", outcome.summary()),
                    Style::default().fg(DONE),
                ),
                PullState::Failed(reason) => text(
                    &format!("{name} · pull failed: {reason}"),
                    Style::default().fg(CLOSED),
                ),
            })
            .collect()
    }

    fn draw_cards(&mut self, list: Rect, buf: &mut Buffer) {
        let content_w = list.width.saturating_sub(4);
        let now = now_secs();
        let cards: Vec<(TaskId, CardLines)> = self
            .listed()
            .into_iter()
            .map(|(item, _)| {
                let session = self.sessions.tasks.get(&item.task.id).copied();
                (
                    item.task.id.clone(),
                    self.card_lines(&item, session, now, content_w),
                )
            })
            .collect();
        if cards.is_empty() {
            let msg = if self.model.tasks.is_empty() {
                "No tasks in the notes repo.".to_owned()
            } else if !self.ui.chips.is_empty() {
                let groups: Vec<&str> = self
                    .ui
                    .chips
                    .active_groups()
                    .into_iter()
                    .map(Group::label)
                    .collect();
                format!(
                    "No item matches the chips ({}). Press clear to drop them.",
                    groups.join(", ")
                )
            } else {
                "Nothing here. Toggle \"show done\"/\"show backlog\", or pick another project."
                    .to_owned()
            };
            buf.set_stringn(
                list.x,
                list.y,
                msg,
                list.width as usize,
                Style::default().fg(DIM),
            );
            self.scroll_list = 0;
            return;
        }

        let mut tops = Vec::with_capacity(cards.len());
        let mut total: u16 = 0;
        for (_, lines) in &cards {
            tops.push(total);
            total = total.saturating_add(lines.len() as u16 + 2);
        }
        if std::mem::take(&mut self.scroll_to_selected)
            && let Some(i) = cards
                .iter()
                .position(|(id, _)| Some(id) == self.selected.as_ref())
        {
            let top = tops[i];
            let bottom = top + cards[i].1.len() as u16 + 2;
            if top < self.scroll_list {
                self.scroll_list = top;
            } else if bottom > self.scroll_list + list.height {
                self.scroll_list = bottom.saturating_sub(list.height).min(top);
            }
        }
        self.scroll_list = self.scroll_list.min(total.saturating_sub(list.height));
        let scroll = self.scroll_list;
        self.cards_per_screen = cards
            .iter()
            .zip(&tops)
            .filter(|((_, lines), top)| {
                **top >= scroll && **top + lines.len() as u16 + 2 <= scroll + list.height
            })
            .count()
            .max(1);

        for ((id, lines), top) in cards.iter().zip(&tops) {
            let height = lines.len() as u16 + 2;
            if top + height <= scroll || *top >= scroll + list.height {
                continue;
            }
            let selected = self.selected.as_ref() == Some(id);
            let border = Style::default().fg(if selected { ACCENT } else { DIM });
            let card_y = list.y as i32 + *top as i32 - scroll as i32;
            if let Some(body) = clip_rect(list.x as i32, card_y, list.width, height, list) {
                self.hits.record(body, list, Control::CardBody(id.clone()));
            }
            for li in 0..height {
                let y = card_y + li as i32;
                if y < list.y as i32 || y >= (list.y + list.height) as i32 {
                    continue;
                }
                let y = y as u16;
                let w = list.width as usize;
                let edge = |l: &str, r: &str| format!("{l}{}{r}", "─".repeat(w.saturating_sub(2)));
                if li == 0 {
                    buf.set_string(list.x, y, edge("┌", "┐"), border);
                } else if li == height - 1 {
                    buf.set_string(list.x, y, edge("└", "┘"), border);
                } else {
                    buf.set_string(list.x, y, "│", border);
                    buf.set_string(list.x + list.width - 1, y, "│", border);
                    for (x, item) in &lines[li as usize - 1] {
                        self.draw_item(buf, list.x + 2 + x, y, item, list);
                    }
                }
            }
        }
    }

    fn card_lines(
        &self,
        item: &Item<'_>,
        session: Option<Session>,
        now: u64,
        width: u16,
    ) -> CardLines {
        let task = item.task;
        let stage = item.stage();
        let mut lines = Vec::new();

        let dot = match stage {
            Stage::Done => DIM,
            Stage::Active if task.dirty_total() == 0 => DONE,
            Stage::Coordinator if task.coordinator.as_ref().is_none_or(|c| c.wants_me() == 0) => {
                DONE
            }
            _ => AMBER,
        };
        let mut header = vec![
            text("●", Style::default().fg(dot)),
            text(item.title(), Style::default().add_modifier(Modifier::BOLD)),
            widgets::Item {
                spans: vec![Span::styled(
                    format!(" {} ", clean(item.topic())),
                    Style::default().fg(Color::Gray).bg(Color::Indexed(238)),
                )],
                control: None,
                gap: 1,
            },
        ];
        if task.done {
            header.push(badge("done", DONE));
        } else if task.ready() {
            let until = task
                .block
                .as_ref()
                .and_then(|b| b.until.clone())
                .unwrap_or_default();
            header.push(badge(&format!("ready · {until}"), AMBER));
        } else if let Some(block) = &task.block {
            let label = match &block.until {
                Some(until) => format!("blocked · until {until}"),
                None => "blocked".to_owned(),
            };
            header.push(badge(&label, DIM));
        }
        let stage_badge = match stage {
            Stage::Orphan => Some(("orphan", AMBER)),
            _ if task.is_coordinator() => Some(("coordinator", ACCENT)),
            Stage::NotStarted => Some(("not started", DIM)),
            _ if !task.present => Some(("removed", DIM)),
            _ => None,
        };
        if let Some((label, bg)) = stage_badge {
            header.push(badge(label, bg));
        }
        if let Some(session) = session {
            let bg = match session.state {
                SessionState::Idle => AMBER,
                SessionState::Busy => DIM,
            };
            header.push(badge(&session_label(&session, now), bg));
        }
        if stage == Stage::Orphan {
            header.push(text("no task.json in the notes repo", Style::default().fg(DIM)).gap(3));
        }
        let main_label = self.main_label(item);
        for (i, b) in card_buttons(task).into_iter().enumerate() {
            let label = match b {
                CardButton::Done if task.done => "Reopen".to_owned(),
                CardButton::Done => "Done".to_owned(),
                CardButton::Backlog if item.backlogged() => "Unbacklog".to_owned(),
                CardButton::Backlog => "Backlog".to_owned(),
                CardButton::Terminal => "Terminal".to_owned(),
                CardButton::Notes => "Notes".to_owned(),
                CardButton::Hide => "Hide".to_owned(),
                CardButton::Main => main_label.clone(),
            };
            let button = button(
                &label,
                Control::Card(task.id.clone(), b),
                b == CardButton::Main,
            );
            header.push(if i == 0 { button.gap(3) } else { button });
        }
        lines.extend(flow(header, width, 0));

        let mut meta = Vec::new();
        let mut line = format!("{} · {}", item.project(), item.topic());
        if let Some(t) = task.last_active {
            line.push_str(&format!(" · {}", hwt_core::rel_time(t)));
        }
        meta.push(text(&line, Style::default().fg(DIM)));
        if task.present {
            meta.extend(task.repos.iter().map(repo_chip));
        } else {
            meta.push(text(item.task_name(), Style::default()));
        }
        if let Some(phase) = &task.phase {
            let color = if phase.starts_with("await-") {
                AMBER
            } else {
                DIM
            };
            meta.push(text(&format!("· {phase}"), Style::default().fg(color)));
        }
        if task.is_coordinator() {
            let (line, style) = match &task.coordinator {
                Some(c) if c.wants_me() > 0 => (c.summary(), Style::default().fg(AMBER)),
                Some(c) => (c.summary(), Style::default()),
                None => ("no tasks.json".to_owned(), Style::default().fg(DIM)),
            };
            meta.push(text(&format!("· {line}"), style));
        }
        if let Some(reason) = task.block.as_ref().and_then(|b| b.reason.as_ref()) {
            meta.push(text(&format!("· {reason}"), Style::default().fg(DIM)));
        }
        lines.extend(flow(meta, width, 0));
        lines.extend(self.todo_lines(task, width));

        if !task.prs.is_empty() {
            let expanded = self.ui.pr_expanded.contains(&task.id);
            let arrow = if expanded { "▾" } else { "▸" };
            let mut items = vec![button(
                &format!("{arrow} PRs ({})", task.prs.len()),
                Control::PrsToggle(task.id.clone()),
                false,
            )];
            let awaiting = task.awaiting_review();
            if awaiting > 0 {
                items.push(text(
                    &format!("· {awaiting} awaiting review"),
                    Style::default().fg(AMBER),
                ));
            }
            lines.extend(flow(items, width, 0));
            if expanded {
                for pr in &task.prs {
                    lines.extend(flow(vec![pr_line(&task.id, pr)], width, 0));
                }
            }
        }

        if !task.aspects.is_empty() {
            lines.extend(flow(
                vec![text(
                    &format!("Aspects ({})", task.aspects.len()),
                    Style::default().fg(DIM),
                )],
                width,
                2,
            ));
            let now = now_secs();
            for aspect in &task.aspects {
                let mut items = vec![aspect_line(&task.id, aspect)];
                if let Some(session) = self.sessions.aspect(&task.id, &aspect.name) {
                    let bg = match session.state {
                        SessionState::Idle => AMBER,
                        SessionState::Busy => DIM,
                    };
                    items.push(badge(&session_label(&session, now), bg));
                }
                lines.extend(flow(items, width, 2));
            }
        }
        lines
    }

    fn todo_lines(&self, task: &Task, width: u16) -> CardLines {
        let mut lines = Vec::new();
        let expanded = self.ui.todo_expanded.contains(&task.id);
        let mut items = Vec::new();
        if !task.todo.is_empty() {
            let arrow = if expanded { "▾" } else { "▸" };
            let count = TodoCount::of(&task.todo);
            items.push(button(
                &format!("{arrow} Todo ({}/{})", count.done, count.total()),
                Control::TodoToggle(task.id.clone()),
                false,
            ));
            if let Some(active) = task.todo.iter().find(|t| t.status == TodoStatus::Active) {
                items.push(text(&format!("▶ {}", active.text), Style::default()));
            }
        }
        if let Some(waiting) = &task.waiting_on {
            items.push(text(
                &format!("waiting: {waiting}"),
                Style::default().fg(AMBER),
            ));
        }
        if items.is_empty() {
            return lines;
        }
        lines.extend(flow(items, width, 0));
        if expanded {
            for todo in &task.todo {
                let (marker, style) = todo_marker(todo.status);
                lines.extend(flow(
                    vec![text(marker, style), text(&todo.text, style)],
                    width,
                    2,
                ));
            }
        }
        lines
    }

    fn main_label(&self, item: &Item<'_>) -> String {
        let task = item.task;
        let target = if task.is_coordinator() {
            "coordinator"
        } else {
            "hcdev"
        };
        match item.stage() {
            Stage::Orphan => "Adopt".to_owned(),
            Stage::NotStarted => "Start task".to_owned(),
            Stage::Removed => "Restore".to_owned(),
            Stage::Active | Stage::Done | Stage::Coordinator => match self.presences.of(task) {
                Presence::Window => "Focus".to_owned(),
                Presence::Detached => "Attach".to_owned(),
                Presence::None | Presence::Unknown => format!("Open {target}"),
            },
        }
    }

    fn draw_item(&mut self, buf: &mut Buffer, x: u16, y: u16, item: &Item_, clip: Rect) {
        draw_item(
            buf,
            &mut self.hits,
            &self.hovered,
            &self.focus,
            x,
            y,
            item,
            clip,
        );
    }
}

type CardLines = Lines<Control>;

fn session_label(session: &Session, now: u64) -> String {
    let state = match session.state {
        SessionState::Idle => "idle",
        SessionState::Busy => "busy",
    };
    format!(
        "{state} {}",
        hwt_core::fmt_duration(now.saturating_sub(session.since))
    )
}

fn repo_chip(repo: &Repo) -> Item_ {
    let chip = Style::default().bg(Color::Indexed(236));
    let branch = if !repo.git.loaded {
        "…".to_owned()
    } else {
        repo.git
            .branch
            .clone()
            .or_else(|| repo.git.head.clone().map(|h| format!("@{h}")))
            .unwrap_or_else(|| "-".into())
    };
    let mut spans = vec![Span::styled(
        format!(" {}", clean(&branch)),
        chip.fg(Color::White),
    )];
    if repo.git.dirty > 0 {
        spans.push(Span::styled(
            format!(" ±{}", repo.git.dirty),
            chip.fg(AMBER),
        ));
    }
    spans.push(Span::styled(" ", chip));
    widgets::Item {
        spans,
        control: None,
        gap: 1,
    }
}

fn pr_line(owner: &TaskId, pr: &Pr) -> Item_ {
    let (state, color) = match pr.status {
        PrStatus::Loading => ("…", DIM),
        PrStatus::Draft => ("draft", DIM),
        PrStatus::Open => ("review", AMBER),
        PrStatus::Merged => ("merged", MERGED),
        PrStatus::Closed => ("closed", CLOSED),
        PrStatus::Unknown => ("?", DIM),
    };
    let num = pr
        .number
        .map_or_else(|| "#?".to_owned(), |n| format!("#{n}"));
    let mut label = format!("{num} ");
    let title = pr.display_title();
    if !title.is_empty() {
        label.push_str(title);
        label.push(' ');
    }
    label.push_str(&format!("({state})"));
    if let Some(repo) = &pr.repo {
        label.push_str(&format!(" · {repo}"));
    }
    if let Some(opened) = &pr.opened {
        label.push_str(&format!(" · opened {opened}"));
    }
    widgets::Item {
        spans: vec![Span::styled(
            format!(" {} ", clean(&label)),
            Style::default().fg(Color::Black).bg(color),
        )],
        control: Some(Control::PrLine(owner.clone(), pr.url.clone())),
        gap: 1,
    }
}

fn todo_marker(status: TodoStatus) -> (&'static str, Style) {
    match status {
        TodoStatus::Done => ("✓", Style::default().fg(DONE)),
        TodoStatus::Active => ("▶", Style::default().fg(ACCENT)),
        TodoStatus::Pending => ("○", Style::default()),
        TodoStatus::Dropped => (
            "✗",
            Style::default().fg(DIM).add_modifier(Modifier::CROSSED_OUT),
        ),
    }
}

fn aspect_line(owner: &TaskId, aspect: &AspectRef) -> Item_ {
    let mut label = format!("⌕ {} · {} · {}", aspect.name, aspect.kind, aspect.state);
    if let Some(goal) = &aspect.goal {
        label.push_str(&format!(" · {goal}"));
    }
    let style = match aspect.state.as_str() {
        "reported" | "done" => Style::default().bg(Color::Indexed(238)).fg(Color::Green),
        "closed" => Style::default().fg(DIM),
        _ => Style::default().bg(Color::Indexed(238)),
    };
    widgets::Item {
        spans: vec![Span::styled(format!(" {} ", clean(&label)), style)],
        control: Some(Control::Aspect(owner.clone(), aspect.name.clone())),
        gap: 1,
    }
}
