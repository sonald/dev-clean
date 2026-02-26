use crate::audit::AuditLogger;
use crate::cleaner::CleanOptions;
use crate::policy::KeepPolicy;
use crate::recommend::{recommend_projects, RecommendOptions, RecommendStrategy};
use crate::scanner::{Category, ProjectDetector, RiskLevel};
use crate::trash::{
    default_trash_root, gc_trash, latest_batch_id, list_trash_batches, purge_trash_batch,
    restore_batch, trash_entries_for_batch,
};
use crate::utils::{format_size, parse_size};
use crate::{Cleaner, CleanupPlan, Config, ProjectInfo, Scanner};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use colored::Colorize;
use serde_json::json;
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
        }

        Ok(())
    }
}

#[derive(Debug, Clone)]
struct ResolvedScanInput {
    roots: Vec<PathBuf>,
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: Option<bool>,
    category: Option<Category>,
    max_risk: Option<RiskLevel>,
}

impl ResolvedScanInput {
    fn from_path(path: PathBuf) -> Self {
        Self {
            roots: vec![path],
            depth: None,
            min_size_mb: None,
            older_than: None,
            gitignore: None,
            category: None,
            max_risk: None,
        }
    }

    fn from_profile(profile: &crate::config::ScanProfile) -> Self {
        Self {
            roots: profile.paths.clone(),
            depth: profile.depth,
            min_size_mb: profile.min_size_mb,
            older_than: profile.max_age_days,
            gitignore: profile.gitignore,
            category: profile.category,
            max_risk: profile.max_risk,
        }
    }
}

fn resolve_scan_inputs(
    path: Option<PathBuf>,
    profile: Option<&str>,
    config: &Config,
) -> Result<ResolvedScanInput> {
    match (path, profile) {
        (Some(_), Some(_)) => anyhow::bail!("Use either [PATH] or --profile, not both"),
        (None, Some(name)) => {
            let p = config
                .scan_profiles
                .get(name)
                .with_context(|| format!("Profile `{}` not found", name))?;
            if p.paths.is_empty() {
                anyhow::bail!("Profile `{}` has no paths", name);
            }
            Ok(ResolvedScanInput::from_profile(p))
        }
        (Some(path), None) => Ok(ResolvedScanInput::from_path(path)),
        (None, None) => Ok(ResolvedScanInput::from_path(PathBuf::from("."))),
    }
}

fn enrich_project_flags(projects: &mut [ProjectInfo], keep_policy: &KeepPolicy, recent_days: i64) {
    for project in projects {
        let decision = keep_policy.evaluate(project);
        project.protected = decision.protected;
        project.protected_by = decision.reason;
        project.recent = project.days_since_modified() < recent_days;
    }
}

fn filter_by_visibility(
    projects: Vec<ProjectInfo>,
    include_protected: bool,
    include_recent: bool,
) -> Vec<ProjectInfo> {
    projects
        .into_iter()
        .filter(|p| (include_protected || !p.protected) && (include_recent || !p.recent))
        .collect()
}

fn deduplicate_projects(projects: Vec<ProjectInfo>) -> Vec<ProjectInfo> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for project in projects {
        if seen.insert(project.cleanable_dir.clone()) {
            out.push(project);
        }
    }
    out
}

