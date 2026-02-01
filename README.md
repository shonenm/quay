# Quay

[![CI](https://github.com/shonenm/quay/actions/workflows/ci.yml/badge.svg)](https://github.com/shonenm/quay/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/quay-tui.svg)](https://crates.io/crates/quay-tui)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A TUI port manager for local processes, SSH forwards, and Docker containers.

## Features

- **Unified View**: See all ports in one place (local, SSH, Docker)
- **Remote Mode**: Scan remote hosts via SSH and forward ports with one key (`quay --remote user@server`)
- **Docker Target Mode**: Discover LISTEN ports inside a Docker container and forward them via SSH (`quay --remote host --docker container`)
- **Interactive TUI**: Navigate with keyboard, filter by source, search by name/port
- **Quick Actions**: Kill processes or create SSH forwards directly from the interface
- **SSH Presets**: Save frequently used port forwards as presets for one-key launch
- **Mouse Support**: Click and scroll navigation (configurable)
- **Configuration**: Customize auto-refresh interval, default filter, and more via `~/.config/quay/config.toml`
- **CLI Support**: Non-interactive commands for scripting (`quay list --json`)
- **Fast**: Written in Rust with ratatui

## Installation

```bash
cargo install quay-tui
```

## Usage

### TUI Mode (default)

```bash
quay
```

### Remote Mode

Scan remote host ports via SSH and forward them locally:

```bash
# TUI with remote port scanning
quay --remote user@server

# List remote ports
quay --remote user@server list
quay --remote user@server list --json

# Kill remote process
quay --remote user@server kill 3000
```

In remote TUI mode:
- Header shows `Quay [remote: user@server]`
- Press `F` on any port to **Quick Forward** (same port number, no form)
- Press `f` to open the forward form (SSH Host is auto-filled and locked)

### Docker Target Mode

Discover and forward ports from inside a Docker container on a remote host:

```bash
# TUI with container port scanning
quay --remote ailab --docker syntopic-dev

# List container ports
quay --remote ailab --docker syntopic-dev list
quay --remote ailab --docker syntopic-dev list --json
```

In docker target TUI mode:
- Header shows `Quay [remote: ailab] [docker: syntopic-dev]`
- Ports are discovered via `ss -tln` inside the container (including unmapped ports)
- Press `F` on any port to **Quick Forward** through SSH to the container IP
- Press `f` to open the forward form (Remote Host = container IP, SSH Host = remote host, both locked)
- The tunnel path: `localhost:port → SSH → container_ip:port`

### CLI Commands

```bash
# List all ports
quay list

# Output as JSON
quay list --json

# Filter by source
quay list --local
quay list --ssh
quay list --docker

# Kill process on port
quay kill 3000

# Kill by PID
quay kill 3000 --pid 12345

# Create SSH port forward
quay forward 8080:localhost:80 remote-host

# Create reverse SSH forward
quay forward 8080:localhost:80 remote-host -R
```

## Keybindings

| Key | Action |
|-----|--------|
| `j` / `↓` | Move down |
| `k` / `↑` | Move up |
| `g` / `Home` | Go to first |
| `G` / `End` | Go to last |
| `/` | Search mode |
| `Enter` | Show details |
| `K` | Kill selected process |
| `f` | Create SSH forward |
| `F` | Quick forward (remote/docker mode, same port) |
| `p` | Open presets |
| `r` | Refresh |
| `a` | Toggle auto-refresh |
| `0` | Show all |
| `1` | Local only |
| `2` | SSH only |
| `3` | Docker only |
| `?` | Help |
| `q` / `Esc` | Quit |

## Screenshots

```
┌─────────────────────────────────────────────────────────────┐
│ Quay - Port Manager                                         │
├─────────────────────────────────────────────────────────────┤
│ Filter: [0] All  [/] search  [?] help                       │
├─────────────────────────────────────────────────────────────┤
│ TYPE   │ LOCAL  │ REMOTE          │ PROCESS/CONTAINER       │
├────────┼────────┼─────────────────┼─────────────────────────┤
│ LOCAL  │ :3000  │                 │ node (pid:1234)         │
│ LOCAL  │ :8080  │                 │ python (pid:5678)       │
│ SSH    │ :9000  │ localhost:80    │ ssh (pid:2345)          │
│ DOCKER │ :5432  │ postgres:5432   │ postgres (abc123)       │
├─────────────────────────────────────────────────────────────┤
│ [j/k] Navigate  [Enter] Details  [K] Kill  [f] Forward  [p] Presets  [?] Help  [q] Quit│
└─────────────────────────────────────────────────────────────┘
```

## Configuration

Configuration files are stored in `~/.config/quay/`.

### config.toml

```toml
[general]
auto_refresh = true
refresh_interval = 5
default_filter = "all"  # all, local, ssh, docker
remote_host = "user@server"  # optional: default remote host
docker_target = "my-container"  # optional: default docker container

[ui]
mouse_enabled = true
```

### presets.toml

```toml
[[preset]]
name = "Production DB"
key = "1"
local_port = 5432
remote_host = "localhost"
remote_port = 5432
ssh_host = "prod-bastion"

[[preset]]
name = "Staging Redis"
local_port = 6379
remote_host = "localhost"
remote_port = 6379
ssh_host = "staging-bastion"
```

## Requirements

- Rust 1.85+ (for building from source)
- macOS or Linux (`lsof` for port detection)
- Docker (optional, for container port detection)

## Developer Tools

Built-in tools for testing without real services running:

```bash
# Launch TUI with mock data
quay dev mock

# Run a scenario (spawns listeners + launches TUI)
quay dev scenario full    # 3 open + 2 closed ports
quay dev scenario web     # Web app + DB + Cache
quay dev scenario micro   # 5 microservices
quay dev scenario --list  # Show available scenarios

# Listen on specific ports
quay dev listen 4000 5000
quay dev listen 8080 --http

# Check if ports are open/closed
quay dev check 3000 8080
```

Scenarios launch the TUI with pre-built entries, so both open (`●`) and closed (`○`) ports are visible even if the underlying ports are already in use.

## Development

```bash
# Run in development
cargo run

# Build release
cargo build --release

# Run tests
cargo test

# Lint (pedantic)
cargo clippy --all-targets -- -D warnings
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for more details.

## License

MIT
