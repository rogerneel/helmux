mod app;
mod input;
mod terminal;
mod tmux;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, stdout, Write as IoWrite};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode},
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
use ui::{Layout, RenameOverlay, Sidebar, SidebarMode, Viewport};

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

    // Create app state and input handler
    let mut app = App::new(vp_width, vp_height);
    let mut input = InputHandler::new();

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
                            tmux.send_command(&Commands::rename_window(window_id, &new_name))
                                .await?;
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
