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
    HexError(#[from] FromHexError),
    #[error(transparent)]
    DeserializationError(#[from] DeserializationError),
    #[error(transparent)]
    AccountIdError(#[from] AccountIdError),
    #[error(transparent)]
    SendError(#[from] Box<mpsc::error::SendError<MixClientRequest>>),
    #[error(transparent)]
    RecvError(#[from] oneshot::error::RecvError),
    #[error(transparent)]
    MixerClientError(#[from] Box<MixerClientError>),
    #[error("{0}")]
    DatabaseLogicError(String),
    #[error(transparent)]
    DatabaseError(#[from] diesel::result::Error),
    #[error(transparent)]
    DatabasePoolError(#[from] diesel::r2d2::PoolError),
}
