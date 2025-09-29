use hex::FromHexError;
use miden_objects::{AccountIdError, utils::DeserializationError};
use rocket::{
    http::Status,
    response,
    serde::json::{Json, json},
};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::mixer::{MixClientRequest, client::MixerClientError};

#[derive(Error, Debug)]
pub enum EndpointError {
    #[error(transparent)]
    FromHex(#[from] FromHexError),
    #[error(transparent)]
    Deserialization(#[from] DeserializationError),
    #[error(transparent)]
    AccountId(#[from] AccountIdError),
    #[error(transparent)]
    MpscSend(#[from] Box<mpsc::error::SendError<MixClientRequest>>),
    #[error(transparent)]
    OneshotRecv(#[from] oneshot::error::RecvError),
    #[error(transparent)]
    MixerClient(#[from] Box<MixerClientError>),
    #[error("Internal storage error")]
    InternalStorage(String),
    #[error("Task queue error")]
    TaskQueue(String),
    #[error("Note not found")]
    NoteNotFound(String),
    #[error("Batch size exceeds the limit")]
    BatchLimit,
    #[error("Unknown source error")]
    Unknown { source: anyhow::Error },
}

// TODO (kochetkov): I believe we should use `snafu` crate or smth for this purpose
impl From<anyhow::Error> for EndpointError {
    fn from(e: anyhow::Error) -> Self {
        match e.downcast::<EndpointError>() {
            Ok(inner) => inner,
            Err(other) => EndpointError::Unknown { source: other },
        }
    }
}

// https://stuarth.github.io/rocket-error-handling/
impl<'r, 'o: 'r> response::Responder<'r, 'o> for EndpointError {
    fn respond_to(self, req: &'r rocket::Request<'_>) -> response::Result<'o> {
        // log `self` to your favored error tracker, e.g.
        // sentry::capture_error(&self);

        match self {
            EndpointError::FromHex(err) => {
                let error_message =
                    Json(json!({"error": format!("FromHex error occurred - {err}")}));
                response::status::Custom(Status::BadRequest, error_message).respond_to(req)
            },
            EndpointError::AccountId(err) => {
                let error_message =
                    Json(json!({"error": format!("AccountId error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::MpscSend(err) => {
                let error_message =
                    Json(json!({"error": format!("Mixer client request error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::OneshotRecv(err) => {
                let error_message =
                    Json(json!({"error": format!("Mixer client response error - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::Deserialization(err) => {
                let error_message =
                    Json(json!({"error": format!("Deserialization error occurred - {err}")}));
                response::status::Custom(Status::BadRequest, error_message).respond_to(req)
            },
            EndpointError::MixerClient(err) => {
                let error_message =
                    Json(json!({"error": format!("Mixer Client error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::TaskQueue(err) => {
                let error_message =
                    Json(json!({"error": format!("Task Queue error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::InternalStorage(err) => {
                let error_message =
                    Json(json!({"error": format!("Internal State error occurred - {err}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            EndpointError::NoteNotFound(_) => Status::NotFound.respond_to(req),
            EndpointError::BatchLimit => {
                let error_message =
                    Json(json!({"error": format!("{}", EndpointError::BatchLimit.to_string())}));
                response::status::Custom(Status::BadRequest, error_message).respond_to(req)
            },
            EndpointError::Unknown { source } => {
                let error_message =
                    Json(json!({"error": format!("An unknown error occurred - {source}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
        }
    }
}

impl From<fang::AsyncQueueError> for EndpointError {
    fn from(qerr: fang::AsyncQueueError) -> Self {
        tracing::error!("AsyncQueueError occured: {qerr:#?}");
        EndpointError::TaskQueue(qerr.to_string())
    }
}

impl From<crate::db::models::NoteRepositoryError> for EndpointError {
    fn from(derr: crate::db::models::NoteRepositoryError) -> Self {
        use crate::db::models::NoteRepositoryError;
        match derr {
            NoteRepositoryError::NotFound(smth) => EndpointError::NoteNotFound(smth),
            NoteRepositoryError::MoreThanOneRowAffected => {
                EndpointError::InternalStorage(derr.to_string())
            },
            NoteRepositoryError::Internal(e) => EndpointError::InternalStorage(e.to_string()),
            NoteRepositoryError::InteractDeadpool(s) => EndpointError::InternalStorage(s),
        }
    }
}
