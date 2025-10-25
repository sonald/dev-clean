use crate::{Scanner, Cleaner, ProjectInfo, Config};
use crate::cleaner::CleanOptions;
use anyhow::Result;
use clap::{Parser, Subcommand};
use colored::Colorize;
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
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let _config = if let Some(config_path) = &self.config {
            Config::load(config_path)?
        } else {
            Config::load_or_default(Config::default_path())?
        };

        match self.command {
            Commands::Scan { path, depth, min_size, older_than, gitignore } => {
                run_scan(path, depth, min_size, older_than, gitignore)?;
            }
            Commands::Clean { path, depth, min_size, older_than, dry_run, auto, force, verbose, gitignore } => {
                run_clean(path, depth, min_size, older_than, dry_run, auto, force, verbose, gitignore)?;
            }
            Commands::Tui { path } => {
                crate::tui::run_tui(path)?;
            }
            Commands::Stats { path, depth, top, json, gitignore } => {
                run_stats(path, depth, top, json, gitignore)?;
            }
            Commands::InitConfig { path } => {
                init_config(path)?;
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
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};

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

    // Use streaming scan for real-time progress
    let (total_count, rx) = scanner.scan_with_streaming()?;

    if total_count == 0 {
        println!("{}", "No cleanable directories found.".yellow());
        return Ok(());
    }

    println!("Found {} potential projects, calculating sizes...\n", total_count);

    // Create progress bar
    let pb = ProgressBar::new(total_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-")
    );

    let mut projects = Vec::new();
    let mut total_size = 0u64;

    // Receive and display results as they complete
    for project in rx.iter() {
        total_size += project.size;

        // Update progress bar message
        let dir_display = project.cleanable_dir.display().to_string();
        let short_path = if dir_display.len() > 50 {
            format!("...{}", &dir_display[dir_display.len()-47..])
        } else {
            dir_display.clone()
        };
        pb.set_message(format!("{}: {}", short_path, project.size_human()));

        // Print result immediately above progress bar (streaming output)
        pb.println(format!("  {} {} {} ({})",
            "✓".green(),
            project.project_type.name().bright_cyan(),
            dir_display.bright_white(),
            project.size_human().yellow()
        ));

        pb.inc(1);
        projects.push(project);
    }

    pb.finish_and_clear();

    if projects.is_empty() {
        println!("\n{}", "No directories match the filter criteria.".yellow());
        return Ok(());
    }

    // Sort by size for summary
    projects.sort_by(|a, b| b.size.cmp(&a.size));

    println!("\n{} {} cleanable directories found",
        "✓".green().bold(),
        projects.len().to_string().green().bold()
    );
    println!("{} {}\n",
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

    println!("\n{} cleanable directories found:\n", projects.len().to_string().green().bold());

    let total_size: u64 = projects.iter().map(|p| p.size).sum();

    display_projects(&projects);

    println!("\n{} {}", "Total size:".bold(), format_size(total_size).green().bold());

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
    };

    let cleaner = Cleaner::with_options(options);
    let result = cleaner.clean_multiple(&projects)?;

    println!("\n{}", "Cleaning completed!".green().bold());
    println!("  Cleaned: {}", result.cleaned_count.to_string().green());
    println!("  Failed: {}", result.failed_count.to_string().red());
    println!("  Space freed: {}", result.size_freed_human().green().bold());

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

    println!("{} {}", "Config file created:".green().bold(), config_path.display());

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.2} {}", size, UNITS[unit_idx])
    }
}
