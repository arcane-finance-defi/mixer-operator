use std::path::PathBuf;

use rocket::serde::Deserialize;

const DEFAULT_PRIVATE_ACCOUNTS_DIR: &str = "./accounts_for_import";

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Config {
    debug: Option<bool>,
    client: MidenClient,
    db: Database,
    tq: TaskQueue,
}

impl Config {
    pub fn debug(&self) -> bool {
        self.debug.unwrap_or(false)
    }

    pub fn client(&self) -> &MidenClient {
        &self.client
    }

    pub fn db(&self) -> &Database {
        &self.db
    }

    pub fn task_queue(&self) -> &TaskQueue {
        &self.tq
    }
}

#[derive(Deserialize, Clone)]
#[serde(crate = "rocket::serde")]
pub struct MidenClient {
    pub rpc_url: String,
    pub rpc_timeout_ms: u64,
    pub internal_queue_size: u32,
    pub private_account_dir: Option<PathBuf>,
    pub public_account_ids: String,
}

impl MidenClient {
    pub fn rpc_url(&self) -> String {
        self.rpc_url.clone()
    }

    pub fn rpc_timeout_ms(&self) -> u64 {
        self.rpc_timeout_ms
    }

    pub fn internal_queue_size(&self) -> u32 {
        self.internal_queue_size
    }

    pub fn private_account_dir(&self) -> PathBuf {
        self.private_account_dir.clone().unwrap_or(DEFAULT_PRIVATE_ACCOUNTS_DIR.into())
    }

    pub fn public_account_ids(&self) -> Vec<String> {
        self.public_account_ids.clone().split(',').map(String::from).collect()
    }
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Database {
    pub url: String,
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct TaskQueue {
    pub db_url: String,
    pub db_max_pool: Option<u32>,
    pub workers_max: Option<u32>,
    pub task_max_retry: Option<u64>,
}
