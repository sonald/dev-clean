use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Configuration for the cleaner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Directories to always exclude from scanning
    #[serde(default)]
    pub exclude_dirs: Vec<String>,

    /// Additional cleanable directory patterns
    #[serde(default)]
    pub custom_patterns: Vec<CustomPattern>,

    /// Default scan depth
    #[serde(default)]
    pub default_depth: Option<usize>,

    /// Minimum size in MB to show by default
    #[serde(default)]
    pub min_size_mb: Option<u64>,

    /// Maximum age in days
    #[serde(default)]
    pub max_age_days: Option<i64>,

    /// Named scan profiles
    #[serde(default)]
    pub scan_profiles: BTreeMap<String, ScanProfile>,

    /// Exact protected paths
    #[serde(default)]
    pub keep_paths: Vec<String>,

    /// Glob protected paths
    #[serde(default)]
    pub keep_globs: Vec<String>,

    /// Protected project roots
    #[serde(default)]
    pub keep_project_roots: Vec<String>,

    /// Audit configuration
    #[serde(default)]
    pub audit: AuditConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            exclude_dirs: vec![
                String::from(".git"),
                String::from(".svn"),
                String::from(".hg"),
            ],
            custom_patterns: Vec::new(),
            default_depth: None,
            min_size_mb: None,
            max_age_days: None,
            scan_profiles: BTreeMap::new(),
            keep_paths: Vec::new(),
            keep_globs: Vec::new(),
            keep_project_roots: Vec::new(),
            audit: AuditConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScanProfile {
    #[serde(default)]
    pub paths: Vec<PathBuf>,
    #[serde(default)]
    pub depth: Option<usize>,
    #[serde(default)]
    pub min_size_mb: Option<u64>,
    #[serde(default)]
    pub max_age_days: Option<i64>,
    #[serde(default)]
    pub gitignore: Option<bool>,
    #[serde(default)]
    pub category: Option<crate::scanner::Category>,
    #[serde(default)]
    pub max_risk: Option<crate::scanner::RiskLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default = "default_audit_max_size_mb")]
    pub max_size_mb: u64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            path: None,
            max_size_mb: default_audit_max_size_mb(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_audit_max_size_mb() -> u64 {
    5
}

/// Custom cleanable pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomPattern {
    /// Name of the pattern
    pub name: String,

    /// Directory name to match
    pub directory: String,

    /// Marker files to identify project type
    pub marker_files: Vec<String>,

    /// How to interpret `marker_files`
    #[serde(default)]
    pub marker_mode: MarkerMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerMode {
    AnyOf,
    AllOf,
}

impl Default for MarkerMode {
    fn default() -> Self {
        Self::AnyOf
    }
}

impl Config {
    /// Load config from file, or create default if not exists
    pub fn load_or_default<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();

        if path.exists() {
            Self::load(path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load config from file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read config file: {}", path.as_ref().display()))?;

        let config: Config =
            toml::from_str(&content).with_context(|| "Failed to parse config file")?;

        Ok(config)
    }

    /// Save config to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let content = toml::to_string_pretty(self).with_context(|| "Failed to serialize config")?;

        fs::write(path.as_ref(), content)
            .with_context(|| format!("Failed to write config file: {}", path.as_ref().display()))?;

        Ok(())
    }

    /// Get default config path
    pub fn default_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("dev-cleaner").join("config.toml")
        } else {
            PathBuf::from(".dev-cleaner.toml")
        }
    }

    /// Create config directory if it doesn't exist
    pub fn ensure_config_dir() -> Result<PathBuf> {
        let config_path = Self::default_path();

        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create config directory: {}", parent.display())
            })?;
        }

        Ok(config_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_config_save_load() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("config.toml");

        let config = Config {
            exclude_dirs: vec![String::from("test")],
            ..Default::default()
        };

        config.save(&config_path).unwrap();

        let loaded = Config::load(&config_path).unwrap();
        assert_eq!(loaded.exclude_dirs, vec!["test"]);
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert!(config.exclude_dirs.contains(&String::from(".git")));
    }
}
