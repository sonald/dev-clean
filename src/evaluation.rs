use crate::ProjectInfo;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SafetyFlags {
    pub protected: bool,
    pub protected_by: Option<String>,
    pub recent: bool,
}

impl SafetyFlags {
    pub fn new(protected: bool, protected_by: Option<String>, recent: bool) -> Self {
        Self {
            protected,
            protected_by,
            recent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    InUse,
    Protected,
    Recent,
    Risk,
}

impl SkipReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InUse => "blocked_in_use",
            Self::Protected => "blocked_protected",
            Self::Recent => "blocked_recent",
            Self::Risk => "blocked_by_risk",
        }
    }

    pub fn from_legacy_str(value: &str) -> Option<Self> {
        match value {
            "blocked_in_use" => Some(Self::InUse),
            "blocked_protected" => Some(Self::Protected),
            "blocked_recent" => Some(Self::Recent),
            "blocked_by_risk" => Some(Self::Risk),
            _ => None,
        }
    }
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionReason {
    StrategySafeFirst,
    StrategyBalanced,
    StrategyMaxSpace,
}

impl SelectionReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StrategySafeFirst => "strategy_safe_first",
            Self::StrategyBalanced => "strategy_balanced",
            Self::StrategyMaxSpace => "strategy_max_space",
        }
    }

    pub fn from_legacy_str(value: &str) -> Option<Self> {
        match value {
            "strategy_safe_first" => Some(Self::StrategySafeFirst),
            "strategy_balanced" => Some(Self::StrategyBalanced),
            "strategy_max_space" => Some(Self::StrategyMaxSpace),
            _ => None,
        }
    }
}

impl std::fmt::Display for SelectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct EvaluatedProject {
    pub project: ProjectInfo,
    pub safety: SafetyFlags,
    pub skip_reason: Option<SkipReason>,
    pub selection_reason: Option<SelectionReason>,
}

impl EvaluatedProject {
    pub fn new(project: ProjectInfo) -> Self {
        let safety = SafetyFlags::new(
            project.protected,
            project.protected_by.clone(),
            project.recent,
        );
        let skip_reason = project
            .skip_reason
            .as_deref()
            .and_then(SkipReason::from_legacy_str);
        let selection_reason = project
            .selection_reason
            .as_deref()
            .and_then(SelectionReason::from_legacy_str);

        Self {
            project,
            safety,
            skip_reason,
            selection_reason,
        }
    }

    pub fn with_safety(mut self, safety: SafetyFlags) -> Self {
        self.safety = safety;
        self
    }

    pub fn mark_recent(mut self, recent: bool) -> Self {
        self.safety.recent = recent;
        self
    }

    pub fn mark_protected(mut self, protected: bool, protected_by: Option<String>) -> Self {
        self.safety.protected = protected;
        self.safety.protected_by = protected_by;
        self
    }

    pub fn mark_skip_reason(mut self, reason: SkipReason) -> Self {
        self.skip_reason = Some(reason);
        self
    }

    pub fn mark_selection_reason(mut self, reason: SelectionReason) -> Self {
        self.selection_reason = Some(reason);
        self
    }

    pub fn to_project_info(&self) -> ProjectInfo {
        let mut project = self.project.clone();
        self.apply_to_project_info(&mut project);
        project
    }

    pub fn into_project_info(self) -> ProjectInfo {
        let mut project = self.project;
        project.protected = self.safety.protected;
        project.protected_by = self.safety.protected_by;
        project.recent = self.safety.recent;
        project.skip_reason = self.skip_reason.map(|reason| reason.as_str().to_string());
        project.selection_reason = self
            .selection_reason
            .map(|reason| reason.as_str().to_string());
        project
    }

    pub fn apply_to_project_info(&self, project: &mut ProjectInfo) {
        project.protected = self.safety.protected;
        project.protected_by = self.safety.protected_by.clone();
        project.recent = self.safety.recent;
        project.skip_reason = self.skip_reason.map(|reason| reason.as_str().to_string());
        project.selection_reason = self
            .selection_reason
            .map(|reason| reason.as_str().to_string());
    }
}

impl From<ProjectInfo> for EvaluatedProject {
    fn from(project: ProjectInfo) -> Self {
        Self::new(project)
    }
}

impl From<EvaluatedProject> for ProjectInfo {
    fn from(value: EvaluatedProject) -> Self {
        value.into_project_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, RiskLevel};
    use crate::ProjectType;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn legacy_reason_strings_roundtrip() {
        assert_eq!(SkipReason::InUse.as_str(), "blocked_in_use");
        assert_eq!(SkipReason::Protected.as_str(), "blocked_protected");
        assert_eq!(SkipReason::Recent.as_str(), "blocked_recent");
        assert_eq!(SkipReason::Risk.as_str(), "blocked_by_risk");
        assert_eq!(
            SelectionReason::StrategySafeFirst.as_str(),
            "strategy_safe_first"
        );
        assert_eq!(
            SelectionReason::StrategyBalanced.as_str(),
            "strategy_balanced"
        );
        assert_eq!(
            SelectionReason::StrategyMaxSpace.as_str(),
            "strategy_max_space"
        );
    }

    #[test]
    fn evaluated_project_preserves_projectinfo_compatibility() {
        let project = ProjectInfo {
            root: PathBuf::from("/scan/app"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from("/scan/app/target"),
            size: 42,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use: false,
            protected: true,
            protected_by: Some("config_keep_paths".to_string()),
            recent: true,
            selection_reason: Some("strategy_balanced".to_string()),
            skip_reason: Some("blocked_recent".to_string()),
        };

        let evaluated = EvaluatedProject::from(project);
        assert!(evaluated.safety.protected);
        assert_eq!(
            evaluated.safety.protected_by.as_deref(),
            Some("config_keep_paths")
        );
        assert!(evaluated.safety.recent);
        assert_eq!(evaluated.skip_reason, Some(SkipReason::Recent));
        assert_eq!(
            evaluated.selection_reason,
            Some(SelectionReason::StrategyBalanced)
        );

        let roundtrip = evaluated.to_project_info();
        assert!(roundtrip.protected);
        assert_eq!(roundtrip.protected_by.as_deref(), Some("config_keep_paths"));
        assert!(roundtrip.recent);
        assert_eq!(
            roundtrip.selection_reason.as_deref(),
            Some("strategy_balanced")
        );
        assert_eq!(roundtrip.skip_reason.as_deref(), Some("blocked_recent"));
    }
}
