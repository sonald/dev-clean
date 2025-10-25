# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Dev Cleaner is a Rust-based CLI/TUI tool for scanning and cleaning temporary build directories across 18+ programming languages. It uses Ripgrep-style traversal for performance and includes intelligent deduplication to report only top-level cleanable directories.

## Build & Development Commands

### Essential Commands

```bash
# Build debug version
cargo build

# Build optimized release version
cargo build --release

# Run the binary
./target/release/dev-cleaner --help

# Run tests
cargo test

# Run specific test
cargo test test_name

# Run with verbose output
cargo test -- --nocapture
```

### Testing the Tool

```bash
# Test scan on current directory
./target/release/dev-cleaner scan . --depth 1

# Test with a Python project (example)
mkdir -p /tmp/test-project && cd /tmp/test-project
echo '{"name":"test"}' > pyproject.toml
mkdir -p .venv/lib/python3.11/site-packages
./target/release/dev-cleaner scan /tmp/test-project

# Test clean with dry-run
./target/release/dev-cleaner clean /tmp/test-project --dry-run
```

## Architecture

### Module Structure

The codebase is organized into 5 main modules:

1. **scanner** (`src/scanner/`)
   - `walker.rs`: Core scanning engine using `ignore` crate (Ripgrep-style)
   - `detector.rs`: Project type detection and cleanable directory mapping
   - `mod.rs`: Public API with `ProjectInfo` struct

2. **cleaner** (`src/cleaner/`)
   - Handles deletion operations with progress bars
   - Implements dry-run mode and verbose output
   - Safe deletion with error collection

3. **cli** (`src/cli/`)
   - Command-line interface using `clap`
   - Subcommands: `scan`, `clean`, `tui`, `init-config`
   - Interactive selection mode for manual cleaning

4. **tui** (`src/tui/`)
   - Full-screen terminal UI using `ratatui`
   - Multi-selection with keyboard navigation
   - Real-time size calculation and display

5. **config** (`src/config/`)
   - TOML-based configuration
   - Custom cleanable patterns
   - Default paths: `~/.config/dev-cleaner/config.toml`

### Key Design Decisions

#### Deduplication Logic

The scanner implements intelligent deduplication to avoid reporting nested cleanable directories. For example, when scanning a Python project with `.venv`, it reports only `.venv` instead of hundreds of nested `__pycache__` directories.

**Implementation**: `Scanner::deduplicate_nested_dirs()` in `src/scanner/walker.rs`
- Filters out directories that are subdirectories of other cleanable directories
- Applied after parallel scanning completes
- Preserves only top-level cleanable targets

#### Parallel Scanning

Uses `rayon` for parallel directory traversal:
- Candidates collected sequentially by `ignore::WalkBuilder`
- Parallel processing of candidates with `par_iter()`
- Thread-safe result collection with `Arc<Mutex<Vec<ProjectInfo>>>`

#### Project Type Detection

Detection happens in `scanner/detector.rs`:
1. Walk up directory tree from candidate directory
2. Check for marker files (e.g., `package.json`, `Cargo.toml`, `pyproject.toml`)
3. Match directory name against cleanable patterns for detected project type
4. Check lock files for "in-use" detection (prevents cleaning active projects)

### Important Behavioral Notes

#### .gitignore Handling

**Default behavior**: Does NOT respect `.gitignore` files
- Rationale: Most cleanable directories (node_modules, target, .venv) are gitignored
- User can enable with `--gitignore` flag if needed

**Implementation**: `Scanner::respect_gitignore()` defaults to `false`

#### VCS Directory Skipping

Always skips `.git`, `.svn`, `.hg` directories regardless of settings
- Hardcoded in `WalkBuilder::filter_entry()`
- Cannot be overridden

## Adding New Language Support

To add support for a new language:

1. **Add project type to `ProjectType` enum** (`src/scanner/detector.rs`):
   ```rust
   pub enum ProjectType {
       // ... existing types
       NewLanguage,
   }
   ```

2. **Add color and name** in `ProjectType` impl:
   ```rust
   pub fn color(&self) -> &'static str {
       match self {
           Self::NewLanguage => "green",
           // ...
       }
   }

   pub fn name(&self) -> &'static str {
       match self {
           Self::NewLanguage => "New Language",
           // ...
       }
   }
   ```

3. **Add detection logic** in `ProjectDetector::detect()`:
   ```rust
   if dir.join("marker-file.config").exists() {
       return Some(ProjectType::NewLanguage);
   }
   ```

4. **Define cleanable directories** in `ProjectDetector::cleanable_dirs()`:
   ```rust
   ProjectType::NewLanguage => vec!["build-output", "cache-dir"],
   ```

5. **Optional: Add lock file detection** in `ProjectDetector::is_in_use()` for active project detection

## Testing Considerations

### Test Data Structure

Use `tempfile` crate for test isolation:
```rust
let temp = TempDir::new().unwrap();
let project_dir = temp.path().join("test-project");
```

### Scanner Tests

- Create mock project structures with marker files
- Verify correct project type detection
- Test deduplication with nested directories
- Validate filter application (size, age)

### Cleaner Tests

- Test dry-run mode (should not delete)
- Verify actual deletion works
- Check error handling for permission issues

## Common Gotchas

1. **Size Calculation**: `calculate_dir_size()` can be slow for large directories - runs synchronously per directory after parallel candidate collection

2. **DateTime Conversion**: Uses `SystemTime::UNIX_EPOCH` for compatibility. If modified time is unavailable, defaults to `Utc::now()`

3. **TUI Terminal Restoration**: Always call `disable_raw_mode()` and restore terminal even on error - use proper cleanup in error paths

4. **Deduplication Order**: Must happen AFTER parallel processing completes but BEFORE filtering, to ensure accurate size calculations

5. **clap Boolean Flags**: Don't use `default_value` with bool types - they are presence flags by default
