use std::sync::Arc;

use anyhow::anyhow;
use chrono::{Duration, Timelike, Utc};
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
use crate::db::models::NoteRepository;

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

    let try_after_seconds: i64 = note_data.try_after_seconds.unwrap_or(DEFAULT_TRY_AFTER_SECONDS)
        .try_into().unwrap();

    let full_note = fill_note_record(
        note.clone(),
        note_data.account_id,
        Some(Utc::now() + Duration::seconds(try_after_seconds)),
        None
    )?;

    tracing::info!("Store note id: {note_id} recipient: {note_recipient}");

    note_repo
        .add_note(full_note)
        .await
        .map_err(|e| EndpointError::from(anyhow!(e.to_string())))?;

    Ok(Json(
        MixDraftResponse {
            note_id: note_id.to_string(),
            recipient_hex: note_recipient.to_string(),
        }
    ))
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

// #[get("/note-drafts")]
// #[tracing::instrument]
// pub async fn get_handler(pool: &State<Pool>) -> Result<Json<Vec<String>>, ErrorResponse> {
//     let conn = pool.get().map_err(EndpointError::from)?;
//     let mut storage = NoteStorage::new(conn);

//     match storage.get_notes() {
//         Ok(notes) => Ok(Json(notes.iter().map(|n| n.note_id.clone()).collect())),
//         Err(error) => Err(EndpointError::DatabaseError(error).into()),
//     }
// }

// #[post("/note-drafts/activate/<note_id>")]
// #[tracing::instrument]
// pub async fn post_activate_by_id_handler(
//     note_id: &str,
//     pool: &State<Pool>,
// ) -> Result<Option<Json<String>>, ErrorResponse> {
//     let conn = pool.get().map_err(EndpointError::from)?;
//     let mut storage = NoteStorage::new(conn);

//     match storage.get_note_by_id(note_id) {
//         Ok(Some(note)) => Ok(Some(Json(note.note_id))), // TODO: return new generated note_id?
//         Ok(None) => Ok(None),
//         Err(error) => Err(EndpointError::DatabaseError(error).into()),
//     }
// }

// #[delete("/note-drafts/<note_id>")]
// #[tracing::instrument]
// pub async fn delete_by_id_handler(
//     pool: &State<Pool>,
//     note_id: &str,
// ) -> Result<Status, ErrorResponse> {
//     let conn = pool.get().map_err(EndpointError::from)?;
//     let mut storage = NoteStorage::new(conn);

//     match storage.delete_note_by_id(note_id) {
//         Ok(0) => Ok(Status::NotFound),
//         Ok(1) => Ok(Status::Accepted),
//         Ok(count) => Err(EndpointError::DatabaseLogicError(format!(
//             "Spurious db error, count={count}"
//         ))
//         .into()),
//         Err(error) => Err(EndpointError::DatabaseError(error).into()),
//     }
// }

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDraftRequest {
    dest_chain_id: u64,
    dest_address: String,
    serial_num_hex: String,
    bridge_serial_num_hex: String,
    amount: u64,
    account_id: String,
    try_after_seconds: Option<u32>
}

#[derive(Debug, Deserialize, Serialize, JsonSchema)]
#[serde(crate = "rocket::serde")]
pub struct MixDraftResponse {
    note_id: String,
    recipient_hex: String,
}

// TODO: should return normal error type
// impl TryFrom<MixDraftRequest> for crate::db::models::notes::FullNote {
//     type Error = anyhow::Error;

//     fn try_from(req: MixDraftRequest) -> Result<Self, Self::Error> {
//         // use miden_objects::block::BlockNumber;
//         use miden_objects::{
//             note::Note as OnchainNote,
//             utils::{Serializable as _, ToHex as _},
//         };
//         note_try_from(&value)

//         let note =
//             OnchainNote::try_from(&req).map_err(|err| ErrorResponse { error: err.to_string() })?;

//         let serialized_note = note.to_bytes().to_hex();
//         let serialized_note_id = note.id().to_string();

//         Ok(models::FullNote {
//             note_id: serialized_note_id,
//             note: serialized_note,
//             account_id: req.account_id,
//             status: models::NoteStatus::ACCEPTED,
//             scheduled_datetime: None,
//             request_id: None,
//         })
//     }
// }

impl TryFrom<&MixDraftRequest> for miden_objects::note::Note {
    type Error = anyhow::Error;
    fn try_from(value: &MixDraftRequest) -> Result<Self, Self::Error> {
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
