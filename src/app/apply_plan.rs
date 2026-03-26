use crate::app::cleanup::{BlockedSummary, CleanupRequest, CleanupSelection, CleanupService};
use crate::app::evaluated::{EvaluatedProject, SafetyFlags, SkipReason};
use crate::app::scan::canonicalize_lossy;
use crate::config::Config;
use crate::plan::CleanupPlan;
use crate::policy::KeepPolicy;
use crate::scanner::{ProjectInfo, Scanner};
use anyhow::{bail, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ApplyPlanRequest {
    pub plan: CleanupPlan,
    pub no_verify: bool,
    pub include_recent: bool,
    pub force: bool,
    pub force_protected: bool,
    pub recent_days: i64,
}

#[derive(Debug, Clone)]
pub struct ApplyPlanResult {
    pub plan: CleanupPlan,
    pub scan_root: PathBuf,
    pub skipped_projects: Vec<EvaluatedProject>,
    pub skipped_pre_count: usize,
    pub skipped_pre_bytes: u64,
    pub verification_blocked: BlockedSummary,
    pub verified_projects: Vec<EvaluatedProject>,
    pub cleanup_selection: CleanupSelection,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ApplyPlanService {
    cleanup: CleanupService,
}

impl ApplyPlanService {
    pub fn new() -> Self {
        Self {
            cleanup: CleanupService::new(),
        }
    }

    pub fn verify(&self, config: &Config, request: ApplyPlanRequest) -> Result<ApplyPlanResult> {
        let ApplyPlanRequest {
            plan,
            no_verify,
            include_recent,
            force,
            force_protected,
            recent_days,
        } = request;
        self.validate_plan_schema(plan.schema_version)?;

        let keep_policy = KeepPolicy::from_config(config);
        let scan_root_is_absolute = plan.scan_root.is_absolute();
        let scan_root = if scan_root_is_absolute {
            canonicalize_lossy(&plan.scan_root)
        } else {
            plan.scan_root.clone()
        };
        let params_max_risk = plan.params.as_ref().and_then(|p| p.max_risk);
        let params_category = plan.params.as_ref().and_then(|p| p.category);
        let mut scanner_cache: HashMap<PathBuf, Scanner> = HashMap::new();

        let mut skipped_projects = Vec::new();
        let mut skipped_pre_count = 0usize;
        let mut skipped_pre_bytes = 0u64;
        let mut verification_blocked = BlockedSummary::default();
        let mut verified_projects = Vec::new();

        for project in &plan.projects {
            let candidate = self.verify_project(
                config,
                &keep_policy,
                &mut scanner_cache,
                &scan_root,
                scan_root_is_absolute,
                project,
                params_max_risk,
                params_category,
                no_verify,
                include_recent,
                force,
                force_protected,
                recent_days,
            )?;

            if let Some(reason) = candidate.skip_reason {
                skipped_pre_count += 1;
                skipped_pre_bytes = skipped_pre_bytes.saturating_add(candidate.info.size);
                match reason {
                    SkipReason::Protected => {
                        verification_blocked.protected_count += 1;
                        verification_blocked.protected_bytes = verification_blocked
                            .protected_bytes
                            .saturating_add(candidate.info.size);
                    }
                    SkipReason::Recent => {
                        verification_blocked.recent_count += 1;
                        verification_blocked.recent_bytes = verification_blocked
                            .recent_bytes
                            .saturating_add(candidate.info.size);
                    }
                    SkipReason::InUse => {
                        verification_blocked.in_use_count += 1;
                        verification_blocked.in_use_bytes = verification_blocked
                            .in_use_bytes
                            .saturating_add(candidate.info.size);
                    }
                    _ => {}
                }
                skipped_projects.push(candidate);
            } else {
                verified_projects.push(candidate);
            }
        }

        let cleanup_selection = self.cleanup.split(
            verified_projects.clone(),
            CleanupRequest {
                include_recent,
                force,
                force_protected,
            },
        );

        Ok(ApplyPlanResult {
            plan,
            scan_root,
            skipped_projects,
            skipped_pre_count,
            skipped_pre_bytes,
            verification_blocked,
            verified_projects,
            cleanup_selection,
        })
    }

    fn verify_project(
        &self,
        config: &Config,
        keep_policy: &KeepPolicy,
        scanner_cache: &mut HashMap<PathBuf, Scanner>,
        scan_root: &Path,
        scan_root_is_absolute: bool,
        project: &ProjectInfo,
        params_max_risk: Option<crate::scanner::RiskLevel>,
        params_category: Option<crate::scanner::Category>,
        no_verify: bool,
        include_recent: bool,
        force: bool,
        force_protected: bool,
        recent_days: i64,
    ) -> Result<EvaluatedProject> {
        let cleanable_dir = canonicalize_lossy(&project.cleanable_dir);
        let project_root = canonicalize_lossy(&project.root);

        if !cleanable_dir.starts_with(&project_root) {
            return Ok(EvaluatedProject::new(project.clone())
                .with_skip_reason(SkipReason::OutsideProjectRoot));
        }

        if scan_root_is_absolute && !cleanable_dir.starts_with(scan_root) {
            return Ok(EvaluatedProject::new(project.clone())
                .with_skip_reason(SkipReason::OutsideScanRoot));
        }

        let mut candidate = if no_verify {
            let mut info = project.clone();
            info.root = project_root.clone();
            info.cleanable_dir = cleanable_dir.clone();
            EvaluatedProject::new(info)
        } else {
            let scanner = scanner_cache
                .entry(project_root.clone())
                .or_insert_with(|| {
                    let mut scanner = Scanner::new(&project_root)
                        .exclude_dirs(&config.exclude_dirs)
                        .custom_patterns(&config.custom_patterns);

                    if let Some(max_risk) = params_max_risk {
                        scanner = scanner.max_risk(max_risk);
                    }
                    if let Some(category) = params_category {
                        scanner = scanner.category(category);
                    }

                    scanner
                });

            match scanner.revalidate_target(&cleanable_dir) {
                Some(info) => EvaluatedProject::from(info),
                None => {
                    return Ok(EvaluatedProject::new(project.clone())
                        .with_skip_reason(SkipReason::RuleMismatchOrMissing));
                }
            }
        };

        let decision = keep_policy.evaluate(&candidate.info);
        candidate.safety = SafetyFlags {
            protected: decision.protected,
            protected_by: decision.reason,
            recent: candidate.info.days_since_modified() < recent_days,
        };

        if candidate.is_protected() && !force_protected {
            candidate.skip_reason = Some(SkipReason::Protected);
        } else if candidate.is_recent() && !include_recent {
            candidate.skip_reason = Some(SkipReason::Recent);
        } else if candidate.info.in_use && !force {
            candidate.skip_reason = Some(SkipReason::InUse);
        }

        Ok(candidate)
    }

    fn validate_plan_schema(&self, schema_version: u32) -> Result<()> {
        if schema_version != 1 && schema_version != 2 && schema_version != 3 {
            bail!("Unsupported plan schema_version: {}", schema_version);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::PlanParams;
    use crate::scanner::{Category, Confidence, ProjectType, RiskLevel};
    use chrono::{Duration, Utc};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    fn touch_with_timestamp(path: &Path, timestamp: &str) {
        let status = Command::new("touch")
            .args(["-t", timestamp, path.to_str().unwrap()])
            .status()
            .expect("failed to run touch");
        assert!(status.success());
    }

    fn sample_project(
        root: PathBuf,
        target: PathBuf,
        project_type: ProjectType,
        category: Category,
        risk_level: RiskLevel,
        in_use: bool,
        last_modified: chrono::DateTime<Utc>,
    ) -> ProjectInfo {
        ProjectInfo {
            root,
            project_type,
            project_name: None,
            category,
            risk_level,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: target,
            size: 7,
            size_calculated: true,
            last_modified,
            in_use,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    fn sample_plan(root: PathBuf, target: PathBuf) -> CleanupPlan {
        CleanupPlan {
            schema_version: 3,
            tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            created_at: Utc::now(),
            scan_root: root.clone(),
            params: None,
            projects: vec![ProjectInfo {
                root,
                project_type: ProjectType::Rust,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir: target,
                size: 7,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
                protected: false,
                protected_by: None,
                recent: false,
                selection_reason: None,
                skip_reason: None,
            }],
        }
    }

    #[test]
    fn verify_rejects_unsupported_schema_version() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(root.join("package.json"), "{}").unwrap();

        let mut plan = sample_plan(root.clone(), target);
        plan.schema_version = 9;

        let request = ApplyPlanRequest {
            plan,
            no_verify: false,
            include_recent: false,
            force: false,
            force_protected: false,
            recent_days: 7,
        };

        let err = ApplyPlanService::new()
            .verify(&Config::default(), request)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("Unsupported plan schema_version: 9"));
    }

    #[test]
    fn verify_rejects_targets_outside_project_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        let target = temp.path().join("elsewhere/target");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::create_dir_all(&target).unwrap();

        let plan = sample_plan(root.clone(), target);
        let request = ApplyPlanRequest {
            plan,
            no_verify: false,
            include_recent: false,
            force: false,
            force_protected: false,
            recent_days: 7,
        };

        let result = ApplyPlanService::new()
            .verify(&Config::default(), request)
            .unwrap();
        assert_eq!(result.skipped_pre_count, 1);
        assert_eq!(result.verified_projects.len(), 0);
        assert_eq!(result.skipped_projects.len(), 1);
        assert_eq!(
            result.skipped_projects[0].skip_reason,
            Some(SkipReason::OutsideProjectRoot)
        );
    }

    #[test]
    fn verify_rejects_targets_outside_scan_root() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path().join("repo/project");
        let scan_root = temp.path().join("scan-root");
        let target = project_root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::create_dir_all(&scan_root).unwrap();
        fs::write(project_root.join("package.json"), "{}").unwrap();

        let mut plan = sample_plan(project_root.clone(), target);
        plan.scan_root = scan_root.clone();

        let request = ApplyPlanRequest {
            plan,
            no_verify: false,
            include_recent: false,
            force: false,
            force_protected: false,
            recent_days: 7,
        };

        let result = ApplyPlanService::new()
            .verify(&Config::default(), request)
            .unwrap();
        assert_eq!(result.skipped_pre_count, 1);
        assert_eq!(result.verified_projects.len(), 0);
        assert_eq!(result.skipped_projects.len(), 1);
        assert_eq!(
            result.skipped_projects[0].skip_reason,
            Some(SkipReason::OutsideScanRoot)
        );
    }

    #[test]
    fn verify_no_verify_bypasses_revalidation_but_stale_plan_is_rejected() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path().join("repo/project");
        let target = project_root.join("node_modules");
        fs::create_dir_all(&target).unwrap();
        fs::write(project_root.join("package.json"), "{}").unwrap();

        let plan = sample_plan(project_root.clone(), target.clone());
        let mut stale_plan = plan.clone();
        stale_plan.projects[0].last_modified = Utc::now() - Duration::days(30);
        fs::remove_file(project_root.join("package.json")).unwrap();

        let no_verify_result = ApplyPlanService::new()
            .verify(
                &Config::default(),
                ApplyPlanRequest {
                    plan: stale_plan.clone(),
                    no_verify: true,
                    include_recent: false,
                    force: false,
                    force_protected: false,
                    recent_days: 7,
                },
            )
            .unwrap();
        assert_eq!(no_verify_result.verified_projects.len(), 1);
        assert_eq!(no_verify_result.skipped_projects.len(), 0);

        let verify_result = ApplyPlanService::new()
            .verify(
                &Config::default(),
                ApplyPlanRequest {
                    plan: stale_plan,
                    no_verify: false,
                    include_recent: false,
                    force: false,
                    force_protected: false,
                    recent_days: 7,
                },
            )
            .unwrap();
        assert_eq!(verify_result.verified_projects.len(), 0);
        assert_eq!(verify_result.skipped_projects.len(), 1);
        assert_eq!(
            verify_result.skipped_projects[0].skip_reason,
            Some(SkipReason::RuleMismatchOrMissing)
        );
    }

    #[test]
    fn verify_tracks_protected_recent_and_in_use_and_force_flags() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        let protected_root = root.join("protected-project");
        let recent_root = root.join("recent-project");
        let in_use_root = root.join("in-use-project");
        let protected_target = protected_root.join("node_modules");
        let recent_target = recent_root.join("node_modules");
        let in_use_target = in_use_root.join("node_modules");
        let in_use_lock = in_use_root.join("package-lock.json");

        fs::create_dir_all(&protected_target).unwrap();
        fs::create_dir_all(&recent_target).unwrap();
        fs::create_dir_all(&in_use_target).unwrap();
        fs::write(protected_root.join("package.json"), "{}").unwrap();
        fs::write(recent_root.join("package.json"), "{}").unwrap();
        fs::write(in_use_root.join("package.json"), "{}").unwrap();
        fs::write(protected_root.join(".dev-cleaner-keep"), "").unwrap();
        fs::write(&in_use_lock, "{}").unwrap();

        touch_with_timestamp(&protected_target, "202603010101");
        touch_with_timestamp(&recent_target, "202603250101");
        touch_with_timestamp(&in_use_target, "202603010101");
        touch_with_timestamp(&in_use_lock, "202603250101");

        let plan = CleanupPlan {
            schema_version: 3,
            tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            created_at: Utc::now(),
            scan_root: root.clone(),
            params: None,
            projects: vec![
                sample_project(
                    protected_root.clone(),
                    protected_target.clone(),
                    ProjectType::NodeJs,
                    Category::Deps,
                    RiskLevel::High,
                    false,
                    Utc::now() - Duration::days(30),
                ),
                sample_project(
                    recent_root.clone(),
                    recent_target.clone(),
                    ProjectType::NodeJs,
                    Category::Deps,
                    RiskLevel::High,
                    false,
                    Utc::now(),
                ),
                sample_project(
                    in_use_root.clone(),
                    in_use_target.clone(),
                    ProjectType::NodeJs,
                    Category::Deps,
                    RiskLevel::High,
                    true,
                    Utc::now() - Duration::days(30),
                ),
            ],
        };

        let default_result = ApplyPlanService::new()
            .verify(
                &Config::default(),
                ApplyPlanRequest {
                    plan: plan.clone(),
                    no_verify: false,
                    include_recent: false,
                    force: false,
                    force_protected: false,
                    recent_days: 7,
                },
            )
            .unwrap();
        assert_eq!(default_result.skipped_pre_count, 3);
        assert_eq!(default_result.verified_projects.len(), 0);
        assert_eq!(default_result.verification_blocked.protected_count, 1);
        assert_eq!(default_result.verification_blocked.recent_count, 1);
        assert_eq!(default_result.verification_blocked.in_use_count, 1);

        let forced_result = ApplyPlanService::new()
            .verify(
                &Config::default(),
                ApplyPlanRequest {
                    plan,
                    no_verify: false,
                    include_recent: true,
                    force: true,
                    force_protected: true,
                    recent_days: 7,
                },
            )
            .unwrap();
        assert_eq!(forced_result.skipped_pre_count, 0);
        assert_eq!(forced_result.verified_projects.len(), 3);
        assert_eq!(forced_result.cleanup_selection.selected.len(), 3);
    }

    #[test]
    fn verify_respects_plan_params_max_risk_and_category() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("repo");
        let deps_root = root.join("deps-project");
        let deps_target = deps_root.join("custom-cache");

        fs::create_dir_all(&deps_target).unwrap();
        fs::write(deps_root.join("package.json"), "{}").unwrap();
        fs::write(deps_root.join(".gitignore"), "custom-cache/\n").unwrap();
        touch_with_timestamp(&deps_target, "202603010101");

        let deps_plan = CleanupPlan {
            schema_version: 3,
            tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            created_at: Utc::now(),
            scan_root: root.clone(),
            params: Some(PlanParams {
                max_risk: Some(RiskLevel::Medium),
                category: Some(Category::Build),
                ..Default::default()
            }),
            projects: vec![sample_project(
                deps_root.clone(),
                deps_target.clone(),
                ProjectType::NodeJs,
                Category::Deps,
                RiskLevel::High,
                false,
                Utc::now() - Duration::days(30),
            )],
        };

        let deps_result = ApplyPlanService::new()
            .verify(
                &Config::default(),
                ApplyPlanRequest {
                    plan: deps_plan,
                    no_verify: false,
                    include_recent: false,
                    force: false,
                    force_protected: false,
                    recent_days: 7,
                },
            )
            .unwrap();
        assert_eq!(deps_result.verified_projects.len(), 0);
        assert_eq!(deps_result.skipped_projects.len(), 1);
        assert_eq!(
            deps_result.skipped_projects[0].skip_reason,
            Some(SkipReason::RuleMismatchOrMissing)
        );
    }
}
