use std::path::PathBuf;
use wasm_bindgen::prelude::*;
use serde::Deserialize;

const DEFAULT_PRIVATE_ACCOUNTS_DIR: &str = "./accounts_for_import";

#[derive(Deserialize)]
#[wasm_bindgen]
pub struct Config {
    rpc_url: String,
    rpc_timeout_ms: u64,
    client_count: u32,
    private_account_dir: Option<String>,
    public_account_ids: String,
}

#[wasm_bindgen]
impl Config {
    #[wasm_bindgen(constructor)]
    pub fn new(
        rpc_url: String,
        rpc_timeout_ms: u64,
        client_count: u32,
        private_account_dir: Option<String>,
        public_account_ids: String,
    ) -> Config {
        Config {
            rpc_url,
            rpc_timeout_ms,
            client_count,
            private_account_dir,
            public_account_ids,
        }
    }

    #[wasm_bindgen(getter)]
    pub fn rpc_url(&self) -> String {
        self.rpc_url.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn rpc_timeout_ms(&self) -> u64 {
        self.rpc_timeout_ms
    }

    #[wasm_bindgen(getter)]
    pub fn client_count(&self) -> u32 {
        self.client_count
    }

    #[wasm_bindgen(getter)]
    pub fn private_account_dir(&self) -> String {
        self.private_account_dir
            .clone()
            .unwrap_or_else(|| DEFAULT_PRIVATE_ACCOUNTS_DIR.to_string())
    }

    #[wasm_bindgen(getter)]
    pub fn public_account_ids(&self) -> Vec<String> {
        self.public_account_ids
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn private_account_dir_path(&self) -> PathBuf {
        self.private_account_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_PRIVATE_ACCOUNTS_DIR))
    }
}