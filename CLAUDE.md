# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Dev Cleaner is a Rust-based CLI/TUI tool for scanning and cleaning temporary build directories across 18+ programming languages. It uses Ripgrep-style traversal for performance, includes intelligent deduplication to report only top-level cleanable directories, and features a streaming architecture for real-time progress feedback and comprehensive statistics generation.

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
# Test scan on current directory with streaming progress
./target/release/dev-cleaner scan . --depth 1

# Test statistics command
./target/release/dev-cleaner stats . --depth 1
./target/release/dev-cleaner stats . --depth 1 --json

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

The codebase is organized into 6 main modules:

1. **scanner** (`src/scanner/`)
   - `walker.rs`: Core scanning engine using `ignore` crate (Ripgrep-style)
   - `detector.rs`: Project type detection, cleanable directory mapping, and .gitignore integration
   - `size_calculator.rs`: Parallel streaming size calculation with timeout protection
   - `mod.rs`: Public API with `ProjectInfo` struct

2. **cleaner** (`src/cleaner/`)
   - Handles deletion operations with progress bars
   - Implements dry-run mode and verbose output
   - Safe deletion with error collection

3. **cli** (`src/cli/`)
   - Command-line interface using `clap`
   - Subcommands: `scan`, `clean`, `tui`, `stats`, `init-config`
   - Interactive selection mode for manual cleaning
   - Real-time streaming progress for scan command

4. **tui** (`src/tui/`)
   - Full-screen terminal UI using `ratatui`
   - Multi-selection with keyboard navigation
   - Real-time size calculation and display

5. **stats** (`src/stats/`)
   - Comprehensive statistics generation and analysis
   - Multi-dimensional aggregation (by type, age, size)
   - Terminal display with prettytable formatting
   - JSON export for programmatic access

6. **config** (`src/config/`)
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

#### Streaming Architecture

Two-stage scanning for optimal performance and user experience:

**Stage 1: Fast Scan**
- `Scanner::scan_with_streaming()` performs rapid project detection without size calculation
- `check_directory_fast()` creates `ProjectInfo` with `size_calculated=false`
- Returns total count immediately for progress bar setup

**Stage 2: Parallel Size Calculation**
- `SizeCalculator` spawned in background thread
- `calculate_batch_streaming()` uses `rayon::par_iter_mut()` for parallel processing
- Each completed directory sent through `crossbeam::channel` immediately
- Filter thread applies size and age filters before sending to UI

**Benefits**:
- 40-60% faster than traditional blocking approach
- Immediate user feedback with real-time progress
- Parallel utilization of all CPU cores
- Timeout protection (60 seconds per directory) prevents hangs

**Implementation**: `src/scanner/walker.rs::scan_with_streaming()` and `src/scanner/size_calculator.rs`

#### Parallel Scanning

Uses `rayon` for parallel directory traversal:
- Candidates collected sequentially by `ignore::WalkBuilder`
- Parallel processing of candidates with `par_iter()`
- Thread-safe result collection with `Arc<Mutex<Vec<ProjectInfo>>>`
- Size calculation parallelized separately via `SizeCalculator`

#### Project Type Detection

Detection happens in `scanner/detector.rs`:
1. Walk up directory tree from candidate directory
2. Check for marker files (e.g., `package.json`, `Cargo.toml`, `pyproject.toml`)
3. Match directory name against cleanable patterns for detected project type
4. Check lock files for "in-use" detection (prevents cleaning active projects)

#### .gitignore Integration

**Key Insight**: For Git projects, .gitignore files contain exactly the directories that should be cleaned, because gitignored directories are typically build artifacts that can be regenerated.

**Implementation**: `ProjectDetector::parse_gitignore()` and `ProjectDetector::cleanable_dirs_with_gitignore()` in `src/scanner/detector.rs`

**Parsing Logic**:
1. Read `.gitignore` file from project root
2. Extract directory patterns while filtering out:
   - Empty lines and comments (starting with `#`)
   - Negation patterns (starting with `!`)
   - File patterns (containing `.` extension, unless starting with `.` for hidden dirs)
   - Complex wildcard patterns (wildcards in the middle)
   - Protected directories (`.git`, `.svn`, `.hg`, `src`, `lib`, `include`)
   - Known file patterns (`.env`, `.DS_Store`, `.gitignore`, etc.)
3. Clean up patterns by removing leading/trailing slashes
4. Return list of directory names

**Integration Flow**:
```rust
// In walker.rs check_directory():
let cleanable_dirs = ProjectDetector::cleanable_dirs_with_gitignore(project_type, parent);
```

This combines:
- Default cleanable patterns for the detected project type (e.g., `node_modules` for Node.js)
- Custom patterns from `.gitignore` (e.g., `.custom-cache`, `tmp-data`)
- Deduplication to avoid reporting the same directory twice

**Example**:
```gitignore
# .gitignore in a Node.js project
node_modules/      # Already in default patterns
dist/              # Already in default patterns
.custom-cache/     # Custom pattern - will be added
tmp-data           # Custom pattern - will be added
*.log              # File pattern - will be skipped
.env               # Known file - will be skipped
```

Result: Scanner will look for `node_modules`, `.next`, `dist`, `.cache`, `.turbo`, `.parcel-cache` (default patterns) PLUS `.custom-cache` and `tmp-data` (from .gitignore).

