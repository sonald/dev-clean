use crate::cleaner::CleanOptions;
use crate::scanner::ProjectDetector;
use crate::trash::{default_trash_root, latest_batch_id, restore_batch};
use crate::utils::format_size;
use crate::{Cleaner, CleanupPlan, Config, ProjectInfo, Scanner};
use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "dev-cleaner")]
#[command(version, about = "A smart developer tool for cleaning temporary build directories", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Config file path
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan directories for cleanable projects
    Scan {
        /// Directory to scan
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum scan depth
        #[arg(short, long)]
        depth: Option<usize>,

        /// Minimum size in MB
        #[arg(long)]
        min_size: Option<u64>,

        /// Older than N days
        #[arg(long)]
        older_than: Option<i64>,

        /// Respect .gitignore files (skips gitignored directories)
        #[arg(long)]
        gitignore: bool,

        /// Output scan results as JSON (machine-readable)
        #[arg(long)]
        json: bool,

        /// Print the matching rule for each result
        #[arg(long)]
        explain: bool,
    },

    /// Clean project directories
    Clean {
        /// Directory to scan
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum scan depth
        #[arg(short, long)]
        depth: Option<usize>,

        /// Minimum size in MB
        #[arg(long)]
        min_size: Option<u64>,

        /// Older than N days
        #[arg(long)]
        older_than: Option<i64>,

        /// Dry run - don't actually delete
        #[arg(long)]
        dry_run: bool,

        /// Move directories to Dev Cleaner's trash (undoable) instead of deleting
        #[arg(long)]
        trash: bool,

        /// Auto mode - clean all matching without confirmation
        #[arg(long)]
        auto: bool,

        /// Force mode - skip all confirmations
        #[arg(short, long)]
        force: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Respect .gitignore files (skips gitignored directories)
        #[arg(long)]
        gitignore: bool,
    },

    /// Launch interactive TUI mode
    Tui {
        /// Directory to scan
        #[arg(default_value = ".")]
        path: PathBuf,
    },

    /// Show statistics about cleanable directories
    Stats {
        /// Directory to scan
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum scan depth
        #[arg(short, long)]
        depth: Option<usize>,

        /// Number of top largest directories to show
        #[arg(long, default_value = "10")]
        top: usize,

        /// Export as JSON
        #[arg(long)]
        json: bool,

        /// Respect .gitignore files (skips gitignored directories)
        #[arg(long)]
        gitignore: bool,
    },

    /// Generate default config file
    InitConfig {
        /// Output path for config file
        path: Option<PathBuf>,
    },

    /// Generate a cleanup plan as JSON
    Plan {
        /// Directory to scan
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Maximum scan depth
        #[arg(short, long)]
        depth: Option<usize>,

        /// Minimum size in MB
        #[arg(long)]
        min_size: Option<u64>,

        /// Older than N days
        #[arg(long)]
        older_than: Option<i64>,

        /// Respect .gitignore files (skips gitignored directories)
        #[arg(long)]
        gitignore: bool,

        /// Output file path (prints to stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Apply a cleanup plan JSON file
    Apply {
        /// Path to plan JSON file
        plan: PathBuf,

        /// Dry run - don't actually delete
        #[arg(long)]
        dry_run: bool,

        /// Move directories to Dev Cleaner's trash (undoable) instead of deleting
        #[arg(long)]
        trash: bool,

        /// Force mode - skip confirmation and allow in-use cleaning
        #[arg(short, long)]
        force: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },

    /// Undo a trash batch (restore directories)
    Undo {
        /// Batch id to restore (defaults to latest)
        #[arg(long)]
        batch: Option<String>,

        /// Dry run - don't actually restore
        #[arg(long)]
        dry_run: bool,

        /// Force mode - overwrite existing targets
        #[arg(short, long)]
        force: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let config = if let Some(config_path) = &self.config {
            Config::load(config_path)?
        } else {
            Config::load_or_default(Config::default_path())?
        };

        match self.command {
            Commands::Scan {
                path,
                depth,
                min_size,
                older_than,
                gitignore,
                json,
                explain,
            } => {
                run_scan(
                    path,
                    depth.or(config.default_depth),
                    min_size.or(config.min_size_mb),
                    older_than.or(config.max_age_days),
                    gitignore,
                    json,
                    explain,
                )?;
            }
            Commands::Clean {
                path,
                depth,
                min_size,
                older_than,
                dry_run,
                trash,
                auto,
                force,
                verbose,
                gitignore,
            } => {
                run_clean(
                    path,
                    depth.or(config.default_depth),
                    min_size.or(config.min_size_mb),
                    older_than.or(config.max_age_days),
                    dry_run,
                    trash,
                    auto,
                    force,
                    verbose,
                    gitignore,
                )?;
            }
            Commands::Tui { path } => {
                crate::tui::run_tui(path)?;
            }
            Commands::Stats {
                path,
                depth,
                top,
                json,
                gitignore,
            } => {
                run_stats(path, depth.or(config.default_depth), top, json, gitignore)?;
            }
            Commands::InitConfig { path } => {
                init_config(path)?;
            }
            Commands::Plan {
                path,
                depth,
                min_size,
                older_than,
                gitignore,
                output,
            } => {
                run_plan(
                    path,
                    depth.or(config.default_depth),
                    min_size.or(config.min_size_mb),
                    older_than.or(config.max_age_days),
                    gitignore,
                    output,
                )?;
            }
            Commands::Apply {
                plan,
                dry_run,
                trash,
                force,
                verbose,
            } => {
                run_apply(plan, dry_run, trash, force, verbose)?;
            }
            Commands::Undo {
                batch,
                dry_run,
                force,
                verbose,
            } => {
                run_undo(batch, dry_run, force, verbose)?;
            }
        }

        Ok(())
    }
}

