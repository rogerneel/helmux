# Helmux

A modern tmux frontend with a clickable left-side tab bar, built in Rust.

## Overview

Helmux wraps tmux's control mode to provide a more user-friendly terminal multiplexer experience with:

- **Left-side tab bar** - Visual, clickable tabs for easy window management
- **Mouse support** - Click to switch tabs, create new tabs
- **Keyboard shortcuts** - Familiar tmux-style prefix key system
- **Full terminal emulation** - VTE-based parsing for accurate rendering

## Features

- Visual tab sidebar with activity indicators
- Click-to-switch tab navigation
- Double-click tab to rename
- Interactive rename dialog (`Ctrl-b ,`)
- Full color and attribute support (256 colors, bold, italic, etc.)
- Mouse passthrough to terminal applications
- Mode indicators in sidebar (shows when prefix key is active)

## Installation

### From Source

```bash
git clone https://github.com/yourusername/helmux
cd helmux
cargo build --release
```

The binary will be at `target/release/helmux`.

### Requirements

- Rust 1.70+ (for building)
- tmux 3.0+ (runtime dependency)

## Usage

```bash
# Start helmux (creates or attaches to default session)
helmux

# Attach to a specific session
helmux --session mysession
```

### Keyboard Shortcuts

All shortcuts use `Ctrl-b` as the prefix key (like tmux):

| Key | Action |
|-----|--------|
| `Ctrl-b c` | Create new tab |
| `Ctrl-b x` | Close current tab |
| `Ctrl-b n` | Next tab |
| `Ctrl-b p` | Previous tab |
| `Ctrl-b 1-9` | Switch to tab N |
| `Ctrl-b b` | Toggle sidebar |
| `Ctrl-b ,` | Rename tab |
| `Ctrl-b d` | Detach |
| `Ctrl-q` | Quit helmux |

### Mouse

- Click a tab in the sidebar to switch to it
- Double-click a tab to rename it
- Click `[+]` at the bottom of sidebar to create a new tab
- Mouse events pass through to terminal applications (vim, etc.)
- Supports click, drag, and scroll in the terminal viewport

## Configuration

Configuration file: `~/.config/helmux/config.toml`

```toml
[sidebar]
width = 20
position = "left"  # or "right"
collapsed = false

[keys]
prefix = "C-b"

[appearance]
# Colors use terminal palette or hex values
active_tab_fg = "white"
active_tab_bg = "blue"
```

## Architecture

Helmux uses tmux's control mode (`tmux -C`) to communicate with tmux programmatically:

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   Helmux    │────▶│    tmux     │────▶│   Shell/    │
│   (Rust)    │◀────│ Control Mode│◀────│    Apps     │
└─────────────┘     └─────────────┘     └─────────────┘
      │
      ▼
┌─────────────┐
│  Terminal   │
│  (ratatui)  │
└─────────────┘
```

- **tmux module**: Connects to tmux, parses notifications, sends commands
- **terminal module**: VTE-based terminal emulator for processing pane output
- **ui module**: ratatui-based rendering with sidebar and viewport widgets
- **input module**: Modal input handling with prefix key system
- **app module**: Application state management with per-tab buffers

## Development Status

Currently implementing core functionality. See `.plan/implementation.md` for the detailed roadmap.

### Completed

- [x] Phase 1: tmux control mode connection
- [x] Phase 2: Terminal buffer with VTE parsing
- [x] Phase 3: Basic TUI rendering
- [x] Phase 4: Layout engine and sidebar
- [x] Phase 5: Tab management
- [x] Phase 6: Input handler and keybindings
- [x] Phase 7: Mouse support

### In Progress

- [ ] Phase 8: Tab renaming (OSC sequences, CLI - interactive rename done)
- [ ] Phase 9: Collapsible sidebar

## License

MIT

## Contributing

Contributions welcome! Please see the implementation plan in `.plan/implementation.md` for areas that need work.
