//! Reading a drawn test buffer back: rows, the whole screen, and where a label was drawn.

use ratatui::buffer::Buffer;

pub fn row(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width)
        .map(|x| buf[(x, y)].symbol().to_owned())
        .collect()
}

pub fn screen(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| row(buf, y))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The cell where `needle` starts, searching rows from the top and, when `on_row_of` is given,
/// only the rows that also hold that text.
pub fn find(buf: &Buffer, needle: &str, on_row_of: Option<&str>, min_x: u16) -> (u16, u16) {
    for y in 0..buf.area.height {
        let line = row(buf, y);
        if on_row_of.is_some_and(|other| !line.contains(other)) {
            continue;
        }
        let cells: Vec<String> = (0..buf.area.width)
            .map(|x| buf[(x, y)].symbol().to_owned())
            .collect();
        for x in min_x..buf.area.width {
            if cells[x as usize..].concat().starts_with(needle) {
                return (x, y);
            }
        }
    }
    panic!("{needle:?} not drawn:\n{}", screen(buf));
}
