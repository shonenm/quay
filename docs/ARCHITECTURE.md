# Architecture

## Overview

Quay is a TUI port manager that displays local processes, SSH port forwards, and Docker container ports in a unified interface.

```
┌─────────────────────────────────────────────────────────────┐
│                        main.rs                              │
│                    (CLI + TUI entry)                        │
├─────────────────────────────────────────────────────────────┤
│     app.rs      │     event.rs      │       ui.rs          │
│  (App State)    │  (Event Handling) │   (UI Rendering)     │
├─────────────────────────────────────────────────────────────┤
│   config.rs     │    preset.rs                              │
│  (Settings)     │   (SSH Presets)                           │
├─────────────────────────────────────────────────────────────┤
│                       port/                                 │
│    local.rs    │    docker.rs    │      ssh.rs             │
│   (lsof)       │   (docker ps)   │    (ps aux)             │
└─────────────────────────────────────────────────────────────┘
```

## Directory Structure

```
src/
├── main.rs           # Entry point, CLI parsing, TUI loop
├── app.rs            # Application state (App struct)
├── config.rs         # Configuration file handling
├── event.rs          # Keyboard/mouse event handling
├── preset.rs         # SSH forward presets
├── ui.rs             # UI rendering with ratatui
└── port/
    ├── mod.rs        # PortEntry, PortSource, collect_all()
    ├── local.rs      # lsof parsing for local ports
    ├── docker.rs     # docker ps parsing
    └── ssh.rs        # SSH forward detection
```

## Data Model

### PortEntry

```rust
pub struct PortEntry {
    pub source: PortSource,      // Local | Ssh | Docker
    pub local_port: u16,
    pub remote_host: Option<String>,
    pub remote_port: Option<u16>,
    pub process_name: String,
    pub pid: Option<u32>,
    pub container_id: Option<String>,
    pub container_name: Option<String>,
}
```

### App State

```rust
pub struct App {
    pub entries: Vec<PortEntry>,          // All collected entries
    pub filtered_entries: Vec<PortEntry>, // After filter/search
    pub selected: usize,                  // Current selection
    pub filter: Filter,                   // All|Local|Ssh|Docker
    pub search_query: String,
    pub input_mode: InputMode,            // Normal|Search
    pub popup: Popup,                     // None|Details|Help|Forward|Presets
    pub should_quit: bool,
    pub forward_input: ForwardInput,      // SSH forward creation form
    pub auto_refresh: bool,               // Auto-refresh enabled
    pub tick_count: u32,                  // Tick counter for refresh
    pub refresh_ticks: u32,              // Ticks between auto-refreshes (from config)
    pub status_message: Option<(String, u32)>, // Status with TTL
    pub presets: Vec<Preset>,             // SSH forward presets
    pub preset_selected: usize,           // Selected preset index
}

pub struct ForwardInput {
    pub local_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub ssh_host: String,
    pub active_field: ForwardField,       // Currently focused field
}
```

## Data Flow

```
1. Startup
   main() → run_tui() → port::collect_all()
                              ↓
              ┌───────────────┼───────────────┐
              ↓               ↓               ↓
         local::collect  docker::collect  ssh::collect
              ↓               ↓               ↓
              └───────────────┼───────────────┘
                              ↓
                      Vec<PortEntry>
                              ↓
                    app.set_entries()

2. Event Loop
   event_handler.next() → KeyEvent
                              ↓
                    handle_key() / handle_popup_key()
                              ↓
                          Action
                              ↓
                    app state mutation
                              ↓
                    ui::draw(&app)
```

## Port Collection

### Local Ports (lsof)

```bash
lsof -i -P -n -sTCP:LISTEN -Fcpn
```

Output format (field-based):
```
p12345      # PID
cnode       # Command name
n*:3000     # Network address
```

### Docker Ports

```bash
docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'
```

Output format:
```
abc123def456  postgres  0.0.0.0:5432->5432/tcp
```

### SSH Forwards

Detection:
```bash
ps aux | grep 'ssh.*-[LR]'
```

Detects `-L` (local) and `-R` (remote) forwards.

Creation:
```bash
ssh -f -N -L local_port:remote_host:remote_port ssh_host
```

Creates background SSH process with port forwarding.

## Key Modules

### event.rs

Event handler functions:
- `handle_key()` - Normal mode key handling
- `handle_search_key()` - Search mode input
- `handle_popup_key()` - Popup dismissal
- `handle_forward_key()` - Forward creation form input
- `handle_preset_key()` - Preset selection
- `handle_mouse()` - Mouse click and scroll handling

### ui.rs

Layout structure:
```
┌─────────────────────────────────┐
│ Header (3 lines)                │
├─────────────────────────────────┤
│ Filter bar (3 lines)            │
├─────────────────────────────────┤
│ Port table (flexible)           │
├─────────────────────────────────┤
│ Footer (2 lines)                │
└─────────────────────────────────┘
```

Popup rendering uses `centered_rect()` for modal positioning.

## Dependencies

| Crate | Purpose |
|-------|---------|
| ratatui | Terminal UI framework |
| crossterm | Terminal manipulation |
| clap | CLI argument parsing |
| tokio | Async runtime |
| regex | Port string parsing |
| anyhow | Error handling |
| toml | Config file parsing |
| dirs | Config directory paths |
