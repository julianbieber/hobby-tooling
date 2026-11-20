//! An [`Overview`] laid out as rows for a pane of a given width, each row optionally a link a click
//! opens.

use std::path::PathBuf;
use std::time::SystemTime;

use ratatui::style::{Color, Modifier, Style};
use unicode_width::UnicodeWidthChar;

use crate::overview::Overview;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Url(String),
    File(PathBuf),
}

/// One screen row: styled pieces of text, and the link the whole row opens.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub spans: Vec<(String, Style)>,
    pub link: Option<Link>,
}

impl Row {
    fn text(text: impl Into<String>, style: Style) -> Self {
        Self {
            spans: vec![(text.into(), style)],
            link: None,
        }
    }

    pub fn plain(&self) -> String {
        self.spans.iter().map(|(t, _)| t.as_str()).collect()
    }
}

const INDENT: usize = 2;

/// The rows of `o` at `width` columns, with durations measured to `now`.
pub fn rows(o: &Overview, width: usize, now: SystemTime) -> Vec<Row> {
    let width = width.max(INDENT + 8);
    let mut out = Vec::new();
    let heading = Style::default().add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(Color::DarkGray);

    let mut status: Vec<String> = Vec::new();
    status.extend(o.kind.clone());
    status.extend(o.state.clone());
    if let Some(phase) = o.phase.as_ref().filter(|p| o.state.as_ref() != Some(*p)) {
        match o.phase_since.and_then(|t| now.duration_since(t).ok()) {
            Some(d) => status.push(format!("{phase} {}", hwt_core::fmt_duration(d.as_secs()))),
            None => status.push(phase.clone()),
        }
    }
    out.push(Row {
        spans: vec![
            (o.name.clone(), heading.fg(Color::Cyan)),
            (format!("  {}", status.join(" · ")), dim),
        ],
        link: None,
    });
    if let Some(w) = &o.waiting_on {
        let amber = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        out.extend(wrapped(&format!("⏸ {w}"), width, 0, amber));
    }
    let has_what = o.sections.iter().any(|s| s.heading == "What");
    if let Some(goal) = o.goal.as_ref().filter(|_| !has_what) {
        out.extend(wrapped(goal, width, 0, Style::default()));
    }

    let link = Style::default().add_modifier(Modifier::UNDERLINED);
    if !o.prs.is_empty() || o.plan.is_some() {
        out.push(Row::default());
        for pr in &o.prs {
            out.push(Row {
                spans: vec![("PR   ".into(), dim), (short_pr(pr), link.fg(Color::Blue))],
                link: Some(Link::Url(pr.clone())),
            });
        }
        if let Some(plan) = &o.plan {
            let name = plan
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            out.push(Row {
                spans: vec![("plan ".into(), dim), (name, link)],
                link: Some(Link::File(plan.clone())),
            });
        }
    }

    for s in &o.sections {
        out.push(Row::default());
        out.push(Row::text(s.heading.clone(), heading));
        for para in paragraphs(&s.body) {
            out.extend(wrapped(&para, width, INDENT, Style::default()));
        }
    }

    let (closed, open): (Vec<_>, Vec<_>) = o.todo.iter().partition(|t| t.is_closed());
    if !o.todo.is_empty() {
        out.push(Row::default());
        out.push(Row::text(
            format!("To do  {}/{} done", closed.len(), o.todo.len()),
            heading,
        ));
        for t in &open {
            let (marker, style) = if t.status == "active" {
                (
                    "▶ ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("○ ", Style::default())
            };
            out.extend(wrapped(
                &format!("{marker}{}", t.text),
                width,
                INDENT,
                style,
            ));
        }
        if !closed.is_empty() {
            out.push(Row::text("Done", heading));
            for t in &closed {
                let marker = if t.status == "dropped" {
                    "✗ "
                } else {
                    "✓ "
                };
                out.extend(marked(marker, &t.text, width, dim));
            }
        }
    }

    if let Some(outcome) = &o.outcome {
        out.push(Row::default());
        out.push(Row::text("Outcome", heading));
        out.extend(wrapped(outcome, width, INDENT, Style::default()));
    }
    if !o.open_items.is_empty() {
        out.push(Row::default());
        out.push(Row::text("Open items", heading));
        for item in &o.open_items {
            let text = match &item.issue {
                Some(issue) => format!("{} → {issue}", item.text),
                None => item.text.clone(),
            };
            out.extend(marked("• ", &text, width, Style::default()));
        }
    }
    for w in &o.warnings {
        out.push(Row::default());
        out.extend(wrapped(w, width, 0, Style::default().fg(Color::Red)));
    }
    out
}

fn short_pr(url: &str) -> String {
    let mut parts = url.trim_end_matches('/').rsplit('/');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(n), Some("pull"), Some(repo)) => format!("{repo}#{n}"),
        _ => url.to_owned(),
    }
}

