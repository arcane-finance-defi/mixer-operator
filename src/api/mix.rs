use std::sync::Arc;

use anyhow::{Context as _, anyhow};
use chrono::{DateTime, TimeDelta, Utc};
use fang::{AsyncQueue, AsyncQueueable};
use miden_bridge::{
    notes::{BRIDGE_USECASE, crosschain::new_crosschain_note},
    utils::evm_address_to_felts,
};
use miden_client::{Felt, Word};
use miden_objects::{
    account::AccountId,
    note::{Note, NoteTag},
};
use rocket::{
    State as RocketState, get, post,
    serde::{Deserialize, Serialize, json::Json},
};
use rocket_okapi::{
    okapi::{schemars, schemars::JsonSchema},
    openapi,
};
use tokio::sync::oneshot;
use tracing::{info, instrument, trace};
use uuid::Uuid;

use super::error::EndpointError;
use crate::{
    MAX_NOTES_IN_BATCH_TRANSACTION,
    db::models::{
        NoteRepository,
        notes::{FullNote, NoteStatus},
    },
    mixer::{MixClientRequest, client::MixerClientError},
    state::MixerState,
    task::AsyncMixTask,
};

type MixResult = Result<String, MixerClientError>;

/// Add single note to mix storage with instant parameter
/// With `instant = true` perform mix immediately
#[openapi(tag = "MixRequest")] //, ignore = "state, note_repo")]
#[instrument(skip(data, state, note_repo))]
#[post("/mix", data = "<data>")]
pub async fn post_handler(
    data: Json<MixRequest>,
    state: &RocketState<MixerState>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
) -> Result<Json<MixResponse>, EndpointError> {
    let data = data.into_inner();
    let is_instant = data.instant;

    if is_instant {
        let responses = mix_instantly(vec![data], state).await?;
        if responses.len() != 1 {
            return Err(EndpointError::from(anyhow!(
                "expected exactly one response from mixer client"
            )));
        }
        // return tx id
        let tx_id = responses[0].to_string();
        Ok(Json(MixResponse::Instant { tx_id: vec![tx_id] }))
    } else {
        let note = Note::try_from(&data)?;
        let full_note = fill_note_record(note, data.account_id, Some(Utc::now()), None)?;
        add_to_note_repo(vec![full_note], note_repo).await?;
        Ok(Json(MixResponse::Empty))
    }
}

#[openapi(tag = "BatchMixRequest")]
#[instrument(skip(data, state, note_repo))]
#[post("/mix/batch", data = "<data>")]
pub async fn post_batch_handler(
    data: Json<BatchMixRequest>,
    state: &RocketState<MixerState>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
) -> Result<Json<MixResponse>, EndpointError> {
    let data = data.into_inner();
    let is_instant = data.instant;

    if is_instant {
        let mut mix_reqs: Vec<MixRequest> = Vec::with_capacity(data.metadata.len());
        for req in data.metadata {
            mix_reqs.push(MixRequest {
                dest_chain_id: req.dest_chain_id,
                dest_address: req.dest_address,
                serial_num_hex: req.serial_num_hex,
                bridge_serial_num_hex: req.bridge_serial_num_hex,
                amount: req.amount,
                account_id: req.account_id,
                instant: data.instant,
            });
        }
        let responses = mix_instantly(mix_reqs, state).await?;
        // return tx id
        let responses = MixResponse::Instant { tx_id: responses };
        Ok(Json(responses))
    } else {
        let notes: Vec<Note> = data
            .metadata
            .iter()
            .map(Note::try_from)
            .collect::<Result<Vec<_>, anyhow::Error>>()?;
        let full_notes = notes
            .into_iter()
            .enumerate()
            .map(|(idx, note)| {
                let fullnote = fill_note_record(
                    note,
                    data.metadata[idx].account_id.clone(),
                    Some(Utc::now()),
                    None,
                )?;
                Ok(fullnote)
            })
            .collect::<Result<Vec<_>, anyhow::Error>>()?;
        add_to_note_repo(full_notes, note_repo).await?;

        Ok(Json(MixResponse::Empty))
    }
}

