# AGENTS.md

This file provides guidance to coding agents (Codex CLI, Claude Code, etc.) when working with code in this repository.

## Project Overview

Dev Cleaner (`dev-cleaner`) is a Rust-based CLI/TUI tool for scanning and cleaning temporary build directories across multiple programming languages. It prioritizes safety and auditability via:
- Ripgrep-style traversal with optional `.gitignore` respect
- Intelligent deduplication (only report top-level cleanable targets)
- Streaming scan mode (fast discovery + parallel size calculation)
- Rule attribution (`--explain`) with category/risk/confidence metadata
- Undoable trash batches (`--trash`, `undo`, `trash ...`)
- Plan/recommend/apply workflow for machine-readable cleanups

## Build & Development Commands

```bash
# Build debug / release
cargo build
cargo build --release

# Run (dev)
cargo run -- --help
cargo run -- scan . --depth 2

# Run (release artifact)
./target/release/dev-cleaner --help

# Tests
cargo test

# Optional hygiene
cargo fmt
cargo clippy --all-targets --all-features
```

## Quick Manual Testing (Local)

```bash
# Scan with streaming progress (default risk filter is --max-risk medium)
cargo run -- scan . --depth 2 --explain

# Include higher-risk targets (e.g. deps like node_modules, gitignore-discovered dirs)
cargo run -- scan . --depth 2 --max-risk high

# Dry-run clean (never deletes)
cargo run -- clean . --depth 2 --dry-run

# Trash mode (undoable) + show latest batch
cargo run -- clean . --depth 2 --trash --auto
cargo run -- trash list --top 5

# Plan -> Apply workflow
cargo run -- plan . --depth 2 -o plan.json
cargo run -- apply plan.json --dry-run

# Goal-based recommendation (does not delete), then apply
cargo run -- recommend . --cleanup 1GB --output-plan plan.json
cargo run -- apply plan.json --trash

# Undo last trash batch (if any)
cargo run -- undo
```

## Architecture

### Module Map

- `src/scanner/`
  - `walker.rs`: core scan engine (parallel traversal, dedup, filters, rule attribution)
  - `detector.rs`: project-type detection, built-in cleanable patterns, `.gitignore` discovery helpers
  - `size_calculator.rs`: parallel + streaming directory size calculation (timeout protected)
- `src/cleaner/mod.rs`: applies deletion/trash operations with progress + safety checks
- `src/trash.rs`: undoable trash store (batches + JSONL log) and maintenance ops (list/show/purge/gc/restore)
- `src/plan.rs`: `CleanupPlan` schema (JSON) used by `plan`, `recommend --output-plan`, and `apply`
- `src/recommend.rs`: goal-based selection logic (space target / free-space target)
- `src/stats/mod.rs`: aggregates scan results; terminal tables/charts and JSON export
- `src/cli/mod.rs`: clap CLI wiring for all commands/subcommands
- `src/tui/`: ratatui interactive UI
- `src/utils.rs`: formatting and parsing utilities (e.g., human sizes like `10GB`)
- `src/config/mod.rs`: TOML config (`exclude_dirs`, `custom_patterns`, defaults)

### Scan Pipeline (Regular vs Streaming)

`Scanner::scan()` (used by commands that need full results like `stats`):
1. Traverse candidates via `ignore::WalkBuilder` (Ripgrep-style)
2. Detect project root/type, match cleanable targets, and compute directory sizes (blocking)
3. Apply filters (size/age/category/risk)
4. Deduplicate nested targets (`Scanner::deduplicate_nested_dirs`)
5. Sort by size (largest first)

`Scanner::scan_with_streaming()` (used by `scan` for real-time UX):
1. Fast discovery (`check_directory_fast`) to enumerate candidate cleanables without sizes
2. Deduplicate before sizing
3. `SizeCalculator::calculate_batch_streaming` computes sizes in parallel and streams completed `ProjectInfo` values via a channel
4. Non-size filters (age/category/risk) are applied before sizing; callers apply any size threshold after sizes are known (the CLI does this)

### `.gitignore`: Traversal vs Discovery (Two Independent Features)

1. **Traversal behavior** (`--gitignore` / `Scanner::respect_gitignore(true)`):
   - Default: do **not** respect `.gitignore` while traversing (so build dirs like `target/`, `node_modules/` are still scanned).
   - When enabled: walker skips gitignored directories.

2. **Discovery of additional candidates** (always on for Git projects):
   - `ProjectDetector::parse_gitignore` extracts *conservative* directory-like patterns from `.gitignore`.
   - These patterns are treated as **high risk** and **low confidence** by default (see Risk/Confidence below).

### Matching, Rule Attribution, Category, Risk, Confidence

Each result is a `ProjectInfo` (`src/scanner/mod.rs`) with metadata:
- `matched_rule`: where the match came from (`builtin`, `custom`, `gitignore`, `heuristic`) + pattern
- `category`: `cache` / `build` / `deps` (heuristic classification from directory names)
- `risk_level`:
  - defaults by category: `cache=low`, `build=medium`, `deps=high`
  - `.gitignore`-discovered matches are forced to `high` risk (`RuleSource::Gitignore`)
- `confidence`:
  - `builtin`/`custom` = `high`
  - `heuristic` = `medium`
  - `.gitignore` discovery = `low`

The CLI defaults to filtering at `--max-risk medium`, so deps and `.gitignore` discoveries are hidden unless the user opts in.

### In-use Protection

`ProjectDetector::is_in_use` flags projects as `in_use` by checking whether lock files were modified recently (within 7 days).
- `clean`/`apply` skip `in_use` targets unless `--force` is provided.

### Trash System (Undoable Clean)

When `--trash` is enabled (and not `--dry-run`), clean/apply moves directories into a per-run trash batch:
- Root: `default_trash_root()` (override via `DEV_CLEANER_TRASH_DIR`)
- Each move appends a JSONL entry to `trash_log.jsonl`
- `undo` restores a batch (defaults to latest)
- `trash list/show/purge/gc` provides maintenance and cleanup

## Adding / Changing Detection Rules

### Add a New Built-in Project Type

1. Add a variant to `ProjectType` in `src/scanner/detector.rs`.
2. Update `ProjectDetector::detect()` with marker-file checks.
3. Add built-in cleanable patterns in `ProjectDetector::cleanable_dirs()`.
4. If new patterns don’t classify correctly, extend `classify_category()` in `src/scanner/walker.rs`.
5. If applicable, add lock-file checks in `ProjectDetector::is_in_use()`.
6. Add/adjust unit tests in `src/scanner/detector.rs` and/or `src/scanner/walker.rs`.

### Custom Patterns (User Config)

Config lives in `src/config/mod.rs` and is loaded via `--config` or the default path (`Config::default_path()`).
Custom patterns allow naming a rule and scoping it via marker files (see `CustomPattern`).

## Common Gotchas

- `--gitignore` is **off by default**; many cleanable dirs are gitignored, so enabling it can hide real targets.
- `.gitignore`-discovered candidates are intentionally `high` risk + `low` confidence; they won’t show up with the default `--max-risk medium`.
- Deduplication must run before reporting results; without it you’ll get noisy nested matches (e.g., `.venv/.../__pycache__`).
- `apply` is plan-driven: updating the plan schema requires bumping `CleanupPlan.schema_version` (`src/plan.rs`).
