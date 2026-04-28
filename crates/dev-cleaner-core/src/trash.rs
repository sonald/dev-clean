use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

const TRASH_LOG_FILENAME: &str = "trash_log.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashEntry {
    pub batch_id: String,
    pub created_at: DateTime<Utc>,
    pub original_path: PathBuf,
    pub trashed_path: PathBuf,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_version: Option<String>,
}

pub struct TrashManager {
    pub batch_id: String,
    root: PathBuf,
    log_path: PathBuf,
}

impl TrashManager {
    pub fn new_default() -> Result<Self> {
        Self::new_with_root(default_trash_root())
    }

    pub fn new_with_root(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)
            .with_context(|| format!("Failed to create trash directory: {}", root.display()))?;

        // Ensure the batch id is unique even across multiple runs started in the same second.
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let batch_id = format!(
            "{}-{}-{}",
            Utc::now().format("%Y%m%d%H%M%S"),
            unique,
            std::process::id()
        );
        let batch_dir = root.join(&batch_id);
        fs::create_dir_all(&batch_dir).with_context(|| {
            format!(
                "Failed to create trash batch directory: {}",
                batch_dir.display()
            )
        })?;

        let log_path = root.join(TRASH_LOG_FILENAME);
        Ok(Self {
            batch_id,
            root,
            log_path,
        })
    }

    pub fn trash_dir(&self, original: &Path, size: u64) -> Result<TrashEntry> {
        let batch_dir = self.root.join(&self.batch_id);
        let rel = path_to_trash_relpath(original);
        let trashed_path = batch_dir.join(rel);

        if let Some(parent) = trashed_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create trash destination directory: {}",
                    parent.display()
                )
            })?;
        }

        move_path_with_exdev_fallback(original, &trashed_path).with_context(|| {
            format!(
                "Failed to move to trash: {} -> {}",
                original.display(),
                trashed_path.display()
            )
        })?;

        let entry = TrashEntry {
            batch_id: self.batch_id.clone(),
            created_at: Utc::now(),
            original_path: original.to_path_buf(),
            trashed_path: trashed_path.clone(),
            size,
            tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        };
        self.append_log(&entry)?;

        Ok(entry)
    }

    fn append_log(&self, entry: &TrashEntry) -> Result<()> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)
            .with_context(|| format!("Failed to open trash log: {}", self.log_path.display()))?;

        serde_json::to_writer(&mut file, entry)?;
        writeln!(&mut file)?;
        Ok(())
    }

    pub fn load_log(&self) -> Result<Vec<TrashEntry>> {
        load_trash_log(&self.log_path)
    }
}

pub fn default_trash_root() -> PathBuf {
    if let Ok(custom) = std::env::var("DEV_CLEANER_TRASH_DIR") {
        return PathBuf::from(custom);
    }

    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dev-cleaner")
        .join("trash")
}

pub fn load_trash_log(log_path: &Path) -> Result<Vec<TrashEntry>> {
    let content = match fs::read_to_string(log_path) {
        Ok(c) => c,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err).with_context(|| format!("Failed to read {}", log_path.display()))
        }
    };

    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<TrashEntry>(line) {
            Ok(entry) => entries.push(entry),
            Err(_) => continue,
        }
    }

    Ok(entries)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashBatchSummary {
    pub batch_id: String,
    pub created_at: DateTime<Utc>,
    pub entries_count: usize,
    pub total_size: u64,
}

pub fn list_trash_batches(root: &Path) -> Result<Vec<TrashBatchSummary>> {
    use std::collections::HashMap;

    let entries = load_trash_log(&root.join(TRASH_LOG_FILENAME))?;
    let mut batches: HashMap<String, TrashBatchSummary> = HashMap::new();

    for entry in entries {
        let summary = batches
            .entry(entry.batch_id.clone())
            .or_insert(TrashBatchSummary {
                batch_id: entry.batch_id.clone(),
                created_at: entry.created_at,
                entries_count: 0,
                total_size: 0,
            });

        summary.entries_count += 1;
        summary.total_size += entry.size;
        if entry.created_at < summary.created_at {
            summary.created_at = entry.created_at;
        }
    }

    let mut results = batches.into_values().collect::<Vec<_>>();
    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(results)
}

pub fn trash_entries_for_batch(root: &Path, batch_id: &str) -> Result<Vec<TrashEntry>> {
    let log_path = root.join(TRASH_LOG_FILENAME);
    let mut entries: Vec<TrashEntry> = load_trash_log(&log_path)?
        .into_iter()
        .filter(|e| e.batch_id == batch_id)
        .collect();
    entries.sort_by_key(|e| e.original_path.clone());
    Ok(entries)
}

pub fn latest_batch_id(root: &Path) -> Result<Option<String>> {
    let log_path = root.join(TRASH_LOG_FILENAME);
    let entries = load_trash_log(&log_path)?;
    let latest = entries
        .into_iter()
        .max_by_key(|e| e.created_at)
        .map(|e| e.batch_id);
    Ok(latest)
}