fn run_scan(
    path: PathBuf,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    json_output: bool,
    explain: bool,
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};

    if json_output {
        let mut scanner = Scanner::new(&path);

        if let Some(d) = depth {
            scanner = scanner.max_depth(d);
        }

        if let Some(size_mb) = min_size_mb {
            scanner = scanner.min_size(size_mb * 1024 * 1024);
        }

        if let Some(days) = older_than {
            scanner = scanner.max_age_days(days);
        }

        scanner = scanner.respect_gitignore(gitignore);

        let projects = scanner.scan()?;
        println!("{}", serde_json::to_string_pretty(&projects)?);
        return Ok(());
    }

    println!("{}", "Scanning for cleanable directories...".cyan().bold());

    let mut scanner = Scanner::new(&path);
    let min_size_bytes = min_size_mb.map(|size_mb| size_mb * 1024 * 1024);

    if let Some(d) = depth {
        scanner = scanner.max_depth(d);
    }

    if let Some(size_mb) = min_size_mb {
        scanner = scanner.min_size(size_mb * 1024 * 1024);
    }

    if let Some(days) = older_than {
        scanner = scanner.max_age_days(days);
    }

    scanner = scanner.respect_gitignore(gitignore);

    // Use streaming scan for real-time progress
    let (total_count, rx) = scanner.scan_with_streaming()?;

    if total_count == 0 {
        println!("{}", "No cleanable directories found.".yellow());
        return Ok(());
    }

    println!(
        "Found {} potential projects, calculating sizes...\n",
        total_count
    );

    // Create progress bar
    let pb = ProgressBar::new(total_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut projects = Vec::new();
    let mut total_size = 0u64;

    // Receive and display results as they complete
    for project in rx.iter() {
        // Always advance progress (we calculate size for every candidate).
        pb.inc(1);

        // Apply size filter in CLI (streaming scanner reports all candidates).
        let passes_size = min_size_bytes.map_or(true, |ms| project.size >= ms);
        if !passes_size {
            continue;
        }

        total_size += project.size;

        // Update progress bar message
        let dir_display = project.cleanable_dir.display().to_string();
        let short_path = if dir_display.len() > 50 {
            format!("...{}", &dir_display[dir_display.len() - 47..])
        } else {
            dir_display.clone()
        };
        pb.set_message(format!("{}: {}", short_path, project.size_human()));

        // Print result immediately above progress bar (streaming output)
        pb.println(format!(
            "  {} {} {} ({})",
            "✓".green(),
            project.project_type.name().bright_cyan(),
            dir_display.bright_white(),
            project.size_human().yellow()
        ));

        if explain {
            let reason = ProjectDetector::explain_cleanable_dir(
                project.project_type,
                &project.root,
                &project.cleanable_dir,
            );
            pb.println(format!(
                "    {} {}",
                "↳".bright_black(),
                reason.bright_black()
            ));
        }

        projects.push(project);
    }

    pb.finish_and_clear();

    if projects.is_empty() {
        println!("\n{}", "No directories match the filter criteria.".yellow());
        return Ok(());
    }

    // Sort by size for summary
    projects.sort_by(|a, b| b.size.cmp(&a.size));

    println!(
        "\n{} {} cleanable directories found",
        "✓".green().bold(),
        projects.len().to_string().green().bold()
    );
    println!(
        "{} {}\n",
        "Total size:".bold(),
        format_size(total_size).green().bold()
    );

    Ok(())
}

