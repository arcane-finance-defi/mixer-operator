use std::any::Any;
use std::fmt::Debug;
use tokio::sync::mpsc::Sender;
use crate::rpc::commands::chain_tip::ChainTipCommand;
use crate::rpc::common::{QueuedRpcRequest, RpcRequest};
use crate::rpc::errors::RpcRequestExecutionError;

pub struct RpcClient {
    sender: Sender<QueuedRpcRequest<Box<dyn Any + Send + Sync>>>,
}

impl RpcClient {
    pub fn new(sender: Sender<QueuedRpcRequest<Box<dyn Any + Send + Sync>>>) -> Self {
        Self { sender }
    }

    pub async fn get_chain_tip(&self) -> Result<Box<u32>, RpcRequestExecutionError> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let request = Box::new(ChainTipCommand {});
        let request: QueuedRpcRequest<Box<dyn Any + Send + Sync>> = QueuedRpcRequest {
            request,
            channel: tx
        };

        self.sender.send(request).await
            .map_err(|e| RpcRequestExecutionError::UnknownError(Box::new(e)))?;

        rx.await
            .map_err(|e| RpcRequestExecutionError::UnknownError(Box::new(e)))?
            .map(|value| value.downcast::<u32>().unwrap())
    }
}