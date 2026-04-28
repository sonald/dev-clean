use crate::scanner::ProjectInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SafetyFlags {
    pub protected: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_by: Option<String>,
    pub recent: bool,
}

impl SafetyFlags {
    pub fn from_project_info(info: &ProjectInfo) -> Self {
        Self {
            protected: info.protected,
            protected_by: info.protected_by.clone(),
            recent: info.recent,
        }
    }

    pub fn apply_to_project_info(&self, info: &mut ProjectInfo) {
        info.protected = self.protected;
        info.protected_by = self.protected_by.clone();
        info.recent = self.recent;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    InUse,
    Protected,
    Recent,
    Risk,
    OutsideProjectRoot,
    OutsideScanRoot,
    RuleMismatchOrMissing,
}

impl SkipReason {
    pub fn legacy_label(self) -> &'static str {
        match self {
            Self::InUse => "blocked_in_use",
            Self::Protected => "blocked_protected",
            Self::Recent => "blocked_recent",
            Self::Risk => "blocked_by_risk",
            Self::OutsideProjectRoot => "outside_project_root",
            Self::OutsideScanRoot => "outside_scan_root",
            Self::RuleMismatchOrMissing => "rule_mismatch_or_missing",
        }
    }

    pub fn from_legacy_label(label: &str) -> Option<Self> {
        match label {
            "blocked_in_use" => Some(Self::InUse),
            "blocked_protected" => Some(Self::Protected),
            "blocked_recent" => Some(Self::Recent),
            "blocked_by_risk" => Some(Self::Risk),
            "outside_project_root" => Some(Self::OutsideProjectRoot),
            "outside_scan_root" => Some(Self::OutsideScanRoot),
            "rule_mismatch_or_missing" => Some(Self::RuleMismatchOrMissing),
            _ => None,
        }
    }
}

impl std::fmt::Display for SkipReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.legacy_label())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionReason {
    StrategySafeFirst,
    StrategyBalanced,
    StrategyMaxSpace,
}

impl SelectionReason {
    pub fn legacy_label(self) -> &'static str {
        match self {
            Self::StrategySafeFirst => "strategy_safe_first",
            Self::StrategyBalanced => "strategy_balanced",
            Self::StrategyMaxSpace => "strategy_max_space",
        }
    }

    pub fn from_legacy_label(label: &str) -> Option<Self> {
        match label {
            "strategy_safe_first" => Some(Self::StrategySafeFirst),
            "strategy_balanced" => Some(Self::StrategyBalanced),
            "strategy_max_space" => Some(Self::StrategyMaxSpace),
            _ => None,
        }
    }
}

impl std::fmt::Display for SelectionReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.legacy_label())
    }
}

#[derive(Debug, Clone)]
pub struct EvaluatedProject {
    pub info: ProjectInfo,
    pub safety: SafetyFlags,
    pub selection_reason: Option<SelectionReason>,
    pub skip_reason: Option<SkipReason>,
}

impl EvaluatedProject {
    pub fn new(info: ProjectInfo) -> Self {
        Self {
            safety: SafetyFlags::from_project_info(&info),
            info,
            selection_reason: None,
            skip_reason: None,
        }
    }

    pub fn with_safety(mut self, safety: SafetyFlags) -> Self {
        self.safety = safety;
        self
    }

    pub fn with_selection_reason(mut self, reason: SelectionReason) -> Self {
        self.selection_reason = Some(reason);
        self
    }

    pub fn with_skip_reason(mut self, reason: SkipReason) -> Self {
        self.skip_reason = Some(reason);
        self
    }

    pub fn to_project_info(&self) -> ProjectInfo {
        let mut info = self.info.clone();
        self.safety.apply_to_project_info(&mut info);
        info.selection_reason = self
            .selection_reason
            .map(|reason| reason.legacy_label().to_string());
        info.skip_reason = self
            .skip_reason
            .map(|reason| reason.legacy_label().to_string());
        info
    }

    pub fn into_project_info(self) -> ProjectInfo {
        let mut info = self.info;
        self.safety.apply_to_project_info(&mut info);
        info.selection_reason = self
            .selection_reason
            .map(|reason| reason.legacy_label().to_string());
        info.skip_reason = self
            .skip_reason
            .map(|reason| reason.legacy_label().to_string());
        info
    }

    pub fn is_recent(&self) -> bool {
        self.safety.recent
    }

    pub fn is_protected(&self) -> bool {
        self.safety.protected
    }
}

impl From<ProjectInfo> for EvaluatedProject {
    fn from(info: ProjectInfo) -> Self {
        let selection_reason = info
            .selection_reason
            .as_deref()
            .and_then(SelectionReason::from_legacy_label);
        let skip_reason = info
            .skip_reason
            .as_deref()
            .and_then(SkipReason::from_legacy_label);

        Self {
            safety: SafetyFlags::from_project_info(&info),
            info,
            selection_reason,
            skip_reason,
        }
    }
}

impl From<EvaluatedProject> for ProjectInfo {
    fn from(project: EvaluatedProject) -> Self {
        project.into_project_info()
    }
}

impl From<&EvaluatedProject> for ProjectInfo {
    fn from(project: &EvaluatedProject) -> Self {
        project.to_project_info()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, RiskLevel};
    use crate::ProjectType;
    use chrono::Utc;
    use std::path::PathBuf;

    fn sample_info() -> ProjectInfo {
        ProjectInfo {
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
            in_use: true,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn compat_roundtrip_preserves_legacy_labels() {
        let project = EvaluatedProject::new(sample_info())
            .with_safety(SafetyFlags {
                protected: true,
                protected_by: Some("config_keep_paths".to_string()),
                recent: true,
            })
            .with_selection_reason(SelectionReason::StrategyBalanced)
            .with_skip_reason(SkipReason::Protected);

        let info = project.into_project_info();
        assert!(info.protected);
        assert_eq!(info.protected_by.as_deref(), Some("config_keep_paths"));
        assert!(info.recent);
        assert_eq!(info.selection_reason.as_deref(), Some("strategy_balanced"));
        assert_eq!(info.skip_reason.as_deref(), Some("blocked_protected"));
    }
}
