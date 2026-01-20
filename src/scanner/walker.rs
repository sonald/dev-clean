use super::{ProjectDetector, ProjectInfo, ProjectType, SizeCalculator};
use crate::config::{CustomPattern, MarkerMode};
use anyhow::Result;
use chrono::{DateTime, Utc};
use crossbeam::channel::{self, Receiver};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use ignore::{WalkBuilder, WalkState};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::SystemTime;

#[derive(Debug)]
struct CleanableMatchers {
    basename: GlobSet,
    relative_path: GlobSet,
}

impl CleanableMatchers {
    fn matches(&self, basename: &str, relative_path: &str) -> bool {
        self.basename.is_match(basename) || self.relative_path.is_match(relative_path)
    }
}

/// Main scanner for finding cleanable project directories
pub struct Scanner {
    /// Root path to scan
    root: PathBuf,

    /// Whether to respect .gitignore files
    respect_gitignore: bool,

    /// Maximum depth to scan (None = unlimited)
    max_depth: Option<usize>,

    /// Minimum size filter in bytes (None = no filter)
    min_size: Option<u64>,

    /// Maximum age in days (None = no filter)
    max_age_days: Option<i64>,

    /// Cache of compiled cleanable directory matchers per project root/type
    matcher_cache: Mutex<HashMap<(PathBuf, ProjectType), Arc<CleanableMatchers>>>,

    /// Directories to always exclude from scanning (by basename)
    exclude_dirs: HashSet<String>,

    /// Custom patterns from user config
    custom_patterns: Vec<CustomPattern>,
}

