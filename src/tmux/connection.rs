use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use thiserror::Error;
use tracing::{debug, warn};

use super::protocol::{decode_output_bytes, Notification, TmuxEvent};

#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("Failed to spawn tmux: {0}")]
    SpawnFailed(#[from] std::io::Error),
    #[error("tmux stdin not available")]
    NoStdin,
    #[error("tmux stdout not available")]
    NoStdout,
    #[error("Protocol error: {0}")]
    Protocol(#[from] super::protocol::ProtocolError),
    #[error("Connection closed")]
    Closed,
    #[error("tmux exited with error: {0}")]
    TmuxError(String),
}

pub type Result<T> = std::result::Result<T, ConnectionError>;

/// Connection to tmux in control mode
pub struct TmuxConnection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    command_id: u64,
    /// Buffer for collecting command response data
    response_buffer: Vec<String>,
    /// Current command ID we're collecting response for
    collecting_for: Option<u64>,
    /// Persistent line buffer for cancellation-safe reads.
    /// Uses Vec<u8> instead of String because tmux %output data can contain
    /// raw bytes >= 0x80 (non-ASCII) that may not form valid UTF-8 if a
    /// multi-byte sequence is split across events. read_line would fail on
    /// invalid UTF-8, so we use read_until which works on raw bytes.
    line_buffer: Vec<u8>,
}

