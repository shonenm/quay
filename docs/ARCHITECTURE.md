# Architecture

## Overview

Quay is a TUI port manager that displays local processes, SSH port forwards, and Docker container ports in a unified interface.

```
┌─────────────────────────────────────────────────────────────┐
│                        main.rs                              │
│                    (CLI + TUI entry)                        │
├─────────────────────────────────────────────────────────────┤
│     app.rs      │     event.rs      │       ui.rs          │
│  (App State)    │  (Key Handling)   │   (UI Rendering)     │
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
├── event.rs          # Keyboard event handling
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
    pub popup: Popup,                     // None|Details|Help
    pub should_quit: bool,
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

```bash
ps aux | grep 'ssh.*-[LR]'
```

Detects `-L` (local) and `-R` (remote) forwards.

## Key Modules

### event.rs

Three handler functions:
- `handle_key()` - Normal mode key handling
- `handle_search_key()` - Search mode input
- `handle_popup_key()` - Popup dismissal

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
