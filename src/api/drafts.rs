use function_name::named;
use rocket::http::Status;
use rocket::response::Responder;
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket::{State, get, post};
use tracing::info_span;

use crate::db::models::Storable;
use crate::db::{Pool, models::NoteStorage};
use super::{error::EndpointError, MixRequest};

#[post("/drafts/new", data = "<note_data>")]
#[named]
pub async fn new_post_handler(
    note_data: Json<MixRequest>,
    pool: &State<Pool>,
) -> Result<Json<()>, ErrorResponse> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);
    
    let note = note_data.into_inner().try_into()?;
    match storage.add_note(note) {
        Ok(1) => Ok(Json(())),
        Ok(count) => Err(EndpointError::DatabaseLogicError(format!("Spurious db error, count={count}")).into()),
        Err(error) => Err(EndpointError::DatabaseError(error).into()),
    }
}

#[get("/drafts")]
#[named]
pub async fn get_handler(pool: &State<Pool>) -> Json<Vec<String>> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let conn = pool.get().map_err(EndpointError::from)?;
    let mut storage = NoteStorage::new(conn);

    let results = storage.get_notes().expect("Error loading notes");
    Json(results)
}

#[post("/drafts/activate/<id>")]
#[named]
pub async fn activate_post_handler(
    id: u64,
    // state: &State<NotesState>
) -> Result<&'static str, Status> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    if id < 10 {
        Ok("Not implemented activate")
    } else {
        Err(Status::NotFound)
    }
}

#[get("/notes/<note_id>")]
#[named]
pub fn get_note_by_id_handler(pool: &State<Pool>, note_id: String) -> Option<Json<Note>> {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let mut conn = pool.get().expect("Failed to get DB connection");
    let result = get_note_by_id(&mut conn, &note_id).expect("Error loading note");
    result.map(Json)
}

#[delete("/notes/<note_id>")]
#[named]
pub fn delete_note_by_id_handler(pool: &State<Pool>, note_id: String) -> Status {
    let span = info_span!(function_name!());
    let _enter = span.enter();

    let mut conn = pool.get().expect("Failed to get DB connection");
    let result = delete_note_by_id(&mut conn, &note_id).expect("Error deleting note");
    if result > 0 {
        Status::NoContent
    } else {
        Status::NotFound
    }
}

#[derive(Debug, Deserialize, Serialize, Responder)]
#[serde(crate = "rocket::serde")]
#[response(status = 500, content_type = "json")]
struct ErrorResponse {
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
    type Error = ErrorResponse; //! FIXME: bad, should return client error convertible to ErrorResponse

    fn try_from(req: MixRequest) -> Result<Self, Self::Error> {
        // TODO: validate note with client and return account_id, note and note_id
        let note_id = "asd"; //! FIXME
        let note_str = "sdf"; //! FIXME

        Ok(crate::db::models::notes::Note {
            note_id: note_id.to_string(),
            note: note_str.to_string(),
            account_id: req.account_id,
        })
    }
}