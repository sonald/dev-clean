use crate::scanner::ProjectInfo;
use crate::trash::TrashManager;
use crate::utils::format_size;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

/// Options for cleaning operations
#[derive(Debug, Clone)]
pub struct CleanOptions {
    /// Dry run mode - don't actually delete
    pub dry_run: bool,

    /// Show verbose output
    pub verbose: bool,

    /// Skip confirmation prompts
    pub force: bool,

    /// Include recently modified targets
    pub include_recent: bool,

    /// Allow deleting protected targets
    pub force_protected: bool,

    /// Move directories to Dev Cleaner's trash (undoable) instead of deleting
    pub trash: bool,

    /// Explicit trash root for non-CLI callers that cannot use process defaults.
    pub trash_root: Option<PathBuf>,
}

impl Default for CleanOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            verbose: false,
            force: false,
            include_recent: false,
            force_protected: false,
            trash: false,
            trash_root: None,
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

/// The type of cleanup being performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanAction {
    Delete,
    Trash,
}

impl CleanAction {
    pub fn dry_run_label(self) -> &'static str {
        match self {
            Self::Delete => "[DRY RUN] Would remove",
            Self::Trash => "[DRY RUN] Would move to trash",
        }
    }
}

/// Observer for cleanup lifecycle events.
pub trait CleanObserver {
    fn on_start(&mut self, _total_projects: usize, _total_size: u64) {}
    fn on_project(&mut self, _project: &ProjectInfo) {}
    fn on_skipped_in_use(&mut self, _project: &ProjectInfo) {}
    fn on_skipped_protected(&mut self, _project: &ProjectInfo) {}
    fn on_skipped_recent(&mut self, _project: &ProjectInfo) {}
    fn on_dry_run(&mut self, _project: &ProjectInfo, _action: CleanAction) {}
    fn on_cleaned(&mut self, _project: &ProjectInfo, _size: u64) {}
    fn on_failed(&mut self, _project: &ProjectInfo, _error: &anyhow::Error) {}
    fn on_finish(&mut self, _result: &CleanResult) {}
}

/// No-op cleanup observer.
#[derive(Debug, Default)]
pub struct NoopCleanObserver;

