use std::ops::Deref;
use std::sync::Arc;
use dotenv::dotenv;
use rocket::{get, routes};
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use crate::config::Config;
use crate::rpc::client::RpcClient;
use crate::rpc::RpcClientWorker;
use rocket::State as RocketState;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate alloc;
extern crate core;
extern crate core;
extern crate core;
extern crate core;

mod rpc;
mod config;
mod transaction;
mod chain;
mod sync;

struct State {
    rpc: Arc<RpcClient>
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ErrorResponse {
    error: String,
}

#[get("/chain-tip")]
async fn chain_tip(state: &RocketState<State>) -> Result<Json<u32>, Status> {
    let chain_tip = state.rpc.get_chain_tip().await
        .map_err(|e| Status::InternalServerError)?;

    Ok(Json::from(chain_tip.deref().clone()))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let rocket = rocket::build();

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let (sender, receiver) = tokio::sync::mpsc::channel(10);

    let mut rpc_worker = RpcClientWorker::new(
        config.rpc_url(),
        config.rpc_timeout_ms(),
        receiver
    )?;

    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap()
    );

    rpc_worker.start(runtime.clone());

    let state = State {
        rpc: Arc::new(RpcClient::new(sender))
    };

    rocket
        .manage(state)
        .mount("/", routes![chain_tip])
        .launch()
        .await
        .unwrap();

    Ok(())
}
