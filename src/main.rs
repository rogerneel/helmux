mod app;
mod input;
mod terminal;
mod tmux;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, stdout, Write as IoWrite};
use std::time::{Duration, Instant};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEventKind},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;
use input::{Action, InputHandler, InputMode};
use tmux::{Commands, TmuxConnection, TmuxEvent};
use ui::{is_new_tab_button, row_to_tab_index, HitRegion, Layout, RenameOverlay, Sidebar, SidebarMode, Viewport};

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
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Clear(ClearType::All))?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.clear()?;

    // Run the app and capture result
    let result = run_app(&mut term).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(term.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
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

    // Create app state and input handler
    let mut app = App::new(vp_width, vp_height);
    let mut input = InputHandler::new();

    // Double-click tracking for tab rename
    let mut last_tab_click: Option<(usize, Instant)> = None;
    const DOUBLE_CLICK_MS: u128 = 400;

    // Query initial window list
    app.sync_from_tmux(&mut tmux).await?;

    // Initial render (empty until we get window list)
    render(term, &layout, &app, &input)?;

    loop {
        // Poll for terminal events with a short timeout
        let has_event = event::poll(Duration::from_millis(10))?;

        if has_event {
            match event::read()? {
                Event::Key(key) => {
                    // Special handling for Enter in rename mode
                    if input.is_renaming() && key.code == KeyCode::Enter {
                        let new_name = input.finish_rename();
                        if let Some(window_id) = app.active_window_id() {
                            if new_name.trim().is_empty() {
                                // Empty name - enable automatic rename (shows running process)
                                tmux.send_command(&Commands::enable_automatic_rename(window_id))
                                    .await?;
                            } else {
                                tmux.send_command(&Commands::rename_window(window_id, &new_name))
                                    .await?;
                            }
                        }
                        render(term, &layout, &app, &input)?;
                        continue;
                    }

                    // Handle key through input handler
                    let action = input.handle_key(key);

                    match handle_action(action, &mut app, &mut tmux, &mut input, &mut layout)
                        .await?
                    {
                        LoopAction::Continue => {}
                        LoopAction::Exit => break,
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
                Event::Mouse(mouse) => {
                    // In rename mode, clicking anywhere cancels the rename
                    if input.is_renaming() {
                        if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                            input.cancel_rename();
                        }
                        continue;
                    }

                    let click_result = handle_mouse_event(
                        mouse,
                        &mut app,
                        &mut tmux,
                        &layout,
                        &input,
                        &mut last_tab_click,
                        DOUBLE_CLICK_MS,
                    ).await?;

                    // If double-click detected, start rename
                    if click_result.start_rename {
                        if let Some(tab) = app.active_tab() {
                            input.start_rename(&tab.name);
                        }
                    }
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
        render(term, &layout, &app, &input)?;
    }

    Ok(())
}

/// Render the UI
fn render(
    term: &mut Terminal<CrosstermBackend<io::Stdout>>,
    layout: &Layout,
    app: &App,
    input: &InputHandler,
) -> anyhow::Result<()> {
    let tabs = app.tab_infos();

    term.draw(|frame| {
        let sidebar_area = layout.sidebar_area();
        let viewport_area = layout.viewport_area();

        // Convert input mode to sidebar mode
        let sidebar_mode = match input.mode() {
            InputMode::Normal => SidebarMode::Normal,
            InputMode::Prefix => SidebarMode::Prefix,
            InputMode::Rename => SidebarMode::Rename,
        };

        frame.render_widget(Sidebar::new(&tabs).mode(sidebar_mode), sidebar_area);

        // Render the active tab's buffer
        if let Some(tab) = app.active_tab() {
            frame.render_widget(Viewport::new(&tab.buffer), viewport_area);
        }

        // Render rename overlay if in rename mode
        if input.is_renaming() {
            let overlay_area = RenameOverlay::centered_rect(frame.area());
            frame.render_widget(RenameOverlay::new(input.rename_buffer()), overlay_area);
        }
    })?;

    Ok(())
}

/// Result of handling an action
enum LoopAction {
    Continue,
    Exit,
}

/// Handle an action from the input handler
async fn handle_action(
    action: Action,
    app: &mut App,
    tmux: &mut TmuxConnection,
    input: &mut InputHandler,
    _layout: &mut Layout,
) -> anyhow::Result<LoopAction> {
    match action {
        Action::None => {}

        Action::Exit => {
            return Ok(LoopAction::Exit);
        }

        Action::NewTab => {
            tmux.send_command(&Commands::new_window(None)).await?;
        }

        Action::CloseTab => {
            if let Some(window_id) = app.active_window_id() {
                tmux.send_command(&Commands::kill_window(window_id)).await?;
            }
        }

        Action::NextTab => {
            if let Some(window_id) = app.next_window_id() {
                tmux.send_command(&Commands::select_window(window_id))
                    .await?;
            }
        }

        Action::PrevTab => {
            if let Some(window_id) = app.prev_window_id() {
                tmux.send_command(&Commands::select_window(window_id))
                    .await?;
            }
        }

        Action::SelectTab(index) => {
            if let Some(window_id) = app.window_id_by_index(index) {
                tmux.send_command(&Commands::select_window(window_id))
                    .await?;
            }
        }

        Action::ToggleSidebar => {
            // Will be implemented in Phase 9
            // layout.toggle_sidebar();
        }

        Action::StartRename => {
            // Get current tab name and start rename mode
            if let Some(tab) = app.active_tab() {
                input.start_rename(&tab.name);
            }
        }

        Action::Detach => {
            tmux.send_command(&Commands::detach()).await?;
            return Ok(LoopAction::Exit);
        }

        Action::SendCtrlB => {
            if let Some(pane_id) = app.active_pane_id() {
                tmux.send_command(&format!("send-keys -t {} C-b", pane_id))
                    .await?;
            }
        }

        Action::SendKey(key_str) => {
            if let Some(pane_id) = app.active_pane_id() {
                tmux.send_command(&format!("send-keys -t {} {}", pane_id, key_str))
                    .await?;
            }
        }
    }

    Ok(LoopAction::Continue)
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

/// Result of handling a mouse event
struct MouseResult {
    /// Whether to start rename mode (double-click on tab)
    start_rename: bool,
}

/// Handle a mouse event
async fn handle_mouse_event(
    mouse: crossterm::event::MouseEvent,
    app: &mut App,
    tmux: &mut TmuxConnection,
    layout: &Layout,
    input: &InputHandler,
    last_tab_click: &mut Option<(usize, Instant)>,
    double_click_ms: u128,
) -> anyhow::Result<MouseResult> {
    let x = mouse.column;
    let y = mouse.row;
    let mut result = MouseResult { start_rename: false };

    match layout.hit_test(x, y) {
        HitRegion::Sidebar { row } => {
            // Only handle clicks in sidebar
            if let MouseEventKind::Down(MouseButton::Left) = mouse.kind {
                let sidebar_area = layout.sidebar_area();
                let num_tabs = app.tab_count();

                // Calculate header rows (1 if in prefix mode, 0 otherwise)
                let header_rows = if matches!(input.mode(), InputMode::Prefix) { 1 } else { 0 };

                if is_new_tab_button(row, sidebar_area.height) {
                    // Click on [+] button - create new tab
                    tmux.send_command(&Commands::new_window(None)).await?;
                    *last_tab_click = None;
                } else if let Some(tab_index) = row_to_tab_index(row, num_tabs, sidebar_area.height, header_rows) {
                    // Check for double-click
                    let now = Instant::now();
                    if let Some((last_index, last_time)) = last_tab_click {
                        if *last_index == tab_index && now.duration_since(*last_time).as_millis() < double_click_ms {
                            // Double-click on same tab - trigger rename
                            result.start_rename = true;
                            *last_tab_click = None;
                        } else {
                            // Different tab or too slow - single click
                            *last_tab_click = Some((tab_index, now));
                            if let Some(window_id) = app.window_id_by_index(tab_index + 1) {
                                tmux.send_command(&Commands::select_window(window_id)).await?;
                            }
                        }
                    } else {
                        // First click
                        *last_tab_click = Some((tab_index, now));
                        if let Some(window_id) = app.window_id_by_index(tab_index + 1) {
                            tmux.send_command(&Commands::select_window(window_id)).await?;
                        }
                    }
                } else {
                    *last_tab_click = None;
                }
            }
        }
        HitRegion::Viewport { row, col } => {
            // Forward mouse events to tmux pane
            *last_tab_click = None;
            if let Some(pane_id) = app.active_pane_id() {
                let mouse_cmd = mouse_event_to_tmux(pane_id, mouse.kind, col, row);
                if let Some(cmd) = mouse_cmd {
                    tmux.send_command(&cmd).await?;
                }
            }
        }
        HitRegion::None => {
            // Click outside any region - reset double-click tracking
            *last_tab_click = None;
        }
    }

    Ok(result)
}

/// Convert a mouse event to a tmux send-keys command
/// Uses SGR (1006) mouse encoding format
fn mouse_event_to_tmux(pane_id: &str, kind: MouseEventKind, col: u16, row: u16) -> Option<String> {
    // tmux expects 1-based coordinates for mouse events
    let x = col + 1;
    let y = row + 1;

    // Build the mouse escape sequence (SGR 1006 format)
    // Format: \e[<Cb;Cx;CyM (press) or \e[<Cb;Cx;Cym (release)
    let (button_code, press) = match kind {
        MouseEventKind::Down(MouseButton::Left) => (0, true),
        MouseEventKind::Down(MouseButton::Middle) => (1, true),
        MouseEventKind::Down(MouseButton::Right) => (2, true),
        MouseEventKind::Up(MouseButton::Left) => (0, false),
        MouseEventKind::Up(MouseButton::Middle) => (1, false),
        MouseEventKind::Up(MouseButton::Right) => (2, false),
        MouseEventKind::Drag(MouseButton::Left) => (32, true),   // 32 = motion with button
        MouseEventKind::Drag(MouseButton::Middle) => (33, true),
        MouseEventKind::Drag(MouseButton::Right) => (34, true),
        MouseEventKind::ScrollUp => (64, true),
        MouseEventKind::ScrollDown => (65, true),
        MouseEventKind::ScrollLeft => (66, true),
        MouseEventKind::ScrollRight => (67, true),
        MouseEventKind::Moved => return None, // Don't send motion without button
    };

    let suffix = if press { 'M' } else { 'm' };

    // Send the escape sequence using send-keys -l (literal mode)
    // We need to escape the escape character for tmux
    Some(format!(
        "send-keys -t {} -l $'\\e[<{};{};{}{}'",
        pane_id, button_code, x, y, suffix
    ))
}
