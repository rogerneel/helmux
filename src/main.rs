mod terminal;
mod tmux;
mod ui;

use std::fs::OpenOptions;
use std::io::{self, stdout, Write as IoWrite};
use std::time::Duration;

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen, Clear, ClearType},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use terminal::TerminalBuffer;
use tmux::{Commands, TmuxConnection, TmuxEvent};
use ui::Viewport;

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
    // Get terminal size for tmux
    let size = term.size()?;
    let width = size.width;
    let height = size.height;
    log_debug(&format!("Terminal size: {}x{}", width, height));

    // Connect to tmux
    let mut tmux = TmuxConnection::connect(DEFAULT_SESSION).await?;
    log_debug("Connected to tmux");

    // Set tmux client size to match our terminal
    tmux.send_command(&Commands::refresh_client_size(width, height))
        .await?;
    log_debug(&format!("Set tmux size to {}x{}", width, height));

    // Terminal buffer for the active pane
    let mut buffer = TerminalBuffer::new(width, height);
    let mut active_pane: Option<String> = None;

    // Initial render
    term.draw(|frame| {
        frame.render_widget(Viewport::new(&buffer), frame.area());
    })?;

    loop {
        // Poll for terminal events with a short timeout
        let has_event = event::poll(Duration::from_millis(10))?;

        if has_event {
            match event::read()? {
                Event::Key(key) => {
                    if handle_key_event(key, &mut tmux, &active_pane).await? {
                        // Exit requested
                        log_debug("Exit requested via Ctrl-Q");
                        break;
                    }
                }
                Event::Resize(w, h) => {
                    log_debug(&format!("Resize to {}x{}", w, h));
                    // Update tmux client size
                    tmux.send_command(&Commands::refresh_client_size(w, h))
                        .await?;
                    // Resize our buffer
                    buffer.resize(w, h);
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
                match event {
                    TmuxEvent::Output { pane_id, data } => {
                        // Track the active pane (first one we see)
                        if active_pane.is_none() {
                            active_pane = Some(pane_id.clone());
                            log_debug(&format!("Active pane set to: {}", pane_id));
                        }

                        // Only process output for the active pane
                        if active_pane.as_ref() == Some(&pane_id) {
                            // Log all output for debugging
                            let preview = String::from_utf8_lossy(&data);
                            log_debug(&format!("Output: {:?}", preview));

                            buffer.process(&data);

                            // Log cursor position after processing
                            let (row, col) = buffer.cursor();
                            log_debug(&format!("  -> cursor at ({}, {})", row, col));
                        }
                    }
                    TmuxEvent::WindowAdd { .. } => {
                        log_debug("Window added");
                    }
                    TmuxEvent::WindowClose { .. } => {
                        log_debug("Window closed");
                    }
                    TmuxEvent::WindowRenamed { .. } => {
                        log_debug("Window renamed");
                    }
                    TmuxEvent::SessionChanged { .. } => {
                        log_debug("Session changed");
                    }
                    TmuxEvent::CommandResponse { .. } => {
                        // Command completed (don't log every one, too noisy)
                    }
                    TmuxEvent::CommandError { id, message } => {
                        log_debug(&format!("Command {} error: {}", id, message));
                    }
                    TmuxEvent::Exit { reason } => {
                        log_debug(&format!("tmux exited: {:?}", reason));
                        break;
                    }
                }
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
        term.draw(|frame| {
            frame.render_widget(Viewport::new(&buffer), frame.area());
        })?;
    }

    Ok(())
}

/// Handle a key event, returning true if we should exit
async fn handle_key_event(
    key: KeyEvent,
    tmux: &mut TmuxConnection,
    active_pane: &Option<String>,
) -> anyhow::Result<bool> {
    // Check for exit key (Ctrl-Q)
    if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(true);
    }

    // Get the pane to send to
    let pane_id = match active_pane {
        Some(id) => id,
        None => {
            log_debug("Key pressed but no active pane yet");
            return Ok(false);
        }
    };

    // Convert key to tmux send-keys format
    let keys = key_to_tmux_keys(key);
    if let Some(ref keys) = keys {
        let cmd = Commands::send_keys(pane_id, keys);
        log_debug(&format!("Sending key: {:?} -> cmd: {}", key.code, cmd));
        tmux.send_command(&cmd).await?;
    } else {
        log_debug(&format!("Unhandled key: {:?}", key.code));
    }

    Ok(false)
}

/// Convert a crossterm KeyEvent to tmux send-keys format
fn key_to_tmux_keys(key: KeyEvent) -> Option<String> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    match key.code {
        KeyCode::Char(c) => {
            if ctrl {
                // Ctrl+letter
                Some(format!("C-{}", c))
            } else if alt {
                // Alt+letter
                Some(format!("M-{}", c))
            } else {
                // Regular character - needs quoting for special chars
                match c {
                    ' ' => Some("Space".to_string()),
                    ';' => Some("\\;".to_string()),
                    '\'' => Some("\"'\"".to_string()),
                    '"' => Some("'\"'".to_string()),
                    '\\' => Some("\\\\".to_string()),
                    _ => Some(c.to_string()),
                }
            }
        }
        KeyCode::Enter => Some("Enter".to_string()),
        KeyCode::Backspace => Some("BSpace".to_string()),
        KeyCode::Tab => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                Some("BTab".to_string())
            } else {
                Some("Tab".to_string())
            }
        }
        KeyCode::Esc => Some("Escape".to_string()),
        KeyCode::Up => Some("Up".to_string()),
        KeyCode::Down => Some("Down".to_string()),
        KeyCode::Left => Some("Left".to_string()),
        KeyCode::Right => Some("Right".to_string()),
        KeyCode::Home => Some("Home".to_string()),
        KeyCode::End => Some("End".to_string()),
        KeyCode::PageUp => Some("PageUp".to_string()),
        KeyCode::PageDown => Some("PageDown".to_string()),
        KeyCode::Delete => Some("DC".to_string()),
        KeyCode::Insert => Some("IC".to_string()),
        KeyCode::F(n) => Some(format!("F{}", n)),
        _ => None,
    }
}
