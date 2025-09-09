//!
//! Flow:
//! - Extract notes with status satisfying condition to execute
//! - Create JoinSet of exectution tasks and wait async client to finish them
//! - Collect results of execution async

use std::sync::Arc;

use anyhow::{Context, bail};
use miden_objects::{
    account::AccountId,
    note::{Note},
    utils::Deserializable,
};
use tokio::{task::JoinSet};
use tokio_util::sync::CancellationToken;

use crate::{
    db::models::{
        NoteRepository,
        notes::{FullNote, NoteStatus},
    },
    mixer::{MixerClientSender},
    named_future::NamedJoinHandle,
};

struct NoteExecutor {
    client: MixerClientSender,
    storage: Arc<dyn NoteRepository>,
}

pub fn spawn(
    client: MixerClientSender,
    storage: Arc<dyn NoteRepository>,
    cancellation_token: CancellationToken,
) -> NamedJoinHandle {
    let executor = NoteExecutor { client, storage };

    crate::named_future::spawn_named("note executor".into(), executor.run(cancellation_token))
}

impl NoteExecutor {
    async fn run(self, cancellation_token: CancellationToken) -> anyhow::Result<()> {
        loop {
            tokio::select! {
                biased;
                () = cancellation_token.cancelled() => {
                    tracing::warn!("NoteExecutor is cancelling...");
                    break;
                }
                result = self.execute() => {
                    if let Err(e) = result {
                        tracing::error!("NoteExecutor general error: {e:#}");
                        // metrics_push(NoteExecutor::Error); // TODO: could be prometheus, etc.
                    }
                }
            }
        }

        tracing::info!("NoteExecutor is cleaning up");
        Ok(())
    }

    // #[tracing::instrument(skip(self))]
    async fn execute(&self) -> anyhow::Result<()> {
        let pending_notes = self.poll_for_ready_notes().await?;

        if pending_notes.is_empty() {
            tracing::debug!("No work for now");
            tokio::task::yield_now().await;
            return Ok(());
        }

        let mut join_set = JoinSet::new();

        for note_record in pending_notes {
            let FullNote { note_id, note, account_id, .. } = note_record;

            // TODO: should be unified methods to store and load serialized notes without client
            let note_bytes = hex::decode(note)
                .with_context(|| format!("decoding from hex string note {note_id}"))?;
            let note = Note::read_from_bytes(note_bytes.as_slice())
                .with_context(|| format!("reading note from bytes for {note_id}"))?;
            let faucet_id = AccountId::from_hex(&account_id)?;

            
            join_set.spawn(crate::task::mix::mix(self.client.clone(), note, faucet_id));
        }

        tracing::info!("Joining notes batch");
        let results = join_set.join_all().await;

        for r in results {
            match r {
                Ok((note_id, tx_id)) => {
                    tracing::info!("Save state note_id={note_id} tx_id={tx_id}");
                    if let Err(err) = crate::task::mix::set_note_txed(self.storage.as_ref(), note_id).await {
                        tracing::error!("Failed to save state because {err:#?}");
                    } else {
                        tracing::info!("Success");
                    }
                },
                Err(err) => tracing::error!("Failed to execute because {err:#?}"),
            }
        }

        Ok(())
    }

    async fn poll_for_ready_notes(&self) -> anyhow::Result<Vec<FullNote>> {
        let status = NoteStatus::ACCEPTED & !NoteStatus::TXED;

        let notes = match self.storage.get_notes_by_status(status).await {
            Ok(notes) => notes,
            Err(err) => bail!("reading notes storage error {err:#?}"),
        };

        Ok(notes)
    }
}
