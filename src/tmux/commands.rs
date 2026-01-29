/// Command builders for common tmux operations
pub struct Commands;

impl Commands {
    /// List windows with their IDs, names, and active status
    pub fn list_windows() -> String {
        "list-windows -F '#{window_id}:#{window_name}:#{window_active}:#{pane_id}'".to_string()
    }

    /// Create a new window with optional name
    pub fn new_window(name: Option<&str>) -> String {
        match name {
            Some(n) => format!("new-window -n '{}'", escape_single_quotes(n)),
            None => "new-window".to_string(),
        }
    }

    /// Select (switch to) a window by ID
    pub fn select_window(window_id: &str) -> String {
        format!("select-window -t {}", window_id)
    }

    /// Rename a window
    pub fn rename_window(window_id: &str, name: &str) -> String {
        format!("rename-window -t {} '{}'", window_id, escape_single_quotes(name))
    }

    /// Kill (close) a window
    pub fn kill_window(window_id: &str) -> String {
        format!("kill-window -t {}", window_id)
    }

    /// Send keys to a pane
    pub fn send_keys(pane_id: &str, keys: &str) -> String {
        format!("send-keys -t {} {}", pane_id, escape_keys(keys))
    }

    /// Send literal text to a pane (automatically quoted)
    pub fn send_text(pane_id: &str, text: &str) -> String {
        format!("send-keys -t {} -l '{}'", pane_id, escape_single_quotes(text))
    }

    /// Refresh client size (set viewport dimensions)
    pub fn refresh_client_size(width: u16, height: u16) -> String {
        format!("refresh-client -C {},{}", width, height)
    }

    /// Capture pane content with escape sequences
    pub fn capture_pane(pane_id: &str) -> String {
        format!("capture-pane -t {} -p -e", pane_id)
    }

    /// Get current session info
    pub fn display_message(format: &str) -> String {
        format!("display-message -p '{}'", format)
    }

    /// Detach from session
    pub fn detach() -> String {
        "detach-client".to_string()
    }

    /// List panes in current window
    pub fn list_panes() -> String {
        "list-panes -F '#{pane_id}:#{pane_active}:#{pane_width}:#{pane_height}'".to_string()
    }
}

/// Escape single quotes for tmux shell arguments
fn escape_single_quotes(s: &str) -> String {
    s.replace('\'', "'\\''")
}

/// Escape keys for send-keys command
fn escape_keys(s: &str) -> String {
    // For now, just wrap in quotes. More sophisticated escaping may be needed.
    format!("'{}'", escape_single_quotes(s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_windows() {
        assert!(Commands::list_windows().contains("list-windows"));
    }

    #[test]
    fn test_new_window() {
        assert_eq!(Commands::new_window(None), "new-window");
        assert_eq!(Commands::new_window(Some("test")), "new-window -n 'test'");
    }

    #[test]
    fn test_escape_single_quotes() {
        assert_eq!(escape_single_quotes("it's"), "it'\\''s");
    }

    #[test]
    fn test_select_window() {
        assert_eq!(Commands::select_window("@1"), "select-window -t @1");
    }

    #[test]
    fn test_rename_window() {
        assert_eq!(
            Commands::rename_window("@1", "my-tab"),
            "rename-window -t @1 'my-tab'"
        );
    }
}
