use crate::config::Config;
use crate::ProjectInfo;
use globset::{GlobBuilder, GlobMatcher, GlobSet, GlobSetBuilder};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ProtectionDecision {
    pub protected: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct KeepPolicy {
    keep_paths: Vec<PathBuf>,
    keep_glob_set: GlobSet,
    keep_project_root_set: GlobSet,
}

impl KeepPolicy {
    pub fn from_config(config: &Config) -> Self {
        // Scan sees paths as typed, apply sees them canonicalized: keep both spellings of each
        // rule so a symlinked path (macOS `/tmp` -> `/private/tmp`) is protected either way.
        let keep_paths = config
            .keep_paths
            .iter()
            .flat_map(|p| path_forms(&expand_tilde(p)))
            .collect::<Vec<_>>();
        let keep_glob_set = build_glob_set(&config.keep_globs);
        let keep_project_root_set = build_glob_set(&config.keep_project_roots);
        Self {
            keep_paths,
            keep_glob_set,
            keep_project_root_set,
        }
    }

    pub fn evaluate(&self, info: &ProjectInfo) -> ProtectionDecision {
        if info.root.join(".dev-cleaner-keep").exists() {
            return ProtectionDecision {
                protected: true,
                reason: Some("project_marker:.dev-cleaner-keep".to_string()),
            };
        }

        match self.matches_project_pattern_file(info) {
            Ok(Some(reason)) => {
                return ProtectionDecision {
                    protected: true,
                    reason: Some(reason),
                };
            }
            Ok(None) => {}
            Err(err) => {
                return ProtectionDecision {
                    protected: true,
                    reason: Some(format!(
                        "project_marker:.dev-cleaner-keep-patterns(parse_error:{})",
                        err
                    )),
                };
            }
        }

        let roots = path_forms(&info.root);
        let targets = path_forms(&info.cleanable_dir);

        if roots.iter().any(|root| self.matches_project_roots(root)) {
            return ProtectionDecision {
                protected: true,
                reason: Some("config_keep_project_roots".to_string()),
            };
        }

        if targets
            .iter()
            .chain(&roots)
            .any(|p| self.matches_keep_paths(p))
        {
            return ProtectionDecision {
                protected: true,
                reason: Some("config_keep_paths".to_string()),
            };
        }

        if targets
            .iter()
            .chain(&roots)
            .any(|p| self.matches_keep_globs(p))
        {
            return ProtectionDecision {
                protected: true,
                reason: Some("config_keep_globs".to_string()),
            };
        }

        ProtectionDecision {
            protected: false,
            reason: None,
        }
    }

    pub fn is_protected(&self, info: &ProjectInfo) -> bool {
        self.evaluate(info).protected
    }

    fn matches_project_pattern_file(&self, info: &ProjectInfo) -> anyhow::Result<Option<String>> {
        let pattern_file = info.root.join(".dev-cleaner-keep-patterns");
        if !pattern_file.is_file() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&pattern_file)?;
        for line in content.lines() {
            let pattern = line.trim();
            if pattern.is_empty() || pattern.starts_with('#') {
                continue;
            }

            if match_project_pattern(&info.root, &info.cleanable_dir, pattern)? {
                return Ok(Some(
                    "project_marker:.dev-cleaner-keep-patterns".to_string(),
                ));
            }
        }

        Ok(None)
    }

    fn matches_project_roots(&self, root: &Path) -> bool {
        self.keep_project_root_set.is_match(root)
    }

    fn matches_keep_paths(&self, path: &Path) -> bool {
        self.keep_paths
            .iter()
            .any(|p| is_same_or_child(path, p) || is_same_or_child(p, path))
    }

    fn matches_keep_globs(&self, path: &Path) -> bool {
        self.keep_glob_set.is_match(path)
    }
}

fn match_project_pattern(root: &Path, target: &Path, pattern: &str) -> anyhow::Result<bool> {
    let pattern = expand_tilde(pattern);

    if pattern.is_absolute() {
        return matches_pattern_path(pattern.to_string_lossy().as_ref(), target);
    }

    let rel_target = target
        .strip_prefix(root)
        .ok()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let rel_pattern = pattern.to_string_lossy().replace('\\', "/");

    if has_glob_chars(&rel_pattern) {
        let matcher = compile_glob_matcher(&rel_pattern)?;
        return Ok(matcher.is_match(&rel_target));
    }

    let literal = root.join(&rel_pattern);
    Ok(is_same_or_child(target, &literal))
}

