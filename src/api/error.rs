use crate::mixer::MixClientRequest;
use crate::mixer::client::MixerClientError;
use hex::FromHexError;
use miden_objects::AccountIdError;
use miden_objects::utils::DeserializationError;
use rocket::serde::json::json;
use rocket::{response, serde::json::Json};
use rocket::http::Status;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

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
    #[error("unknown source error")]
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
            EndpointError::Unknown { source } => {
                let error_message = Json(json!({"error": format!("An unknown error occurred - {source}")}));
                response::status::Custom(Status::InternalServerError, error_message)
                    .respond_to(req)
            }
            _ => Status::BadRequest
                    .respond_to(req)
        }
    }
}