//!
//! Flow:
//! - Extract notes with status satisfying condition to execute
//! - Create JoinSet of exectution tasks and wait async client to finish them
//! - Collect results of execution async
//!

use anyhow::{Context, bail};
use miden_objects::{
    account::AccountId,
    note::{Note, NoteId},
    utils::Deserializable,
};
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use crate::db::models::NoteRepository;
use crate::db::models::notes::{FullNote, NoteStatus};
use crate::mixer::MixClientRequest;
use crate::mixer::client::MixerClientError;
use crate::{mixer::MixerClientSender, named_future::NamedJoinHandle};

struct NoteExecutor {
    client: MixerClientSender,
    storage: Box<dyn NoteRepository>,
}

pub fn spawn(
    client: MixerClientSender,
    storage: impl NoteRepository,
    cancellation_token: CancellationToken,
) -> NamedJoinHandle {
    let executor = NoteExecutor {
        client,
        storage: Box::new(storage),
    };

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
            let FullNote {
                note_id,
                note,
                account_id,
                ..
            } = note_record;

            // TODO: should be unified methods to store and load serialized notes without client
            let note_bytes =
                hex::decode(note).context("decoding from hex string note {note_id}")?;
            let note = Note::read_from_bytes(note_bytes.as_slice())
                .context("reading note from bytes for {note_id}")?;

            let faucet_id = AccountId::from_hex(&account_id)?;

            join_set.spawn(mix(self.client.clone(), note, faucet_id));
        }

        tracing::info!("Joining notes batch");
        let results = join_set.join_all().await;

        for r in results {
            match r {
                Ok((note_id, tx_id)) => {
                    tracing::info!("Save state note_id={note_id} tx_id={tx_id}");
                    if let Err(err) = self.set_note_txed(note_id).await {
                        tracing::error!("Failed to save state because {err:#?}");
                    } else {
                        tracing::info!("Success");
                    }
                }
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

    #[tracing::instrument(skip(self))]
    async fn set_note_txed(&self, note_id: NoteId) -> anyhow::Result<()> {
        match self
            .storage
            .update_note_status_by_id(&note_id.to_string(), NoteStatus::TXED)
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => bail!("update notes status error {err:#?}"),
        }
    }
}

#[tracing::instrument(skip(client, note))]
async fn mix(
    client: MixerClientSender,
    note: Note,
    account_id: AccountId,
) -> anyhow::Result<(NoteId, String)> {
    let note_id = note.id();
    tracing::info!("Executor trying to mix {note_id}");

    let (request, response) = oneshot::channel::<Result<String, MixerClientError>>();

    client
        .send(MixClientRequest::Mix {
            note,
            account_id,
            response_sink: request,
        })
        .await?;

    // await for result of mixing
    let tx_id = response
        .await?
        .with_context(|| format!("internal mixer error for {note_id}"))?;

    Ok((note_id, tx_id))
}
