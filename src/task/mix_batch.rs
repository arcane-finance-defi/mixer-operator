use std::{collections::HashMap, sync::Arc};

use anyhow::Context;
use chrono::Utc;
use fang::{
    AsyncRunnable, FangError, Scheduled, async_trait,
    asynk::async_queue::AsyncQueueable,
    serde::{Deserialize, Serialize},
};

use crate::{
    MAX_NOTES_IN_BATCH_TRANSACTION,
    db::{
        DatabaseStorage,
        models::{NoteRepository, notes::FullNote},
    },
    mixer::MixerClientSender,
    task::worker::mixer_client_sender,
};

struct AsyncMixBatchTaskError(anyhow::Error);

#[derive(Default, Serialize, Deserialize)]
#[serde(crate = "fang::serde")]
pub struct AsyncMixBatchTask {}

#[typetag::serde]
#[async_trait]
impl AsyncRunnable for AsyncMixBatchTask {
    #[tracing::instrument(skip_all)]
    async fn run(&self, _queueable: &dyn AsyncQueueable) -> Result<(), FangError> {
        tracing::info!("Do AsyncMixBatchTask");

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

    // Default task_type is `common`, if task_type does not match, task will not be fetched!
    // fn task_type(&self) -> String {
    //     "mix-batch_task-type".to_string()
    // }

    // If `uniq` is set to true and the task is already in the storage, it won't be inserted again
    // The existing record will be returned for any insertions operaiton
    fn uniq(&self) -> bool {
        true
    }

    // Every 10 seconds
    fn cron(&self) -> Option<Scheduled> {
        // NB! Intervals less than worker's sleep time (default 10s) won't work!
        let cron_schedule = "0/10 * * * * *";
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

async fn mix_batch_inner(
    notes_batch: &[FullNote],
    account_id: &str,
    db: Arc<dyn NoteRepository>,
    client: &MixerClientSender,
) -> anyhow::Result<()> {
    // TODO: try to adopt use of &str in `mix_batch` trait method
    let note_ids: Vec<_> = notes_batch.iter().map(|note| note.note_id.to_string()).collect();

    let tx_id = match db
        .mix_batch(note_ids, account_id.to_string(), client)
        .await
        .with_context(|| "async mix_batch worker is executing batch")
    {
        Ok(tx_id) => tx_id,
        Err(error) => {
            tracing::error!("Error when trying to mix batch to {account_id} with {error:#?}");
            anyhow::bail!("mix_batch {error}");
        },
    };
    tracing::info!("Completed mix for account_id={account_id} with tx_id={tx_id}");
    Ok(())
}

// #[cfg(test)]
// mod test {
//     #[tokio::test]
//     async fn test_batch_grouping() {
//         // TODO: mock database records
//     }
// }
