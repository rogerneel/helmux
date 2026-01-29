mod app;
mod terminal;
mod tmux;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, stdout, Write as IoWrite};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use tmux::{Commands, TmuxConnection, TmuxEvent};
use ui::{Layout, Sidebar, Viewport};

const DEFAULT_SESSION: &str = "helmux-default";
const DEBUG_LOG: &str = "/tmp/helmux-debug.log";

fn log_debug(msg: &str) {
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(DEBUG_LOG) {
        let _ = writeln!(file, "{}", msg);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Clear debug log
    let _ = std::fs::write(DEBUG_LOG, "");
    log_debug("=== helmux starting ===");

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Clear(ClearType::All))?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.clear()?;

    // Run the app and capture result
    let result = run_app(&mut term).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    term.show_cursor()?;

    log_debug("=== helmux exiting ===");

    // Return any error from the app
    if let Err(ref e) = result {
        log_debug(&format!("Error: {}", e));
    }
    result
}

async fn run_app(term: &mut Terminal<CrosstermBackend<io::Stdout>>) -> anyhow::Result<()> {
    // Get terminal size and create layout
    let size = term.size()?;
    let area = ratatui::layout::Rect::new(0, 0, size.width, size.height);
    let mut layout = Layout::new(area);
    let (vp_width, vp_height) = layout.tmux_size();

    // Connect to tmux
    let mut tmux = TmuxConnection::connect(DEFAULT_SESSION).await?;

    // Set tmux client size to match viewport (not full terminal)
    tmux.send_command(&Commands::refresh_client_size(vp_width, vp_height))
        .await?;

    // Create app state
    let mut app = App::new(vp_width, vp_height);

    // Query initial window list
    app.sync_from_tmux(&mut tmux).await?;

    // Initial render (empty until we get window list)
    render(term, &layout, &app)?;

    loop {
        // Poll for terminal events with a short timeout
        let has_event = event::poll(Duration::from_millis(10))?;

        if has_event {
            match event::read()? {
                Event::Key(key) => {
                    match handle_key_event(key, &mut app, &mut tmux).await? {
                        KeyAction::Continue => {}
                        KeyAction::Exit => break,
                    }
                }
                Event::Resize(w, h) => {
                    // Update layout with new size
                    layout.set_area(ratatui::layout::Rect::new(0, 0, w, h));
                    let (vp_width, vp_height) = layout.tmux_size();
                    // Update tmux client size to match viewport
                    tmux.send_command(&Commands::refresh_client_size(vp_width, vp_height))
                        .await?;
                    // Resize all tab buffers
                    app.resize(vp_width, vp_height);
                }
                Event::Mouse(_) => {
                    // Mouse handling will be added in Phase 7
                }
                _ => {}
            }
        }

        // Check for tmux events (non-blocking)
        match tokio::time::timeout(Duration::from_millis(1), tmux.next_event()).await {
            Ok(Ok(event)) => {
                handle_tmux_event(event, &mut app, &mut tmux).await?;
            }
            Ok(Err(e)) => {
                log_debug(&format!("Connection error: {}", e));
                break;
            }
            Err(_) => {
                // Timeout - no tmux event, continue
            }
        }

        // Render
        render(term, &layout, &app)?;
    }

    Ok(())
}

/// Render the UI
fn render(
    term: &mut Terminal<CrosstermBackend<io::Stdout>>,
    layout: &Layout,
    app: &App,
) -> anyhow::Result<()> {
    let tabs = app.tab_infos();

    term.draw(|frame| {
        let sidebar_area = layout.sidebar_area();
        let viewport_area = layout.viewport_area();

        frame.render_widget(Sidebar::new(&tabs), sidebar_area);

        // Render the active tab's buffer
        if let Some(tab) = app.active_tab() {
            frame.render_widget(Viewport::new(&tab.buffer), viewport_area);
        }
    })?;

    Ok(())
}

/// Result of handling a key event
enum KeyAction {
    Continue,
    Exit,
}

/// Handle a key event
async fn handle_key_event(
    key: KeyEvent,
    app: &mut App,
    tmux: &mut TmuxConnection,
) -> anyhow::Result<KeyAction> {
    // Check for exit key (Ctrl-Q) - always works
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(KeyAction::Exit);
    }

    // Check for prefix key (Ctrl-B)
    if key.code == KeyCode::Char('b') && key.modifiers.contains(KeyModifiers::CONTROL) {
        app.prefix_pending = true;
        return Ok(KeyAction::Continue);
    }

    // Handle prefix commands
    if app.prefix_pending {
        app.prefix_pending = false;
        return handle_prefix_command(key, app, tmux).await;
    }

    // Normal key - send to active pane
    if let Some(pane_id) = app.active_pane_id() {
        if let Some(cmd) = key_to_tmux_command(pane_id, key) {
            tmux.send_command(&cmd).await?;
        }
    }

    Ok(KeyAction::Continue)
}