#[openapi(tag = "MixDelayedRequest")]
#[post("/mix/delayed", data = "<data>")]
#[instrument(skip(data, note_repo, task_queue))]
pub async fn delayed_post_handler(
    data: Json<MixDelayedRequest>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
    task_queue: &RocketState<Arc<AsyncQueue>>,
) -> Result<Json<MixDelayedResponse>, EndpointError> {
    let data = data.into_inner();
    let responses = mix_delayed(vec![data], note_repo, task_queue).await?;
    if responses.len() != 1 {
        return Err(EndpointError::from(anyhow!(
            "expected exactly one response from mixer client"
        )));
    }
    let request_id = responses[0].to_string();
    Ok(Json(MixDelayedResponse { request_id }))
}

#[openapi(tag = "MixDelayedRequest")]
#[post("/mix/batch/delayed", data = "<data>")]
#[instrument(skip(data, note_repo, task_queue))]
pub async fn delayed_post_batch_handler(
    data: Json<Vec<MixDelayedRequest>>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
    task_queue: &RocketState<Arc<AsyncQueue>>,
) -> Result<Json<Vec<MixDelayedResponse>>, EndpointError> {
    let data = data.into_inner();
    let responses = mix_delayed(data, note_repo, task_queue).await?;
    let responses = responses
        .into_iter()
        .map(|request_id| MixDelayedResponse { request_id })
        .collect();
    Ok(Json(responses))
}

#[openapi]
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

async fn mix_delayed(
    reqs: Vec<MixDelayedRequest>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
    task_queue: &RocketState<Arc<AsyncQueue>>,
) -> Result<Vec<String>, EndpointError> {
    let mut responses = Vec::new();

    for req in reqs {
        let request_id = Uuid::new_v4();
        let scheduled_at = schedule_after(req.delayed_ms)?;

        let note = Note::try_from(&req)?;
        let note_id = &note.id();
        let full_note = fill_note_record(
            note,
            req.account_id,
            Some(scheduled_at),
            Some(&request_id.to_string()),
        )?;

        info!("Schedule delayed mixing for note {note_id:?} {request_id} at {scheduled_at}");

        note_repo.add_note(full_note).await?;
        trace!("Note {note_id} added to storage as {request_id}");

        let task = AsyncMixTask::new(&request_id.to_string(), scheduled_at);
        task_queue.schedule_task(&task as &dyn fang::AsyncRunnable).await?;
        trace!("Task for note {note_id} enqueued");

        responses.push(request_id.to_string());
    }

    Ok(responses)
}

