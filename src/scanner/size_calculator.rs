use crate::ProjectInfo;
use anyhow::Result;
use crossbeam::channel::Sender;
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Size calculator for parallel and streaming directory size computation
pub struct SizeCalculator {
    /// Timeout for calculating a single directory (in seconds)
    timeout_secs: u64,
}

impl SizeCalculator {
    /// Create a new size calculator with default timeout (60 seconds)
    pub fn new() -> Self {
        Self { timeout_secs: 60 }
    }

    /// Create a new size calculator with custom timeout
    pub fn with_timeout(timeout_secs: u64) -> Self {
        Self { timeout_secs }
    }

    /// Calculate sizes for projects in parallel, streaming results as they complete
    ///
    /// Projects are processed in parallel using rayon, and completed results are sent
    /// through the provided sender as soon as they're ready. This allows for real-time
    /// progress display.
    ///
    /// # Arguments
    /// * `projects` - Vector of ProjectInfo with size_calculated=false
    /// * `tx` - Channel sender for streaming completed projects
    ///
    /// # Returns
    /// The number of projects successfully processed
    pub fn calculate_batch_streaming(
        &self,
        mut projects: Vec<ProjectInfo>,
        tx: Sender<ProjectInfo>,
    ) -> usize {
        let timeout = Duration::from_secs(self.timeout_secs);
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Process in parallel using rayon
        projects.par_iter_mut().for_each(|project| {
            // Calculate size with timeout protection
            match calculate_dir_size_with_timeout(&project.cleanable_dir, timeout) {
                Ok(size) => {
                    project.size = size;
                    project.size_calculated = true;
                    completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

                    // Send completed project through channel
                    // Ignore errors if receiver is dropped
                    let _ = tx.send(project.clone());
                }
                Err(_) => {
                    // On timeout or error, mark as calculated with size 0
                    // This prevents infinite waiting
                    project.size = 0;
                    project.size_calculated = true;
                    let _ = tx.send(project.clone());
                }
            }
        });

        completed.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Calculate size for a single project
    ///
    /// This is a convenience method for calculating size for a single project.
    /// For batch operations, use `calculate_batch_streaming` instead.
    pub fn calculate_single(&self, project: &mut ProjectInfo) -> Result<u64> {
        let timeout = Duration::from_secs(self.timeout_secs);
        let size = calculate_dir_size_with_timeout(&project.cleanable_dir, timeout)?;
        project.size = size;
        project.size_calculated = true;
        Ok(size)
    }
}

impl Default for SizeCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// Calculate directory size with timeout protection
fn calculate_dir_size_with_timeout(dir: &Path, timeout: Duration) -> Result<u64> {
    if timeout.is_zero() {
        return Err(anyhow::anyhow!("Timeout calculating size for {:?}", dir));
    }

    let start = Instant::now();
    let mut total = 0u64;
    let mut checked_entries = 0usize;

    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        checked_entries += 1;
        if checked_entries % 256 == 0 && start.elapsed() >= timeout {
            return Err(anyhow::anyhow!("Timeout calculating size for {:?}", dir));
        }

        if entry.file_type().is_file() {
            total += entry.metadata()?.len();
        }
    }

    if start.elapsed() >= timeout {
        Err(anyhow::anyhow!("Timeout calculating size for {:?}", dir))
    } else {
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProjectInfo, ProjectType};
    use chrono::Utc;
    use std::fs;
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn test_size_calculator() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("test-dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("file1.txt"), "test content").unwrap();
        fs::write(dir.join("file2.txt"), "more test content").unwrap();

        let mut project = ProjectInfo::new_pending(
            dir.clone(),
            ProjectType::NodeJs,
            dir.clone(),
            Utc::now(),
            false,
        );

        let calculator = SizeCalculator::new();
        let size = calculator.calculate_single(&mut project).unwrap();

        assert!(size > 0);
        assert!(project.size_calculated);
        assert_eq!(project.size, size);
    }

    #[test]
    fn test_streaming_calculation() {
        let temp = TempDir::new().unwrap();
        let mut projects = vec![];

        // Create 3 test directories
        for i in 0..3 {
            let dir = temp.path().join(format!("dir{}", i));
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("file.txt"), "content").unwrap();

            projects.push(ProjectInfo::new_pending(
                dir.clone(),
                ProjectType::NodeJs,
                dir.clone(),
                Utc::now(),
                false,
            ));
        }

        let (tx, rx) = crossbeam::channel::unbounded();
        let calculator = SizeCalculator::new();

        // Start calculation in background
        let projects_clone = projects.clone();
        thread::spawn(move || {
            calculator.calculate_batch_streaming(projects_clone, tx);
        });

        // Collect results
        let mut results = vec![];
        while results.len() < 3 {
            if let Ok(project) = rx.recv_timeout(Duration::from_secs(5)) {
                results.push(project);
            } else {
                break;
            }
        }

        assert_eq!(results.len(), 3);
        for project in results {
            assert!(project.size_calculated);
            assert!(project.size > 0);
        }
    }

    #[test]
    fn test_calculate_single_timeout_zero() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("timeout-dir");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("file.txt"), "content").unwrap();

        let mut project = ProjectInfo::new_pending(
            dir.clone(),
            ProjectType::NodeJs,
            dir,
            Utc::now(),
            false,
        );

        let calculator = SizeCalculator::with_timeout(0);
        let result = calculator.calculate_single(&mut project);
        assert!(result.is_err());
    }

    #[test]
    fn test_streaming_timeout_still_sends_all_projects() {
        let temp = TempDir::new().unwrap();
        let mut projects = vec![];

        for i in 0..3 {
            let dir = temp.path().join(format!("timeout{}", i));
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("file.txt"), "content").unwrap();

            projects.push(ProjectInfo::new_pending(
                dir.clone(),
                ProjectType::NodeJs,
                dir,
                Utc::now(),
                false,
            ));
        }

        let expected_len = projects.len();
        let (tx, rx) = crossbeam::channel::unbounded();
        let calculator = SizeCalculator::with_timeout(0);
        let completed = calculator.calculate_batch_streaming(projects, tx);

        let results = rx.try_iter().collect::<Vec<_>>();
        assert_eq!(completed, 0);
        assert_eq!(results.len(), expected_len);
        for project in results {
            assert!(project.size_calculated);
            assert_eq!(project.size, 0);
        }
    }
}
