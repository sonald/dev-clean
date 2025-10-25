use crate::scanner::ProjectInfo;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::fs;
use std::path::Path;

/// Options for cleaning operations
#[derive(Debug, Clone)]
pub struct CleanOptions {
    /// Dry run mode - don't actually delete
    pub dry_run: bool,

    /// Show verbose output
    pub verbose: bool,

    /// Skip confirmation prompts
    pub force: bool,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            verbose: false,
            force: false,
        }
    }
}

/// Result of a cleaning operation
#[derive(Debug)]
pub struct CleanResult {
    /// Number of directories cleaned
    pub cleaned_count: usize,

    /// Total bytes freed
    pub bytes_freed: u64,

    /// Number of failed operations
    pub failed_count: usize,

    /// Error messages
    pub errors: Vec<String>,
}

impl CleanResult {
    /// Returns a human-readable size string
    pub fn size_freed_human(&self) -> String {
        format_size(self.bytes_freed)
    }
}

/// Main cleaner for removing project directories
pub struct Cleaner {
    options: CleanOptions,
}

impl Cleaner {
    /// Create a new cleaner with default options
    pub fn new() -> Self {
        Self {
            options: CleanOptions::default(),
        }
    }

    /// Create a cleaner with custom options
    pub fn with_options(options: CleanOptions) -> Self {
        Self { options }
    }

    /// Set dry run mode
    pub fn dry_run(mut self, dry_run: bool) -> Self {
        self.options.dry_run = dry_run;
        self
    }

    /// Set verbose mode
    pub fn verbose(mut self, verbose: bool) -> Self {
        self.options.verbose = verbose;
        self
    }

    /// Set force mode
    pub fn force(mut self, force: bool) -> Self {
        self.options.force = force;
        self
    }

    /// Clean multiple projects with progress bar
    pub fn clean_multiple(&self, projects: &[ProjectInfo]) -> Result<CleanResult> {
        if projects.is_empty() {
            return Ok(CleanResult {
                cleaned_count: 0,
                bytes_freed: 0,
                failed_count: 0,
                errors: Vec::new(),
            });
        }

        let multi_progress = MultiProgress::new();
        let total_size: u64 = projects.iter().map(|p| p.size).sum();

        let main_pb = multi_progress.add(ProgressBar::new(projects.len() as u64));
        main_pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} projects")
                .unwrap()
                .progress_chars("=>-"),
        );
        main_pb.set_message(format!("Cleaning {} total", format_size(total_size)));

        let mut cleaned_count = 0;
        let mut bytes_freed = 0u64;
        let mut failed_count = 0;
        let mut errors = Vec::new();

        for project in projects {
            let path_str = project.cleanable_dir.display().to_string();
            main_pb.set_message(format!("Cleaning: {}", path_str));

            match self.clean_single(project) {
                Ok(size) => {
                    cleaned_count += 1;
                    bytes_freed += size;

                    if self.options.verbose {
                        println!("✓ Cleaned {} (freed {})",
                            path_str,
                            format_size(size)
                        );
                    }
                }
                Err(e) => {
                    failed_count += 1;
                    let error_msg = format!("Failed to clean {}: {}", path_str, e);
                    errors.push(error_msg.clone());

                    if self.options.verbose {
                        eprintln!("✗ {}", error_msg);
                    }
                }
            }

            main_pb.inc(1);
        }

        main_pb.finish_with_message(format!(
            "Completed: {} cleaned, {} failed, {} freed",
            cleaned_count,
            failed_count,
            format_size(bytes_freed)
        ));

        Ok(CleanResult {
            cleaned_count,
            bytes_freed,
            failed_count,
            errors,
        })
    }

    /// Clean a single project directory
    pub fn clean_single(&self, project: &ProjectInfo) -> Result<u64> {
        let path = &project.cleanable_dir;

        if !path.exists() {
            return Ok(0);
        }

        let size = project.size;

        if self.options.dry_run {
            println!("[DRY RUN] Would remove: {} ({})",
                path.display(),
                format_size(size)
            );
            return Ok(size);
        }

        // Perform actual deletion
        remove_dir_all(path)
            .with_context(|| format!("Failed to remove directory: {}", path.display()))?;

        Ok(size)
    }
}

/// Safely remove a directory and all its contents
fn remove_dir_all(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    // Use fs::remove_dir_all for simplicity
    // Could be replaced with a more robust implementation if needed
    fs::remove_dir_all(path)
        .with_context(|| format!("Failed to remove directory: {}", path.display()))?;

    Ok(())
}

/// Format bytes into human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
    }

    #[test]
    fn test_dry_run() {
        let cleaner = Cleaner::new().dry_run(true);
        assert!(cleaner.options.dry_run);
    }
}
