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
    /// %session-window-changed <session-id> <window-id>
    SessionWindowChanged { session_id: String, window_id: String },
    /// %unlinked-window-add <window-id>
    UnlinkedWindowAdd { window_id: String },
    /// %unlinked-window-close <window-id>
    UnlinkedWindowClose { window_id: String },
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
    /// Active window changed (tab switch)
    WindowChanged { window_id: String },
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
                // Note: in production, %output is parsed at the byte level in
                // connection.rs before reaching here. This path exists for tests.
                let pane_id = parts.get(1)
                    .ok_or_else(|| ProtocolError::InvalidFormat("missing pane_id".to_string()))?
                    .to_string();
                let prefix_len = "%output ".len() + pane_id.len() + 1;
                let data = if line.len() > prefix_len {
                    decode_output_bytes(line[prefix_len..].as_bytes())
                } else {
                    Vec::new()
                };
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
                // Name can contain spaces, so we need everything after "%window-renamed <window_id> "
                let prefix_len = "%window-renamed ".len() + window_id.len() + 1;
                let name = if line.len() > prefix_len {
                    line[prefix_len..].to_string()
                } else {
                    String::new()
                };
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
            "%session-window-changed" => {
                let session_id = parts.get(1).unwrap_or(&"").to_string();
                let window_id = parts.get(2).unwrap_or(&"").to_string();
                Ok(Notification::SessionWindowChanged { session_id, window_id })
            }
            "%unlinked-window-add" => {
                let window_id = parts.get(1).unwrap_or(&"").to_string();
                Ok(Notification::UnlinkedWindowAdd { window_id })
            }
            "%unlinked-window-close" => {
                let window_id = parts.get(1).unwrap_or(&"").to_string();
                Ok(Notification::UnlinkedWindowClose { window_id })
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

/// Decode tmux escaped output from raw bytes.
///
/// tmux control mode escapes:
/// - `\\` -> backslash
/// - `\r`, `\n`, `\t` -> CR, LF, TAB
/// - `\ooo` (1-3 octal digits) -> byte value (e.g., `\033` = ESC, `\177` = DEL)
/// - Bytes >= 0x80 are sent as raw bytes (not escaped)
///
/// This operates on `&[u8]` rather than `&str` because the data may contain
/// raw non-UTF-8 bytes from terminal output.
pub fn decode_output_bytes(encoded: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(encoded.len());
    let mut i = 0;

    while i < encoded.len() {
        if encoded[i] == b'\\' {
            i += 1;
            if i >= encoded.len() {
                result.push(b'\\');
                break;
            }
            match encoded[i] {
                b'\\' => {
                    result.push(b'\\');
                    i += 1;
                }
                b'r' => {
                    result.push(b'\r');
                    i += 1;
                }
                b'n' => {
                    result.push(b'\n');
                    i += 1;
                }
                b't' => {
                    result.push(b'\t');
                    i += 1;
                }
                b'0'..=b'7' => {
                    // Octal escape: \o, \oo, or \ooo (tmux uses \%03o = 3 digits)
                    let mut val: u16 = (encoded[i] - b'0') as u16;
                    i += 1;
                    for _ in 0..2 {
                        if i < encoded.len() && encoded[i] >= b'0' && encoded[i] <= b'7' {
                            val = val * 8 + (encoded[i] - b'0') as u16;
                            i += 1;
                        } else {
                            break;
                        }
                    }
                    result.push(val as u8);
                }
                c => {
                    // Unknown escape, keep as-is
                    result.push(b'\\');
                    result.push(c);
                    i += 1;
                }
            }
        } else {
            result.push(encoded[i]);
            i += 1;
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
    fn test_parse_window_renamed() {
        // Simple name without spaces
        let notif = Notification::parse("%window-renamed @1 mytab").unwrap();
        match notif {
            Notification::WindowRenamed { window_id, name } => {
                assert_eq!(window_id, "@1");
                assert_eq!(name, "mytab");
            }
            _ => panic!("Expected WindowRenamed notification"),
        }

        // Name with spaces
        let notif = Notification::parse("%window-renamed @1 my tab name").unwrap();
        match notif {
            Notification::WindowRenamed { window_id, name } => {
                assert_eq!(window_id, "@1");
                assert_eq!(name, "my tab name");
            }
            _ => panic!("Expected WindowRenamed notification"),
        }

        // Empty name
        let notif = Notification::parse("%window-renamed @1").unwrap();
        match notif {
            Notification::WindowRenamed { window_id, name } => {
                assert_eq!(window_id, "@1");
                assert_eq!(name, "");
            }
            _ => panic!("Expected WindowRenamed notification"),
        }
    }

    #[test]
    fn test_decode_output_bytes() {
        assert_eq!(decode_output_bytes(b"hello\\nworld"), b"hello\nworld");
        assert_eq!(decode_output_bytes(b"tab\\there"), b"tab\there");
        assert_eq!(decode_output_bytes(b"back\\\\slash"), b"back\\slash");
        // Octal: \033 = ESC (27)
        assert_eq!(decode_output_bytes(b"\\033"), vec![27u8]);
        // Octal: \177 = DEL (127) - previously broken, only \0xx was handled
        assert_eq!(decode_output_bytes(b"\\177"), vec![127u8]);
        // Octal: \007 = BEL (7)
        assert_eq!(decode_output_bytes(b"\\007"), vec![7u8]);
        // Raw bytes >= 0x80 passed through as-is
        assert_eq!(decode_output_bytes(&[0xE4, 0xB8, 0xAD]), vec![0xE4, 0xB8, 0xAD]);
        // Trailing backslash
        assert_eq!(decode_output_bytes(b"end\\"), b"end\\");
    }
}
