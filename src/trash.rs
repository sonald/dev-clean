use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};

const TRASH_LOG_FILENAME: &str = "trash_log.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrashEntry {
    pub batch_id: String,
    pub created_at: DateTime<Utc>,
    pub original_path: PathBuf,
    pub trashed_path: PathBuf,
    pub size: u64,
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

        let batch_id = format!(
            "{}-{}",
            Utc::now().format("%Y%m%d%H%M%S"),
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

        fs::rename(original, &trashed_path).with_context(|| {
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

pub fn restore_batch(
    root: &Path,
    batch_id: &str,
    dry_run: bool,
    force: bool,
    verbose: bool,
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
            if verbose {
                println!(
                    "[DRY RUN] Would restore: {} -> {}",
                    entry.trashed_path.display(),
                    entry.original_path.display()
                );
            }
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

        match fs::rename(&entry.trashed_path, &entry.original_path) {
            Ok(_) => {
                restored_count += 1;
                if verbose {
                    println!("✓ Restored {}", entry.original_path.display());
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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

        let result = restore_batch(&trash_root, &manager.batch_id, false, false, false).unwrap();
        assert_eq!(result.restored_count, 1);
        assert!(original.exists());
    }
}