#[derive(Debug)]
pub struct RestoreResult {
    pub restored_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub errors: Vec<String>,
}

pub trait RestoreObserver {
    fn on_dry_run(&mut self, _entry: &TrashEntry) {}
    fn on_restored(&mut self, _entry: &TrashEntry) {}
}

#[derive(Debug, Default)]
pub struct NoopRestoreObserver;

impl RestoreObserver for NoopRestoreObserver {}

pub fn restore_batch(
    root: &Path,
    batch_id: &str,
    dry_run: bool,
    force: bool,
) -> Result<RestoreResult> {
    let mut observer = NoopRestoreObserver;
    restore_batch_with_observer(root, batch_id, dry_run, force, &mut observer)
}

pub fn restore_batch_with_observer<O: RestoreObserver>(
    root: &Path,
    batch_id: &str,
    dry_run: bool,
    force: bool,
    observer: &mut O,
) -> Result<RestoreResult> {
    let log_path = root.join(TRASH_LOG_FILENAME);
    let mut entries: Vec<TrashEntry> = load_trash_log(&log_path)?
        .into_iter()
        .filter(|e| e.batch_id == batch_id)
        .collect();

    // Restore deeper paths first just in case.
    entries.sort_by_key(|e| std::cmp::Reverse(e.original_path.components().count()));

    if entries.is_empty() {
        return Ok(RestoreResult {
            restored_count: 0,
            skipped_count: 0,
            failed_count: 0,
            errors: vec![format!("No entries found for batch_id `{}`", batch_id)],
        });
    }

    let mut restored_count = 0;
    let mut skipped_count = 0;
    let mut failed_count = 0;
    let mut errors = Vec::new();

    for entry in entries {
        if !entry.trashed_path.exists() {
            skipped_count += 1;
            continue;
        }

        if entry.original_path.exists() && !force {
            skipped_count += 1;
            errors.push(format!(
                "Restore target already exists (use --force to override): {}",
                entry.original_path.display()
            ));
            continue;
        }

        if dry_run {
            restored_count += 1;
            observer.on_dry_run(&entry);
            continue;
        }

        if entry.original_path.exists() && force {
            // If forced, remove the existing target first.
            if entry.original_path.is_dir() {
                fs::remove_dir_all(&entry.original_path).with_context(|| {
                    format!(
                        "Failed to remove existing dir: {}",
                        entry.original_path.display()
                    )
                })?;
            } else {
                fs::remove_file(&entry.original_path).with_context(|| {
                    format!(
                        "Failed to remove existing file: {}",
                        entry.original_path.display()
                    )
                })?;
            }
        }

        if let Some(parent) = entry.original_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create restore parent directory: {}",
                    parent.display()
                )
            })?;
        }

        match move_path_with_exdev_fallback(&entry.trashed_path, &entry.original_path) {
            Ok(_) => {
                restored_count += 1;
                observer.on_restored(&entry);
            }
            Err(err) => {
                failed_count += 1;
                errors.push(format!(
                    "Failed to restore {}: {}",
                    entry.original_path.display(),
                    err
                ));
            }
        }
    }

    Ok(RestoreResult {
        restored_count,
        skipped_count,
        failed_count,
        errors,
    })
}

#[derive(Debug)]
pub struct PurgeResult {
    pub removed_batches: usize,
    pub removed_entries: usize,
    pub removed_bytes: u64,
    pub failed_batches: usize,
    pub errors: Vec<String>,
}

pub fn purge_trash_batch(root: &Path, batch_id: &str, dry_run: bool) -> Result<PurgeResult> {
    let log_path = root.join(TRASH_LOG_FILENAME);
    let entries = load_trash_log(&log_path)?;

    let (kept, removed): (Vec<_>, Vec<_>) =
        entries.into_iter().partition(|e| e.batch_id != batch_id);
    let removed_entries = removed.len();
    let removed_bytes = removed.iter().map(|e| e.size).sum::<u64>();

    if dry_run {
        return Ok(PurgeResult {
            removed_batches: 1,
            removed_entries,
            removed_bytes,
            failed_batches: 0,
            errors: Vec::new(),
        });
    }

    let mut failed_batches = 0;
    let mut errors = Vec::new();

    let batch_dir = root.join(batch_id);
    if batch_dir.exists() {
        if is_symlink_path(&batch_dir)? {
            failed_batches += 1;
            errors.push(format!(
                "Refusing to purge symlink path: {}",
                batch_dir.display()
            ));
        } else if let Err(err) = fs::remove_dir_all(&batch_dir) {
            failed_batches += 1;
            errors.push(format!(
                "Failed to remove batch dir {}: {}",
                batch_dir.display(),
                err
            ));
        }
    }

    if failed_batches == 0 {
        save_trash_log(&log_path, &kept)?;
    }

    Ok(PurgeResult {
        removed_batches: if removed_entries > 0 { 1 } else { 0 },
        removed_entries,
        removed_bytes,
        failed_batches,
        errors,
    })
}

