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
    let fallback_path = fallback_events_log_path();
    log_event_to_paths(&primary_path, &fallback_path, event, props)
}

fn log_event_to_paths(
    primary_path: &Path,
    fallback_path: &Path,
    event: &str,
    props: Value,
) -> Result<()> {
    match append_event_to_path(primary_path, event, props.clone()) {
        Ok(()) => Ok(()),
        Err(primary_err) => {
            if fallback_path == primary_path {
                return Err(primary_err);
            }

            append_event_to_path(fallback_path, event, props).with_context(|| {
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
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Mutex, OnceLock};
    use tempfile::TempDir;

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

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

    #[test]
    fn log_event_falls_back_when_primary_path_is_invalid() {
        let _guard = env_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let temp = TempDir::new().unwrap();
        let primary_base = temp.path().join("Library").join("Application Support");
        fs::create_dir_all(primary_base.parent().unwrap()).unwrap();
        fs::write(&primary_base, "not a directory").unwrap();

        let cwd = temp.path().join("work");
        fs::create_dir(&cwd).unwrap();

        let old_home = std::env::var_os("HOME");
        let old_dir = std::env::current_dir().unwrap();

        std::env::set_var("HOME", temp.path());
        std::env::set_current_dir(&cwd).unwrap();

        let result = log_event("share_generated", json!({ "bytes_freed": 123 }));

        std::env::set_current_dir(old_dir).unwrap();
        match old_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }

        assert!(result.is_ok());
        let fallback_path = cwd.join(FALLBACK_EVENTS_FILENAME);
        assert!(fallback_path.exists());
    }

    #[test]
    fn log_event_returns_primary_error_when_fallback_matches() {
        let temp = TempDir::new().unwrap();
        let primary = temp.path().join("events.jsonl");
        fs::write(&primary, "locked").unwrap();
        let mut perms = fs::metadata(&primary).unwrap().permissions();
        perms.set_mode(0o444);
        fs::set_permissions(&primary, perms).unwrap();

        let result = log_event_to_paths(
            &primary,
            &primary,
            "share_generated",
            json!({ "bytes_freed": 123 }),
        );
        assert!(result.is_err());
    }
}