impl Scanner {
    /// Create a new scanner for the given root path
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            respect_gitignore: false, // Default to false - we want to scan gitignored build dirs
            max_depth: None,
            min_size: None,
            max_age_days: None,
            matcher_cache: Mutex::new(HashMap::new()),
            exclude_dirs: HashSet::new(),
            custom_patterns: Vec::new(),
        }
    }

    /// Set directories to exclude from scanning (by basename)
    pub fn exclude_dirs(mut self, dirs: &[String]) -> Self {
        self.exclude_dirs = dirs.iter().cloned().collect();
        self
    }

    /// Set custom patterns from config
    pub fn custom_patterns(mut self, patterns: &[CustomPattern]) -> Self {
        self.custom_patterns = patterns.to_vec();
        self
    }

    /// Set whether to respect .gitignore files (default: false)
    pub fn respect_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// Set maximum scan depth
    pub fn max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Set minimum size filter in bytes
    pub fn min_size(mut self, size: u64) -> Self {
        self.min_size = Some(size);
        self
    }

    /// Set maximum age in days
    pub fn max_age_days(mut self, days: i64) -> Self {
        self.max_age_days = Some(days);
        self
    }

    /// Scan and return list of cleanable projects
    pub fn scan(&self) -> Result<Vec<ProjectInfo>> {
        let results = Arc::new(Mutex::new(Vec::new()));

        // Build walker with Ripgrep-style configuration
        let mut walker = WalkBuilder::new(&self.root);
        let exclude_dirs = self.exclude_dirs.clone();
        walker
            .hidden(false) // Don't skip hidden files/dirs
            .ignore(self.respect_gitignore) // Respect .gitignore if enabled
            .git_ignore(self.respect_gitignore)
            .git_exclude(self.respect_gitignore)
            .filter_entry(move |entry| {
                // Skip common VCS directories that should never be scanned
                let file_name = entry.file_name().to_string_lossy();
                !matches!(file_name.as_ref(), ".git" | ".svn" | ".hg")
                    && !exclude_dirs.contains(file_name.as_ref())
            });

        if let Some(depth) = self.max_depth {
            walker.max_depth(Some(depth));
        }

        // Use parallel walker for better performance
        walker.threads(num_cpus::get());

        let scanner = self;
        walker.build_parallel().run(|| {
            let results = Arc::clone(&results);
            Box::new(move |entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => return WalkState::Continue,
                };

                if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    return WalkState::Continue;
                }

                let dir = entry.path();
                if let Some(project_info) = scanner.check_directory(dir) {
                    if scanner.passes_filters(&project_info) {
                        results.lock().unwrap().push(project_info);
                    }
                    // If we found a cleanable directory, avoid walking into it.
                    return WalkState::Skip;
                }

                WalkState::Continue
            })
        });

        let mut final_results = Arc::try_unwrap(results).unwrap().into_inner().unwrap();

        // Remove nested cleanable directories to avoid duplicates
        // For example, if we have both .venv and .venv/lib/.../pycache, keep only .venv
        final_results = self.deduplicate_nested_dirs(final_results);

        // Sort by size (largest first)
        final_results.sort_by(|a, b| b.size.cmp(&a.size));

        Ok(final_results)
    }

    /// Scan with streaming size calculation for real-time progress
    ///
    /// This method performs a fast scan first (without calculating sizes), then
    /// streams size calculation results through a channel as they complete.
    ///
    /// # Returns
    /// A tuple of (total_count, receiver) where:
    /// - total_count: Total number of projects found (for progress calculation)
    /// - receiver: Channel to receive completed ProjectInfo with calculated sizes
    ///
    /// # Example
    /// ```no_run
    /// use dev_cleaner::Scanner;
    ///
    /// let scanner = Scanner::new("~/projects");
    /// let (total, rx) = scanner.scan_with_streaming().unwrap();
    ///
    /// println!("Found {} projects, calculating sizes...", total);
    /// for project in rx.iter() {
    ///     println!("{}: {}", project.cleanable_dir.display(), project.size_human());
    /// }
    /// ```
    pub fn scan_with_streaming(&self) -> Result<(usize, Receiver<ProjectInfo>)> {
        // Step 1: Fast scan without size calculation
        let results = Arc::new(Mutex::new(Vec::new()));

        // Build walker with Ripgrep-style configuration
        let mut walker = WalkBuilder::new(&self.root);
        let exclude_dirs = self.exclude_dirs.clone();
        walker
            .hidden(false)
            .ignore(self.respect_gitignore)
            .git_ignore(self.respect_gitignore)
            .git_exclude(self.respect_gitignore)
            .filter_entry(move |entry| {
                let file_name = entry.file_name().to_string_lossy();
                !matches!(file_name.as_ref(), ".git" | ".svn" | ".hg")
                    && !exclude_dirs.contains(file_name.as_ref())
            });

        if let Some(depth) = self.max_depth {
            walker.max_depth(Some(depth));
        }

        walker.threads(num_cpus::get());

        let scanner = self;
        walker.build_parallel().run(|| {
            let results = Arc::clone(&results);
            Box::new(move |entry| {
                let entry = match entry {
                    Ok(e) => e,
                    Err(_) => return WalkState::Continue,
                };

                if !entry.file_type().map_or(false, |ft| ft.is_dir()) {
                    return WalkState::Continue;
                }

                let dir = entry.path();
                if let Some(project_info) = scanner.check_directory_fast(dir) {
                    // Apply age filter early (size filtering will be applied after size calculation).
                    let passes_age = scanner
                        .max_age_days
                        .map_or(true, |ma| project_info.days_since_modified() >= ma);
                    if passes_age {
                        results.lock().unwrap().push(project_info);
                    }

                    // Avoid walking into cleanable directories.
                    return WalkState::Skip;
                }

                WalkState::Continue
            })
        });

        let mut pending_projects = Arc::try_unwrap(results).unwrap().into_inner().unwrap();

        // Deduplicate
        pending_projects = self.deduplicate_nested_dirs(pending_projects);

        let total_count = pending_projects.len();

        // Step 2: Calculate sizes in parallel and stream results
        let (tx, rx) = channel::unbounded();

        // Spawn background thread for size calculation
        thread::spawn(move || {
            let calculator = SizeCalculator::new();
            calculator.calculate_batch_streaming(pending_projects, tx);
        });

        Ok((total_count, rx))
    }

    /// Remove nested cleanable directories, keeping only the topmost ones
    fn deduplicate_nested_dirs(&self, results: Vec<ProjectInfo>) -> Vec<ProjectInfo> {
        let mut deduplicated = Vec::new();

        for info in &results {
            // Check if this directory is nested inside any other cleanable directory
            let is_nested = results.iter().any(|other| {
                // Skip self-comparison
                if info.cleanable_dir == other.cleanable_dir {
                    return false;
                }

                // Check if info.cleanable_dir is a subdirectory of other.cleanable_dir
                info.cleanable_dir.starts_with(&other.cleanable_dir)
            });

            // Only keep this directory if it's not nested inside another
            if !is_nested {
                deduplicated.push(info.clone());
            }
        }

        deduplicated
    }

    /// Check if a directory is a cleanable project directory
    fn check_directory(&self, dir: &Path) -> Option<ProjectInfo> {
        self.check_directory_impl(dir, false)
    }

    /// Check if a directory is a cleanable project directory (fast mode - no size calc)
    fn check_directory_fast(&self, dir: &Path) -> Option<ProjectInfo> {
        self.check_directory_impl(dir, true)
    }

    /// Implementation of directory checking with configurable fast mode
    fn check_directory_impl(&self, dir: &Path, fast_mode: bool) -> Option<ProjectInfo> {
        // Try to detect project type by looking at parent directories
        let mut current = dir;

        // Check if this directory itself is a cleanable target
        let dir_name = dir.file_name()?.to_string_lossy();

        // Look for project root by checking parent directories
        while let Some(parent) = current.parent() {
            let relative_path = normalize_relative_path(dir.strip_prefix(parent).ok()?);

            // Custom patterns (higher priority than builtin/.gitignore patterns)
            for custom in &self.custom_patterns {
                if !custom_root_matches(parent, custom) {
                    continue;
                }

                if !pattern_matches(&custom.directory, dir_name.as_ref(), &relative_path) {
                    continue;
                }

                let info = if fast_mode {
                    self.build_project_info_fast(parent, ProjectType::Generic, dir)
                } else {
                    self.build_project_info(parent, ProjectType::Generic, dir)
                };

                if let Some(mut info) = info {
                    info.project_name = Some(custom.name.clone());
                    return Some(info);
                }
            }

            if let Some(project_type) = ProjectDetector::detect(parent) {
                // Check if current directory is a cleanable dir for this project type
                // This includes both default patterns AND patterns from .gitignore
                let matchers = self.matchers_for(project_type, parent);

                if matchers.matches(dir_name.as_ref(), &relative_path) {
                    return if fast_mode {
                        self.build_project_info_fast(parent, project_type, dir)
                    } else {
                        self.build_project_info(parent, project_type, dir)
                    };
                }
            }
            current = parent;

            // Don't go too far up
            if !current.starts_with(&self.root) {
                break;
            }
        }

        // Heuristic detection for CMake build directories
        // If the directory contains CMakeCache.txt but not CMakeLists.txt,
        // it's likely an out-of-source build directory that can be safely deleted
        if ProjectDetector::is_cmake_build_dir(dir) {
            // Look for parent directory containing CMakeLists.txt (project root)
            let mut search_path = dir;
            while let Some(parent) = search_path.parent() {
                if parent.join("CMakeLists.txt").exists() {
                    // Found the project root
                    return if fast_mode {
                        self.build_project_info_fast(parent, ProjectType::Cpp, dir)
                    } else {
                        self.build_project_info(parent, ProjectType::Cpp, dir)
                    };
                }
                search_path = parent;

                // Don't go too far up
                if !search_path.starts_with(&self.root) {
                    break;
                }
            }
        }

        None
    }

    fn matchers_for(
        &self,
        project_type: ProjectType,
        project_root: &Path,
    ) -> Arc<CleanableMatchers> {
        let key = (project_root.to_path_buf(), project_type);

        // Fast path: cache hit
        if let Some(cached) = self.matcher_cache.lock().unwrap().get(&key) {
            return Arc::clone(cached);
        }

        // Build outside lock to avoid blocking other threads while reading .gitignore
        let built = Arc::new(build_matchers(project_type, project_root));

        let mut cache = self.matcher_cache.lock().unwrap();
        Arc::clone(cache.entry(key).or_insert_with(|| Arc::clone(&built)))
    }

    /// Build ProjectInfo for a cleanable directory (fast scan - no size calculation)
    fn build_project_info_fast(
        &self,
        project_root: &Path,
        project_type: ProjectType,
        cleanable_dir: &Path,
    ) -> Option<ProjectInfo> {
        // Get last modified time
        let metadata = cleanable_dir.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        let last_modified = system_time_to_datetime(modified);

        // Check if project is in use
        let in_use = ProjectDetector::is_in_use(project_root, project_type);

        Some(ProjectInfo::new_pending(
            project_root.to_path_buf(),
            project_type,
            cleanable_dir.to_path_buf(),
            last_modified,
            in_use,
        ))
    }

    /// Build ProjectInfo for a cleanable directory (with size calculation)
    fn build_project_info(
        &self,
        project_root: &Path,
        project_type: ProjectType,
        cleanable_dir: &Path,
    ) -> Option<ProjectInfo> {
        // Calculate directory size
        let size = calculate_dir_size(cleanable_dir).ok()?;

        // Get last modified time
        let metadata = cleanable_dir.metadata().ok()?;
        let modified = metadata.modified().ok()?;
        let last_modified = system_time_to_datetime(modified);

        // Check if project is in use
        let in_use = ProjectDetector::is_in_use(project_root, project_type);

        Some(ProjectInfo {
            root: project_root.to_path_buf(),
            project_type,
            project_name: None,
            cleanable_dir: cleanable_dir.to_path_buf(),
            size,
            size_calculated: true,
            last_modified,
            in_use,
        })
    }

    /// Check if project info passes all filters
    fn passes_filters(&self, info: &ProjectInfo) -> bool {
        // Size filter
        if let Some(min_size) = self.min_size {
            if info.size < min_size {
                return false;
            }
        }

        // Age filter
        if let Some(max_age) = self.max_age_days {
            if info.days_since_modified() < max_age {
                return false;
            }
        }

        true
    }
}

