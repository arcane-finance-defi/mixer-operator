use std::ops::Not;
use std::sync::Arc;

use anyhow::anyhow;
use rocket::response::{Responder, status};
use rocket::serde::json::Json;
use rocket::serde::{Deserialize, Serialize};
use rocket::{State, delete, get, post};

use super::{MixRequest, error::EndpointError};
use crate::db::models::{NoteRepository, NoteRepositoryError, notes};
use crate::mixer::utils;

/// Add note to mix storage
#[post("/note-drafts/new", data = "<note_data>")]
#[tracing::instrument(skip(note_repo))]
pub async fn post_new_handler(
    note_data: Json<MixRequest>,
    note_repo: &State<Arc<dyn NoteRepository>>,
) -> Result<Json<String>, ErrorResponse> {
    let note: notes::FullNote = note_data.into_inner().try_into()?;

    let note_id = note.note_id.clone();
    tracing::info!("Store note {note_id}");

    note_repo
        .add_note(note)
        .await
        .map_err(|e| EndpointError::from(anyhow!(e.to_string())))?;

    Ok(Json(note_id))
}

/// Retrieve note status bitflags
#[get("/note-drafts/status/<note_id>")]
#[tracing::instrument(skip(note_repo))]
pub async fn get_status_handler(
    note_id: &str,
    note_repo: &State<Arc<dyn NoteRepository>>,
) -> Result<Option<Json<u8>>, ErrorResponse> {
    let note_status = note_repo.get_note_status_by_id(note_id).await;

    match note_status {
        Ok(status) => Ok(Some(Json(status.bits()))),
        Err(error) => match error {
            NoteRepositoryError::NotFound => Ok(None), // 404
            NoteRepositoryError::Internal(inner) => Err(ErrorResponse {
                error: inner.to_string(),
            }),
            _any_other => Err(ErrorResponse {
                error: "undefined note repository error".to_string(),
            }),
        },
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
impl TryFrom<MixRequest> for crate::db::models::notes::FullNote {
    type Error = ErrorResponse; // ! FIXME: bad, should return client error convertible to ErrorResponse

    fn try_from(req: MixRequest) -> Result<Self, Self::Error> {
        // use miden_objects::block::BlockNumber;
        use crate::db::models::notes as models;
        use miden_objects::note::{Note as OnchainNote, NoteFile};
        use miden_objects::utils::Serializable as _;
        use miden_objects::utils::ToHex as _;

        let note = OnchainNote::try_from(&req).map_err(|err| ErrorResponse {
            error: err.to_string(),
        })?;

        let serialized_note = note.to_bytes().to_hex();
        let serialized_note_id = note.id().to_string();

        Ok(models::FullNote {
            note_id: serialized_note_id,
            note: serialized_note,
            account_id: req.account_id,
            status: models::NoteStatus::ACCEPTED,
            scheduled_datetime: None,
        })
    }
}
