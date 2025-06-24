use tokio::sync::{mpsc, oneshot};
use hex::{FromHexError};
use thiserror::Error;
use tracing::info_span;

use miden_objects::{
    account::AccountId,
    note::NoteFile,
    utils::{Deserializable, DeserializationError},
    AccountIdError
};

use rocket::{
    post, routes,
    http::{Method, Status},
    serde::{json::Json, Deserialize, Serialize}, 
    Responder, State as RocketState
};

use crate::mixer::{client::MixerClientError, MixClientRequest};

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

type MixResult = Result<String, MixerClientError>;

#[post("/mix", data = "<data>")]
pub async fn mix_post_handler(
    data: Json<MixRequest>,
    state: &RocketState<MixerState>,
) -> Result<Json<MixResponse>, ErrorResponse> {
    let span = info_span!("mix_post_handler");
    let _enter = span.enter();

    let note_bytes = hex::decode(&data.note_text).map_err(EndpointError::from)?;
    let note_file =
        NoteFile::read_from_bytes(note_bytes.as_slice()).map_err(EndpointError::from)?;

    let account_id = AccountId::from_hex(&data.account_id).map_err(EndpointError::from)?;

    let (request, response) = 
        oneshot::channel::<MixResult>();

    state
        .client
        .send(MixClientRequest::Mix {
            note_file,
            account_id,
            response_sink: request,
        })
        .await
        .map_err(EndpointError::from)?;

    let response = response
        .await
        .map_err(EndpointError::from)?
        .map_err(EndpointError::from)?;

    Ok(Json(MixResponse { tx_id: response }))
}

pub struct MixerState {
    client: mpsc::Sender<MixClientRequest>,
}

impl MixerState {
    pub fn new(client: mpsc::Sender<MixClientRequest>) -> Self {
        MixerState {
            client
        }
    }
}


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