fn build_matchers(project_type: ProjectType, project_root: &Path) -> CleanableMatchers {
    let patterns = ProjectDetector::cleanable_dirs_with_gitignore(project_type, project_root);

    let mut basename_builder = GlobSetBuilder::new();
    let mut relpath_builder = GlobSetBuilder::new();

    for pattern in patterns {
        let pattern = pattern.replace('\\', "/");
        let is_relpath = pattern.contains('/');

        let glob = match GlobBuilder::new(&pattern).literal_separator(true).build() {
            Ok(g) => g,
            Err(_) => continue,
        };

        if is_relpath {
            relpath_builder.add(glob);
        } else {
            basename_builder.add(glob);
        }
    }

    let basename = basename_builder
        .build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap());
    let relative_path = relpath_builder
        .build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().unwrap());

    CleanableMatchers {
        basename,
        relative_path,
    }
}

fn normalize_relative_path(relative: &Path) -> String {
    relative.to_string_lossy().replace('\\', "/")
}

fn custom_root_matches(project_root: &Path, custom: &CustomPattern) -> bool {
    if custom.marker_files.is_empty() {
        return false;
    }

    match custom.marker_mode {
        MarkerMode::AnyOf => custom
            .marker_files
            .iter()
            .any(|marker| project_root.join(marker).exists()),
        MarkerMode::AllOf => custom
            .marker_files
            .iter()
            .all(|marker| project_root.join(marker).exists()),
    }
}

