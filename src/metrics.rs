use anyhow::{Context, Result};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const EVENTS_FILENAME: &str = "events.jsonl";
const FALLBACK_EVENTS_FILENAME: &str = ".dev-cleaner-events.jsonl";

#[derive(Debug, Serialize)]
struct EventRecord<'a> {
    ts: String,
    event: &'a str,
    props: Value,
}

pub fn log_event(event: &str, props: Value) -> Result<()> {
    let primary_path = events_log_path();
    match append_event_to_path(&primary_path, event, props.clone()) {
        Ok(()) => Ok(()),
        Err(primary_err) => {
            let fallback_path = fallback_events_log_path();
            if fallback_path == primary_path {
                return Err(primary_err);
            }

            append_event_to_path(&fallback_path, event, props).with_context(|| {
                format!(
                    "Failed to log metrics to primary path {}: {}",
                    primary_path.display(),
                    primary_err
                )
            })
        }
    }
}

fn events_log_path() -> PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("dev-cleaner").join(EVENTS_FILENAME);
    }

    fallback_events_log_path()
}

fn fallback_events_log_path() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(FALLBACK_EVENTS_FILENAME)
}

fn append_event_to_path(path: &Path, event: &str, props: Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create metrics directory: {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("Failed to open metrics log: {}", path.display()))?;

    let record = EventRecord {
        ts: Utc::now().to_rfc3339(),
        event,
        props,
    };
    serde_json::to_writer(&mut file, &record)?;
    writeln!(&mut file)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn append_event_writes_valid_json_line() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("nested").join("events.jsonl");

        append_event_to_path(&path, "share_generated", json!({ "bytes_freed": 123 })).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let line = content.lines().next().unwrap();
        let parsed: serde_json::Value = serde_json::from_str(line).unwrap();

        assert_eq!(parsed["event"], "share_generated");
        assert_eq!(parsed["props"]["bytes_freed"], 123);
        assert!(parsed["ts"].as_str().is_some());
    }
}
