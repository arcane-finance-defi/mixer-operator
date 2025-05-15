use miden_client::{block::BlockHeader, crypto::MmrProof, note::BlockNumber};

use super::MidenFacadeRpcError;
#[derive(Debug, Clone)]
pub struct GetBlockHeaderRequest {
    pub block_num: Option<BlockNumber>,
    pub include_mmr_proof: bool,
}
#[derive(Debug, Clone)]
pub struct GetBlockHeaderResponse {
    pub block_header: BlockHeader,
    pub mmr_proof: Option<MmrProof>,
}

pub type ResponseOneShot<T> = tokio::sync::oneshot::Sender<Result<T, MidenFacadeRpcError>>;

#[derive(Debug)]
pub enum MidenRequest {
    GetBlockHeader {
        request: GetBlockHeaderRequest,
        on_response: ResponseOneShot<(BlockHeader, Option<MmrProof>)>,
    },

}
