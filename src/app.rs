use std::collections::HashMap;

use crate::terminal::TerminalBuffer;
use crate::tmux::{Commands, TmuxConnection};
use crate::ui::TabInfo;

/// A single tab in helmux (corresponds to a tmux window)
pub struct Tab {
    /// tmux window ID (e.g., "@1")
    pub window_id: String,
    /// tmux pane ID for this window's main pane (e.g., "%1")
    pub pane_id: String,
    /// Display name
    pub name: String,
    /// Terminal buffer for this tab
    pub buffer: TerminalBuffer,
    /// Whether there's unseen activity
    pub activity: bool,
}

impl Tab {
    pub fn new(window_id: String, pane_id: String, name: String, width: u16, height: u16) -> Self {
        Self {
            window_id,
            pane_id,
            name,
            buffer: TerminalBuffer::new(width, height),
            activity: false,
        }
    }
}

/// Application state
pub struct App {
    /// All tabs, keyed by window ID
    tabs: HashMap<String, Tab>,
    /// Order of tabs (window IDs in display order)
    tab_order: Vec<String>,
    /// Currently active window ID
    active_window_id: Option<String>,
    /// Viewport dimensions
    viewport_width: u16,
    viewport_height: u16,
}

impl App {
    pub fn new(viewport_width: u16, viewport_height: u16) -> Self {
        Self {
            tabs: HashMap::new(),
            tab_order: Vec::new(),
            active_window_id: None,
            viewport_width,
            viewport_height,
        }
    }

    /// Initialize tabs from tmux window list
    pub async fn sync_from_tmux(&mut self, tmux: &mut TmuxConnection) -> anyhow::Result<()> {
        // Query current windows
        tmux.send_command(&Commands::list_windows()).await?;
        Ok(())
    }

    /// Process list-windows response data
    /// This preserves existing tab buffers when updating
    pub fn process_window_list(&mut self, data: &str) {
        // Format: @window_id:name:active:pane_id per line
        let mut new_order = Vec::new();
        let mut seen_windows = std::collections::HashSet::new();
        let mut new_active = None;

        for line in data.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 4 {
                let window_id = parts[0].to_string();
                let name = parts[1].to_string();
                let is_active = parts[2] == "1";
                let pane_id = parts[3].to_string();

                seen_windows.insert(window_id.clone());
                new_order.push(window_id.clone());

                if is_active {
                    new_active = Some(window_id.clone());
                }

                // Update existing tab or create new one
                if let Some(tab) = self.tabs.get_mut(&window_id) {
                    // Preserve buffer, update metadata
                    tab.name = name;
                    tab.pane_id = pane_id;
                } else {
                    // Create new tab
                    let tab = Tab::new(
                        window_id.clone(),
                        pane_id,
                        name,
                        self.viewport_width,
                        self.viewport_height,
                    );
                    self.tabs.insert(window_id, tab);
                }
            }
        }

        // Remove tabs that are no longer in the list
        self.tabs.retain(|id, _| seen_windows.contains(id));

