use anyhow::{Context, Result};
use clap::{Subcommand, ValueEnum};
use dev_cleaner_core::app::{
    ApplyPlanRequest, ApplyPlanService, ScanRequest, ScanService, VisibilityOptions,
};
use dev_cleaner_core::audit::AuditLogger;
use dev_cleaner_core::cleaner::{CleanAction, CleanObserver, CleanOptions};
use dev_cleaner_core::recommend::{recommend_projects, RecommendOptions, RecommendStrategy};
use dev_cleaner_core::scanner::{Category, ProjectInfo, RiskLevel};
use dev_cleaner_core::trash::{
    default_trash_root, gc_trash, list_trash_batches, purge_trash_batch,
    restore_batch_with_observer, trash_entries_for_batch, RestoreObserver, TrashEntry,
};
use dev_cleaner_core::utils::{format_size, parse_size};
use dev_cleaner_core::{Cleaner, CleanupPlan, Config};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{self, Read, Write};
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum BridgeCommands {
    /// Stream scan results as JSONL events.
    Scan(BridgeScanArgs),
    /// Generate a recommendation preview and optional plan.
    Recommend(BridgeRecommendArgs),
    /// Scan and clean all matching items.
    Clean(BridgeCleanArgs),
    /// Apply a cleanup plan.
    Apply(BridgeApplyArgs),
    /// Manage internal trash.
    Trash {
        #[command(subcommand)]
        command: BridgeTrashCommands,
    },
    /// Query audit records.
    Audit {
        #[command(subcommand)]
        command: BridgeAuditCommands,
    },
    /// Read or save app configuration snapshots.
    Config {
        #[command(subcommand)]
        command: BridgeConfigCommands,
    },
}

#[derive(clap::Args)]
pub struct BridgeScanArgs {
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub depth: Option<usize>,
    #[arg(long)]
    pub min_size: Option<u64>,
    #[arg(long)]
    pub older_than: Option<i64>,
    #[arg(long)]
    pub gitignore: bool,
    #[arg(long, value_enum, default_value = "all")]
    pub category: BridgeCategoryArg,
    #[arg(long, value_enum, default_value = "medium")]
    pub max_risk: BridgeRiskArg,
    #[arg(long)]
    pub include_protected: bool,
    #[arg(long)]
    pub include_recent: bool,
    #[arg(long, default_value = "7")]
    pub recent_days: i64,
}

