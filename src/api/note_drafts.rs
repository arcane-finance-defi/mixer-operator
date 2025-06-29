use function_name::named;
use rocket::http::uri::Error;
use rocket::http::Status;
use rocket::response::{status, Responder};
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket::{delete, get, post, State};
use tracing::info_span;

use crate::db::models::Storable as _;
use crate::db::{Pool, models::NoteStorage, models::notes::Note as DbNote};
use super::{error::EndpointError, MixRequest};

#[post("/note-drafts/new", data = "<note_data>")]
#[named]
pub async fn post_new_handler(
    note_data: Json<MixRequest>,
    pool: &State<Pool>,
) -> Result<Json<String>, ErrorResponse> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);
    
    let note: DbNote = note_data.into_inner().try_into()?;
    let note_id = note.note_id.clone();

    match storage.add_note(note) {
        Ok(1) => Ok(Json(note_id)),
        Ok(count) => Err(EndpointError::DatabaseLogicError(format!("Spurious db error, count={count}")).into()),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[get("/note-drafts")]
#[named]
pub async fn get_handler(pool: &State<Pool>) -> Result<Json<Vec<String>>, ErrorResponse> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.get_notes() {
        Ok(notes) => Ok(Json(notes.iter().map(|n| n.note_id.clone()).collect())),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[get("/note-drafts/<note_id>")]
#[named]
pub fn get_by_id_handler(note_id: &str, pool: &State<Pool>) -> Result<Option<Json<String>>, ErrorResponse> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.get_note_by_id(note_id) {
        Ok(Some(note)) => Ok(Some(Json(note.note_id))),
        Ok(None) => Ok(None),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[post("/note-drafts/activate/<note_id>")]
#[named]
pub async fn post_activate_by_id_handler(note_id: &str, pool: &State<Pool>) -> Result<Option<Json<String>>, ErrorResponse> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.get_note_by_id(note_id) {
        Ok(Some(note)) => Ok(Some(Json(note.note_id))), // TODO: return new generated note_id?
        Ok(None) => Ok(None),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[delete("/note-drafts/<note_id>")]
#[named]
pub fn delete_by_id_handler(pool: &State<Pool>, note_id: &str) -> Result<Status, ErrorResponse> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    match storage.delete_note_by_id(note_id) {
        Ok(0) => Ok(Status::NotFound),
        Ok(1) => Ok(Status::Accepted),
        Ok(count) => Err(EndpointError::DatabaseLogicError(format!("Spurious db error, count={count}")).into()),
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

// TODO: should be able to extract note without miden client
impl TryFrom<MixRequest> for crate::db::models::notes::Note {
    type Error = ErrorResponse; // ! FIXME: bad, should return client error convertible to ErrorResponse

    fn try_from(req: MixRequest) -> Result<Self, Self::Error> {
        // TODO: validate note with client and return account_id, note and note_id
        let note_id = "asd"; // ! FIXME
        let note_str = "sdf"; // ! FIXME

        Ok(crate::db::models::notes::Note {
            note_id: note_id.to_string(),
            note: note_str.to_string(),
            account_id: req.account_id,
        })
    }
}