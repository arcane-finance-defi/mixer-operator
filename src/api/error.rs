use hex::FromHexError;
use miden_objects::utils::DeserializationError;
use miden_objects::AccountIdError;
use tokio::sync::{mpsc, oneshot};
use crate::mixer::client::MixerClientError;
use crate::mixer::MixClientRequest;
use thiserror::Error;

#[derive(Error, Debug)]
pub(super) enum EndpointError {
    #[error(transparent)]
    HexError(#[from] FromHexError),
    #[error(transparent)]
    DeserializationError(#[from] DeserializationError),
    #[error(transparent)]
    AccountIdError(#[from] AccountIdError),
    #[error(transparent)]
    SendError(#[from] mpsc::error::SendError<MixClientRequest>),
    #[error(transparent)]
    RecvError(#[from] oneshot::error::RecvError),
    #[error(transparent)]
    MixerClientError(#[from] MixerClientError),
    #[error("{0}")]
    DatabaseLogicError(String),
    #[error(transparent)]
    DatabaseError(#[from] diesel::result::Error),
    #[error(transparent)]
    DatabasePoolError(#[from] diesel::r2d2::PoolError),
}