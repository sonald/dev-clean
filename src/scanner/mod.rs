mod detector;
mod size_calculator;
mod walker;

pub use detector::{ProjectDetector, ProjectType};
pub use size_calculator::SizeCalculator;
pub use walker::Scanner;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Information about a cleanable project directory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectInfo {
    /// Root directory of the project
    pub root: PathBuf,

    /// Type of the project (Node, Rust, Python, etc.)
    pub project_type: ProjectType,

    /// Optional custom project type name (from config `custom_patterns`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,

    /// Cleanable directory path (e.g., node_modules, target)
    pub cleanable_dir: PathBuf,

    /// Size of the cleanable directory in bytes
    pub size: u64,

    /// Whether the size has been calculated
    #[serde(default = "default_true")]
    pub size_calculated: bool,

    /// Last modified time of the cleanable directory
    pub last_modified: DateTime<Utc>,

    /// Whether this directory is currently in use (based on lock files)
    pub in_use: bool,
}

fn default_true() -> bool {
    true
}

impl ProjectInfo {
    /// Create a new ProjectInfo with pending size calculation
    pub fn new_pending(
        root: PathBuf,
        project_type: ProjectType,
        cleanable_dir: PathBuf,
        last_modified: DateTime<Utc>,
        in_use: bool,
    ) -> Self {
        Self {
            root,
            project_type,
            project_name: None,
            cleanable_dir,
            size: 0,
            size_calculated: false,
            last_modified,
            in_use,
        }
    }

    pub fn project_type_display_name(&self) -> String {
        self.project_name
            .clone()
            .unwrap_or_else(|| self.project_type.name().to_string())
    }

    /// Returns a human-readable size string
    pub fn size_human(&self) -> String {
        if !self.size_calculated {
            "Calculating...".to_string()
        } else {
            crate::utils::format_size(self.size)
        }
    }

    /// Returns how many days since last modification
    pub fn days_since_modified(&self) -> i64 {
        let now = Utc::now();
        (now - self.last_modified).num_days()
    }
}
