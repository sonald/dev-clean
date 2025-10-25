# Dev Cleaner 🧹

A fast, intelligent developer tool for scanning and cleaning temporary build directories across multiple programming languages.

## Features

- **Multi-Language Support**: Automatically detects and cleans 18+ project types
- **Smart Scanning**: Uses Ripgrep-style traversal with `.gitignore` respect
- **Intelligent .gitignore Integration**: Automatically reads `.gitignore` files to discover custom cleanable directories (because what's gitignored is usually cleanable!)
- **Intelligent Deduplication**: Automatically detects only top-level cleanable directories (e.g., reports `.venv` instead of hundreds of nested `__pycache__` directories)
- **Two Modes**: CLI for quick operations, TUI for interactive selection
- **Safe by Default**: Dry-run mode, confirmation prompts, and in-use detection
- **Fast & Parallel**: Leverages Rust's performance and parallel processing
- **Configurable**: Custom rules, filters, and exclusions

## Supported Project Types

| Language/Framework | Cleanable Directories |
|-------------------|----------------------|
| **Node.js** | `node_modules`, `.next`, `.nuxt`, `dist`, `build`, `.cache`, `.turbo`, `.parcel-cache` |
| **Rust** | `target` |
| **Python** | `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.tox`, `*.egg-info`, `.eggs`, `build`, `dist` |
| **Java/Maven** | `target`, `out` |
| **Kotlin/Gradle** | `build`, `.gradle`, `out` |
| **Go** | `vendor`, `bin` |
| **C/C++** | `build`, `cmake-build-*`, `out` |
| **Ruby** | `vendor/bundle`, `.bundle` |
| **Swift** | `.build`, `DerivedData`, `.swiftpm` |
| **PHP** | `vendor` |
| **Elixir** | `_build`, `deps` |
| **.NET** | `bin`, `obj` |

## Installation

### From Source

```bash
git clone https://github.com/yourusername/dev-cleaner
cd dev-cleaner
cargo build --release
sudo cp target/release/dev-cleaner /usr/local/bin/
```

### Using Cargo

```bash
cargo install dev-cleaner
```

## Usage

### Quick Start

```bash
# Scan current directory
dev-cleaner scan

# Scan specific directory
dev-cleaner scan ~/projects

# Scan with filters
dev-cleaner scan --min-size 100 --older-than 30

# Interactive TUI mode
dev-cleaner tui ~/projects

# Clean with confirmation
dev-cleaner clean

# Auto-clean with filters (dry-run first!)
dev-cleaner clean --older-than 60 --min-size 500 --dry-run
dev-cleaner clean --older-than 60 --min-size 500 --auto
```

### CLI Commands

#### Scan

Scan directories for cleanable projects:

```bash
dev-cleaner scan [PATH] [OPTIONS]

Options:
  -d, --depth <DEPTH>           Maximum scan depth
  --min-size <MIN_SIZE>         Minimum size in MB
  --older-than <OLDER_THAN>     Older than N days
  --gitignore                   Respect .gitignore files (default: false)
```

#### Clean

Clean project directories:

```bash
dev-cleaner clean [PATH] [OPTIONS]

Options:
  -d, --depth <DEPTH>           Maximum scan depth
  --min-size <MIN_SIZE>         Minimum size in MB
  --older-than <OLDER_THAN>     Older than N days
  --dry-run                     Preview without deleting
  --auto                        Skip interactive selection
  -f, --force                   Skip all confirmations
  -v, --verbose                 Verbose output
  --gitignore                   Respect .gitignore files (default: false)
```

#### TUI

Launch interactive terminal UI:

```bash
dev-cleaner tui [PATH]

Keyboard Shortcuts:
  ↑/k      - Move up
  ↓/j      - Move down
  Space    - Toggle selection
  a        - Select all
  d        - Deselect all
  Enter    - Clean selected
  ?/h      - Toggle help
  q/Esc    - Quit
```

### Configuration

Generate default config file:

```bash
dev-cleaner init-config
# Creates config at: ~/.config/dev-cleaner/config.toml (Linux/macOS)
#                    %APPDATA%\dev-cleaner\config.toml (Windows)
```

Example `config.toml`:

```toml
# Directories to always exclude
exclude_dirs = [".git", ".svn", ".hg"]

# Default scan depth
default_depth = 10

# Minimum size in MB
min_size_mb = 100

# Maximum age in days
max_age_days = 30

# Custom patterns
[[custom_patterns]]
name = "Custom Build"
directory = "my-build-dir"
marker_files = ["my-project.config"]
```

## Examples

### Find all Node.js projects over 500MB

```bash
dev-cleaner scan ~/projects --min-size 500 | grep Node
```

### Clean old Python virtual environments

```bash
dev-cleaner clean ~/projects --older-than 90 --dry-run
# Review output, then:
dev-cleaner clean ~/projects --older-than 90 --auto
```

### Interactive cleaning with TUI

```bash
dev-cleaner tui ~/workspace
# Use arrow keys to navigate, Space to select, Enter to clean
```

### Scan monorepo while respecting gitignore

```bash
# By default, scans all directories including gitignored ones
dev-cleaner scan ~/monorepo --depth 5

# Use --gitignore to skip gitignored directories (less common use case)
dev-cleaner scan ~/monorepo --gitignore --depth 5
```

## Performance

Dev Cleaner is built for speed:

- **Parallel Scanning**: Utilizes all CPU cores via `rayon`
- **Smart Traversal**: Uses `ignore` crate (same as Ripgrep) for efficient directory walking
- **Optimized Detection**: Stops scanning when project type is detected
- **Minimal Overhead**: Rust's zero-cost abstractions ensure near-native performance

Benchmark on a typical dev machine (32 projects, ~50GB cleanable):
- Scan: ~2-3 seconds
- Clean: ~5-10 seconds (depends on disk I/O)

## Safety Features

1. **Dry Run**: Test commands with `--dry-run` before actual deletion
2. **In-Use Detection**: Checks lock files to avoid cleaning active projects
3. **Confirmation Prompts**: Interactive selection unless `--auto` is specified
4. **Smart Scanning**: By default scans build directories even if gitignored (use `--gitignore` to respect .gitignore)
5. **VCS Protection**: Never scans `.git`, `.svn`, `.hg` directories

> **Note**: By default, the tool does NOT respect `.gitignore` files, because the directories we want to clean (like `node_modules`, `target`) are typically gitignored. Use `--gitignore` flag if you want to skip gitignored directories.

## Development

### Requirements

- Rust 1.70+
- Cargo

### Build

```bash
cargo build
cargo test
cargo run -- scan
```

### Run Tests

```bash
cargo test
```

## License

MIT

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## Acknowledgments

- Built with [clap](https://github.com/clap-rs/clap) for CLI parsing
- Uses [ratatui](https://github.com/ratatui-org/ratatui) for TUI
- Powered by [ignore](https://github.com/BurntSushi/ripgrep/tree/master/crates/ignore) (from Ripgrep) for fast traversal
- Parallel processing via [rayon](https://github.com/rayon-rs/rayon)
