# Dev Cleaner 🧹

A fast, intelligent developer tool for scanning and cleaning temporary build directories across multiple programming languages.

## Features

- **Multi-Language Support**: Automatically detects and cleans 18+ project types
- **Smart Scanning**: Uses Ripgrep-style traversal with `.gitignore` respect
- **Streaming Progress**: Real-time progress bars with live size calculation streaming
- **Statistics Dashboard**: Comprehensive stats with charts, breakdowns by type, age, and size
- **.gitignore Discovery (Conservative)**: Reads project `.gitignore` to discover extra candidates, but treats them as high-risk by default unless explicitly included
- **Intelligent Deduplication**: Automatically detects only top-level cleanable directories (e.g., reports `.venv` instead of hundreds of nested `__pycache__` directories)
- **Two Modes**: CLI for quick operations, TUI for interactive selection
- **Safe by Default**: Dry-run mode, confirmation prompts, in-use detection, and risk filtering (default: `--max-risk medium`)
- **Fast & Parallel**: Leverages Rust's performance and parallel processing with streaming
- **Configurable**: Custom rules, filters, and exclusions
- **Trash & GC**: Undoable trash batches + `trash list/show/purge/gc`
- **Goal-based Recommend**: `recommend --cleanup 10GB` / `recommend --free-at-least 50GB` with optional `--output-plan`

## Supported Project Types

| Language/Framework | Cleanable Directories |
|-------------------|----------------------|
| **Node.js** | `node_modules`, `.next`, `.nuxt`, `dist`, `build`, `.cache`, `.turbo`, `.parcel-cache` |
| **Rust** | `target` |
| **Python** | `.venv`, `venv`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.tox`, `*.egg-info`, `.eggs`, `build`, `dist` |
| **Java/Maven** | `target`, `out` |
| **Kotlin/Gradle** | `build`, `.gradle`, `out` |
| **Scala (sbt)** | `target`, `project/target` |
| **Clojure** | `target` |
| **Dart/Flutter** | `build`, `.dart_tool` |
| **Haskell** | `.stack-work`, `dist`, `dist-newstyle` |
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
# Scan current directory with real-time progress
dev-cleaner scan

# View comprehensive statistics
dev-cleaner stats ~/projects

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
dev-cleaner clean --dry-run --share

# Include higher-risk targets (e.g. deps like node_modules)
dev-cleaner scan --max-risk high
dev-cleaner clean --max-risk high --auto

# Recommend a cleanup plan (does not delete), then apply it
dev-cleaner recommend ~/projects --cleanup 10GB --output-plan plan.json
dev-cleaner apply plan.json --trash

# Manage trash batches
dev-cleaner trash list
dev-cleaner trash gc --keep-days 30 --keep-gb 20
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
  --json                        Output scan results as JSON
  --explain                     Print the matching rule for each result
  --category <CATEGORY>         Filter by category (cache/build/deps/all)
  --max-risk <MAX_RISK>         Filter by max risk level (low/medium/high/all) (alias: --risk)
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
  --trash                       Move directories to Dev Cleaner's trash (undoable)
  --share                       Print a copy-friendly share summary and log local share_generated event
  --auto                        Skip interactive selection
  -f, --force                   Skip all confirmations
  -v, --verbose                 Verbose output
  --gitignore                   Respect .gitignore files (default: false)
  --category <CATEGORY>         Filter by category (cache/build/deps/all)
  --max-risk <MAX_RISK>         Filter by max risk level (low/medium/high/all) (alias: --risk)
```

#### Plan / Apply / Undo

Generate a machine-readable cleanup plan and apply it later:

```bash
# Create a plan file
dev-cleaner plan ~/projects --older-than 60 --min-size 500 -o plan.json

# Apply the plan (with confirmation)
dev-cleaner apply plan.json

# Apply the plan but move to Dev Cleaner trash (undoable)
dev-cleaner apply plan.json --trash

# Undo a trash batch (printed after clean/apply with --trash)
dev-cleaner undo --batch <BATCH_ID>
```

#### Recommend

Output a recommended list (does not delete), optionally writing a plan file:

```bash
dev-cleaner recommend [PATH] --cleanup 10GB --output-plan plan.json
dev-cleaner recommend [PATH] --free-at-least 50GB --output-plan plan.json
```

#### Trash

Manage trash batches:

```bash
dev-cleaner trash list
dev-cleaner trash show --batch <BATCH_ID>
dev-cleaner trash purge --batch <BATCH_ID>
dev-cleaner trash gc --keep-days 30 --keep-gb 20
```