#[derive(Debug)]
pub struct GcResult {
    pub removed_batches: usize,
    pub removed_entries: usize,
    pub removed_bytes: u64,
    pub remaining_bytes: u64,
    pub target_keep_bytes: Option<u64>,
    pub blocked_by_keep_days: bool,
    pub failed_batches: usize,
    pub errors: Vec<String>,
}

pub fn gc_trash(
    root: &Path,
    keep_days: Option<i64>,
    keep_bytes: Option<u64>,
    dry_run: bool,
) -> Result<GcResult> {
    let now = Utc::now();
    let log_path = root.join(TRASH_LOG_FILENAME);
    let entries = load_trash_log(&log_path)?;

    let summaries = summarize_batches(entries.iter().cloned().collect());
    let total_bytes = summaries.iter().map(|s| s.total_size).sum::<u64>();

    let mut blocked_by_keep_days = false;
    let mut selected = Vec::new();

    // Always delete batches older than keep-days (if set).
    if let Some(days) = keep_days {
        selected.extend(
            summaries
                .iter()
                .filter(|s| (now - s.created_at).num_days() > days)
                .cloned(),
        );
    }

    let mut selected_ids = selected
        .iter()
        .map(|s| s.batch_id.clone())
        .collect::<std::collections::HashSet<_>>();
    let selected_bytes = selected.iter().map(|s| s.total_size).sum::<u64>();
    let mut bytes_after = total_bytes.saturating_sub(selected_bytes);

    // Enforce keep-bytes cap.
    if let Some(limit) = keep_bytes {
        if bytes_after > limit {
            if keep_days.is_some() {
                // Respect keep-days: we only delete older batches, even if this can't satisfy keep-gb.
                blocked_by_keep_days = true;
            } else {
                // No keep-days: delete oldest batches until within keep-gb.
                let mut candidates = summaries
                    .iter()
                    .filter(|s| !selected_ids.contains(&s.batch_id))
                    .cloned()
                    .collect::<Vec<_>>();
                candidates.sort_by(|a, b| a.created_at.cmp(&b.created_at)); // oldest first

                while bytes_after > limit {
                    let Some(next) = candidates.first().cloned() else {
                        break;
                    };
                    candidates.remove(0);
                    bytes_after = bytes_after.saturating_sub(next.total_size);
                    selected_ids.insert(next.batch_id.clone());
                    selected.push(next);
                }
            }
        }
    }

    if dry_run {
        let removed_batches = selected.len();
        let removed_entries = selected.iter().map(|s| s.entries_count).sum();
        let removed_bytes = selected.iter().map(|s| s.total_size).sum();
        return Ok(GcResult {
            removed_batches,
            removed_entries,
            removed_bytes,
            remaining_bytes: bytes_after,
            target_keep_bytes: keep_bytes,
            blocked_by_keep_days,
            failed_batches: 0,
            errors: Vec::new(),
        });
    }

    let mut failed_batches = 0;
    let mut errors = Vec::new();
    let mut removed_ok_ids = std::collections::HashSet::new();

    for summary in &selected {
        let batch_dir = root.join(&summary.batch_id);
        if batch_dir.exists() {
            if is_symlink_path(&batch_dir)? {
                failed_batches += 1;
                errors.push(format!(
                    "Refusing to purge symlink path: {}",
                    batch_dir.display()
                ));
                continue;
            }

            if let Err(err) = fs::remove_dir_all(&batch_dir) {
                failed_batches += 1;
                errors.push(format!(
                    "Failed to remove batch dir {}: {}",
                    batch_dir.display(),
                    err
                ));
                continue;
            }
        }
        removed_ok_ids.insert(summary.batch_id.clone());
    }

    let kept_entries = entries
        .into_iter()
        .filter(|e| !removed_ok_ids.contains(&e.batch_id))
        .collect::<Vec<_>>();

    if !removed_ok_ids.is_empty() {
        save_trash_log(&log_path, &kept_entries)?;
    }

    let removed_batches = removed_ok_ids.len();
    let removed_entries = selected
        .iter()
        .filter(|s| removed_ok_ids.contains(&s.batch_id))
        .map(|s| s.entries_count)
        .sum();
    let removed_bytes = selected
        .iter()
        .filter(|s| removed_ok_ids.contains(&s.batch_id))
        .map(|s| s.total_size)
        .sum();

    Ok(GcResult {
        removed_batches,
        removed_entries,
        removed_bytes,
        remaining_bytes: total_bytes.saturating_sub(removed_bytes),
        target_keep_bytes: keep_bytes,
        blocked_by_keep_days,
        failed_batches,
        errors,
    })
}

fn path_to_trash_relpath(path: &Path) -> PathBuf {
    let mut rel = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::Prefix(prefix) => {
                // Windows: "C:" etc
                rel.push(prefix.as_os_str().to_string_lossy().replace(':', ""));
            }
            Component::RootDir => {
                // Drop the root separator for portability inside trash.
            }
            Component::CurDir | Component::ParentDir | Component::Normal(_) => {
                rel.push(comp.as_os_str());
            }
        }
    }
    rel
}

