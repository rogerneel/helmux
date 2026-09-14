# Helmux

A modern tmux frontend with a clickable left-side tab bar, built in Rust.

## Status: paused (September 2026)

Helmux was a two-week prototype (January to February 2026) of a tmux-native,
agent-forward terminal multiplexer: a place to run several coding agents side
by side while keeping a real tmux server underneath. Development is paused and
the code is left as-is.

[herdr](https://github.com/herdrdev/herdr) now covers this space thoroughly.
It is a tmux *replacement* rather than a tmux frontend: its own Rust server owns
the PTYs, every pane is classified as working, blocked, idle, or done using
lifecycle hooks and per-agent screen manifests, agents can drive it through a
CLI and socket API, and it handles detach, layout restore, and multiple SSH
machines. If you want a runtime for coding agents today, use herdr.

### How the approaches differ

Helmux bet on layering over stock tmux. It attaches in control mode (`tmux -C`),
re-emulates each pane's `%output` stream with its own VT parser, and draws the
result in a ratatui UI with a tab sidebar. Persistence, detach, and remote attach
came free from tmux. The cost was double terminal emulation, control-mode output
flooding under heavy agent output, and a UI that replaced tmux's own rather than
extending it. Helmux never got as far as agent state detection.

Herdr bet the other way: own the terminals directly, and treat tmux as something
to replace. That made it far more work to build but removed every limit tmux
imposed.

A few helmux ideas still hold up:

- tmux is a perfectly good persistence and remote substrate for agent panes.
- The OSC window title is a strong, cheap "agent is working" signal. Herdr's
  highest-priority Claude Code rule is a regex on exactly that title.
- Agent state detection only needs a snapshot of the bottom of the pane buffer,
  never a full emulator. tmux already exposes that via `capture-pane` and
  `#{pane_title}`.

If this project is ever revived, the promising direction is not a TUI. It is a
thin agent-state layer for stock tmux: watch panes, classify them with
herdr-style manifests, publish state as a pane option for the status line, and
offer `wait` and `prompt` commands so agents can coordinate inside plain tmux.

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
