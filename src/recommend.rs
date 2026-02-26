use crate::scanner::RiskLevel;
use crate::ProjectInfo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecommendStrategy {
    SafeFirst,
    Balanced,
    MaxSpace,
}

impl RecommendStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SafeFirst => "safe-first",
            Self::Balanced => "balanced",
            Self::MaxSpace => "max-space",
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecommendOptions {
    pub target_bytes: u64,
    pub include_in_use: bool,
    pub include_recent: bool,
    pub include_protected: bool,
    pub recent_days: i64,
    pub strategy: RecommendStrategy,
    pub max_risk: Option<RiskLevel>,
}

impl RecommendOptions {
    pub fn new(target_bytes: u64) -> Self {
        Self {
            target_bytes,
            include_in_use: false,
            include_recent: false,
            include_protected: false,
            recent_days: 7,
            strategy: RecommendStrategy::SafeFirst,
            max_risk: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct BlockedSummary {
    pub in_use_count: usize,
    pub in_use_bytes: u64,
    pub protected_count: usize,
    pub protected_bytes: u64,
    pub recent_count: usize,
    pub recent_bytes: u64,
    pub risk_count: usize,
    pub risk_bytes: u64,
}

impl BlockedSummary {
    pub fn is_empty(&self) -> bool {
        self.in_use_count == 0
            && self.protected_count == 0
            && self.recent_count == 0
            && self.risk_count == 0
    }
}

#[derive(Debug, Clone)]
pub struct RecommendResult {
    pub target_bytes: u64,
    pub selected_bytes: u64,
    pub selected: Vec<ProjectInfo>,
    pub blocked: BlockedSummary,
}

pub fn recommend_projects(
    candidates: Vec<ProjectInfo>,
    options: &RecommendOptions,
) -> RecommendResult {
    let mut blocked = BlockedSummary::default();
    let mut eligible = Vec::new();

    for mut p in candidates {
        p.recent = p.days_since_modified() < options.recent_days;

        if let Some(max_risk) = options.max_risk {
            if p.risk_level > max_risk {
                blocked.risk_count += 1;
                blocked.risk_bytes = blocked.risk_bytes.saturating_add(p.size);
                p.skip_reason = Some("blocked_by_risk".to_string());
                continue;
            }
        }

        if !options.include_in_use && p.in_use {
            blocked.in_use_count += 1;
            blocked.in_use_bytes = blocked.in_use_bytes.saturating_add(p.size);
            p.skip_reason = Some("blocked_in_use".to_string());
            continue;
        }

        if !options.include_protected && p.protected {
            blocked.protected_count += 1;
            blocked.protected_bytes = blocked.protected_bytes.saturating_add(p.size);
            p.skip_reason = Some("blocked_protected".to_string());
            continue;
        }

        if !options.include_recent && p.recent {
            blocked.recent_count += 1;
            blocked.recent_bytes = blocked.recent_bytes.saturating_add(p.size);
            p.skip_reason = Some("blocked_recent".to_string());
            continue;
        }

        eligible.push(p);
    }

    eligible.sort_by(|a, b| {
        score_project(b, options.strategy)
            .cmp(&score_project(a, options.strategy))
            .then_with(|| b.size.cmp(&a.size))
            .then_with(|| b.days_since_modified().cmp(&a.days_since_modified()))
    });

    let mut selected = Vec::new();
    let mut selected_bytes = 0u64;

    for mut project in eligible {
        if selected_bytes >= options.target_bytes {
            break;
        }
        selected_bytes = selected_bytes.saturating_add(project.size);
        project.selection_reason = Some(match options.strategy {
            RecommendStrategy::SafeFirst => "strategy_safe_first".to_string(),
            RecommendStrategy::Balanced => "strategy_balanced".to_string(),
            RecommendStrategy::MaxSpace => "strategy_max_space".to_string(),
        });
        selected.push(project);
    }

    RecommendResult {
        target_bytes: options.target_bytes,
        selected_bytes,
        selected,
        blocked,
    }
}

fn score_project(p: &ProjectInfo, strategy: RecommendStrategy) -> i64 {
    let risk_penalty = match p.risk_level {
        RiskLevel::Low => 0,
        RiskLevel::Medium => 30,
        RiskLevel::High => 80,
    };
    let age_bonus = p.days_since_modified().clamp(0, 365);
    let size_mb = (p.size / (1024 * 1024)) as i64;

    match strategy {
        RecommendStrategy::SafeFirst => age_bonus * 2 + size_mb - risk_penalty * 3,
        RecommendStrategy::Balanced => age_bonus + size_mb * 2 - risk_penalty * 2,
        RecommendStrategy::MaxSpace => size_mb * 4 + age_bonus - risk_penalty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence};
    use crate::ProjectType;
    use chrono::{Duration, Utc};
    use std::path::PathBuf;

    fn mk_project(size: u64, days: i64, risk: RiskLevel) -> ProjectInfo {
        let now = Utc::now();
        ProjectInfo {
            root: PathBuf::from("/p"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: risk,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from(format!("/p/target-{}", size)),
            size,
            size_calculated: true,
            last_modified: now - Duration::days(days),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn strategy_max_space_prefers_larger() {
        let projects = vec![
            mk_project(10 * 1024 * 1024, 100, RiskLevel::Medium),
            mk_project(500 * 1024 * 1024, 20, RiskLevel::High),
        ];

        let mut opts = RecommendOptions::new(100);
        opts.strategy = RecommendStrategy::MaxSpace;
        opts.max_risk = Some(RiskLevel::High);

        let result = recommend_projects(projects, &opts);
        assert_eq!(result.selected.len(), 1);
        assert_eq!(result.selected[0].size, 500 * 1024 * 1024);
    }

    #[test]
    fn blocks_recent_by_default() {
        let projects = vec![mk_project(1024, 1, RiskLevel::Low)];
        let opts = RecommendOptions::new(100);
        let result = recommend_projects(projects, &opts);
        assert!(result.selected.is_empty());
        assert_eq!(result.blocked.recent_count, 1);
    }
}