fn run_clean(
    path: PathBuf,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    dry_run: bool,
    trash: bool,
    auto: bool,
    force: bool,
    verbose: bool,
    gitignore: bool,
) -> Result<()> {
    println!("{}", "Scanning for cleanable directories...".cyan().bold());

    let mut scanner = Scanner::new(&path);

    if let Some(d) = depth {
        scanner = scanner.max_depth(d);
    }

    if let Some(size_mb) = min_size_mb {
        scanner = scanner.min_size(size_mb * 1024 * 1024);
    }

    if let Some(days) = older_than {
        scanner = scanner.max_age_days(days);
    }

    scanner = scanner.respect_gitignore(gitignore);

    let mut projects = scanner.scan()?;

    if projects.is_empty() {
        println!("{}", "No cleanable directories found.".yellow());
        return Ok(());
    }

    println!(
        "\n{} cleanable directories found:\n",
        projects.len().to_string().green().bold()
    );

    let total_size: u64 = projects.iter().map(|p| p.size).sum();

    display_projects(&projects);

    println!(
        "\n{} {}",
        "Total size:".bold(),
        format_size(total_size).green().bold()
    );

    // Filter or confirm
    if !auto && !force {
        projects = select_projects_interactive(&projects)?;

        if projects.is_empty() {
            println!("{}", "No directories selected for cleaning.".yellow());
            return Ok(());
        }
    }

    // Perform cleaning
    let options = CleanOptions {
        dry_run,
        verbose,
        force,
        trash,
    };

    let cleaner = Cleaner::with_options(options);
    let result = cleaner.clean_multiple(&projects)?;

    println!("\n{}", "Cleaning completed!".green().bold());
    println!("  Cleaned: {}", result.cleaned_count.to_string().green());
    println!(
        "  Skipped: {} ({})",
        result.skipped_count.to_string().yellow(),
        format_size(result.bytes_skipped).yellow()
    );
    println!("  Failed: {}", result.failed_count.to_string().red());
    println!(
        "  Space freed: {}",
        result.size_freed_human().green().bold()
    );

    if let Some(batch_id) = &result.trash_batch_id {
        println!("  Trash batch: {}", batch_id.cyan().bold());
        println!(
            "  Undo: {}",
            format!("dev-cleaner undo --batch {}", batch_id).bright_black()
        );
    }

    if !result.errors.is_empty() {
        println!("\n{}", "Errors:".red().bold());
        for error in &result.errors {
            println!("  {}", error.red());
        }
    }

    Ok(())
}

