mod tmux;

use tmux::{Commands, TmuxConnection, TmuxEvent};
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

const DEFAULT_SESSION: &str = "helmux-default";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::DEBUG)
        .with_target(false)
        .init();

    info!("Starting helmux");

    // Connect to tmux
    let mut tmux = TmuxConnection::connect(DEFAULT_SESSION).await?;
    info!("Connected to tmux session: {}", DEFAULT_SESSION);

    // Query initial window list
    let cmd_id = tmux.send_command(&Commands::list_windows()).await?;
    info!("Sent list-windows command (id: {})", cmd_id);

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
                        let text = String::from_utf8_lossy(data);
                        info!("Output from {}: {:?}", pane_id, text);
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
