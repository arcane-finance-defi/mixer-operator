use miden_objects::{
    account::AccountId,
    note::{Note, NoteId},
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot},
};
use tokio_util::sync::CancellationToken;

use crate::{
    config::{MidenClient as MidenClientConfig},
    mixer::client::{MixerClient, MixerClientError},
};

pub mod client;
pub mod utils;

pub type MixerClientSender = mpsc::Sender<MixClientRequest>;
pub type MixerClientReceiver = mpsc::Receiver<MixClientRequest>;

type MixerClientResponse<T> = oneshot::Sender<Result<T, MixerClientError>>;

pub enum MixClientRequest {
    Mix {
        note: Note,
        account_id: AccountId,
        response_sink: MixerClientResponse<String>,
    },
    Poll {
        note_id: NoteId,
        response_sink: MixerClientResponse<bool>,
    },
}

pub fn event_loop(
    config: MidenClientConfig,
    debug: bool,
    mut receiver: MixerClientReceiver,
    runtime: Runtime,
    cancellation_token: CancellationToken,
) {
    let mut client = runtime
        .block_on(MixerClient::new(
            config.rpc_url().as_str(),
            config.rpc_timeout_ms(),
            None,
            debug,
        ))
        .unwrap();

    runtime
        .block_on(client.initialize(config.private_account_dir(), config.public_account_ids()))
        .unwrap();

    loop {
        if cancellation_token.is_cancelled() && receiver.is_empty() {
            tracing::warn!("Cancellation token trigger");
            break;
        }

        let request = runtime.block_on(receiver.recv());

        match request {
            Some(MixClientRequest::Mix { note, account_id, response_sink }) => {
                let result = runtime.block_on(client.mix(note, account_id));
                tracing::info!("MixerClient::Mix {result:#?}");
                response_sink.send(result).unwrap();
            },

            Some(MixClientRequest::Poll { note_id, response_sink }) => {
                let result = runtime.block_on(client.is_note_onchain(note_id));
                tracing::info!("MixerClient::Poll {result:#?}");
                response_sink.send(result).unwrap();
            },

            None => {
                tracing::warn!("Channel closed");
                break;
            },
        }
    }

    tracing::warn!("Event loop finished!");
}
