# Development Guide

## Prerequisites

- Rust 1.88+ (for latest dependencies)
- Docker (optional, for container port detection)

## Setup

```bash
# Clone repository
git clone https://github.com/shonenm/quay.git
cd quay

# Build
cargo build

# Run in development
cargo run

# Run tests
cargo test
```

## Project Structure

```
quay/
├── Cargo.toml            # Dependencies
├── Cargo.lock            # Lock file
├── README.md             # Public documentation
├── DESIGN.md             # Design specification
├── CONTRIBUTING.md       # Contribution guidelines
├── CODE_OF_CONDUCT.md    # Code of conduct
├── SECURITY.md           # Vulnerability reporting
├── LICENSE               # MIT license
├── .github/
│   ├── workflows/        # CI workflows (release, security, apt-repo)
│   └── ISSUE_TEMPLATE/   # Bug report / feature request templates
├── docs/
│   ├── ARCHITECTURE.md
│   ├── DEVELOPMENT.md
│   ├── OSS_BLUEPRINT.md  # Open-source roadmap
│   ├── HOMEBREW_SETUP.md # Homebrew tap setup guide
│   └── APT_SETUP.md      # APT repository setup guide
└── src/
    ├── main.rs           # Entry point
    ├── app.rs            # State management
    ├── config.rs         # Configuration handling
    ├── event.rs          # Event handling
    ├── preset.rs         # SSH presets
    ├── ui.rs             # UI rendering
    ├── port/             # Port collection modules
    └── dev/              # Developer/testing tools
        ├── mod.rs        # DevCommands, scenarios, run_scenario()
        ├── listen.rs     # spawn_listeners(), TCP listener spawning
        ├── check.rs      # Port open/closed checking
        └── mock.rs       # Mock data TUI launch
```

## Commands

### Development

```bash
# Run with debug output
RUST_LOG=debug cargo run

# Run specific test
cargo test test_parse_lsof

# Check without building
cargo check

# Format code
cargo fmt

# Lint
cargo clippy
```

### Build

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Install locally
cargo install --path .
```

## Testing

### Unit Tests

Tests are located alongside the code:

```
src/port/local.rs   → test_parse_lsof_fields, test_parse_lsof_ipv6, test_extract_port,
                      test_parse_lsof_remote_mode
src/port/docker.rs  → test_parse_docker_ps, test_parse_docker_ps_multiple_ports,
                      test_parse_docker_ps_ipv6, test_parse_docker_ps_port_range,
                      test_parse_docker_ps_mixed_range_and_single,
                      test_parse_docker_ps_ipv4_ipv6_dedup, test_parse_docker_ps_empty,
                      test_parse_ss_output, test_parse_ss_output_ipv6_dedup,
                      test_parse_ss_output_loopback, test_parse_ss_output_with_process,
                      test_parse_ss_output_empty
src/port/ssh.rs     → test_parse_ssh_local_forward, test_parse_ssh_remote_forward,
                      test_parse_ssh_multiple_forwards, test_parse_ssh_no_forwards
src/config.rs       → test_default_config, test_parse_config, test_parse_partial_config,
                      test_parse_config_with_remote_host, test_parse_config_with_docker_target
src/preset.rs       → test_default_presets, test_parse_presets
src/app.rs          → test_refresh_ticks_default, test_should_refresh_uses_refresh_ticks,
                      test_is_remote, test_is_docker_target, test_forward_input_for_remote_entry
src/dev/mod.rs      → test_scenario_lookup, test_scenario_web_ports,
                      test_scenario_micro_has_five, test_scenario_full_has_inactive
src/dev/mock.rs     → test_mock_entries_not_empty, test_mock_entries_have_all_sources,
                      test_mock_entries_have_mixed_open_status, test_mock_entries_have_unique_ports,
                      test_mock_docker_entries_have_container_fields, test_mock_local_entries_have_pid
src/main.rs         → test_cli_parse_default, test_cli_parse_list,
                      test_cli_parse_forward, test_cli_parse_kill,
                      test_cli_parse_remote, test_cli_parse_remote_with_list,
                      test_cli_parse_dev_listen, test_cli_parse_dev_listen_http,
                      test_cli_parse_dev_scenario, test_cli_parse_dev_scenario_list,
                      test_cli_parse_dev_check, test_cli_parse_dev_mock,
                      test_cli_parse_docker, test_cli_parse_remote_docker,
                      test_cli_parse_docker_short_flag
