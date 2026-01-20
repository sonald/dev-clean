use crate::ProjectInfo;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    pub scan_root: PathBuf,
    pub projects: Vec<ProjectInfo>,
}

impl CleanupPlan {
    pub fn new(scan_root: PathBuf, projects: Vec<ProjectInfo>) -> Self {
        Self {
            schema_version: 1,
            created_at: Utc::now(),
            scan_root,
            projects,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn load_json<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path.as_ref())
            .with_context(|| format!("Failed to read plan file: {}", path.as_ref().display()))?;
        let plan: CleanupPlan =
            serde_json::from_str(&content).with_context(|| "Failed to parse plan JSON")?;
        Ok(plan)
    }

    pub fn save_json<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json = self.to_json_pretty()?;
        fs::write(path.as_ref(), json)
            .with_context(|| format!("Failed to write plan file: {}", path.as_ref().display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, RiskLevel};
    use crate::ProjectType;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_plan_roundtrip_json() {
        let temp = TempDir::new().unwrap();
        let plan_path = temp.path().join("plan.json");

        let plan = CleanupPlan {
            schema_version: 1,
            created_at: Utc::now(),
            scan_root: PathBuf::from("/scan"),
            projects: vec![ProjectInfo {
                root: PathBuf::from("/scan/p1"),
                project_type: ProjectType::NodeJs,
                project_name: None,
                category: Category::Deps,
                risk_level: RiskLevel::High,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/scan/p1/node_modules"),
                size: 123,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
            }],
        };

        plan.save_json(&plan_path).unwrap();
        let loaded = CleanupPlan::load_json(&plan_path).unwrap();
        assert_eq!(loaded.schema_version, 1);
        assert_eq!(loaded.projects.len(), 1);
    }
}
