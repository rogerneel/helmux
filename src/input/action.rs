/// Actions that can be triggered by keybindings
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// No action (key was handled but no further action needed)
    None,
    /// Exit the application
    Exit,
    /// Create a new tab
    NewTab,
    /// Close the current tab
    CloseTab,
    /// Switch to next tab
    NextTab,
    /// Switch to previous tab
    PrevTab,
    /// Switch to tab by number (1-based)
    SelectTab(usize),
    /// Toggle sidebar visibility
    ToggleSidebar,
    /// Start rename mode for current tab
    StartRename,
    /// Detach from tmux session
    Detach,
    /// Send literal Ctrl-B to the pane
    SendCtrlB,
    /// Send a key to the active pane (key string for tmux send-keys)
    SendKey(String),
}
