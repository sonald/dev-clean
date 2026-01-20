mod detector;
mod size_calculator;
mod walker;

pub use detector::{ProjectDetector, ProjectType};
pub use size_calculator::SizeCalculator;
pub use walker::Scanner;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::{fmt, fmt::Display};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Cache,
    Build,
    Deps,
    Unknown,
}

impl Default for Category {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::Build => "build",
            Self::Deps => "deps",
            Self::Unknown => "unknown",
        }
    }
}

impl Display for Category {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl Default for RiskLevel {
    fn default() -> Self {
        Self::Medium
    }
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        }
    }
}

impl Display for RiskLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
    Unknown,
}

impl Default for Confidence {
    fn default() -> Self {
        Self::Unknown
    }
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Medium => "medium",
            Self::Low => "low",
            Self::Unknown => "unknown",
        }
    }
}

impl Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSource {
    Custom,
    Builtin,
    Gitignore,
    Heuristic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleRef {
    pub source: RuleSource,
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

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

    /// Category of this cleanable target (cache/build/deps)
    #[serde(default)]
    pub category: Category,

    /// Default risk level (low/medium/high)
    #[serde(default)]
    pub risk_level: RiskLevel,

    /// Confidence of the matching rule (high/medium/low)
    #[serde(default)]
    pub confidence: Confidence,

    /// Matching rule reference (source + pattern) for explain/audit
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_rule: Option<RuleRef>,

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
            category: Category::default(),
            risk_level: RiskLevel::default(),
            confidence: Confidence::default(),
            matched_rule: None,
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
