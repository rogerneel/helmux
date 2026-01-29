mod terminal;
mod tmux;

use std::collections::HashMap;
use terminal::TerminalBuffer;
use tmux::{Commands, TmuxConnection, TmuxEvent};
use tracing::{debug, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

const DEFAULT_SESSION: &str = "helmux-default";
const DEFAULT_WIDTH: u16 = 80;
const DEFAULT_HEIGHT: u16 = 24;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let _subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();

    info!("Starting helmux");

    // Connect to tmux
    let mut tmux = TmuxConnection::connect(DEFAULT_SESSION).await?;
    info!("Connected to tmux session: {}", DEFAULT_SESSION);

    // Terminal buffers for each pane
    let mut buffers: HashMap<String, TerminalBuffer> = HashMap::new();
    let mut initialized = false;

    // Main event loop - read and display events
    loop {
        match tmux.next_event().await {
            Ok(event) => {
                match &event {
                    TmuxEvent::CommandResponse { id, data } => {
                        info!("Command {} response:", id);
                        for line in data.lines() {
                            info!("  {}", line);
                        }
                    }
                    TmuxEvent::CommandError { id, message } => {
                        warn!("Command {} error: {}", id, message);
                    }
                    TmuxEvent::Output { pane_id, data } => {
                        // Get or create buffer for this pane
                        let buffer = buffers.entry(pane_id.clone()).or_insert_with(|| {
                            info!("Created buffer for pane {}", pane_id);
                            TerminalBuffer::new(DEFAULT_WIDTH, DEFAULT_HEIGHT)
                        });

                        // Process the output through VTE parser
                        buffer.process(data);

                        // Log a preview of the buffer state
                        let (cursor_row, cursor_col) = buffer.cursor();
                        debug!(
                            "Pane {} buffer updated, cursor at ({}, {})",
                            pane_id, cursor_row, cursor_col
                        );

                        // Show first line of buffer as preview
                        let first_line: String = buffer
                            .cells()
                            .first()
                            .map(|row| row.iter().map(|c| c.character).collect())
                            .unwrap_or_default();
                        let first_line = first_line.trim_end();
                        if !first_line.is_empty() {
                            debug!("  Line 0: {:?}", first_line);
                        }

                        // After first output, send initial commands
                        if !initialized {
                            initialized = true;
                            // Set the tmux client size
                            tmux.send_command(&Commands::refresh_client_size(
                                DEFAULT_WIDTH,
                                DEFAULT_HEIGHT,
                            ))
                            .await?;

                            // Query initial window list
                            let cmd_id = tmux.send_command(&Commands::list_windows()).await?;
                            info!("Sent list-windows command (id: {})", cmd_id);
                        }
                    }
                    TmuxEvent::WindowAdd { window_id } => {
                        info!("Window added: {}", window_id);
                    }
                    TmuxEvent::WindowClose { window_id } => {
                        info!("Window closed: {}", window_id);
                    }
                    TmuxEvent::WindowRenamed { window_id, name } => {
                        info!("Window {} renamed to: {}", window_id, name);
                    }
                    TmuxEvent::SessionChanged { session_id, name } => {
                        info!("Session changed: {} ({})", name, session_id);

                        // After session changed, send initial commands
                        if !initialized {
                            initialized = true;
                            // Set the tmux client size
                            tmux.send_command(&Commands::refresh_client_size(
                                DEFAULT_WIDTH,
                                DEFAULT_HEIGHT,
                            ))
                            .await?;

                            // Query initial window list
                            let cmd_id = tmux.send_command(&Commands::list_windows()).await?;
                            info!("Sent list-windows command (id: {})", cmd_id);
                        }
                    }
                    TmuxEvent::Exit { reason } => {
                        info!("tmux exited: {:?}", reason);
                        break;
                    }
                }
            }
            Err(e) => {
                warn!("Error reading from tmux: {}", e);
                break;
            }
        }
    }

    info!("helmux shutting down");
    Ok(())
}