impl CleanObserver for NoopCleanObserver {}

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

    /// Include recently modified targets.
    pub fn include_recent(mut self, include_recent: bool) -> Self {
        self.options.include_recent = include_recent;
        self
    }

    /// Allow deleting protected targets.
    pub fn force_protected(mut self, force_protected: bool) -> Self {
        self.options.force_protected = force_protected;
        self
    }

    /// Set trash mode
    pub fn trash(mut self, trash: bool) -> Self {
        self.options.trash = trash;
        self
    }

    /// Set an explicit trash root.
    pub fn trash_root(mut self, trash_root: Option<PathBuf>) -> Self {
        self.options.trash_root = trash_root;
        self
    }

    /// Clean multiple projects with progress bar
    pub fn clean_multiple(&self, projects: &[ProjectInfo]) -> Result<CleanResult> {
        let mut observer = NoopCleanObserver;
        self.clean_multiple_with_observer(projects, &mut observer)
    }

    /// Clean multiple projects while reporting progress through an observer.
    pub fn clean_multiple_with_observer<O: CleanObserver>(
        &self,
        projects: &[ProjectInfo],
        observer: &mut O,
    ) -> Result<CleanResult> {
        if projects.is_empty() {
            return Ok(empty_clean_result());
        }

        let total_size: u64 = projects.iter().map(|p| p.size).sum();
        observer.on_start(projects.len(), total_size);

        let mut cleaned_count = 0;
        let mut bytes_freed = 0u64;
        let mut skipped_count = 0;
        let mut bytes_skipped = 0u64;
        let mut failed_count = 0;
        let mut errors = Vec::new();

        let trash_manager = self.build_trash_manager()?;
        let trash_batch_id = trash_manager.as_ref().map(|m| m.batch_id.clone());

        for project in projects {
            observer.on_project(project);

            if self.skip_blocked_project(project, observer, &mut skipped_count, &mut bytes_skipped)
            {
                continue;
            }

            match self.clean_single_impl(project, trash_manager.as_ref(), observer) {
                Ok(size) => {
                    cleaned_count += 1;
                    bytes_freed += size;
                    if !self.options.dry_run {
                        observer.on_cleaned(project, size);
                    }
                }
                Err(e) => {
                    failed_count += 1;
                    let error_msg =
                        format!("Failed to clean {}: {}", project.cleanable_dir.display(), e);
                    errors.push(error_msg.clone());
                    observer.on_failed(project, &e);
                }
            }
        }

        let result = CleanResult {
            cleaned_count,
            bytes_freed,
            skipped_count,
            bytes_skipped,
            failed_count,
            errors,
            trash_batch_id,
            run_id: None,
        };
        observer.on_finish(&result);
        Ok(result)
    }

    fn skip_blocked_project(
        &self,
        project: &ProjectInfo,
        observer: &mut dyn CleanObserver,
        skipped_count: &mut usize,
        bytes_skipped: &mut u64,
    ) -> bool {
        if project.protected && !self.options.force_protected {
            *skipped_count += 1;
            *bytes_skipped = bytes_skipped.saturating_add(project.size);
            observer.on_skipped_protected(project);
            return true;
        }

        if project.recent && !self.options.include_recent {
            *skipped_count += 1;
            *bytes_skipped = bytes_skipped.saturating_add(project.size);
            observer.on_skipped_recent(project);
            return true;
        }

        if project.in_use && !self.options.force {
            *skipped_count += 1;
            *bytes_skipped = bytes_skipped.saturating_add(project.size);
            observer.on_skipped_in_use(project);
            return true;
        }

        false
    }

    fn blocked_single_result(
        &self,
        project: &ProjectInfo,
        observer: &mut dyn CleanObserver,
    ) -> Option<u64> {
        let mut skipped_count = 0usize;
        let mut bytes_skipped = 0u64;
        if self.skip_blocked_project(project, observer, &mut skipped_count, &mut bytes_skipped) {
            let result = CleanResult {
                cleaned_count: 0,
                bytes_freed: 0,
                skipped_count,
                bytes_skipped,
                failed_count: 0,
                errors: Vec::new(),
                trash_batch_id: None,
                run_id: None,
            };
            observer.on_finish(&result);
            return Some(0);
        }
        None
    }

    /// Clean a single project directory
    pub fn clean_single(&self, project: &ProjectInfo) -> Result<u64> {
        let mut observer = NoopCleanObserver;
        self.clean_single_with_observer(project, &mut observer)
    }

    /// Clean a single project directory while reporting through an observer.
    pub fn clean_single_with_observer<O: CleanObserver>(
        &self,
        project: &ProjectInfo,
        observer: &mut O,
    ) -> Result<u64> {
        let existed = project.cleanable_dir.exists();
        observer.on_start(1, project.size);
        observer.on_project(project);

        if let Some(size) = self.blocked_single_result(project, observer) {
            return Ok(size);
        }

        let trash_manager = self.build_trash_manager()?;

        match self.clean_single_impl(project, trash_manager.as_ref(), observer) {
            Ok(size) => {
                if !self.options.dry_run && existed {
                    observer.on_cleaned(project, size);
                }

                let result = CleanResult {
                    cleaned_count: usize::from(existed),
                    bytes_freed: if existed { size } else { 0 },
                    skipped_count: 0,
                    bytes_skipped: 0,
                    failed_count: 0,
                    errors: Vec::new(),
                    trash_batch_id: trash_manager
                        .as_ref()
                        .map(|manager| manager.batch_id.clone()),
                    run_id: None,
                };
                observer.on_finish(&result);
                Ok(size)
            }
            Err(error) => {
                observer.on_failed(project, &error);
                let result = CleanResult {
                    cleaned_count: 0,
                    bytes_freed: 0,
                    skipped_count: 0,
                    bytes_skipped: 0,
                    failed_count: 1,
                    errors: vec![format!(
                        "Failed to clean {}: {}",
                        project.cleanable_dir.display(),
                        error
                    )],
                    trash_batch_id: trash_manager
                        .as_ref()
                        .map(|manager| manager.batch_id.clone()),
                    run_id: None,
                };
                observer.on_finish(&result);
                Err(error)
            }
        }
    }

    fn clean_single_impl(
        &self,
        project: &ProjectInfo,
        trash_manager: Option<&TrashManager>,
        observer: &mut dyn CleanObserver,
    ) -> Result<u64> {
        let path = &project.cleanable_dir;

        if !path.exists() {
            return Ok(0);
        }

        let size = project.size;

        if self.options.dry_run {
            observer.on_dry_run(
                project,
                if self.options.trash {
                    CleanAction::Trash
                } else {
                    CleanAction::Delete
                },
            );
            return Ok(size);
        }

        if self.options.trash {
            let manager = trash_manager.context("Trash manager not initialized")?;
            manager.trash_dir(path, size)?;
        } else {
            remove_dir_all(path)
                .with_context(|| format!("Failed to remove directory: {}", path.display()))?;
        }

        Ok(size)
    }

    fn build_trash_manager(&self) -> Result<Option<TrashManager>> {
        if !self.options.trash || self.options.dry_run {
            return Ok(None);
        }

        let manager = match &self.options.trash_root {
            Some(root) => TrashManager::new_with_root(root.clone())?,
            None => TrashManager::new_default()?,
        };

        Ok(Some(manager))
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

fn empty_clean_result() -> CleanResult {
    CleanResult {
        cleaned_count: 0,
        bytes_freed: 0,
        skipped_count: 0,
        bytes_skipped: 0,
        failed_count: 0,
        errors: Vec::new(),
        trash_batch_id: None,
        run_id: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_dry_run() {
        let cleaner = Cleaner::new().dry_run(true);
        assert!(cleaner.options.dry_run);
    }

    #[derive(Default)]
    struct RecordingObserver {
        events: RefCell<Vec<String>>,
    }

    impl CleanObserver for RecordingObserver {
        fn on_start(&mut self, total_projects: usize, total_size: u64) {
            self.events
                .borrow_mut()
                .push(format!("start:{total_projects}:{total_size}"));
        }

        fn on_project(&mut self, project: &ProjectInfo) {
            self.events
                .borrow_mut()
                .push(format!("project:{}", project.cleanable_dir.display()));
        }

        fn on_skipped_in_use(&mut self, project: &ProjectInfo) {
            self.events
                .borrow_mut()
                .push(format!("skip_in_use:{}", project.cleanable_dir.display()));
        }

        fn on_skipped_protected(&mut self, project: &ProjectInfo) {
            self.events.borrow_mut().push(format!(
                "skip_protected:{}",
                project.cleanable_dir.display()
            ));
        }

        fn on_skipped_recent(&mut self, project: &ProjectInfo) {
            self.events
                .borrow_mut()
                .push(format!("skip_recent:{}", project.cleanable_dir.display()));
        }

        fn on_dry_run(&mut self, project: &ProjectInfo, action: CleanAction) {
            self.events.borrow_mut().push(format!(
                "dry_run:{action:?}:{}",
                project.cleanable_dir.display()
            ));
        }

        fn on_cleaned(&mut self, project: &ProjectInfo, size: u64) {
            self.events.borrow_mut().push(format!(
                "cleaned:{}:{size}",
                project.cleanable_dir.display()
            ));
        }

        fn on_failed(&mut self, project: &ProjectInfo, error: &anyhow::Error) {
            self.events.borrow_mut().push(format!(
                "failed:{}:{error}",
                project.cleanable_dir.display()
            ));
        }

        fn on_finish(&mut self, result: &CleanResult) {
            self.events.borrow_mut().push(format!(
                "finish:{}:{}:{}",
                result.cleaned_count, result.skipped_count, result.failed_count
            ));
        }
    }

    fn project(path: PathBuf, size: u64, in_use: bool) -> ProjectInfo {
        ProjectInfo {
            root: path.parent().unwrap_or(path.as_path()).to_path_buf(),
            project_type: crate::scanner::ProjectType::Rust,
            project_name: None,
            category: crate::scanner::Category::Build,
            risk_level: crate::scanner::RiskLevel::Medium,
            confidence: crate::scanner::Confidence::High,
            matched_rule: None,
            cleanable_dir: path,
            size,
            size_calculated: true,
            last_modified: chrono::Utc::now(),
            in_use,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn test_clean_multiple_with_observer_records_events() {
        let temp = TempDir::new().unwrap();
        let first = temp.path().join("target");
        let second = temp.path().join("cache");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let cleaner = Cleaner::new();
        let mut observer = RecordingObserver::default();
        let projects = vec![
            project(first.clone(), 10, false),
            project(second.clone(), 20, true),
        ];

        let result = cleaner
            .clean_multiple_with_observer(&projects, &mut observer)
            .unwrap();

        assert_eq!(result.cleaned_count, 1);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.bytes_freed, 10);
        assert_eq!(result.bytes_skipped, 20);
        assert!(!first.exists());
        assert!(second.exists());

        let events = observer.events.borrow().clone();
        assert_eq!(
            events,
            vec![
                "start:2:30".to_string(),
                format!("project:{}", first.display()),
                format!("cleaned:{}:10", first.display()),
                format!("project:{}", second.display()),
                format!("skip_in_use:{}", second.display()),
                "finish:1:1:0".to_string(),
            ]
        );
    }

    #[test]
    fn test_cleaner_blocks_protected_and_recent_by_default() {
        let temp = TempDir::new().unwrap();
        let protected_target = temp.path().join("protected");
        let recent_target = temp.path().join("recent");
        fs::create_dir_all(&protected_target).unwrap();
        fs::create_dir_all(&recent_target).unwrap();

        let mut protected = project(protected_target.clone(), 10, false);
        protected.protected = true;
        let mut recent = project(recent_target.clone(), 20, false);
        recent.recent = true;

        let mut observer = RecordingObserver::default();
        let result = Cleaner::new()
            .clean_multiple_with_observer(&[protected, recent], &mut observer)
            .unwrap();

        assert_eq!(result.cleaned_count, 0);
        assert_eq!(result.skipped_count, 2);
        assert_eq!(result.bytes_skipped, 30);
        assert!(protected_target.exists());
        assert!(recent_target.exists());

        let events = observer.events.borrow().clone();
        assert!(events.contains(&format!("skip_protected:{}", protected_target.display())));
        assert!(events.contains(&format!("skip_recent:{}", recent_target.display())));
    }

    #[test]
    fn test_cleaner_force_flags_allow_protected_and_recent() {
        let temp = TempDir::new().unwrap();
        let protected_target = temp.path().join("protected");
        let recent_target = temp.path().join("recent");
        fs::create_dir_all(&protected_target).unwrap();
        fs::create_dir_all(&recent_target).unwrap();

        let mut protected = project(protected_target.clone(), 10, false);
        protected.protected = true;
        let mut recent = project(recent_target.clone(), 20, false);
        recent.recent = true;

        let mut observer = RecordingObserver::default();
        let result = Cleaner::new()
            .force_protected(true)
            .include_recent(true)
            .clean_multiple_with_observer(&[protected, recent], &mut observer)
            .unwrap();

        assert_eq!(result.cleaned_count, 2);
        assert_eq!(result.bytes_freed, 30);
        assert!(!protected_target.exists());
        assert!(!recent_target.exists());
    }

    #[test]
    fn test_clean_single_with_observer_emits_dry_run() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("target");
        fs::create_dir_all(&target).unwrap();

        let cleaner = Cleaner::new().dry_run(true).trash(true);
        let mut observer = RecordingObserver::default();
        let size = cleaner
            .clean_single_with_observer(&project(target.clone(), 42, false), &mut observer)
            .unwrap();

        assert_eq!(size, 42);
        assert!(target.exists());
        assert_eq!(
            observer.events.borrow().clone(),
            vec![
                "start:1:42".to_string(),
                format!("project:{}", target.display()),
                format!("dry_run:{:?}:{}", CleanAction::Trash, target.display()),
                "finish:1:0:0".to_string(),
            ]
        );
    }

    #[test]
    fn test_clean_single_with_observer_emits_cleaned_and_finish() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("target");
        fs::create_dir_all(&target).unwrap();

        let cleaner = Cleaner::new();
        let mut observer = RecordingObserver::default();
        let size = cleaner
            .clean_single_with_observer(&project(target.clone(), 24, false), &mut observer)
            .unwrap();

        assert_eq!(size, 24);
        assert!(!target.exists());
        assert_eq!(
            observer.events.borrow().clone(),
            vec![
                "start:1:24".to_string(),
                format!("project:{}", target.display()),
                format!("cleaned:{}:24", target.display()),
                "finish:1:0:0".to_string(),
            ]
        );
    }

    #[test]
    fn test_clean_single_with_observer_emits_failed_and_finish() {
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("not-a-dir");
        fs::write(&target, "content").unwrap();

        let cleaner = Cleaner::new();
        let mut observer = RecordingObserver::default();
        let result =
            cleaner.clean_single_with_observer(&project(target.clone(), 11, false), &mut observer);

        assert!(result.is_err());
        let events = observer.events.borrow().clone();
        assert_eq!(events[0], "start:1:11");
        assert_eq!(events[1], format!("project:{}", target.display()));
        assert!(events[2].starts_with(&format!("failed:{}:", target.display())));
        assert_eq!(events[3], "finish:0:0:1");
    }
}
