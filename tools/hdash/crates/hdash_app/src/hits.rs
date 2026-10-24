//! The hit regions of the last frame: every clickable thing records its rect and control in the
//! call that draws it, and a point resolves to the topmost control under it.

use ratatui::layout::{Position, Rect};

/// The clickable regions of one frame, in drawing order.
#[derive(Debug)]
pub struct Hits<C> {
    regions: Vec<(Rect, C)>,
}

impl<C> Default for Hits<C> {
    fn default() -> Self {
        Self {
            regions: Vec::new(),
        }
    }
}

impl<C: Clone> Hits<C> {
    /// Record `control` over the part of `area` that lies inside `clip`; nothing when they do not
    /// overlap.
    pub fn record(&mut self, area: Rect, clip: Rect, control: C) {
        let visible = area.intersection(clip);
        if visible.width > 0 && visible.height > 0 {
            self.regions.push((visible, control));
        }
    }

    /// The control drawn last under the cell `(x, y)`.
    pub fn at(&self, x: u16, y: u16) -> Option<C> {
        self.regions
            .iter()
            .rev()
            .find(|(r, _)| r.contains(Position::new(x, y)))
            .map(|(_, c)| c.clone())
    }

    pub fn clear(&mut self) {
        self.regions.clear();
    }
}
