use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Information about a single tab
#[derive(Debug, Clone)]
pub struct TabInfo {
    /// Unique identifier (tmux window ID like "@1")
    pub id: String,
    /// Display name
    pub name: String,
    /// Whether this tab is currently active
    pub active: bool,
    /// Whether there's unseen activity
    pub activity: bool,
    /// Tab index (1-based for display)
    pub index: usize,
}

/// Mode indicator for the sidebar
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarMode {
    /// Normal mode
    Normal,
    /// Prefix key was pressed, waiting for command
    Prefix,
    /// Renaming a tab
    Rename,
}

/// Widget that renders the sidebar with tab list
pub struct Sidebar<'a> {
    tabs: &'a [TabInfo],
    collapsed: bool,
    mode: SidebarMode,
}

impl<'a> Sidebar<'a> {
    pub fn new(tabs: &'a [TabInfo]) -> Self {
        Self {
            tabs,
            collapsed: false,
            mode: SidebarMode::Normal,
        }
    }

    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    pub fn mode(mut self, mode: SidebarMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Widget for Sidebar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        // Draw background
        let bg_style = Style::default().bg(Color::DarkGray);
        for y in area.y..area.y + area.height {
            for x in area.x..area.x + area.width {
                buf.set_string(x, y, " ", bg_style);
            }
        }

        // Draw border on the right edge
        let border_style = Style::default().fg(Color::Gray).bg(Color::DarkGray);
        let border_x = area.x + area.width - 1;
        for y in area.y..area.y + area.height {
            buf.set_string(border_x, y, "│", border_style);
        }

        // Content area (excluding border)
        let content_width = area.width.saturating_sub(1);

        // Draw mode indicator at top if not in normal mode
        let tabs_start_y = self.render_mode_indicator(area, buf, content_width);

        // Adjust area for tabs
        let tabs_area = Rect {
            x: area.x,
            y: tabs_start_y,
            width: area.width,
            height: area.height.saturating_sub(tabs_start_y - area.y),
        };

        if self.collapsed {
            self.render_collapsed(tabs_area, buf, content_width);
        } else {
            self.render_expanded(tabs_area, buf, content_width);
        }

        // Draw [+] button at bottom
        self.render_new_tab_button(area, buf, content_width);
    }
}

impl Sidebar<'_> {
    /// Render mode indicator at top of sidebar, returns the y position where tabs should start
    fn render_mode_indicator(&self, area: Rect, buf: &mut Buffer, content_width: u16) -> u16 {
        match self.mode {
            SidebarMode::Normal => area.y, // No indicator in normal mode
            SidebarMode::Prefix => {
                let style = Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD);
                let text = if content_width >= 10 {
                    "-- ^B --"
                } else {
                    "^B"
                };
                let fill = " ".repeat(content_width as usize);
                buf.set_string(area.x, area.y, &fill, style);
                buf.set_string(area.x, area.y, text, style);
                area.y + 1
            }
            SidebarMode::Rename => {
                let style = Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD);
                let text = if content_width >= 10 {
                    "RENAME"
                } else {
                    "REN"
                };
                let fill = " ".repeat(content_width as usize);
                buf.set_string(area.x, area.y, &fill, style);
                buf.set_string(area.x, area.y, text, style);
                area.y + 1
            }
        }
    }

    fn render_collapsed(&self, area: Rect, buf: &mut Buffer, content_width: u16) {
        // Collapsed mode: show only indicator and number
        // Format: "● 1" or "  2" or "* 3"
        for (i, tab) in self.tabs.iter().enumerate() {
            if i as u16 >= area.height.saturating_sub(1) {
                break;
            }

            let y = area.y + i as u16;
            let indicator = if tab.active {
                "●"
            } else if tab.activity {
                "*"
            } else {
                " "
            };

            let style = if tab.active {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else if tab.activity {
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::DarkGray)
            } else {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
            };

            let text = format!("{}{}", indicator, tab.index);
            let text = truncate_to_width(&text, content_width as usize);
            buf.set_string(area.x, y, text, style);
        }
    }

    fn render_expanded(&self, area: Rect, buf: &mut Buffer, content_width: u16) {
        // Expanded mode: show full tab names
        // Format: "● 1: tab-name" or "  2: other-tab"
        for (i, tab) in self.tabs.iter().enumerate() {
            if i as u16 >= area.height.saturating_sub(1) {
                break;
            }

            let y = area.y + i as u16;
            let indicator = if tab.active {
                "●"
            } else if tab.activity {
                "*"
            } else {
                " "
            };

            let style = if tab.active {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else if tab.activity {
                Style::default()
                    .fg(Color::Yellow)
                    .bg(Color::DarkGray)
            } else {
                Style::default()
                    .fg(Color::White)
                    .bg(Color::DarkGray)
            };

            // Format: "● 1: name"
            let text = format!("{} {}: {}", indicator, tab.index, tab.name);
            let text = truncate_to_width(&text, content_width as usize);

            // Fill the entire row with background color first
            let fill = " ".repeat(content_width as usize);
            buf.set_string(area.x, y, &fill, style);
            buf.set_string(area.x, y, text, style);
        }
    }

    fn render_new_tab_button(&self, area: Rect, buf: &mut Buffer, content_width: u16) {
        if area.height == 0 {
            return;
        }

        let y = area.y + area.height - 1;
        let style = Style::default()
            .fg(Color::Green)
            .bg(Color::DarkGray);

        let text = if content_width >= 9 {
            "[+] New"
        } else {
            "[+]"
        };

        // Fill row first
        let fill = " ".repeat(content_width as usize);
        buf.set_string(area.x, y, &fill, Style::default().bg(Color::DarkGray));
        buf.set_string(area.x, y, text, style);
    }
}

