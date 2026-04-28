use crate::evaluation::{EvaluatedProject, SelectionReason, SkipReason};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BlockedSummary {
    pub in_use_count: usize,
    pub in_use_bytes: u64,
    pub protected_count: usize,
    pub protected_bytes: u64,
    pub recent_count: usize,
    pub recent_bytes: u64,
}

impl BlockedSummary {
    pub fn is_empty(&self) -> bool {
        self.in_use_count == 0 && self.protected_count == 0 && self.recent_count == 0
    }

    pub fn total_count(&self) -> usize {
        self.in_use_count + self.protected_count + self.recent_count
    }

    pub fn total_bytes(&self) -> u64 {
        self.in_use_bytes
            .saturating_add(self.protected_bytes)
            .saturating_add(self.recent_bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CleanupRequest {
    pub include_recent: bool,
    pub force: bool,
    pub force_protected: bool,
}

#[derive(Debug, Clone)]
pub struct CleanupSelection {
    pub selected: Vec<EvaluatedProject>,
    pub blocked: Vec<EvaluatedProject>,
    pub blocked_summary: BlockedSummary,
}

impl CleanupSelection {
    pub fn empty() -> Self {
        Self {
            selected: Vec::new(),
            blocked: Vec::new(),
            blocked_summary: BlockedSummary::default(),
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CleanupService;

impl CleanupService {
    pub fn new() -> Self {
        Self
    }

    pub fn split(
        &self,
        projects: Vec<EvaluatedProject>,
        request: CleanupRequest,
    ) -> CleanupSelection {
        let mut selected = Vec::new();
        let mut blocked = Vec::new();
        let mut blocked_summary = BlockedSummary::default();

        for mut project in projects {
            if project.is_protected() && !request.force_protected {
                project.skip_reason = Some(SkipReason::Protected);
                blocked_summary.protected_count += 1;
                blocked_summary.protected_bytes = blocked_summary
                    .protected_bytes
                    .saturating_add(project.info.size);
                blocked.push(project);
                continue;
            }

            if project.is_recent() && !request.include_recent {
                project.skip_reason = Some(SkipReason::Recent);
                blocked_summary.recent_count += 1;
                blocked_summary.recent_bytes = blocked_summary
                    .recent_bytes
                    .saturating_add(project.info.size);
                blocked.push(project);
                continue;
            }

            if project.info.in_use && !request.force {
                project.skip_reason = Some(SkipReason::InUse);
                blocked_summary.in_use_count += 1;
                blocked_summary.in_use_bytes = blocked_summary
                    .in_use_bytes
                    .saturating_add(project.info.size);
                blocked.push(project);
                continue;
            }

            selected.push(project);
        }

        CleanupSelection {
            selected,
            blocked,
            blocked_summary,
        }
    }

    pub fn mark_selected(
        mut project: EvaluatedProject,
        reason: SelectionReason,
    ) -> EvaluatedProject {
        project.selection_reason = Some(reason);
        project
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evaluation::{SafetyFlags, SkipReason};
    use crate::scanner::{Category, Confidence, ProjectType, RiskLevel};
    use crate::ProjectInfo;
    use chrono::Utc;
    use std::path::PathBuf;

    fn sample_project() -> EvaluatedProject {
        EvaluatedProject::new(ProjectInfo {
            root: PathBuf::from("/repo/app"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from("/repo/app/target"),
            size: 99,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        })
    }

    #[test]
    fn split_tracks_blocked_reasons() {
        let mut protected = sample_project();
        protected.safety = SafetyFlags {
            protected: true,
            protected_by: Some("config_keep_paths".to_string()),
            recent: false,
        };

        let mut recent = sample_project();
        recent.safety.recent = true;

        let mut in_use = sample_project();
        in_use.info.in_use = true;

        let selection =
            CleanupService::new().split(vec![protected, recent, in_use], CleanupRequest::default());

        assert!(selection.selected.is_empty());
        assert_eq!(selection.blocked_summary.total_count(), 3);
        assert_eq!(selection.blocked_summary.protected_count, 1);
        assert_eq!(selection.blocked_summary.recent_count, 1);
        assert_eq!(selection.blocked_summary.in_use_count, 1);
        assert!(selection.blocked.iter().all(|p| p.skip_reason.is_some()));
        assert!(selection
            .blocked
            .iter()
            .any(|p| p.skip_reason == Some(SkipReason::Protected)));
    }
}