#### Stats

Show comprehensive statistics about cleanable directories:

```bash
dev-cleaner stats [PATH] [OPTIONS]

Options:
  -d, --depth <DEPTH>      Maximum scan depth
  --top <TOP>              Number of top largest directories to show (default: 10)
  --json                   Export statistics as JSON
  --gitignore              Respect .gitignore files (default: false)
  --category <CATEGORY>    Filter by category (cache/build/deps/all)
  --max-risk <MAX_RISK>    Filter by max risk level (low/medium/high/all) (alias: --risk)
```

The stats command provides:
- **Overview**: Total projects and cleanable space
- **By Project Type**: Aggregated statistics for each language/framework
- **Top N Largest**: List of largest cleanable directories
- **By Age Group**: Breakdown by project age (<30d, 30-90d, >90d)
- **Smart Recommendations**: Actionable insights based on analysis

Example output:
```bash
$ dev-cleaner stats ~/projects --top 5

📊 Dev Cleaner Statistics
================================================================================

📁 Overview
  Total projects: 47
  Cleanable space: 12.5 GB

📦 By Project Type
 Type    | Count | Total Size | Avg Size
---------+-------+------------+----------
 Node.js | 25    | 8.2 GB     | 335 MB
 Rust    | 12    | 3.1 GB     | 258 MB
 Python  | 10    | 1.2 GB     | 120 MB

🏆 Top 5 Largest Directories
 # | Path                           | Size    | Type    | Age
---+--------------------------------+---------+---------+-----
 1 | ~/projects/web-app/node_modules| 1.2 GB  | Node.js | 45d
 2 | ~/projects/rust-cli/target     | 850 MB  | Rust    | 12d
 3 | ~/projects/api/node_modules    | 720 MB  | Node.js | 90d
...

⏰ By Age Group
  📗 Recent (<30 days):   15 projects, 4.2 GB
  📙 Medium (30-90 days): 20 projects, 5.8 GB
  📕 Old (>90 days):      12 projects, 2.5 GB

💡 Recommendations
  • 12 old projects (>90 days) can likely be safely cleaned, freeing up 2.5 GB
  • Top 5 largest directories account for 38% of total space
```

Export as JSON for further analysis:
```bash
dev-cleaner stats ~/projects --json > stats.json
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
# marker_mode = "any_of" # default
# marker_mode = "all_of"
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

### Analyze project statistics

```bash
# Get comprehensive statistics about your projects
dev-cleaner stats ~/projects

# Focus on top 20 largest directories
dev-cleaner stats ~/projects --top 20

# Export statistics as JSON for further analysis
dev-cleaner stats ~/projects --json > stats.json

# Combine with jq for custom queries
dev-cleaner stats ~/projects --json | jq '.by_type | to_entries | sort_by(.value.total_size) | reverse'
```

## Performance

Dev Cleaner is built for speed:

- **Streaming Architecture**: Two-stage scanning with real-time size calculation
  - Stage 1: Fast project detection without size calculation
  - Stage 2: Parallel size calculation with live progress streaming
  - **40-60% faster** than traditional blocking approach
- **Parallel Processing**: Utilizes all CPU cores via `rayon` for both scanning and size calculation
- **Smart Traversal**: Uses `ignore` crate (same as Ripgrep) for efficient directory walking
- **Optimized Detection**: Stops scanning when project type is detected
- **Timeout Protection**: 60-second timeout per directory prevents hangs on extremely large directories
- **Minimal Overhead**: Rust's zero-cost abstractions ensure near-native performance

Benchmark on a typical dev machine (32 projects, ~50GB cleanable):
- Scan: ~2-3 seconds (with real-time streaming progress)
- Clean: ~5-10 seconds (depends on disk I/O)

### Streaming Benefits

The new streaming architecture provides:
- **Immediate Feedback**: See results as they're calculated, not after everything completes
- **Better Resource Utilization**: Parallel size calculation across all CPU cores
- **Progress Visibility**: Real-time progress bar with current directory and completion ETA
- **Improved UX**: No more waiting for scans to complete before seeing any results

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
- Streaming communication with [crossbeam](https://github.com/crossbeam-rs/crossbeam)
- Statistics tables via [prettytable-rs](https://github.com/phsym/prettytable-rs)
- JSON serialization with [serde](https://serde.rs/) and [serde_json](https://github.com/serde-rs/json)
