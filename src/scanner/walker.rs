use super::{ProjectInfo, ProjectType, ProjectDetector, SizeCalculator};
use anyhow::Result;
use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use std::thread;
use chrono::{DateTime, Utc};
use crossbeam::channel::{self, Receiver};

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
}

impl Scanner {
    /// Create a new scanner for the given root path
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
            respect_gitignore: false,  // Default to false - we want to scan gitignored build dirs
            max_depth: None,
            min_size: None,
            max_age_days: None,
        }
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
        walker
            .hidden(false)                    // Don't skip hidden files/dirs
            .ignore(self.respect_gitignore)   // Respect .gitignore if enabled
            .git_ignore(self.respect_gitignore)
            .git_exclude(self.respect_gitignore)
            .filter_entry(|entry| {
                // Skip common VCS directories that should never be scanned
                let file_name = entry.file_name().to_string_lossy();
                !matches!(file_name.as_ref(), ".git" | ".svn" | ".hg")
            });

        if let Some(depth) = self.max_depth {
            walker.max_depth(Some(depth));
        }

        // Use parallel walker for better performance
        walker.threads(num_cpus::get());

        // Collect candidate directories
        let candidates: Vec<PathBuf> = walker
            .build()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_dir()))
            .map(|entry| entry.into_path())
            .collect();

        // Process candidates in parallel
        candidates.par_iter().for_each(|dir| {
            if let Some(project_info) = self.check_directory(dir) {
                if self.passes_filters(&project_info) {
                    results.lock().unwrap().push(project_info);
                }
            }
        });

        let mut final_results = Arc::try_unwrap(results)
            .unwrap()
            .into_inner()
            .unwrap();

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
        walker
            .hidden(false)
            .ignore(self.respect_gitignore)
            .git_ignore(self.respect_gitignore)
            .git_exclude(self.respect_gitignore)
            .filter_entry(|entry| {
                let file_name = entry.file_name().to_string_lossy();
                !matches!(file_name.as_ref(), ".git" | ".svn" | ".hg")
            });

        if let Some(depth) = self.max_depth {
            walker.max_depth(Some(depth));
        }

        walker.threads(num_cpus::get());

        // Collect candidate directories
        let candidates: Vec<PathBuf> = walker
            .build()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().map_or(false, |ft| ft.is_dir()))
            .map(|entry| entry.into_path())
            .collect();

        // Process candidates in parallel (fast mode - no size calculation)
        candidates.par_iter().for_each(|dir| {
            if let Some(project_info) = self.check_directory_fast(dir) {
                // Note: Size filtering will be applied after size calculation
                results.lock().unwrap().push(project_info);
            }
        });

        let mut pending_projects = Arc::try_unwrap(results)
            .unwrap()
            .into_inner()
            .unwrap();

        // Deduplicate
        pending_projects = self.deduplicate_nested_dirs(pending_projects);

        let total_count = pending_projects.len();

        // Step 2: Calculate sizes in parallel and stream results
        let (tx, rx) = channel::unbounded();
        let min_size = self.min_size;
        let max_age_days = self.max_age_days;

        // Spawn background thread for size calculation
        thread::spawn(move || {
            let calculator = SizeCalculator::new();
            calculator.calculate_batch_streaming(pending_projects, tx);
        });

        // Create a new receiver that filters results
        let (filtered_tx, filtered_rx) = channel::unbounded();
        let min_size_clone = min_size;
        let max_age_clone = max_age_days;

        thread::spawn(move || {
            for project in rx.iter() {
                // Apply filters
                let passes_size = min_size_clone.map_or(true, |ms| project.size >= ms);
                let passes_age = max_age_clone.map_or(true, |ma| {
                    project.days_since_modified() >= ma
                });

                if passes_size && passes_age {
                    let _ = filtered_tx.send(project);
                }
            }
        });

        Ok((total_count, filtered_rx))
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
            if let Some(project_type) = ProjectDetector::detect(parent) {
                // Check if current directory is a cleanable dir for this project type
                // This includes both default patterns AND patterns from .gitignore
                let cleanable_dirs = ProjectDetector::cleanable_dirs_with_gitignore(project_type, parent);

                if cleanable_dirs.iter().any(|d| d == dir_name.as_ref()) {
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

        None
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
    let duration = time.duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    DateTime::from_timestamp(duration.as_secs() as i64, 0)
        .unwrap_or_else(|| Utc::now())
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
}
