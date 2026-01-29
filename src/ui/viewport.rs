use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::terminal::{Cell, CellAttributes, TerminalBuffer};

/// Widget that renders a terminal buffer to the screen
pub struct Viewport<'a> {
    buffer: &'a TerminalBuffer,
    show_cursor: bool,
}

impl<'a> Viewport<'a> {
    pub fn new(buffer: &'a TerminalBuffer) -> Self {
        Self {
            buffer,
            show_cursor: true,
        }
    }

    pub fn show_cursor(mut self, show: bool) -> Self {
        self.show_cursor = show;
        self
    }
}

impl Widget for Viewport<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let cells = self.buffer.cells();
        let (cursor_row, cursor_col) = self.buffer.cursor();

        // Render each cell from the terminal buffer
        for (row_idx, row) in cells.iter().enumerate() {
            if row_idx as u16 >= area.height {
                break;
            }

            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx as u16 >= area.width {
                    break;
                }

                let x = area.x + col_idx as u16;
                let y = area.y + row_idx as u16;

                // Check if this is the cursor position
                let is_cursor = self.show_cursor
                    && self.buffer.cursor_visible()
                    && row_idx as u16 == cursor_row
                    && col_idx as u16 == cursor_col;

                let style = cell_to_style(cell, is_cursor);
                let ch = if cell.character.is_control() {
                    ' '
                } else {
                    cell.character
                };

                buf.set_string(x, y, ch.to_string(), style);
            }
        }
    }
}

/// Convert a terminal Cell to a ratatui Style
fn cell_to_style(cell: &Cell, is_cursor: bool) -> Style {
    let mut style = Style::default();

    // Set foreground color - map dark colors to lighter variants for visibility
    let fg = match cell.fg {
        Color::Reset => Color::White,
        Color::Black => Color::DarkGray,      // Make black visible
        Color::DarkGray => Color::Gray,       // Make dark gray lighter
        c => c,
    };
    style = style.fg(fg);

    // Set background color - use terminal default for Reset
    let bg = match cell.bg {
        Color::Reset => Color::Reset,  // Use terminal's default background
        c => c,
    };
    style = style.bg(bg);

    // Apply attributes
    let modifier = attrs_to_modifier(&cell.attrs);
    style = style.add_modifier(modifier);

    // If this is the cursor, invert colors
    if is_cursor {
        style = style.add_modifier(Modifier::REVERSED);
    }

    style
}

/// Convert CellAttributes to ratatui Modifier
fn attrs_to_modifier(attrs: &CellAttributes) -> Modifier {
    let mut m = Modifier::empty();
    if attrs.bold {
        m |= Modifier::BOLD;
    }
    if attrs.italic {
        m |= Modifier::ITALIC;
    }
    if attrs.underline {
        m |= Modifier::UNDERLINED;
    }
    if attrs.blink {
        m |= Modifier::SLOW_BLINK;
    }
    if attrs.reverse {
        m |= Modifier::REVERSED;
    }
    if attrs.hidden {
        m |= Modifier::HIDDEN;
    }
    if attrs.strikethrough {
        m |= Modifier::CROSSED_OUT;
    }
    m
}
