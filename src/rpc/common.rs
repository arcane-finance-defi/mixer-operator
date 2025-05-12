use std::fmt::Debug;
use miden_client::rpc::TonicRpcClient;
use rocket::async_trait;
use tokio::sync::oneshot;
use super::errors::RpcRequestExecutionError;

#[async_trait]
pub trait RpcRequest<T> where T: Send + Debug {
    async fn send(&self, rpc: &mut TonicRpcClient) -> Result<T, RpcRequestExecutionError>;
}

pub struct QueuedRpcRequest<T> where T: Send + Sync + 'static {
    pub request: Box<dyn RpcRequest<T> + Send>,
    pub channel: oneshot::Sender<Result<T, RpcRequestExecutionError>>,
}