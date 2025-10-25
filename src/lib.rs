pub mod scanner;
pub mod cleaner;
pub mod config;
pub mod cli;
pub mod tui;
pub mod stats;

// Re-export commonly used types
pub use scanner::{Scanner, ProjectInfo, ProjectType};
pub use cleaner::Cleaner;
pub use config::Config;
pub use stats::Statistics;
