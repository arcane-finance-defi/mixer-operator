use miden_bridge::notes::BRIDGE_USECASE;
use miden_bridge::notes::crosschain::new_crosschain_note;
use miden_bridge::utils::evm_address_to_felts;
use miden_client::Felt;
use miden_objects::account::AccountId;
use miden_objects::note::{Note, NoteTag};
use miden_objects::utils::parse_hex_string_as_word;
use tokio::sync::oneshot;
use tracing;
use rocket::{
    Responder, State as RocketState, post,
    serde::{Deserialize, Serialize, json::Json},
};
use tracing::info;
use self::error::EndpointError;
use crate::mixer::{MixClientRequest, client::MixerClientError};
use crate::state::MixerState;

mod error;
pub mod note_drafts;

type MixResult = Result<String, MixerClientError>;

#[post("/mix", data = "<data>")]
#[tracing::instrument(skip(data, state))]
pub async fn mix_post_handler(
    data: Json<MixRequest>,
    state: &RocketState<MixerState>,
) -> Result<Json<MixResponse>, ErrorResponse> {
    let note = Note::try_from(&data.0).map_err(|err| ErrorResponse {
        error: err.to_string(),
    })?;

    info!("Mixing note: {:?}", &note.id());

    let account_id = AccountId::from_hex(&data.account_id).map_err(EndpointError::from)?;

    let (request, response) = oneshot::channel::<MixResult>();

    // send request for mixing to miden
    state
        .client
        .send(MixClientRequest::Mix {
            note,
            account_id,
            response_sink: request,
        })
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

#[derive(Debug, Deserialize, Serialize)]
#[serde(crate = "rocket::serde")]
pub struct MixRequest {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    amount: u64,
    account_id: String,
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
            faucet_id,
            value.amount,
            faucet_id,
            NoteTag::for_local_use_case(BRIDGE_USECASE, 0)?
        )?;

        Ok(note)
    }
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

        assert_eq!(
            serialized_request,
            expected_request
        );
    }
}