impl TmuxConnection {
    /// Connect to tmux in control mode, creating or attaching to the given session
    pub async fn connect(session: &str) -> Result<Self> {
        debug!("Connecting to tmux session: {}", session);

        // Use -C for control mode (not -CC which is iTerm2 specific)
        let mut child = Command::new("tmux")
            .args(["-C", "new-session", "-A", "-s", session])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().ok_or(ConnectionError::NoStdin)?;
        let stdout = child.stdout.take().ok_or(ConnectionError::NoStdout)?;

        // Spawn a task to log stderr
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                let mut reader = BufReader::new(stderr);
                let mut line = String::new();
                while let Ok(n) = reader.read_line(&mut line).await {
                    if n == 0 {
                        break;
                    }
                    warn!("tmux stderr: {}", line.trim());
                    line.clear();
                }
            });
        }

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            command_id: 0,
            response_buffer: Vec::new(),
            collecting_for: None,
            line_buffer: Vec::new(),
        })
    }

    /// Send a command to tmux and return a command ID
    /// The response will come back via next_event() as CommandResponse
    pub async fn send_command(&mut self, cmd: &str) -> Result<u64> {
        self.command_id += 1;
        let id = self.command_id;
        debug!("Sending command [{}]: {}", id, cmd);
        self.stdin.write_all(cmd.as_bytes()).await?;
        self.stdin.write_all(b"\n").await?;
        self.stdin.flush().await?;
        Ok(id)
    }

    /// Read the next event from tmux.
    ///
    /// Uses `read_until` with a persistent `Vec<u8>` buffer instead of `read_line`
    /// for two reasons:
    /// 1. Cancellation safety: partial reads from timed-out calls are preserved.
    /// 2. UTF-8 safety: tmux `%output` lines can contain raw bytes >= 0x80 that
    ///    may not form valid UTF-8 (e.g. if a multi-byte sequence is split across
    ///    events). `read_line` would return an error on such lines.
    pub async fn next_event(&mut self) -> Result<TmuxEvent> {
        loop {
            let bytes_read = self.stdout.read_until(b'\n', &mut self.line_buffer).await?;

            if bytes_read == 0 {
                if let Ok(Some(status)) = self.child.try_wait() {
                    debug!("tmux process exited with status: {:?}", status);
                }
                return Err(ConnectionError::Closed);
            }

            // Trim trailing \n and \r
            while self.line_buffer.last() == Some(&b'\n')
                || self.line_buffer.last() == Some(&b'\r')
            {
                self.line_buffer.pop();
            }

            // Fast path: handle %output at the byte level to avoid UTF-8 issues.
            // %output data can contain raw non-UTF-8 bytes from the terminal program.
            if self.line_buffer.starts_with(b"%output ") {
                let rest = &self.line_buffer[b"%output ".len()..];
                if let Some(space_pos) = rest.iter().position(|&b| b == b' ') {
                    let pane_id = String::from_utf8_lossy(&rest[..space_pos]).to_string();
                    let data = decode_output_bytes(&rest[space_pos + 1..]);
                    self.line_buffer.clear();
                    return Ok(TmuxEvent::Output { pane_id, data });
                } else {
                    let pane_id = String::from_utf8_lossy(rest).to_string();
                    self.line_buffer.clear();
                    return Ok(TmuxEvent::Output {
                        pane_id,
                        data: Vec::new(),
                    });
                }
            }

            // All other notification types are ASCII, safe to convert to String
            let line = String::from_utf8_lossy(&self.line_buffer).to_string();
            self.line_buffer.clear();

            let notification = Notification::parse(&line)?;

            match notification {
                Notification::Begin { id } => {
                    self.collecting_for = Some(id);
                    self.response_buffer.clear();
                }
                Notification::End { id } => {
                    if self.collecting_for == Some(id) {
                        let data = self.response_buffer.join("\n");
                        self.collecting_for = None;
                        self.response_buffer.clear();
                        return Ok(TmuxEvent::CommandResponse { id, data });
                    }
                }
                Notification::Error { id } => {
                    let message = self.response_buffer.join("\n");
                    self.collecting_for = None;
                    self.response_buffer.clear();
                    return Ok(TmuxEvent::CommandError { id, message });
                }
                Notification::Data(data) => {
                    if self.collecting_for.is_some() {
                        self.response_buffer.push(data);
                    }
                }
                Notification::Output { pane_id, data } => {
                    return Ok(TmuxEvent::Output { pane_id, data });
                }
                Notification::WindowAdd { window_id } => {
                    return Ok(TmuxEvent::WindowAdd { window_id });
                }
                Notification::WindowClose { window_id } => {
                    return Ok(TmuxEvent::WindowClose { window_id });
                }
                Notification::WindowRenamed { window_id, name } => {
                    return Ok(TmuxEvent::WindowRenamed { window_id, name });
                }
                Notification::SessionChanged { session_id, name } => {
                    return Ok(TmuxEvent::SessionChanged { session_id, name });
                }
                Notification::Exit { reason } => {
                    return Ok(TmuxEvent::Exit { reason });
                }
                Notification::SessionWindowChanged { window_id, .. } => {
                    return Ok(TmuxEvent::WindowChanged { window_id });
                }
                Notification::UnlinkedWindowClose { window_id } => {
                    return Ok(TmuxEvent::WindowClose { window_id });
                }
                Notification::LayoutChange { .. }
                | Notification::PaneModeChanged { .. }
                | Notification::SessionsChanged
                | Notification::ClientSessionChanged { .. }
                | Notification::WindowPaneChanged { .. }
                | Notification::UnlinkedWindowAdd { .. }
                | Notification::ClientDetached { .. } => {
                    // Ignore, continue reading
                }
                Notification::Unknown { notification_type, .. } => {
                    debug!("Unknown tmux notification: {}", notification_type);
                }
            }
        }
    }

    /// Check if the tmux process is still running
    pub fn is_running(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            _ => false,
        }
    }

    /// Gracefully detach from tmux
    pub async fn detach(&mut self) -> Result<()> {
        self.send_command("detach-client").await?;
        Ok(())
    }

    /// Kill the tmux session
    pub async fn kill_session(&mut self) -> Result<()> {
        self.send_command("kill-session").await?;
        Ok(())
    }
}

impl Drop for TmuxConnection {
    fn drop(&mut self) {
        // Try to kill the child process if still running
        let _ = self.child.start_kill();
    }
}