fn pattern_matches(pattern: &str, basename: &str, relative_path: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let text = if pattern.contains('/') {
        relative_path
    } else {
        basename
    };

    let glob = match GlobBuilder::new(&pattern).literal_separator(true).build() {
        Ok(g) => g,
        Err(_) => return false,
    };

    glob.compile_matcher().is_match(text)
}

/// Calculate total size of a directory recursively
fn calculate_dir_size(dir: &Path) -> Result<u64> {
    let mut total = 0u64;

    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }

    Ok(total)
}

/// Convert SystemTime to DateTime<Utc>
fn system_time_to_datetime(time: SystemTime) -> DateTime<Utc> {
    let duration = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    DateTime::from_timestamp(duration.as_secs() as i64, 0).unwrap_or_else(|| Utc::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scanner_basic() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a fake Node.js project
        let project_dir = root.join("test-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("package.json"), "{}").unwrap();

        let node_modules = project_dir.join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("test.txt"), "test").unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_type, ProjectType::NodeJs);
    }

    #[test]
    fn test_scanner_python_egg_info() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a fake Python project
        let project_dir = root.join("py-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(
            project_dir.join("pyproject.toml"),
            "[project]\nname = \"x\"\n",
        )
        .unwrap();

        // Create egg-info directory
        let egg_info = project_dir.join("mypkg.egg-info");
        fs::create_dir(&egg_info).unwrap();
        fs::write(egg_info.join("PKG-INFO"), "Name: mypkg\n").unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_type, ProjectType::Python);
        assert!(results[0].cleanable_dir.ends_with("mypkg.egg-info"));
    }

    #[test]
    fn test_scanner_ruby_vendor_bundle() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a fake Ruby project
        let project_dir = root.join("rb-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(
            project_dir.join("Gemfile"),
            "source \"https://rubygems.org\"",
        )
        .unwrap();

        // Create vendor/bundle directory
        let vendor_dir = project_dir.join("vendor");
        fs::create_dir(&vendor_dir).unwrap();
        let vendor_bundle = vendor_dir.join("bundle");
        fs::create_dir(&vendor_bundle).unwrap();
        fs::write(vendor_bundle.join("x"), "y").unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_type, ProjectType::Ruby);
        assert!(results[0].cleanable_dir.ends_with("vendor/bundle"));
    }

    #[test]
    fn test_scanner_cmake_out_of_source() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a CMake project
        let project_dir = root.join("cmake-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("CMakeLists.txt"), "project(test)").unwrap();

        // Create out-of-source build directory with custom name
        let build_dir = project_dir.join("mybuild");
        fs::create_dir(&build_dir).unwrap();
        fs::write(build_dir.join("CMakeCache.txt"), "# CMake cache").unwrap();
        fs::write(build_dir.join("test.o"), "binary").unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        // Should detect the custom-named build directory
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_type, ProjectType::Cpp);
        assert!(results[0].cleanable_dir.ends_with("mybuild"));
    }

    #[test]
    fn test_scanner_cmake_in_source() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a CMake project with in-source build (NOT recommended)
        let project_dir = root.join("cmake-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("CMakeLists.txt"), "project(test)").unwrap();
        fs::write(project_dir.join("CMakeCache.txt"), "# CMake cache").unwrap();

        // Create src directory to simulate real source code
        let src_dir = project_dir.join("src");
        fs::create_dir(&src_dir).unwrap();
        fs::write(src_dir.join("main.cpp"), "int main() {}").unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        // Should NOT detect the project directory itself as cleanable
        // because it contains both source (CMakeLists.txt) and build (CMakeCache.txt)
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_scanner_cmake_multiple_builds() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Create a CMake project
        let project_dir = root.join("cmake-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("CMakeLists.txt"), "project(test)").unwrap();

        // Create multiple build directories with different names
        let build_debug = project_dir.join("build-debug");
        fs::create_dir(&build_debug).unwrap();
        fs::write(build_debug.join("CMakeCache.txt"), "# Debug").unwrap();

        let build_release = project_dir.join("_build");
        fs::create_dir(&build_release).unwrap();
        fs::write(build_release.join("CMakeCache.txt"), "# Release").unwrap();

        // Also create a standard "build" directory
        let build_standard = project_dir.join("build");
        fs::create_dir(&build_standard).unwrap();
        fs::write(build_standard.join("CMakeCache.txt"), "# Standard").unwrap();

        let scanner = Scanner::new(root);
        let results = scanner.scan().unwrap();

        // Should detect all three build directories
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|r| r.project_type == ProjectType::Cpp));

        // Verify all build directories are found
        let found_dirs: Vec<String> = results
            .iter()
            .map(|r| {
                r.cleanable_dir
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert!(found_dirs.contains(&"build-debug".to_string()));
        assert!(found_dirs.contains(&"_build".to_string()));
        assert!(found_dirs.contains(&"build".to_string()));
    }

    #[test]
    fn test_scanner_exclude_dirs_prunes() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let excluded = root.join("excluded");
        fs::create_dir_all(&excluded).unwrap();

        let project_dir = excluded.join("test-project");
        fs::create_dir(&project_dir).unwrap();
        fs::write(project_dir.join("package.json"), "{}").unwrap();

        let node_modules = project_dir.join("node_modules");
        fs::create_dir(&node_modules).unwrap();
        fs::write(node_modules.join("test.txt"), "test").unwrap();

        let exclude_dirs = vec!["excluded".to_string()];
        let scanner = Scanner::new(root).exclude_dirs(&exclude_dirs);
        let results = scanner.scan().unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scanner_custom_patterns() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        let project_dir = root.join("unity-project");
        fs::create_dir(&project_dir).unwrap();
        fs::create_dir(project_dir.join("Assets")).unwrap();
        fs::create_dir(project_dir.join("ProjectSettings")).unwrap();

        let library_dir = project_dir.join("Library");
        fs::create_dir(&library_dir).unwrap();
        fs::write(library_dir.join("x"), "y").unwrap();

        let patterns = vec![CustomPattern {
            name: "Unity".to_string(),
            directory: "Library".to_string(),
            marker_files: vec!["Assets".to_string(), "ProjectSettings".to_string()],
            marker_mode: MarkerMode::AllOf,
        }];

        let scanner = Scanner::new(root).custom_patterns(&patterns);
        let results = scanner.scan().unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].project_type, ProjectType::Generic);
        assert_eq!(results[0].project_name.as_deref(), Some("Unity"));
        assert!(results[0].cleanable_dir.ends_with("Library"));
    }
}
