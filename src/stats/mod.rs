use colored::Colorize;
use dev_cleaner_core::utils::format_size;
use prettytable::{format, Cell, Row, Table};

pub use dev_cleaner_core::Statistics;

pub fn display_terminal(stats: &Statistics, top_n: usize) {
    println!("\n{}", "📊 Dev Cleaner Statistics".bright_cyan().bold());
    println!("{}", "=".repeat(80).bright_black());

    display_overview(stats);
    display_by_type(stats);
    display_charts(stats);
    display_top_largest(stats, top_n);
    display_by_age(stats);
    display_recommendations(stats);

    println!();
}

fn display_overview(stats: &Statistics) {
    println!("\n{}", "📁 Overview".bright_green().bold());
    println!(
        "  Total projects: {}",
        stats.total_projects.to_string().bright_white()
    );
    println!(
        "  Cleanable space: {}",
        format_size(stats.total_size).bright_yellow()
    );
}

fn display_by_type(stats: &Statistics) {
    println!("\n{}", "📦 By Project Type".bright_green().bold());

    let mut table = Table::new();
    table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
    table.set_titles(Row::new(vec![
        Cell::new("Type"),
        Cell::new("Count"),
        Cell::new("Total Size"),
        Cell::new("Avg Size"),
    ]));

    let mut types: Vec<_> = stats.by_type.iter().collect();
    types.sort_by(|a, b| b.1.total_size.cmp(&a.1.total_size));

    for (type_name, type_stats) in types {
        table.add_row(Row::new(vec![
            Cell::new(type_name),
            Cell::new(&type_stats.count.to_string()),
            Cell::new(&format_size(type_stats.total_size)),
            Cell::new(&format_size(type_stats.avg_size)),
        ]));
    }

    table.printstd();
}

fn display_charts(stats: &Statistics) {
    println!("\n{}", "Charts".bright_green().bold());
    display_chart_by_type(stats, 8);
    display_chart_by_age(stats);
}

fn display_chart_by_type(stats: &Statistics, top_n: usize) {
    println!("  Size by project type");

    let mut types: Vec<_> = stats.by_type.iter().collect();
    if types.is_empty() {
        println!("  (no data)");
        return;
    }

    types.sort_by(|a, b| b.1.total_size.cmp(&a.1.total_size));
    let max_size = types
        .first()
        .map(|(_, type_stats)| type_stats.total_size)
        .unwrap_or(0);

    for (type_name, type_stats) in types.into_iter().take(top_n) {
        let bar = render_bar(type_stats.total_size, max_size, 24);
        let size_label = format_size(type_stats.total_size);
        println!("  {:<18} {:>10} {}", type_name, size_label, bar);
    }
}

fn display_chart_by_age(stats: &Statistics) {
    println!("\n  Size by age group");

    let groups = vec![
        ("Recent (<30d)", stats.by_age_group.recent),
        ("Medium (30-90d)", stats.by_age_group.medium),
        ("Old (>90d)", stats.by_age_group.old),
    ];

    let max_size = groups.iter().map(|(_, (_, size))| *size).max().unwrap_or(0);

    for (label, (_, size)) in groups {
        let bar = render_bar(size, max_size, 24);
        println!("  {:<18} {:>10} {}", label, format_size(size), bar);
    }
}

fn display_top_largest(stats: &Statistics, top_n: usize) {
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

    for (i, project) in stats.top_largest.iter().take(top_n).enumerate() {
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

fn display_by_age(stats: &Statistics) {
    println!("\n{}", "⏰ By Age Group".bright_green().bold());

    let (recent_count, recent_size) = stats.by_age_group.recent;
    let (medium_count, medium_size) = stats.by_age_group.medium;
    let (old_count, old_size) = stats.by_age_group.old;

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

fn display_recommendations(stats: &Statistics) {
    println!("\n{}", "💡 Recommendations".bright_green().bold());

    let (old_count, old_size) = stats.by_age_group.old;
    if old_count > 0 {
        println!(
            "  • {} old projects (>90 days) can likely be safely cleaned, freeing up {}",
            old_count,
            format_size(old_size).bright_yellow()
        );
    }

    if stats.top_largest.len() >= 5 && stats.total_size > 0 {
        let top5_size: u64 = stats.top_largest.iter().take(5).map(|p| p.size).sum();
        let percentage = (top5_size as f64 / stats.total_size as f64 * 100.0) as u32;
        println!(
            "  • Top 5 largest directories account for {}% of total space",
            percentage.to_string().bright_yellow()
        );
    }

    let (recent_count, _) = stats.by_age_group.recent;
    if recent_count > 0 {
        println!(
            "  • {} recent projects (<30 days) are likely still in use, consider keeping them",
            recent_count
        );
    }
}

fn render_bar(value: u64, max: u64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let filled = if max == 0 {
        0
    } else {
        ((value.saturating_mul(width as u64)) / max) as usize
    };
    let filled = filled.min(width);

    format!("{}{}", "#".repeat(filled), "-".repeat(width - filled))
}

#[cfg(test)]
mod tests {
    use super::*;
    use dev_cleaner_core::scanner::{Category, Confidence, ProjectType, RiskLevel};
    use dev_cleaner_core::ProjectInfo;
    use std::path::PathBuf;

    fn project(project_type: ProjectType, size: u64, days_since_modified: i64) -> ProjectInfo {
        ProjectInfo {
            root: PathBuf::from("/repo"),
            project_type,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from(format!("/repo/{project_type:?}-{size}")),
            size,
            size_calculated: true,
            last_modified: chrono::Utc::now() - chrono::Duration::days(days_since_modified),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn render_bar_handles_empty_and_scaled_values() {
        assert_eq!(render_bar(0, 0, 0), "");
        assert_eq!(render_bar(3, 0, 5), "-----");
        assert_eq!(render_bar(5, 10, 5), "##---");
    }

    #[test]
    fn display_terminal_smoke_covers_recommendations() {
        let stats = Statistics::from_projects(vec![
            project(ProjectType::NodeJs, 50, 120),
            project(ProjectType::Rust, 40, 15),
            project(ProjectType::Python, 30, 5),
            project(ProjectType::Java, 20, 200),
            project(ProjectType::Go, 10, 90),
        ]);

        display_terminal(&stats, 3);
    }
}
