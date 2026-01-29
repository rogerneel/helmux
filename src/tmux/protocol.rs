use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("Invalid notification format: {0}")]
    InvalidFormat(String),
    #[error("Unknown notification type: {0}")]
    UnknownType(String),
}

/// A parsed tmux control mode notification
#[derive(Debug, Clone)]
pub enum Notification {
    /// %begin <time> <num> <flags>
    Begin { id: u64 },
    /// %end <time> <num> <flags>
    End { id: u64 },
    /// %error <time> <num> <flags>
    Error { id: u64 },
    /// %output <pane-id> <data>
    Output { pane_id: String, data: Vec<u8> },
    /// %window-add <window-id>
    WindowAdd { window_id: String },
    /// %window-close <window-id>
    WindowClose { window_id: String },
    /// %window-renamed <window-id> <name>
    WindowRenamed { window_id: String, name: String },
    /// %session-changed <session-id> <name>
    SessionChanged { session_id: String, name: String },
    /// %sessions-changed - session list changed
    SessionsChanged,
    /// %client-session-changed <client> <session-id> <name>
    ClientSessionChanged { client: String, session_id: String, name: String },
    /// %layout-change <window-id> <layout>
    LayoutChange { window_id: String, layout: String },
    /// %pane-mode-changed <pane-id>
    PaneModeChanged { pane_id: String },
    /// %window-pane-changed <window-id> <pane-id>
    WindowPaneChanged { window_id: String, pane_id: String },
    /// %unlinked-window-add <window-id>
    UnlinkedWindowAdd { window_id: String },
    /// %client-detached <client> [reason]
    ClientDetached { client: String, reason: Option<String> },
    /// %exit or %exit [reason]
    Exit { reason: Option<String> },
    /// Data line (part of command response between %begin and %end)
    Data(String),
    /// Unknown notification type (logged but ignored)
    Unknown { notification_type: String, raw: String },
}

/// Higher-level event derived from notifications
#[derive(Debug, Clone)]
pub enum TmuxEvent {
    /// Output from a pane
    Output { pane_id: String, data: Vec<u8> },
    /// A window was added
    WindowAdd { window_id: String },
    /// A window was closed
    WindowClose { window_id: String },
    /// A window was renamed
    WindowRenamed { window_id: String, name: String },
    /// Command response completed
    CommandResponse { id: u64, data: String },
    /// Command error
    CommandError { id: u64, message: String },
    /// Session changed
    SessionChanged { session_id: String, name: String },
    /// tmux server exited
    Exit { reason: Option<String> },
}

