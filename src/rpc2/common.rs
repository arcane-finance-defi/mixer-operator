use std::future::Future;
use std::thread::{self};

use super::errors::MidenFacadeRpcError;
use super::requests::{GetBlockHeaderRequest, MidenRequest, ResponseOneShot};
use miden_client::block::BlockHeader;
use miden_client::crypto::MmrProof;
use miden_client::rpc::{Endpoint, NodeRpcClient, RpcError, TonicRpcClient};
use miden_objects::block::BlockNumber;
use tokio::sync::oneshot;
use tracing::{error, info};

#[async_trait::async_trait]
pub trait MidenRpcAsyncFacade {
    async fn get_block_header(
        &self,
        block_num: Option<BlockNumber>,
        include_mmr_proof: bool,
    ) -> Result<(BlockHeader, Option<MmrProof>), MidenFacadeRpcError>;
}

pub struct ThreadPoolMidenRpcAsyncFacade {
    queue: crossbeam_channel::Sender<MidenRequest>,
    handles: Vec<thread::JoinHandle<()>>,
}

impl ThreadPoolMidenRpcAsyncFacade {
    pub fn new(client_count: u32, endpoint: &Endpoint, timeout_ms: u64) -> Self {
        let (sx, rx) = crossbeam_channel::unbounded::<MidenRequest>();
        let mut handles = Vec::new();

        for i in 0..client_count {
            let rpc_client = TonicRpcClient::new(endpoint, timeout_ms);
            let receiver = rx.clone();
            let handle = thread::Builder::new()
                .name(format!("rpc-worker-{}", i))
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();

                    runtime.block_on(async {
                        loop {
                            if let Ok(request) = receiver.recv() {
                                execute_request(&rpc_client, request).await;
                            } else {
                                error!("Channel disconnected for worker {}", i);
                                break;
                            }
                        }
                    });
                })
                .unwrap();
            handles.push(handle);
        }
        ThreadPoolMidenRpcAsyncFacade { queue: sx, handles }
    }

    async fn enqueue<Resp, T: FnOnce(ResponseOneShot<Resp>) -> MidenRequest>(
        &self,
        request: T,
    ) -> Result<Resp, MidenFacadeRpcError> {
        let (sx, rx) = oneshot::channel();
        let request = request(sx);
        info!(
            "Sending request {:?} on thread {}",
            request,
            thread::current().name().unwrap_or("unnamed")
        );
        self.queue.send(request)?;
        rx.await.map_err(MidenFacadeRpcError::ResponseError)?
    }
}

async fn execute_request(rpc_client: &TonicRpcClient, request: MidenRequest) {
    match request {
        MidenRequest::GetBlockHeader {
        3request,
            on_response,
        } => {
            let result = rpc_client
                .get_block_header_by_number(request.block_num, request.include_mmr_proof)
                .await
                .map_err(MidenFacadeRpcError::RpcError);
            let _ = on_response.send(result);
        }
    }
}

impl Drop for ThreadPoolMidenRpcAsyncFacade {
    fn drop(&mut self) {
        let (_, _) = crossbeam_channel::unbounded::<MidenRequest>();
        let _ = std::mem::replace(
            &mut self.queue,
            crossbeam_channel::unbounded::<MidenRequest>().0,
        );

        for handle in self.handles.drain(..) {
            let _ = handle.join();
        }
    }
}

#[async_trait::async_trait]
impl MidenRpcAsyncFacade for ThreadPoolMidenRpcAsyncFacade {
    async fn get_block_header(
        &self,
        block_num: Option<BlockNumber>,
        include_mmr_proof: bool,
    ) -> Result<(BlockHeader, Option<MmrProof>), MidenFacadeRpcError> {
        self.enqueue(|sx| MidenRequest::GetBlockHeader {
            request: GetBlockHeaderRequest {
                block_num,
                include_mmr_proof,
            },
            on_response: sx,
        })
        .await
    }
}
