use crate::config::{CustomPattern, MarkerMode};
use globset::GlobBuilder;
use ignore::gitignore::GitignoreBuilder;
use ignore::Match;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Supported project types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectType {
    NodeJs,
    Rust,
    Python,
    Java,
    Kotlin,
    Scala,
    Clojure,
    Dart,
    Haskell,
    Go,
    C,
    Cpp,
    Ruby,
    Swift,
    Php,
    Elixir,
    DotNet,
    Maven,
    Gradle,
    Terraform,
    Elm,
    Unity,
    Generic,
}

impl ProjectType {
    /// Returns the color code for CLI display
    pub fn color(&self) -> &'static str {
        match self {
            Self::NodeJs => "green",
            Self::Rust => "red",
            Self::Python => "blue",
            Self::Java | Self::Kotlin | Self::Maven | Self::Gradle => "cyan",
            Self::Scala => "red",
            Self::Clojure => "cyan",
            Self::Dart => "blue",
            Self::Haskell => "yellow",
            Self::Go => "cyan",
            Self::Ruby => "red",
            Self::Swift => "yellow",
            Self::Php => "magenta",
            Self::Elixir => "magenta",
            Self::DotNet => "blue",
            Self::Terraform => "magenta",
            Self::Elm => "cyan",
            _ => "white",
        }
    }

    /// Returns the display name
    pub fn name(&self) -> &'static str {
        match self {
            Self::NodeJs => "Node.js",
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::Java => "Java",
            Self::Kotlin => "Kotlin",
            Self::Scala => "Scala",
            Self::Clojure => "Clojure",
            Self::Dart => "Dart",
            Self::Haskell => "Haskell",
            Self::Go => "Go",
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Ruby => "Ruby",
            Self::Swift => "Swift",
            Self::Php => "PHP",
            Self::Elixir => "Elixir",
            Self::DotNet => ".NET",
            Self::Maven => "Maven",
            Self::Gradle => "Gradle",
            Self::Terraform => "Terraform",
            Self::Elm => "Elm",
            Self::Unity => "Unity",
            Self::Generic => "Generic",
        }
    }
}

