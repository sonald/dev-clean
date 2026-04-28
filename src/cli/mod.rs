use crate::bridge::{run_bridge, BridgeCommands};
use crate::clean_progress::{TerminalCleanObserver, TerminalRestoreObserver};
use crate::interactive::{ProjectSelector, SelectorOptions};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use dev_cleaner_core::app::{
    ApplyPlanRequest, ApplyPlanService, BlockedSummary as AppBlockedSummary, CleanupRequest,
    CleanupService, ScanRequest, ScanService, VisibilityOptions,
};
use dev_cleaner_core::audit::AuditLogger;
use dev_cleaner_core::cleaner::CleanOptions;
use dev_cleaner_core::recommend::{recommend_projects, RecommendOptions, RecommendStrategy};
use dev_cleaner_core::scanner::{Category, ProjectDetector, RiskLevel, RuleSource};
use dev_cleaner_core::trash::{
    default_trash_root, gc_trash, latest_batch_id, list_trash_batches, purge_trash_batch,
    restore_batch_with_observer, trash_entries_for_batch,
};
use dev_cleaner_core::utils::{format_size, parse_size};
use dev_cleaner_core::{
    Cleaner, CleanupPlan, Config, EvaluatedProject as AppEvaluatedProject, ProjectInfo,
};
use serde_json::json;
use std::fs;
use std::io::{self, IsTerminal, Write};
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

    /// Named scan profile from config
    #[arg(long, global = true)]
    pub profile: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum CategoryFilterArg {
    Cache,
    Build,
    Deps,
    All,
}

