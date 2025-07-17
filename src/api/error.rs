use crate::mixer::MixClientRequest;
use crate::mixer::client::MixerClientError;
use hex::FromHexError;
use miden_objects::AccountIdError;
use miden_objects::utils::DeserializationError;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

#[derive(Error, Debug)]
pub(super) enum EndpointError {
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
            Ok(inner) => EndpointError::from(inner),
            Err(other) => EndpointError::Unknown { source: other },
        }
    }
}
