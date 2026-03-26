pub mod apply_plan;
pub mod cleanup;
pub mod evaluated;
pub mod scan;

pub use apply_plan::{ApplyPlanRequest, ApplyPlanResult, ApplyPlanService};
pub use cleanup::{BlockedSummary, CleanupRequest, CleanupSelection, CleanupService};
pub use evaluated::{EvaluatedProject, SafetyFlags, SelectionReason, SkipReason};
pub use scan::{
    canonicalize_lossy, common_ancestor, derive_scan_root, DiscoveredProjects, ResolvedScanInput,
    ScanRequest, ScanResult, ScanService, VisibilityOptions,
};