impl CategoryFilterArg {
    fn to_filter(self) -> Option<Category> {
        match self {
            Self::Cache => Some(Category::Cache),
            Self::Build => Some(Category::Build),
            Self::Deps => Some(Category::Deps),
            Self::All => None,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RiskArg {
    Low,
    Medium,
    High,
    All,
}

impl RiskArg {
    fn to_max_risk(self) -> RiskLevel {
        match self {
            Self::Low => RiskLevel::Low,
            Self::Medium => RiskLevel::Medium,
            Self::High | Self::All => RiskLevel::High,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum RecommendStrategyArg {
    SafeFirst,
    Balanced,
    MaxSpace,
}

impl RecommendStrategyArg {
    fn to_strategy(self) -> RecommendStrategy {
        match self {
            Self::SafeFirst => RecommendStrategy::SafeFirst,
            Self::Balanced => RecommendStrategy::Balanced,
            Self::MaxSpace => RecommendStrategy::MaxSpace,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ExportFormatArg {
    Json,
    Csv,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Scan directories for cleanable projects
    Scan {
        /// Directory to scan
        path: Option<PathBuf>,

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

        /// Filter by category (cache/build/deps/all)
        #[arg(long, value_enum, default_value = "all")]
        category: CategoryFilterArg,

        /// Filter by max risk level (low/medium/high/all)
        #[arg(long, value_enum, default_value = "medium", alias = "risk")]
        max_risk: RiskArg,

        /// Include protected targets in results
        #[arg(long)]
        include_protected: bool,

        /// Include recently modified targets in results
        #[arg(long)]
        include_recent: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,
    },

    /// Clean project directories
    Clean {
        /// Directory to scan
        path: Option<PathBuf>,

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

        /// Print a copy-friendly share summary after cleaning
        #[arg(long)]
        share: bool,

        /// Verbose output
        #[arg(short, long)]
        verbose: bool,

        /// Respect .gitignore files (skips gitignored directories)
        #[arg(long)]
        gitignore: bool,

        /// Filter by category (cache/build/deps/all)
        #[arg(long, value_enum, default_value = "all")]
        category: CategoryFilterArg,

        /// Filter by max risk level (low/medium/high/all)
        #[arg(long, value_enum, default_value = "medium", alias = "risk")]
        max_risk: RiskArg,

        /// Include recently modified targets
        #[arg(long)]
        include_recent: bool,

        /// Include protected targets
        #[arg(long)]
        include_protected: bool,

        /// Allow deleting protected targets
        #[arg(long)]
        force_protected: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,
    },

    /// Launch interactive TUI mode
    Tui {
        /// Directory to scan
        path: Option<PathBuf>,

        /// Include recently modified targets
        #[arg(long)]
        include_recent: bool,

        /// Include protected targets
        #[arg(long)]
        include_protected: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,
    },

    /// Show statistics about cleanable directories
    Stats {
        /// Directory to scan
        path: Option<PathBuf>,

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

        /// Filter by category (cache/build/deps/all)
        #[arg(long, value_enum, default_value = "all")]
        category: CategoryFilterArg,

        /// Filter by max risk level (low/medium/high/all)
        #[arg(long, value_enum, default_value = "medium", alias = "risk")]
        max_risk: RiskArg,

        /// Include recently modified targets
        #[arg(long)]
        include_recent: bool,

        /// Include protected targets
        #[arg(long)]
        include_protected: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,
    },

    /// Generate default config file
    InitConfig {
        /// Output path for config file
        path: Option<PathBuf>,
    },

    /// Generate a cleanup plan as JSON
    Plan {
        /// Directory to scan
        path: Option<PathBuf>,

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

        /// Filter by category (cache/build/deps/all)
        #[arg(long, value_enum, default_value = "all")]
        category: CategoryFilterArg,

        /// Filter by max risk level (low/medium/high/all)
        #[arg(long, value_enum, default_value = "medium", alias = "risk")]
        max_risk: RiskArg,

        /// Include recently modified targets
        #[arg(long)]
        include_recent: bool,

        /// Include protected targets
        #[arg(long)]
        include_protected: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,
    },

    /// Recommend a cleanup plan to meet a space goal (does not execute)
    Recommend {
        /// Directory to scan
        path: Option<PathBuf>,

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

        /// Target bytes to free (e.g. 10GB)
        #[arg(long)]
        cleanup: Option<String>,

        /// Ensure disk free space is at least this value (e.g. 50GB)
        #[arg(long)]
        free_at_least: Option<String>,

        /// Include in-use projects (default: false)
        #[arg(long)]
        include_in_use: bool,

        /// Include recently modified targets
        #[arg(long)]
        include_recent: bool,

        /// Include protected targets
        #[arg(long)]
        include_protected: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,

        /// Recommendation strategy
        #[arg(long, value_enum, default_value = "safe-first")]
        strategy: RecommendStrategyArg,

        /// Output recommended plan to a JSON file
        #[arg(long)]
        output_plan: Option<PathBuf>,

        /// Output as JSON
        #[arg(long)]
        json: bool,

        /// Print the matching rule for each result
        #[arg(long)]
        explain: bool,

        /// Filter by category (cache/build/deps/all)
        #[arg(long, value_enum, default_value = "all")]
        category: CategoryFilterArg,

        /// Filter by max risk level (low/medium/high/all)
        #[arg(long, value_enum, default_value = "medium", alias = "risk")]
        max_risk: RiskArg,
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

        /// Disable plan target re-validation before applying
        #[arg(long)]
        no_verify: bool,

        /// Include recently modified targets from plan
        #[arg(long)]
        include_recent: bool,

        /// Allow deleting protected targets from plan
        #[arg(long)]
        force_protected: bool,

        /// Mark as recent when modified within N days
        #[arg(long, default_value = "7")]
        recent_days: i64,

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

    /// Manage Dev Cleaner trash (list/show/purge/gc)
    Trash {
        #[command(subcommand)]
        command: TrashCommands,
    },

    /// Manage named scan profiles
    Profile {
        #[command(subcommand)]
        command: ProfileCommands,
    },

    /// Query audit logs
    Audit {
        #[command(subcommand)]
        command: AuditCommands,
    },

    /// Internal JSONL bridge for the macOS app.
    #[command(hide = true)]
    Bridge {
        #[command(subcommand)]
        command: BridgeCommands,
    },
}

#[derive(Subcommand)]
pub enum TrashCommands {
    /// List trash batches
    List {
        /// Show only top N batches (by most recent)
        #[arg(long, default_value = "20")]
        top: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show entries for a trash batch
    Show {
        /// Batch id to show
        #[arg(long)]
        batch: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Permanently delete a trash batch
    Purge {
        /// Batch id to permanently delete
        #[arg(long)]
        batch: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Garbage-collect old/oversize trash batches
    Gc {
        /// Keep batches newer than N days
        #[arg(long)]
        keep_days: Option<i64>,

        /// Keep total trash size under N GiB
        #[arg(long)]
        keep_gb: Option<u64>,

        /// Dry run (show what would be deleted)
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// List profile names
    List,
    /// Show profile contents
    Show {
        /// Profile name
        name: String,
    },
    /// Add or update a profile
    Add {
        /// Profile name
        name: String,
        /// Paths for this profile
        #[arg(long, required = true)]
        path: Vec<PathBuf>,
        /// Default depth
        #[arg(long)]
        depth: Option<usize>,
        /// Default minimum size in MB
        #[arg(long)]
        min_size_mb: Option<u64>,
        /// Default older-than days
        #[arg(long)]
        max_age_days: Option<i64>,
        /// Respect .gitignore by default
        #[arg(long)]
        gitignore: bool,
        /// Default category
        #[arg(long, value_enum)]
        category: Option<CategoryFilterArg>,
        /// Default max risk
        #[arg(long, value_enum)]
        max_risk: Option<RiskArg>,
    },
    /// Remove a profile
    Remove {
        /// Profile name
        name: String,
    },
}

#[derive(Subcommand)]
pub enum AuditCommands {
    /// List recent runs
    List {
        /// Show only top N runs
        #[arg(long, default_value = "20")]
        top: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Show all records for a run
    Show {
        /// Run id
        #[arg(long)]
        run: String,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Export audit records
    Export {
        /// Optional run id to export only one run
        #[arg(long)]
        run: Option<String>,
        /// Export format
        #[arg(long, value_enum, default_value = "json")]
        format: ExportFormatArg,
        /// Output path (stdout if omitted)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

impl Cli {
    pub fn run(self) -> Result<()> {
        let mut config = if let Some(config_path) = &self.config {
            Config::load(config_path)?
        } else {
            Config::load_or_default(Config::default_path())?
        };
        let profile = self.profile.clone();

        match self.command {
            Commands::Scan {
                path,
                depth,
                min_size,
                older_than,
                gitignore,
                json,
                explain,
                category,
                max_risk,
                include_protected,
                include_recent,
                recent_days,
            } => {
                run_scan(
                    path,
                    profile.as_deref(),
                    depth,
                    min_size,
                    older_than,
                    gitignore,
                    json,
                    explain,
                    category,
                    max_risk,
                    include_protected,
                    include_recent,
                    recent_days,
                    &config,
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
                share,
                verbose,
                gitignore,
                category,
                max_risk,
                include_recent,
                include_protected,
                force_protected,
                recent_days,
            } => {
                run_clean(
                    path,
                    profile.as_deref(),
                    depth,
                    min_size,
                    older_than,
                    dry_run,
                    trash,
                    auto,
                    force,
                    share,
                    verbose,
                    gitignore,
                    category,
                    max_risk,
                    include_recent,
                    include_protected,
                    force_protected,
                    recent_days,
                    &config,
                )?;
            }
            Commands::Tui {
                path,
                include_recent,
                include_protected,
                recent_days,
            } => {
                run_tui(
                    path,
                    profile.as_deref(),
                    include_recent,
                    include_protected,
                    recent_days,
                    &config,
                )?;
            }
            Commands::Stats {
                path,
                depth,
                top,
                json,
                gitignore,
                category,
                max_risk,
                include_recent,
                include_protected,
                recent_days,
            } => {
                run_stats(
                    path,
                    profile.as_deref(),
                    depth,
                    top,
                    json,
                    gitignore,
                    category,
                    max_risk,
                    include_recent,
                    include_protected,
                    recent_days,
                    &config,
                )?;
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
                category,
                max_risk,
                include_recent,
                include_protected,
                recent_days,
            } => {
                run_plan(
                    path,
                    profile.as_deref(),
                    depth,
                    min_size,
                    older_than,
                    gitignore,
                    output,
                    category,
                    max_risk,
                    include_recent,
                    include_protected,
                    recent_days,
                    &config,
                )?;
            }
            Commands::Recommend {
                path,
                depth,
                min_size,
                older_than,
                gitignore,
                cleanup,
                free_at_least,
                include_in_use,
                include_recent,
                include_protected,
                recent_days,
                strategy,
                output_plan,
                json,
                explain,
                category,
                max_risk,
            } => {
                run_recommend(
                    path,
                    profile.as_deref(),
                    depth,
                    min_size,
                    older_than,
                    gitignore,
                    cleanup,
                    free_at_least,
                    include_in_use,
                    include_recent,
                    include_protected,
                    recent_days,
                    strategy,
                    output_plan,
                    json,
                    explain,
                    category,
                    max_risk,
                    &config,
                )?;
            }
            Commands::Apply {
                plan,
                dry_run,
                trash,
                force,
                no_verify,
                include_recent,
                force_protected,
                recent_days,
                verbose,
            } => {
                run_apply(
                    plan,
                    dry_run,
                    trash,
                    force,
                    no_verify,
                    include_recent,
                    force_protected,
                    recent_days,
                    verbose,
                    &config,
                )?;
            }
            Commands::Undo {
                batch,
                dry_run,
                force,
                verbose,
            } => {
                run_undo(batch, dry_run, force, verbose, &config)?;
            }
            Commands::Trash { command } => {
                run_trash(command, &config)?;
            }
            Commands::Profile { command } => {
                run_profile(command, &mut config, self.config)?;
            }
            Commands::Audit { command } => {
                run_audit(command, &config)?;
            }
            Commands::Bridge { command } => {
                let config_path = self.config.unwrap_or_else(Config::default_path);
                run_bridge(command, &config, config_path)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
fn canonicalize_lossy(path: &std::path::Path) -> PathBuf {
    dev_cleaner_core::app::canonicalize_lossy(path)
}

#[cfg(test)]
fn derive_scan_root(roots: &[PathBuf]) -> PathBuf {
    dev_cleaner_core::app::derive_scan_root(roots)
}

fn build_scan_request(
    path: Option<PathBuf>,
    profile: Option<&str>,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    category: CategoryFilterArg,
    max_risk: RiskArg,
    include_protected: bool,
    include_recent: bool,
    recent_days: i64,
) -> ScanRequest {
    ScanRequest {
        path,
        profile: profile.map(str::to_owned),
        depth,
        min_size_mb,
        older_than_days: older_than,
        gitignore: gitignore.then_some(true),
        category: category.to_filter(),
        max_risk: (!matches!(max_risk, RiskArg::Medium)).then_some(max_risk.to_max_risk()),
        visibility: VisibilityOptions {
            include_protected,
            include_recent,
            recent_days,
        },
    }
}

fn project_infos_from_evaluated(projects: Vec<AppEvaluatedProject>) -> Vec<ProjectInfo> {
    let mut projects = projects
        .into_iter()
        .map(ProjectInfo::from)
        .collect::<Vec<_>>();
    projects.sort_by(|a, b| b.size.cmp(&a.size));
    projects
}

fn run_scan(
    path: Option<PathBuf>,
    profile: Option<&str>,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    json_output: bool,
    explain: bool,
    category: CategoryFilterArg,
    max_risk: RiskArg,
    include_protected: bool,
    include_recent: bool,
    recent_days: i64,
    config: &Config,
) -> Result<()> {
    use indicatif::{ProgressBar, ProgressStyle};

    let scan_service = ScanService::new();
    let request = build_scan_request(
        path,
        profile,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        include_protected,
        include_recent,
        recent_days,
    );
    let resolved = scan_service.resolve_inputs(config, &request)?;

    if json_output || resolved.roots.len() > 1 {
        let projects =
            project_infos_from_evaluated(scan_service.discover_visible(config, &request)?.projects);
        println!("{}", serde_json::to_string_pretty(&projects)?);
        return Ok(());
    }

    println!("{}", "Scanning for cleanable directories...".cyan().bold());
    let root = resolved.roots[0].clone();
    let scanner = scan_service.build_scanner(&root, config, &resolved);
    let (total_count, rx) = scanner.scan_with_streaming()?;
    if total_count == 0 {
        println!("{}", "No cleanable directories found.".yellow());
        return Ok(());
    }

    println!(
        "Found {} potential projects, calculating sizes...\n",
        total_count
    );

    let pb = ProgressBar::new(total_count as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut projects = Vec::new();
    let mut total_size = 0u64;
    for project in rx.iter() {
        pb.inc(1);
        if !resolved
            .min_size_bytes
            .map_or(true, |min_size_bytes| project.size >= min_size_bytes)
        {
            continue;
        }

        let evaluated = scan_service.evaluate_project_with_config(
            config,
            project,
            resolved.visibility.recent_days,
        );
        if !resolved.visibility.is_visible(&evaluated) {
            continue;
        }
        let project = evaluated.into_project_info();

        total_size += project.size;
        let dir_display = project.cleanable_dir.display().to_string();
        let short_path = if dir_display.len() > 50 {
            format!("...{}", &dir_display[dir_display.len() - 47..])
        } else {
            dir_display.clone()
        };
        pb.set_message(format!("{}: {}", short_path, project.size_human()));
        let rule_meta = detection_meta(&project);
        pb.println(format!(
            "  {} {} {} {} ({}){}{}",
            "✓".green(),
            project.project_type_display_name().bright_cyan(),
            rule_meta.bright_black(),
            dir_display.bright_white(),
            project.size_human().yellow(),
            if project.protected {
                " [PROTECTED]".yellow().to_string()
            } else {
                String::new()
            },
            if project.recent {
                " [RECENT]".bright_black().to_string()
            } else {
                String::new()
            }
        ));

        if explain {
            let reason = ProjectDetector::explain_cleanable_dir(
                project.project_type,
                &project.root,
                &project.cleanable_dir,
                &config.custom_patterns,
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct BlockedSummary {
    in_use_count: usize,
    in_use_bytes: u64,
    protected_count: usize,
    protected_bytes: u64,
    recent_count: usize,
    recent_bytes: u64,
}

impl BlockedSummary {
    fn total_count(&self) -> usize {
        self.in_use_count + self.protected_count + self.recent_count
    }

    fn total_bytes(&self) -> u64 {
        self.in_use_bytes
            .saturating_add(self.protected_bytes)
            .saturating_add(self.recent_bytes)
    }
}

impl From<AppBlockedSummary> for BlockedSummary {
    fn from(value: AppBlockedSummary) -> Self {
        Self {
            in_use_count: value.in_use_count,
            in_use_bytes: value.in_use_bytes,
            protected_count: value.protected_count,
            protected_bytes: value.protected_bytes,
            recent_count: value.recent_count,
            recent_bytes: value.recent_bytes,
        }
    }
}

struct CleanSelectionSplit {
    selected: Vec<ProjectInfo>,
    blocked: Vec<ProjectInfo>,
    blocked_summary: BlockedSummary,
}

fn split_selected_projects_for_clean(
    projects: Vec<ProjectInfo>,
    include_recent: bool,
    force: bool,
    force_protected: bool,
) -> CleanSelectionSplit {
    let selection = CleanupService::new().split(
        projects
            .into_iter()
            .map(AppEvaluatedProject::from)
            .collect::<Vec<_>>(),
        CleanupRequest {
            include_recent,
            force,
            force_protected,
        },
    );

    CleanSelectionSplit {
        selected: project_infos_from_evaluated(selection.selected),
        blocked: project_infos_from_evaluated(selection.blocked),
        blocked_summary: selection.blocked_summary.into(),
    }
}

fn is_interactive_tty() -> bool {
    io::stdin().is_terminal() && io::stdout().is_terminal()
}

fn run_keyboard_selector(
    projects: Vec<ProjectInfo>,
    force: bool,
    force_protected: bool,
) -> Result<Vec<ProjectInfo>> {
    let mut selector = ProjectSelector::new(
        projects,
        SelectorOptions {
            force,
            force_protected,
        },
    );
    Ok(selector.run()?.unwrap_or_default())
}

fn execution_mode_label(dry_run: bool, trash: bool) -> &'static str {
    if dry_run {
        "dry-run"
    } else if trash {
        "trash (undoable)"
    } else {
        "permanent delete"
    }
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

fn confirm_execution_summary(
    operation: &str,
    selected_count: usize,
    selected_bytes: u64,
    mode_label: &str,
    blocked: BlockedSummary,
    verify_skipped_total: usize,
) -> Result<bool> {
    println!("\n{}", "Execution summary".cyan().bold());
    println!("  Operation: {}", operation);
    println!(
        "  Selected: {} ({})",
        selected_count.to_string().green(),
        format_size(selected_bytes).green().bold()
    );
    println!("  Mode: {}", mode_label);

    if blocked.total_count() > 0 {
        println!(
            "  Blocked: in_use={} ({}) protected={} ({}) recent={} ({})",
            blocked.in_use_count.to_string().yellow(),
            format_size(blocked.in_use_bytes).yellow(),
            blocked.protected_count.to_string().yellow(),
            format_size(blocked.protected_bytes).yellow(),
            blocked.recent_count.to_string().yellow(),
            format_size(blocked.recent_bytes).yellow(),
        );
    }

    if verify_skipped_total > 0 {
        let verify_other = verify_skipped_total.saturating_sub(blocked.total_count());
        if verify_other > 0 {
            println!(
                "  Verify skipped (other): {}",
                verify_other.to_string().yellow()
            );
        }
    }

    if !is_interactive_tty() {
        return confirm("Continue?");
    }

    println!("  Action: Enter to execute, Esc/q to cancel");
    let _raw_mode = RawModeGuard::new()?;
    loop {
        if let Event::Key(key) = event::read()? {
            match key.code {
                KeyCode::Enter => return Ok(true),
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(false),
                _ => {}
            }
        }
    }
}

fn run_clean(
    path: Option<PathBuf>,
    profile: Option<&str>,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    dry_run: bool,
    trash: bool,
    auto: bool,
    force: bool,
    share: bool,
    verbose: bool,
    gitignore: bool,
    category: CategoryFilterArg,
    max_risk: RiskArg,
    include_recent: bool,
    include_protected: bool,
    force_protected: bool,
    recent_days: i64,
    config: &Config,
) -> Result<()> {
    println!("{}", "Scanning for cleanable directories...".cyan().bold());
    let scan_service = ScanService::new();
    let request = build_scan_request(
        path,
        profile,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        include_protected,
        include_recent,
        recent_days,
    );
    let mut projects =
        project_infos_from_evaluated(scan_service.discover_visible(config, &request)?.projects);

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
        projects = if is_interactive_tty() {
            run_keyboard_selector(projects, force, force_protected)?
        } else {
            select_projects_interactive(&projects)?
        };

        if projects.is_empty() {
            println!("{}", "No directories selected for cleaning.".yellow());
            return Ok(());
        }
    }

    let split = split_selected_projects_for_clean(projects, include_recent, force, force_protected);
    let selected_total_size: u64 = split.selected.iter().map(|p| p.size).sum();

    if split.selected.is_empty() {
        println!(
            "{}",
            "No directories are eligible for cleaning under current safety settings.".yellow()
        );
        if split.blocked_summary.total_count() > 0 {
            println!(
                "  Blocked: in_use={} protected={} recent={}",
                split.blocked_summary.in_use_count.to_string().yellow(),
                split.blocked_summary.protected_count.to_string().yellow(),
                split.blocked_summary.recent_count.to_string().yellow()
            );
        }
        return Ok(());
    }

    if !auto
        && !force
        && !confirm_execution_summary(
            "clean",
            split.selected.len(),
            selected_total_size,
            execution_mode_label(dry_run, trash),
            split.blocked_summary,
            0,
        )?
    {
        println!("{}", "Cancelled.".yellow());
        return Ok(());
    }

    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run("clean").ok();

    if let Some(run_id) = &run_id {
        for project in &split.blocked {
            let _ = audit.log_item(
                run_id,
                "clean",
                &project.cleanable_dir,
                "remove",
                "skipped",
                project.size,
                project.skip_reason.clone(),
            );
        }
    }

    // Perform cleaning
    let options = CleanOptions {
        dry_run,
        verbose,
        force,
        include_recent,
        force_protected,
        trash,
        trash_root: None,
        cancel_file: None,
    };

    let cleaner = Cleaner::with_options(options);
    let mut observer = TerminalCleanObserver::new(verbose);
    let mut result = cleaner.clean_multiple_with_observer(&split.selected, &mut observer)?;
    result.skipped_count += split.blocked_summary.total_count();
    result.bytes_skipped = result
        .bytes_skipped
        .saturating_add(split.blocked_summary.total_bytes());
    result.run_id = run_id.clone();

    if let Some(run_id) = &run_id {
        for project in &split.selected {
            let _ = audit.log_item(
                run_id,
                "clean",
                &project.cleanable_dir,
                if dry_run { "dry_run" } else { "remove" },
                "attempted",
                project.size,
                None,
            );
        }
    }

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

    if share {
        print_share_block_if_applicable(&result, dry_run, trash, auto, force, verbose);
    }

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

    if let Some(run_id) = run_id {
        let _ = audit.finish_run(
            &run_id,
            "clean",
            result.cleaned_count,
            result.skipped_count,
            result.failed_count,
            result.bytes_freed,
        );
    }

    Ok(())
}

fn build_share_snippet(
    result: &dev_cleaner_core::cleaner::CleanResult,
    dry_run: bool,
    trash: bool,
) -> Option<String> {
    if result.cleaned_count == 0 || result.bytes_freed == 0 {
        return None;
    }

    let action = if dry_run { "could free" } else { "just freed" };
    let undoable = if trash && !dry_run {
        " (undoable via trash)"
    } else {
        ""
    };

    Some(format!(
        "I {} {} by cleaning {} directories with dev-cleaner{}.\nTry: dev-cleaner scan",
        action,
        format_size(result.bytes_freed),
        result.cleaned_count,
        undoable
    ))
}

fn print_share_block_if_applicable(
    result: &dev_cleaner_core::cleaner::CleanResult,
    dry_run: bool,
    trash: bool,
    auto: bool,
    force: bool,
    verbose: bool,
) {
    let Some(snippet) = build_share_snippet(result, dry_run, trash) else {
        return;
    };

    println!("\n{}", "Share this result:".cyan().bold());
    println!("------------------------------");
    println!("{}", snippet);
    println!("------------------------------");

    let props = json!({
        "cleaned_count": result.cleaned_count,
        "bytes_freed": result.bytes_freed,
        "dry_run": dry_run,
        "trash": trash,
        "auto": auto,
        "force": force,
        "tool_version": env!("CARGO_PKG_VERSION"),
    });

    if let Err(err) = crate::metrics::log_event("share_generated", props) {
        if verbose {
            eprintln!(
                "{} {}",
                "Warning:".yellow().bold(),
                format!("failed to write metrics event: {}", err).yellow()
            );
        }
    }
}

fn display_projects(projects: &[ProjectInfo]) {
    for (idx, project) in projects.iter().enumerate() {
        let project_type = project.project_type_display_name();
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
        let protected = if project.protected {
            " [PROTECTED]".yellow()
        } else {
            "".white()
        };
        let recent = if project.recent {
            " [RECENT]".bright_black()
        } else {
            "".white()
        };
        let rule_meta = detection_meta(project);

        println!(
            "{}. [{}] {} {} - {}{}{}{} ({})",
            (idx + 1).to_string().dimmed(),
            colored_type,
            rule_meta.bright_black(),
            project.cleanable_dir.display().to_string().bold(),
            project.size_human().green(),
            in_use,
            protected,
            recent,
            format!("{} days old", project.days_since_modified()).dimmed()
        );
    }
}

fn rule_source_label(source: RuleSource) -> &'static str {
    match source {
        RuleSource::Custom => "custom",
        RuleSource::Builtin => "builtin",
        RuleSource::Gitignore => "gitignore",
        RuleSource::Heuristic => "heuristic",
    }
}

fn detection_meta(project: &ProjectInfo) -> String {
    let source = project
        .matched_rule
        .as_ref()
        .map(|rule| rule_source_label(rule.source))
        .unwrap_or("unknown");
    format!("source: {}", source)
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
    path: Option<PathBuf>,
    profile: Option<&str>,
    depth: Option<usize>,
    top_n: usize,
    json_output: bool,
    gitignore: bool,
    category: CategoryFilterArg,
    max_risk: RiskArg,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    config: &Config,
) -> Result<()> {
    use crate::Statistics;

    println!("{}", "Scanning for cleanable directories...".cyan().bold());
    let scan_service = ScanService::new();
    let request = build_scan_request(
        path,
        profile,
        depth,
        None,
        None,
        gitignore,
        category,
        max_risk,
        include_protected,
        include_recent,
        recent_days,
    );
    let projects =
        project_infos_from_evaluated(scan_service.discover_visible(config, &request)?.projects);

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
        crate::stats::display_terminal(&stats, top_n);
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
    path: Option<PathBuf>,
    profile: Option<&str>,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    output: Option<PathBuf>,
    category: CategoryFilterArg,
    max_risk: RiskArg,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    config: &Config,
) -> Result<()> {
    let scan_service = ScanService::new();
    let request = build_scan_request(
        path,
        profile,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        include_protected,
        include_recent,
        recent_days,
    );
    let discovered = scan_service.discover_visible(config, &request)?;
    let scan_root = discovered.resolved.scan_root.clone();
    let category_filter = discovered.resolved.category;
    let max_risk_level = discovered.resolved.max_risk;
    let projects = project_infos_from_evaluated(discovered.projects);

    let mut params = dev_cleaner_core::plan::PlanParams::default();
    params.max_risk = Some(max_risk_level);
    params.category = category_filter;
    params.recent_days = Some(recent_days);
    params.verify_mode = Some("strict".to_string());
    let plan = CleanupPlan::new_with_params(scan_root, projects, params);

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

fn run_recommend(
    path: Option<PathBuf>,
    profile: Option<&str>,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    cleanup: Option<String>,
    free_at_least: Option<String>,
    include_in_use: bool,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    strategy: RecommendStrategyArg,
    output_plan: Option<PathBuf>,
    json_output: bool,
    explain: bool,
    category: CategoryFilterArg,
    max_risk: RiskArg,
    config: &Config,
) -> Result<()> {
    use serde::Serialize;

    let scan_service = ScanService::new();
    let request = build_scan_request(
        path,
        profile,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        false,
        false,
        recent_days,
    );
    let discovered = scan_service.discover(config, &request)?;
    let scan_root = discovered.resolved.scan_root.clone();
    let category = discovered.resolved.category;
    let max_risk = discovered.resolved.max_risk;

    let cleanup_bytes = cleanup.as_deref().map(parse_size).transpose()?;
    let free_at_least_bytes = free_at_least.as_deref().map(parse_size).transpose()?;

    let target_bytes = match (cleanup_bytes, free_at_least_bytes) {
        (Some(_), Some(_)) => anyhow::bail!("Use either --cleanup or --free-at-least (not both)"),
        (None, None) => anyhow::bail!("Missing goal: use --cleanup or --free-at-least"),
        (Some(bytes), None) => bytes,
        (None, Some(want_free)) => {
            let free_now = fs2::available_space(&scan_root).with_context(|| {
                format!(
                    "Failed to read available disk space for {}",
                    scan_root.display()
                )
            })?;
            want_free.saturating_sub(free_now)
        }
    };

    let projects = project_infos_from_evaluated(discovered.projects);

    let mut opts = RecommendOptions::new(target_bytes);
    opts.include_in_use = include_in_use;
    opts.include_recent = include_recent;
    opts.include_protected = include_protected;
    opts.recent_days = recent_days;
    opts.strategy = strategy.to_strategy();
    opts.max_risk = Some(max_risk);

    let result = recommend_projects(projects, &opts);
    let selected_projects = result
        .selected
        .iter()
        .map(ProjectInfo::from)
        .collect::<Vec<_>>();

    #[derive(Serialize)]
    struct RecommendOutput {
        scan_root: String,
        target_bytes: u64,
        selected_bytes: u64,
        selected_count: usize,
        strategy: String,
        blocked: serde_json::Value,
        projects: Vec<ProjectInfo>,
    }

    let out = RecommendOutput {
        scan_root: scan_root.display().to_string(),
        target_bytes: result.target_bytes,
        selected_bytes: result.selected_bytes,
        selected_count: result.selected.len(),
        strategy: opts.strategy.as_str().to_string(),
        blocked: serde_json::json!({
            "in_use": { "count": result.blocked.in_use_count, "bytes": result.blocked.in_use_bytes },
            "protected": { "count": result.blocked.protected_count, "bytes": result.blocked.protected_bytes },
            "recent": { "count": result.blocked.recent_count, "bytes": result.blocked.recent_bytes },
            "risk": { "count": result.blocked.risk_count, "bytes": result.blocked.risk_bytes },
        }),
        projects: selected_projects.clone(),
    };

    if let Some(plan_path) = &output_plan {
        let mut params = dev_cleaner_core::plan::PlanParams::default();
        params.cleanup_bytes = cleanup_bytes;
        params.free_at_least_bytes = free_at_least_bytes;
        params.max_risk = Some(max_risk);
        params.category = category;
        params.strategy = Some(opts.strategy.as_str().to_string());
        params.recent_days = Some(recent_days);

        let plan =
            CleanupPlan::new_with_params(scan_root.clone(), selected_projects.clone(), params);
        plan.save_json(plan_path)?;
    }

    if json_output {
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("{}", "Recommended cleanup:".cyan().bold());
    println!("  Scan root: {}", scan_root.display());
    println!(
        "  Target: {}",
        if free_at_least_bytes.is_some() {
            format!(
                "free at least {} (need {})",
                format_size(free_at_least_bytes.unwrap()),
                format_size(target_bytes)
            )
        } else {
            format!("cleanup {}", format_size(target_bytes))
        }
        .green()
    );
    println!("  Strategy: {}", opts.strategy.as_str().green().bold());
    println!(
        "  Selected: {} ({})",
        out.selected_count.to_string().green(),
        format_size(out.selected_bytes).green().bold()
    );

    if !result.blocked.is_empty() {
        println!(
            "  Blocked: in_use={} protected={} recent={} risk={}",
            result.blocked.in_use_count.to_string().yellow(),
            result.blocked.protected_count.to_string().yellow(),
            result.blocked.recent_count.to_string().yellow(),
            result.blocked.risk_count.to_string().yellow()
        );
    }

    if out.projects.is_empty() {
        println!("{}", "No directories selected.".yellow());
        return Ok(());
    }

    println!();
    display_projects(&out.projects);

    if explain {
        println!();
        for project in &out.projects {
            let reason = ProjectDetector::explain_cleanable_dir(
                project.project_type,
                &project.root,
                &project.cleanable_dir,
                &config.custom_patterns,
            );
            println!("  {} {}", "↳".bright_black(), reason.bright_black());
        }
    }

    if let Some(plan_path) = output_plan {
        println!();
        println!(
            "{} {}",
            "Plan file created:".green().bold(),
            plan_path.display()
        );
    }

    Ok(())
}

fn run_apply(
    plan_path: PathBuf,
    dry_run: bool,
    trash: bool,
    force: bool,
    no_verify: bool,
    include_recent: bool,
    force_protected: bool,
    recent_days: i64,
    verbose: bool,
    config: &Config,
) -> Result<()> {
    let plan = CleanupPlan::load_json(&plan_path)?;
    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run("apply").ok();
    let apply_result = ApplyPlanService::new().verify(
        config,
        ApplyPlanRequest {
            plan,
            no_verify,
            include_recent,
            force,
            force_protected,
            recent_days,
        },
    )?;
    let verified_projects = project_infos_from_evaluated(apply_result.verified_projects.clone());
    let skipped_pre = apply_result.skipped_pre_count;
    let skipped_pre_bytes = apply_result.skipped_pre_bytes;
    let verify_blocked: BlockedSummary = apply_result.verification_blocked.into();

    if let Some(run_id) = &run_id {
        for project in &apply_result.skipped_projects {
            let skipped = project.to_project_info();
            let _ = audit.log_item(
                run_id,
                "apply",
                &skipped.cleanable_dir,
                "verify",
                "skipped",
                skipped.size,
                skipped.skip_reason.clone(),
            );
        }
    }

    let total_size: u64 = verified_projects.iter().map(|p| p.size).sum();
    println!("{}", "Applying cleanup plan...".cyan().bold());
    println!("  Plan: {}", plan_path.display());
    println!(
        "  Projects: {} ({} skipped in verify)",
        verified_projects.len().to_string().green(),
        skipped_pre.to_string().yellow()
    );
    println!("  Total size: {}", format_size(total_size).green().bold());
    if verify_blocked.total_count() > 0 {
        println!(
            "  Blocked: in_use={} protected={} recent={}",
            verify_blocked.in_use_count.to_string().yellow(),
            verify_blocked.protected_count.to_string().yellow(),
            verify_blocked.recent_count.to_string().yellow()
        );
    }

    if verified_projects.is_empty() {
        println!("{}", "Nothing to clean.".yellow());
        if let Some(run_id) = run_id {
            let _ = audit.finish_run(&run_id, "apply", 0, skipped_pre, 0, 0);
        }
        return Ok(());
    }

    if !force {
        let should_continue = confirm_execution_summary(
            "apply",
            verified_projects.len(),
            total_size,
            execution_mode_label(dry_run, trash),
            verify_blocked,
            skipped_pre,
        )?;
        if !should_continue {
            println!("{}", "Cancelled.".yellow());
            if let Some(run_id) = &run_id {
                let _ = audit.finish_run(run_id, "apply", 0, skipped_pre, 0, 0);
            }
            return Ok(());
        }
    }

    let cleaner = Cleaner::with_options(CleanOptions {
        dry_run,
        verbose,
        force,
        include_recent,
        force_protected,
        trash,
        trash_root: None,
        cancel_file: None,
    });
    let mut observer = TerminalCleanObserver::new(verbose);
    let mut result = cleaner.clean_multiple_with_observer(&verified_projects, &mut observer)?;
    result.skipped_count += skipped_pre;
    result.bytes_skipped = result.bytes_skipped.saturating_add(skipped_pre_bytes);
    result.run_id = run_id.clone();

    if let Some(run_id) = &run_id {
        for p in &verified_projects {
            let _ = audit.log_item(
                run_id,
                "apply",
                &p.cleanable_dir,
                if dry_run { "dry_run" } else { "remove" },
                "attempted",
                p.size,
                None,
            );
        }
    }

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

    if let Some(run_id) = run_id {
        let _ = audit.finish_run(
            &run_id,
            "apply",
            result.cleaned_count,
            result.skipped_count,
            result.failed_count,
            result.bytes_freed,
        );
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

struct AuditRunGuard<'a> {
    audit: &'a AuditLogger,
    run_id: Option<String>,
    command: &'static str,
}

impl<'a> AuditRunGuard<'a> {
    fn new(audit: &'a AuditLogger, command: &'static str) -> Self {
        Self {
            audit,
            run_id: audit.start_run(command).ok(),
            command,
        }
    }

    fn run_id(&self) -> Option<&str> {
        self.run_id.as_deref()
    }
}

impl<'a> Drop for AuditRunGuard<'a> {
    fn drop(&mut self) {
        if let Some(run_id) = &self.run_id {
            let _ = self.audit.finish_run(run_id, self.command, 0, 0, 0, 0);
        }
    }
}

fn run_undo(
    batch: Option<String>,
    dry_run: bool,
    force: bool,
    verbose: bool,
    config: &Config,
) -> Result<()> {
    let trash_root = default_trash_root();
    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run("undo").ok();
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

    let mut observer = TerminalRestoreObserver::new(verbose);
    let result =
        restore_batch_with_observer(&trash_root, &batch_id, dry_run, force, &mut observer)?;

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

    if let Some(run_id) = &run_id {
        let _ = audit.log_item(
            run_id,
            "undo",
            &trash_root.join(&batch_id),
            if dry_run {
                "dry_run_restore"
            } else {
                "restore"
            },
            "completed",
            0,
            None,
        );
        let _ = audit.finish_run(
            run_id,
            "undo",
            result.restored_count,
            result.skipped_count,
            result.failed_count,
            0,
        );
    }

    Ok(())
}

fn run_trash(command: TrashCommands, config: &Config) -> Result<()> {
    run_trash_with_root(command, config, default_trash_root())
}

fn run_trash_with_root(command: TrashCommands, config: &Config, trash_root: PathBuf) -> Result<()> {
    let audit = AuditLogger::from_config(config);
    let audit_run = AuditRunGuard::new(&audit, "trash");

    match command {
        TrashCommands::List { top, json } => {
            let batches = list_trash_batches(&trash_root)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&batches)?);
                return Ok(());
            }

            if batches.is_empty() {
                println!("{}", "No trash batches found.".yellow());
                return Ok(());
            }

            println!("{}", "Trash batches:".cyan().bold());
            println!("  Trash root: {}", trash_root.display());
            println!(
                "  Showing: {} / {}",
                std::cmp::min(top, batches.len()),
                batches.len()
            );

            for batch in batches.iter().take(top) {
                println!(
                    "  {}  {}  {}  {}",
                    batch.batch_id.cyan().bold(),
                    format!("{} items", batch.entries_count).bright_black(),
                    format_size(batch.total_size).green(),
                    batch
                        .created_at
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string()
                        .bright_black()
                );
            }
        }
        TrashCommands::Show { batch, json } => {
            let entries = trash_entries_for_batch(&trash_root, &batch)?;

            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
                return Ok(());
            }

            if entries.is_empty() {
                println!("{}", "No entries found for this batch.".yellow());
                return Ok(());
            }

            let total_size: u64 = entries.iter().map(|e| e.size).sum();
            println!("{}", "Trash batch:".cyan().bold());
            println!("  Trash root: {}", trash_root.display());
            println!("  Batch: {}", batch.cyan().bold());
            println!("  Entries: {}", entries.len().to_string().green());
            println!("  Total size: {}", format_size(total_size).green().bold());
            println!();

            for entry in entries {
                println!(
                    "  {} {} ({})",
                    "•".bright_black(),
                    entry.original_path.display(),
                    format_size(entry.size).yellow()
                );
                println!(
                    "    {} {}",
                    "↳".bright_black(),
                    entry.trashed_path.display()
                );
            }
        }
        TrashCommands::Purge { batch, force } => {
            let entries = trash_entries_for_batch(&trash_root, &batch)?;
            let total_size: u64 = entries.iter().map(|e| e.size).sum();

            if !force
                && !confirm(&format!(
                    "Permanently delete trash batch `{}` ({} entries, {})?",
                    batch,
                    entries.len(),
                    format_size(total_size)
                ))?
            {
                println!("{}", "Cancelled.".yellow());
                return Ok(());
            }

            let result = purge_trash_batch(&trash_root, &batch, false)?;

            if result.failed_batches > 0 {
                println!("{}", "Purge completed with errors.".yellow().bold());
            } else {
                println!("{}", "Purge completed.".green().bold());
            }
            println!("  Batch: {}", batch.cyan().bold());
            println!(
                "  Removed entries: {} ({})",
                result.removed_entries.to_string().green(),
                format_size(result.removed_bytes).green()
            );
            if !result.errors.is_empty() {
                println!("\n{}", "Errors:".red().bold());
                for error in &result.errors {
                    println!("  {}", error.red());
                }
            }
            if let Some(run_id) = audit_run.run_id() {
                let _ = audit.log_item(
                    run_id,
                    "trash",
                    &trash_root.join(&batch),
                    "purge_batch",
                    "completed",
                    result.removed_bytes,
                    None,
                );
            }
        }
        TrashCommands::Gc {
            keep_days,
            keep_gb,
            dry_run,
        } => {
            let (keep_days, keep_bytes) = match (keep_days, keep_gb) {
                (None, None) => (Some(30), Some(20_u64.saturating_mul(1024 * 1024 * 1024))),
                (days, gb) => (days, gb.map(|g| g.saturating_mul(1024 * 1024 * 1024))),
            };

            let result = gc_trash(&trash_root, keep_days, keep_bytes, true)?;

            if result.removed_batches == 0 {
                println!("{}", "Nothing to delete.".yellow());
                return Ok(());
            }

            println!("{}", "Trash GC:".cyan().bold());
            println!("  Trash root: {}", trash_root.display());
            println!(
                "  Would delete batches: {}",
                result.removed_batches.to_string().green()
            );
            println!(
                "  Would free: {}",
                format_size(result.removed_bytes).green().bold()
            );

            if result.blocked_by_keep_days {
                println!(
                    "  {} {}",
                    "Note:".yellow().bold(),
                    "keep-days prevents meeting keep-gb; only older batches are eligible.".yellow()
                );
            }

            if dry_run {
                println!("{}", "Dry run only; no changes made.".bright_black());
                if let Some(run_id) = audit_run.run_id() {
                    let _ = audit.log_item(
                        run_id,
                        "trash",
                        &trash_root,
                        "gc",
                        "dry_run",
                        result.removed_bytes,
                        None,
                    );
                }
                return Ok(());
            }

            if !confirm(&format!(
                "Permanently delete {} trash batches (free {})?",
                result.removed_batches,
                format_size(result.removed_bytes)
            ))? {
                println!("{}", "Cancelled.".yellow());
                return Ok(());
            }

            let applied = gc_trash(&trash_root, keep_days, keep_bytes, false)?;
            if applied.failed_batches > 0 {
                println!("{}", "Trash GC completed with errors.".yellow().bold());
            } else {
                println!("{}", "Trash GC completed.".green().bold());
            }
            println!(
                "  Deleted: {} batches ({} freed)",
                applied.removed_batches.to_string().green(),
                format_size(applied.removed_bytes).green()
            );
            println!(
                "  Remaining trash: {}",
                format_size(applied.remaining_bytes).yellow()
            );

            if !applied.errors.is_empty() {
                println!("\n{}", "Errors:".red().bold());
                for error in &applied.errors {
                    println!("  {}", error.red());
                }
            }
            if let Some(run_id) = audit_run.run_id() {
                let _ = audit.log_item(
                    run_id,
                    "trash",
                    &trash_root,
                    "gc",
                    "completed",
                    applied.removed_bytes,
                    None,
                );
            }
        }
    }

    Ok(())
}

fn run_profile(
    command: ProfileCommands,
    config: &mut Config,
    config_path: Option<PathBuf>,
) -> Result<()> {
    let path = config_path.unwrap_or_else(Config::default_path);
    match command {
        ProfileCommands::List => {
            if config.scan_profiles.is_empty() {
                println!("{}", "No profiles configured.".yellow());
                return Ok(());
            }
            println!("{}", "Profiles:".cyan().bold());
            for (name, profile) in &config.scan_profiles {
                println!(
                    "  {} ({} paths)",
                    name.green().bold(),
                    profile.paths.len().to_string().bright_black()
                );
            }
        }
        ProfileCommands::Show { name } => {
            let profile = config
                .scan_profiles
                .get(&name)
                .with_context(|| format!("Profile `{}` not found", name))?;
            println!("{}", serde_json::to_string_pretty(profile)?);
        }
        ProfileCommands::Add {
            name,
            path: paths,
            depth,
            min_size_mb,
            max_age_days,
            gitignore,
            category,
            max_risk,
        } => {
            let profile = dev_cleaner_core::config::ScanProfile {
                paths,
                depth,
                min_size_mb,
                max_age_days,
                gitignore: if gitignore { Some(true) } else { None },
                category: category.and_then(|c| c.to_filter()),
                max_risk: max_risk.map(|r| r.to_max_risk()),
            };
            config.scan_profiles.insert(name.clone(), profile);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create config directory: {}", parent.display())
                })?;
            }
            config.save(&path)?;
            println!(
                "{} {}",
                "Saved profile:".green().bold(),
                name.green().bold()
            );
        }
        ProfileCommands::Remove { name } => {
            if config.scan_profiles.remove(&name).is_none() {
                anyhow::bail!("Profile `{}` not found", name);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).with_context(|| {
                    format!("Failed to create config directory: {}", parent.display())
                })?;
            }
            config.save(&path)?;
            println!(
                "{} {}",
                "Removed profile:".green().bold(),
                name.green().bold()
            );
        }
    }
    Ok(())
}

fn run_audit(command: AuditCommands, config: &Config) -> Result<()> {
    let logger = AuditLogger::from_config(config);
    match command {
        AuditCommands::List { top, json } => {
            let runs = logger.list_runs()?;
            let shown = runs.into_iter().take(top).collect::<Vec<_>>();
            if json {
                println!("{}", serde_json::to_string_pretty(&shown)?);
                return Ok(());
            }
            if shown.is_empty() {
                println!("{}", "No audit runs found.".yellow());
                println!("  Log path: {}", logger.path().display());
                return Ok(());
            }
            println!("{}", "Audit runs:".cyan().bold());
            println!("  Log path: {}", logger.path().display());
            for run in shown {
                println!(
                    "  {}  {}  cleaned={} skipped={} failed={} freed={}",
                    run.run_id.cyan().bold(),
                    run.command.bright_black(),
                    run.cleaned.to_string().green(),
                    run.skipped.to_string().yellow(),
                    run.failed.to_string().red(),
                    format_size(run.freed_bytes).green()
                );
            }
        }
        AuditCommands::Show { run, json } => {
            let records = logger.records_for_run(&run)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&records)?);
                return Ok(());
            }
            if records.is_empty() {
                println!("{}", "No records found for this run.".yellow());
                return Ok(());
            }
            println!("{}", format!("Audit run {}", run).cyan().bold());
            for record in records {
                println!("{}", serde_json::to_string_pretty(&record)?);
            }
        }
        AuditCommands::Export {
            run,
            format,
            output,
        } => {
            let records = if let Some(run) = run {
                logger.records_for_run(&run)?
            } else {
                logger.read_records()?
            };
            let content = match format {
                ExportFormatArg::Json => serde_json::to_string_pretty(&records)?,
                ExportFormatArg::Csv => AuditLogger::export_csv(&records),
            };
            if let Some(output) = output {
                fs::write(&output, content)?;
                println!(
                    "{} {}",
                    "Exported audit to".green().bold(),
                    output.display()
                );
            } else {
                println!("{}", content);
            }
        }
    }
    Ok(())
}

fn run_tui(
    path: Option<PathBuf>,
    profile: Option<&str>,
    include_recent: bool,
    include_protected: bool,
    recent_days: i64,
    config: &Config,
) -> Result<()> {
    let scan_service = ScanService::new();
    let request = build_scan_request(
        path,
        profile,
        None,
        None,
        None,
        false,
        CategoryFilterArg::All,
        RiskArg::Medium,
        include_protected,
        include_recent,
        recent_days,
    );
    let projects =
        project_infos_from_evaluated(scan_service.discover_visible(config, &request)?.projects);
    crate::tui::run_tui_projects(projects, include_recent, include_protected, recent_days)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProjectType;
    use chrono::Utc;
    use dev_cleaner_core::audit::AuditRecord;
    use dev_cleaner_core::cleaner::CleanResult;
    use dev_cleaner_core::plan::CleanupPlan;
    use dev_cleaner_core::scanner::{Category, Confidence};
    use std::fs;
    use tempfile::TempDir;

    fn result(cleaned_count: usize, bytes_freed: u64) -> CleanResult {
        CleanResult {
            cleaned_count,
            bytes_freed,
            skipped_count: 0,
            bytes_skipped: 0,
            failed_count: 0,
            errors: Vec::new(),
            trash_batch_id: None,
            run_id: None,
        }
    }

    fn project_for_clean(
        path: &str,
        size: u64,
        in_use: bool,
        protected: bool,
        recent: bool,
    ) -> ProjectInfo {
        ProjectInfo {
            root: PathBuf::from("/workspace"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from(path),
            size,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use,
            protected,
            protected_by: None,
            recent,
            selection_reason: None,
            skip_reason: None,
        }
    }

    #[test]
    fn share_snippet_omits_zero_freed() {
        let snippet = build_share_snippet(&result(2, 0), false, false);
        assert!(snippet.is_none());
    }

    #[test]
    fn share_snippet_uses_dry_run_wording() {
        let snippet = build_share_snippet(&result(2, 1024), true, false).unwrap();
        assert!(snippet.contains("could free"));
        assert!(snippet.contains("Try: dev-cleaner scan"));
    }

    #[test]
    fn share_snippet_marks_undoable_trash() {
        let snippet = build_share_snippet(&result(2, 1024), false, true).unwrap();
        assert!(snippet.contains("just freed"));
        assert!(snippet.contains("undoable via trash"));
    }

    #[test]
    fn split_selected_projects_tracks_blocked_reasons() {
        let projects = vec![
            project_for_clean("/safe", 100, false, false, false),
            project_for_clean("/in-use", 90, true, false, false),
            project_for_clean("/protected", 80, false, true, false),
            project_for_clean("/recent", 70, false, false, true),
        ];

        let split = split_selected_projects_for_clean(projects, false, false, false);

        assert_eq!(split.selected.len(), 1);
        assert_eq!(split.blocked_summary.in_use_count, 1);
        assert_eq!(split.blocked_summary.protected_count, 1);
        assert_eq!(split.blocked_summary.recent_count, 1);
        assert_eq!(split.blocked_summary.total_count(), 3);
        assert_eq!(split.blocked_summary.total_bytes(), 240);
        assert!(split
            .blocked
            .iter()
            .any(|p| p.skip_reason.as_deref() == Some("blocked_in_use")));
        assert!(split
            .blocked
            .iter()
            .any(|p| p.skip_reason.as_deref() == Some("blocked_protected")));
        assert!(split
            .blocked
            .iter()
            .any(|p| p.skip_reason.as_deref() == Some("blocked_recent")));
    }

    #[test]
    fn split_selected_projects_respects_force_flags() {
        let projects = vec![
            project_for_clean("/in-use", 90, true, false, false),
            project_for_clean("/protected", 80, false, true, false),
            project_for_clean("/recent", 70, false, false, true),
        ];

        let split = split_selected_projects_for_clean(projects, true, true, true);
        assert_eq!(split.selected.len(), 3);
        assert_eq!(split.blocked_summary.total_count(), 0);
    }

    #[test]
    fn execution_mode_label_formats_modes() {
        assert_eq!(execution_mode_label(true, false), "dry-run");
        assert_eq!(execution_mode_label(false, true), "trash (undoable)");
        assert_eq!(execution_mode_label(false, false), "permanent delete");
    }

    #[test]
    fn build_scan_request_maps_flags_and_default_risk_filter() {
        let request = build_scan_request(
            Some(PathBuf::from("/scan")),
            Some("profile"),
            Some(4),
            Some(8),
            Some(9),
            true,
            CategoryFilterArg::Deps,
            RiskArg::All,
            true,
            false,
            11,
        );

        assert_eq!(request.path, Some(PathBuf::from("/scan")));
        assert_eq!(request.profile.as_deref(), Some("profile"));
        assert_eq!(request.depth, Some(4));
        assert_eq!(request.min_size_mb, Some(8));
        assert_eq!(request.older_than_days, Some(9));
        assert_eq!(request.gitignore, Some(true));
        assert_eq!(request.category, Some(Category::Deps));
        assert_eq!(request.max_risk, Some(RiskLevel::High));
        assert!(request.visibility.include_protected);
        assert!(!request.visibility.include_recent);
        assert_eq!(request.visibility.recent_days, 11);
    }

    #[test]
    fn build_scan_request_keeps_medium_as_default_filter() {
        let request = build_scan_request(
            None,
            None,
            None,
            None,
            None,
            false,
            CategoryFilterArg::All,
            RiskArg::Medium,
            false,
            true,
            7,
        );

        assert_eq!(request.max_risk, None);
        assert_eq!(request.visibility.include_recent, true);
    }

    #[test]
    fn project_infos_from_evaluated_sorts_largest_first() {
        let small = ProjectInfo {
            root: PathBuf::from("/workspace"),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: PathBuf::from("/workspace/small"),
            size: 1,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        };
        let mut large = small.clone();
        large.cleanable_dir = PathBuf::from("/workspace/large");
        large.size = 10;

        let sorted = project_infos_from_evaluated(vec![
            AppEvaluatedProject::new(small),
            AppEvaluatedProject::new(large),
        ]);

        assert_eq!(sorted[0].size, 10);
        assert_eq!(sorted[1].size, 1);
    }

    #[test]
    fn derive_scan_root_uses_common_ancestor_for_multi_roots() {
        let temp = TempDir::new().unwrap();
        let root_a = temp.path().join("workspace").join("a");
        let root_b = temp.path().join("workspace").join("b").join("nested");
        fs::create_dir_all(&root_a).unwrap();
        fs::create_dir_all(&root_b).unwrap();

        let derived = derive_scan_root(&vec![root_a, root_b]);
        let expected = canonicalize_lossy(&temp.path().join("workspace"));
        assert_eq!(derived, expected);
    }

    #[test]
    fn run_apply_accepts_legacy_relative_scan_root() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path().join("app");
        let cleanable_dir = project_root.join("target");
        fs::create_dir_all(&cleanable_dir).unwrap();
        fs::write(cleanable_dir.join("artifact.bin"), "x").unwrap();

        let plan_path = temp.path().join("plan.json");
        let project = ProjectInfo {
            root: project_root.clone(),
            project_type: ProjectType::Rust,
            project_name: None,
            category: Category::Build,
            risk_level: RiskLevel::Medium,
            confidence: Confidence::High,
            matched_rule: None,
            cleanable_dir: cleanable_dir.clone(),
            size: 1,
            size_calculated: true,
            last_modified: Utc::now(),
            in_use: false,
            protected: false,
            protected_by: None,
            recent: false,
            selection_reason: None,
            skip_reason: None,
        };

        let plan = CleanupPlan {
            schema_version: 3,
            tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            created_at: Utc::now(),
            scan_root: PathBuf::from("."),
            params: Some(dev_cleaner_core::plan::PlanParams::default()),
            projects: vec![project],
        };
        plan.save_json(&plan_path).unwrap();

        let mut config = Config::default();
        let audit_path = temp.path().join("operations.jsonl");
        config.audit.enabled = true;
        config.audit.path = Some(audit_path);

        run_apply(
            plan_path, true, false, true, true, true, true, 7, false, &config,
        )
        .unwrap();

        let records = AuditLogger::from_config(&config).read_records().unwrap();
        let mut has_attempted = false;
        let mut has_outside_scan_root = false;
        for record in records {
            if let AuditRecord::ItemAction {
                action,
                result,
                reason,
                ..
            } = record
            {
                if action == "dry_run" && result == "attempted" {
                    has_attempted = true;
                }
                if reason.as_deref() == Some("outside_scan_root") {
                    has_outside_scan_root = true;
                }
            }
        }

        assert!(has_attempted);
        assert!(!has_outside_scan_root);
    }

    #[test]
    fn run_apply_logs_verify_skips_before_early_return() {
        let temp = TempDir::new().unwrap();
        let project_root = temp.path().join("app");
        let cleanable_dir = temp.path().join("outside").join("target");
        fs::create_dir_all(&project_root).unwrap();
        fs::create_dir_all(&cleanable_dir).unwrap();
        fs::write(cleanable_dir.join("artifact.bin"), "x").unwrap();

        let plan_path = temp.path().join("plan.json");
        let plan = CleanupPlan {
            schema_version: 3,
            tool_version: Some(env!("CARGO_PKG_VERSION").to_string()),
            created_at: Utc::now(),
            scan_root: project_root.clone(),
            params: Some(dev_cleaner_core::plan::PlanParams::default()),
            projects: vec![ProjectInfo {
                root: project_root,
                project_type: ProjectType::Rust,
                project_name: None,
                category: Category::Build,
                risk_level: RiskLevel::Medium,
                confidence: Confidence::High,
                matched_rule: None,
                cleanable_dir,
                size: 1,
                size_calculated: true,
                last_modified: Utc::now(),
                in_use: false,
                protected: false,
                protected_by: None,
                recent: false,
                selection_reason: None,
                skip_reason: None,
            }],
        };
        plan.save_json(&plan_path).unwrap();

        let mut config = Config::default();
        let audit_path = temp.path().join("operations.jsonl");
        config.audit.enabled = true;
        config.audit.path = Some(audit_path);

        run_apply(
            plan_path, false, true, false, true, false, false, 7, false, &config,
        )
        .unwrap();

        let records = AuditLogger::from_config(&config).read_records().unwrap();
        let mut has_verify_skip = false;

        for record in records {
            if let AuditRecord::ItemAction {
                action,
                result,
                reason,
                ..
            } = record
            {
                if action == "verify"
                    && result == "skipped"
                    && reason.as_deref() == Some("outside_project_root")
                {
                    has_verify_skip = true;
                }
            }
        }

        assert!(has_verify_skip);
    }

    #[test]
    fn run_trash_list_json_finishes_audit_run_on_early_return() {
        let temp = TempDir::new().unwrap();
        let mut config = Config::default();
        config.audit.enabled = true;
        config.audit.path = Some(temp.path().join("operations.jsonl"));

        run_trash_with_root(
            TrashCommands::List {
                top: 20,
                json: true,
            },
            &config,
            temp.path().join("trash-root"),
        )
        .unwrap();

        let records = AuditLogger::from_config(&config).read_records().unwrap();
        let mut started_ids = Vec::new();
        let mut finished_ids = Vec::new();

        for record in records {
            match record {
                AuditRecord::RunStarted {
                    run_id, command, ..
                } if command == "trash" => started_ids.push(run_id),
                AuditRecord::RunFinished {
                    run_id, command, ..
                } if command == "trash" => finished_ids.push(run_id),
                _ => {}
            }
        }

        assert_eq!(started_ids.len(), 1);
        assert_eq!(finished_ids.len(), 1);
        assert_eq!(started_ids[0], finished_ids[0]);
    }
}
