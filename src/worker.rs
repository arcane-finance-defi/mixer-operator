use chrono::{DateTime, Utc};
use diesel::PgConnection;
use diesel::{Connection as _, connection};
use fang::{FangError, Scheduled};
use fang::serde::{Deserialize, Serialize};
use fang::{
    AsyncQueue, AsyncRunnable, async_trait,
    asynk::async_queue::AsyncQueueable,
    run_migrations_postgres,
};

#[derive(Serialize, Deserialize)]
#[serde(crate = "fang::serde")]
struct AsyncMixTask {
    pub task_id: String,
    pub scheduled_at: DateTime<Utc>,
}

#[typetag::serde]
#[async_trait]
impl AsyncRunnable for AsyncMixTask {
    async fn run(&self, _queueable: &dyn AsyncQueueable) -> Result<(), FangError> {
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
    // default value is None (the task is not scheduled, it's just executed as soon as it's inserted)     
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

fn do_migration(db_url: &str) -> anyhow::Result<()> {
    let mut connection = PgConnection::establish(db_url)?;
    tracing::info!("Running migrations");
    run_migrations_postgres(&mut connection)
        .map_err(|e| anyhow::anyhow!("run migration err {e}"))?;
    tracing::info!("Migrations done");
    Ok(())
}

pub async fn prepare_task_queue(config: &crate::config::TaskQueue) -> anyhow::Result<()> {
    do_migration(&config.db_url)?;

    let mut queue: AsyncQueue = AsyncQueue::builder()
        // Postgres database url
        .uri(config.db_url.clone())
        // Max number of connections that are allowed
        .max_pool_size(config.db_max_pool.unwrap_or(3))
        .build();

    // Always connect first in order to perform any operation
    queue.connect().await?;
    tracing::info!("Queue connected to database");

    let mut pool: AsyncWorkerPool<AsyncQueue> = AsyncWorkerPool::builder()
        .number_of_workers(config.workers_max.unwrap_or(1))
        .queue(queue.clone())
        .build();

    tracing::info!("Pool created");

    pool.start().await;
    tracing::info!("Workers started");

    Ok(())
}

///////////////////////////////////////////////////////////////////
use fang::asynk::async_worker_pool::AsyncWorkerPool;

// Need to create a queue
// Also insert some tasks

pub async fn spawn_workers() {}

// let mut pool: AsyncWorkerPool<AsyncQueue> = AsyncWorkerPool::builder()
//         .number_of_workers(max_pool_size)
//         .queue(queue.clone())
//         // if you want to run tasks of the specific kind
//         .task_type("my_task_type")
//         .build();

// pool.start().await;
