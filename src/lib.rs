// ponytail: command handlers take their flattened clap flags as arguments; bundling them
// into structs only to satisfy this lint would add types nobody else uses.
#![allow(clippy::too_many_arguments)]

pub mod bridge;
mod clean_progress;
pub mod cli;
pub mod interactive;
pub mod stats;
pub mod tui;

pub use dev_cleaner_core as core;
pub use dev_cleaner_core::{
    recommend_projects, ApplyPlanRequest, ApplyPlanResult, ApplyPlanService, Category, CleanAction,
    CleanObserver, CleanOptions, CleanResult, Cleaner, CleanupPlan, CleanupRequest,
    CleanupSelection, CleanupService, Confidence, Config, CustomPattern, DiscoveredProjects,
    EvaluatedProject, PlanParams, ProjectDetector, ProjectInfo, ProjectType, RecommendOptions,
    RecommendResult, RecommendStrategy, RiskLevel, RuleRef, RuleSource, SafetyFlags, ScanRequest,
    ScanResult, ScanService, Scanner, SelectionReason, SizeCalculator, SkipReason, Statistics,
    TrashEntry, TrashManager, VisibilityOptions,
};