        // Update order and active window
        self.tab_order = new_order;
        self.active_window_id = new_active;
    }

    /// Add a new tab from tmux window-add event
    pub fn add_tab(&mut self, window_id: &str, pane_id: &str, name: &str) {
        if !self.tabs.contains_key(window_id) {
            let tab = Tab::new(
                window_id.to_string(),
                pane_id.to_string(),
                name.to_string(),
                self.viewport_width,
                self.viewport_height,
            );
            self.tab_order.push(window_id.to_string());
            self.tabs.insert(window_id.to_string(), tab);
        }
    }

    /// Remove a tab
    pub fn remove_tab(&mut self, window_id: &str) {
        self.tabs.remove(window_id);
        self.tab_order.retain(|id| id != window_id);

        // If we removed the active tab, select another
        if self.active_window_id.as_deref() == Some(window_id) {
            self.active_window_id = self.tab_order.first().cloned();
        }
    }

    /// Rename a tab
    pub fn rename_tab(&mut self, window_id: &str, name: &str) {
        if let Some(tab) = self.tabs.get_mut(window_id) {
            tab.name = name.to_string();
        }
    }

    /// Set the active tab by window ID
    pub fn set_active(&mut self, window_id: &str) {
        if self.tabs.contains_key(window_id) {
            // Clear activity on the newly active tab
            if let Some(tab) = self.tabs.get_mut(window_id) {
                tab.activity = false;
            }
            self.active_window_id = Some(window_id.to_string());
        }
    }

    /// Get the active tab
    pub fn active_tab(&self) -> Option<&Tab> {
        self.active_window_id
            .as_ref()
            .and_then(|id| self.tabs.get(id))
    }

    /// Get the active tab mutably
    pub fn active_tab_mut(&mut self) -> Option<&mut Tab> {
        self.active_window_id
            .as_ref()
            .and_then(|id| self.tabs.get_mut(id))
    }

    /// Get the active pane ID
    pub fn active_pane_id(&self) -> Option<&str> {
        self.active_tab().map(|t| t.pane_id.as_str())
    }

    /// Get the active window ID
    pub fn active_window_id(&self) -> Option<&str> {
        self.active_window_id.as_deref()
    }

    /// Find tab by pane ID and get mutable reference
    pub fn tab_by_pane_mut(&mut self, pane_id: &str) -> Option<&mut Tab> {
        self.tabs.values_mut().find(|t| t.pane_id == pane_id)
    }

    /// Find window ID by pane ID
    pub fn window_id_for_pane(&self, pane_id: &str) -> Option<&str> {
        self.tabs
            .iter()
            .find(|(_, t)| t.pane_id == pane_id)
            .map(|(id, _)| id.as_str())
    }

    /// Process output for a pane
    pub fn process_output(&mut self, pane_id: &str, data: &[u8]) {
        // Check if this is the active pane
        let is_active = self.active_pane_id() == Some(pane_id);

        if let Some(tab) = self.tab_by_pane_mut(pane_id) {
            tab.buffer.process(data);
            // Mark activity if not active tab
            if !is_active {
                tab.activity = true;
            }
        }
    }

    /// Get tab info for the sidebar
    pub fn tab_infos(&self) -> Vec<TabInfo> {
        self.tab_order
            .iter()
            .enumerate()
            .filter_map(|(idx, window_id)| {
                self.tabs.get(window_id).map(|tab| TabInfo {
                    id: window_id.clone(),
                    name: tab.name.clone(),
                    active: self.active_window_id.as_ref() == Some(window_id),
                    activity: tab.activity,
                    index: idx + 1,
                })
            })
            .collect()
    }

    /// Get the number of tabs
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Get next tab's window ID (for Ctrl-b n)
    pub fn next_window_id(&self) -> Option<&str> {
        let current_idx = self
            .active_window_id
            .as_ref()
            .and_then(|id| self.tab_order.iter().position(|x| x == id))?;
        let next_idx = (current_idx + 1) % self.tab_order.len();
        self.tab_order.get(next_idx).map(|s| s.as_str())
    }

    /// Get previous tab's window ID (for Ctrl-b p)
    pub fn prev_window_id(&self) -> Option<&str> {
        let current_idx = self
            .active_window_id
            .as_ref()
            .and_then(|id| self.tab_order.iter().position(|x| x == id))?;
        let prev_idx = if current_idx == 0 {
            self.tab_order.len().saturating_sub(1)
        } else {
            current_idx - 1
        };
        self.tab_order.get(prev_idx).map(|s| s.as_str())
    }

    /// Get window ID by index (1-based, for Ctrl-b 1-9)
    pub fn window_id_by_index(&self, index: usize) -> Option<&str> {
        if index == 0 || index > self.tab_order.len() {
            return None;
        }
        self.tab_order.get(index - 1).map(|s| s.as_str())
    }

    /// Resize all tab buffers
    pub fn resize(&mut self, width: u16, height: u16) {
        self.viewport_width = width;
        self.viewport_height = height;
        for tab in self.tabs.values_mut() {
            tab.buffer.resize(width, height);
        }
    }

    /// Check if we have any tabs
    pub fn has_tabs(&self) -> bool {
        !self.tabs.is_empty()
    }
}
