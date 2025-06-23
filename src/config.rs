use rocket::serde::Deserialize;
use std::path::PathBuf;

const DEFAULT_PRIVATE_ACCOUNTS_DIR: &str = "./accounts_for_import";

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Config {
    rpc_url: String,
    rpc_timeout_ms: u64,
    client_count: u32,
    private_account_dir: Option<PathBuf>,
    public_account_ids: String,
}

impl Config {
    pub fn rpc_url(&self) -> String {
        self.rpc_url.clone()
    }

    pub fn rpc_timeout_ms(&self) -> u64 {
        self.rpc_timeout_ms
    }

    pub fn client_count(&self) -> u32 {
        self.client_count
    }

    pub fn private_account_dir(&self) -> PathBuf {
        self.private_account_dir
            .clone()
            .or(Some(DEFAULT_PRIVATE_ACCOUNTS_DIR.into()))
            .unwrap()
    }

    pub fn public_account_ids(&self) -> Vec<String> {
        self.public_account_ids
            .clone()
            .split(',')
            .map(String::from)
            .collect()
    }
}
