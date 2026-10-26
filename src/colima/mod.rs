//! Colima CLI wrapper. Plain Rust: no `gpui` imports, so it stays unit-testable.

pub mod cache;
pub mod cli;
pub mod commands;
pub mod model;
pub mod paths;

pub use cli::CliError;
pub use model::{Instance, Status};