/// Project type detector
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect the primary project type (the first of `detect_all`).
    pub fn detect(dir: &Path) -> Option<ProjectType> {
        Self::detect_all(dir).first().copied()
    }

    /// Detect every project type whose marker files are present, in priority order.
    ///
    /// Mixed roots (e.g. `package.json` + `pyproject.toml`) need the cleanable patterns of
    /// every ecosystem; picking only the first one hid `.venv` in Node+Python projects.
    pub fn detect_all(dir: &Path) -> Vec<ProjectType> {
        let has = |name: &str| dir.join(name).exists();
        let exts = dir_extensions(dir);
        let has_ext = |ext: &str| exts.iter().any(|e| e.eq_ignore_ascii_case(ext));

        let checks = [
            (
                has("package.json") || has("package-lock.json"),
                ProjectType::NodeJs,
            ),
            (has("Cargo.toml"), ProjectType::Rust),
            (
                has("requirements.txt")
                    || has("setup.py")
                    || has("pyproject.toml")
                    || has("Pipfile"),
                ProjectType::Python,
            ),
            (has("pom.xml"), ProjectType::Maven),
            (
                has("build.gradle") || has("build.gradle.kts"),
                ProjectType::Gradle,
            ),
            (has("build.sbt"), ProjectType::Scala),
            (has("project.clj") || has("deps.edn"), ProjectType::Clojure),
            (has("pubspec.yaml"), ProjectType::Dart),
            (has("stack.yaml") || has_ext("cabal"), ProjectType::Haskell),
            (has("go.mod"), ProjectType::Go),
            (has("Gemfile"), ProjectType::Ruby),
            (has("Package.swift"), ProjectType::Swift),
            (has("composer.json"), ProjectType::Php),
            (has("mix.exs"), ProjectType::Elixir),
            (has_ext("csproj") || has_ext("sln"), ProjectType::DotNet),
            (has_ext("tf"), ProjectType::Terraform),
            (has("elm.json"), ProjectType::Elm),
            (
                has("ProjectSettings/ProjectVersion.txt"),
                ProjectType::Unity,
            ),
            (has("CMakeLists.txt"), ProjectType::Cpp),
        ];
        let mut types: Vec<ProjectType> = checks
            .into_iter()
            .filter_map(|(hit, project_type)| hit.then_some(project_type))
            .collect();

        // A bare Makefile is too common (Go/Python/Node repos ship one) to add C++ patterns
        // like `out/` to a project already identified by a stronger marker.
        if types.is_empty() && has("Makefile") {
            types.push(ProjectType::Cpp);
        }
        types
    }

    /// Check if a directory is a CMake build directory (out-of-source build)
    ///
    /// This uses heuristic detection by checking for CMakeCache.txt, which is
    /// generated by CMake during configuration and only exists in build directories.
    ///
    /// Safety: Returns false if the directory also contains CMakeLists.txt, which
    /// indicates an in-source build where source code and build artifacts are mixed.
    /// We must NOT delete such directories as they contain source code.
    ///
    /// # Arguments
    /// * `dir` - The directory to check
    ///
    /// # Returns
    /// * `true` - Safe to delete (out-of-source build directory)
    /// * `false` - Not a build directory, or in-source build (unsafe to delete)
    pub fn is_cmake_build_dir(dir: &Path) -> bool {
        // Optimize for the common case: most directories don't have a CMake cache.
        if !dir.join("CMakeCache.txt").exists() {
            return false;
        }

        // Safety: Only pure build directories are safe to delete (cache present, no source).
        !dir.join("CMakeLists.txt").exists()
    }

    /// Get cleanable directories for a project type
    pub fn cleanable_dirs(project_type: ProjectType) -> Vec<&'static str> {
        match project_type {
            ProjectType::NodeJs => vec![
                "node_modules",
                ".next",
                ".nuxt",
                "dist",
                "build",
                ".cache",
                ".turbo",
                ".parcel-cache",
                ".vite",
                ".nyc_output",
                "storybook-static",
            ],
            ProjectType::Rust => vec!["target"],
            ProjectType::Python => vec![
                ".venv",
                "venv",
                "__pycache__",
                ".pytest_cache",
                ".mypy_cache",
                ".tox",
                ".hypothesis",
                "*.egg-info",
                ".eggs",
                "build",
                "dist",
            ],
            ProjectType::Java | ProjectType::Maven => vec!["target", "out"],
            ProjectType::Kotlin | ProjectType::Gradle => vec!["build", ".gradle", "out"],
            ProjectType::Scala => vec!["target", "project/target"],
            ProjectType::Clojure => vec!["target"],
            ProjectType::Dart => vec!["build", ".dart_tool"],
            ProjectType::Haskell => vec!["dist", "dist-newstyle", ".stack-work"],
            ProjectType::Go => vec!["vendor", "bin"],
            ProjectType::C | ProjectType::Cpp => {
                vec!["build", "cmake-build-debug", "cmake-build-release", "out"]
            }
            ProjectType::Ruby => vec!["vendor/bundle", ".bundle"],
            ProjectType::Swift => vec![".build", "DerivedData", ".swiftpm"],
            ProjectType::Php => vec!["vendor"],
            ProjectType::Elixir => vec!["_build", "deps"],
            ProjectType::DotNet => vec!["bin", "obj"],
            ProjectType::Terraform => vec![".terraform"],
            ProjectType::Elm => vec!["elm-stuff"],
            ProjectType::Unity => vec!["Library", "Temp", "obj"],
            ProjectType::Generic => vec![],
        }
    }

    /// True when `dir` holds files tracked by git (it is source, not a build artifact).
    ///
    /// Returns false outside a work tree or when git cannot run, so callers keep their
    /// normal rules. Only spawns git when a `.git` entry exists on the way up, which keeps
    /// scans of non-repo folders free of process launches.
    pub fn contains_tracked_files(dir: &Path) -> bool {
        let Some(work_tree) = dir.ancestors().find(|p| p.join(".git").exists()) else {
            return false;
        };
        let Ok(relative) = dir.strip_prefix(work_tree) else {
            return false;
        };
        let pathspec = if relative.as_os_str().is_empty() {
            Path::new(".")
        } else {
            relative
        };
        std::process::Command::new("git")
            .arg("--literal-pathspecs")
            .arg("-C")
            .arg(work_tree)
            .args(["ls-files", "-z", "--"])
            .arg(pathspec)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .output()
            .map(|out| out.status.success() && !out.stdout.is_empty())
            .unwrap_or(false)
    }

    /// Check if a directory is currently in use based on lock files
    pub fn is_in_use(project_dir: &Path, project_type: ProjectType) -> bool {
        match project_type {
            ProjectType::NodeJs => {
                // Check if package-lock.json or yarn.lock was recently modified
                let lock_files = [
                    "package-lock.json",
                    "npm-shrinkwrap.json",
                    "yarn.lock",
                    "pnpm-lock.yaml",
                    "bun.lock",
                    "bun.lockb",
                ];
                Self::check_recent_lock_files(project_dir, &lock_files)
            }
            ProjectType::Rust => Self::check_recent_lock_files(project_dir, &["Cargo.lock"]),
            ProjectType::Python => Self::check_recent_lock_files(
                project_dir,
                &["Pipfile.lock", "poetry.lock", "uv.lock", "pdm.lock"],
            ),
            ProjectType::Dart => Self::check_recent_lock_files(project_dir, &["pubspec.lock"]),
            ProjectType::Haskell => {
                Self::check_recent_lock_files(project_dir, &["stack.yaml.lock"])
            }
            ProjectType::Go => Self::check_recent_lock_files(project_dir, &["go.sum"]),
            ProjectType::Ruby => Self::check_recent_lock_files(project_dir, &["Gemfile.lock"]),
            ProjectType::Php => Self::check_recent_lock_files(project_dir, &["composer.lock"]),
            ProjectType::Swift => Self::check_recent_lock_files(project_dir, &["Package.resolved"]),
            ProjectType::Elixir => Self::check_recent_lock_files(project_dir, &["mix.lock"]),
            ProjectType::DotNet => {
                Self::check_recent_lock_files(project_dir, &["packages.lock.json"])
            }
            ProjectType::Terraform => {
                Self::check_recent_lock_files(project_dir, &[".terraform.lock.hcl"])
            }
            // The editor holds this file for as long as the project is open.
            ProjectType::Unity => project_dir.join("Temp/UnityLockfile").exists(),
            _ => false,
        }
    }

    fn check_recent_lock_files(dir: &Path, lock_files: &[&str]) -> bool {
        use std::time::{Duration, SystemTime};

        for lock_file in lock_files {
            if let Ok(metadata) = dir.join(lock_file).metadata() {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                        // Consider in use if modified within last 7 days
                        if elapsed < Duration::from_secs(7 * 24 * 60 * 60) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Parse .gitignore file and extract potential cleanable directory patterns
    pub fn parse_gitignore(project_dir: &Path) -> Vec<String> {
        let gitignore_path = project_dir.join(".gitignore");

        if !gitignore_path.exists() {
            return Vec::new();
        }

        let content = match fs::read_to_string(&gitignore_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        let mut cleanable_patterns = Vec::new();

        for line in content.lines() {
            if let Some(pattern) = Self::gitignore_discovery_pattern(line) {
                cleanable_patterns.push(pattern);
            }
        }

        cleanable_patterns
    }

    pub(crate) fn gitignore_discovery_pattern(line: &str) -> Option<String> {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            return None;
        }

        // Skip negation patterns (starting with !)
        if line.starts_with('!') {
            return None;
        }

        // Skip file patterns (contains extension or is clearly a file)
        if line.contains('.') && !line.ends_with('/') && !line.starts_with('.') {
            // This might be a file pattern like *.log, skip it
            return None;
        }

        // Clean up the pattern
        let mut pattern = line.to_string();

        // Remove leading slash
        if pattern.starts_with('/') {
            pattern = pattern[1..].to_string();
        }

        // Remove trailing slash
        if pattern.ends_with('/') {
            pattern = pattern[..pattern.len() - 1].to_string();
        }

        // Skip patterns with wildcards in the middle (too complex)
        if pattern.contains('*') && !pattern.starts_with('*') && !pattern.ends_with('*') {
            return None;
        }

        // Skip obviously protected directories
        if matches!(
            pattern.as_str(),
            ".git" | ".svn" | ".hg" | "." | ".." | "src" | "lib" | "include"
        ) {
            return None;
        }

        // Skip common file patterns (even if they start with .)
        if matches!(
            pattern.as_str(),
            ".env"
                | ".DS_Store"
                | ".gitignore"
                | ".gitattributes"
                | ".editorconfig"
                | ".npmrc"
                | ".yarnrc"
        ) {
            return None;
        }

        // Only include patterns that look like directories
        // Typically: no extension, or starts with . (hidden dirs)
        if !pattern.is_empty() && (pattern.starts_with('.') || !pattern.contains('.')) {
            return Some(pattern);
        }

        None
    }

    /// Get cleanable directories including both default patterns and .gitignore patterns
    pub fn cleanable_dirs_with_gitignore(
        project_type: ProjectType,
        project_dir: &Path,
    ) -> Vec<String> {
        let mut cleanable = Self::cleanable_dirs(project_type)
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<String>>();

        // Add patterns from .gitignore
        let gitignore_patterns = Self::parse_gitignore(project_dir);

        for pattern in gitignore_patterns {
            // Only add if not already in the list
            if !cleanable.contains(&pattern) {
                cleanable.push(pattern);
            }
        }

        cleanable
    }

    /// Explain why a directory is considered cleanable.
    ///
    /// This is primarily intended for CLI output (`scan --explain`).
    pub fn explain_cleanable_dir(
        project_type: ProjectType,
        project_root: &Path,
        cleanable_dir: &Path,
        custom_patterns: &[CustomPattern],
    ) -> String {
        let basename = cleanable_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or_default();

        let relative_path = cleanable_dir
            .strip_prefix(project_root)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|| cleanable_dir.display().to_string());

        // Prefer user config custom patterns.
        for pattern in custom_patterns {
            if !custom_root_matches(project_root, pattern) {
                continue;
            }
            if pattern_matches(&pattern.directory, basename, &relative_path) {
                return format!(
                    "matched config custom pattern `{}` directory `{}`",
                    pattern.name, pattern.directory
                );
            }
        }

        // Then built-in patterns.
        for pattern in Self::cleanable_dirs(project_type) {
            if pattern_matches(pattern, basename, &relative_path) {
                return format!("matched builtin pattern `{}`", pattern);
            }
        }

        // Then `.gitignore`-derived patterns (conservative discovery).
        if let Some(pattern) = explain_gitignore_discovery_pattern(project_root, cleanable_dir) {
            return format!("matched .gitignore pattern `{}`", pattern);
        }

        // Finally, heuristics.
        if Self::is_cmake_build_dir(cleanable_dir) {
            return "matched CMake out-of-source build heuristic (CMakeCache.txt present, no CMakeLists.txt)".to_string();
        }

        "matched cleanable rules".to_string()
    }
}

fn explain_gitignore_discovery_pattern(project_root: &Path, dir: &Path) -> Option<String> {
    let mut builder = GitignoreBuilder::new(project_root);
    let path = project_root.join(".gitignore");
    if !path.is_file() {
        return None;
    }
    let _ = builder.add(path);
    let gi = builder.build().ok()?;
    match gi.matched(dir, /* is_dir */ true) {
        Match::Ignore(glob) => ProjectDetector::gitignore_discovery_pattern(glob.original()),
        Match::None | Match::Whitelist(_) => None,
    }
}

fn pattern_matches(pattern: &str, basename: &str, relative_path: &str) -> bool {
    let pattern = pattern.replace('\\', "/");
    let text = if pattern.contains('/') {
        relative_path
    } else {
        basename
    };

    let glob = match GlobBuilder::new(&pattern).literal_separator(true).build() {
        Ok(g) => g,
        Err(_) => return false,
    };

    glob.compile_matcher().is_match(text)
}

fn custom_root_matches(project_root: &Path, custom: &CustomPattern) -> bool {
    if custom.marker_files.is_empty() {
        return false;
    }

    match custom.marker_mode {
        MarkerMode::AnyOf => custom
            .marker_files
            .iter()
            .any(|marker| project_root.join(marker).exists()),
        MarkerMode::AllOf => custom
            .marker_files
            .iter()
            .all(|marker| project_root.join(marker).exists()),
    }
}

fn dir_extensions(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|ft| ft.is_file()))
        .filter_map(|entry| {
            Path::new(&entry.file_name())
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_string)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use std::fs;
    use std::process::Command;
    use tempfile::TempDir;

    #[test]
    fn test_in_use_detects_modern_lock_files() {
        let temp = TempDir::new().unwrap();
        for (lock, project_type) in [
            ("uv.lock", ProjectType::Python),
            ("bun.lock", ProjectType::NodeJs),
            ("Package.resolved", ProjectType::Swift),
            ("mix.lock", ProjectType::Elixir),
            ("packages.lock.json", ProjectType::DotNet),
        ] {
            let dir = temp.path().join(lock.replace('.', "-"));
            fs::create_dir_all(&dir).unwrap();
            assert!(!ProjectDetector::is_in_use(&dir, project_type), "{lock}");
            fs::write(dir.join(lock), "").unwrap();
            assert!(ProjectDetector::is_in_use(&dir, project_type), "{lock}");
        }
    }

    fn touch_with_timestamp(path: &Path, timestamp: &str) {
        let status = Command::new("touch")
            .args(["-t", timestamp, path.to_str().unwrap()])
            .status()
            .expect("failed to run touch");
        assert!(status.success());
    }

    fn touch_days_ago(path: &Path, days: i64) {
        let timestamp = (Utc::now() - Duration::days(days))
            .format("%Y%m%d%H%M")
            .to_string();
        touch_with_timestamp(path, &timestamp);
    }

    #[test]
    fn test_detect_dotnet_project() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        fs::write(dir.join("app.csproj"), "<Project></Project>").unwrap();
        assert_eq!(ProjectDetector::detect(dir), Some(ProjectType::DotNet));
    }

    #[test]
    fn test_detect_scala_project() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        fs::write(dir.join("build.sbt"), "name := \"demo\"").unwrap();
        assert_eq!(ProjectDetector::detect(dir), Some(ProjectType::Scala));
    }

    #[test]
    fn test_detect_clojure_project() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        fs::write(dir.join("deps.edn"), "{:deps {}}").unwrap();
        assert_eq!(ProjectDetector::detect(dir), Some(ProjectType::Clojure));
    }

    #[test]
    fn test_detect_dart_project() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        fs::write(dir.join("pubspec.yaml"), "name: demo").unwrap();
        assert_eq!(ProjectDetector::detect(dir), Some(ProjectType::Dart));
    }

    #[test]
    fn test_detect_haskell_project() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path();

        fs::write(dir.join("demo.cabal"), "name: demo").unwrap();
        assert_eq!(ProjectDetector::detect(dir), Some(ProjectType::Haskell));
    }

    #[test]
    fn test_parse_gitignore() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        let gitignore_content = r#"
