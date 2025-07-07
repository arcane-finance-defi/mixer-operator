use miden_objects::{account::AccountId, note::NoteFile, utils::Deserializable};
use tokio::sync::oneshot;
use tracing;
use rocket::{
    Responder, State as RocketState, post,
    serde::{Deserialize, Serialize, json::Json},
};

use self::error::EndpointError;
use crate::mixer::{MixClientRequest, client::MixerClientError};
use crate::state::MixerState;

mod error;
pub mod note_drafts;

type MixResult = Result<String, MixerClientError>;

#[post("/mix", data = "<data>")]
#[tracing::instrument]
pub async fn mix_post_handler(
    data: Json<MixRequest>,
    state: &RocketState<MixerState>,
) -> Result<Json<MixResponse>, ErrorResponse> {
    let note_bytes = hex::decode(&data.note_text).map_err(EndpointError::from)?;
    let note_file =
        NoteFile::read_from_bytes(note_bytes.as_slice()).map_err(EndpointError::from)?;

    let account_id = AccountId::from_hex(&data.account_id).map_err(EndpointError::from)?;

    let (request, response) = oneshot::channel::<MixResult>();

    // send request for mixing to miden
    state
        .client
        .send(MixClientRequest::Mix {
            note_file,
            account_id,
            response_sink: request,
        })
        .await
        .map_err(EndpointError::from)?;

    // await for result of mixing
    let response = response
        .await
        .map_err(EndpointError::from)? // TODO: doubled Result unwraping
        .map_err(EndpointError::from)?;
    tracing::warn!("{response:#?}");

    // return tx id
    Ok(Json(MixResponse { tx_id: response }))
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

#[cfg(test)]
mod test {
    use super::MixRequest;
    use rocket::serde::json;

    #[test]
    fn test_request_serder() {
        let req = MixRequest {
            note_text: "hexsomehex".to_string(),
            account_id: "0xsomehex".to_string(),
        };

        let serialized_request = json::to_string(&req).expect("Serialized MixRequest");

        assert_eq!(
            serialized_request,
            r#"{"note_text":"hexsomehex","account_id":"0xsomehex"}"#
        );
    }
}
