use ratatui::style::{Color, Modifier};
use std::collections::VecDeque;
use vte::{Params, Perform};

/// Default scrollback buffer size (number of lines)
const DEFAULT_SCROLLBACK: usize = 1000;

/// Attributes that can be applied to a cell
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellAttributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub blink: bool,
    pub reverse: bool,
    pub hidden: bool,
    pub strikethrough: bool,
}

impl CellAttributes {
    pub fn to_modifier(&self) -> Modifier {
        let mut m = Modifier::empty();
        if self.bold {
            m |= Modifier::BOLD;
        }
        if self.italic {
            m |= Modifier::ITALIC;
        }
        if self.underline {
            m |= Modifier::UNDERLINED;
        }
        if self.blink {
            m |= Modifier::SLOW_BLINK;
        }
        if self.reverse {
            m |= Modifier::REVERSED;
        }
        if self.hidden {
            m |= Modifier::HIDDEN;
        }
        if self.strikethrough {
            m |= Modifier::CROSSED_OUT;
        }
        m
    }
}

/// A single cell in the terminal buffer
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    pub character: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttributes,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            character: ' ',
            fg: Color::Reset,
            bg: Color::Reset,
            attrs: CellAttributes::default(),
        }
    }
}

impl Cell {
    pub fn new(c: char) -> Self {
        Self {
            character: c,
            ..Default::default()
        }
    }

    pub fn with_style(c: char, fg: Color, bg: Color, attrs: CellAttributes) -> Self {
        Self {
            character: c,
            fg,
            bg,
            attrs,
        }
    }
}

/// The terminal screen buffer
pub struct TerminalBuffer {
    /// Buffer width in columns
    width: u16,
    /// Buffer height in rows
    height: u16,
    /// The visible screen area (height rows of width cells each)
    cells: Vec<Vec<Cell>>,
    /// Cursor position (row, col) - 0-indexed
    cursor_row: u16,
    cursor_col: u16,
    /// Whether cursor is visible
    cursor_visible: bool,
    /// Scrollback buffer (lines that scrolled off the top)
    scrollback: VecDeque<Vec<Cell>>,
    /// Maximum scrollback lines
    scrollback_limit: usize,
    /// Current text attributes for new characters
    current_fg: Color,
    current_bg: Color,
    current_attrs: CellAttributes,
    /// Scroll region (top, bottom) - 0-indexed, inclusive
    scroll_top: u16,
    scroll_bottom: u16,
    /// Saved cursor position for save/restore
    saved_cursor: Option<(u16, u16)>,
    /// Origin mode - cursor positions relative to scroll region
    origin_mode: bool,
}

impl TerminalBuffer {
    /// Create a new terminal buffer with the given dimensions
    pub fn new(width: u16, height: u16) -> Self {
        let cells = vec![vec![Cell::default(); width as usize]; height as usize];
        Self {
            width,
            height,
            cells,
            cursor_row: 0,
            cursor_col: 0,
            cursor_visible: true,
            scrollback: VecDeque::with_capacity(DEFAULT_SCROLLBACK),
            scrollback_limit: DEFAULT_SCROLLBACK,
            current_fg: Color::Reset,
            current_bg: Color::Reset,
            current_attrs: CellAttributes::default(),
            scroll_top: 0,
            scroll_bottom: height.saturating_sub(1),
            saved_cursor: None,
            origin_mode: false,
        }
    }

    /// Process raw bytes from terminal output
    pub fn process(&mut self, data: &[u8]) {
        let mut parser = vte::Parser::new();
        for byte in data {
            parser.advance(self, *byte);
        }
    }

    /// Get buffer dimensions
    pub fn size(&self) -> (u16, u16) {
        (self.width, self.height)
    }

    /// Get cursor position
    pub fn cursor(&self) -> (u16, u16) {
        (self.cursor_row, self.cursor_col)
    }

    /// Check if cursor is visible
    pub fn cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    /// Get a reference to the cells grid
    pub fn cells(&self) -> &Vec<Vec<Cell>> {
        &self.cells
    }

    /// Get a cell at the given position
    pub fn get_cell(&self, row: u16, col: u16) -> Option<&Cell> {
        self.cells
            .get(row as usize)
            .and_then(|r| r.get(col as usize))
    }

