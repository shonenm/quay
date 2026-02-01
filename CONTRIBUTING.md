# Contributing to Quay

Thank you for your interest in contributing to Quay!

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/quay.git`
3. Create a branch: `git checkout -b feature/your-feature`
4. Make your changes
5. Run tests: `cargo test`
6. Commit your changes
7. Push to your fork: `git push origin feature/your-feature`
8. Open a Pull Request

## Development Setup

See [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) for detailed setup instructions.

```bash
# Quick start
cargo build
cargo test
cargo run
```

## Code Style

- Run `cargo fmt` before committing (`rustfmt.toml` enforces `style_edition = "2024"`)
- Run `cargo clippy --all-targets -- -D warnings` to check for issues (pedantic lints enabled)
- `unsafe` code is forbidden
- Follow Rust naming conventions
- Add tests for new functionality
- CI will verify all of the above on every PR

## Commit Messages

Use conventional commits format:

```
type(scope): description

Types:
- feat: New feature
- fix: Bug fix
- docs: Documentation changes
- refactor: Code restructuring
- test: Adding tests
- chore: Maintenance tasks
```

Examples:
```
feat(port): add kubernetes port detection
fix(ui): handle empty port list
docs: update installation instructions
```

## Pull Request Guidelines

- Keep PRs focused on a single change
- Include tests for new features
- Update documentation if needed
- Ensure all tests pass
- Write a clear PR description

## Reporting Issues

When reporting bugs, please include:

- Quay version (`quay --version`)
- Operating system and version
- Steps to reproduce
- Expected vs actual behavior
- Error messages (if any)

## Feature Requests

Feature requests are welcome! Please:

- Check if the feature is already requested
- Describe the use case
- Explain how it should work

## Questions?

Feel free to open an issue for questions or discussions.
