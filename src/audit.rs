use crate::config::Config;
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const DEFAULT_AUDIT_FILENAME: &str = "operations.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuditRecord {
    RunStarted {
        run_id: String,
        command: String,
        ts: String,
    },
    ItemAction {
        run_id: String,
        command: String,
        path: String,
        action: String,
        result: String,
        bytes: u64,
        reason: Option<String>,
        ts: String,
    },
    RunFinished {
        run_id: String,
        command: String,
        ts: String,
        cleaned: usize,
        skipped: usize,
        failed: usize,
        freed_bytes: u64,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditRunSummary {
    pub run_id: String,
    pub command: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub cleaned: usize,
    pub skipped: usize,
    pub failed: usize,
    pub freed_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct AuditLogger {
    path: PathBuf,
    enabled: bool,
    max_size_bytes: u64,
}

impl AuditLogger {
    pub fn from_config(config: &Config) -> Self {
        let path = config.audit.path.clone().unwrap_or_else(default_audit_path);
        let max_size_bytes = config.audit.max_size_mb.saturating_mul(1024 * 1024);
        Self {
            path,
            enabled: config.audit.enabled,
            max_size_bytes,
        }
    }

    pub fn new(path: PathBuf, enabled: bool, max_size_bytes: u64) -> Self {
        Self {
            path,
            enabled,
            max_size_bytes,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn start_run(&self, command: &str) -> Result<String> {
        let run_id = generate_run_id();
        self.append(&AuditRecord::RunStarted {
            run_id: run_id.clone(),
            command: command.to_string(),
            ts: Utc::now().to_rfc3339(),
        })?;
        Ok(run_id)
    }

    pub fn log_item(
        &self,
        run_id: &str,
        command: &str,
        path: &Path,
        action: &str,
        result: &str,
        bytes: u64,
        reason: Option<String>,
    ) -> Result<()> {
        self.append(&AuditRecord::ItemAction {
            run_id: run_id.to_string(),
            command: command.to_string(),
            path: path.display().to_string(),
            action: action.to_string(),
            result: result.to_string(),
            bytes,
            reason,
            ts: Utc::now().to_rfc3339(),
        })
    }

    pub fn finish_run(
        &self,
        run_id: &str,
        command: &str,
        cleaned: usize,
        skipped: usize,
        failed: usize,
        freed_bytes: u64,
    ) -> Result<()> {
        self.append(&AuditRecord::RunFinished {
            run_id: run_id.to_string(),
            command: command.to_string(),
            ts: Utc::now().to_rfc3339(),
            cleaned,
            skipped,
            failed,
            freed_bytes,
        })
    }

    pub fn append(&self, record: &AuditRecord) -> Result<()> {
        if !self.enabled {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create audit directory: {}", parent.display())
            })?;
        }

        self.rotate_if_needed()?;

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .with_context(|| format!("Failed to open audit log: {}", self.path.display()))?;

        serde_json::to_writer(&mut file, record)?;
        writeln!(file)?;
        Ok(())
    }

    pub fn read_records(&self) -> Result<Vec<AuditRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let content = fs::read_to_string(&self.path)
            .with_context(|| format!("Failed to read audit log: {}", self.path.display()))?;
        let mut out = Vec::new();
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<AuditRecord>(trimmed) {
                out.push(record);
            }
        }
        Ok(out)
    }

    pub fn records_for_run(&self, run_id: &str) -> Result<Vec<AuditRecord>> {
        Ok(self
            .read_records()?
            .into_iter()
            .filter(|r| match r {
                AuditRecord::RunStarted { run_id: id, .. }
                | AuditRecord::RunFinished { run_id: id, .. }
                | AuditRecord::ItemAction { run_id: id, .. } => id == run_id,
            })
            .collect())
    }

    pub fn list_runs(&self) -> Result<Vec<AuditRunSummary>> {
        let records = self.read_records()?;
        let mut runs = HashMap::<String, AuditRunSummary>::new();

        for record in records {
            match record {
                AuditRecord::RunStarted {
                    run_id,
                    command,
                    ts,
                } => {
                    let summary = runs.entry(run_id.clone()).or_insert(AuditRunSummary {
                        run_id,
                        command,
                        started_at: None,
                        finished_at: None,
                        cleaned: 0,
                        skipped: 0,
                        failed: 0,
                        freed_bytes: 0,
                    });
                    summary.started_at = Some(ts);
                }
                AuditRecord::RunFinished {
                    run_id,
                    command,
                    ts,
                    cleaned,
                    skipped,
                    failed,
                    freed_bytes,
                } => {
                    let summary = runs.entry(run_id.clone()).or_insert(AuditRunSummary {
                        run_id,
                        command,
                        started_at: None,
                        finished_at: None,
                        cleaned: 0,
                        skipped: 0,
                        failed: 0,
                        freed_bytes: 0,
                    });
                    summary.finished_at = Some(ts);
                    summary.cleaned = cleaned;
                    summary.skipped = skipped;
                    summary.failed = failed;
                    summary.freed_bytes = freed_bytes;
                }
                AuditRecord::ItemAction { .. } => {}
            }
        }

        let mut out = runs.into_values().collect::<Vec<_>>();
        out.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        Ok(out)
    }

    pub fn export_csv(records: &[AuditRecord]) -> String {
        let mut out = String::from("type,run_id,command,ts,path,action,result,bytes,reason,cleaned,skipped,failed,freed_bytes\n");
        for record in records {
            match record {
                AuditRecord::RunStarted {
                    run_id,
                    command,
                    ts,
                } => {
                    out.push_str(&format!(
                        "run_started,{},{},{},,,,,,,,\n",
                        csv_escape(run_id),
                        csv_escape(command),
                        csv_escape(ts)
                    ));
                }
                AuditRecord::ItemAction {
                    run_id,
                    command,
                    path,
                    action,
                    result,
                    bytes,
                    reason,
                    ts,
                } => {
                    out.push_str(&format!(
                        "item_action,{},{},{},{},{},{},{},{},,,,\n",
                        csv_escape(run_id),
                        csv_escape(command),
                        csv_escape(ts),
                        csv_escape(path),
                        csv_escape(action),
                        csv_escape(result),
                        bytes,
                        csv_escape(reason.as_deref().unwrap_or(""))
                    ));
                }
                AuditRecord::RunFinished {
                    run_id,
                    command,
                    ts,
                    cleaned,
                    skipped,
                    failed,
                    freed_bytes,
                } => {
                    out.push_str(&format!(
                        "run_finished,{},{},{},,,,,,,{},{},{},{}\n",
                        csv_escape(run_id),
                        csv_escape(command),
                        csv_escape(ts),
                        cleaned,
                        skipped,
                        failed,
                        freed_bytes
                    ));
                }
            }
        }
        out
    }

    fn rotate_if_needed(&self) -> Result<()> {
        if self.max_size_bytes == 0 || !self.path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&self.path)?;
        if metadata.len() <= self.max_size_bytes {
            return Ok(());
        }

        let rotated = self.path.with_extension("jsonl.old");
        let _ = fs::remove_file(&rotated);
        fs::rename(&self.path, &rotated).with_context(|| {
            format!(
                "Failed to rotate audit log: {} -> {}",
                self.path.display(),
                rotated.display()
            )
        })?;
        Ok(())
    }
}

pub fn default_audit_path() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dev-cleaner")
        .join(DEFAULT_AUDIT_FILENAME)
}

fn generate_run_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!(
        "{}-{}-{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        nanos,
        std::process::id()
    )
}

fn csv_escape(input: &str) -> String {
    if input.contains(',') || input.contains('"') || input.contains('\n') {
        format!("\"{}\"", input.replace('"', "\"\""))
    } else {
        input.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn audit_roundtrip_records() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("operations.jsonl");
        let logger = AuditLogger::new(path, true, 1024 * 1024);
        let run = logger.start_run("clean").unwrap();
        logger
            .log_item(
                &run,
                "clean",
                Path::new("/tmp/test"),
                "remove",
                "ok",
                42,
                None,
            )
            .unwrap();
        logger.finish_run(&run, "clean", 1, 0, 0, 42).unwrap();

        let runs = logger.list_runs().unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].run_id, run);
    }
}