```

Run all tests:
```bash
cargo test
```

### Manual Testing

1. **TUI Mode**
   ```bash
   cargo run
   ```
   - Press `j`/`k` to navigate
   - Press `a` to toggle auto-refresh
   - Press `?` for help
   - Press `q` to quit

2. **CLI Mode**
   ```bash
   cargo run -- list
   cargo run -- list --json
   cargo run -- list --local
   ```

3. **SSH Forward**
   ```bash
   # Create forward via CLI
   cargo run -- forward 8080:localhost:80 user@host

   # Create forward via TUI
   # Press 'f' to open forward dialog
   ```

4. **Presets**
   - Create `~/.config/quay/presets.toml` with a `[[preset]]` entry
   - Press `p` in TUI to open preset list
   - Verify `j`/`k` navigates presets, `Enter` launches forward

5. **Mouse**
   - Set `mouse_enabled = true` in `~/.config/quay/config.toml`
   - Verify clicking a row selects it
   - Verify scroll wheel moves selection
   - Set `mouse_enabled = false` and verify mouse is disabled

6. **Configuration**
   - Change `refresh_interval` in `config.toml` and verify auto-refresh timing changes
   - Change `default_filter` and verify the TUI starts with the specified filter
   - Remove `config.toml` and verify defaults are applied

7. **Remote Mode**
   ```bash
   # Start a listener
   cargo run -- dev listen 19000 --http

   # In another terminal: list remote ports (via SSH to localhost)
   cargo run -- --remote localhost list

   # TUI with remote scanning
   cargo run -- --remote localhost
   # → Header shows [remote: localhost]
   # → Select port, press F for Quick Forward
   # → Press f for forward form (SSH Host is locked)

   # Test forwarding (use different local port to avoid collision)
   # In TUI: f → Local Port: 19001, Remote Port: 19000 → Enter
   curl -4 localhost:19001
   ```
   Note: `--remote localhost` is useful for testing the code path but port collisions occur since remote and local are the same machine. Use different local/remote ports when testing forwards.

8. **Docker Target Mode**
   ```bash
   # List container ports (requires a running container on remote host)
   cargo run -- --remote ailab --docker syntopic-dev list
   cargo run -- --remote ailab --docker syntopic-dev list --json

   # TUI with container ports
   cargo run -- --remote ailab --docker syntopic-dev
   # → Header shows [remote: ailab] [docker: syntopic-dev]
   # → Container LISTEN ports displayed (discovered via ss -tln)
   # → Select port, press F for Quick Forward (tunnels to container IP)
   # → Press f for forward form (Remote Host = container IP, SSH Host = ailab, both locked)
   # → Press ? for help (shows container IP and docker target info)

   # Test forwarding
   # In TUI: select port 3000 → F
   curl localhost:3000  # Should reach container's service

   # Short flags
   cargo run -- -r ailab -d syntopic-dev
   ```

9. **Dev Scenarios**
   ```bash
   # TUI with mock data
   cargo run -- dev mock

   # Scenario with open + closed ports
   cargo run -- dev scenario full
   # → TUI shows 5 entries: 3 open (●) + 2 closed (○)

   # Web scenario (all open)
   cargo run -- dev scenario web

   # List available scenarios
   cargo run -- dev scenario --list

   # Standalone listener (Ctrl+C to stop)
   cargo run -- dev listen 4000 5000
   ```
   Note: If scenario listen ports are already in use, the TUI still launches with all entries displayed (listeners are best-effort).

## Configuration

Configuration files are stored in `~/.config/quay/`.

### Creating Config Directory

```bash
mkdir -p ~/.config/quay
```

### config.toml

```bash
cat > ~/.config/quay/config.toml << 'EOF'
[general]
auto_refresh = true
refresh_interval = 5
default_filter = "all"
# remote_host = "user@server"  # optional: default remote host
# docker_target = "my-container"  # optional: default docker container

[ui]
mouse_enabled = true
EOF
```

### presets.toml

```bash
cat > ~/.config/quay/presets.toml << 'EOF'
[[preset]]
name = "Example DB"
local_port = 5432
remote_host = "localhost"
remote_port = 5432
ssh_host = "bastion-host"
EOF
```

## Adding Features

### New Port Source

1. Create `src/port/newtype.rs`
2. Implement `pub async fn collect() -> Result<Vec<PortEntry>>`
3. Add to `src/port/mod.rs`:
   ```rust
   pub mod newtype;

   // In collect_all():
   match newtype::collect().await {
       Ok(new_entries) => entries.extend(new_entries),
       Err(_) => {}
   }
   ```
4. Add `PortSource::NewType` variant

### New Key Binding

1. Add variant to `Action` enum in `event.rs`
2. Add key mapping in `handle_key()`
3. Handle action in `main.rs` event loop
4. Update help screen in `ui.rs`

### New Popup

1. Add variant to `Popup` enum in `app.rs`
2. Add `draw_*_popup()` function in `ui.rs`
3. Add match arm in `draw()` function
4. Handle trigger in `main.rs`

## Code Style

- Use `cargo fmt` before committing
- Follow Rust naming conventions
- Keep functions small and focused
- Add tests for parsing logic

## Commit Convention

```
type(scope): description

Types:
- feat: New feature
- fix: Bug fix
- docs: Documentation
- chore: Maintenance
- refactor: Code restructuring
```

## Release Checklist

1. Update version in `Cargo.toml`
2. Run full test suite: `cargo test`
3. Build release: `cargo build --release`
4. Test binary: `./target/release/quay`
5. Tag release: `git tag v0.x.x`
