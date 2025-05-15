use rocket::serde::Deserialize;

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
pub struct Config {
    rpc_url: String,
    rpc_timeout_ms: u64,
    client_count: u32,
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
}