fn paragraphs(body: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut open = false;
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            open = false;
            continue;
        }
        let starts_item = line.starts_with("- ")
            || line.starts_with("* ")
            || line
                .split_once(". ")
                .is_some_and(|(n, _)| n.chars().all(|c| c.is_ascii_digit()));
        match out.last_mut() {
            Some(last) if open && !starts_item => {
                last.push(' ');
                last.push_str(line);
            }
            _ => out.push(line.to_owned()),
        }
        open = true;
    }
    out
}

fn marked(marker: &str, text: &str, width: usize, style: Style) -> Vec<Row> {
    let hang = INDENT + marker.chars().count();
    let mut rows = wrapped(text, width, hang, style);
    if let Some(first) = rows.first_mut() {
        let rest = first.plain();
        let rest = rest.get(hang..).unwrap_or_default();
        *first = Row::text(format!("{}{marker}{rest}", " ".repeat(INDENT)), style);
    }
    rows
}

fn wrapped(text: &str, width: usize, indent: usize, style: Style) -> Vec<Row> {
    let room = width.saturating_sub(indent).max(1);
    let pad = " ".repeat(indent);
    wrap(text, room)
        .into_iter()
        .map(|line| Row::text(format!("{pad}{line}"), style))
        .collect()
}

fn wrap(text: &str, room: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for word in text.split_whitespace() {
        let w: usize = word.chars().map(|c| c.width().unwrap_or(0)).sum();
        if used > 0 && used + 1 + w > room {
            lines.push(std::mem::take(&mut line));
            used = 0;
        }
        if used > 0 {
            line.push(' ');
            used += 1;
        }
        if w > room {
            for c in word.chars() {
                let cw = c.width().unwrap_or(0);
                if used + cw > room {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push(c);
                used += cw;
            }
        } else {
            line.push_str(word);
            used += w;
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::overview::{OpenItem, Section, Todo};

    fn todo(text: &str, status: &str) -> Todo {
        Todo {
            text: text.into(),
            status: status.into(),
        }
    }

    fn sample() -> Overview {
        Overview {
            name: "persist".into(),
            kind: Some("code".into()),
            state: Some("active".into()),
            phase: Some("implement".into()),
            phase_since: Some(SystemTime::UNIX_EPOCH),
            waiting_on: Some("answer".into()),
            goal: Some("Save".into()),
            sections: vec![Section {
                heading: "Why".into(),
                body: "Runs are lost".into(),
            }],
            todo: vec![todo("a", "done"), todo("b", "active"), todo("c", "pending")],
            open_items: vec![OpenItem {
                text: "tune".into(),
                issue: Some("p/t/x".into()),
            }],
            prs: vec!["https://github.com/o/r/pull/7".into()],
            plan: Some(PathBuf::from("/n/plan-1.md")),
            ..Overview::default()
        }
    }

    #[test]
    fn open_steps_come_before_done_ones_and_links_are_clickable() {
        let rows = rows(
            &sample(),
            40,
            SystemTime::UNIX_EPOCH + Duration::from_secs(120),
        );
        let text: Vec<String> = rows.iter().map(Row::plain).collect();
        assert_eq!(text[0], "persist  code · active · implement 2m");
        assert_eq!(text[1], "⏸ answer");
        assert!(text.contains(&"Save".to_owned()));
        let pos = |s: &str| text.iter().position(|t| t == s).unwrap();
        assert!(pos("  ▶ b") < pos("  ○ c"));
        assert!(pos("  ○ c") < pos("Done"));
        assert!(pos("Done") < pos("  ✓ a"));
        assert!(text.contains(&"To do  1/3 done".to_owned()));
        assert!(text.contains(&"  • tune → p/t/x".to_owned()));
        let pr = rows.iter().find(|r| r.plain() == "PR   r#7").unwrap();
        assert_eq!(
            pr.link,
            Some(Link::Url("https://github.com/o/r/pull/7".into()))
        );
        let plan = rows.iter().find(|r| r.plain() == "plan plan-1.md").unwrap();
        assert_eq!(plan.link, Some(Link::File("/n/plan-1.md".into())));
    }

    #[test]
    fn hard_wrapped_markdown_reflows_and_list_items_stay_apart() {
        let body = "one\ntwo\n\nthree\n- a\n  b\n- c\n1. d";
        assert_eq!(
            paragraphs(body),
            ["one two", "three", "- a b", "- c", "1. d"]
        );
    }

    #[test]
    fn a_marked_line_hangs_under_its_text() {
        let rows = marked("▶ ", "aaa bbb", 9, Style::default());
        let text: Vec<String> = rows.iter().map(Row::plain).collect();
        assert_eq!(text, ["  ▶ aaa", "    bbb"]);
    }

    #[test]
    fn text_wraps_to_the_pane_and_splits_overlong_words() {
        assert_eq!(wrap("aa bb cc", 5), ["aa bb", "cc"]);
        assert_eq!(wrap("abcdefg", 3), ["abc", "def", "g"]);
        assert_eq!(wrap("", 3), [""]);
    }
}
