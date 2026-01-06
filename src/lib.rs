extern crate core;

mod test;

pub mod api;
pub mod config;
pub mod db;
pub mod logging;
pub mod mixer;
pub mod spawn_task;
pub mod state;
pub mod task;

pub const MAX_NOTES_IN_BATCH_TRANSACTION: usize = 1024;

pub const PACKAGE: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
