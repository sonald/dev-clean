use crate::scanner::ProjectInfo;
use crate::trash::TrashManager;
use crate::utils::format_size;
use anyhow::{Context, Result};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
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

    /// Move directories to Dev Cleaner's trash (undoable) instead of deleting
    pub trash: bool,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            verbose: false,
            force: false,
            trash: false,
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

    /// Number of directories skipped (e.g., in-use protection)
    pub skipped_count: usize,

    /// Total bytes skipped
    pub bytes_skipped: u64,

    /// Number of failed operations
    pub failed_count: usize,

    /// Error messages
    pub errors: Vec<String>,

    /// Trash batch id (when `trash=true`)
    pub trash_batch_id: Option<String>,

    /// Optional audit run id
    pub run_id: Option<String>,
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

    /// Set trash mode
    pub fn trash(mut self, trash: bool) -> Self {
        self.options.trash = trash;
        self
    }

    /// Clean multiple projects with progress bar
    pub fn clean_multiple(&self, projects: &[ProjectInfo]) -> Result<CleanResult> {
        if projects.is_empty() {
            return Ok(CleanResult {
                cleaned_count: 0,
                bytes_freed: 0,
                skipped_count: 0,
                bytes_skipped: 0,
                failed_count: 0,
                errors: Vec::new(),
                trash_batch_id: None,
                run_id: None,
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
        let mut skipped_count = 0;
        let mut bytes_skipped = 0u64;
        let mut failed_count = 0;
        let mut errors = Vec::new();

        let trash_manager = if self.options.trash && !self.options.dry_run {
            Some(TrashManager::new_default()?)
        } else {
            None
        };
        let trash_batch_id = trash_manager.as_ref().map(|m| m.batch_id.clone());

        for project in projects {
            let path_str = project.cleanable_dir.display().to_string();
            main_pb.set_message(format!("Cleaning: {}", path_str));

            if project.in_use && !self.options.force {
                skipped_count += 1;
                bytes_skipped += project.size;

                if self.options.verbose {
                    println!("↷ Skipped {} (in use)", path_str);
                }

                main_pb.inc(1);
                continue;
            }

            match self.clean_single_impl(project, trash_manager.as_ref()) {
                Ok(size) => {
                    cleaned_count += 1;
                    bytes_freed += size;

                    if self.options.verbose {
                        println!("✓ Cleaned {} (freed {})", path_str, format_size(size));
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
            "Completed: {} cleaned, {} skipped, {} failed, {} freed",
            cleaned_count,
            skipped_count,
            failed_count,
            format_size(bytes_freed)
        ));

        Ok(CleanResult {
            cleaned_count,
            bytes_freed,
            skipped_count,
            bytes_skipped,
            failed_count,
            errors,
            trash_batch_id,
            run_id: None,
        })
    }

    /// Clean a single project directory
    pub fn clean_single(&self, project: &ProjectInfo) -> Result<u64> {
        let trash_manager = if self.options.trash && !self.options.dry_run {
            Some(TrashManager::new_default()?)
        } else {
            None
        };
        self.clean_single_impl(project, trash_manager.as_ref())
    }

    fn clean_single_impl(
        &self,
        project: &ProjectInfo,
        trash_manager: Option<&TrashManager>,
    ) -> Result<u64> {
        let path = &project.cleanable_dir;

        if !path.exists() {
            return Ok(0);
        }

        let size = project.size;

        if self.options.dry_run {
            if self.options.trash {
                println!(
                    "[DRY RUN] Would move to trash: {} ({})",
                    path.display(),
                    format_size(size)
                );
            } else {
                println!(
                    "[DRY RUN] Would remove: {} ({})",
                    path.display(),
                    format_size(size)
                );
            }
            return Ok(size);
        }

        if self.options.trash {
            let manager = trash_manager.context("Trash manager not initialized")?;
            manager.trash_dir(path, size)?;
        } else {
            // Perform actual deletion
            remove_dir_all(path)
                .with_context(|| format!("Failed to remove directory: {}", path.display()))?;
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dry_run() {
        let cleaner = Cleaner::new().dry_run(true);
        assert!(cleaner.options.dry_run);
    }
}