    /// Resize the buffer
    pub fn resize(&mut self, new_width: u16, new_height: u16) {
        if new_width == self.width && new_height == self.height {
            return;
        }

        // Resize existing rows
        for row in &mut self.cells {
            row.resize(new_width as usize, Cell::default());
        }

        // Add or remove rows
        self.cells
            .resize(new_height as usize, vec![Cell::default(); new_width as usize]);

        self.width = new_width;
        self.height = new_height;

        // Adjust scroll region
        self.scroll_bottom = new_height.saturating_sub(1);
        if self.scroll_top >= new_height {
            self.scroll_top = 0;
        }

        // Clamp cursor
        self.cursor_row = self.cursor_row.min(new_height.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(new_width.saturating_sub(1));
    }

    /// Clear the entire screen
    pub fn clear(&mut self) {
        for row in &mut self.cells {
            for cell in row {
                *cell = Cell::default();
            }
        }
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    /// Clear from cursor to end of screen
    fn clear_to_end_of_screen(&mut self) {
        // Clear rest of current line
        self.clear_to_end_of_line();

        // Clear all lines below
        for row in (self.cursor_row + 1) as usize..self.height as usize {
            for cell in &mut self.cells[row] {
                *cell = Cell::default();
            }
        }
    }

    /// Clear from start of screen to cursor
    fn clear_to_start_of_screen(&mut self) {
        // Clear all lines above
        for row in 0..self.cursor_row as usize {
            for cell in &mut self.cells[row] {
                *cell = Cell::default();
            }
        }

        // Clear start of current line
        self.clear_to_start_of_line();
    }

    /// Clear the current line
    fn clear_line(&mut self) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for cell in row {
                *cell = Cell::default();
            }
        }
    }

