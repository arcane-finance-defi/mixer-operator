use miden_client::rpc::RpcError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RpcWorkerErrors {
    #[error("Url parse error {0:}")]
    MalformedUrl(#[source] url::ParseError),
    #[error("Url should include host")]
    UrlWithoutHost
}

#[derive(Debug, Error)]
pub enum RpcRequestExecutionError {
    #[error(transparent)]
    InternalRpcError(#[from] RpcError),
    #[error("Internal error {0:}")]
    UnknownError(#[source] Box<dyn std::error::Error + Send>),
}