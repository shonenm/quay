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
├─────────────────────────────────────────────────────────────┤
│                       dev/                                  │
│   listen.rs    │    mock.rs      │    check.rs             │
│ (TCP listeners) │  (mock data)   │  (port probing)         │
│                 mod.rs (scenarios + TUI launch)             │
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
├── port/
│   ├── mod.rs        # PortEntry, PortSource, collect_all(remote_host, docker_target)
│   ├── local.rs      # lsof parsing for local ports
│   ├── docker.rs     # docker ps parsing, collect_from_container(), get_container_ip()
│   └── ssh.rs        # SSH forward detection
└── dev/
    ├── mod.rs        # DevCommands, Scenario definitions, run_scenario()
    ├── listen.rs     # spawn_listeners(), TCP accept loop
    ├── check.rs      # Port open/closed probing
    └── mock.rs       # Mock data generation for TUI testing
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
    pub ssh_host: Option<String>,
    pub is_open: bool,
    pub is_loopback: bool,           // 127.0.0.1 bind (docker target)
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
    pub remote_host: Option<String>,      // Remote mode SSH host
    pub docker_target: Option<String>,    // Docker target container name
    pub container_ip: Option<String>,     // Docker target container IP
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
   main() → run_tui(remote_host, docker_target)
                         ↓
              port::collect_all(remote_host, docker_target)
                         ↓
         ┌───────────────┴───────────────────────────────┐
         │  docker_target = None (normal mode)            │
         │  ┌─────────────┼──────────────────┐           │
         │  ↓             ↓                  ↓           │
         │  local(remote) docker(remote) ssh::collect()  │
         ├───────────────────────────────────────────────┤
         │  docker_target = Some (docker target mode)     │
         │  → docker::collect_from_container(target, host)│
         │  → ss -tln inside container                   │
         │  → skip local/docker/ssh + TCP probe          │
         └───────────────┬───────────────────────────────┘
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
# Local mode
lsof -i -P -n -sTCP:LISTEN -Fcpn

# Remote mode
ssh host "lsof -i -P -n -sTCP:LISTEN -Fcpn"
```

Output format (field-based):
```
p12345      # PID
cnode       # Command name
n*:3000     # Network address
```

### Docker Ports

```bash
# Local mode
docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'

# Remote mode
ssh host "docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'"
```

Output format:
```
abc123def456  postgres  0.0.0.0:5432->5432/tcp
def789abc012  app       0.0.0.0:3000-3001->3000-3001/tcp, :::3000-3001->3000-3001/tcp
```

Port range mappings (e.g., `3000-3001->3000-3001/tcp`) are expanded into individual entries. IPv4/IPv6 duplicates are deduplicated per container.

### Docker Container Internal Ports (Docker Target Mode)

```bash
# Remote mode (via SSH)
ssh host "docker exec CONTAINER ss -tln"

# Local mode (future)
docker exec CONTAINER ss -tln
```

Discovers ports LISTEN inside the container, including those not mapped to the host. Container IP is obtained via:
```bash
docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' CONTAINER
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
- `handle_forward_key()` - Forward creation form input (remote_mode skips SSH Host, docker_mode skips Remote Host)
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

## Dev Scenario Flow

`quay dev scenario <name>` bypasses port scanning and builds entries directly from scenario definitions:

```
1. run_scenario()
   ├── spawn_listeners(listen_ports)   # Best-effort, non-fatal on failure
   │       ↓
   │   Vec<JoinHandle<()>>             # Background TCP accept loops
   │
   ├── Build Vec<PortEntry> from scenario definition
   │   ├── should_listen: true  → is_open: true,  process_name: label
   │   └── should_listen: false → is_open: false, process_name: label
   │
   ├── run_tui_with_entries(Some(entries))
   │       ↓
   │   TUI event loop (mock mode — no port::collect_all)
   │
   └── Abort all JoinHandles on TUI exit
```

This allows testing the TUI with both open and closed port entries without requiring real services.

## Remote Mode Flow

`quay --remote user@server` scans a remote host's ports via SSH and allows forwarding them locally:

```
1. Startup
   CLI --remote flag or config.general.remote_host
       ↓
   port::collect_all(Some("user@server"))
       ↓
   ┌────────────────────────────────────────────┐
   │ local::collect(Some(host))                 │
   │   → ssh host "lsof -i -P -n ..."          │
   │   → is_open: true (lsof LISTEN = open)     │
   ├────────────────────────────────────────────┤
   │ docker::collect(Some(host))                │
   │   → ssh host "docker ps ..."               │
   │   → is_open: true                          │
   ├────────────────────────────────────────────┤
   │ ssh::collect() ← always local              │
   │   → ps aux (local SSH tunnel processes)    │
   │   → TCP probe for is_open                  │
   └────────────────────────────────────────────┘

2. Quick Forward (F key)
   selected port → ssh -f -N -L port:localhost:port host
   (same port number, no form needed)

3. Forward Form (f key)
   SSH Host field auto-filled with remote host and locked
   User edits Local Port / Remote Host / Remote Port only

4. Kill
   SSH entries → local kill (tunnel process is local)
   Local/Docker entries → ssh host "kill pid" / ssh host "docker stop id"
```

## Docker Target Mode Flow

`quay --remote ailab --docker syntopic-dev` discovers ports inside a Docker container and forwards them via SSH:

```
1. Startup
   CLI --remote + --docker flags (or config equivalents)
       ↓
   docker::get_container_ip("syntopic-dev", Some("ailab"))
       → ssh ailab "docker inspect -f '{{...IPAddress...}}' syntopic-dev"
       → container_ip = "172.17.0.2"
       ↓
   port::collect_all(Some("ailab"), Some("syntopic-dev"))
       ↓
   docker::collect_from_container("syntopic-dev", Some("ailab"))
       → ssh ailab "docker exec syntopic-dev ss -tln"
       → parse_ss_output() → Vec<PortEntry>
       → is_open: true (ss LISTEN = open)
       → is_loopback: true/false (127.0.0.1 vs 0.0.0.0)
       → Skips local::collect, docker::collect, ssh::collect, probe

2. Quick Forward (F key)
   selected port (3000) + container_ip (172.17.0.2) + host (ailab)
       → ssh -f -N -L 3000:172.17.0.2:3000 ailab
       → localhost:3000 → ailab → 172.17.0.2:3000

3. Forward Form (f key)
   Remote Host = container_ip (locked)
   SSH Host = remote_host (locked)
   User edits Local Port / Remote Port only

4. Kill
   docker exec kill: ssh ailab "docker exec syntopic-dev kill PID"
   (only works if PID is available from ss output)
```

### Docker Target Port Collection

```bash
# ss -tln output from inside container
State  Recv-Q Send-Q  Local Address:Port   Peer Address:Port Process
LISTEN 0      511           *:3000              *:*
LISTEN 0      511     0.0.0.0:5173        0.0.0.0:*
LISTEN 0      128     127.0.0.1:5432      0.0.0.0:*
LISTEN 0      511        [::]:3000           [::]:*
```

Parse rules:
- Skip header line (starts with "State")
- Extract port from 4th column (last segment after `:`)
- IPv4/IPv6 deduplication via HashSet
- `*:port` and `0.0.0.0:port` → forwardable (`is_loopback: false`)
- `127.0.0.1:port` → loopback-only (`is_loopback: true`)