fn summarize_batches(entries: Vec<TrashEntry>) -> Vec<TrashBatchSummary> {
    use std::collections::HashMap;

    let mut batches: HashMap<String, TrashBatchSummary> = HashMap::new();
    for entry in entries {
        let summary = batches
            .entry(entry.batch_id.clone())
            .or_insert(TrashBatchSummary {
                batch_id: entry.batch_id.clone(),
                created_at: entry.created_at,
                entries_count: 0,
                total_size: 0,
            });

        summary.entries_count += 1;
        summary.total_size += entry.size;
        if entry.created_at < summary.created_at {
            summary.created_at = entry.created_at;
        }
    }

    let mut results = batches.into_values().collect::<Vec<_>>();
    results.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    results
}

fn save_trash_log(log_path: &Path, entries: &[TrashEntry]) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create parent directory for trash log: {}",
                parent.display()
            )
        })?;
    }

    let tmp_path = log_path.with_extension("jsonl.tmp");
    let mut file = fs::File::create(&tmp_path)
        .with_context(|| format!("Failed to create {}", tmp_path.display()))?;
    for entry in entries {
        serde_json::to_writer(&mut file, entry)?;
        writeln!(&mut file)?;
    }
    file.sync_all()
        .with_context(|| format!("Failed to fsync {}", tmp_path.display()))?;

    fs::rename(&tmp_path, log_path).with_context(|| {
        format!(
            "Failed to replace trash log: {} -> {}",
            tmp_path.display(),
            log_path.display()
        )
    })?;

    Ok(())
}

fn is_symlink_path(path: &Path) -> Result<bool> {
    Ok(fs::symlink_metadata(path)
        .with_context(|| format!("Failed to stat {}", path.display()))?
        .file_type()
        .is_symlink())
}

fn move_path_with_exdev_fallback(src: &Path, dst: &Path) -> Result<()> {
    if is_symlink_path(src)? {
        anyhow::bail!("Refusing to move symlink path: {}", src.display());
    }

    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::CrossesDevices => {
            copy_dir_recursive(src, dst).with_context(|| {
                format!(
                    "Failed to copy across devices: {} -> {}",
                    src.display(),
                    dst.display()
                )
            })?;
            fs::remove_dir_all(src).with_context(|| {
                format!(
                    "Failed to remove source directory after copy: {}",
                    src.display()
                )
            })?;
            Ok(())
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "Failed to rename/move directory: {} -> {}",
                src.display(),
                dst.display()
            )
        }),
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        anyhow::bail!("Destination already exists: {}", dst.display());
    }
    fs::create_dir_all(dst).with_context(|| format!("Failed to create {}", dst.display()))?;

    for entry in walkdir::WalkDir::new(src).follow_links(false).into_iter() {
        let entry =
            entry.with_context(|| format!("Failed to read dir entry under {}", src.display()))?;
        let rel = entry.path().strip_prefix(src).with_context(|| {
            format!(
                "Failed to compute relative path for {}",
                entry.path().display()
            )
        })?;
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest_path = dst.join(rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest_path)
                .with_context(|| format!("Failed to create directory {}", dest_path.display()))?;
            continue;
        }

        if entry.file_type().is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create parent directory {}", parent.display())
                })?;
            }
            fs::copy(entry.path(), &dest_path).with_context(|| {
                format!(
                    "Failed to copy file {} -> {}",
                    entry.path().display(),
                    dest_path.display()
                )
            })?;
            continue;
        }

        if entry.file_type().is_symlink() {
            copy_symlink(entry.path(), &dest_path)?;
            continue;
        }
    }

    Ok(())
}

#[cfg(unix)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::fs::symlink;

    let target =
        fs::read_link(src).with_context(|| format!("Failed to readlink {}", src.display()))?;
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    symlink(&target, dst).with_context(|| {
        format!(
            "Failed to create symlink {} -> {}",
            dst.display(),
            target.display()
        )
    })?;
    Ok(())
}

