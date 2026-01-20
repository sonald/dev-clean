use crate::utils::format_size;
use crate::ProjectInfo;
use colored::Colorize;
use prettytable::{format, Cell, Row, Table};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Statistics about cleanable directories
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Statistics {
    /// Total size of all cleanable directories
    pub total_size: u64,

    /// Total number of projects
    pub total_projects: usize,

    /// Statistics grouped by project type
    pub by_type: HashMap<String, TypeStats>,

    /// Top N largest directories
    pub top_largest: Vec<ProjectStats>,

    /// Statistics grouped by age
    pub by_age_group: AgeGroupStats,
}

/// Statistics for a specific project type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeStats {
    /// Total size for this type
    pub total_size: u64,

    /// Number of projects of this type
    pub count: usize,

    /// Average size per project
    pub avg_size: u64,
}

/// Simplified project info for statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectStats {
    /// Path to cleanable directory
    pub path: String,

    /// Size in bytes
    pub size: u64,

    /// Project type
    pub project_type: String,

    /// Days since last modification
    pub age_days: i64,
}

/// Age-based grouping of statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgeGroupStats {
    /// Recent projects (<30 days): (count, total_size)
    pub recent: (usize, u64),

    /// Medium age projects (30-90 days): (count, total_size)
    pub medium: (usize, u64),

    /// Old projects (>90 days): (count, total_size)
    pub old: (usize, u64),
}

impl Statistics {
    /// Create statistics from a list of projects
    pub fn from_projects(projects: Vec<ProjectInfo>) -> Self {
        let total_projects = projects.len();
        let total_size: u64 = projects.iter().map(|p| p.size).sum();

        // Group by type
        let mut by_type: HashMap<String, TypeStats> = HashMap::new();
        for project in &projects {
            let type_name = project.project_type_display_name();
            let entry = by_type.entry(type_name.clone()).or_insert(TypeStats {
                total_size: 0,
                count: 0,
                avg_size: 0,
            });
            entry.total_size += project.size;
            entry.count += 1;
        }

        // Calculate average sizes
        for stats in by_type.values_mut() {
            stats.avg_size = if stats.count > 0 {
                stats.total_size / stats.count as u64
            } else {
                0
            };
        }

        // Create top largest list
        let mut sorted_projects = projects.clone();
        sorted_projects.sort_by(|a, b| b.size.cmp(&a.size));
        let top_largest: Vec<ProjectStats> = sorted_projects
            .iter()
            .map(|p| ProjectStats {
                path: p.cleanable_dir.display().to_string(),
                size: p.size,
                project_type: p.project_type_display_name(),
                age_days: p.days_since_modified(),
            })
            .collect();

        // Group by age
        let mut recent = (0, 0u64);
        let mut medium = (0, 0u64);
        let mut old = (0, 0u64);

        for project in &projects {
            let age = project.days_since_modified();
            if age < 30 {
                recent.0 += 1;
                recent.1 += project.size;
            } else if age < 90 {
                medium.0 += 1;
                medium.1 += project.size;
            } else {
                old.0 += 1;
                old.1 += project.size;
            }
        }

        let by_age_group = AgeGroupStats {
            recent,
            medium,
            old,
        };

        Self {
            total_size,
            total_projects,
            by_type,
            top_largest,
            by_age_group,
        }
    }

    /// Display statistics to terminal with formatted tables
    pub fn display_terminal(&self, top_n: usize) {
        println!("\n{}", "📊 Dev Cleaner Statistics".bright_cyan().bold());
        println!("{}", "=".repeat(80).bright_black());

        // Overview
        self.display_overview();

        // By Type
        self.display_by_type();

        // Top N Largest
        self.display_top_largest(top_n);

        // By Age
        self.display_by_age();

        // Recommendations
        self.display_recommendations();

        println!();
    }

    fn display_overview(&self) {
        println!("\n{}", "📁 Overview".bright_green().bold());
        println!(
            "  Total projects: {}",
            self.total_projects.to_string().bright_white()
        );
        println!(
            "  Cleanable space: {}",
            format_size(self.total_size).bright_yellow()
        );
    }

