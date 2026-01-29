use ratatui::layout::Rect;

/// Default sidebar width in characters
pub const DEFAULT_SIDEBAR_WIDTH: u16 = 20;

/// Minimum sidebar width when collapsed
pub const COLLAPSED_SIDEBAR_WIDTH: u16 = 3;

/// Layout manager for splitting screen into sidebar and main viewport
#[derive(Debug, Clone)]
pub struct Layout {
    /// Full screen area
    area: Rect,
    /// Sidebar width (0 = hidden, COLLAPSED_SIDEBAR_WIDTH = collapsed, else = full)
    sidebar_width: u16,
    /// Whether sidebar is on the left (true) or right (false)
    sidebar_left: bool,
}

impl Layout {
    /// Create a new layout for the given area
    pub fn new(area: Rect) -> Self {
        Self {
            area,
            sidebar_width: DEFAULT_SIDEBAR_WIDTH,
            sidebar_left: true,
        }
    }

    /// Set the sidebar width
    pub fn with_sidebar_width(mut self, width: u16) -> Self {
        self.sidebar_width = width;
        self
    }

    /// Set sidebar position
    pub fn with_sidebar_left(mut self, left: bool) -> Self {
        self.sidebar_left = left;
        self
    }

    /// Get the sidebar area
    pub fn sidebar_area(&self) -> Rect {
        if self.sidebar_width == 0 {
            return Rect::default();
        }

        let width = self.sidebar_width.min(self.area.width);

        if self.sidebar_left {
            Rect {
                x: self.area.x,
                y: self.area.y,
                width,
                height: self.area.height,
            }
        } else {
            Rect {
                x: self.area.x + self.area.width.saturating_sub(width),
                y: self.area.y,
                width,
                height: self.area.height,
            }
        }
    }

    /// Get the main viewport area (terminal content)
    pub fn viewport_area(&self) -> Rect {
        if self.sidebar_width == 0 {
            return self.area;
        }

        let sidebar_w = self.sidebar_width.min(self.area.width);
        let main_width = self.area.width.saturating_sub(sidebar_w);

        if self.sidebar_left {
            Rect {
                x: self.area.x + sidebar_w,
                y: self.area.y,
                width: main_width,
                height: self.area.height,
            }
        } else {
            Rect {
                x: self.area.x,
                y: self.area.y,
                width: main_width,
                height: self.area.height,
            }
        }
    }

    /// Get the dimensions for tmux (viewport size)
    pub fn tmux_size(&self) -> (u16, u16) {
        let vp = self.viewport_area();
        (vp.width, vp.height)
    }

    /// Determine which region a point is in
    pub fn hit_test(&self, x: u16, y: u16) -> HitRegion {
        let sidebar = self.sidebar_area();
        let viewport = self.viewport_area();

        if x >= sidebar.x
            && x < sidebar.x + sidebar.width
            && y >= sidebar.y
            && y < sidebar.y + sidebar.height
        {
            // Calculate row within sidebar
            let row = y - sidebar.y;
            HitRegion::Sidebar { row }
        } else if x >= viewport.x
            && x < viewport.x + viewport.width
            && y >= viewport.y
            && y < viewport.y + viewport.height
        {
            // Calculate position within viewport
            let col = x - viewport.x;
            let row = y - viewport.y;
            HitRegion::Viewport { row, col }
        } else {
            HitRegion::None
        }
    }

    /// Update the total area (e.g., on terminal resize)
    pub fn set_area(&mut self, area: Rect) {
        self.area = area;
    }

    /// Get current sidebar width
    pub fn sidebar_width(&self) -> u16 {
        self.sidebar_width
    }

    /// Set sidebar width
    pub fn set_sidebar_width(&mut self, width: u16) {
        self.sidebar_width = width;
    }

    /// Toggle between collapsed and expanded sidebar
    pub fn toggle_sidebar(&mut self) {
        if self.sidebar_width == COLLAPSED_SIDEBAR_WIDTH {
            self.sidebar_width = DEFAULT_SIDEBAR_WIDTH;
        } else if self.sidebar_width > 0 {
            self.sidebar_width = COLLAPSED_SIDEBAR_WIDTH;
        }
    }
}

/// Result of a hit test
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitRegion {
    /// Click was in the sidebar at the given row
    Sidebar { row: u16 },
    /// Click was in the main viewport at the given position
    Viewport { row: u16, col: u16 },
    /// Click was outside any region
    None,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_layout_areas() {
        let area = Rect::new(0, 0, 100, 40);
        let layout = Layout::new(area);

        let sidebar = layout.sidebar_area();
        assert_eq!(sidebar.x, 0);
        assert_eq!(sidebar.width, DEFAULT_SIDEBAR_WIDTH);

        let viewport = layout.viewport_area();
        assert_eq!(viewport.x, DEFAULT_SIDEBAR_WIDTH);
        assert_eq!(viewport.width, 100 - DEFAULT_SIDEBAR_WIDTH);
    }

    #[test]
    fn test_hit_test() {
        let area = Rect::new(0, 0, 100, 40);
        let layout = Layout::new(area);

        // Click in sidebar
        let hit = layout.hit_test(5, 10);
        assert_eq!(hit, HitRegion::Sidebar { row: 10 });

        // Click in viewport
        let hit = layout.hit_test(50, 20);
        assert_eq!(hit, HitRegion::Viewport { row: 20, col: 50 - DEFAULT_SIDEBAR_WIDTH });
    }

    #[test]
    fn test_toggle_sidebar() {
        let area = Rect::new(0, 0, 100, 40);
        let mut layout = Layout::new(area);

        assert_eq!(layout.sidebar_width(), DEFAULT_SIDEBAR_WIDTH);

        layout.toggle_sidebar();
        assert_eq!(layout.sidebar_width(), COLLAPSED_SIDEBAR_WIDTH);

        layout.toggle_sidebar();
        assert_eq!(layout.sidebar_width(), DEFAULT_SIDEBAR_WIDTH);
    }
}
