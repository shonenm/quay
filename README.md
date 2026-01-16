# Quay

A TUI port manager for local processes, SSH forwards, and Docker containers.

## Features

- **Unified View**: See all ports in one place (local, SSH, Docker)
- **Interactive TUI**: Navigate with keyboard, filter by source, search by name/port
- **Quick Actions**: Kill processes directly from the interface
- **CLI Support**: Non-interactive commands for scripting
- **Fast**: Written in Rust with ratatui

## Installation

```bash
cargo install --path .
```

## Usage

### TUI Mode (default)

```bash
quay
```

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
| `r` | Refresh |
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
│ [j/k] Navigate  [Enter] Details  [K] Kill  [f] Forward  [q] Quit│
└─────────────────────────────────────────────────────────────┘
```

## Requirements

- Rust 1.88+
- macOS (uses `lsof` for port detection)
- Docker (optional, for container port detection)

## Development

```bash
# Run in development
cargo run

# Build release
cargo build --release

# Run tests
cargo test
```

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for more details.

## License

MIT