async fn mix_instantly(
    reqs: Vec<MixRequest>,
    state: &RocketState<MixerState>,
) -> Result<Vec<String>, EndpointError> {
    let mut responses = Vec::new();

    if reqs.len() > MAX_NOTES_IN_BATCH_TRANSACTION {
        return Err(EndpointError::BatchLimit);
    }

    for req in reqs {
        let note = Note::try_from(&req)?;
        info!("Mixing note: {:?}", &note.id().to_string());

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

// TODO: add all notes in one transaction
async fn add_to_note_repo(
    notes: Vec<FullNote>,
    note_repo: &RocketState<Arc<dyn NoteRepository>>,
) -> Result<(), EndpointError> {
    for note in notes {
        let note_id = note.note_id.clone();
        tracing::info!("Store note {note_id}");

        note_repo
            .add_note(note)
            .await
            .map_err(|e| EndpointError::from(anyhow!(e.to_string())))?;
    }
    Ok(())
}

// TODO: maybe we should use `trusted` source of time instead or additionally
fn schedule_after(delay_ms: u64) -> anyhow::Result<DateTime<Utc>> {
    let now: DateTime<Utc> = Utc::now();
    let duration = TimeDelta::try_milliseconds(delay_ms as i64)
        .with_context(|| "invalid milliseconds duration")?;
    let scheduled_datetime = now + duration;
    Ok(scheduled_datetime)
}

// ! Deprecated request, should be deleted in next release
// #[deprecated] // clippy
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixRequest {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    amount: u64,
    account_id: String,
    instant: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixMetadata {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    account_id: String,
    amount: u64,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct BatchMixRequest {
    metadata: Vec<MixMetadata>,
    instant: bool,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub enum MixResponse {
    Empty,
    Instant { tx_id: Vec<String> },
    Delayed { note_id: Vec<String> },
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
        let value = NoteFrom {
            serial_num_hex: &value.serial_num_hex,
            bridge_serial_num_hex: &value.bridge_serial_num_hex,
            dest_chain_id: value.dest_chain_id,
            dest_address: &value.dest_address,
            faucet_id: &value.account_id,
            amount: value.amount,
        };
        note_try_from(&value)
    }
}

impl TryFrom<&MixDelayedRequest> for Note {
    type Error = anyhow::Error;
    fn try_from(value: &MixDelayedRequest) -> Result<Self, Self::Error> {
        let value = NoteFrom {
            serial_num_hex: &value.serial_num_hex,
            bridge_serial_num_hex: &value.bridge_serial_num_hex,
            dest_chain_id: value.dest_chain_id,
            dest_address: &value.dest_address,
            faucet_id: &value.account_id,
            amount: value.amount,
        };
        note_try_from(&value)
    }
}

impl TryFrom<&MixMetadata> for Note {
    type Error = anyhow::Error;
    fn try_from(value: &MixMetadata) -> Result<Self, Self::Error> {
        let value = NoteFrom {
            serial_num_hex: &value.serial_num_hex,
            bridge_serial_num_hex: &value.bridge_serial_num_hex,
            dest_chain_id: value.dest_chain_id,
            dest_address: &value.dest_address,
            faucet_id: &value.account_id,
            amount: value.amount,
        };
        note_try_from(&value)
    }
}

pub(super) struct NoteFrom<'a> {
    pub serial_num_hex: &'a str,
    pub bridge_serial_num_hex: &'a str,
    pub dest_chain_id: u64,
    pub dest_address: &'a str,
    pub faucet_id: &'a str,
    pub amount: u64,
}

pub(super) fn note_try_from(value: &NoteFrom) -> anyhow::Result<Note> {
    let faucet_id = AccountId::from_hex(value.faucet_id)?;

    // NB: https://github.com/0xMiden/crypto/pull/450 parse_hex_string_as_word -> Word::parse
    let note = new_crosschain_note(
        Word::parse(value.serial_num_hex)
            .map_err(|e| anyhow!("Failed to parse serial number hex {e:?}"))?,
        Word::parse(value.bridge_serial_num_hex)
            .map_err(|e| anyhow!("Failed to parse bridge serial number hex {e:?}"))?,
        Felt::new(value.dest_chain_id),
        evm_address_to_felts(value.dest_address)?,
        None,
        faucet_id,
        value.amount,
        faucet_id,
        NoteTag::for_local_use_case(BRIDGE_USECASE, 0)?,
    )?;

    Ok(note)
}

/// Fill `FullNote` model with NoteStatus ACCEPTED and datetime so "batch mix" worker can catch it
/// up Optionally `request_id` can be specified for "delayed mix" worker
pub(super) fn fill_note_record(
    note: Note,
    account_id: String,
    scheduled_date: Option<DateTime<Utc>>,
    request_id: Option<&str>,
) -> anyhow::Result<FullNote> {
    use miden_objects::utils::{Serializable as _, ToHex as _};

    use crate::db::models::notes as models;

    let serialized_note = note.to_bytes().to_hex();
    let serialized_note_id = note.id().to_string();

    Ok(models::FullNote {
        note_id: serialized_note_id,
        note: serialized_note,
        account_id,
        status: models::NoteStatus::ACCEPTED,
        scheduled_datetime: scheduled_date.map(|d| d.naive_utc()),
        request_id: request_id.map(|r| r.to_owned()),
    })
}

#[cfg(test)]
mod test {
    use rocket::serde::json;

    use super::{BatchMixRequest, MixDelayedRequest, MixMetadata, MixRequest, Note};

    #[test]
    fn note_try_from_mix_request() {
        let mix_request = MixRequest {
            dest_chain_id: 11155111,
            dest_address: "0xA09E268420a7C43Be8e6af64E348482585C1a688".to_string(),
            serial_num_hex: "0xc5f184597aae8760fc0506e721550c7a350b8a03e82bc1fd4badda244af696c5"
                .to_string(),
            bridge_serial_num_hex:
                "0xc41914731dc9db66076460db409e5e88cc36b6827ab1fed9ac7c07e811d51832".to_string(),
            account_id: "0x4de3bc8d67731a2067af0fcc7a2e34".to_string(),
            amount: 700000,
            instant: true,
        };

        let note = Note::try_from(&mix_request).expect("from MixRequest");

        assert_eq!(
            note.id().to_hex().as_str(),
            "0xaae7ac59b582903a49a2dd43037d405443107d48f2c82f5eee790b857840a641"
        );
    }

    #[test]
    fn note_try_from_mix_delayed_request() {
        let mix_request = MixDelayedRequest {
            dest_chain_id: 11155111,
            dest_address: "0xA09E268420a7C43Be8e6af64E348482585C1a688".to_string(),
            serial_num_hex: "0xc5f184597aae8760fc0506e721550c7a350b8a03e82bc1fd4badda244af696c5"
                .to_string(),
            bridge_serial_num_hex:
                "0xc41914731dc9db66076460db409e5e88cc36b6827ab1fed9ac7c07e811d51832".to_string(),
            account_id: "0x4de3bc8d67731a2067af0fcc7a2e34".to_string(),
            amount: 700000,
            delayed_ms: 0,
        };

        let note = Note::try_from(&mix_request).expect("from MixDelayedRequest");

        assert_eq!(
            note.id().to_hex().as_str(),
            "0xaae7ac59b582903a49a2dd43037d405443107d48f2c82f5eee790b857840a641"
        );
    }

    #[test]
    fn note_try_from_mix_metadata() {
        let mix_request = MixMetadata {
            dest_chain_id: 11155111,
            dest_address: "0xA09E268420a7C43Be8e6af64E348482585C1a688".to_string(),
            serial_num_hex: "0xc5f184597aae8760fc0506e721550c7a350b8a03e82bc1fd4badda244af696c5"
                .to_string(),
            bridge_serial_num_hex:
                "0xc41914731dc9db66076460db409e5e88cc36b6827ab1fed9ac7c07e811d51832".to_string(),
            account_id: "0x4de3bc8d67731a2067af0fcc7a2e34".to_string(),
            amount: 700000,
        };

        let note = Note::try_from(&mix_request).expect("from MixMetadata");

        assert_eq!(
            note.id().to_hex().as_str(),
            "0xaae7ac59b582903a49a2dd43037d405443107d48f2c82f5eee790b857840a641"
        );
    }

    #[test]
    fn test_mix_request_json_schema() {
        let req = MixRequest {
            dest_chain_id: 112211,
            dest_address: "0xsomehexdstaddr".to_string(),
            serial_num_hex: "0xsomehexserial".to_string(),
            bridge_serial_num_hex: "0xsomehexbridge".to_string(),
            amount: 50000,
            account_id: "0xsomehex".to_string(),
            instant: true,
        };
        let expected_request: &str = r#"{
            "dest_chain_id": 112211,
            "dest_address": "0xsomehexdstaddr",
            "serial_num_hex": "0xsomehexserial",
            "bridge_serial_num_hex": "0xsomehexbridge",
            "amount": 50000,
            "account_id": "0xsomehex",
            "instant": true
            }"#;
        let expected_request = expected_request.replace("\n", "");
        let expected_request = expected_request.replace(" ", "");

        let serialized_request = json::to_string(&req).expect("Serialized MixRequest");

        assert_eq!(serialized_request, expected_request);
    }

    #[test]
    fn test_mix_batch_request_json_schema() {
        let req = BatchMixRequest {
            metadata: vec![
                MixMetadata {
                    dest_chain_id: 112211,
                    dest_address: "0xsomehexdstaddr".to_string(),
                    serial_num_hex: "0xsomehexserial".to_string(),
                    bridge_serial_num_hex: "0xsomehexbridge".to_string(),
                    amount: 50000,
                    account_id: "0xsomehex".to_string(),
                },
                MixMetadata {
                    dest_chain_id: 112233,
                    dest_address: "0xsomehexdstaddr2".to_string(),
                    serial_num_hex: "0xsomehexserial2".to_string(),
                    bridge_serial_num_hex: "0xsomehexbridge2".to_string(),
                    amount: u64::MAX,
                    account_id: "0xsomehex2".to_string(),
                },
            ],
            instant: false,
        };
        let expected_request: &str = r#"{
            "metadata": [
                {
                    "dest_chain_id": 112211,
                    "dest_address": "0xsomehexdstaddr",
                    "serial_num_hex": "0xsomehexserial",
                    "bridge_serial_num_hex": "0xsomehexbridge",
                    "account_id": "0xsomehex",
                    "amount": 50000
                },
                {
                    "dest_chain_id": 112233,
                    "dest_address": "0xsomehexdstaddr2",
                    "serial_num_hex": "0xsomehexserial2",
                    "bridge_serial_num_hex": "0xsomehexbridge2",
                    "account_id": "0xsomehex2",
                    "amount": 18446744073709551615
                }
            ],
            "instant": false
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
