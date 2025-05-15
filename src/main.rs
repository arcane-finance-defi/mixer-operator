use crate::config::Config;
use dotenv::dotenv;
use rocket::State as RocketState;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, routes};
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use std::sync::Arc;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate alloc;
extern crate core;

mod config;
mod rpc;
mod rpc2;
pub use rpc::RpcClientWorker;
pub use rpc::commands::client::RpcClient;
pub use rpc2::ThreadPoolMidenRpcAsyncFacade;

struct State {
    rpc: Arc<ThreadPoolMidenRpcAsyncFacade>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ErrorResponse {
    error: String,
}

#[get("/chain-tip")]
async fn chain_tip(state: &RocketState<State>) -> Result<Json<u32>, Status> {
    let chain_tip = state
        .rpc
        .get_chain_tip()
        .await
        .map_err(|_e| Status::InternalServerError)?;

    Ok(Json::from(chain_tip.deref().clone()))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let rocket = rocket::build();

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let endpoint = miden_client::rpc::Endpoint::new("http".to_string(), config.rpc_url(), None);

    let client_count = config.client_count();

    let miden_facade = Arc::new(ThreadPoolMidenRpcAsyncFacade::new(
        client_count,
        &endpoint,
        config.rpc_timeout_ms(),
    ));

    let state = State {
        rpc: miden_facade,
    };

    rocket
        .manage(state)
        .mount("/", routes![chain_tip])
        .launch()
        .await
        .unwrap();

    Ok(())
}
