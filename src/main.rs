use std::any::Any;
use std::error::Error;
use dotenv::dotenv;
use miden_objects::account::AccountId;
use miden_objects::note::NoteFile;
use miden_objects::utils::{Deserializable, DeserializationError};
use rocket::{get, routes};
use rocket::http::{Method, Status};
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use crate::config::Config;
use rocket::State as RocketState;
use crate::mixer::client::MixerClientError;
use tokio::sync::{oneshot, mpsc};
use crate::mixer::{event_loop, MixClientRequest};
use hex::{decode, FromHexError};
use miden_objects::AccountIdError;
use rocket_cors::{AllowedOrigins, CorsOptions};
use thiserror::Error;

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate alloc;
extern crate core;

mod config;
mod mixer;

#[derive(Error, Debug)]
pub enum EndpointError {
    #[error(transparent)]
    HexError(#[from] FromHexError),
    #[error(transparent)]
    DeserializationError(#[from] DeserializationError),
    #[error(transparent)]
    AccountIdError(#[from] AccountIdError),
    #[error(transparent)]
    SendError(#[from] mpsc::error::SendError<MixClientRequest>),
    #[error(transparent)]
    RecvError(#[from] oneshot::error::RecvError),
    #[error(transparent)]
    MixerClientError(#[from] MixerClientError),
}

#[derive(Debug, Deserialize, Serialize, Responder)]
#[serde(crate = "rocket::serde")]
#[response(status = 500, content_type = "json")]
pub struct ErrorResponse {
    error: String,
}

impl From<EndpointError> for ErrorResponse {
    fn from(value: EndpointError) -> Self {
        Self {
            error: value.to_string(),
        }
    }
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
async fn mix(data: Json<MixRequest>, state: &RocketState<MixerState>) -> Result<Json<MixResponse>, ErrorResponse> {
    let note_bytes = decode(&data.note_text).map_err(EndpointError::from)?;

    let note_file = NoteFile::read_from_bytes(note_bytes.as_slice())
        .map_err(EndpointError::from)?;
    let account_id = AccountId::from_hex(&data.account_id)
        .map_err(EndpointError::from)?;

    let (request, response) = oneshot::channel::<Result<String, MixerClientError>>();

    state.client.send(MixClientRequest::Mix {
        note_file,
        account_id,
        response_sink: request
    }).await.map_err(EndpointError::from)?;

    let response = response.await
        .map_err(EndpointError::from)?
        .map_err(EndpointError::from)?;

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

    let cors = CorsOptions::default()
        .allowed_origins(AllowedOrigins::all())
        .allowed_methods(
            vec![Method::Get, Method::Post, Method::Patch]
                .into_iter()
                .map(From::from)
                .collect(),
        )
        .allow_credentials(true);

    let rocket = rocket::build().attach(cors.to_cors().unwrap());

    let figment = rocket.figment();
    let config: Config = figment.extract().expect("config");

    let (sender, receiver) = mpsc::channel::<MixClientRequest>(100);

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build().unwrap();
        event_loop(config, receiver, runtime);
    });

    rocket
        .manage(MixerState { client: sender })
        .mount("/", routes![mix])
        .launch()
        .await
        .unwrap();

    Ok(())
}
