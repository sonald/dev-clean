pub mod app;
pub mod audit;
pub mod cleaner;
pub mod config;
pub mod evaluation;
pub mod plan;
pub mod policy;
pub mod recommend;
pub mod scanner;
pub mod stats;
pub mod trash;
pub mod utils;

pub use app::{
    canonicalize_lossy, common_ancestor, derive_scan_root, ApplyPlanRequest, ApplyPlanResult,
    ApplyPlanService, BlockedSummary as CleanupBlockedSummary, CleanupRequest, CleanupSelection,
    CleanupService, DiscoveredProjects, ResolvedScanInput, ScanRequest, ScanResult, ScanService,
    VisibilityOptions,
};
pub use audit::{AuditLogger, AuditRecord, AuditRunSummary};
pub use cleaner::{CleanAction, CleanObserver, CleanOptions, CleanResult, Cleaner};
pub use config::{AuditConfig, Config, CustomPattern, MarkerMode, ScanProfile};
pub use evaluation::{EvaluatedProject, SafetyFlags, SelectionReason, SkipReason};
pub use plan::{CleanupPlan, PlanParams};
pub use recommend::{recommend_projects, RecommendOptions, RecommendResult, RecommendStrategy};
pub use scanner::{
    Category, Confidence, ProjectDetector, ProjectInfo, ProjectType, RiskLevel, RuleRef,
    RuleSource, Scanner, SizeCalculator,
};
pub use stats::Statistics;
pub use trash::{
    default_trash_root, gc_trash, latest_batch_id, list_trash_batches, purge_trash_batch,
    restore_batch, restore_batch_with_observer, trash_entries_for_batch, GcResult, PurgeResult,
    RestoreObserver, RestoreResult, TrashBatchSummary, TrashEntry, TrashManager,
};
