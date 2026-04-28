use crate::ProjectInfo;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Statistics about cleanable directories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    /// Total size of all cleanable directories
    pub total_size: u64,

    /// Total number of projects
    pub total_projects: usize,

    /// Statistics grouped by project type
    pub by_type: HashMap<String, TypeStats>,

    /// Top N largest directories
    pub top_largest: Vec<ProjectStats>,

    /// Statistics grouped by age
    pub by_age_group: AgeGroupStats,
}

/// Statistics for a specific project type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeStats {
    /// Total size for this type
    pub total_size: u64,

    /// Number of projects of this type
    pub count: usize,

    /// Average size per project
    pub avg_size: u64,
}

/// Simplified project info for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    /// Path to cleanable directory
    pub path: String,

    /// Size in bytes
    pub size: u64,

    /// Project type
    pub project_type: String,

    /// Days since last modification
    pub age_days: i64,
}

/// Age-based grouping of statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeGroupStats {
    /// Recent projects (<30 days): (count, total_size)
    pub recent: (usize, u64),

    /// Medium age projects (30-90 days): (count, total_size)
    pub medium: (usize, u64),

    /// Old projects (>90 days): (count, total_size)
    pub old: (usize, u64),
}

impl Statistics {
    /// Create statistics from a list of projects
    pub fn from_projects(projects: Vec<ProjectInfo>) -> Self {
        let total_projects = projects.len();
        let total_size: u64 = projects.iter().map(|p| p.size).sum();

        // Group by type
        let mut by_type: HashMap<String, TypeStats> = HashMap::new();
        for project in &projects {
            let type_name = project.project_type_display_name();
            let entry = by_type.entry(type_name.clone()).or_insert(TypeStats {
                total_size: 0,
                count: 0,
                avg_size: 0,
            });
            entry.total_size += project.size;
            entry.count += 1;
        }

        // Calculate average sizes
        for stats in by_type.values_mut() {
            stats.avg_size = if stats.count > 0 {
                stats.total_size / stats.count as u64
            } else {
                0
            };
        }

        // Create top largest list
        let mut sorted_projects = projects.clone();
        sorted_projects.sort_by(|a, b| b.size.cmp(&a.size));
        let top_largest: Vec<ProjectStats> = sorted_projects
            .iter()
            .map(|p| ProjectStats {
                path: p.cleanable_dir.display().to_string(),
                size: p.size,
                project_type: p.project_type_display_name(),
                age_days: p.days_since_modified(),
            })
            .collect();

        // Group by age
        let mut recent = (0, 0u64);
        let mut medium = (0, 0u64);
        let mut old = (0, 0u64);

        for project in &projects {
            let age = project.days_since_modified();
            if age < 30 {
                recent.0 += 1;
                recent.1 += project.size;
            } else if age < 90 {
                medium.0 += 1;
                medium.1 += project.size;
            } else {
                old.0 += 1;
                old.1 += project.size;
            }
        }

        let by_age_group = AgeGroupStats {
            recent,
            medium,
            old,
        };

        Self {
            total_size,
            total_projects,
            by_type,
            top_largest,
            by_age_group,
        }
    }

    /// Export statistics as JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, RiskLevel};
    use crate::ProjectType;
    use chrono::Utc;
    use std::path::PathBuf;

    fn project(
        project_type: ProjectType,
        size: u64,
        days_since_modified: i64,
        category: Category,
        risk_level: RiskLevel,
    ) -> ProjectInfo {
        ProjectInfo {
            root: PathBuf::from("/repo"),
            project_type,
            project_name: None,
            category,
            risk_level,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from(format!("/repo/{project_type:?}-{size}")),
            size,
            size_calculated: true,
            last_modified: Utc::now() - chrono::Duration::days(days_since_modified),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn test_statistics_from_projects() {
        let projects = vec![
            ProjectInfo {
                root: PathBuf::from("/test1"),
                project_type: ProjectType::NodeJs,
                project_name: None,
                category: Category::Deps,
                risk_level: RiskLevel::High,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/test1/node_modules"),
                size: 1000000,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
                protected: false,
                protected_by: None,
                recent: false,
                selection_reason: None,
                skip_reason: None,
            },
            ProjectInfo {
                root: PathBuf::from("/test2"),
                project_type: ProjectType::Rust,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/test2/target"),
                size: 2000000,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
                protected: false,
                protected_by: None,
                recent: false,
                selection_reason: None,
                skip_reason: None,
            },
        ];

        let stats = Statistics::from_projects(projects);

        assert_eq!(stats.total_projects, 2);
        assert_eq!(stats.total_size, 3000000);
        assert_eq!(stats.by_type.len(), 2);
        assert_eq!(stats.top_largest.len(), 2);
    }

    #[test]
    fn test_statistics_age_boundaries_and_render_bar() {
        let stats = Statistics::from_projects(vec![
            project(ProjectType::NodeJs, 10, 29, Category::Deps, RiskLevel::High),
            project(
                ProjectType::Rust,
                20,
                30,
                Category::Build,
                RiskLevel::Medium,
            ),
            project(ProjectType::Python, 30, 89, Category::Cache, RiskLevel::Low),
            project(
                ProjectType::Java,
                40,
                90,
                Category::Build,
                RiskLevel::Medium,
            ),
        ]);

        assert_eq!(stats.total_projects, 4);
        assert_eq!(stats.total_size, 100);
        assert_eq!(stats.by_age_group.recent, (1, 10));
        assert_eq!(stats.by_age_group.medium, (2, 50));
        assert_eq!(stats.by_age_group.old, (1, 40));
        assert_eq!(stats.top_largest[0].size, 40);
        assert_eq!(stats.top_largest[1].size, 30);
        assert_eq!(stats.top_largest[2].size, 20);
        assert_eq!(stats.top_largest[3].size, 10);
        assert_eq!(stats.by_type["Rust"].avg_size, 20);
        let json = stats.to_json().unwrap();
        assert!(json.contains("\"total_projects\": 4"));
    }
}
