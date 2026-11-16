//! The drawing primitives of the dashboard: how a piece of text, a button, a badge or a field
//! lands in terminal cells, and where a click on it is recorded.

use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use unicode_width::UnicodeWidthChar;

use crate::hits::Hits;

pub use hwt_core::{clean, is_hidden};

/// Width of a view's sidebar, in columns.
pub const SIDEBAR_COLS: u16 = 36;
/// Lines one wheel notch scrolls.
pub const WHEEL_LINES: u16 = 3;
const MIN_MAIN_COLS: u16 = 40;
const MIN_LINES: u16 = 12;

pub const DIM: Color = Color::DarkGray;
pub const AMBER: Color = Color::Yellow;
pub const ACCENT: Color = Color::Cyan;
pub const MERGED: Color = Color::Magenta;
pub const CLOSED: Color = Color::Red;
pub const DONE: Color = Color::Green;
pub const FIELD_BG: Color = Color::Indexed(236);

/// Whether `area` is too small for a sidebar and a main column; if so, draws only a "terminal too
/// small" line into it.
pub fn too_small(area: Rect, buf: &mut Buffer) -> bool {
    if area.width >= SIDEBAR_COLS + MIN_MAIN_COLS && area.height >= MIN_LINES {
        return false;
    }
    if area.height > 0 {
        buf.set_stringn(
            area.x,
            area.y,
            "terminal too small",
            usize::from(area.width),
            Style::default().fg(DIM),
        );
    }
    true
}

/// One unit of a wrapped row: its styled text, the control it is when clickable, and the blank
/// columns before it when it does not start a line.
#[derive(Clone, Debug)]
pub struct Item<C> {
    pub spans: Vec<Span<'static>>,
    pub control: Option<C>,
    pub gap: u16,
}

impl<C> Item<C> {
    /// The width in terminal cells.
    pub fn width(&self) -> u16 {
        self.spans.iter().map(|s| cells(s.width())).sum()
    }

    /// This item with `gap` blank columns before it when it does not start a line.
    pub fn gap(mut self, gap: u16) -> Self {
        self.gap = gap;
        self
    }
}

/// Lines of positioned items, as [`flow`] lays them out.
pub type Lines<C> = Vec<Vec<(u16, Item<C>)>>;

/// Plain text, cleaned of control and invisible characters.
pub fn text<C>(s: &str, style: Style) -> Item<C> {
    Item {
        spans: vec![Span::styled(clean(s), style)],
        control: None,
        gap: 1,
    }
}

