mod walker;
mod detector;

pub use walker::Scanner;
pub use detector::{ProjectType, ProjectDetector};

use std::path::PathBuf;
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

/// Information about a cleanable project directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Root directory of the project
    pub root: PathBuf,

    /// Type of the project (Node, Rust, Python, etc.)
    pub project_type: ProjectType,

    /// Cleanable directory path (e.g., node_modules, target)
    pub cleanable_dir: PathBuf,

    /// Size of the cleanable directory in bytes
    pub size: u64,

    /// Last modified time of the cleanable directory
    pub last_modified: DateTime<Utc>,

    /// Whether this directory is currently in use (based on lock files)
    pub in_use: bool,
}

impl ProjectInfo {
    /// Returns a human-readable size string
    pub fn size_human(&self) -> String {
        format_size(self.size)
    }

    /// Returns how many days since last modification
    pub fn days_since_modified(&self) -> i64 {
        let now = Utc::now();
        (now - self.last_modified).num_days()
    }
}

/// Format bytes into human-readable size
fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(1048576), "1.00 MB");
        assert_eq!(format_size(1073741824), "1.00 GB");
    }
}
