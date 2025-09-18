use anyhow::Context;
use chrono::{DateTime, Utc};
use fang::{
    AsyncRunnable, FangError, Scheduled, async_trait,
    asynk::async_queue::AsyncQueueable,
    serde::{Deserialize, Serialize},
};
use miden_objects::{
    account::AccountId,
    note::{Note, NoteId},
    utils::Deserializable,
};
use tokio::sync::oneshot;

use crate::{
    db::{
        DatabaseStorage,
        models::{
            NoteRepository,
            notes::{FullNote, NoteStatus},
        },
    },
    mixer::{MixClientRequest, MixerClientSender, client::MixerClientError},
    task::worker::mixer_client_sender,
};

struct AsyncMixTaskError(anyhow::Error);

#[derive(Serialize, Deserialize)]
#[serde(crate = "fang::serde")]
pub struct AsyncMixTask {
    pub task_id: String,
    pub scheduled_at: DateTime<Utc>,
}

impl AsyncMixTask {
    pub fn new(task_id: &str, scheduled_at: DateTime<Utc>) -> Self {
        AsyncMixTask {
            task_id: task_id.to_string(),
            scheduled_at,
        }
    }
}

#[typetag::serde]
#[async_trait]
impl AsyncRunnable for AsyncMixTask {
    async fn run(&self, _queueable: &dyn AsyncQueueable) -> Result<(), FangError> {
        let db = DatabaseStorage::note_storage().await.map_err(AsyncMixTaskError)?;

        // task_id is effectively request_id in the storage
        let note_record = db
            .get_note_by_request_id(&self.task_id)
            .await
            .map_err(|e| AsyncMixTaskError(anyhow::anyhow!("note repo {}", e.to_string())))?;

        tracing::trace!("Unpacking note record");
        let FullNote { note_id, note, account_id, .. } = note_record;

        let note_bytes = hex::decode(note)
            .with_context(|| format!("decoding from hex string note {note_id}"))
            .map_err(AsyncMixTaskError)?;
        let note = Note::read_from_bytes(note_bytes.as_slice())
            .with_context(|| format!("reading note from bytes for {note_id}"))
            .map_err(AsyncMixTaskError)?;
        let faucet_id = AccountId::from_hex(&account_id)
            .map_err(|e| AsyncMixTaskError(anyhow::anyhow!("{e}")))?;

        tracing::trace!("Obtaining mixer client sender");
        let client = mixer_client_sender().map_err(AsyncMixTaskError)?;

        let (note_id, tx_id) = mix(client.clone(), note, faucet_id)
            .await
            .with_context(|| "async mix task worker is mixing note {}")
            .map_err(AsyncMixTaskError)?;
        tracing::info!("Completed mix for note_id={note_id} tx_id={tx_id}");

        match set_note_txed(&*db, note_id).await {
            Ok(_) => {
                tracing::info!(
                    "Successfully save state for txed note note_id={note_id} tx_id={tx_id}"
                );
                Ok(())
            },
            Err(err) => {
                tracing::error!(
                    "Failed to save txed note note_id={note_id} tx_id={tx_id} because {err:#?}"
                );
                Err(AsyncMixTaskError(err).into())
            },
        }
    }
    // this func is optional
    // Default task_type is common
    fn task_type(&self) -> String {
        "mix-task-type".to_string()
    }

    // If `uniq` is set to true and the task is already in the storage, it won't be inserted again
    // The existing record will be returned for for any insertions operaiton
    fn uniq(&self) -> bool {
        true
    }

    // This will be useful if you would like to schedule tasks.
    // default value is None (the task is not scheduled, it's just executed as soon as it's
    // inserted)
    fn cron(&self) -> Option<Scheduled> {
        Some(Scheduled::ScheduleOnce(self.scheduled_at))
    }

    // the maximum number of retries. Set it to 0 to make it not retriable
    // the default value is 20
    fn max_retries(&self) -> i32 {
        20
    }

    // backoff mode for retries in seconds?
    fn backoff(&self, attempt: u32) -> u32 {
        u32::pow(2, attempt)
    }
}

impl From<AsyncMixTaskError> for FangError {
    fn from(err: AsyncMixTaskError) -> Self {
        FangError { description: format!("{:#?}", err.0) }
    }
}

// TODO: probably should be move out to trait like `Mixer`
#[tracing::instrument(skip(client, note, account_id))]
pub async fn mix(
    client: MixerClientSender,
    note: Note,
    account_id: AccountId,
) -> anyhow::Result<(NoteId, String)> {
    let note_id = note.id();
    tracing::info!("Executor trying to mix {note_id}");

    let (request, response) = oneshot::channel::<Result<String, MixerClientError>>();

    client
        .send(MixClientRequest::Mix { note, account_id, response_sink: request })
        .await?;

    // await for result of mixing
    let tx_id = response.await?.with_context(|| format!("internal mixer error for {note_id}"))?;

    Ok((note_id, tx_id))
}

#[tracing::instrument(skip(storage))]
pub async fn set_note_txed(storage: &dyn NoteRepository, note_id: NoteId) -> anyhow::Result<()> {
    match storage.update_note_status_by_id(&note_id.to_string(), NoteStatus::TXED).await {
        Ok(_) => Ok(()),
        Err(err) => anyhow::bail!("update notes status error {err:#?}"),
    }
}