    fn display_by_type(&self) {
        println!("\n{}", "📦 By Project Type".bright_green().bold());

        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        table.set_titles(Row::new(vec![
            Cell::new("Type"),
            Cell::new("Count"),
            Cell::new("Total Size"),
            Cell::new("Avg Size"),
        ]));

        // Sort by total size
        let mut types: Vec<_> = self.by_type.iter().collect();
        types.sort_by(|a, b| b.1.total_size.cmp(&a.1.total_size));

        for (type_name, stats) in types {
            table.add_row(Row::new(vec![
                Cell::new(type_name),
                Cell::new(&stats.count.to_string()),
                Cell::new(&format_size(stats.total_size)),
                Cell::new(&format_size(stats.avg_size)),
            ]));
        }

        table.printstd();
    }

    fn display_top_largest(&self, top_n: usize) {
        println!(
            "\n{}",
            format!("🏆 Top {} Largest Directories", top_n)
                .bright_green()
                .bold()
        );

        let mut table = Table::new();
        table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
        table.set_titles(Row::new(vec![
            Cell::new("#"),
            Cell::new("Path"),
            Cell::new("Size"),
            Cell::new("Type"),
            Cell::new("Age"),
        ]));

        for (i, project) in self.top_largest.iter().take(top_n).enumerate() {
            // Shorten path if too long
            let path = if project.path.len() > 60 {
                format!("...{}", &project.path[project.path.len() - 57..])
            } else {
                project.path.clone()
            };

            table.add_row(Row::new(vec![
                Cell::new(&(i + 1).to_string()),
                Cell::new(&path),
                Cell::new(&format_size(project.size)),
                Cell::new(&project.project_type),
                Cell::new(&format!("{}d", project.age_days)),
            ]));
        }

        table.printstd();
    }

    fn display_by_age(&self) {
        println!("\n{}", "⏰ By Age Group".bright_green().bold());

        let (recent_count, recent_size) = self.by_age_group.recent;
        let (medium_count, medium_size) = self.by_age_group.medium;
        let (old_count, old_size) = self.by_age_group.old;

        println!(
            "  {} Recent (<30 days):   {} projects, {}",
            "📗".green(),
            recent_count,
            format_size(recent_size).bright_white()
        );
        println!(
            "  {} Medium (30-90 days): {} projects, {}",
            "📙".yellow(),
            medium_count,
            format_size(medium_size).bright_white()
        );
        println!(
            "  {} Old (>90 days):      {} projects, {}",
            "📕".red(),
            old_count,
            format_size(old_size).bright_white()
        );
    }

    fn display_recommendations(&self) {
        println!("\n{}", "💡 Recommendations".bright_green().bold());

        let (old_count, old_size) = self.by_age_group.old;
        if old_count > 0 {
            println!(
                "  • {} old projects (>90 days) can likely be safely cleaned, freeing up {}",
                old_count,
                format_size(old_size).bright_yellow()
            );
        }

        if self.top_largest.len() >= 5 {
            let top5_size: u64 = self.top_largest.iter().take(5).map(|p| p.size).sum();
            let percentage = (top5_size as f64 / self.total_size as f64 * 100.0) as u32;
            println!(
                "  • Top 5 largest directories account for {}% of total space",
                percentage.to_string().bright_yellow()
            );
        }

        let (recent_count, _) = self.by_age_group.recent;
        if recent_count > 0 {
            println!(
                "  • {} recent projects (<30 days) are likely still in use, consider keeping them",
                recent_count
            );
        }
    }

    /// Export statistics as JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectType;
    use chrono::Utc;
    use std::path::PathBuf;

    #[test]
    fn test_statistics_from_projects() {
        let projects = vec![
            ProjectInfo {
                root: PathBuf::from("/test1"),
                project_type: ProjectType::NodeJs,
                project_name: None,
                cleanable_dir: PathBuf::from("/test1/node_modules"),
                size: 1000000,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
            },
            ProjectInfo {
                root: PathBuf::from("/test2"),
                project_type: ProjectType::Rust,
                project_name: None,
                cleanable_dir: PathBuf::from("/test2/target"),
                size: 2000000,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
            },
        ];

        let stats = Statistics::from_projects(projects);

        assert_eq!(stats.total_projects, 2);
        assert_eq!(stats.total_size, 3000000);
        assert_eq!(stats.by_type.len(), 2);
        assert_eq!(stats.top_largest.len(), 2);
    }
}