fn matches_pattern_path(pattern: &str, target: &Path) -> anyhow::Result<bool> {
    if has_glob_chars(pattern) {
        let matcher = compile_glob_matcher(pattern)?;
        return Ok(matcher.is_match(target));
    }

    Ok(is_same_or_child(target, Path::new(pattern)))
}

fn compile_glob_matcher(pattern: &str) -> anyhow::Result<GlobMatcher> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    Ok(glob.compile_matcher())
}

/// `path`, plus its canonical form when that differs.
fn path_forms(path: &Path) -> Vec<PathBuf> {
    let mut forms = vec![path.to_path_buf()];
    if let Ok(canonical) = path.canonicalize() {
        if canonical != path {
            forms.push(canonical);
        }
    }
    forms
}

/// The glob with its literal leading directory canonicalized, when that changes it.
fn canonical_glob(pattern: &str) -> Option<String> {
    if !Path::new(pattern).is_absolute() {
        return None;
    }
    let glob_start = pattern.find(['*', '?', '['])?;
    let dir_end = pattern[..glob_start].rfind('/')?;
    let dir = Path::new(&pattern[..dir_end.max(1)]);
    let canonical = dir.canonicalize().ok()?;
    (canonical != dir).then(|| format!("{}{}", canonical.display(), &pattern[dir_end..]))
}

fn build_glob_set(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let pattern = expand_tilde(pattern).to_string_lossy().to_string();
        // Invalid patterns are rejected by `Config::validate` when the config is loaded.
        for pattern in std::iter::once(pattern.clone()).chain(canonical_glob(&pattern)) {
            if let Ok(glob) = GlobBuilder::new(&pattern).literal_separator(true).build() {
                builder.add(glob);
            }
        }
    }
    builder
        .build()
        .unwrap_or_else(|_| GlobSetBuilder::new().build().expect("empty glob set"))
}

