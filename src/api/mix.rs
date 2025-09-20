use std::sync::Arc;

use anyhow::{anyhow, Context as _};
use chrono::{DateTime, TimeDelta, Utc};
use fang::{AsyncQueue, AsyncQueueable};
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
    State as RocketState, get, post,
    serde::{Deserialize, Serialize, json::Json},
};
use schemars::JsonSchema;
use tokio::sync::oneshot;
use tracing::{info, instrument, trace};
use uuid::Uuid;

use super::error::EndpointError;
use crate::{
    db::models::{
        NoteRepository,
        notes::{FullNote, NoteStatus},
    },
    mixer::{MixClientRequest, client::MixerClientError},
    state::MixerState,
    task::mix::AsyncMixTask,
};

type MixResult = Result<String, MixerClientError>;

const MAX_NOTES_IN_BATCH: usize = 1024;

#[instrument(skip(data, state))]
#[post("/mix", data = "<data>")]
pub async fn post_handler(
    data: Json<MixRequest>,
    state: &RocketState<MixerState>,
) -> Result<Json<MixResponse>, EndpointError> {
    let data = data.into_inner();

    let responses = mix_instantly(vec![data], state).await?;

    if responses.len() != 1 {
        return Err(EndpointError::from(anyhow!("expected exactly one response from mixer client")));
    }

    // return tx id
    let tx_id = (&responses[0]).to_string();
    Ok(Json(MixResponse { tx_id }))
}

#[instrument(skip(data, state))]
#[post("/mix/batch", data = "<data>")]
pub async fn post_batch_handler(
    data: Json<Vec<MixRequest>>,
    state: &RocketState<MixerState>,
) -> Result<Json<Vec<MixResponse>>, EndpointError> {
    let data = data.into_inner();

    let responses = mix_instantly(data, state).await?;

    // return tx id
    let responses = responses.into_iter().map(|tx_id| MixResponse { tx_id }).collect();
    Ok(Json(responses))
}

async fn mix_instantly(reqs: Vec<MixRequest>, state: &RocketState<MixerState>) -> Result<Vec<String>, EndpointError> {
    let mut responses = Vec::new();

    if reqs.len() > MAX_NOTES_IN_BATCH {
        return Err(EndpointError::BatchLimit);
    }
    
    for req in reqs {
        let note = Note::try_from(&req)?;
        info!("Mixing note: {:?}", &note.id());

        let account_id = AccountId::from_hex(&req.account_id).map_err(EndpointError::from)?;

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

        responses.push(response);
    }
    
    Ok(responses)
}

#[post("/mix/delayed", data = "<data>")]
#[instrument(skip(data, note_repo, task_queue))]
pub async fn delayed_post_handler(
    data: Json<MixDelayedRequest>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
    task_queue: &RocketState<Arc<AsyncQueue>>,
) -> Result<Json<MixDelayedResponse>, EndpointError> {
    let request_id = Uuid::new_v4();
    let scheduled_at = schedule_after(data.delayed_ms)?;

    let data = data.into_inner();
    let note = Note::try_from(&data)?;
    let note_id = &note.id();
    let full_note = fill_note_record(note, data.account_id, scheduled_at, &request_id.to_string())?;

    info!("Schedule delayed mixing for note {note_id:?} {request_id} at {scheduled_at}");

    note_repo.add_note(full_note).await?;
    trace!("Note {note_id} added to storage as {request_id}");

    let task = AsyncMixTask::new(&request_id.to_string(), scheduled_at);
    task_queue.insert_task(&task).await?;
    trace!("Task for note {note_id} enqueued");

    Ok(Json(MixDelayedResponse { request_id: request_id.to_string() }))
}

#[get("/mix/delayed/status/<id>")]
#[instrument(skip(note_repo))]
pub async fn delayed_status_get_handler(
    id: &str,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
) -> Result<String, EndpointError> {
    let note = note_repo.get_note_by_request_id(id).await?;
    if note.status.contains(NoteStatus::TXED) {
        Ok(String::from("TXED"))
    } else {
        Ok(String::from("PENDING"))
    }
}

// TODO: maybe we should use `trusted` source of time instead or additionally
fn schedule_after(delay_ms: u64) -> anyhow::Result<DateTime<Utc>> {
    let now: DateTime<Utc> = Utc::now();
    let duration = TimeDelta::try_milliseconds(delay_ms as i64)
        .with_context(|| "invalid milliseconds duration")?;
    let scheduled_datetime = now + duration;
    Ok(scheduled_datetime)
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

impl TryFrom<&MixRequest> for Note {
    type Error = anyhow::Error;
    fn try_from(value: &MixRequest) -> Result<Self, Self::Error> {
        let faucet_id = AccountId::from_hex(&value.account_id)?;
        let note = new_crosschain_note(
            parse_hex_string_as_word(value.serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse serial number hex"))?
                .into(),
            parse_hex_string_as_word(value.bridge_serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse bridge serial number hex"))?
                .into(),
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
                .map_err(|_| Self::Error::msg("Failed to parse serial number hex"))?
                .into(),
            parse_hex_string_as_word(value.bridge_serial_num_hex.as_str())
                .map_err(|_| Self::Error::msg("Failed to parse bridge serial number hex"))?
                .into(),
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

fn fill_note_record(
    note: Note,
    account_id: String,
    scheduled_date: DateTime<Utc>,
    request_id: &str,
) -> anyhow::Result<FullNote> {
    use miden_objects::utils::{Serializable as _, ToHex as _};

    use crate::db::models::notes as models;

    let serialized_note = note.to_bytes().to_hex();
    let serialized_note_id = note.id().to_string();

    Ok(models::FullNote {
        note_id: serialized_note_id,
        note: serialized_note,
        account_id,
        // ! for now just leave status blank to prevent from execution by executor
        status: models::NoteStatus::UNDEFINED,
        scheduled_datetime: Some(scheduled_date.naive_utc()),
        request_id: Some(request_id.to_owned()),
    })
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