#[cfg(windows)]
fn copy_symlink(src: &Path, dst: &Path) -> Result<()> {
    anyhow::bail!(
        "Symlink copy is not supported on this platform: {} -> {}",
        src.display(),
        dst.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::io::Write;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_entry(
        batch_id: &str,
        created_at: chrono::DateTime<Utc>,
        original_path: PathBuf,
        trashed_path: PathBuf,
        size: u64,
    ) -> TrashEntry {
        TrashEntry {
            batch_id: batch_id.to_string(),
            created_at,
            original_path,
            trashed_path,
            size,
            tool_version: Some("test".to_string()),
        }
    }

    fn write_log_lines(log_path: &Path, lines: &[String]) {
        if let Some(parent) = log_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut file = fs::File::create(log_path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn write_entries(log_path: &Path, entries: &[TrashEntry]) {
        let lines = entries
            .iter()
            .map(|entry| serde_json::to_string(entry).unwrap())
            .collect::<Vec<_>>();
        write_log_lines(log_path, &lines);
    }

    fn create_dir_with_file(path: &Path) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join("payload.txt"), "payload").unwrap();
    }

    #[test]
    fn test_trash_and_restore_roundtrip() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let manager = TrashManager::new_with_root(trash_root.clone()).unwrap();

        let src_root = temp.path().join("src");
        fs::create_dir_all(&src_root).unwrap();
        let dir = src_root.join("to-delete");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("x"), "y").unwrap();

        let original = dir.clone();
        manager.trash_dir(&original, 1).unwrap();
        assert!(!original.exists());

        let result = restore_batch(&trash_root, &manager.batch_id, false, false).unwrap();
        assert_eq!(result.restored_count, 1);
        assert!(original.exists());
    }

    #[test]
    fn test_list_and_purge_trash_batch() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let manager = TrashManager::new_with_root(trash_root.clone()).unwrap();

        let src_root = temp.path().join("src");
        fs::create_dir_all(&src_root).unwrap();

        let dir1 = src_root.join("a");
        fs::create_dir_all(&dir1).unwrap();
        fs::write(dir1.join("x"), "y").unwrap();
        manager.trash_dir(&dir1, 10).unwrap();

        let dir2 = src_root.join("b");
        fs::create_dir_all(&dir2).unwrap();
        fs::write(dir2.join("x"), "y").unwrap();
        manager.trash_dir(&dir2, 20).unwrap();

        let batches = list_trash_batches(&trash_root).unwrap();
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].batch_id, manager.batch_id);
        assert_eq!(batches[0].entries_count, 2);
        assert_eq!(batches[0].total_size, 30);

        let entries = trash_entries_for_batch(&trash_root, &manager.batch_id).unwrap();
        assert_eq!(entries.len(), 2);

        let purge = purge_trash_batch(&trash_root, &manager.batch_id, false).unwrap();
        assert_eq!(purge.removed_entries, 2);
        assert_eq!(purge.removed_bytes, 30);
        assert!(trash_root.join(&manager.batch_id).exists() == false);

        let batches_after = list_trash_batches(&trash_root).unwrap();
        assert!(batches_after.is_empty());
    }

    #[test]
    fn test_load_trash_log_skips_invalid_lines() {
        let temp = TempDir::new().unwrap();
        let log_path = temp.path().join(TRASH_LOG_FILENAME);
        let valid_one = make_entry(
            "batch-a",
            Utc::now() - Duration::minutes(10),
            PathBuf::from("/tmp/a"),
            PathBuf::from("/trash/a"),
            5,
        );
        let valid_two = make_entry(
            "batch-b",
            Utc::now(),
            PathBuf::from("/tmp/b"),
            PathBuf::from("/trash/b"),
            6,
        );
        write_log_lines(
            &log_path,
            &[
                serde_json::to_string(&valid_one).unwrap(),
                String::new(),
                "{not-json".to_string(),
                serde_json::to_string(&valid_two).unwrap(),
            ],
        );

        let entries = load_trash_log(&log_path).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].batch_id, "batch-a");
        assert_eq!(entries[1].batch_id, "batch-b");
    }

    #[test]
    fn test_latest_batch_id_picks_newest_entry() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("trash");
        let log_path = root.join(TRASH_LOG_FILENAME);
        let entries = vec![
            make_entry(
                "batch-old",
                Utc::now() - Duration::minutes(5),
                PathBuf::from("/tmp/a"),
                PathBuf::from("/trash/a"),
                5,
            ),
            make_entry(
                "batch-new",
                Utc::now(),
                PathBuf::from("/tmp/b"),
                PathBuf::from("/trash/b"),
                6,
            ),
        ];
        write_entries(&log_path, &entries);

        let latest = latest_batch_id(&root).unwrap();
        assert_eq!(latest.as_deref(), Some("batch-new"));
    }

    #[test]
    fn test_path_to_trash_relpath_strips_root_separator() {
        let rel = path_to_trash_relpath(Path::new("/var/tmp/project/target"));
        assert_eq!(rel, PathBuf::from("var/tmp/project/target"));
    }

    #[cfg(unix)]
    #[test]
    fn test_move_path_with_exdev_fallback_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let src = temp.path().join("src");
        let dst = temp.path().join("dst");
        fs::create_dir_all(&src).unwrap();
        symlink(&src, temp.path().join("src-link")).unwrap();

        let err = move_path_with_exdev_fallback(&temp.path().join("src-link"), &dst)
            .expect_err("symlink sources must be rejected");
        assert!(err.to_string().contains("Refusing to move symlink path"));
    }

    #[test]
    fn test_restore_batch_reports_missing_batch() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");

        let result = restore_batch(&trash_root, "missing", false, false).unwrap();
        assert_eq!(result.restored_count, 0);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.failed_count, 0);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_restore_batch_skips_missing_trashed_path() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entry = make_entry(
            "batch-1",
            Utc::now(),
            temp.path().join("restore").join("target"),
            trash_root.join("batch-1").join("restore").join("target"),
            1,
        );
        write_entries(&log_path, &[entry]);

        let result = restore_batch(&trash_root, "batch-1", false, false).unwrap();
        assert_eq!(result.restored_count, 0);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_restore_batch_skips_existing_destination_without_force() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let original = temp.path().join("restore").join("target");
        let trashed = trash_root.join("batch-1").join("restore").join("target");
        create_dir_with_file(&trashed);
        fs::create_dir_all(original.parent().unwrap()).unwrap();
        fs::write(&original, "existing").unwrap();

        write_entries(
            &log_path,
            &[make_entry(
                "batch-1",
                Utc::now(),
                original.clone(),
                trashed.clone(),
                1,
            )],
        );

        let result = restore_batch(&trash_root, "batch-1", false, false).unwrap();
        assert_eq!(result.restored_count, 0);
        assert_eq!(result.skipped_count, 1);
        assert_eq!(result.failed_count, 0);
        assert!(result.errors[0].contains("Restore target already exists"));
        assert!(original.exists());
        assert!(trashed.exists());
    }

    #[test]
    fn test_restore_batch_force_overwrites_directory_and_file_targets() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let dir_original = temp.path().join("restore").join("dir-target");
        let file_original = temp.path().join("restore").join("file-target");
        let dir_trashed = trash_root.join("batch-1").join("dir-target");
        let file_trashed = trash_root.join("batch-1").join("file-target");

        create_dir_with_file(&dir_trashed);
        create_dir_with_file(&file_trashed);
        fs::create_dir_all(&dir_original).unwrap();
        fs::write(dir_original.join("old.txt"), "old").unwrap();
        fs::create_dir_all(file_original.parent().unwrap()).unwrap();
        fs::write(&file_original, "old").unwrap();

        write_entries(
            &log_path,
            &[
                make_entry(
                    "batch-1",
                    Utc::now(),
                    dir_original.clone(),
                    dir_trashed.clone(),
                    1,
                ),
                make_entry(
                    "batch-1",
                    Utc::now(),
                    file_original.clone(),
                    file_trashed.clone(),
                    1,
                ),
            ],
        );

        let result = restore_batch(&trash_root, "batch-1", false, true).unwrap();
        assert_eq!(result.restored_count, 2);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.failed_count, 0);
        assert!(!dir_trashed.exists());
        assert!(!file_trashed.exists());
        assert!(dir_original.join("payload.txt").exists());
        assert!(file_original.join("payload.txt").exists());
    }

    #[test]
    fn test_restore_batch_dry_run_counts_restores_without_mutating() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let original = temp.path().join("restore").join("target");
        let trashed = trash_root.join("batch-1").join("restore").join("target");
        create_dir_with_file(&trashed);

        write_entries(
            &log_path,
            &[make_entry(
                "batch-1",
                Utc::now(),
                original.clone(),
                trashed.clone(),
                1,
            )],
        );

        let result = restore_batch(&trash_root, "batch-1", true, false).unwrap();
        assert_eq!(result.restored_count, 1);
        assert_eq!(result.skipped_count, 0);
        assert_eq!(result.failed_count, 0);
        assert!(!original.exists());
        assert!(trashed.exists());
    }

    #[test]
    fn test_restore_batch_restores_deeper_paths_first() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let parent_original = temp.path().join("restore").join("project");
        let child_original = parent_original.join("cache");
        let parent_trashed = trash_root.join("batch-1").join("project");
        let child_trashed = trash_root.join("batch-1").join("project").join("cache");

        create_dir_with_file(&parent_trashed);
        create_dir_with_file(&child_trashed);

        write_entries(
            &log_path,
            &[
                make_entry(
                    "batch-1",
                    Utc::now(),
                    parent_original.clone(),
                    parent_trashed.clone(),
                    1,
                ),
                make_entry(
                    "batch-1",
                    Utc::now(),
                    child_original.clone(),
                    child_trashed.clone(),
                    1,
                ),
            ],
        );

        let result = restore_batch(&trash_root, "batch-1", false, false).unwrap();
        assert_eq!(result.restored_count, 1);
        assert_eq!(result.skipped_count, 1);
        assert!(child_original.exists());
        assert!(parent_trashed.exists());
        assert!(!child_trashed.exists());
    }

    #[test]
    fn test_purge_trash_batch_dry_run_keeps_log_and_files() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let batch_dir = trash_root.join("batch-1");
        let other_dir = trash_root.join("batch-2");
        fs::create_dir_all(&batch_dir).unwrap();
        fs::create_dir_all(&other_dir).unwrap();

        let entries = vec![
            make_entry(
                "batch-1",
                Utc::now(),
                PathBuf::from("/tmp/a"),
                batch_dir.join("a"),
                5,
            ),
            make_entry(
                "batch-2",
                Utc::now(),
                PathBuf::from("/tmp/b"),
                other_dir.join("b"),
                6,
            ),
        ];
        write_entries(&log_path, &entries);

        let result = purge_trash_batch(&trash_root, "batch-1", true).unwrap();
        assert_eq!(result.removed_batches, 1);
        assert_eq!(result.removed_entries, 1);
        assert_eq!(result.removed_bytes, 5);
        assert!(batch_dir.exists());
        assert_eq!(load_trash_log(&log_path).unwrap().len(), 2);
    }

    #[test]
    fn test_purge_trash_batch_unknown_batch_is_a_noop() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let batch_dir = trash_root.join("batch-1");
        fs::create_dir_all(&batch_dir).unwrap();

        let entries = vec![make_entry(
            "batch-1",
            Utc::now(),
            PathBuf::from("/tmp/a"),
            batch_dir.join("a"),
            5,
        )];
        write_entries(&log_path, &entries);

        let result = purge_trash_batch(&trash_root, "missing", false).unwrap();
        assert_eq!(result.removed_batches, 0);
        assert_eq!(result.removed_entries, 0);
        assert_eq!(result.removed_bytes, 0);
        assert_eq!(result.failed_batches, 0);
        assert!(batch_dir.exists());
        assert_eq!(load_trash_log(&log_path).unwrap().len(), 1);
    }

    #[test]
    fn test_purge_trash_batch_records_removal_failure() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        fs::create_dir_all(&trash_root).unwrap();
        let batch_path = trash_root.join("batch-1");
        fs::write(&batch_path, "not a directory").unwrap();

        let entries = vec![make_entry(
            "batch-1",
            Utc::now(),
            PathBuf::from("/tmp/a"),
            batch_path.join("a"),
            5,
        )];
        write_entries(&log_path, &entries);

        let result = purge_trash_batch(&trash_root, "batch-1", false).unwrap();
        assert_eq!(result.failed_batches, 1);
        assert!(result.errors[0].contains("Failed to remove batch dir"));
        assert_eq!(load_trash_log(&log_path).unwrap().len(), 1);
    }

    #[cfg(unix)]
    #[test]
    fn test_purge_trash_batch_rejects_symlink_paths() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let batch_dir = trash_root.join("batch-1");
        let target_dir = temp.path().join("real-batch");
        fs::create_dir_all(&target_dir).unwrap();
        fs::create_dir_all(&trash_root).unwrap();
        symlink(&target_dir, &batch_dir).unwrap();

        let entries = vec![
            make_entry(
                "batch-1",
                Utc::now(),
                PathBuf::from("/tmp/a"),
                batch_dir.join("a"),
                5,
            ),
            make_entry(
                "batch-2",
                Utc::now(),
                PathBuf::from("/tmp/b"),
                trash_root.join("batch-2").join("b"),
                6,
            ),
        ];
        write_entries(&log_path, &entries);

        let result = purge_trash_batch(&trash_root, "batch-1", false).unwrap();
        assert_eq!(result.failed_batches, 1);
        assert!(result.errors[0].contains("Refusing to purge symlink path"));
        assert!(batch_dir.exists());
        assert_eq!(load_trash_log(&log_path).unwrap().len(), 2);
    }

    #[test]
    fn test_gc_trash_removes_old_batches_by_keep_days() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let old_dir = trash_root.join("old-batch");
        let recent_dir = trash_root.join("recent-batch");
        fs::create_dir_all(&old_dir).unwrap();
        fs::create_dir_all(&recent_dir).unwrap();

        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entries = vec![
            make_entry(
                "old-batch",
                Utc::now() - Duration::days(10),
                PathBuf::from("/tmp/a"),
                old_dir.join("a"),
                5,
            ),
            make_entry(
                "recent-batch",
                Utc::now() - Duration::hours(1),
                PathBuf::from("/tmp/b"),
                recent_dir.join("b"),
                6,
            ),
        ];
        write_entries(&log_path, &entries);

        let result = gc_trash(&trash_root, Some(3), None, false).unwrap();
        assert_eq!(result.removed_batches, 1);
        assert_eq!(result.removed_entries, 1);
        assert_eq!(result.removed_bytes, 5);
        assert!(!old_dir.exists());
        assert!(recent_dir.exists());
        let remaining = load_trash_log(&log_path).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].batch_id, "recent-batch");
    }

    #[test]
    fn test_gc_trash_removes_oldest_batches_until_limit() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let batch_old = trash_root.join("batch-old");
        let batch_mid = trash_root.join("batch-mid");
        let batch_new = trash_root.join("batch-new");
        fs::create_dir_all(&batch_old).unwrap();
        fs::create_dir_all(&batch_mid).unwrap();
        fs::create_dir_all(&batch_new).unwrap();

        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entries = vec![
            make_entry(
                "batch-old",
                Utc::now() - Duration::days(3),
                PathBuf::from("/tmp/a"),
                batch_old.join("a"),
                7,
            ),
            make_entry(
                "batch-mid",
                Utc::now() - Duration::days(2),
                PathBuf::from("/tmp/b"),
                batch_mid.join("b"),
                6,
            ),
            make_entry(
                "batch-new",
                Utc::now() - Duration::days(1),
                PathBuf::from("/tmp/c"),
                batch_new.join("c"),
                4,
            ),
        ];
        write_entries(&log_path, &entries);

        let result = gc_trash(&trash_root, None, Some(9), false).unwrap();
        assert_eq!(result.removed_batches, 2);
        assert_eq!(result.removed_entries, 2);
        assert_eq!(result.removed_bytes, 13);
        assert!(!batch_old.exists());
        assert!(!batch_mid.exists());
        assert!(batch_new.exists());
        let remaining = load_trash_log(&log_path).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].batch_id, "batch-new");
    }

    #[test]
    fn test_gc_trash_blocks_keep_bytes_when_keep_days_is_set() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let old_dir = trash_root.join("old-batch");
        let recent_dir = trash_root.join("recent-batch");
        fs::create_dir_all(&old_dir).unwrap();
        fs::create_dir_all(&recent_dir).unwrap();

        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entries = vec![
            make_entry(
                "old-batch",
                Utc::now() - Duration::days(10),
                PathBuf::from("/tmp/a"),
                old_dir.join("a"),
                5,
            ),
            make_entry(
                "recent-batch",
                Utc::now(),
                PathBuf::from("/tmp/b"),
                recent_dir.join("b"),
                100,
            ),
        ];
        write_entries(&log_path, &entries);

        let result = gc_trash(&trash_root, Some(3), Some(1), true).unwrap();
        assert_eq!(result.removed_batches, 1);
        assert!(result.blocked_by_keep_days);
        assert_eq!(result.remaining_bytes, 100);
        assert!(old_dir.exists());
        assert!(recent_dir.exists());
    }

    #[cfg(unix)]
    #[test]
    fn test_gc_trash_rejects_symlink_batch_dirs() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let real_dir = temp.path().join("real-batch");
        let batch_dir = trash_root.join("batch-old");
        fs::create_dir_all(&real_dir).unwrap();
        fs::create_dir_all(&trash_root).unwrap();
        symlink(&real_dir, &batch_dir).unwrap();

        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entries = vec![make_entry(
            "batch-old",
            Utc::now() - Duration::days(10),
            PathBuf::from("/tmp/a"),
            batch_dir.join("a"),
            5,
        )];
        write_entries(&log_path, &entries);

        let result = gc_trash(&trash_root, Some(3), None, false).unwrap();
        assert_eq!(result.failed_batches, 1);
        assert!(result.errors[0].contains("Refusing to purge symlink path"));
        assert!(batch_dir.exists());
        assert_eq!(load_trash_log(&log_path).unwrap().len(), 1);
    }

    #[test]
    fn test_gc_trash_preserves_log_when_batch_removal_fails() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        fs::create_dir_all(&trash_root).unwrap();
        let batch_path = trash_root.join("batch-old");
        fs::write(&batch_path, "not a directory").unwrap();

        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entries = vec![make_entry(
            "batch-old",
            Utc::now() - Duration::days(10),
            PathBuf::from("/tmp/a"),
            batch_path.join("a"),
            5,
        )];
        write_entries(&log_path, &entries);

        let result = gc_trash(&trash_root, Some(3), None, false).unwrap();
        assert_eq!(result.failed_batches, 1);
        assert_eq!(result.removed_batches, 0);
        assert!(result.errors[0].contains("Failed to remove batch dir"));
        assert_eq!(load_trash_log(&log_path).unwrap().len(), 1);
    }

    #[test]
    fn test_gc_trash_dry_run_reports_keep_days_and_keep_bytes() {
        let temp = TempDir::new().unwrap();
        let trash_root = temp.path().join("trash");
        let old_dir = trash_root.join("old-batch");
        let recent_dir = trash_root.join("recent-batch");
        fs::create_dir_all(&old_dir).unwrap();
        fs::create_dir_all(&recent_dir).unwrap();

        let log_path = trash_root.join(TRASH_LOG_FILENAME);
        let entries = vec![
            make_entry(
                "old-batch",
                Utc::now() - Duration::days(10),
                PathBuf::from("/tmp/a"),
                old_dir.join("a"),
                5,
            ),
            make_entry(
                "recent-batch",
                Utc::now(),
                PathBuf::from("/tmp/b"),
                recent_dir.join("b"),
                100,
            ),
        ];
        write_entries(&log_path, &entries);

        let result = gc_trash(&trash_root, Some(3), Some(1), true).unwrap();
        assert_eq!(result.removed_batches, 1);
        assert!(result.blocked_by_keep_days);
        assert_eq!(result.remaining_bytes, 100);
        assert!(old_dir.exists());
        assert!(recent_dir.exists());
    }
}
