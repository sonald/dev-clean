pub mod app;
pub mod audit;
pub mod cleaner;
pub mod cli;
pub mod config;
pub mod evaluation;
pub mod interactive;
pub mod metrics;
pub mod plan;
pub mod policy;
pub mod recommend;
pub mod scanner;
pub mod stats;
pub mod trash;
pub mod tui;
pub mod utils;

// Re-export commonly used types
pub use cleaner::Cleaner;
pub use config::Config;
pub use evaluation::{EvaluatedProject, SafetyFlags, SelectionReason, SkipReason};
pub use plan::CleanupPlan;
pub use scanner::{ProjectInfo, ProjectType, Scanner};
pub use stats::Statistics;
pub use trash::{TrashEntry, TrashManager};