fn has_glob_chars(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

fn is_same_or_child(path: &Path, parent: &Path) -> bool {
    path == parent || path.starts_with(parent)
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner::{Category, Confidence, ProjectType, RiskLevel};
    use chrono::Utc;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn project_info(root: &Path, cleanable_dir: &Path) -> ProjectInfo {
        ProjectInfo {
            root: root.to_path_buf(),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: cleanable_dir.to_path_buf(),
            size: 10,
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

    fn policy_from_config(config: Config) -> KeepPolicy {
        KeepPolicy::from_config(&config)
    }

    fn write_keep_patterns(root: &Path, content: &str) {
        fs::write(root.join(".dev-cleaner-keep-patterns"), content).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn keep_rules_match_through_symlinks_in_both_directions() {
        let temp = TempDir::new().unwrap();
        let real = temp.path().join("real");
        let root = real.join("app");
        fs::create_dir_all(root.join("target")).unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let info_at = |base: &Path| project_info(&base.join("app"), &base.join("app/target"));

        for (rule_base, seen_base) in [(&link, &real), (&real, &link)] {
            for config in [
                Config {
                    keep_paths: vec![rule_base.join("app").display().to_string()],
                    ..Default::default()
                },
                Config {
                    keep_globs: vec![format!("{}/*/target", rule_base.display())],
                    ..Default::default()
                },
            ] {
                let policy = KeepPolicy::from_config(&config);
                assert!(policy.evaluate(&info_at(seen_base)).protected, "{config:?}");
            }
        }
    }

    #[test]
    fn invalid_keep_glob_is_a_config_error() {
        let config = Config {
            keep_globs: vec!["/a/[unclosed".to_string()],
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn project_keep_marker_protects_target() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("app");
        let target = root.join("target");
        fs::create_dir_all(&target).unwrap();
        fs::write(root.join(".dev-cleaner-keep"), "").unwrap();

        let policy = policy_from_config(Config::default());
        let decision = policy.evaluate(&project_info(&root, &target));
        assert!(decision.protected);
    }

    #[test]
    fn keep_paths_match_exact_and_parent_child_pairs() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        let target = root.join("app").join("target");
        fs::create_dir_all(&target).unwrap();

        let mut config = Config {
            keep_paths: vec![target.display().to_string()],
            ..Default::default()
        };
        let policy = policy_from_config(config.clone());
        assert!(
            policy
                .evaluate(&project_info(&root.join("app"), &target))
                .protected
        );

        config.keep_paths = vec![root.display().to_string()];
        let policy = policy_from_config(config.clone());
        assert!(
            policy
                .evaluate(&project_info(&root.join("app"), &target))
                .protected
        );

        config.keep_paths = vec![target.display().to_string()];
        let policy = policy_from_config(config);
        assert!(
            policy
                .evaluate(&project_info(&root, &root.join("app")))
                .protected
        );
    }

    #[test]
    fn keep_globs_match_cleanable_dir_and_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        let project = root.join("app");
        let target = project.join("target");
        fs::create_dir_all(&target).unwrap();

        let mut config = Config {
            keep_globs: vec![format!("{}/*/target", root.display())],
            ..Default::default()
        };
        let policy = policy_from_config(config.clone());
        assert!(policy.evaluate(&project_info(&root, &target)).protected);

        config.keep_globs = vec![project.display().to_string()];
        let policy = policy_from_config(config);
        assert!(policy.evaluate(&project_info(&project, &target)).protected);
    }

    #[test]
    fn keep_project_roots_protects_project_root() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        let target = root.join("app").join("target");
        fs::create_dir_all(&target).unwrap();

        let config = Config {
            keep_project_roots: vec![root.join("app").display().to_string()],
            ..Default::default()
        };
        let policy = policy_from_config(config);

        let decision = policy.evaluate(&project_info(&root.join("app"), &target));
        assert!(decision.protected);
        assert_eq!(
            decision.reason.as_deref(),
            Some("config_keep_project_roots")
        );
    }

    #[test]
    fn keep_patterns_match_supported_forms_and_skip_comments() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        let target = root.join("app").join("target");
        let nested = target.join("nested");
        fs::create_dir_all(&nested).unwrap();

        write_keep_patterns(
            &root.join("app"),
            r#"
# keep the build target
target

# and one nested directory
target/*
"#,
        );

        let policy = policy_from_config(Config::default());
        let info = project_info(&root.join("app"), &target);
        assert!(policy.evaluate(&info).protected);

        let nested_info = project_info(&root.join("app"), &nested);
        assert!(policy.evaluate(&nested_info).protected);
    }

    #[test]
    fn keep_patterns_support_absolute_paths_and_non_matching_entries() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        let project = root.join("app");
        let target = project.join("target");
        fs::create_dir_all(&target).unwrap();

        write_keep_patterns(
            &project,
            &format!(
                r#"
#
{}
"#,
                target.display()
            ),
        );

        let policy = policy_from_config(Config::default());
        let info = project_info(&project, &target);
        assert!(policy.evaluate(&info).protected);

        let other_target = project.join("other-target");
        fs::create_dir_all(&other_target).unwrap();
        let other_info = project_info(&project, &other_target);
        assert!(!policy.evaluate(&other_info).protected);
    }

    #[test]
    fn keep_patterns_fail_closed_on_parse_error() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("workspace");
        let project = root.join("app");
        let target = project.join("target");
        fs::create_dir_all(&target).unwrap();

        write_keep_patterns(&project, "[");

        let policy = policy_from_config(Config::default());
        let decision = policy.evaluate(&project_info(&project, &target));
        assert!(decision.protected);
        assert!(decision
            .reason
            .as_deref()
            .unwrap_or_default()
            .contains("parse_error"));
    }

    #[test]
    fn expand_tilde_uses_home_directory() {
        let home = dirs::home_dir().expect("home directory should exist in tests");
        let expanded = expand_tilde("~/dev-cleaner-keep");
        assert_eq!(expanded, home.join("dev-cleaner-keep"));
    }
}