fn display_projects(projects: &[ProjectInfo]) {
    for (idx, project) in projects.iter().enumerate() {
        let project_type = project.project_type.name();
        let colored_type = match project.project_type.color() {
            "green" => project_type.green(),
            "red" => project_type.red(),
            "blue" => project_type.blue(),
            "cyan" => project_type.cyan(),
            "yellow" => project_type.yellow(),
            "magenta" => project_type.magenta(),
            _ => project_type.white(),
        };

        let in_use = if project.in_use {
            " [IN USE]".yellow()
        } else {
            "".white()
        };

        println!(
            "{}. [{}] {} - {}{} ({})",
            (idx + 1).to_string().dimmed(),
            colored_type,
            project.cleanable_dir.display().to_string().bold(),
            project.size_human().green(),
            in_use,
            format!("{} days old", project.days_since_modified()).dimmed()
        );
    }
}

fn select_projects_interactive(projects: &[ProjectInfo]) -> Result<Vec<ProjectInfo>> {
    println!("\n{}", "Select directories to clean:".cyan().bold());
    println!("  Enter numbers separated by spaces (e.g., 1 3 5)");
    println!("  Or 'all' to select all, 'none' to cancel");

    print!("\n> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let input = input.trim().to_lowercase();

    if input == "none" || input.is_empty() {
        return Ok(Vec::new());
    }

    if input == "all" {
        return Ok(projects.to_vec());
    }

    let mut selected = Vec::new();

    for num_str in input.split_whitespace() {
        if let Ok(num) = num_str.parse::<usize>() {
            if num > 0 && num <= projects.len() {
                selected.push(projects[num - 1].clone());
            }
        }
    }

    Ok(selected)
}

fn run_stats(
    path: PathBuf,
    depth: Option<usize>,
    top_n: usize,
    json_output: bool,
    gitignore: bool,
) -> Result<()> {
    use crate::Statistics;

    println!("{}", "Scanning for cleanable directories...".cyan().bold());

    let mut scanner = Scanner::new(&path);

    if let Some(d) = depth {
        scanner = scanner.max_depth(d);
    }

    scanner = scanner.respect_gitignore(gitignore);

    // Use regular scan for statistics (we need all results)
    let projects = scanner.scan()?;

    if projects.is_empty() {
        println!("{}", "No cleanable directories found.".yellow());
        return Ok(());
    }

    // Generate statistics
    let stats = Statistics::from_projects(projects);

    if json_output {
        // Output JSON
        match stats.to_json() {
            Ok(json) => println!("{}", json),
            Err(e) => eprintln!("Error generating JSON: {}", e),
        }
    } else {
        // Display terminal output
        stats.display_terminal(top_n);
    }

    Ok(())
}

fn init_config(path: Option<PathBuf>) -> Result<()> {
    let config_path = path.unwrap_or_else(|| {
        Config::ensure_config_dir().unwrap_or_else(|_| PathBuf::from("config.toml"))
    });

    let config = Config::default();
    config.save(&config_path)?;

    println!(
        "{} {}",
        "Config file created:".green().bold(),
        config_path.display()
    );

    Ok(())
}

fn run_plan(
    path: PathBuf,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    output: Option<PathBuf>,
) -> Result<()> {
    let scan_root = fs::canonicalize(&path).unwrap_or(path);
    let mut scanner = Scanner::new(&scan_root);

    if let Some(d) = depth {
        scanner = scanner.max_depth(d);
    }

    if let Some(size_mb) = min_size_mb {
        scanner = scanner.min_size(size_mb * 1024 * 1024);
    }

    if let Some(days) = older_than {
        scanner = scanner.max_age_days(days);
    }

    scanner = scanner.respect_gitignore(gitignore);

    let projects = scanner.scan()?;
    let plan = CleanupPlan::new(scan_root, projects);

    if let Some(output_path) = output {
        plan.save_json(&output_path)?;
        println!(
            "{} {}",
            "Plan file created:".green().bold(),
            output_path.display()
        );
    } else {
        println!("{}", plan.to_json_pretty()?);
    }

    Ok(())
}

fn run_apply(
    plan_path: PathBuf,
    dry_run: bool,
    trash: bool,
    force: bool,
    verbose: bool,
) -> Result<()> {
    let plan = CleanupPlan::load_json(&plan_path)?;

    if plan.schema_version != 1 {
        anyhow::bail!("Unsupported plan schema_version: {}", plan.schema_version);
    }

    for project in &plan.projects {
        if !project.cleanable_dir.starts_with(&plan.scan_root) {
            anyhow::bail!(
                "Plan contains path outside scan_root: {}",
                project.cleanable_dir.display()
            );
        }
    }

    let total_size: u64 = plan.projects.iter().map(|p| p.size).sum();
    println!("{}", "Applying cleanup plan...".cyan().bold());
    println!("  Plan: {}", plan_path.display());
    println!("  Projects: {}", plan.projects.len().to_string().green());
    println!("  Total size: {}", format_size(total_size).green().bold());

    if plan.projects.is_empty() {
        println!("{}", "Nothing to clean.".yellow());
        return Ok(());
    }

    if !force
        && !confirm(&format!(
            "Apply this plan and remove {} directories?",
            plan.projects.len()
        ))?
    {
        println!("{}", "Cancelled.".yellow());
        return Ok(());
    }

    let cleaner = Cleaner::with_options(CleanOptions {
        dry_run,
        verbose,
        force,
        trash,
    });
    let result = cleaner.clean_multiple(&plan.projects)?;

    println!("\n{}", "Cleaning completed!".green().bold());
    println!("  Cleaned: {}", result.cleaned_count.to_string().green());
    println!(
        "  Skipped: {} ({})",
        result.skipped_count.to_string().yellow(),
        format_size(result.bytes_skipped).yellow()
    );
    println!("  Failed: {}", result.failed_count.to_string().red());
    println!(
        "  Space freed: {}",
        result.size_freed_human().green().bold()
    );

    if let Some(batch_id) = &result.trash_batch_id {
        println!("  Trash batch: {}", batch_id.cyan().bold());
        println!(
            "  Undo: {}",
            format!("dev-cleaner undo --batch {}", batch_id).bright_black()
        );
    }

    if !result.errors.is_empty() {
        println!("\n{}", "Errors:".red().bold());
        for error in &result.errors {
            println!("  {}", error.red());
        }
    }

    Ok(())
}

fn confirm(prompt: &str) -> Result<bool> {
    print!("\n{} [y/N] > ", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_lowercase();

    Ok(matches!(input.as_str(), "y" | "yes"))
}

fn run_undo(batch: Option<String>, dry_run: bool, force: bool, verbose: bool) -> Result<()> {
    let trash_root = default_trash_root();
    let batch_id = match batch {
        Some(b) => Some(b),
        None => latest_batch_id(&trash_root)?,
    };

    let Some(batch_id) = batch_id else {
        println!("{}", "No trash batches found.".yellow());
        return Ok(());
    };

    println!("{}", "Restoring from trash...".cyan().bold());
    println!("  Trash root: {}", trash_root.display());
    println!("  Batch: {}", batch_id.cyan().bold());

    let result = restore_batch(&trash_root, &batch_id, dry_run, force, verbose)?;

    println!("\n{}", "Restore completed!".green().bold());
    println!("  Restored: {}", result.restored_count.to_string().green());
    println!("  Skipped: {}", result.skipped_count.to_string().yellow());
    println!("  Failed: {}", result.failed_count.to_string().red());

    if !result.errors.is_empty() {
        println!("\n{}", "Errors:".red().bold());
        for error in &result.errors {
            println!("  {}", error.red());
        }
    }

    Ok(())
}
