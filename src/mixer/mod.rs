use miden_objects::{
    account::AccountId,
    note::NoteFile
};
use tokio::{
    runtime::Runtime,
    sync::{mpsc, oneshot}
};

use crate::config::Config;
use crate::mixer::client::{MixerClient, MixerClientError};

pub mod client;

pub enum MixClientRequest {
    Mix {
        note_file: NoteFile,
        account_id: AccountId,
        response_sink: oneshot::Sender<Result<String, MixerClientError>>,
    },
}

pub fn event_loop(
    config: Config,
    mut receiver: mpsc::Receiver<MixClientRequest>,
    runtime: Runtime,
) {
    let mut client = runtime
        .block_on(MixerClient::new(
            config.rpc_url().as_str(),
            config.rpc_timeout_ms(),
            None,
        ))
        .unwrap();

    runtime
        .block_on(client.initialize(config.private_account_dir(), config.public_account_ids()))
        .unwrap();

    loop {
        let request = runtime.block_on(receiver.recv()).unwrap();

        match request {
            MixClientRequest::Mix {
                note_file,
                account_id,
                response_sink,
            } => {
                let result = runtime.block_on(client.mix(note_file, account_id));

                response_sink.send(result).unwrap();
            }
        }
    }
}
