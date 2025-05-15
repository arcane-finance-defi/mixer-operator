use crate::config::Config;
use dotenv::dotenv;
use log::{error, info};
use rocket::State as RocketState;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::{get, routes};
use rpc::MidenRpcAsyncFacade;
use rpc::ThreadPoolMidenRpcAsyncFacade;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate alloc;
extern crate core;

mod config;
mod rpc;
struct State {
    rpc: Arc<ThreadPoolMidenRpcAsyncFacade>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ErrorResponse {
    error: String,
}

#[get("/block-number")]
async fn block_number(state: &RocketState<State>) -> Result<Json<u32>, Status> {
    let rpc = state.rpc.clone();

    match rpc.get_block_header(None, false).await {
        Ok((header, _)) => {
            info!(
                "Successfully retrieved block header with number: {}",
                header.block_num().as_u32()
            );
            Ok(Json::from(header.block_num().as_u32()))
        }
        Err(e) => {
            error!("Failed to get block header: {}", e);
            Err(Status::InternalServerError)
        }
    }
}

#[rocket::launch]
fn rocket() -> _ {
    dotenv().ok();
    env_logger::init();

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

    let state = State { rpc: miden_facade };

    rocket.manage(state).mount("/", routes![block_number])
}
