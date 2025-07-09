use std::ops::Not;

use rocket::http::Status;
use rocket::response::{Responder, status};
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket::{State, delete, get, post};

use super::{MixRequest, error::EndpointError};
use crate::db::{Pool, models::NoteStorage, models::Storable as _, models::notes::Note as DbNote};
use crate::mixer::utils;

#[post("/note-drafts/new", data = "<note_data>")]
#[tracing::instrument]
pub async fn post_new_handler(
    note_data: Json<MixRequest>,
    pool: &State<Pool>,
) -> Result<Json<String>, ErrorResponse> {
    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    let note: DbNote = note_data.into_inner().try_into()?;
    let note_id = note.note_id.clone();

    match storage.add_note(note) {
        Ok(1) => Ok(Json(note_id)),
        Ok(count) => Err(EndpointError::DatabaseLogicError(format!(
            "Spurious db error, count={count}"
        ))
        .into()),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[get("/note-drafts")]
#[tracing::instrument]
pub async fn get_handler(pool: &State<Pool>) -> Result<Json<Vec<String>>, ErrorResponse> {
    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.get_notes() {
        Ok(notes) => Ok(Json(notes.iter().map(|n| n.note_id.clone()).collect())),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[get("/note-drafts/<note_id>")]
#[tracing::instrument]
pub async fn get_by_id_handler(
    note_id: &str,
    pool: &State<Pool>,
) -> Result<Option<Json<String>>, ErrorResponse> {
    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.get_note_by_id(note_id) {
        Ok(Some(note)) => Ok(Some(Json(note.note_id))),
        Ok(None) => Ok(None),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[post("/note-drafts/activate/<note_id>")]
#[tracing::instrument]
pub async fn post_activate_by_id_handler(
    note_id: &str,
    pool: &State<Pool>,
) -> Result<Option<Json<String>>, ErrorResponse> {
    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.get_note_by_id(note_id) {
        Ok(Some(note)) => Ok(Some(Json(note.note_id))), // TODO: return new generated note_id?
        Ok(None) => Ok(None),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[delete("/note-drafts/<note_id>")]
#[tracing::instrument]
pub async fn delete_by_id_handler(
    pool: &State<Pool>,
    note_id: &str,
) -> Result<Status, ErrorResponse> {
    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.delete_note_by_id(note_id) {
        Ok(0) => Ok(Status::NotFound),
        Ok(1) => Ok(Status::Accepted),
        Ok(count) => Err(EndpointError::DatabaseLogicError(format!(
            "Spurious db error, count={count}"
        ))
        .into()),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
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

// TODO: should return normal error type
// TODO: use TypeState + Composition pattern to cover Note conversions and state transitions
// E.g. ScheduledNote -> PendingNote -> ActivatedNote -> FinishedNote
// couple with status field in Note table
impl TryFrom<MixRequest> for crate::db::models::notes::Note {
    type Error = ErrorResponse; // ! FIXME: bad, should return client error convertible to ErrorResponse

    fn try_from(req: MixRequest) -> Result<Self, Self::Error> {
        let note_file = utils::from_hex_string(&req.note_text).map_err(|e| ErrorResponse {
            error: format!("error reading note content: {e}"),
        })?;

        if utils::is_note_with_proof(&note_file).not() {
            return Err(ErrorResponse {
                error: "note is without proof".to_string(),
            });
        }

        let note_id: miden_objects::note::NoteId = utils::extract_note_id(&note_file);

        Ok(crate::db::models::notes::Note {
            note_id: note_id.to_string(),
            note: utils::to_hex_string(note_file),
            account_id: req.account_id,
            status: 0,
            scheduled_datetime: None,
        })
    }
}
