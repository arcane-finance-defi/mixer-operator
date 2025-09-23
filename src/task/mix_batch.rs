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

struct AsyncMixBatchTaskError(anyhow::Error);

#[derive(Serialize, Deserialize)]
#[serde(crate = "fang::serde")]
pub struct AsyncMixBatchTask {
    pub task_id: String,
    pub scheduled_at: DateTime<Utc>,
}

impl AsyncMixBatchTask {
    pub fn new(task_id: &str, scheduled_at: DateTime<Utc>) -> Self {
        AsyncMixBatchTask {
            task_id: task_id.to_string(),
            scheduled_at,
        }
    }
}

#[typetag::serde]
#[async_trait]
impl AsyncRunnable for AsyncMixBatchTask {
    async fn run(&self, _queueable: &dyn AsyncQueueable) -> Result<(), FangError> {
        let db = DatabaseStorage::note_storage().await.map_err(AsyncMixBatchTaskError)?;

        // TODO:
        // 1. Get ready notes by status and schedule, and not in processing currently by scheduled mix worker
        todo!();
        // 2. Batch them to transactions by MAX_NOTES_IN_BATCH_TRANSACTION size
        todo!();
        // 3. Check progress and mark executed notes ready 
        todo!();
        // task_id is effectively request_id in the storage
        let note_record = db
            .get_note_by_request_id(&self.task_id)
            .await
            .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("note repo {}", e.to_string())))?;

        tracing::trace!("Unpacking note record");
        let FullNote { note_id, note, account_id, .. } = note_record;

        let note_bytes = hex::decode(note)
            .with_context(|| format!("decoding from hex string note {note_id}"))
            .map_err(AsyncMixBatchTaskError)?;
        let note = Note::read_from_bytes(note_bytes.as_slice())
            .with_context(|| format!("reading note from bytes for {note_id}"))
            .map_err(AsyncMixBatchTaskError)?;
        let faucet_id = AccountId::from_hex(&account_id)
            .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("{e}")))?;

        tracing::trace!("Obtaining mixer client sender");
        let client = mixer_client_sender().map_err(AsyncMixBatchTaskError)?;
        // TODO: should lock note to avoid inclusion to batch transaction by mix_batch
        let (note_id, tx_id) = mix(client.clone(), note, faucet_id)
            .await
            .with_context(|| "async mix task worker is mixing note {}")
            .map_err(AsyncMixBatchTaskError)?;
        tracing::info!("Completed mix for note_id={note_id} tx_id={tx_id}");

        match super::storage::set_note_txed(&*db, note_id).await {
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
                Err(AsyncMixBatchTaskError(err).into())
            },
        }
    }
    // this func is optional
    // Default task_type is common
    fn task_type(&self) -> String {
        "mix-batch_task-type".to_string()
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
        let cron_schedule = "*/10 * * * * * *";
        Some(Scheduled::CronPattern(cron_schedule.to_string()))
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

impl From<AsyncMixBatchTaskError> for FangError {
    fn from(err: AsyncMixBatchTaskError) -> Self {
        FangError { description: format!("mix batch err {:#?}", err.0) }
    }
}
