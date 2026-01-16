# Quay

A TUI port manager for local processes, SSH forwards, and Docker containers.

## Features

- **Unified View**: See all ports in one place (local, SSH, Docker)
- **Interactive**: Navigate with keyboard, filter, search
- **Actions**: Kill processes, create/remove SSH forwards
- **Fast**: Written in Rust with ratatui

## Installation

```bash
cargo install --path .
```

## Usage

```bash
# Launch TUI
quay

# List ports (non-interactive)
quay list
quay list --json

# Create SSH port forward
quay forward 8080:localhost:80 remote-host

# Kill process on port
quay kill 3000
```

## Keybindings

| Key | Action |
|-----|--------|
| `j/k` | Navigate up/down |
| `/` | Search |
| `Enter` | Show details |
| `K` | Kill selected |
| `f` | Create forward |
| `r` | Refresh |
| `q` | Quit |

## Development

```bash
# Run in development
cargo run

# Build release
cargo build --release

# Run tests
cargo test
```

## License

MIT