#[derive(clap::Args)]
pub struct BridgeRecommendArgs {
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub depth: Option<usize>,
    #[arg(long)]
    pub min_size: Option<u64>,
    #[arg(long)]
    pub older_than: Option<i64>,
    #[arg(long)]
    pub gitignore: bool,
    #[arg(long)]
    pub cleanup: Option<String>,
    #[arg(long)]
    pub free_at_least: Option<String>,
    #[arg(long, value_enum, default_value = "balanced")]
    pub strategy: BridgeStrategyArg,
    #[arg(long, value_enum, default_value = "medium")]
    pub max_risk: BridgeRiskArg,
    #[arg(long, value_enum, default_value = "all")]
    pub category: BridgeCategoryArg,
    #[arg(long)]
    pub include_in_use: bool,
    #[arg(long)]
    pub include_recent: bool,
    #[arg(long)]
    pub include_protected: bool,
    #[arg(long, default_value = "7")]
    pub recent_days: i64,
    #[arg(long)]
    pub output_plan: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct BridgeCleanArgs {
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub depth: Option<usize>,
    #[arg(long)]
    pub min_size: Option<u64>,
    #[arg(long)]
    pub older_than: Option<i64>,
    #[arg(long)]
    pub gitignore: bool,
    #[arg(long, value_enum, default_value = "all")]
    pub category: BridgeCategoryArg,
    #[arg(long, value_enum, default_value = "medium")]
    pub max_risk: BridgeRiskArg,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub trash: bool,
    #[arg(long)]
    pub permanent_delete: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub include_recent: bool,
    #[arg(long)]
    pub include_protected: bool,
    #[arg(long)]
    pub force_protected: bool,
    #[arg(long, default_value = "7")]
    pub recent_days: i64,
    #[arg(long)]
    pub cancel_file: Option<PathBuf>,
}

#[derive(clap::Args)]
pub struct BridgeApplyArgs {
    pub plan: PathBuf,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub trash: bool,
    #[arg(long)]
    pub permanent_delete: bool,
    #[arg(long)]
    pub force: bool,
    #[arg(long)]
    pub no_verify: bool,
    #[arg(long)]
    pub include_recent: bool,
    #[arg(long)]
    pub force_protected: bool,
    #[arg(long, default_value = "7")]
    pub recent_days: i64,
    #[arg(long)]
    pub cancel_file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum BridgeTrashCommands {
    List {
        #[arg(long, default_value = "20")]
        top: usize,
    },
    Show {
        #[arg(long)]
        batch: String,
    },
    Restore {
        #[arg(long)]
        batch: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        force: bool,
    },
    Purge {
        #[arg(long)]
        batch: String,
        #[arg(long)]
        dry_run: bool,
    },
    Gc {
        #[arg(long)]
        keep_days: Option<i64>,
        #[arg(long)]
        keep_gb: Option<u64>,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum BridgeAuditCommands {
    List {
        #[arg(long, default_value = "50")]
        top: usize,
    },
    Show {
        #[arg(long)]
        run: String,
    },
    Export {
        #[arg(long)]
        run: Option<String>,
        #[arg(long, value_enum, default_value = "json")]
        format: BridgeExportFormatArg,
    },
}

#[derive(Subcommand)]
pub enum BridgeConfigCommands {
    Get,
    Save {
        #[arg(long)]
        input: Option<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BridgeCategoryArg {
    Cache,
    Build,
    Deps,
    All,
}

impl BridgeCategoryArg {
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
pub enum BridgeRiskArg {
    Low,
    Medium,
    High,
    All,
}

impl BridgeRiskArg {
    fn to_max_risk(self) -> RiskLevel {
        match self {
            Self::Low => RiskLevel::Low,
            Self::Medium => RiskLevel::Medium,
            Self::High | Self::All => RiskLevel::High,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BridgeStrategyArg {
    Safe,
    Balanced,
    Maximum,
}

impl BridgeStrategyArg {
    fn to_strategy(self) -> RecommendStrategy {
        match self {
            Self::Safe => RecommendStrategy::SafeFirst,
            Self::Balanced => RecommendStrategy::Balanced,
            Self::Maximum => RecommendStrategy::MaxSpace,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BridgeExportFormatArg {
    Json,
    Csv,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BridgeEvent {
    Ready {
        version: String,
    },
    ScanStarted {
        roots: Vec<String>,
        total: usize,
    },
    ScanItem {
        project: ProjectInfo,
    },
    ScanProgress {
        completed: usize,
        total: usize,
    },
    ScanFinished {
        total_count: usize,
        total_bytes: u64,
    },
    RecommendationReady {
        payload: serde_json::Value,
    },
    CleanupStarted {
        total_count: usize,
        total_bytes: u64,
        mode: String,
    },
    CleanupProject {
        path: String,
        size: u64,
    },
    CleanupDryRun {
        path: String,
        action: String,
        size: u64,
    },
    CleanupSkipped {
        path: String,
        reason: String,
        size: u64,
    },
    CleanupCompleted {
        path: String,
        size: u64,
    },
    CleanupFailed {
        path: String,
        error: String,
    },
    CleanupCancelled {
        remaining: usize,
    },
    CleanupFinished {
        payload: serde_json::Value,
    },
    TrashList {
        payload: serde_json::Value,
    },
    TrashEntries {
        payload: serde_json::Value,
    },
    TrashOperationFinished {
        payload: serde_json::Value,
    },
    AuditList {
        payload: serde_json::Value,
    },
    AuditRecords {
        payload: serde_json::Value,
    },
    AuditExport {
        payload: serde_json::Value,
    },
    ConfigSnapshot {
        payload: BridgeConfigSnapshot,
    },
    ConfigSaved {
        path: String,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BridgeConfigSnapshot {
    pub config_path: PathBuf,
    pub config: Config,
    #[serde(default)]
    pub gui_preferences: GuiPreferences,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GuiPreferences {
    #[serde(default = "default_appearance")]
    pub appearance: String,
    #[serde(default = "default_scan_root_path")]
    pub scan_root_path: String,
    #[serde(default)]
    pub launch_at_login: bool,
    #[serde(default = "default_true")]
    pub show_menubar_icon: bool,
    #[serde(default)]
    pub alerts_enabled: bool,
    #[serde(default)]
    pub notification_threshold_gb: u64,
    #[serde(default = "default_trash_retention_days")]
    pub trash_retention_days: i64,
    #[serde(default = "default_trash_limit_gb")]
    pub trash_limit_gb: u64,
}

impl Default for GuiPreferences {
    fn default() -> Self {
        Self {
            appearance: default_appearance(),
            scan_root_path: default_scan_root_path(),
            launch_at_login: false,
            show_menubar_icon: true,
            alerts_enabled: false,
            notification_threshold_gb: 1,
            trash_retention_days: default_trash_retention_days(),
            trash_limit_gb: default_trash_limit_gb(),
        }
    }
}

fn default_appearance() -> String {
    "dark".to_string()
}

fn default_scan_root_path() -> String {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .display()
        .to_string()
}

fn default_true() -> bool {
    true
}

fn default_trash_retention_days() -> i64 {
    30
}

fn default_trash_limit_gb() -> u64 {
    10
}

pub fn run_bridge(command: BridgeCommands, config: &Config, config_path: PathBuf) -> Result<()> {
    match command {
        BridgeCommands::Scan(args) => bridge_scan(args, config),
        BridgeCommands::Recommend(args) => bridge_recommend(args, config),
        BridgeCommands::Clean(args) => bridge_clean(args, config),
        BridgeCommands::Apply(args) => bridge_apply(args, config),
        BridgeCommands::Trash { command } => bridge_trash(command),
        BridgeCommands::Audit { command } => bridge_audit(command, config),
        BridgeCommands::Config { command } => bridge_config(command, config, config_path),
    }
}

fn emit(event: &BridgeEvent) {
    let mut stdout = io::stdout().lock();
    let _ = serde_json::to_writer(&mut stdout, event);
    let _ = stdout.write_all(b"\n");
    let _ = stdout.flush();
}

fn build_scan_request(args: &BridgeScanArgs) -> ScanRequest {
    ScanRequest {
        path: args.path.clone(),
        profile: None,
        depth: args.depth,
        min_size_mb: args.min_size,
        older_than_days: args.older_than,
        gitignore: args.gitignore.then_some(true),
        category: args.category.to_filter(),
        max_risk: (!matches!(args.max_risk, BridgeRiskArg::Medium))
            .then_some(args.max_risk.to_max_risk()),
        visibility: VisibilityOptions {
            include_protected: args.include_protected,
            include_recent: args.include_recent,
            recent_days: args.recent_days,
        },
    }
}

fn bridge_scan(args: BridgeScanArgs, config: &Config) -> Result<()> {
    emit(&BridgeEvent::Ready {
        version: env!("CARGO_PKG_VERSION").to_string(),
    });

    let service = ScanService::new();
    let request = build_scan_request(&args);
    let resolved = service.resolve_inputs(config, &request)?;
    let mut projects = Vec::new();
    let mut total_seen = 0usize;
    let mut completed = 0usize;

    for root in &resolved.roots {
        let scanner = service.build_scanner(root, config, &resolved);
        let (total, rx) = scanner.scan_with_streaming()?;
        total_seen += total;
        emit(&BridgeEvent::ScanStarted {
            roots: resolved
                .roots
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
            total: total_seen,
        });

        for project in rx.iter() {
            completed += 1;
            if !resolved
                .min_size_bytes
                .map_or(true, |min_size_bytes| project.size >= min_size_bytes)
            {
                emit(&BridgeEvent::ScanProgress {
                    completed,
                    total: total_seen,
                });
                continue;
            }

            let evaluated = service.evaluate_project_with_config(
                config,
                project,
                resolved.visibility.recent_days,
            );
            if resolved.visibility.is_visible(&evaluated) {
                let project = evaluated.into_project_info();
                emit(&BridgeEvent::ScanItem {
                    project: project.clone(),
                });
                projects.push(project);
            }
            emit(&BridgeEvent::ScanProgress {
                completed,
                total: total_seen,
            });
        }
    }

    projects.sort_by(|a, b| b.size.cmp(&a.size));
    emit(&BridgeEvent::ScanFinished {
        total_count: projects.len(),
        total_bytes: projects.iter().map(|p| p.size).sum(),
    });
    Ok(())
}

fn bridge_recommend(args: BridgeRecommendArgs, config: &Config) -> Result<()> {
    let scan_args = BridgeScanArgs {
        path: args.path.clone(),
        depth: args.depth,
        min_size: args.min_size,
        older_than: args.older_than,
        gitignore: args.gitignore,
        category: args.category,
        max_risk: args.max_risk,
        include_protected: true,
        include_recent: true,
        recent_days: args.recent_days,
    };
    let service = ScanService::new();
    let request = build_scan_request(&scan_args);
    let discovered = service.discover(config, &request)?;
    let cleanup_bytes = args.cleanup.as_deref().map(parse_size).transpose()?;
    let free_at_least_bytes = args.free_at_least.as_deref().map(parse_size).transpose()?;
    let target_bytes = match (cleanup_bytes, free_at_least_bytes) {
        (Some(_), Some(_)) => anyhow::bail!("Use either --cleanup or --free-at-least, not both"),
        (Some(bytes), None) => bytes,
        (None, Some(want_free)) => {
            let free_now = fs2::available_space(&discovered.resolved.scan_root)?;
            want_free.saturating_sub(free_now)
        }
        (None, None) => parse_size("10GB")?,
    };

    let mut options = RecommendOptions::new(target_bytes);
    options.include_in_use = args.include_in_use;
    options.include_recent = args.include_recent;
    options.include_protected = args.include_protected;
    options.recent_days = args.recent_days;
    options.strategy = args.strategy.to_strategy();
    options.max_risk = Some(discovered.resolved.max_risk);

    let candidates = discovered
        .projects
        .into_iter()
        .map(ProjectInfo::from)
        .collect::<Vec<_>>();
    let result = recommend_projects(candidates, &options);
    let selected_projects = result
        .selected
        .iter()
        .map(ProjectInfo::from)
        .collect::<Vec<_>>();

    if let Some(path) = &args.output_plan {
        let params = dev_cleaner_core::plan::PlanParams {
            cleanup_bytes,
            free_at_least_bytes,
            max_risk: Some(discovered.resolved.max_risk),
            category: discovered.resolved.category,
            verify_mode: Some("revalidate".to_string()),
            strategy: Some(options.strategy.as_str().to_string()),
            recent_days: Some(args.recent_days),
        };
        CleanupPlan::new_with_params(
            discovered.resolved.scan_root.clone(),
            selected_projects.clone(),
            params,
        )
        .save_json(path)?;
    }

    let payload = json!({
        "scan_root": discovered.resolved.scan_root,
        "target_bytes": result.target_bytes,
        "selected_bytes": result.selected_bytes,
        "selected_size_human": format_size(result.selected_bytes),
        "selected_count": result.selected.len(),
        "strategy": options.strategy.as_str(),
        "blocked": {
            "in_use": { "count": result.blocked.in_use_count, "bytes": result.blocked.in_use_bytes },
            "protected": { "count": result.blocked.protected_count, "bytes": result.blocked.protected_bytes },
            "recent": { "count": result.blocked.recent_count, "bytes": result.blocked.recent_bytes },
            "risk": { "count": result.blocked.risk_count, "bytes": result.blocked.risk_bytes }
        },
        "projects": selected_projects,
        "plan_path": args.output_plan
    });
    emit(&BridgeEvent::RecommendationReady { payload });
    Ok(())
}

fn bridge_clean(args: BridgeCleanArgs, config: &Config) -> Result<()> {
    let scan_args = BridgeScanArgs {
        path: args.path,
        depth: args.depth,
        min_size: args.min_size,
        older_than: args.older_than,
        gitignore: args.gitignore,
        category: args.category,
        max_risk: args.max_risk,
        include_protected: args.include_protected,
        include_recent: args.include_recent,
        recent_days: args.recent_days,
    };
    let service = ScanService::new();
    let request = build_scan_request(&scan_args);
    let projects = service
        .discover_visible(config, &request)?
        .projects
        .into_iter()
        .map(ProjectInfo::from)
        .collect::<Vec<_>>();
    run_cleaner(
        projects,
        args.dry_run,
        args.trash,
        args.permanent_delete,
        args.force,
        args.include_recent,
        args.force_protected,
        args.cancel_file,
        config,
        "clean",
    )
}

fn bridge_apply(args: BridgeApplyArgs, config: &Config) -> Result<()> {
    let plan = CleanupPlan::load_json(&args.plan)?;
    let result = ApplyPlanService::new().verify(
        config,
        ApplyPlanRequest {
            plan,
            no_verify: args.no_verify,
            include_recent: args.include_recent,
            force: args.force,
            force_protected: args.force_protected,
            recent_days: args.recent_days,
        },
    )?;
    let projects = result
        .verified_projects
        .into_iter()
        .map(ProjectInfo::from)
        .collect::<Vec<_>>();
    run_cleaner(
        projects,
        args.dry_run,
        args.trash,
        args.permanent_delete,
        args.force,
        args.include_recent,
        args.force_protected,
        args.cancel_file,
        config,
        "apply",
    )
}

fn run_cleaner(
    projects: Vec<ProjectInfo>,
    dry_run: bool,
    trash: bool,
    permanent_delete: bool,
    force: bool,
    include_recent: bool,
    force_protected: bool,
    cancel_file: Option<PathBuf>,
    config: &Config,
    audit_command: &'static str,
) -> Result<()> {
    if trash && permanent_delete {
        anyhow::bail!("Use either --trash or --permanent-delete, not both");
    }
    let trash = !permanent_delete || trash;
    let mode = if dry_run {
        "dry_run"
    } else if trash {
        "trash"
    } else {
        "permanent_delete"
    };
    emit(&BridgeEvent::CleanupStarted {
        total_count: projects.len(),
        total_bytes: projects.iter().map(|p| p.size).sum(),
        mode: mode.to_string(),
    });
    let cleaner = Cleaner::with_options(CleanOptions {
        dry_run,
        verbose: false,
        force,
        include_recent,
        force_protected,
        trash,
        trash_root: None,
        cancel_file,
    });
    let audit = AuditLogger::from_config(config);
    let run_id = audit.start_run(audit_command).ok();
    let mut observer = BridgeCleanObserver::new(if dry_run {
        "dry_run"
    } else if trash {
        "trash"
    } else {
        "remove"
    });
    let result = cleaner.clean_multiple_with_observer(&projects, &mut observer)?;
    if let Some(run_id) = &run_id {
        for item in &observer.audit_items {
            let _ = audit.log_item(
                run_id,
                audit_command,
                &item.path,
                item.action,
                item.result,
                item.bytes,
                item.reason.clone(),
            );
        }
        let _ = audit.finish_run(
            run_id,
            audit_command,
            result.cleaned_count,
            result.skipped_count,
            result.failed_count,
            result.bytes_freed,
        );
    }
    emit(&BridgeEvent::CleanupFinished {
        payload: json!({
            "cleaned_count": result.cleaned_count,
            "bytes_freed": result.bytes_freed,
            "skipped_count": result.skipped_count,
            "bytes_skipped": result.bytes_skipped,
            "failed_count": result.failed_count,
            "errors": result.errors,
            "trash_batch_id": result.trash_batch_id,
            "run_id": run_id,
            "cancelled": observer.cancelled,
        }),
    });
    Ok(())
}

#[derive(Default)]
struct BridgeCleanObserver {
    cancelled: bool,
    audit_action: &'static str,
    audit_items: Vec<BridgeAuditItem>,
}

struct BridgeAuditItem {
    path: PathBuf,
    action: &'static str,
    result: &'static str,
    bytes: u64,
    reason: Option<String>,
}

impl CleanObserver for BridgeCleanObserver {
    fn on_project(&mut self, project: &ProjectInfo) {
        emit(&BridgeEvent::CleanupProject {
            path: project.cleanable_dir.display().to_string(),
            size: project.size,
        });
    }

    fn on_skipped_in_use(&mut self, project: &ProjectInfo) {
        self.skipped(project, "in_use");
    }

    fn on_skipped_protected(&mut self, project: &ProjectInfo) {
        self.skipped(project, "protected");
    }

    fn on_skipped_recent(&mut self, project: &ProjectInfo) {
        self.skipped(project, "recent");
    }

    fn on_dry_run(&mut self, project: &ProjectInfo, action: CleanAction) {
        emit(&BridgeEvent::CleanupDryRun {
            path: project.cleanable_dir.display().to_string(),
            action: match action {
                CleanAction::Delete => "delete",
                CleanAction::Trash => "trash",
            }
            .to_string(),
            size: project.size,
        });
        self.audit_items.push(BridgeAuditItem {
            path: project.cleanable_dir.clone(),
            action: "dry_run",
            result: "dry_run",
            bytes: project.size,
            reason: Some(
                match action {
                    CleanAction::Delete => "delete",
                    CleanAction::Trash => "trash",
                }
                .to_string(),
            ),
        });
    }

    fn on_cleaned(&mut self, project: &ProjectInfo, size: u64) {
        emit(&BridgeEvent::CleanupCompleted {
            path: project.cleanable_dir.display().to_string(),
            size,
        });
        self.audit_items.push(BridgeAuditItem {
            path: project.cleanable_dir.clone(),
            action: self.audit_action,
            result: "completed",
            bytes: size,
            reason: None,
        });
    }

    fn on_failed(&mut self, project: &ProjectInfo, error: &anyhow::Error) {
        emit(&BridgeEvent::CleanupFailed {
            path: project.cleanable_dir.display().to_string(),
            error: error.to_string(),
        });
        self.audit_items.push(BridgeAuditItem {
            path: project.cleanable_dir.clone(),
            action: self.audit_action,
            result: "failed",
            bytes: project.size,
            reason: Some(error.to_string()),
        });
    }

    fn on_cancelled(&mut self, remaining_projects: usize) {
        self.cancelled = true;
        emit(&BridgeEvent::CleanupCancelled {
            remaining: remaining_projects,
        });
    }
}

impl BridgeCleanObserver {
    fn new(audit_action: &'static str) -> Self {
        Self {
            cancelled: false,
            audit_action,
            audit_items: Vec::new(),
        }
    }

    fn skipped(&mut self, project: &ProjectInfo, reason: &str) {
        emit(&BridgeEvent::CleanupSkipped {
            path: project.cleanable_dir.display().to_string(),
            reason: reason.to_string(),
            size: project.size,
        });
        self.audit_items.push(BridgeAuditItem {
            path: project.cleanable_dir.clone(),
            action: self.audit_action,
            result: "skipped",
            bytes: project.size,
            reason: Some(reason.to_string()),
        });
    }
}

fn bridge_trash(command: BridgeTrashCommands) -> Result<()> {
    let root = default_trash_root();
    match command {
        BridgeTrashCommands::List { top } => {
            let batches = list_trash_batches(&root)?
                .into_iter()
                .take(top)
                .collect::<Vec<_>>();
            emit(&BridgeEvent::TrashList {
                payload: json!({ "trash_root": root, "batches": batches }),
            });
        }
        BridgeTrashCommands::Show { batch } => {
            let entries = trash_entries_for_batch(&root, &batch)?;
            emit(&BridgeEvent::TrashEntries {
                payload: json!({ "trash_root": root, "batch": batch, "entries": entries }),
            });
        }
        BridgeTrashCommands::Restore {
            batch,
            dry_run,
            force,
        } => {
            let mut observer = BridgeRestoreObserver;
            let result = restore_batch_with_observer(&root, &batch, dry_run, force, &mut observer)?;
            emit(&BridgeEvent::TrashOperationFinished {
                payload: json!({ "operation": "restore", "batch": batch, "result": result }),
            });
        }
        BridgeTrashCommands::Purge { batch, dry_run } => {
            let result = purge_trash_batch(&root, &batch, dry_run)?;
            emit(&BridgeEvent::TrashOperationFinished {
                payload: json!({ "operation": "purge", "batch": batch, "result": result }),
            });
        }
        BridgeTrashCommands::Gc {
            keep_days,
            keep_gb,
            dry_run,
        } => {
            let keep_bytes = keep_gb.map(|gb| gb.saturating_mul(1024 * 1024 * 1024));
            let result = gc_trash(&root, keep_days, keep_bytes, dry_run)?;
            emit(&BridgeEvent::TrashOperationFinished {
                payload: json!({ "operation": "gc", "result": result }),
            });
        }
    }
    Ok(())
}

struct BridgeRestoreObserver;

impl RestoreObserver for BridgeRestoreObserver {
    fn on_dry_run(&mut self, entry: &TrashEntry) {
        emit(&BridgeEvent::TrashOperationFinished {
            payload: json!({ "operation": "restore_dry_run_item", "entry": entry }),
        });
    }

    fn on_restored(&mut self, entry: &TrashEntry) {
        emit(&BridgeEvent::TrashOperationFinished {
            payload: json!({ "operation": "restore_item", "entry": entry }),
        });
    }
}

fn bridge_audit(command: BridgeAuditCommands, config: &Config) -> Result<()> {
    let logger = AuditLogger::from_config(config);
    match command {
        BridgeAuditCommands::List { top } => {
            let runs = logger
                .list_runs()?
                .into_iter()
                .take(top)
                .collect::<Vec<_>>();
            emit(&BridgeEvent::AuditList {
                payload: json!({ "path": logger.path(), "runs": runs }),
            });
        }
        BridgeAuditCommands::Show { run } => {
            let records = logger.records_for_run(&run)?;
            emit(&BridgeEvent::AuditRecords {
                payload: json!({ "run": run, "records": records }),
            });
        }
        BridgeAuditCommands::Export { run, format } => {
            let records = if let Some(run) = run {
                logger.records_for_run(&run)?
            } else {
                logger.read_records()?
            };
            let payload = match format {
                BridgeExportFormatArg::Json => {
                    json!({ "format": "json", "content": serde_json::to_string_pretty(&records)? })
                }
                BridgeExportFormatArg::Csv => {
                    json!({ "format": "csv", "content": AuditLogger::export_csv(&records) })
                }
            };
            emit(&BridgeEvent::AuditExport { payload });
        }
    }
    Ok(())
}

fn bridge_config(
    command: BridgeConfigCommands,
    config: &Config,
    config_path: PathBuf,
) -> Result<()> {
    match command {
        BridgeConfigCommands::Get => {
            emit(&BridgeEvent::ConfigSnapshot {
                payload: BridgeConfigSnapshot {
                    config_path,
                    config: config.clone(),
                    gui_preferences: load_gui_preferences()?,
                },
            });
        }
        BridgeConfigCommands::Save { input } => {
            let snapshot = read_config_snapshot(input)?;
            if let Some(parent) = config_path.parent() {
                fs::create_dir_all(parent)?;
            }
            snapshot.config.save(&config_path)?;
            save_gui_preferences(&snapshot.gui_preferences)?;
            emit(&BridgeEvent::ConfigSaved {
                path: config_path.display().to_string(),
            });
        }
    }
    Ok(())
}

fn read_config_snapshot(input: Option<PathBuf>) -> Result<BridgeConfigSnapshot> {
    let mut content = String::new();
    if let Some(path) = input {
        content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config snapshot {}", path.display()))?;
    } else {
        io::stdin().read_to_string(&mut content)?;
    }
    Ok(serde_json::from_str(&content)?)
}

fn gui_preferences_path() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("dev-cleaner")
        .join("preferences.json")
}

fn load_gui_preferences() -> Result<GuiPreferences> {
    let path = gui_preferences_path();
    if !path.exists() {
        return Ok(GuiPreferences::default());
    }
    let content = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&content).unwrap_or_default())
}

fn save_gui_preferences(preferences: &GuiPreferences) -> Result<()> {
    let path = gui_preferences_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(preferences)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_event_roundtrips() {
        let event = BridgeEvent::CleanupStarted {
            total_count: 2,
            total_bytes: 42,
            mode: "trash".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        let decoded: BridgeEvent = serde_json::from_str(&json).unwrap();
        let decoded_json = serde_json::to_value(decoded).unwrap();
        assert_eq!(decoded_json["type"], "cleanup_started");
        assert_eq!(decoded_json["total_count"], 2);
        assert_eq!(decoded_json["mode"], "trash");
    }

    #[test]
    fn gui_preferences_have_safe_defaults() {
        let prefs = GuiPreferences::default();
        assert_eq!(prefs.appearance, "dark");
        assert!(!prefs.scan_root_path.is_empty());
        assert!(prefs.show_menubar_icon);
        assert_eq!(prefs.trash_retention_days, 30);
    }
}
