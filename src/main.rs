use std::ops::Deref;
use std::sync::Arc;
use dotenv::dotenv;
use rocket::{get, routes};
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use crate::config::Config;
use rocket::State as RocketState;
#[macro_use]
extern crate rocket;
#[macro_use]
extern crate alloc;
extern crate core;

mod config;

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ErrorResponse {
    error: String,
}

#[get("/chain-tip")]
async fn chain_tip() -> Result<(), Status> {
    todo!()
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let rocket = rocket::build();

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    rocket
        .mount("/", routes![chain_tip])
        .launch()
        .await
        .unwrap();

    Ok(())
}
