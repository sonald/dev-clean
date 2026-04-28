use dev_cleaner_core::cleaner::{CleanAction, CleanObserver, CleanResult};
use dev_cleaner_core::trash::{RestoreObserver, TrashEntry};
use dev_cleaner_core::utils::format_size;
use dev_cleaner_core::ProjectInfo;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

pub(crate) struct TerminalCleanObserver {
    verbose: bool,
    progress: Option<ProgressBar>,
    multi_progress: MultiProgress,
}

pub(crate) struct TerminalRestoreObserver {
    verbose: bool,
}

impl TerminalRestoreObserver {
    pub(crate) fn new(verbose: bool) -> Self {
        Self { verbose }
    }
}

impl RestoreObserver for TerminalRestoreObserver {
    fn on_dry_run(&mut self, entry: &TrashEntry) {
        if self.verbose {
            println!(
                "[DRY RUN] Would restore: {} -> {}",
                entry.trashed_path.display(),
                entry.original_path.display()
            );
        }
    }

    fn on_restored(&mut self, entry: &TrashEntry) {
        if self.verbose {
            println!("✓ Restored {}", entry.original_path.display());
        }
    }
}

impl TerminalCleanObserver {
    pub(crate) fn new(verbose: bool) -> Self {
        Self {
            verbose,
            progress: None,
            multi_progress: MultiProgress::new(),
        }
    }

    fn progress_bar(&self) -> Option<&ProgressBar> {
        self.progress.as_ref()
    }

    fn inc(&self) {
        if let Some(progress) = self.progress_bar() {
            progress.inc(1);
        }
    }
}

impl CleanObserver for TerminalCleanObserver {
    fn on_start(&mut self, total_projects: usize, total_size: u64) {
        let progress = self
            .multi_progress
            .add(ProgressBar::new(total_projects as u64));
        progress.set_style(
            ProgressStyle::default_bar()
                .template("{msg}\n{bar:40.cyan/blue} {pos}/{len} projects")
                .unwrap()
                .progress_chars("=>-"),
        );
        progress.set_message(format!("Cleaning {} total", format_size(total_size)));
        self.progress = Some(progress);
    }

    fn on_project(&mut self, project: &ProjectInfo) {
        if let Some(progress) = self.progress_bar() {
            progress.set_message(format!("Cleaning: {}", project.cleanable_dir.display()));
        }
    }

    fn on_skipped_in_use(&mut self, project: &ProjectInfo) {
        if self.verbose {
            println!("↷ Skipped {} (in use)", project.cleanable_dir.display());
        }
        self.inc();
    }

    fn on_skipped_protected(&mut self, project: &ProjectInfo) {
        if self.verbose {
            println!("↷ Skipped {} (protected)", project.cleanable_dir.display());
        }
        self.inc();
    }

    fn on_skipped_recent(&mut self, project: &ProjectInfo) {
        if self.verbose {
            println!("↷ Skipped {} (recent)", project.cleanable_dir.display());
        }
        self.inc();
    }

    fn on_dry_run(&mut self, project: &ProjectInfo, action: CleanAction) {
        println!(
            "{}: {} ({})",
            action.dry_run_label(),
            project.cleanable_dir.display(),
            format_size(project.size)
        );
        self.inc();
    }

    fn on_cleaned(&mut self, project: &ProjectInfo, size: u64) {
        if self.verbose {
            println!(
                "✓ Cleaned {} (freed {})",
                project.cleanable_dir.display(),
                format_size(size)
            );
        }
        self.inc();
    }

    fn on_failed(&mut self, project: &ProjectInfo, error: &anyhow::Error) {
        let error_msg = format!(
            "Failed to clean {}: {}",
            project.cleanable_dir.display(),
            error
        );
        if self.verbose {
            eprintln!("✗ {}", error_msg);
        }
        self.inc();
    }

    fn on_finish(&mut self, result: &CleanResult) {
        if let Some(progress) = self.progress_bar() {
            progress.finish_with_message(format!(
                "Completed: {} cleaned, {} skipped, {} failed, {} freed",
                result.cleaned_count,
                result.skipped_count,
                result.failed_count,
                format_size(result.bytes_freed)
            ));
        }
    }
}