# Node dependencies
node_modules/
dist/

# Cache directories
.cache/
.temp
.custom-cache/

# Files (should be skipped)
*.log
.env
README.md

# Protected dirs (should be skipped)
.git/
src/
lib/

# Complex patterns (should be skipped)
**/temp/*
"#;
        fs::write(project_dir.join(".gitignore"), gitignore_content).unwrap();

        let patterns = ProjectDetector::parse_gitignore(project_dir);

        assert!(patterns.contains(&"node_modules".to_string()));
        assert!(patterns.contains(&"dist".to_string()));
        assert!(patterns.contains(&".cache".to_string()));
        assert!(patterns.contains(&".temp".to_string()));
        assert!(patterns.contains(&".custom-cache".to_string()));
        assert!(!patterns.iter().any(|p| p.contains(".log")));
        assert!(!patterns.iter().any(|p| p.contains(".env")));
        assert!(!patterns.iter().any(|p| p.contains("README")));
        assert!(!patterns.contains(&".git".to_string()));
        assert!(!patterns.contains(&"src".to_string()));
        assert!(!patterns.contains(&"lib".to_string()));
    }

    #[test]
    fn test_gitignore_discovery_pattern_edge_cases() {
        assert_eq!(ProjectDetector::gitignore_discovery_pattern("!dist/"), None);
        assert_eq!(
            ProjectDetector::gitignore_discovery_pattern("/dist/"),
            Some("dist".to_string())
        );
        assert_eq!(
            ProjectDetector::gitignore_discovery_pattern(".cache/"),
            Some(".cache".to_string())
        );
        assert_eq!(ProjectDetector::gitignore_discovery_pattern("src/"), None);
        assert_eq!(ProjectDetector::gitignore_discovery_pattern(".git/"), None);
        assert_eq!(
            ProjectDetector::gitignore_discovery_pattern("**/temp/*"),
            Some("**/temp/*".to_string())
        );
        assert_eq!(ProjectDetector::gitignore_discovery_pattern("*.log"), None);
    }

    #[test]
    fn test_is_in_use_recent_and_stale_lock_files() {
        let temp = TempDir::new().unwrap();
        let node_dir = temp.path().join("node");
        let rust_dir = temp.path().join("rust");
        let python_dir = temp.path().join("python");
        let generic_dir = temp.path().join("generic");
        fs::create_dir_all(&node_dir).unwrap();
        fs::create_dir_all(&rust_dir).unwrap();
        fs::create_dir_all(&python_dir).unwrap();
        fs::create_dir_all(&generic_dir).unwrap();

        fs::write(node_dir.join("package-lock.json"), "{}").unwrap();
        fs::write(node_dir.join("yarn.lock"), "{}").unwrap();
        touch_days_ago(&node_dir.join("package-lock.json"), 30);
        touch_days_ago(&node_dir.join("yarn.lock"), 1);

        fs::write(rust_dir.join("Cargo.lock"), "# lock").unwrap();
        touch_days_ago(&rust_dir.join("Cargo.lock"), 30);

        fs::write(python_dir.join("Pipfile.lock"), "{}").unwrap();
        fs::write(python_dir.join("poetry.lock"), "{}").unwrap();
        touch_days_ago(&python_dir.join("Pipfile.lock"), 30);
        touch_days_ago(&python_dir.join("poetry.lock"), 1);

        assert!(ProjectDetector::is_in_use(&node_dir, ProjectType::NodeJs));
        assert!(!ProjectDetector::is_in_use(&rust_dir, ProjectType::Rust));
        assert!(ProjectDetector::is_in_use(&python_dir, ProjectType::Python));
        assert!(!ProjectDetector::is_in_use(
            &generic_dir,
            ProjectType::Generic
        ));
    }

    #[test]
    fn test_is_in_use_seven_day_boundary_is_not_recent() {
        let temp = TempDir::new().unwrap();
        let node_dir = temp.path().join("node");
        fs::create_dir_all(&node_dir).unwrap();
        fs::write(node_dir.join("package-lock.json"), "{}").unwrap();
        touch_days_ago(&node_dir.join("package-lock.json"), 7);

        assert!(!ProjectDetector::is_in_use(&node_dir, ProjectType::NodeJs));
    }

    #[test]
    fn test_explain_cleanable_dir_priority() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("Assets"), "").unwrap();
        fs::write(root.join("ProjectSettings"), "").unwrap();
        fs::write(root.join(".gitignore"), "mybuild/\n").unwrap();

        let custom_patterns = vec![CustomPattern {
            name: "Unity".to_string(),
            directory: "build".to_string(),
            marker_files: vec!["Assets".to_string(), "ProjectSettings".to_string()],
            marker_mode: MarkerMode::AllOf,
        }];

        let custom_dir = root.join("build");
        fs::create_dir(&custom_dir).unwrap();
        let builtin_dir = root.join("dist");
        fs::create_dir(&builtin_dir).unwrap();
        let gitignore_dir = root.join("mybuild");
        fs::create_dir(&gitignore_dir).unwrap();
        fs::write(gitignore_dir.join("CMakeCache.txt"), "# cache").unwrap();
        let heuristic_dir = root.join("heuristicbuild");
        fs::create_dir(&heuristic_dir).unwrap();
        fs::write(heuristic_dir.join("CMakeCache.txt"), "# cache").unwrap();

        let custom = ProjectDetector::explain_cleanable_dir(
            ProjectType::Python,
            root,
            &custom_dir,
            &custom_patterns,
        );
        assert!(custom.contains("matched config custom pattern `Unity`"));

        let builtin =
            ProjectDetector::explain_cleanable_dir(ProjectType::NodeJs, root, &builtin_dir, &[]);
        assert_eq!(builtin, "matched builtin pattern `dist`");

        let gitignore =
            ProjectDetector::explain_cleanable_dir(ProjectType::Generic, root, &gitignore_dir, &[]);
        assert_eq!(gitignore, "matched .gitignore pattern `mybuild`");

        let heuristic =
            ProjectDetector::explain_cleanable_dir(ProjectType::Generic, root, &heuristic_dir, &[]);
        assert_eq!(
            heuristic,
            "matched CMake out-of-source build heuristic (CMakeCache.txt present, no CMakeLists.txt)"
        );
    }

    #[test]
    fn test_cleanable_dirs_with_gitignore() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        // Create a Node.js project
        fs::write(project_dir.join("package.json"), "{}").unwrap();

        // Create a .gitignore with custom patterns
        let gitignore_content = r#"
node_modules/
dist/
.custom-cache/
"#;
        fs::write(project_dir.join(".gitignore"), gitignore_content).unwrap();

        let cleanable =
            ProjectDetector::cleanable_dirs_with_gitignore(ProjectType::NodeJs, project_dir);

        // Should include default Node.js patterns
        assert!(cleanable.iter().any(|d| d == "node_modules"));
        assert!(cleanable.iter().any(|d| d == ".next"));

        // Should also include .gitignore patterns
        assert!(cleanable.iter().any(|d| d == "dist"));
        assert!(cleanable.iter().any(|d| d == ".custom-cache"));
    }

    #[test]
    fn test_parse_gitignore_no_file() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path();

        let patterns = ProjectDetector::parse_gitignore(project_dir);
        assert!(patterns.is_empty());
    }

    #[test]
    fn test_is_cmake_build_dir_out_of_source() {
        let temp = TempDir::new().unwrap();
        let build_dir = temp.path().join("mybuild");
        fs::create_dir(&build_dir).unwrap();

        // Create CMakeCache.txt (build artifact)
        fs::write(build_dir.join("CMakeCache.txt"), "# CMake cache").unwrap();

        // Should be recognized as cleanable build directory
        assert!(ProjectDetector::is_cmake_build_dir(&build_dir));
    }

    #[test]
    fn test_is_cmake_build_dir_in_source() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("myproject");
        fs::create_dir(&project_dir).unwrap();

        // Create both CMakeLists.txt (source) and CMakeCache.txt (build)
        // This indicates in-source build - should NOT be deletable
        fs::write(project_dir.join("CMakeLists.txt"), "project(test)").unwrap();
        fs::write(project_dir.join("CMakeCache.txt"), "# CMake cache").unwrap();

        // Should NOT be recognized as cleanable (source + build mixed)
        assert!(!ProjectDetector::is_cmake_build_dir(&project_dir));
    }

    #[test]
    fn test_is_cmake_build_dir_no_cache() {
        let temp = TempDir::new().unwrap();
        let dir = temp.path().join("somedir");
        fs::create_dir(&dir).unwrap();

        // Directory without CMakeCache.txt
        assert!(!ProjectDetector::is_cmake_build_dir(&dir));
    }

    #[test]
    fn test_is_cmake_build_dir_source_only() {
        let temp = TempDir::new().unwrap();
        let project_dir = temp.path().join("myproject");
        fs::create_dir(&project_dir).unwrap();

        // Only source file, no build artifacts
        fs::write(project_dir.join("CMakeLists.txt"), "project(test)").unwrap();

        // Should NOT be recognized as cleanable (source only)
        assert!(!ProjectDetector::is_cmake_build_dir(&project_dir));
    }
}
