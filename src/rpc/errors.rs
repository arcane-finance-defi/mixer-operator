use super::requests::MidenRequest;
use crossbeam_channel::SendError;
use miden_client::rpc::RpcError;
use thiserror::Error;
use tokio::sync::oneshot::error::RecvError;

#[derive(Debug, Error)]
pub enum MidenFacadeRpcError {
    #[error("RPC client error: {0}")]
    RpcError(#[from] RpcError),
    #[error("Channel error: {0}")]
    ChannelError(#[from] SendError<MidenRequest>),
    #[error("Response error: {0}")]
    ResponseError(#[from] RecvError),
    #[error("Invalid address: {0}")]
    InvalidAddress(String),
}
