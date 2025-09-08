use hex::FromHexError;
use miden_objects::{AccountIdError, utils::DeserializationError};
use rocket::{
    http::Status,
    response,
    serde::json::{Json, json},
};
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::{mixer::{client::MixerClientError, MixClientRequest}};

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

impl<'r, 'o: 'r> response::Responder<'r, 'o> for EndpointError {
    fn respond_to(self, req: &'r rocket::Request<'_>) -> response::Result<'o> {
        // log `self` to your favored error tracker, e.g.
        // sentry::capture_error(&self);

        match self {
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
            // TODO: other
            EndpointError::Unknown { source } => {
                let error_message =
                    Json(json!({"error": format!("An unknown error occurred - {source}")}));
                response::status::Custom(Status::InternalServerError, error_message).respond_to(req)
            },
            _ => Status::InternalServerError.respond_to(req),
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
        EndpointError::InternalStorage(derr.to_string())
    }
}