use dotenv::dotenv;
use miden_objects::account::AccountId;
use miden_objects::note::NoteFile;
use miden_objects::utils::Deserializable;
use rocket::{get, routes};
use rocket::http::Status;
use rocket::serde::json::Json;
use serde::{Deserialize, Serialize};
use crate::config::Config;
use rocket::State as RocketState;
use crate::mixer::client::MixerClientError;
use tokio::sync::{oneshot, mpsc};
use crate::mixer::{event_loop, MixClientRequest};
use hex::decode;

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate alloc;
extern crate core;

mod config;
mod rpc;
mod mixer;

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct ErrorResponse {
    error: String,
}

#[get("/chain-tip")]
async fn chain_tip() -> Result<(), Status> {
    todo!()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct MixRequest {
    note_text: String,
    account_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct MixResponse {
    tx_id: String,
}

#[post("/mix", data = "<data>")]
async fn mix(data: Json<MixRequest>, state: &RocketState<MixerState>) -> Result<Json<MixResponse>, Status> {
    let note_bytes = decode(&data.note_text)
        .map_err(|_| Status::InternalServerError)?;

    let note_file = NoteFile::read_from_bytes(note_bytes.as_slice())
        .map_err(|_| Status::InternalServerError)?;
    let account_id = AccountId::from_hex(&data.account_id).unwrap();

    let (request, response) = oneshot::channel::<Result<String, MixerClientError>>();

    state.client.send(MixClientRequest::Mix {
        note_file,
        account_id,
        response_sink: request
    }).await
        .map_err(|_| Status::InternalServerError)?;

    let response = response.await
        .map_err(|_| Status::InternalServerError)?
        .map_err(|_| Status::InternalServerError)?;

    Ok(Json(MixResponse {
        tx_id: response
    }))
}

pub struct MixerState {
    client: mpsc::Sender<MixClientRequest>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let rocket = rocket::build();

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let (sender, receiver) = mpsc::channel::<MixClientRequest>(100);

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        event_loop(config, receiver, runtime);
    });

    rocket
        .manage(MixerState { client: sender })
        .mount("/", routes![chain_tip, mix])
        .launch()
        .await
        .unwrap();

    Ok(())
}
