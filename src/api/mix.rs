use std::sync::Arc;

use miden_bridge::{
    notes::{BRIDGE_USECASE, crosschain::new_crosschain_note},
    utils::evm_address_to_felts,
};
use miden_client::Felt;
use miden_objects::{
    account::AccountId,
    note::{Note, NoteTag},
    utils::parse_hex_string_as_word,
};
use rocket::{
    State as RocketState,
    http::Status,
    post, response,
    serde::{Deserialize, Serialize, json::Json},
};
use rocket_okapi::okapi::{schemars, schemars::JsonSchema};
use tokio::sync::oneshot;
use tracing::{info, instrument};
use uuid::Uuid;

use super::error::EndpointError;
use crate::{
    db::models::NoteRepository,
    mixer::{MixClientRequest, client::MixerClientError},
    state::MixerState,
};

type MixResult = Result<String, MixerClientError>;

#[instrument(skip(data, state))]
#[post("/mix", data = "<data>")]
pub async fn post_handler(
    data: Json<MixRequest>,
    state: &RocketState<MixerState>,
) -> Result<Json<MixResponse>, EndpointError> {
    let data = data.into_inner();

    let note = Note::try_from(&data)?; //.map_err(|err| ErrorResponse {
    // error: err.to_string(),
    // })?;

    info!("Mixing note: {:?}", &note.id());

    let account_id = AccountId::from_hex(&data.account_id).map_err(EndpointError::from)?;

    let (request, response) = oneshot::channel::<MixResult>();

    // send request for mixing to miden
    state
        .client
        .send(MixClientRequest::Mix { note, account_id, response_sink: request })
        .await
        .map_err(|e| EndpointError::from(Box::new(e)))?;

    // await for result of mixing
    let response = response
        .await
        .map_err(EndpointError::from)?
        .map_err(|e| EndpointError::from(Box::new(e)))?;

    // return tx id
    Ok(Json(MixResponse { tx_id: response }))
}

#[post("/mix/delayed", data = "<data>")]
#[instrument(skip(data, state, note_repo))]
pub async fn delayed_post_handler(
    data: Json<MixDelayedRequest>,
    state: &RocketState<MixerState>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
) -> Result<Json<MixDelayedResponse>, EndpointError> {
    let data = data.into_inner();
    let note = Note::try_from(&data)?;
    let request_id = Uuid::new_v4();
    info!("Schedule delayed mixing for note {:?} {request_id}", &note.id());

    // note_repo
    //     .add_note(note)
    //     .await
    //     .map_err(|e| EndpointError::from(anyhow!(e.to_string())))?;

    Ok(Json(MixDelayedResponse { request_id: request_id.to_string() }))
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixRequest {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    amount: u64,
    account_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixResponse {
    tx_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDelayedRequest {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    amount: u64,
    account_id: String,
    delayed_ms: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDelayedResponse {
    request_id: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
// #[response(status = 500, content_type = "json")]
pub struct ErrorResponse {
    error: String,
}

// TODO: to be replaced with EndpointError and moved out to error.rs module
// https://stuarth.github.io/rocket-error-handling/
impl<'r, 'o: 'r> response::Responder<'r, 'o> for ErrorResponse {
    fn respond_to(self, req: &'r rocket::Request<'_>) -> response::Result<'o> {
        // log `self` to your favored error tracker, e.g.
        // sentry::capture_error(&self);

        match self {
            // in our simplistic example, we're happy to respond with the default 500 responder in
            // all cases
            _ => Status::InternalServerError.respond_to(req),
        }
    }
}

impl From<EndpointError> for ErrorResponse {
    fn from(value: EndpointError) -> Self {
        Self { error: value.to_string() }
    }
}

impl TryFrom<&MixRequest> for Note {
    type Error = anyhow::Error;
    fn try_from(value: &MixRequest) -> Result<Self, Self::Error> {
        let faucet_id = AccountId::from_hex(&value.account_id)?;
        let note = new_crosschain_note(
            parse_hex_string_as_word(value.serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse serial number hex"))?,
            parse_hex_string_as_word(value.bridge_serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse bridge serial number hex"))?,
            Felt::new(value.dest_chain_id),
            evm_address_to_felts(&value.dest_address)?,
            None,
            faucet_id,
            value.amount,
            faucet_id,
            NoteTag::for_local_use_case(BRIDGE_USECASE, 0)?,
        )?;

        Ok(note)
    }
}

impl TryFrom<&MixDelayedRequest> for Note {
    type Error = anyhow::Error;

    fn try_from(value: &MixDelayedRequest) -> Result<Self, Self::Error> {
        let faucet_id = AccountId::from_hex(&value.account_id)?;
        let note = new_crosschain_note(
            parse_hex_string_as_word(value.serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse serial number hex"))?,
            parse_hex_string_as_word(value.bridge_serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse bridge serial number hex"))?,
            Felt::new(value.dest_chain_id),
            evm_address_to_felts(&value.dest_address)?,
            None,
            faucet_id,
            value.amount,
            faucet_id,
            NoteTag::for_local_use_case(BRIDGE_USECASE, 0)?,
        )?;

        Ok(note)
    }
}

#[cfg(test)]
mod test {
    use rocket::serde::json;

    use super::MixRequest;
    use crate::api::mix::MixDelayedRequest;

    #[test]
    fn test_mix_request_json_schema() {
        let req = MixRequest {
            dest_chain_id: 112211,
            dest_address: "0xsomehexdstaddr".to_string(),
            serial_num_hex: "0xsomehexserial".to_string(),
            bridge_serial_num_hex: "0xsomehexbridge".to_string(),
            amount: 50000,
            account_id: "0xsomehex".to_string(),
        };
        let expected_request: &str = r#"{
            "dest_chain_id": 112211,
            "dest_address": "0xsomehexdstaddr",
            "serial_num_hex": "0xsomehexserial",
            "bridge_serial_num_hex": "0xsomehexbridge",
            "amount": 50000,
            "account_id": "0xsomehex"
            }"#;
        let expected_request = expected_request.replace("\n", "");
        let expected_request = expected_request.replace(" ", "");

        let serialized_request = json::to_string(&req).expect("Serialized MixRequest");

        assert_eq!(serialized_request, expected_request);
    }

    #[test]
    fn test_mix_delayed_json_schema() {
        let req = MixDelayedRequest {
            dest_chain_id: 112211,
            dest_address: "0xsomehexdstaddr".to_string(),
            serial_num_hex: "0xsomehexserial".to_string(),
            bridge_serial_num_hex: "0xsomehexbridge".to_string(),
            amount: 50000,
            account_id: "0xsomehex".to_string(),
            delayed_ms: u64::MAX,
        };
        let expected_request: &str = r#"{
            "dest_chain_id": 112211,
            "dest_address": "0xsomehexdstaddr",
            "serial_num_hex": "0xsomehexserial",
            "bridge_serial_num_hex": "0xsomehexbridge",
            "amount": 50000,
            "account_id": "0xsomehex",
            "delayed_ms": 18446744073709551615
            }"#;
        let expected_request = expected_request.replace("\n", "");
        let expected_request = expected_request.replace(" ", "");

        let serialized_request = json::to_string(&req).expect("Serialized MixRequest");

        assert_eq!(serialized_request, expected_request);
    }
}
