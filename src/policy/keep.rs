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
        let keep_paths = config
            .keep_paths
            .iter()
            .map(|p| expand_tilde(p))
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

        if self.matches_project_roots(&info.root) {
            return ProtectionDecision {
                protected: true,
                reason: Some("config_keep_project_roots".to_string()),
            };
        }

        if self.matches_keep_paths(&info.cleanable_dir) || self.matches_keep_paths(&info.root) {
            return ProtectionDecision {
                protected: true,
                reason: Some("config_keep_paths".to_string()),
            };
        }

        if self.matches_keep_globs(&info.cleanable_dir) || self.matches_keep_globs(&info.root) {
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
        return Ok(matches_pattern_path(
            pattern.to_string_lossy().as_ref(),
            target,
        )?);
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

fn build_glob_set(patterns: &[String]) -> GlobSet {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let pattern = expand_tilde(pattern).to_string_lossy().to_string();
        let Ok(glob) = GlobBuilder::new(&pattern).literal_separator(true).build() else {
            continue;
        };
        builder.add(glob);
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
    use tempfile::TempDir;

    #[test]
    fn project_keep_marker_protects_target() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("app");
        let target = root.join("target");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(root.join(".dev-cleaner-keep"), "").unwrap();

        let info = ProjectInfo {
            root: root.clone(),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: target,
            size: 10,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        };

        let policy = KeepPolicy::from_config(&Config::default());
        let decision = policy.evaluate(&info);
        assert!(decision.protected);
    }
}