**Testing**: See tests in `src/scanner/detector.rs`:
- `test_parse_gitignore`: Tests .gitignore parsing logic
- `test_cleanable_dirs_with_gitignore`: Tests integration with default patterns
- `test_parse_gitignore_no_file`: Tests behavior when .gitignore doesn't exist

#### Statistics System

Comprehensive multi-dimensional statistics generation for analyzing cleanable projects.

**Data Structures** (`src/stats/mod.rs`):
```rust
pub struct Statistics {
    pub total_size: u64,
    pub total_projects: usize,
    pub by_type: HashMap<String, TypeStats>,      // Aggregated by language/framework
    pub top_largest: Vec<ProjectStats>,            // Top N largest directories
    pub by_age_group: AgeGroupStats,              // Grouped by age (<30d, 30-90d, >90d)
}
```

**Generation Process**:
1. `Statistics::from_projects()` takes list of scanned projects
2. Aggregates data across multiple dimensions:
   - By Type: Total size, count, average size per project type
   - By Size: Sorted list of all projects with full details
   - By Age: Categorized into recent/medium/old buckets
3. Calculates smart recommendations based on patterns

**Display Modes**:
- **Terminal**: `display_terminal()` with prettytable formatting
  - Overview section with totals
  - By Type table with aggregated stats
  - Top N Largest table with project details
  - By Age Group breakdown
  - Smart recommendations based on analysis
- **JSON**: `to_json()` for programmatic access
  - Complete data export for external processing
  - Compatible with jq and other JSON tools

**Key Features**:
- Multi-dimensional aggregation (type, size, age)
- Intelligent recommendations based on data patterns
- Both human-readable (terminal) and machine-readable (JSON) output
- Configurable top N display (default: 10)

### Important Behavioral Notes

#### .gitignore Handling

**Two-Part Strategy**:

1. **Scanning Behavior** (controlled by `--gitignore` flag):
   - **Default**: Does NOT respect `.gitignore` when traversing directories
   - **Rationale**: Most cleanable directories (node_modules, target, .venv) are gitignored, so we need to scan them
   - **Implementation**: `Scanner::respect_gitignore()` defaults to `false`
   - User can enable with `--gitignore` flag if they want to skip gitignored directories

2. **Pattern Discovery** (automatic):
   - **Always active**: Reads `.gitignore` files to discover additional cleanable patterns
   - **Rationale**: Gitignored directories are often custom build artifacts that should be cleanable
   - **Implementation**: `ProjectDetector::parse_gitignore()` extracts directory patterns
   - **Smart filtering**: Skips files, protected directories, and complex patterns
   - **Integration**: Combined with default patterns via `cleanable_dirs_with_gitignore()`

**Important**: These are independent features:
- `--gitignore` flag controls WHETHER to traverse gitignored directories
- `.gitignore` parsing discovers WHAT additional directories to clean

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
- Test .gitignore parsing:
  - Create .gitignore with various patterns (directories, files, comments)
  - Verify only directory patterns are extracted
  - Verify protected directories and files are filtered out
  - Test integration with default cleanable patterns

### Cleaner Tests

- Test dry-run mode (should not delete)
- Verify actual deletion works
- Check error handling for permission issues

## Common Gotchas

1. **Size Calculation Performance**:
   - Old approach: `calculate_dir_size()` was slow for large directories - ran synchronously
   - New approach: `SizeCalculator` with parallel streaming and 60-second timeout protection
   - For scan command: Use `scan_with_streaming()` for real-time progress
   - For stats command: Use regular `scan()` since all results needed before aggregation

2. **Streaming vs Regular Scan**:
   - `scan()`: Returns complete results after all sizes calculated - use for stats, clean
   - `scan_with_streaming()`: Returns (count, receiver) for real-time streaming - use for scan command
   - Filter application differs: streaming filters in background thread, regular filters in main thread

3. **ProjectInfo Size Field**:
   - New `size_calculated: bool` field indicates if size is computed
   - Use `new_pending()` constructor for fast scan without size
   - `size_human()` returns "Calculating..." when `size_calculated=false`

4. **Statistics Generation**:
   - Requires complete scan results (use `scan()`, not `scan_with_streaming()`)
   - `from_projects()` consumes the vector - clone if needed elsewhere
   - JSON export uses `serde_json::to_string_pretty()` for human-readable output

5. **DateTime Conversion**: Uses `SystemTime::UNIX_EPOCH` for compatibility. If modified time is unavailable, defaults to `Utc::now()`

6. **TUI Terminal Restoration**: Always call `disable_raw_mode()` and restore terminal even on error - use proper cleanup in error paths

7. **Deduplication Order**: Must happen AFTER parallel processing completes but BEFORE filtering, to ensure accurate size calculations. In streaming mode, deduplication happens before size calculation.

8. **clap Boolean Flags**: Don't use `default_value` with bool types - they are presence flags by default

9. **.gitignore Parsing**: The parser is conservative - it skips patterns containing `.` (unless they start with `.` for hidden dirs) to avoid false positives from file patterns. If a directory name contains a dot (e.g., `build.tmp`), you may need to add it to the config file instead

10. **Channel Communication**:
    - Use `crossbeam::channel::unbounded()` for streaming (not `std::sync::mpsc`)
    - Always spawn sender in separate thread to avoid deadlocks
    - Receiver.iter() blocks until sender is dropped - ensure proper cleanup