impl Notification {
    /// Parse a line from tmux control mode output
    pub fn parse(line: &str) -> Result<Self, ProtocolError> {
        if !line.starts_with('%') {
            // Data line (part of command response)
            return Ok(Notification::Data(line.to_string()));
        }

        let parts: Vec<&str> = line.splitn(4, ' ').collect();
        let notification_type = parts.first().ok_or_else(|| {
            ProtocolError::InvalidFormat("empty notification".to_string())
        })?;

        match *notification_type {
            "%begin" => {
                let id = parts.get(2)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                Ok(Notification::Begin { id })
            }
            "%end" => {
                let id = parts.get(2)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                Ok(Notification::End { id })
            }
            "%error" => {
                let id = parts.get(2)
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                Ok(Notification::Error { id })
            }
            "%output" => {
                let pane_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing pane_id".to_string()))?
                    .to_string();
                let data = parts.get(2)
                    .map(|s| decode_output(s))
                    .unwrap_or_default();
                Ok(Notification::Output { pane_id, data })
            }
            "%window-add" => {
                let window_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing window_id".to_string()))?
                    .to_string();
                Ok(Notification::WindowAdd { window_id })
            }
            "%window-close" => {
                let window_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing window_id".to_string()))?
                    .to_string();
                Ok(Notification::WindowClose { window_id })
            }
            "%window-renamed" => {
                let window_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing window_id".to_string()))?
                    .to_string();
                let name = parts.get(2).unwrap_or(&"").to_string();
                Ok(Notification::WindowRenamed { window_id, name })
            }
            "%session-changed" => {
                let session_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing session_id".to_string()))?
                    .to_string();
                let name = parts.get(2).unwrap_or(&"").to_string();
                Ok(Notification::SessionChanged { session_id, name })
            }
            "%layout-change" => {
                let window_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing window_id".to_string()))?
                    .to_string();
                let layout = parts.get(2).unwrap_or(&"").to_string();
                Ok(Notification::LayoutChange { window_id, layout })
            }
            "%pane-mode-changed" => {
                let pane_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing pane_id".to_string()))?
                    .to_string();
                Ok(Notification::PaneModeChanged { pane_id })
            }
            "%sessions-changed" => {
                Ok(Notification::SessionsChanged)
            }
            "%client-session-changed" => {
                let client = parts.get(1).unwrap_or(&"").to_string();
                let session_id = parts.get(2).unwrap_or(&"").to_string();
                let name = parts.get(3).unwrap_or(&"").to_string();
                Ok(Notification::ClientSessionChanged { client, session_id, name })
            }
            "%window-pane-changed" => {
                let window_id = parts.get(1).unwrap_or(&"").to_string();
                let pane_id = parts.get(2).unwrap_or(&"").to_string();
                Ok(Notification::WindowPaneChanged { window_id, pane_id })
            }
            "%unlinked-window-add" => {
                let window_id = parts.get(1).unwrap_or(&"").to_string();
                Ok(Notification::UnlinkedWindowAdd { window_id })
            }
            "%client-detached" => {
                let client = parts.get(1).unwrap_or(&"").to_string();
                let reason = parts.get(2).map(|s| s.to_string());
                Ok(Notification::ClientDetached { client, reason })
            }
            "%exit" => {
                let reason = parts.get(1).map(|s| s.to_string());
                Ok(Notification::Exit { reason })
            }
            _ => {
                // Return unknown notification instead of error - allows graceful handling
                Ok(Notification::Unknown {
                    notification_type: notification_type.to_string(),
                    raw: line.to_string(),
                })
            }
        }
    }
}

/// Decode tmux escaped output
/// tmux escapes special characters in %output data
fn decode_output(encoded: &str) -> Vec<u8> {
    let mut result = Vec::new();
    let mut chars = encoded.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('\\') => result.push(b'\\'),
                Some('r') => result.push(b'\r'),
                Some('n') => result.push(b'\n'),
                Some('t') => result.push(b'\t'),
                Some('0') => {
                    // Octal escape: \0xx
                    let mut octal = String::new();
                    for _ in 0..2 {
                        if let Some(&c) = chars.peek() {
                            if c.is_ascii_digit() && c < '8' {
                                octal.push(chars.next().unwrap());
                            } else {
                                break;
                            }
                        }
                    }
                    if let Ok(byte) = u8::from_str_radix(&octal, 8) {
                        result.push(byte);
                    }
                }
                Some(c) => {
                    // Unknown escape, keep as-is
                    result.push(b'\\');
                    let mut buf = [0u8; 4];
                    result.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                None => result.push(b'\\'),
            }
        } else {
            let mut buf = [0u8; 4];
            result.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_begin() {
        let notif = Notification::parse("%begin 1234567890 1 0").unwrap();
        match notif {
            Notification::Begin { id } => assert_eq!(id, 1),
            _ => panic!("Expected Begin notification"),
        }
    }

    #[test]
    fn test_parse_output() {
        let notif = Notification::parse("%output %0 hello\\nworld").unwrap();
        match notif {
            Notification::Output { pane_id, data } => {
                assert_eq!(pane_id, "%0");
                assert_eq!(data, b"hello\nworld");
            }
            _ => panic!("Expected Output notification"),
        }
    }

    #[test]
    fn test_parse_window_add() {
        let notif = Notification::parse("%window-add @1").unwrap();
        match notif {
            Notification::WindowAdd { window_id } => assert_eq!(window_id, "@1"),
            _ => panic!("Expected WindowAdd notification"),
        }
    }

    #[test]
    fn test_parse_data_line() {
        let notif = Notification::parse("some data line").unwrap();
        match notif {
            Notification::Data(s) => assert_eq!(s, "some data line"),
            _ => panic!("Expected Data"),
        }
    }

    #[test]
    fn test_decode_output() {
        assert_eq!(decode_output("hello\\nworld"), b"hello\nworld");
        assert_eq!(decode_output("tab\\there"), b"tab\there");
        assert_eq!(decode_output("back\\\\slash"), b"back\\slash");
    }
}