/// Handle a command after the prefix key (Ctrl-B)
async fn handle_prefix_command(
    key: KeyEvent,
    app: &mut App,
    tmux: &mut TmuxConnection,
) -> anyhow::Result<KeyAction> {
    match key.code {
        // Create new tab
        KeyCode::Char('c') => {
            tmux.send_command(&Commands::new_window(None)).await?;
        }

        // Close current tab
        KeyCode::Char('x') => {
            if let Some(window_id) = app.active_window_id() {
                tmux.send_command(&Commands::kill_window(window_id)).await?;
            }
        }

        // Next tab
        KeyCode::Char('n') => {
            if let Some(window_id) = app.next_window_id() {
                tmux.send_command(&Commands::select_window(window_id))
                    .await?;
            }
        }

        // Previous tab
        KeyCode::Char('p') => {
            if let Some(window_id) = app.prev_window_id() {
                tmux.send_command(&Commands::select_window(window_id))
                    .await?;
            }
        }

        // Tab by number (1-9)
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            let index = c.to_digit(10).unwrap() as usize;
            if let Some(window_id) = app.window_id_by_index(index) {
                tmux.send_command(&Commands::select_window(window_id))
                    .await?;
            }
        }

        // Toggle sidebar (will be implemented in Phase 9)
        KeyCode::Char('b') => {
            // TODO: layout.toggle_sidebar()
        }

        // Detach
        KeyCode::Char('d') => {
            tmux.send_command(&Commands::detach()).await?;
            return Ok(KeyAction::Exit);
        }

        // Send literal Ctrl-B to the pane (Ctrl-B Ctrl-B)
        KeyCode::Char('B') if key.modifiers.contains(KeyModifiers::SHIFT) => {
            if let Some(pane_id) = app.active_pane_id() {
                tmux.send_command(&format!("send-keys -t {} C-b", pane_id))
                    .await?;
            }
        }

        _ => {
            // Unknown prefix command - ignore
        }
    }

    Ok(KeyAction::Continue)
}

/// Handle a tmux event
async fn handle_tmux_event(
    event: TmuxEvent,
    app: &mut App,
    tmux: &mut TmuxConnection,
) -> anyhow::Result<()> {
    match event {
        TmuxEvent::Output { pane_id, data } => {
            // If we don't have tabs yet, this output might tell us about the initial pane
            if !app.has_tabs() {
                // We'll get proper tab info from the list-windows response
                return Ok(());
            }

            app.process_output(&pane_id, &data);
        }

        TmuxEvent::WindowAdd { window_id } => {
            log_debug(&format!("Window added: {}", window_id));
            // Query updated window list to get full info
            tmux.send_command(&Commands::list_windows()).await?;
        }

        TmuxEvent::WindowClose { window_id } => {
            log_debug(&format!("Window closed: {}", window_id));
            app.remove_tab(&window_id);
            // Re-sync to ensure consistency
            tmux.send_command(&Commands::list_windows()).await?;
        }

        TmuxEvent::WindowRenamed { window_id, name } => {
            log_debug(&format!("Window renamed: {} -> {}", window_id, name));
            app.rename_tab(&window_id, &name);
        }

        TmuxEvent::SessionChanged { .. } => {
            // Session changed - refresh window list
            tmux.send_command(&Commands::list_windows()).await?;
        }

        TmuxEvent::WindowChanged { window_id } => {
            log_debug(&format!("Window changed to: {}", window_id));
            app.set_active(&window_id);
        }

        TmuxEvent::CommandResponse { data, .. } => {
            // Check if this looks like a window list response
            if data.contains(':') && (data.contains('@') || data.contains('%')) {
                app.process_window_list(&data);
                log_debug(&format!("Loaded {} tabs", app.tab_count()));
            }
        }

        TmuxEvent::CommandError { id, message } => {
            log_debug(&format!("Command {} error: {}", id, message));
        }

        TmuxEvent::Exit { reason } => {
            log_debug(&format!("tmux exited: {:?}", reason));
        }
    }

    Ok(())
}

/// Build the tmux send-keys command for a key event
fn key_to_tmux_command(pane_id: &str, key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                // Ctrl+letter - use key name
                Some(format!("send-keys -t {} C-{}", pane_id, c))
            } else if alt {
                // Alt+letter - use key name
                Some(format!("send-keys -t {} M-{}", pane_id, c))
            } else {
                // Regular character - use literal mode for reliability
                let escaped = match c {
                    '\'' => "'\\''".to_string(),
                    _ => c.to_string(),
                };
                Some(format!("send-keys -t {} -l '{}'", pane_id, escaped))
            }
        }
        // Special keys use key names (not literal mode)
        KeyCode::Enter => Some(format!("send-keys -t {} Enter", pane_id)),
        KeyCode::Backspace => Some(format!("send-keys -t {} BSpace", pane_id)),
        KeyCode::Tab => {
            let key_name = if key.modifiers.contains(KeyModifiers::SHIFT) {
                "BTab"
            } else {
                "Tab"
            };
            Some(format!("send-keys -t {} {}", pane_id, key_name))
        }
        KeyCode::Esc => Some(format!("send-keys -t {} Escape", pane_id)),
        KeyCode::Up => Some(format!("send-keys -t {} Up", pane_id)),
        KeyCode::Down => Some(format!("send-keys -t {} Down", pane_id)),
        KeyCode::Left => Some(format!("send-keys -t {} Left", pane_id)),
        KeyCode::Right => Some(format!("send-keys -t {} Right", pane_id)),
        KeyCode::Home => Some(format!("send-keys -t {} Home", pane_id)),
        KeyCode::End => Some(format!("send-keys -t {} End", pane_id)),
        KeyCode::PageUp => Some(format!("send-keys -t {} PageUp", pane_id)),
        KeyCode::PageDown => Some(format!("send-keys -t {} PageDown", pane_id)),
        KeyCode::Delete => Some(format!("send-keys -t {} DC", pane_id)),
        KeyCode::Insert => Some(format!("send-keys -t {} IC", pane_id)),
        KeyCode::F(n) => Some(format!("send-keys -t {} F{}", pane_id, n)),
        _ => None,
    }
}
