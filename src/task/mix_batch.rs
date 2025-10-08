use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use chrono::{DateTime, Utc};
use fang::{
    AsyncRunnable, FangError, Scheduled, async_trait,
    asynk::async_queue::AsyncQueueable,
    serde::{Deserialize, Serialize},
};
use miden_objects::{
    account::AccountId, note::Note, transaction::TransactionId, utils::Deserializable,
};
use tokio::sync::oneshot;

use crate::{
    MAX_NOTES_IN_BATCH_TRANSACTION,
    db::{
        DatabaseStorage,
        models::{NoteRepository, notes::FullNote},
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
        tracing::debug!("Obtaining database connection for AsyncMixBatchTask worker");
        let db = DatabaseStorage::note_storage().await.map_err(AsyncMixBatchTaskError)?;
        tracing::debug!("Obtaining mixer client sender");
        let client = mixer_client_sender().map_err(AsyncMixBatchTaskError)?;

        // 1. Get ready notes by status and current date
        let now = Utc::now();
        let notes = super::storage::poll_for_ready_notes(&(*db), now)
            .await
            .map_err(|e| AsyncMixBatchTaskError(anyhow::anyhow!("poll_for_ready_notes {}", e)))?;

        if notes.is_empty() {
            return Ok(());
        }

        // 2. Group notes by `faucet_id` string
        let mut note_groups: HashMap<String, Vec<FullNote>> = HashMap::new();
        notes.into_iter().for_each(|note| {
            let group = note_groups.entry(note.account_id.clone()).or_default();
            group.push(note);
        });

        // TODO: test batching logic
        // 3. Execute each notes group by MAX_NOTES_IN_BATCH_TRANSACTION at once
        for account_id in note_groups.keys() {
            tracing::info!("Try mix for faucet_id {:#?}", account_id);
            // form batches for current acccount_id of MAX_NOTES_IN_BATCH_TRANSACTION size
            let notes = note_groups
                .get(account_id)
                .ok_or_else(|| AsyncMixBatchTaskError(anyhow::anyhow!("grouping notes failed!")))?;
            let notes: Vec<&[FullNote]> = notes.chunks(MAX_NOTES_IN_BATCH_TRANSACTION).collect();

            for (idx, notes_batch) in notes.into_iter().enumerate() {
                mix_batch_inner(notes_batch, account_id, db.clone(), client).await.map_err(
                    |error| {
                        tracing::error!("Failed to run batch #{idx} with account_id={account_id}");
                        AsyncMixBatchTaskError(error)
                    },
                )?;
            }
        }
        Ok(())
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
        FangError {
            description: format!("mix batch err {:#?}", err.0),
        }
    }
}

// TODO: execute in separate SINGLE transaction to avoid any r/w status hassle and sideffects
async fn mix_batch_inner(
    notes_batch: &[FullNote],
    account_id: &str,
    db: Arc<dyn NoteRepository>,
    client: &MixerClientSender,
) -> anyhow::Result<()> {
    // 1. Set status to PROCESSING
    let note_ids: Vec<_> = notes_batch.iter().map(|note| note.note_id.as_str()).collect();
    super::storage::set_note_processing(&(*db), &note_ids, true)
        .await
        .map_err(|e| anyhow::anyhow!("set_note_processing {}", e))?;

    // 2. Batch to single transaction to first note acount_id clear PROCESSING status if there are
    //    any errors
    tracing::debug!("Converting from FullNote to Miden Note");
    let notes_batch = notes_batch
        .iter()
        .map(|fullnote| {
            let FullNote { note_id, note, .. } = fullnote;
            let note_bytes = hex::decode(note)
                .with_context(|| format!("decoding from hex string note {note_id}"))?;
            let note = Note::read_from_bytes(note_bytes.as_slice())
                .with_context(|| format!("reading note from bytes for {note_id}"))?;
            Ok(note)
        })
        .collect::<Result<Vec<_>, anyhow::Error>>()?;

    tracing::debug!("Converting from String to Miden AccountId");
    let faucet_id = AccountId::from_hex(account_id).map_err(|e| anyhow::anyhow!("{e}"))?;

    // 3. Execute transaction
    let tx_id = match mix_batch_with_client(notes_batch, faucet_id, client.clone())
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
                .map_err(|e| anyhow::anyhow!("reset_note_processing {}", e))?;

            anyhow::bail!("mix_batch internal client error: {error:#?}");
        },
    };
    tracing::info!("Completed mix for account_id={account_id} with tx_id={tx_id}");

    // 4. Clear note statuses to TXED
    match super::storage::set_notes_txed(&*db, &note_ids).await {
        Ok(_) => {
            tracing::info!("Successfully save state for txed notes tx_id={tx_id}");
            // TODO: should we save somewhere processed tx_id for tracing?
            Ok(())
        },
        Err(err) => {
            tracing::error!("Failed to save txed notes tx_id={tx_id} because {err:#?}");
            Err(err)
        },
    }
}

#[tracing::instrument(skip(client, notes, account_id))]
async fn mix_batch_with_client(
    notes: Vec<Note>,
    account_id: AccountId,
    client: MixerClientSender,
) -> anyhow::Result<String> {
    tracing::info!("Executor trying to mix batch for {account_id}");

    let (request, response) = oneshot::channel::<Result<TransactionId, MixerClientError>>();

    client
        .send(MixClientRequest::MixBatch {
            notes,
            account_id,
            response_sink: request,
        })
        .await?;

    // await for result of mixing
    let tx_id = response
        .await?
        .with_context(|| format!("internal mix batch error for {account_id}"))?;

    Ok(tx_id.to_string())
}

#[cfg(test)]
mod test {
    #[tokio::test]
    async fn test_batch_grouping() {
        // TODO: mock database records
    }
}
