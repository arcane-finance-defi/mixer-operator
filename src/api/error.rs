use crate::mixer::MixClientRequest;
use crate::mixer::client::MixerClientError;
use hex::FromHexError;
use miden_objects::AccountIdError;
use miden_objects::utils::DeserializationError;
use rocket::response;
use rocket::http::Status;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

#[derive(Error, Debug)]
pub(super) enum EndpointError { // TODO: what visibility should we use?
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
            // in our simplistic example, we're happy to respond with the default 500 responder in all cases 
            _ => Status::InternalServerError.respond_to(req)
        }
    }
}