use anyhow::Context;
use chrono::{DateTime, Utc};
use diesel::{Connection as _, PgConnection, connection};
use fang::{
    AsyncQueue, AsyncRunnable, FangError, Scheduled, async_trait,
    asynk::async_queue::AsyncQueueable,
    run_migrations_postgres,
    serde::{Deserialize, Serialize},
};

use crate::db::{models::NoteRepository as _, DatabaseStorage};

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
        let db = DatabaseStorage::storage().await?;

        // task_id is effectively request_id in the storage
        let detailed_note = db.get_note_by_request_id(self.task_id).await?;

        tracing::info!("abcd");
        let FullNote { note_id, note, account_id, .. } = note_record;

            // TODO: should be unified methods to store and load serialized notes without client
            let note_bytes =
                hex::decode(note).context("decoding from hex string note {note_id}")?;
            let note = Note::read_from_bytes(note_bytes.as_slice())
                .context("reading note from bytes for {note_id}")?;

            let faucet_id = AccountId::from_hex(&account_id)?;

            join_set.spawn(mix(self.client.clone(), note, faucet_id));

        // println!("the number is {}", self.number);

        // my_func(self.number)?; // TODO: mix
        // You can use ? operator because
        // From<FangError> is implemented thanks to ToFangError derive macro.

        Ok(())
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
