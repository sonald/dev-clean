use std::path::Path;
use serde::{Serialize, Deserialize};

/// Supported project types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProjectType {
    NodeJs,
    Rust,
    Python,
    Java,
    Kotlin,
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
            Self::Go => "cyan",
            Self::Ruby => "red",
            Self::Swift => "yellow",
            Self::Php => "magenta",
            Self::Elixir => "magenta",
            Self::DotNet => "blue",
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
            Self::Generic => "Generic",
        }
    }
}

/// Project type detector
pub struct ProjectDetector;

impl ProjectDetector {
    /// Detect project type by checking marker files
    pub fn detect(dir: &Path) -> Option<ProjectType> {
        if dir.join("package.json").exists() || dir.join("package-lock.json").exists() {
            return Some(ProjectType::NodeJs);
        }

        if dir.join("Cargo.toml").exists() {
            return Some(ProjectType::Rust);
        }

        if dir.join("requirements.txt").exists()
            || dir.join("setup.py").exists()
            || dir.join("pyproject.toml").exists()
            || dir.join("Pipfile").exists() {
            return Some(ProjectType::Python);
        }

        if dir.join("pom.xml").exists() {
            return Some(ProjectType::Maven);
        }

        if dir.join("build.gradle").exists() || dir.join("build.gradle.kts").exists() {
            return Some(ProjectType::Gradle);
        }

        if dir.join("go.mod").exists() {
            return Some(ProjectType::Go);
        }

        if dir.join("Gemfile").exists() {
            return Some(ProjectType::Ruby);
        }

        if dir.join("Package.swift").exists() {
            return Some(ProjectType::Swift);
        }

        if dir.join("composer.json").exists() {
            return Some(ProjectType::Php);
        }

        if dir.join("mix.exs").exists() {
            return Some(ProjectType::Elixir);
        }

        if dir.join("*.csproj").exists() || dir.join("*.sln").exists() {
            return Some(ProjectType::DotNet);
        }

        if dir.join("CMakeLists.txt").exists() || dir.join("Makefile").exists() {
            // Could be C or C++, default to C++
            return Some(ProjectType::Cpp);
        }

        None
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
            ],
            ProjectType::Rust => vec!["target"],
            ProjectType::Python => vec![
                ".venv",
                "venv",
                "__pycache__",
                ".pytest_cache",
                ".mypy_cache",
                ".tox",
                "*.egg-info",
                ".eggs",
                "build",
                "dist",
            ],
            ProjectType::Java | ProjectType::Maven => vec!["target", "out"],
            ProjectType::Kotlin | ProjectType::Gradle => vec!["build", ".gradle", "out"],
            ProjectType::Go => vec!["vendor", "bin"],
            ProjectType::C | ProjectType::Cpp => vec![
                "build",
                "cmake-build-debug",
                "cmake-build-release",
                "out",
            ],
            ProjectType::Ruby => vec!["vendor/bundle", ".bundle"],
            ProjectType::Swift => vec![".build", "DerivedData", ".swiftpm"],
            ProjectType::Php => vec!["vendor"],
            ProjectType::Elixir => vec!["_build", "deps"],
            ProjectType::DotNet => vec!["bin", "obj"],
            ProjectType::Generic => vec![],
        }
    }

    /// Check if a directory is currently in use based on lock files
    pub fn is_in_use(project_dir: &Path, project_type: ProjectType) -> bool {
        match project_type {
            ProjectType::NodeJs => {
                // Check if package-lock.json or yarn.lock was recently modified
                let lock_files = ["package-lock.json", "yarn.lock", "pnpm-lock.yaml"];
                Self::check_recent_lock_files(project_dir, &lock_files)
            }
            ProjectType::Rust => {
                Self::check_recent_lock_files(project_dir, &["Cargo.lock"])
            }
            ProjectType::Python => {
                Self::check_recent_lock_files(project_dir, &["Pipfile.lock", "poetry.lock"])
            }
            ProjectType::Go => {
                Self::check_recent_lock_files(project_dir, &["go.sum"])
            }
            ProjectType::Ruby => {
                Self::check_recent_lock_files(project_dir, &["Gemfile.lock"])
            }
            ProjectType::Php => {
                Self::check_recent_lock_files(project_dir, &["composer.lock"])
            }
            _ => false,
        }
    }

    fn check_recent_lock_files(dir: &Path, lock_files: &[&str]) -> bool {
        use std::time::{SystemTime, Duration};

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
}
