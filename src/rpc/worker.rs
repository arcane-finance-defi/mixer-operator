use std::any::Any;
use std::fmt::Debug;
use std::sync::Arc;
use miden_client::rpc::{Endpoint, TonicRpcClient};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;
use url::Url;
use super::common::QueuedRpcRequest;
use super::errors::RpcWorkerErrors;


pub struct RpcClientWorker {
    rpc: TonicRpcClient,
    queue: mpsc::Receiver<QueuedRpcRequest<Box<dyn Any + Send + Sync>>>,
}

impl RpcClientWorker {
    pub fn new(
        endpoint: String,
        timeout_ms: u64,
        receiver: mpsc::Receiver<QueuedRpcRequest<Box<dyn Any + Send + Sync>>>,
    ) -> Result<Self, RpcWorkerErrors> {
        let endpoint = Url::parse(endpoint.as_str()).map_err(RpcWorkerErrors::MalformedUrl)?;

        let endpoint = Endpoint::new(
            endpoint.scheme().to_string(),
            endpoint.host().ok_or(RpcWorkerErrors::UrlWithoutHost)?.to_string(),
            endpoint.port()
        );

        let rpc = TonicRpcClient::new(&endpoint, timeout_ms);

        Ok(
            Self {
                rpc,
                queue: receiver
            }
        )
    }

    pub fn start(mut self, runtime: Arc<Runtime>) {
        let inner_runtime = runtime.clone();
        std::thread::spawn(move || {
            while let Some(from_queue) = inner_runtime.block_on(self.queue.recv()) {
                let request = from_queue.request;
                let rx = from_queue.channel;
                let response = inner_runtime.block_on(
                    request.send(&mut self.rpc)
                );
                rx.send(response).unwrap()
            }
        });
    }
}