use crate::ProjectInfo;

#[derive(Debug, Clone)]
pub struct RecommendResult {
    pub target_bytes: u64,
    pub selected_bytes: u64,
    pub selected: Vec<ProjectInfo>,
}

pub fn recommend_projects(
    mut candidates: Vec<ProjectInfo>,
    target_bytes: u64,
    include_in_use: bool,
) -> RecommendResult {
    if !include_in_use {
        candidates.retain(|p| !p.in_use);
    }

    candidates.sort_by(|a, b| {
        a.risk_level
            .cmp(&b.risk_level)
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| b.days_since_modified().cmp(&a.days_since_modified()))
    });

    let mut selected = Vec::new();
    let mut selected_bytes = 0u64;

    for project in candidates {
        if selected_bytes >= target_bytes {
            break;
        }

        selected_bytes = selected_bytes.saturating_add(project.size);
        selected.push(project);
    }

    RecommendResult {
        target_bytes,
        selected_bytes,
        selected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, RiskLevel};
    use crate::ProjectType;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    #[test]
    fn test_recommend_projects_basic() {
        let now = Utc::now();
        let projects = vec![
            ProjectInfo {
                root: PathBuf::from("/p1"),
                project_type: ProjectType::Python,
                project_name: None,
                category: Category::Cache,
                risk_level: RiskLevel::Low,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/p1/__pycache__"),
                size: 5,
                size_calculated: true,
                last_modified: now - Duration::days(200),
                in_use: false,
            },
            ProjectInfo {
                root: PathBuf::from("/p2"),
                project_type: ProjectType::Rust,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/p2/target"),
                size: 100,
                size_calculated: true,
                last_modified: now - Duration::days(1),
                in_use: false,
            },
            ProjectInfo {
                root: PathBuf::from("/p3"),
                project_type: ProjectType::NodeJs,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/p3/dist"),
                size: 50,
                size_calculated: true,
                last_modified: now - Duration::days(100),
                in_use: false,
            },
        ];

        let result = recommend_projects(projects, 60, false);
        assert_eq!(result.target_bytes, 60);
        assert_eq!(result.selected.len(), 2);
        assert!(result.selected_bytes >= 60);
        assert_eq!(result.selected[0].risk_level, RiskLevel::Low);
        assert_eq!(result.selected[1].size, 100);
    }

    #[test]
    fn test_recommend_projects_skips_in_use() {
        let now = Utc::now();
        let projects = vec![
            ProjectInfo {
                root: PathBuf::from("/p1"),
                project_type: ProjectType::Rust,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/p1/target"),
                size: 100,
                size_calculated: true,
                last_modified: now,
                in_use: true,
            },
            ProjectInfo {
                root: PathBuf::from("/p2"),
                project_type: ProjectType::Rust,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: PathBuf::from("/p2/target"),
                size: 80,
                size_calculated: true,
                last_modified: now,
                in_use: false,
            },
        ];

        let result = recommend_projects(projects, 50, false);
        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].size, 80);
    }
}

