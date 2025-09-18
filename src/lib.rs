extern crate core;

mod test;

pub mod api;
pub mod config;
pub mod db;
pub mod executor;
pub mod logging;
pub mod mixer;
pub mod named_future;
pub mod state;
pub mod task;

pub const PACKAGE: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