fn scan_projects_for_roots(
    roots: &[PathBuf],
    depth: Option<usize>,
    min_size_mb: Option<u64>,
    older_than: Option<i64>,
    gitignore: bool,
    category: Option<Category>,
    max_risk: RiskLevel,
    config: &Config,
) -> Result<Vec<ProjectInfo>> {
    let mut all = Vec::new();
    for root in roots {
        let mut scanner = Scanner::new(root)
            .exclude_dirs(&config.exclude_dirs)
            .custom_patterns(&config.custom_patterns)
            .max_risk(max_risk);

        if let Some(category) = category {
            scanner = scanner.category(category);
        }
        if let Some(depth) = depth {
            scanner = scanner.max_depth(depth);
        }
        if let Some(min_size_mb) = min_size_mb {
            scanner = scanner.min_size(min_size_mb * 1024 * 1024);
        }
        if let Some(older_than) = older_than {
            scanner = scanner.max_age_days(older_than);
        }
        scanner = scanner.respect_gitignore(gitignore);
        let mut projects = scanner.scan()?;
        all.append(&mut projects);
    }

    let mut deduped = deduplicate_projects(all);
    deduped.sort_by(|a, b| b.size.cmp(&a.size));
    Ok(deduped)
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

    let resolved = resolve_scan_inputs(path, profile, config)?;
    let depth = depth.or(resolved.depth).or(config.default_depth);
    let min_size_mb = min_size_mb.or(resolved.min_size_mb).or(config.min_size_mb);
    let older_than = older_than.or(resolved.older_than).or(config.max_age_days);
    let gitignore = gitignore || resolved.gitignore.unwrap_or(false);
    let category = category.to_filter().or(resolved.category);
    let max_risk = if matches!(max_risk, RiskArg::Medium) {
        resolved.max_risk.unwrap_or(max_risk.to_max_risk())
    } else {
        max_risk.to_max_risk()
    };
    let keep_policy = KeepPolicy::from_config(config);

    if json_output || resolved.roots.len() > 1 {
        let mut projects = scan_projects_for_roots(
            &resolved.roots,
            depth,
            min_size_mb,
            older_than,
            gitignore,
            category,
            max_risk,
            config,
        )?;
        enrich_project_flags(&mut projects, &keep_policy, recent_days);
        projects = filter_by_visibility(projects, include_protected, include_recent);
        println!("{}", serde_json::to_string_pretty(&projects)?);
        return Ok(());
    }

    println!("{}", "Scanning for cleanable directories...".cyan().bold());
    let root = resolved.roots[0].clone();

    let mut scanner = Scanner::new(&root)
        .exclude_dirs(&config.exclude_dirs)
        .custom_patterns(&config.custom_patterns)
        .max_risk(max_risk);
    if let Some(category) = category {
        scanner = scanner.category(category);
    }
    if let Some(depth) = depth {
        scanner = scanner.max_depth(depth);
    }
    if let Some(min_size_mb) = min_size_mb {
        scanner = scanner.min_size(min_size_mb * 1024 * 1024);
    }
    if let Some(days) = older_than {
        scanner = scanner.max_age_days(days);
    }
    scanner = scanner.respect_gitignore(gitignore);

    let min_size_bytes = min_size_mb.map(|size_mb| size_mb * 1024 * 1024);
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
    for mut project in rx.iter() {
        pb.inc(1);
        if !min_size_bytes.map_or(true, |ms| project.size >= ms) {
            continue;
        }

        let decision = keep_policy.evaluate(&project);
        project.protected = decision.protected;
        project.protected_by = decision.reason;
        project.recent = project.days_since_modified() < recent_days;
        if (!include_protected && project.protected) || (!include_recent && project.recent) {
            continue;
        }

        total_size += project.size;
        let dir_display = project.cleanable_dir.display().to_string();
        let short_path = if dir_display.len() > 50 {
            format!("...{}", &dir_display[dir_display.len() - 47..])
        } else {
            dir_display.clone()
        };
        pb.set_message(format!("{}: {}", short_path, project.size_human()));
        pb.println(format!(
            "  {} {} {} {} ({}){}{}",
            "✓".green(),
            project.project_type_display_name().bright_cyan(),
            format!(
                "[{}/{}/{}]",
                project.category, project.risk_level, project.confidence
            )
            .bright_black(),
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
    let resolved = resolve_scan_inputs(path, profile, config)?;
    let depth = depth.or(resolved.depth).or(config.default_depth);
    let min_size_mb = min_size_mb.or(resolved.min_size_mb).or(config.min_size_mb);
    let older_than = older_than.or(resolved.older_than).or(config.max_age_days);
    let gitignore = gitignore || resolved.gitignore.unwrap_or(false);
    let category = category.to_filter().or(resolved.category);
    let max_risk = if matches!(max_risk, RiskArg::Medium) {
        resolved.max_risk.unwrap_or(max_risk.to_max_risk())
    } else {
        max_risk.to_max_risk()
    };

    let mut projects = scan_projects_for_roots(
        &resolved.roots,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        config,
    )?;
    let keep_policy = KeepPolicy::from_config(config);
    enrich_project_flags(&mut projects, &keep_policy, recent_days);
    projects = filter_by_visibility(projects, include_protected, include_recent);

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

    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run("clean").ok();

    let mut pre_skipped_count = 0usize;
    let mut pre_skipped_bytes = 0u64;
    let mut selected_for_clean = Vec::new();
    for mut project in projects {
        if project.protected && !force_protected {
            project.skip_reason = Some("blocked_protected".to_string());
            pre_skipped_count += 1;
            pre_skipped_bytes = pre_skipped_bytes.saturating_add(project.size);
            if let Some(run_id) = &run_id {
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
            continue;
        }
        if project.recent && !include_recent {
            project.skip_reason = Some("blocked_recent".to_string());
            pre_skipped_count += 1;
            pre_skipped_bytes = pre_skipped_bytes.saturating_add(project.size);
            if let Some(run_id) = &run_id {
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
            continue;
        }
        if project.in_use && !force {
            project.skip_reason = Some("blocked_in_use".to_string());
            pre_skipped_count += 1;
            pre_skipped_bytes = pre_skipped_bytes.saturating_add(project.size);
            if let Some(run_id) = &run_id {
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
            continue;
        }
        selected_for_clean.push(project);
    }

    // Perform cleaning
    let options = CleanOptions {
        dry_run,
        verbose,
        force,
        trash,
    };

    let cleaner = Cleaner::with_options(options);
    let mut result = cleaner.clean_multiple(&selected_for_clean)?;
    result.skipped_count += pre_skipped_count;
    result.bytes_skipped = result.bytes_skipped.saturating_add(pre_skipped_bytes);
    result.run_id = run_id.clone();

    if let Some(run_id) = &run_id {
        for project in &selected_for_clean {
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
    result: &crate::cleaner::CleanResult,
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
    result: &crate::cleaner::CleanResult,
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

        println!(
            "{}. [{}] {} {} - {}{}{}{} ({})",
            (idx + 1).to_string().dimmed(),
            colored_type,
            format!(
                "[{}/{}/{}]",
                project.category, project.risk_level, project.confidence
            )
            .bright_black(),
            project.cleanable_dir.display().to_string().bold(),
            project.size_human().green(),
            in_use,
            protected,
            recent,
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
    let resolved = resolve_scan_inputs(path, profile, config)?;
    let depth = depth.or(resolved.depth).or(config.default_depth);
    let min_size_mb = resolved.min_size_mb.or(config.min_size_mb);
    let older_than = resolved.older_than.or(config.max_age_days);
    let gitignore = gitignore || resolved.gitignore.unwrap_or(false);
    let category = category.to_filter().or(resolved.category);
    let max_risk = if matches!(max_risk, RiskArg::Medium) {
        resolved.max_risk.unwrap_or(max_risk.to_max_risk())
    } else {
        max_risk.to_max_risk()
    };
    let keep_policy = KeepPolicy::from_config(config);

    let mut projects = scan_projects_for_roots(
        &resolved.roots,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        config,
    )?;
    enrich_project_flags(&mut projects, &keep_policy, recent_days);
    projects = filter_by_visibility(projects, include_protected, include_recent);

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
    let resolved = resolve_scan_inputs(path, profile, config)?;
    let depth = depth.or(resolved.depth).or(config.default_depth);
    let min_size_mb = min_size_mb.or(resolved.min_size_mb).or(config.min_size_mb);
    let older_than = older_than.or(resolved.older_than).or(config.max_age_days);
    let gitignore = gitignore || resolved.gitignore.unwrap_or(false);
    let category_filter = category.to_filter().or(resolved.category);
    let max_risk_level = if matches!(max_risk, RiskArg::Medium) {
        resolved.max_risk.unwrap_or(max_risk.to_max_risk())
    } else {
        max_risk.to_max_risk()
    };

    let scan_root = if resolved.roots.len() == 1 {
        fs::canonicalize(&resolved.roots[0]).unwrap_or_else(|_| resolved.roots[0].clone())
    } else {
        PathBuf::from(".")
    };

    let keep_policy = KeepPolicy::from_config(config);
    let mut projects = scan_projects_for_roots(
        &resolved.roots,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category_filter,
        max_risk_level,
        config,
    )?;
    enrich_project_flags(&mut projects, &keep_policy, recent_days);
    projects = filter_by_visibility(projects, include_protected, include_recent);

    let mut params = crate::plan::PlanParams::default();
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

    let resolved = resolve_scan_inputs(path, profile, config)?;
    let scan_root = if resolved.roots.len() == 1 {
        fs::canonicalize(&resolved.roots[0]).unwrap_or_else(|_| resolved.roots[0].clone())
    } else {
        PathBuf::from(".")
    };
    let depth = depth.or(resolved.depth).or(config.default_depth);
    let min_size_mb = min_size_mb.or(resolved.min_size_mb).or(config.min_size_mb);
    let older_than = older_than.or(resolved.older_than).or(config.max_age_days);
    let gitignore = gitignore || resolved.gitignore.unwrap_or(false);
    let category = category.to_filter().or(resolved.category);
    let max_risk = if matches!(max_risk, RiskArg::Medium) {
        resolved.max_risk.unwrap_or(max_risk.to_max_risk())
    } else {
        max_risk.to_max_risk()
    };

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

    let keep_policy = KeepPolicy::from_config(config);
    let mut projects = scan_projects_for_roots(
        &resolved.roots,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        config,
    )?;
    enrich_project_flags(&mut projects, &keep_policy, recent_days);

    let mut opts = RecommendOptions::new(target_bytes);
    opts.include_in_use = include_in_use;
    opts.include_recent = include_recent;
    opts.include_protected = include_protected;
    opts.recent_days = recent_days;
    opts.strategy = strategy.to_strategy();
    opts.max_risk = Some(max_risk);

    let result = recommend_projects(projects, &opts);

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
        projects: result.selected.clone(),
    };

    if let Some(plan_path) = &output_plan {
        let mut params = crate::plan::PlanParams::default();
        params.cleanup_bytes = cleanup_bytes;
        params.free_at_least_bytes = free_at_least_bytes;
        params.max_risk = Some(max_risk);
        params.category = category;
        params.strategy = Some(opts.strategy.as_str().to_string());
        params.recent_days = Some(recent_days);

        let plan = CleanupPlan::new_with_params(scan_root.clone(), result.selected.clone(), params);
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

    if plan.schema_version != 1 && plan.schema_version != 2 && plan.schema_version != 3 {
        anyhow::bail!("Unsupported plan schema_version: {}", plan.schema_version);
    }
    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run("apply").ok();

    let keep_policy = KeepPolicy::from_config(config);
    let mut scanner = Scanner::new(&plan.scan_root)
        .exclude_dirs(&config.exclude_dirs)
        .custom_patterns(&config.custom_patterns);
    if let Some(max_risk) = plan.params.as_ref().and_then(|p| p.max_risk) {
        scanner = scanner.max_risk(max_risk);
    }
    if let Some(category) = plan.params.as_ref().and_then(|p| p.category) {
        scanner = scanner.category(category);
    }

    let mut verified_projects = Vec::new();
    let mut skipped_pre = 0usize;
    let mut skipped_pre_bytes = 0u64;
    for project in &plan.projects {
        if !project.cleanable_dir.starts_with(&plan.scan_root) {
            skipped_pre += 1;
            skipped_pre_bytes = skipped_pre_bytes.saturating_add(project.size);
            if let Some(run_id) = &run_id {
                let _ = audit.log_item(
                    run_id,
                    "apply",
                    &project.cleanable_dir,
                    "verify",
                    "skipped",
                    project.size,
                    Some("outside_scan_root".to_string()),
                );
            }
            continue;
        }

        let mut candidate = if no_verify {
            project.clone()
        } else {
            match scanner.revalidate_target(&project.cleanable_dir) {
                Some(info) => info,
                None => {
                    skipped_pre += 1;
                    skipped_pre_bytes = skipped_pre_bytes.saturating_add(project.size);
                    if let Some(run_id) = &run_id {
                        let _ = audit.log_item(
                            run_id,
                            "apply",
                            &project.cleanable_dir,
                            "verify",
                            "skipped",
                            project.size,
                            Some("rule_mismatch_or_missing".to_string()),
                        );
                    }
                    continue;
                }
            }
        };

        let decision = keep_policy.evaluate(&candidate);
        candidate.protected = decision.protected;
        candidate.protected_by = decision.reason;
        candidate.recent = candidate.days_since_modified() < recent_days;

        if candidate.protected && !force_protected {
            skipped_pre += 1;
            skipped_pre_bytes = skipped_pre_bytes.saturating_add(candidate.size);
            if let Some(run_id) = &run_id {
                let _ = audit.log_item(
                    run_id,
                    "apply",
                    &candidate.cleanable_dir,
                    "verify",
                    "skipped",
                    candidate.size,
                    Some("blocked_protected".to_string()),
                );
            }
            continue;
        }
        if candidate.recent && !include_recent {
            skipped_pre += 1;
            skipped_pre_bytes = skipped_pre_bytes.saturating_add(candidate.size);
            if let Some(run_id) = &run_id {
                let _ = audit.log_item(
                    run_id,
                    "apply",
                    &candidate.cleanable_dir,
                    "verify",
                    "skipped",
                    candidate.size,
                    Some("blocked_recent".to_string()),
                );
            }
            continue;
        }
        if candidate.in_use && !force {
            skipped_pre += 1;
            skipped_pre_bytes = skipped_pre_bytes.saturating_add(candidate.size);
            if let Some(run_id) = &run_id {
                let _ = audit.log_item(
                    run_id,
                    "apply",
                    &candidate.cleanable_dir,
                    "verify",
                    "skipped",
                    candidate.size,
                    Some("blocked_in_use".to_string()),
                );
            }
            continue;
        }
        verified_projects.push(candidate);
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

    if verified_projects.is_empty() {
        println!("{}", "Nothing to clean.".yellow());
        if let Some(run_id) = run_id {
            let _ = audit.finish_run(&run_id, "apply", 0, skipped_pre, 0, 0);
        }
        return Ok(());
    }

    if !force
        && !confirm(&format!(
            "Apply this plan and remove {} directories?",
            verified_projects.len()
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
    let mut result = cleaner.clean_multiple(&verified_projects)?;
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
    let trash_root = default_trash_root();
    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run("trash").ok();

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
            if let Some(run_id) = &run_id {
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
                if let Some(run_id) = &run_id {
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
            if let Some(run_id) = &run_id {
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

    if let Some(run_id) = run_id {
        let _ = audit.finish_run(&run_id, "trash", 0, 0, 0, 0);
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
            let profile = crate::config::ScanProfile {
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
    let resolved = resolve_scan_inputs(path, profile, config)?;
    let depth = resolved.depth.or(config.default_depth);
    let min_size_mb = resolved.min_size_mb.or(config.min_size_mb);
    let older_than = resolved.older_than.or(config.max_age_days);
    let gitignore = resolved.gitignore.unwrap_or(false);
    let category = resolved.category;
    let max_risk = resolved.max_risk.unwrap_or(RiskLevel::Medium);
    let keep_policy = KeepPolicy::from_config(config);

    let mut projects = scan_projects_for_roots(
        &resolved.roots,
        depth,
        min_size_mb,
        older_than,
        gitignore,
        category,
        max_risk,
        config,
    )?;
    enrich_project_flags(&mut projects, &keep_policy, recent_days);
    projects = filter_by_visibility(projects, include_protected, include_recent);
    crate::tui::run_tui_projects(projects, include_recent, include_protected, recent_days)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleaner::CleanResult;

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
}
