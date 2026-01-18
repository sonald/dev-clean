# Dev Cleaner - Quick Start Guide

## Installation

```bash
# Clone the repository
git clone <your-repo-url>
cd dev-cleaner

# Build the project
cargo build --release

# Optionally, install to PATH
sudo cp target/release/dev-cleaner /usr/local/bin/
```

## Basic Usage

### 1. Scan for Cleanable Directories

```bash
# Scan current directory
dev-cleaner scan

# Scan specific path
dev-cleaner scan ~/projects

# Scan with filters
dev-cleaner scan --min-size 100 --older-than 30
```

### 2. Interactive TUI Mode (Recommended)

```bash
# Launch interactive mode
dev-cleaner tui ~/projects

# Use keyboard shortcuts:
# ↑/↓ or j/k   - Navigate
# Space        - Select/deselect
# a            - Select all
# d            - Deselect all
# Enter        - Clean selected
# q            - Quit
```

### 3. Clean with CLI

```bash
# Preview what would be deleted (dry-run)
dev-cleaner clean --dry-run

# Move directories to Dev Cleaner trash (undoable)
dev-cleaner clean --trash

# Interactive selection
dev-cleaner clean

# Auto-clean old projects
dev-cleaner clean --older-than 90 --auto

# Clean large projects only
dev-cleaner clean --min-size 500 --auto
```

### 4. Plan / Apply (for scripts)

```bash
# Generate a plan as JSON
dev-cleaner plan ~/projects --older-than 60 --min-size 500 -o plan.json

# Apply the plan later
dev-cleaner apply plan.json

# Apply but move to Dev Cleaner trash (undoable)
dev-cleaner apply plan.json --trash

# Undo the most recent trash batch
dev-cleaner undo
```

## Common Scenarios

### Clean old Node.js projects

```bash
# Find all node_modules older than 60 days
dev-cleaner scan ~/projects --older-than 60

# Clean them (with confirmation)
dev-cleaner clean ~/projects --older-than 60
```

### Free up space on your dev machine

```bash
# Find all projects larger than 500MB
dev-cleaner scan ~ --min-size 500

# Use TUI to selectively clean
dev-cleaner tui ~
```

### Clean specific directory recursively

```bash
# Scan with limited depth
dev-cleaner scan ~/workspace --depth 5

# Clean interactively
dev-cleaner tui ~/workspace
```

## Safety Tips

1. **Always use dry-run first**: `--dry-run` flag shows what would be deleted
2. **Check active projects**: Tool auto-detects projects in use via lock files
3. **Start with filters**: Use `--older-than` and `--min-size` to limit scope
4. **Use TUI for control**: Interactive mode gives you full control over selection

## Configuration

Generate a config file:

```bash
dev-cleaner init-config
```

Edit the config at `~/.config/dev-cleaner/config.toml`:

```toml
# Set defaults for all commands
min_size_mb = 100
max_age_days = 30
default_depth = 10

# Add custom project types
[[custom_patterns]]
name = "Unity"
directory = "Library"
marker_files = ["Assets", "ProjectSettings"]
```

## Troubleshooting

### Permission errors

```bash
# Some directories may require elevated permissions
sudo dev-cleaner clean ~/protected-path
```

### Scan is slow

```bash
# Limit scan depth
dev-cleaner scan --depth 3

# Skip gitignored directories
dev-cleaner scan --gitignore
```

### False positives

```bash
# Use dry-run to check before deleting
dev-cleaner clean --dry-run

# Manually select in TUI mode
dev-cleaner tui
```

## Next Steps

- Read the full [README.md](README.md) for complete documentation
- Check [config.example.toml](config.example.toml) for configuration options
- Report issues or contribute on GitHub

Happy cleaning! 🧹
