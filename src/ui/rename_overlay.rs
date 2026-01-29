use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// A modal overlay for renaming tabs
pub struct RenameOverlay<'a> {
    /// Current input text
    text: &'a str,
}

impl<'a> RenameOverlay<'a> {
    pub fn new(text: &'a str) -> Self {
        Self { text }
    }

    /// Calculate the centered area for the overlay
    pub fn centered_rect(area: Rect) -> Rect {
        let width = 40.min(area.width.saturating_sub(4));
        let height = 3;
        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;
        Rect::new(x, y, width, height)
    }
}

impl Widget for RenameOverlay<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first
        Clear.render(area, buf);

        // Draw the box
        let block = Block::default()
            .title(" Rename Tab ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(area);
        block.render(area, buf);

        // Draw the input text with cursor
        let display_text = format!("{}▏", self.text);
        let input = Paragraph::new(display_text).style(Style::default().fg(Color::White));

        input.render(inner, buf);
    }
}
