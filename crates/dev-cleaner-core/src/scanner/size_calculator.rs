use super::emit_perf_trace;
use crate::ProjectInfo;
use anyhow::Result;
use crossbeam::channel::Sender;
use rayon::prelude::*;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
struct SizeCalculationTrace {
    mode: &'static str,
    projects: usize,
    completed: usize,
    elapsed_ms: u128,
    timeout_secs: u64,
}

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
        projects: Vec<ProjectInfo>,
        tx: Sender<ProjectInfo>,
    ) -> usize {
        let total_projects = projects.len();
        let started = Instant::now();
        let timeout = Duration::from_secs(self.timeout_secs);
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        // Stop sizing once the receiver is gone (e.g. the UI cancelled the scan).
        let _ = projects.into_par_iter().try_for_each(|mut project| {
            if calculate_project_size(&mut project, timeout) {
                completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            tx.send(project).map_err(drop)
        });

        let completed = completed.load(std::sync::atomic::Ordering::Relaxed);
        emit_perf_trace(
            "size_calculation",
            &SizeCalculationTrace {
                mode: "streaming",
                projects: total_projects,
                completed,
                elapsed_ms: started.elapsed().as_millis(),
                timeout_secs: self.timeout_secs,
            },
        );
        completed
    }

    /// Calculate sizes for projects in parallel and return the updated list.
    pub fn calculate_batch(&self, mut projects: Vec<ProjectInfo>) -> Vec<ProjectInfo> {
        let total_projects = projects.len();
        let started = Instant::now();
        let timeout = Duration::from_secs(self.timeout_secs);
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        projects.par_iter_mut().for_each(|project| {
            if calculate_project_size(project, timeout) {
                completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });

        emit_perf_trace(
            "size_calculation",
            &SizeCalculationTrace {
                mode: "batch",
                projects: total_projects,
                completed: completed.load(std::sync::atomic::Ordering::Relaxed),
                elapsed_ms: started.elapsed().as_millis(),
                timeout_secs: self.timeout_secs,
            },
        );

        projects
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

fn calculate_project_size(project: &mut ProjectInfo, timeout: Duration) -> bool {
    match calculate_dir_size_with_timeout(&project.cleanable_dir, timeout) {
        Ok(size) => {
            project.size = size;
            project.size_calculated = true;
            true
        }
        Err(_) => {
            // Preserve the "unknown size" state so batch callers do not mistake failures for 0 B.
            project.size = 0;
            project.size_calculated = false;
            false
        }
    }
}

/// Calculate directory size with timeout protection
fn calculate_dir_size_with_timeout(dir: &Path, timeout: Duration) -> Result<u64> {
    if timeout.is_zero() {
        return Err(anyhow::anyhow!("Timeout calculating size for {:?}", dir));
    }
    reclaimable_size(dir, Some(timeout))
}

/// Bytes that deleting `dir` would release.
///
/// Counts allocated blocks rather than apparent length, and counts a hard-linked file
/// only when every one of its links is inside `dir`: pnpm and uv link packages from a
/// shared store, and deleting those links frees nothing.
// ponytail: APFS clones (pnpm/uv `clone` mode) still count in full. Upgrade path, verified
// on APFS: getattrlist(ATTR_CMNEXT_PRIVATESIZE, FSOPT_ATTR_CMN_EXTENDED) returns 0 for a clone.
pub(crate) fn reclaimable_size(dir: &Path, timeout: Option<Duration>) -> Result<u64> {
    let start = Instant::now();
    let mut total = 0u64;
    // (device, inode) -> (bytes, link count, links seen inside `dir`)
    let mut linked: HashMap<(u64, u64), (u64, u64, u64)> = HashMap::new();

    for (checked, entry) in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .enumerate()
    {
        if let Some(timeout) = timeout {
            if checked % 256 == 0 && start.elapsed() >= timeout {
                return Err(anyhow::anyhow!("Timeout calculating size for {:?}", dir));
            }
        }
        if !entry.file_type().is_file() {
            continue;
        }
        // A file deleted mid-walk (a running build) must not void the whole size.
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let bytes = allocated_bytes(&metadata);
        match hard_link(&metadata) {
            Some((key, links)) => linked.entry(key).or_insert((bytes, links, 0)).2 += 1,
            None => total = total.saturating_add(bytes),
        }
    }

    let fully_contained = linked
        .values()
        .filter(|(_, links, seen)| seen >= links)
        .map(|(bytes, _, _)| *bytes)
        .sum::<u64>();
    // A walk that finished is a valid size even if it ran past the timeout.
    Ok(total.saturating_add(fully_contained))
}

#[cfg(unix)]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &std::fs::Metadata) -> u64 {
    metadata.len()
}

#[cfg(unix)]
fn hard_link(metadata: &std::fs::Metadata) -> Option<((u64, u64), u64)> {
    use std::os::unix::fs::MetadataExt;
    (metadata.nlink() > 1).then(|| ((metadata.dev(), metadata.ino()), metadata.nlink()))
}

#[cfg(not(unix))]
fn hard_link(_metadata: &std::fs::Metadata) -> Option<((u64, u64), u64)> {
    None
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
    #[cfg(unix)]
    fn reclaimable_size_counts_blocks_and_only_fully_contained_hard_links() {
        use std::os::unix::fs::MetadataExt;
        let temp = TempDir::new().unwrap();
        let target = temp.path().join("node_modules");
        let store = temp.path().join("store");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&store).unwrap();

        fs::write(target.join("own.bin"), vec![1u8; 10_000]).unwrap();
        fs::hard_link(target.join("own.bin"), target.join("own-link.bin")).unwrap();
        fs::write(store.join("shared.bin"), vec![2u8; 50_000]).unwrap();
        fs::hard_link(store.join("shared.bin"), target.join("shared.bin")).unwrap();

        let own_blocks = fs::metadata(target.join("own.bin")).unwrap().blocks() * 512;
        assert!(own_blocks >= 10_000);
        // Two links to one inode count once; the store-shared file is not reclaimable.
        assert_eq!(reclaimable_size(&target, None).unwrap(), own_blocks);
    }

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
    fn test_batch_calculation() {
        let temp = TempDir::new().unwrap();
        let mut projects = vec![];

        for i in 0..3 {
            let dir = temp.path().join(format!("batch{}", i));
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

        let calculator = SizeCalculator::new();
        let results = calculator.calculate_batch(projects);

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

        let mut project =
            ProjectInfo::new_pending(dir.clone(), ProjectType::NodeJs, dir, Utc::now(), false);

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
            assert!(!project.size_calculated);
            assert_eq!(project.size, 0);
        }
    }
}