/// A clickable `[label]`; `primary` draws it bold in the accent colour.
pub fn button<C>(label: &str, control: C, primary: bool) -> Item<C> {
    let style = if primary {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    Item {
        spans: vec![Span::styled(format!("[{}]", clean(label)), style)],
        control: Some(control),
        gap: 1,
    }
}

/// A label in black on `bg`.
pub fn badge<C>(label: &str, bg: Color) -> Item<C> {
    Item {
        spans: vec![Span::styled(
            format!(" {} ", clean(label)),
            Style::default().fg(Color::Black).bg(bg),
        )],
        control: None,
        gap: 1,
    }
}

/// Lay `items` out in lines of `width` columns starting at column `indent`: an item never splits,
/// and one wider than a whole line is cut with "…".
pub fn flow<C>(items: Vec<Item<C>>, width: u16, indent: u16) -> Lines<C> {
    let width = width.saturating_sub(indent).max(1);
    let mut lines: Lines<C> = Vec::new();
    let mut line: Vec<(u16, Item<C>)> = Vec::new();
    let mut x = 0u16;
    for mut item in items {
        if item.width() > width {
            item = truncate_item(item, width);
        }
        let w = item.width();
        let gap = if line.is_empty() { 0 } else { item.gap };
        if !line.is_empty() && x + gap + w > width {
            lines.push(std::mem::take(&mut line));
            x = 0;
        }
        let gap = if line.is_empty() { 0 } else { gap };
        line.push((indent + x + gap, item));
        x += gap + w;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn truncate_item<C>(mut item: Item<C>, width: u16) -> Item<C> {
    let mut left = width.saturating_sub(1);
    let mut spans = Vec::new();
    for span in item.spans {
        if left == 0 {
            break;
        }
        let cut = truncate_to(&span.content, left);
        left -= str_width(&cut);
        spans.push(Span::styled(cut, span.style));
    }
    let style = spans.last().map(|s| s.style).unwrap_or_default();
    spans.push(Span::styled("…", style));
    item.spans = spans;
    item
}

/// Draw `item` at `(x, y)`, cut at the right edge of `clip`, reversed while it is hovered or holds
/// the keyboard focus, and record its hit region when it is a control.
pub fn draw_item<C: Clone + PartialEq>(
    buf: &mut Buffer,
    hits: &mut Hits<C>,
    hovered: &Option<C>,
    focus: &Option<C>,
    at: Position,
    item: &Item<C>,
    clip: Rect,
) {
    let Position { x, y } = at;
    let highlighted =
        item.control.is_some() && (item.control == *hovered || item.control == *focus);
    let mut cx = x;
    let limit = clip.x + clip.width;
    for span in &item.spans {
        if cx >= limit {
            break;
        }
        let mut style = span.style;
        if highlighted {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let (nx, _) = buf.set_stringn(cx, y, &span.content, usize::from(limit - cx), style);
        cx = nx;
    }
    if let Some(control) = &item.control {
        hits.record(Rect::new(x, y, item.width(), 1), clip, control.clone());
    }
}

/// Draw a one-line text field showing the end of `content`, with a cursor after it while focused.
pub fn draw_field(buf: &mut Buffer, field: Rect, content: &str, focused: bool) {
    let style = Style::default().bg(FIELD_BG);
    buf.set_style(field, style);
    let shown = tail_fitting(content, field.width.saturating_sub(1));
    buf.set_stringn(field.x, field.y, &shown, usize::from(field.width), style);
    if focused {
        let cx = field.x + str_width(&shown);
        if cx < field.x + field.width {
            buf.set_string(cx, field.y, " ", style.add_modifier(Modifier::REVERSED));
        }
    }
}

/// The longest prefix of `s` that fits in `width` columns.
pub fn truncate_to(s: &str, width: u16) -> String {
    let mut out = String::new();
    let mut w = 0u16;
    for c in s.chars() {
        let cw = char_width(c);
        if w + cw > width {
            break;
        }
        w += cw;
        out.push(c);
    }
    out
}

/// `s` cut with "…" when wider than `width` columns.
pub fn truncate(s: &str, width: u16) -> String {
    if str_width(s) <= width {
        return s.to_owned();
    }
    format!("{}…", truncate_to(s, width.saturating_sub(1)))
}

/// The longest suffix of `s` that fits in `width` columns.
pub fn tail_fitting(s: &str, width: u16) -> String {
    let mut out: Vec<char> = Vec::new();
    let mut w = 0u16;
    for c in s.chars().rev() {
        let cw = char_width(c);
        if w + cw > width {
            break;
        }
        w += cw;
        out.push(c);
    }
    out.into_iter().rev().collect()
}

/// The width of `s` in terminal cells.
pub fn str_width(s: &str) -> u16 {
    s.chars().map(char_width).sum()
}

/// `n` terminal cells as a cell count, saturating at `u16::MAX`, the widest a terminal can be.
pub fn cells(n: usize) -> u16 {
    u16::try_from(n).unwrap_or(u16::MAX)
}

fn char_width(c: char) -> u16 {
    cells(c.width().unwrap_or(0))
}

/// The part of the rect at `(x, y)` (which may start above `clip`) that lies inside `clip`.
pub fn clip_rect(x: i32, y: i32, width: u16, height: u16, clip: Rect) -> Option<Rect> {
    let top = y.max(i32::from(clip.y));
    let bottom = (y + i32::from(height)).min(i32::from(clip.y + clip.height));
    if bottom <= top {
        return None;
    }
    Some(Rect::new(
        u16::try_from(x).ok()?,
        u16::try_from(top).ok()?,
        width,
        u16::try_from(bottom - top).ok()?,
    ))
}
