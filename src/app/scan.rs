use crate::app::evaluated::{EvaluatedProject, SafetyFlags};
use crate::config::{Config, ScanProfile};
use crate::policy::KeepPolicy;
use crate::scanner::{Category, ProjectInfo, RiskLevel, Scanner};
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VisibilityOptions {
    pub include_protected: bool,
    pub include_recent: bool,
    pub recent_days: i64,
}

impl VisibilityOptions {
    pub fn is_visible(&self, project: &EvaluatedProject) -> bool {
        (self.include_protected || !project.safety.protected)
            && (self.include_recent || !project.safety.recent)
    }
}

impl Default for VisibilityOptions {
    fn default() -> Self {
        Self {
            include_protected: false,
            include_recent: false,
            recent_days: 7,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub path: Option<PathBuf>,
    pub profile: Option<String>,
    pub depth: Option<usize>,
    pub min_size_mb: Option<u64>,
    pub older_than_days: Option<i64>,
    pub gitignore: Option<bool>,
    pub category: Option<Category>,
    pub max_risk: Option<RiskLevel>,
    pub visibility: VisibilityOptions,
}

impl Default for ScanRequest {
    fn default() -> Self {
        Self {
            path: None,
            profile: None,
            depth: None,
            min_size_mb: None,
            older_than_days: None,
            gitignore: None,
            category: None,
            max_risk: None,
            visibility: VisibilityOptions::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedScanInput {
    pub roots: Vec<PathBuf>,
    pub scan_root: PathBuf,
    pub depth: Option<usize>,
    pub min_size_bytes: Option<u64>,
    pub older_than_days: Option<i64>,
    pub respect_gitignore: bool,
    pub category: Option<Category>,
    pub max_risk: RiskLevel,
    pub visibility: VisibilityOptions,
}

#[derive(Debug, Clone)]
pub struct DiscoveredProjects {
    pub resolved: ResolvedScanInput,
    pub projects: Vec<EvaluatedProject>,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ScanService;

impl ScanService {
    pub fn new() -> Self {
        Self
    }

    pub fn resolve_inputs(
        &self,
        config: &Config,
        request: &ScanRequest,
    ) -> Result<ResolvedScanInput> {
        let profile = self.resolve_profile(config, request.profile.as_deref())?;
        let roots = self.resolve_roots(request.path.as_ref(), profile)?;
        let scan_root = derive_scan_root(&roots);

        let depth = request
            .depth
            .or(profile.and_then(|p| p.depth))
            .or(config.default_depth);
        let min_size_mb = request
            .min_size_mb
            .or(profile.and_then(|p| p.min_size_mb))
            .or(config.min_size_mb);
        let older_than_days = request
            .older_than_days
            .or(profile.and_then(|p| p.max_age_days))
            .or(config.max_age_days);
        let respect_gitignore = request
            .gitignore
            .or(profile.and_then(|p| p.gitignore))
            .unwrap_or(false);
        let category = request.category.or(profile.and_then(|p| p.category));
        let max_risk = request
            .max_risk
            .or(profile.and_then(|p| p.max_risk))
            .unwrap_or(RiskLevel::Medium);

        Ok(ResolvedScanInput {
            roots,
            scan_root,
            depth,
            min_size_bytes: min_size_mb.map(|mb| mb.saturating_mul(1024 * 1024)),
            older_than_days,
            respect_gitignore,
            category,
            max_risk,
            visibility: request.visibility,
        })
    }

    pub fn build_scanner(
        &self,
        root: impl AsRef<Path>,
        config: &Config,
        resolved: &ResolvedScanInput,
    ) -> Scanner {
        let mut scanner = Scanner::new(root)
            .exclude_dirs(&config.exclude_dirs)
            .custom_patterns(&config.custom_patterns)
            .max_risk(resolved.max_risk);

        if let Some(category) = resolved.category {
            scanner = scanner.category(category);
        }
        if let Some(depth) = resolved.depth {
            scanner = scanner.max_depth(depth);
        }
        if let Some(min_size_bytes) = resolved.min_size_bytes {
            scanner = scanner.min_size(min_size_bytes);
        }
        if let Some(days) = resolved.older_than_days {
            scanner = scanner.max_age_days(days);
        }

        scanner.respect_gitignore(resolved.respect_gitignore)
    }

    pub fn evaluate_projects(
        &self,
        projects: Vec<ProjectInfo>,
        keep_policy: &KeepPolicy,
        recent_days: i64,
    ) -> Vec<EvaluatedProject> {
        projects
            .into_iter()
            .map(|info| {
                let decision = keep_policy.evaluate(&info);
                let safety = SafetyFlags {
                    protected: decision.protected,
                    protected_by: decision.reason,
                    recent: info.days_since_modified() < recent_days,
                };

                EvaluatedProject::new(info).with_safety(safety)
            })
            .collect()
    }

    pub fn evaluate_projects_with_config(
        &self,
        config: &Config,
        projects: Vec<ProjectInfo>,
        recent_days: i64,
    ) -> Vec<EvaluatedProject> {
        let keep_policy = KeepPolicy::from_config(config);
        self.evaluate_projects(projects, &keep_policy, recent_days)
    }

    pub fn evaluate_project_with_config(
        &self,
        config: &Config,
        project: ProjectInfo,
        recent_days: i64,
    ) -> EvaluatedProject {
        self.evaluate_projects_with_config(config, vec![project], recent_days)
            .into_iter()
            .next()
            .expect("single project evaluation should return one project")
    }

    pub fn filter_visible(
        &self,
        projects: Vec<EvaluatedProject>,
        visibility: VisibilityOptions,
    ) -> Vec<EvaluatedProject> {
        projects
            .into_iter()
            .filter(|project| visibility.is_visible(project))
            .collect()
    }

    pub fn deduplicate_projects(&self, projects: Vec<EvaluatedProject>) -> Vec<EvaluatedProject> {
        deduplicate_projects(projects)
    }

    pub fn discover(&self, config: &Config, request: &ScanRequest) -> Result<DiscoveredProjects> {
        let resolved = self.resolve_inputs(config, request)?;
        let keep_policy = KeepPolicy::from_config(config);
        let mut discovered = Vec::new();

        for root in &resolved.roots {
            let scanner = self.build_scanner(root, config, &resolved);
            let mut projects = scanner.scan()?;
            discovered.append(&mut projects);
        }

        let mut evaluated =
            self.evaluate_projects(discovered, &keep_policy, resolved.visibility.recent_days);
        evaluated = self.deduplicate_projects(evaluated);
        evaluated.sort_by(|a, b| b.info.size.cmp(&a.info.size));

        Ok(DiscoveredProjects {
            resolved,
            projects: evaluated,
        })
    }

    pub fn discover_visible(
        &self,
        config: &Config,
        request: &ScanRequest,
    ) -> Result<DiscoveredProjects> {
        let mut discovered = self.discover(config, request)?;
        discovered.projects =
            self.filter_visible(discovered.projects, discovered.resolved.visibility);
        Ok(discovered)
    }

    fn resolve_profile<'a>(
        &self,
        config: &'a Config,
        profile_name: Option<&str>,
    ) -> Result<Option<&'a ScanProfile>> {
        match profile_name {
            None => Ok(None),
            Some(name) => config
                .scan_profiles
                .get(name)
                .map(Some)
                .with_context(|| format!("Profile `{}` not found", name)),
        }
    }

    fn resolve_roots(
        &self,
        path: Option<&PathBuf>,
        profile: Option<&ScanProfile>,
    ) -> Result<Vec<PathBuf>> {
        match (path, profile) {
            (Some(_), Some(_)) => bail!("Use either [PATH] or --profile, not both"),
            (None, Some(profile)) => {
                if profile.paths.is_empty() {
                    bail!("Profile has no paths");
                }
                Ok(profile.paths.clone())
            }
            (Some(path), None) => Ok(vec![path.clone()]),
            (None, None) => Ok(vec![PathBuf::from(".")]),
        }
    }
}

pub fn canonicalize_lossy(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn common_ancestor(paths: &[PathBuf]) -> Option<PathBuf> {
    let first = paths.first()?;
    let mut ancestor = canonicalize_lossy(first);

    for path in &paths[1..] {
        let candidate = canonicalize_lossy(path);
        while !candidate.starts_with(&ancestor) {
            if !ancestor.pop() {
                return None;
            }
        }
    }

    Some(ancestor)
}

pub fn derive_scan_root(roots: &[PathBuf]) -> PathBuf {
    match roots.len() {
        0 => PathBuf::from("."),
        1 => canonicalize_lossy(&roots[0]),
        _ => common_ancestor(roots).unwrap_or_else(|| canonicalize_lossy(&roots[0])),
    }
}

fn deduplicate_projects(mut projects: Vec<EvaluatedProject>) -> Vec<EvaluatedProject> {
    projects.sort_by(|a, b| {
        let depth_a = a.info.cleanable_dir.components().count();
        let depth_b = b.info.cleanable_dir.components().count();
        depth_a
            .cmp(&depth_b)
            .then_with(|| a.info.cleanable_dir.cmp(&b.info.cleanable_dir))
    });

    let mut kept_paths = HashSet::new();
    let mut deduplicated = Vec::new();

    for project in projects {
        if kept_paths.contains(&project.info.cleanable_dir) {
            continue;
        }

        let mut ancestor = project.info.cleanable_dir.parent();
        let mut is_nested = false;

        while let Some(parent) = ancestor {
            if kept_paths.contains(parent) {
                is_nested = true;
                break;
            }
            ancestor = parent.parent();
        }

        if !is_nested {
            kept_paths.insert(project.info.cleanable_dir.clone());
            deduplicated.push(project);
        }
    }

    deduplicated
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub discovered: DiscoveredProjects,
    pub visible_projects: Vec<EvaluatedProject>,
}

impl ScanResult {
    pub fn new(discovered: DiscoveredProjects, visible_projects: Vec<EvaluatedProject>) -> Self {
        Self {
            discovered,
            visible_projects,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, ProjectType, RiskLevel};
    use crate::ProjectInfo;
    use chrono::Utc;
    fn sample_project() -> ProjectInfo {
        ProjectInfo {
            root: PathBuf::from("/repo/app"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from("/repo/app/target"),
            size: 123,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn resolve_inputs_prefers_profile_and_defaults() {
        let mut config = Config::default();
        config.default_depth = Some(3);
        config.min_size_mb = Some(10);
        config.max_age_days = Some(9);
        config.scan_profiles.insert(
            "demo".to_string(),
            ScanProfile {
                paths: vec![PathBuf::from("/one"), PathBuf::from("/two")],
                depth: Some(7),
                min_size_mb: Some(20),
                max_age_days: Some(11),
                gitignore: Some(true),
                category: Some(Category::Cache),
                max_risk: Some(RiskLevel::High),
            },
        );

        let request = ScanRequest {
            profile: Some("demo".to_string()),
            ..Default::default()
        };

        let resolved = ScanService::new()
            .resolve_inputs(&config, &request)
            .unwrap();
        assert_eq!(
            resolved.roots,
            vec![PathBuf::from("/one"), PathBuf::from("/two")]
        );
        assert_eq!(resolved.depth, Some(7));
        assert_eq!(resolved.min_size_bytes, Some(20 * 1024 * 1024));
        assert_eq!(resolved.older_than_days, Some(11));
        assert!(resolved.respect_gitignore);
        assert_eq!(resolved.category, Some(Category::Cache));
        assert_eq!(resolved.max_risk, RiskLevel::High);
    }

    #[test]
    fn visibility_filter_respects_flags() {
        let service = ScanService::new();
        let mut project = EvaluatedProject::new(sample_project());
        project.safety.protected = true;
        project.safety.recent = true;

        let visible = service.filter_visible(
            vec![project.clone()],
            VisibilityOptions {
                include_protected: false,
                include_recent: false,
                recent_days: 7,
            },
        );
        assert!(visible.is_empty());

        let visible = service.filter_visible(
            vec![project],
            VisibilityOptions {
                include_protected: true,
                include_recent: true,
                recent_days: 7,
            },
        );
        assert_eq!(visible.len(), 1);
    }

    #[test]
    fn deduplicate_projects_removes_exact_path_duplicates() {
        let service = ScanService::new();
        let first = EvaluatedProject::new(sample_project());
        let mut second = sample_project();
        second.size = 456;

        let deduplicated = service.deduplicate_projects(vec![first, EvaluatedProject::new(second)]);

        assert_eq!(deduplicated.len(), 1);
        assert_eq!(
            deduplicated[0].info.cleanable_dir,
            PathBuf::from("/repo/app/target")
        );
    }
}
