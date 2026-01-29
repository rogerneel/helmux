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
                            // Log raw bytes for detailed debugging
                            log_debug(&format!("Output ({} bytes): {:?}", data.len(),
                                data.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ")));
                            // Also log as string for readability
                            let preview = String::from_utf8_lossy(&data);
                            log_debug(&format!("  text: {:?}", preview));

                            let (row_before, col_before) = buffer.cursor();
                            buffer.process(&data);

                            // Log cursor position after processing
                            let (row, col) = buffer.cursor();
                            log_debug(&format!("  cursor: ({},{}) -> ({},{})",
                                row_before, col_before, row, col));
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
    let cmd = key_to_tmux_command(pane_id, key);
    if let Some(cmd) = cmd {
        log_debug(&format!("Sending key: {:?} -> cmd: {}", key.code, cmd));
        tmux.send_command(&cmd).await?;
    } else {
        log_debug(&format!("Unhandled key: {:?}", key.code));
    }

    Ok(false)
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
                // Escape single quotes in the character
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