/// Truncate a string to fit within a given width
fn truncate_to_width(s: &str, max_width: usize) -> String {
    if s.len() <= max_width {
        s.to_string()
    } else if max_width >= 3 {
        format!("{}...", &s[..max_width - 3])
    } else {
        s.chars().take(max_width).collect()
    }
}

/// Calculate which tab index was clicked given a row in the sidebar
/// Returns None if the click was on the [+] button or outside tabs
/// `header_rows` is the number of rows used by mode indicator (0 in normal mode, 1 in prefix/rename)
pub fn row_to_tab_index(row: u16, num_tabs: usize, area_height: u16, header_rows: u16) -> Option<usize> {
    // Account for header rows (mode indicator)
    if row < header_rows {
        return None;
    }
    let adjusted_row = (row - header_rows) as usize;

    // Last row is the [+] button
    if row >= area_height.saturating_sub(1) {
        return None;
    }

    // Check if row corresponds to a tab
    if adjusted_row < num_tabs {
        Some(adjusted_row)
    } else {
        None
    }
}

/// Check if a row is the [+] new tab button
pub fn is_new_tab_button(row: u16, area_height: u16) -> bool {
    row == area_height.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_to_width() {
        assert_eq!(truncate_to_width("hello", 10), "hello");
        assert_eq!(truncate_to_width("hello world", 8), "hello...");
        assert_eq!(truncate_to_width("hi", 2), "hi");
    }

    #[test]
    fn test_row_to_tab_index() {
        // 3 tabs, height 10 (last row is [+]), no header
        assert_eq!(row_to_tab_index(0, 3, 10, 0), Some(0));
        assert_eq!(row_to_tab_index(1, 3, 10, 0), Some(1));
        assert_eq!(row_to_tab_index(2, 3, 10, 0), Some(2));
        assert_eq!(row_to_tab_index(3, 3, 10, 0), None); // No tab at row 3
        assert_eq!(row_to_tab_index(9, 3, 10, 0), None); // [+] button row

        // With 1 header row (prefix/rename mode)
        assert_eq!(row_to_tab_index(0, 3, 10, 1), None); // Header row
        assert_eq!(row_to_tab_index(1, 3, 10, 1), Some(0)); // First tab
        assert_eq!(row_to_tab_index(2, 3, 10, 1), Some(1)); // Second tab
        assert_eq!(row_to_tab_index(3, 3, 10, 1), Some(2)); // Third tab
        assert_eq!(row_to_tab_index(4, 3, 10, 1), None); // No tab at row 4
    }

    #[test]
    fn test_is_new_tab_button() {
        assert!(!is_new_tab_button(0, 10));
        assert!(!is_new_tab_button(8, 10));
        assert!(is_new_tab_button(9, 10));
    }
}
