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
    transaction::TransactionId,
    utils::Deserializable,
};
use tokio::sync::oneshot;

use crate::{
    db::{
        models::{
            notes::FullNote,
        }, DatabaseStorage
    },
    mixer::{client::MixerClientError, MixClientRequest, MixerClientSender},
    task::worker::mixer_client_sender, MAX_NOTES_IN_BATCH_TRANSACTION,
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
        
        // 1. Get ready notes by status and current date
        //    and set status to PROCESSING
        let now = Utc::now();
        let notes = super::storage::poll_for_ready_notes(&(*db), now)
            .await
            .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("poll_for_ready_notes {}", e)))?;
        
        // TODO: group notes somehow by account_id and execute in separate transactions 
        let account_id = notes[0].account_id.clone();
        let mut notes: Vec<_> = notes.into_iter().filter(|note| note.account_id == account_id).collect();
        
        notes.truncate(MAX_NOTES_IN_BATCH_TRANSACTION);

        let note_ids: Vec<_> = notes.iter().map(|note| note.note_id.as_str()).collect();
        super::storage::set_note_processing(&(*db), &note_ids, true)
            .await
            .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("set_note_processing {}", e)))?;
        
        // 2. Batch to single transaction to first note acount_id
        //    clear PROCESSING status if there are errors
        tracing::debug!("Converting from FullNote to Miden Note");
        let notes = notes.iter().map(|fullnote| {
            let FullNote { note_id, note, .. } = fullnote;
            let note_bytes = hex::decode(note)
                .with_context(|| format!("decoding from hex string note {note_id}"))
                .map_err(AsyncMixBatchTaskError)?;
            let note = Note::read_from_bytes(note_bytes.as_slice())
                .with_context(|| format!("reading note from bytes for {note_id}"))
                .map_err(AsyncMixBatchTaskError)?;
            Ok(note)
        }).collect::<Result<Vec<_>, AsyncMixBatchTaskError>>()?;

        tracing::debug!("Converting from String to Miden AccountId");
        let faucet_id = AccountId::from_hex(&account_id)
            .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("{e}")))?;

        tracing::debug!("Obtaining mixer client sender");
        let client = mixer_client_sender().map_err(AsyncMixBatchTaskError)?;
        let tx_id = match mix_batch(client.clone(), notes, faucet_id)
            .await
            .with_context(|| "async mix task worker is mixing note {}") 
        {
            Ok(tx_id) => tx_id,
            Err(error) => {
                tracing::error!("Error when trying to batch mix to {account_id}: {error:#?}");
                
                tracing::debug!("Reset notes status");
                // ! if it fails, notes will be locked up by their status
                super::storage::set_note_processing(&(*db), &note_ids, true)
                    .await
                    .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("reset_note_processing {}", e)))?;
                
                return Err(AsyncMixBatchTaskError(anyhow::anyhow!("mix_batch internal client error: {error:#?}")).into());
            },
        };        
        tracing::info!("Completed mix for account_id={account_id} with tx_id={tx_id}");

        match super::storage::set_notes_txed(&*db, &note_ids).await {
            Ok(_) => {
                tracing::info!(
                    "Successfully save state for txed notes tx_id={tx_id}"
                );
                // TODO: should we save processed tx_id somewhere for tracing?
                Ok(())
            },
            Err(err) => {
                tracing::error!(
                    "Failed to save txed notes tx_id={tx_id} because {err:#?}"
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

    // Every 10 seconds
    fn cron(&self) -> Option<Scheduled> {
        let cron_schedule = "*/10 * * * * * *";
        Some(Scheduled::CronPattern(cron_schedule.to_string()))
    }

    // The maximum number of retries. Set it to 0 to make it not retriable
    // the default value is 20
    fn max_retries(&self) -> i32 {
        20
    }

    fn backoff(&self, attempt: u32) -> u32 {
        u32::pow(2, attempt)
    }
}

impl From<AsyncMixBatchTaskError> for FangError {
    fn from(err: AsyncMixBatchTaskError) -> Self {
        FangError { description: format!("mix batch err {:#?}", err.0) }
    }
}

// TODO: probably should be move out to trait like `Mixer`
#[tracing::instrument(skip(client, notes, account_id))]
pub async fn mix_batch(
    client: MixerClientSender,
    notes: Vec<Note>,
    account_id: AccountId,
) -> anyhow::Result<String> {
    tracing::info!("Executor trying to mix batch for {account_id}");

    let (request, response) = oneshot::channel::<Result<TransactionId, MixerClientError>>();

    client
        .send(MixClientRequest::MixBatch { notes, account_id, response_sink: request })
        .await?;

    // await for result of mixing
    let tx_id = response.await?.with_context(|| format!("internal mix batch error for {account_id}"))?;

    Ok(tx_id.to_string())
}
