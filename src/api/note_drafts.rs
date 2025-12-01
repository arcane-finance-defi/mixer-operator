use std::sync::Arc;

use anyhow::anyhow;
use chrono::{Duration, Utc};
use miden_client::note::Note;
use rocket::{
    State, get, post,
    serde::{Deserialize, Serialize, json::Json},
};
use rocket_okapi::{
    okapi::{schemars, schemars::JsonSchema},
    openapi,
};

use super::{
    error::EndpointError,
    mix::{NoteFrom, fill_note_record, note_try_from},
};
use crate::db::models::{NoteRepository, notes::FullNote};

const DEFAULT_TRY_AFTER_SECONDS: u32 = 15;

/// Add single note to mix storage to execute with delay specified
#[openapi(tag = "MixDraftRequest")]
#[post("/note-drafts/new", data = "<note_data>")]
#[tracing::instrument(skip(note_repo))]
pub async fn post_new_handler(
    note_data: Json<MixDraftRequest>,
    note_repo: &State<Arc<dyn NoteRepository>>,
) -> Result<Json<MixDraftResponse>, EndpointError> {
    let note_data = note_data.into_inner();
    let note = Note::try_from(&note_data)?;
    let note_id = &note.id().to_hex();
    let note_recipient = note.recipient().digest().to_hex();

    let try_after_seconds: i64 =
        note_data.try_after_seconds.unwrap_or(DEFAULT_TRY_AFTER_SECONDS).into();

    let full_note = fill_note_record(
        note.clone(),
        note_data.account_id,
        Some(Utc::now() + Duration::seconds(try_after_seconds)),
        None,
    )?;

    tracing::info!("Store note id: {note_id} recipient: {note_recipient}");

    note_repo
        .add_note(full_note)
        .await
        .map_err(|e| EndpointError::from(anyhow!(e.to_string())))?;

    Ok(Json(MixDraftResponse {
        note_id: note_id.to_string(),
        recipient_hex: note_recipient.to_string(),
    }))
}

#[openapi(tag = "MixDraftRequest")]
#[post("/note-drafts/new/batch", data = "<notes_data>")]
#[tracing::instrument(skip(note_repo))]
pub async fn post_new_batch_handler(
    notes_data: Json<MixDraftBatchRequest>,
    note_repo: &State<Arc<dyn NoteRepository>>,
) -> Result<Json<MixDraftBatchResponse>, EndpointError> {
    let notes_data = notes_data.into_inner();
    let notes = notes_data
        .drafts
        .iter()
        .map(|draft| Ok((draft, Note::try_from(draft)?)))
        .collect::<anyhow::Result<Vec<(&MixDraftRequest, Note)>>>()?;

    let db_records = notes
        .iter()
        .map(|(draft, note)| -> anyhow::Result<FullNote> {
            let try_after_seconds: i64 =
                draft.try_after_seconds.unwrap_or(DEFAULT_TRY_AFTER_SECONDS).into();

            let full_note = fill_note_record(
                note.clone(),
                draft.account_id.clone(),
                Some(Utc::now() + Duration::seconds(try_after_seconds)),
                None,
            )?;

            Ok(full_note)
        })
        .collect::<anyhow::Result<Vec<FullNote>>>()?;

    note_repo
        .add_notes(db_records)
        .await
        .map_err(|e| EndpointError::from(anyhow!(e.to_string())))?;

    let generated_info = notes
        .into_iter()
        .map(|(_, note)| {
            let note_id = &note.id().to_hex();
            let note_recipient = note.recipient().digest().to_hex();
            tracing::info!("Store note id: {note_id} recipient: {note_recipient}");

            MixDraftResponse {
                note_id: note_id.to_string(),
                recipient_hex: note_recipient.to_string(),
            }
        })
        .collect();

    Ok(Json(MixDraftBatchResponse { generated: generated_info }))
}

/// Retrieve note status bitflags (integer with some bits set) by `note_id`
#[openapi(tag = "MixDraftRequest")]
#[get("/note-drafts/status/<note_id>")]
#[tracing::instrument(skip(note_repo))]
pub async fn get_status_handler(
    note_id: &str,
    note_repo: &State<Arc<dyn NoteRepository>>,
) -> Result<Option<Json<u8>>, EndpointError> {
    let note_status = note_repo.get_note_status_by_id(note_id).await;

    match note_status {
        Ok(status) => Ok(Some(Json(status.bits()))),
        Err(error) => Err(EndpointError::from(error)),
    }
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDraftRequest {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    amount: u64,
    account_id: String,
    sender_id: Option<String>,
    try_after_seconds: Option<u32>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDraftResponse {
    note_id: String,
    recipient_hex: String,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDraftBatchRequest {
    drafts: Vec<MixDraftRequest>,
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDraftBatchResponse {
    generated: Vec<MixDraftResponse>,
}

impl TryFrom<&MixDraftRequest> for miden_objects::note::Note {
    type Error = anyhow::Error;
    fn try_from(value: &MixDraftRequest) -> Result<Self, Self::Error> {
        let account_id = value.account_id.clone();
        let sender_id = value.sender_id.clone();

        let value = NoteFrom {
            serial_num_hex: &value.serial_num_hex,
            bridge_serial_num_hex: &value.bridge_serial_num_hex,
            dest_chain_id: value.dest_chain_id,
            dest_address: &value.dest_address,
            faucet_id: &account_id,
            sender_id: &sender_id.unwrap_or(account_id.clone()),
            amount: value.amount,
        };
        note_try_from(&value)
    }
}
