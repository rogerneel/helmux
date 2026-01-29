use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Action;

/// Input mode for the application
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputMode {
    /// Normal mode - keys pass through to tmux
    Normal,
    /// Prefix key was pressed, waiting for command
    Prefix,
    /// Renaming a tab - capturing input
    Rename,
}

/// Input handler with modal state
pub struct InputHandler {
    /// Current input mode
    mode: InputMode,
    /// Buffer for rename input
    rename_buffer: String,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler {
    pub fn new() -> Self {
        Self {
            mode: InputMode::Normal,
            rename_buffer: String::new(),
        }
    }

    /// Get the current input mode
    pub fn mode(&self) -> &InputMode {
        &self.mode
    }

    /// Check if we're in rename mode
    pub fn is_renaming(&self) -> bool {
        self.mode == InputMode::Rename
    }

    /// Get the current rename buffer content
    pub fn rename_buffer(&self) -> &str {
        &self.rename_buffer
    }

    /// Start rename mode with the current tab name
    pub fn start_rename(&mut self, current_name: &str) {
        self.mode = InputMode::Rename;
        self.rename_buffer = current_name.to_string();
    }

    /// Cancel rename mode
    pub fn cancel_rename(&mut self) {
        self.mode = InputMode::Normal;
        self.rename_buffer.clear();
    }

    /// Finish rename mode and return the new name
    pub fn finish_rename(&mut self) -> String {
        self.mode = InputMode::Normal;
        std::mem::take(&mut self.rename_buffer)
    }

    /// Handle a key event and return the corresponding action
    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        // Ctrl-Q always exits
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            return Action::Exit;
        }

        match self.mode {
            InputMode::Normal => self.handle_normal_key(key),
            InputMode::Prefix => self.handle_prefix_key(key),
            InputMode::Rename => self.handle_rename_key(key),
        }
    }

    /// Handle key in normal mode
    fn handle_normal_key(&mut self, key: KeyEvent) -> Action {
        // Check for prefix key (Ctrl-B)
        if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.mode = InputMode::Prefix;
            return Action::None;
        }

        // Pass key through to tmux
        key_to_send_action(key)
    }

    /// Handle key after prefix (Ctrl-B)
    fn handle_prefix_key(&mut self, key: KeyEvent) -> Action {
        // Always return to normal mode after handling prefix command
        self.mode = InputMode::Normal;

        match key.code {
            // Create new tab
            KeyCode::Char('c') => Action::NewTab,

            // Close current tab
            KeyCode::Char('x') => Action::CloseTab,

            // Next tab
            KeyCode::Char('n') => Action::NextTab,

            // Previous tab
            KeyCode::Char('p') => Action::PrevTab,

            // Tab by number (1-9)
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let index = c.to_digit(10).unwrap() as usize;
                Action::SelectTab(index)
            }

            // Toggle sidebar
            KeyCode::Char('b') => Action::ToggleSidebar,

            // Rename tab
            KeyCode::Char(',') => Action::StartRename,

            // Detach
            KeyCode::Char('d') => Action::Detach,

            // Send literal Ctrl-B (Ctrl-B Ctrl-B)
            KeyCode::Char('B') if key.modifiers.contains(KeyModifiers::SHIFT) => Action::SendCtrlB,

            // Unknown prefix command - ignore
            _ => Action::None,
        }
    }

    /// Handle key in rename mode
    fn handle_rename_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            // Cancel rename
            KeyCode::Esc => {
                self.cancel_rename();
                Action::None
            }

            // Confirm rename - we don't have a FinishRename action,
            // the main loop should check rename_buffer and send the command
            KeyCode::Enter => {
                // The caller should call finish_rename() to get the name
                // and send the rename command to tmux
                Action::None
            }

            // Backspace - delete character
            KeyCode::Backspace => {
                self.rename_buffer.pop();
                Action::None
            }

            // Type character
            KeyCode::Char(c) => {
                // Don't allow control characters
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT)
                {
                    self.rename_buffer.push(c);
                }
                Action::None
            }

            _ => Action::None,
        }
    }
}

/// Convert a key event to a SendKey action with the tmux key string
fn key_to_send_action(key: KeyEvent) -> Action {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    let key_str = match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                format!("C-{}", c)
            } else if alt {
                format!("M-{}", c)
            } else {
                // Regular character - use literal mode
                let escaped = match c {
                    '\'' => "'\\''".to_string(),
                    _ => c.to_string(),
                };
                return Action::SendKey(format!("-l '{}'", escaped));
            }
        }
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Backspace => "BSpace".to_string(),
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                "BTab".to_string()
            } else {
                "Tab".to_string()
            }
        }
        KeyCode::Esc => "Escape".to_string(),
        KeyCode::Up => "Up".to_string(),
        KeyCode::Down => "Down".to_string(),
        KeyCode::Left => "Left".to_string(),
        KeyCode::Right => "Right".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Delete => "DC".to_string(),
        KeyCode::Insert => "IC".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => return Action::None,
    };

    Action::SendKey(key_str)
}
