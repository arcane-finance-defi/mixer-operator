use std::any::Any;
use miden_client::rpc::{NodeRpcClient, TonicRpcClient};
use miden_objects::block::BlockNumber;
use rocket::async_trait;
use crate::rpc::errors::RpcRequestExecutionError;
use super::super::common::RpcRequest;

#[derive(Debug)]
pub struct ChainTipCommand;

#[async_trait]
impl RpcRequest<Box<dyn Any + Send + Sync>> for ChainTipCommand {
    async fn send(&self, rpc: &mut TonicRpcClient) -> Result<Box<dyn Any + Send + Sync>, RpcRequestExecutionError> {
        let result = rpc.sync_notes(BlockNumber::from(0), &[]).await
            .map_err(RpcRequestExecutionError::from)?;

        Ok(Box::new(result.chain_tip))
    }
}