    /// Clear from cursor to end of line
    fn clear_to_end_of_line(&mut self) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for col in self.cursor_col as usize..self.width as usize {
                if let Some(cell) = row.get_mut(col) {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Clear from start of line to cursor
    fn clear_to_start_of_line(&mut self) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for col in 0..=self.cursor_col as usize {
                if let Some(cell) = row.get_mut(col) {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Write a character at the current cursor position
    fn write_char(&mut self, c: char) {
        if self.cursor_col >= self.width {
            // Wrap to next line
            self.cursor_col = 0;
            self.move_cursor_down(1);
        }

        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            if let Some(cell) = row.get_mut(self.cursor_col as usize) {
                *cell = Cell::with_style(c, self.current_fg, self.current_bg, self.current_attrs);
            }
        }

        self.cursor_col += 1;
    }

    /// Move cursor down, scrolling if necessary
    fn move_cursor_down(&mut self, count: u16) {
        for _ in 0..count {
            if self.cursor_row >= self.scroll_bottom {
                self.scroll_up(1);
            } else {
                self.cursor_row += 1;
            }
        }
    }

    /// Move cursor up
    fn move_cursor_up(&mut self, count: u16) {
        self.cursor_row = self.cursor_row.saturating_sub(count).max(self.scroll_top);
    }

    /// Scroll the screen up (content moves up, new blank line at bottom)
    fn scroll_up(&mut self, count: u16) {
        for _ in 0..count {
            // Move top line of scroll region to scrollback
            if self.scroll_top == 0 {
                let line = self.cells[0].clone();
                if self.scrollback.len() >= self.scrollback_limit {
                    self.scrollback.pop_front();
                }
                self.scrollback.push_back(line);
            }

            // Shift lines up within scroll region
            for row in self.scroll_top as usize..self.scroll_bottom as usize {
                self.cells.swap(row, row + 1);
            }

            // Clear the bottom line of scroll region
            if let Some(row) = self.cells.get_mut(self.scroll_bottom as usize) {
                for cell in row {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Scroll the screen down (content moves down, new blank line at top)
    fn scroll_down(&mut self, count: u16) {
        for _ in 0..count {
            // Shift lines down within scroll region
            for row in ((self.scroll_top as usize + 1)..=self.scroll_bottom as usize).rev() {
                self.cells.swap(row, row - 1);
            }

            // Clear the top line of scroll region
            if let Some(row) = self.cells.get_mut(self.scroll_top as usize) {
                for cell in row {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Set cursor position (1-indexed input, converted to 0-indexed)
    fn set_cursor_position(&mut self, row: u16, col: u16) {
        let row = row.saturating_sub(1); // Convert from 1-indexed
        let col = col.saturating_sub(1);

        let (min_row, max_row) = if self.origin_mode {
            (self.scroll_top, self.scroll_bottom)
        } else {
            (0, self.height.saturating_sub(1))
        };

        self.cursor_row = row.clamp(min_row, max_row);
        self.cursor_col = col.min(self.width.saturating_sub(1));
    }

    /// Insert blank lines at cursor position
    fn insert_lines(&mut self, count: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }

        for _ in 0..count {
            // Shift lines down from cursor to bottom of scroll region
            for row in ((self.cursor_row as usize + 1)..=self.scroll_bottom as usize).rev() {
                self.cells.swap(row, row - 1);
            }

            // Clear the line at cursor
            if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
                for cell in row {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Delete lines at cursor position
    fn delete_lines(&mut self, count: u16) {
        if self.cursor_row < self.scroll_top || self.cursor_row > self.scroll_bottom {
            return;
        }

        for _ in 0..count {
            // Shift lines up from cursor to bottom of scroll region
            for row in self.cursor_row as usize..self.scroll_bottom as usize {
                self.cells.swap(row, row + 1);
            }

            // Clear the bottom line of scroll region
            if let Some(row) = self.cells.get_mut(self.scroll_bottom as usize) {
                for cell in row {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Delete characters at cursor position
    fn delete_chars(&mut self, count: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let start = self.cursor_col as usize;
            let count = count as usize;
            let width = self.width as usize;

            // Shift characters left
            for col in start..width {
                let src = col + count;
                row[col] = if src < width {
                    row[src].clone()
                } else {
                    Cell::default()
                };
            }
        }
    }

    /// Insert blank characters at cursor position
    fn insert_chars(&mut self, count: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            let start = self.cursor_col as usize;
            let count = count as usize;
            let width = self.width as usize;

            // Shift characters right
            for col in (start..width).rev() {
                let dst = col + count;
                if dst < width {
                    row.swap(col, dst);
                }
            }

            // Clear inserted positions
            for col in start..(start + count).min(width) {
                row[col] = Cell::default();
            }
        }
    }

    /// Erase characters (replace with blanks, don't shift)
    fn erase_chars(&mut self, count: u16) {
        if let Some(row) = self.cells.get_mut(self.cursor_row as usize) {
            for col in self.cursor_col as usize..(self.cursor_col + count) as usize {
                if let Some(cell) = row.get_mut(col) {
                    *cell = Cell::default();
                }
            }
        }
    }

    /// Handle carriage return
    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    /// Handle newline/line feed
    fn linefeed(&mut self) {
        self.move_cursor_down(1);
    }

    /// Handle backspace
    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        }
    }

    /// Handle tab
    fn tab(&mut self) {
        // Move to next tab stop (every 8 columns)
        let next_tab = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next_tab.min(self.width.saturating_sub(1));
    }

    /// Reset all attributes to defaults
    fn reset_attributes(&mut self) {
        self.current_fg = Color::Reset;
        self.current_bg = Color::Reset;
        self.current_attrs = CellAttributes::default();
    }

    /// Set scroll region (1-indexed input)
    fn set_scroll_region(&mut self, top: u16, bottom: u16) {
        let top = top.saturating_sub(1).min(self.height.saturating_sub(1));
        let bottom = bottom.saturating_sub(1).min(self.height.saturating_sub(1));

        if top < bottom {
            self.scroll_top = top;
            self.scroll_bottom = bottom;
            // Move cursor to home position
            self.set_cursor_position(1, 1);
        }
    }

    /// Handle SGR (Select Graphic Rendition) parameters
    fn handle_sgr(&mut self, params: &[u16]) {
        if params.is_empty() {
            self.reset_attributes();
            return;
        }

        let mut iter = params.iter().peekable();
        while let Some(&param) = iter.next() {
            match param {
                0 => self.reset_attributes(),
                1 => self.current_attrs.bold = true,
                2 => {} // Dim (not widely supported)
                3 => self.current_attrs.italic = true,
                4 => self.current_attrs.underline = true,
                5 | 6 => self.current_attrs.blink = true,
                7 => self.current_attrs.reverse = true,
                8 => self.current_attrs.hidden = true,
                9 => self.current_attrs.strikethrough = true,

                21 => self.current_attrs.bold = false,
                22 => self.current_attrs.bold = false, // Normal intensity
                23 => self.current_attrs.italic = false,
                24 => self.current_attrs.underline = false,
                25 => self.current_attrs.blink = false,
                27 => self.current_attrs.reverse = false,
                28 => self.current_attrs.hidden = false,
                29 => self.current_attrs.strikethrough = false,

                // Standard foreground colors
                30..=37 => self.current_fg = ansi_to_color(param - 30),
                38 => {
                    // Extended foreground color
                    if let Some(&&mode) = iter.peek() {
                        iter.next();
                        match mode {
                            5 => {
                                // 256-color mode
                                if let Some(&&color) = iter.peek() {
                                    iter.next();
                                    self.current_fg = ansi_to_color(color);
                                }
                            }
                            2 => {
                                // RGB mode
                                let r = iter.next().copied().unwrap_or(0) as u8;
                                let g = iter.next().copied().unwrap_or(0) as u8;
                                let b = iter.next().copied().unwrap_or(0) as u8;
                                self.current_fg = Color::Rgb(r, g, b);
                            }
                            _ => {}
                        }
                    }
                }
                39 => self.current_fg = Color::Reset, // Default foreground

                // Standard background colors
                40..=47 => self.current_bg = ansi_to_color(param - 40),
                48 => {
                    // Extended background color
                    if let Some(&&mode) = iter.peek() {
                        iter.next();
                        match mode {
                            5 => {
                                // 256-color mode
                                if let Some(&&color) = iter.peek() {
                                    iter.next();
                                    self.current_bg = ansi_to_color(color);
                                }
                            }
                            2 => {
                                // RGB mode
                                let r = iter.next().copied().unwrap_or(0) as u8;
                                let g = iter.next().copied().unwrap_or(0) as u8;
                                let b = iter.next().copied().unwrap_or(0) as u8;
                                self.current_bg = Color::Rgb(r, g, b);
                            }
                            _ => {}
                        }
                    }
                }
                49 => self.current_bg = Color::Reset, // Default background

                // Bright foreground colors
                90..=97 => self.current_fg = ansi_to_color(param - 90 + 8),
                // Bright background colors
                100..=107 => self.current_bg = ansi_to_color(param - 100 + 8),

                _ => {}
            }
        }
    }
}

/// Convert ANSI color code to ratatui Color
fn ansi_to_color(code: u16) -> Color {
    match code {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::White,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        15 => Color::Gray,
        16..=231 => {
            // 216 color cube: 16 + 36*r + 6*g + b
            let c = code - 16;
            let r = (c / 36) * 51;
            let g = ((c / 6) % 6) * 51;
            let b = (c % 6) * 51;
            Color::Rgb(r as u8, g as u8, b as u8)
        }
        232..=255 => {
            // Grayscale: 24 shades
            let gray = ((code - 232) * 10 + 8) as u8;
            Color::Rgb(gray, gray, gray)
        }
        _ => Color::Reset,
    }
}

// Implement VTE Perform trait for terminal emulation
impl Perform for TerminalBuffer {
    fn print(&mut self, c: char) {
        self.write_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            0x07 => {
                // BEL - Bell (ignore for now)
            }
            0x08 => {
                // BS - Backspace
                self.backspace();
            }
            0x09 => {
                // HT - Horizontal Tab
                self.tab();
            }
            0x0A | 0x0B | 0x0C => {
                // LF, VT, FF - Line Feed
                self.linefeed();
            }
            0x0D => {
                // CR - Carriage Return
                self.carriage_return();
            }
            _ => {}
        }
    }

    fn hook(&mut self, _params: &Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

    fn put(&mut self, _byte: u8) {}

    fn unhook(&mut self) {}

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        // OSC sequences we care about:
        // OSC 0 ; title BEL - Set icon name and window title
        // OSC 2 ; title BEL - Set window title
        if let Some(&code) = params.first() {
            if code == b"0" || code == b"2" {
                if let Some(_title) = params.get(1) {
                    // TODO: Emit event for title change
                }
            }
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let params: Vec<u16> = params.iter().flat_map(|p| p.first().copied()).collect();

        match action {
            // Cursor movement
            'A' => {
                // CUU - Cursor Up
                let n = params.first().copied().unwrap_or(1).max(1);
                self.move_cursor_up(n);
            }
            'B' | 'e' => {
                // CUD - Cursor Down, VPR - Vertical Position Relative
                let n = params.first().copied().unwrap_or(1).max(1);
                self.move_cursor_down(n);
            }
            'C' | 'a' => {
                // CUF - Cursor Forward, HPR - Horizontal Position Relative
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = (self.cursor_col + n).min(self.width.saturating_sub(1));
            }
            'D' => {
                // CUB - Cursor Back
                let n = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = self.cursor_col.saturating_sub(n);
            }
            'E' => {
                // CNL - Cursor Next Line
                let n = params.first().copied().unwrap_or(1).max(1);
                self.move_cursor_down(n);
                self.carriage_return();
            }
            'F' => {
                // CPL - Cursor Previous Line
                let n = params.first().copied().unwrap_or(1).max(1);
                self.move_cursor_up(n);
                self.carriage_return();
            }
            'G' | '`' => {
                // CHA - Cursor Horizontal Absolute, HPA
                let col = params.first().copied().unwrap_or(1).max(1);
                self.cursor_col = (col - 1).min(self.width.saturating_sub(1));
            }
            'H' | 'f' => {
                // CUP - Cursor Position, HVP
                let row = params.first().copied().unwrap_or(1);
                let col = params.get(1).copied().unwrap_or(1);
                self.set_cursor_position(row, col);
            }
            'd' => {
                // VPA - Vertical Position Absolute
                let row = params.first().copied().unwrap_or(1);
                self.set_cursor_position(row, self.cursor_col + 1);
            }

            // Erasing
            'J' => {
                // ED - Erase Display
                match params.first().copied().unwrap_or(0) {
                    0 => self.clear_to_end_of_screen(),
                    1 => self.clear_to_start_of_screen(),
                    2 | 3 => self.clear(),
                    _ => {}
                }
            }
            'K' => {
                // EL - Erase Line
                match params.first().copied().unwrap_or(0) {
                    0 => self.clear_to_end_of_line(),
                    1 => self.clear_to_start_of_line(),
                    2 => self.clear_line(),
                    _ => {}
                }
            }

            // Line operations
            'L' => {
                // IL - Insert Lines
                let n = params.first().copied().unwrap_or(1).max(1);
                self.insert_lines(n);
            }
            'M' => {
                // DL - Delete Lines
                let n = params.first().copied().unwrap_or(1).max(1);
                self.delete_lines(n);
            }

            // Character operations
            'P' => {
                // DCH - Delete Characters
                let n = params.first().copied().unwrap_or(1).max(1);
                self.delete_chars(n);
            }
            '@' => {
                // ICH - Insert Characters
                let n = params.first().copied().unwrap_or(1).max(1);
                self.insert_chars(n);
            }
            'X' => {
                // ECH - Erase Characters
                let n = params.first().copied().unwrap_or(1).max(1);
                self.erase_chars(n);
            }

            // Scrolling
            'S' => {
                // SU - Scroll Up
                let n = params.first().copied().unwrap_or(1).max(1);
                self.scroll_up(n);
            }
            'T' => {
                // SD - Scroll Down
                let n = params.first().copied().unwrap_or(1).max(1);
                self.scroll_down(n);
            }
            'r' => {
                // DECSTBM - Set Scrolling Region
                let top = params.first().copied().unwrap_or(1);
                let bottom = params.get(1).copied().unwrap_or(self.height);
                self.set_scroll_region(top, bottom);
            }

            // SGR - Select Graphic Rendition
            'm' => {
                self.handle_sgr(&params);
            }

            // Mode setting
            'h' => {
                // SM - Set Mode
                if intermediates.first() == Some(&b'?') {
                    // DEC Private Mode Set
                    for param in &params {
                        match param {
                            25 => self.cursor_visible = true,   // DECTCEM - Show Cursor
                            6 => self.origin_mode = true,       // DECOM
                            _ => {}
                        }
                    }
                }
            }
            'l' => {
                // RM - Reset Mode
                if intermediates.first() == Some(&b'?') {
                    // DEC Private Mode Reset
                    for param in &params {
                        match param {
                            25 => self.cursor_visible = false,  // DECTCEM - Hide Cursor
                            6 => self.origin_mode = false,      // DECOM
                            _ => {}
                        }
                    }
                }
            }

            // Cursor save/restore
            's' => {
                // SCP - Save Cursor Position
                self.saved_cursor = Some((self.cursor_row, self.cursor_col));
            }
            'u' => {
                // RCP - Restore Cursor Position
                if let Some((row, col)) = self.saved_cursor {
                    self.cursor_row = row;
                    self.cursor_col = col;
                }
            }

            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            ([], b'7') => {
                // DECSC - Save Cursor
                self.saved_cursor = Some((self.cursor_row, self.cursor_col));
            }
            ([], b'8') => {
                // DECRC - Restore Cursor
                if let Some((row, col)) = self.saved_cursor {
                    self.cursor_row = row;
                    self.cursor_col = col;
                }
            }
            ([], b'D') => {
                // IND - Index (move down, scroll if needed)
                self.linefeed();
            }
            ([], b'E') => {
                // NEL - Next Line
                self.linefeed();
                self.carriage_return();
            }
            ([], b'M') => {
                // RI - Reverse Index (move up, scroll if needed)
                if self.cursor_row <= self.scroll_top {
                    self.scroll_down(1);
                } else {
                    self.cursor_row -= 1;
                }
            }
            ([], b'c') => {
                // RIS - Reset to Initial State
                self.clear();
                self.reset_attributes();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_buffer() {
        let buf = TerminalBuffer::new(80, 24);
        assert_eq!(buf.size(), (80, 24));
        assert_eq!(buf.cursor(), (0, 0));
    }

    #[test]
    fn test_write_char() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.write_char('H');
        buf.write_char('i');
        assert_eq!(buf.cursor(), (0, 2));
        assert_eq!(buf.get_cell(0, 0).unwrap().character, 'H');
        assert_eq!(buf.get_cell(0, 1).unwrap().character, 'i');
    }

    #[test]
    fn test_line_wrap() {
        let mut buf = TerminalBuffer::new(5, 3);
        for c in "Hello World".chars() {
            buf.write_char(c);
        }
        // "Hello" on line 0, " Worl" on line 1, "d" on line 2
        assert_eq!(buf.get_cell(0, 0).unwrap().character, 'H');
        assert_eq!(buf.get_cell(1, 0).unwrap().character, ' ');
        assert_eq!(buf.get_cell(2, 0).unwrap().character, 'd');
    }

    #[test]
    fn test_clear() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.write_char('X');
        buf.clear();
        assert_eq!(buf.get_cell(0, 0).unwrap().character, ' ');
        assert_eq!(buf.cursor(), (0, 0));
    }

    #[test]
    fn test_resize() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.resize(40, 12);
        assert_eq!(buf.size(), (40, 12));
    }

    #[test]
    fn test_scroll_up() {
        let mut buf = TerminalBuffer::new(80, 3);
        buf.write_char('1');
        buf.linefeed();
        buf.carriage_return();
        buf.write_char('2');
        buf.linefeed();
        buf.carriage_return();
        buf.write_char('3');
        buf.linefeed(); // This should scroll
        buf.carriage_return();
        buf.write_char('4');

        // Line with '1' should have scrolled off
        assert_eq!(buf.get_cell(0, 0).unwrap().character, '2');
        assert_eq!(buf.get_cell(1, 0).unwrap().character, '3');
        assert_eq!(buf.get_cell(2, 0).unwrap().character, '4');
    }

    #[test]
    fn test_process_text() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process(b"Hello");
        assert_eq!(buf.get_cell(0, 0).unwrap().character, 'H');
        assert_eq!(buf.get_cell(0, 4).unwrap().character, 'o');
        assert_eq!(buf.cursor(), (0, 5));
    }

    #[test]
    fn test_process_newline() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process(b"Line1\r\nLine2");
        assert_eq!(buf.get_cell(0, 0).unwrap().character, 'L');
        assert_eq!(buf.get_cell(1, 0).unwrap().character, 'L');
    }

    #[test]
    fn test_process_cursor_movement() {
        let mut buf = TerminalBuffer::new(80, 24);
        // Move cursor to row 5, col 10
        buf.process(b"\x1b[5;10H");
        assert_eq!(buf.cursor(), (4, 9)); // 0-indexed

        // Move cursor up 2
        buf.process(b"\x1b[2A");
        assert_eq!(buf.cursor(), (2, 9));
    }

    #[test]
    fn test_process_clear_screen() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process(b"Hello");
        buf.process(b"\x1b[2J"); // Clear screen
        assert_eq!(buf.get_cell(0, 0).unwrap().character, ' ');
    }

    #[test]
    fn test_process_colors() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process(b"\x1b[31mRed\x1b[0m");
        assert_eq!(buf.get_cell(0, 0).unwrap().fg, Color::Red);
        assert_eq!(buf.get_cell(0, 0).unwrap().character, 'R');
    }

    #[test]
    fn test_process_bold() {
        let mut buf = TerminalBuffer::new(80, 24);
        buf.process(b"\x1b[1mBold\x1b[0m");
        assert!(buf.get_cell(0, 0).unwrap().attrs.bold);
    }
}
