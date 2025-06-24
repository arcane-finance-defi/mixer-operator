pub mod api;
pub mod config;
pub mod mixer;
pub mod logging;

pub const PACKAGE: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");