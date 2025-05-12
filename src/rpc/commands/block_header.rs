use std::any::Any;
use miden_client::rpc::{NodeRpcClient, TonicRpcClient};
use miden_objects::block::BlockNumber;
use rocket::async_trait;
use crate::rpc::errors::RpcRequestExecutionError;
use super::super::common::RpcRequest;

#[derive(Debug)]
pub struct BlockHeaderCommand {
    block_height: u32
}

impl BlockHeaderCommand {
    fn block_height(&self) -> u32 {
        self.block_height
    }
}

impl From<BlockNumber> for BlockHeaderCommand {
    fn from(block_number: BlockNumber) -> Self {
        BlockHeaderCommand {
            block_height: block_number.as_u32()
        }
    }
}

#[async_trait]
impl RpcRequest<Box<dyn Any + Send + Sync>> for BlockHeaderCommand {
    async fn send(&self, rpc: &mut TonicRpcClient) -> Result<Box<dyn Any + Send + Sync>, RpcRequestExecutionError> {
        let (header, _) = rpc.get_block_header_by_number(Some(self.block_height.into()),false).await
            .map_err(RpcRequestExecutionError::from)?;

        let mmr = rpc.sync_state()

        Ok(Box::new(result.chain_tip))
    }
}