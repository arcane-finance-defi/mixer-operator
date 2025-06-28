pub mod api;
pub mod config;
pub mod db;
pub mod logging;
pub mod mixer;
pub mod state;

pub const PACKAGE: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
