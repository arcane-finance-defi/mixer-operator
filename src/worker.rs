use diesel::PgConnection;
// #[derive(Serialize, Deserialize)]
// #[serde(crate = "fang::serde")]
// struct AsyncTask {
//     pub number: u16,
// }

// #[typetag::serde]
// #[async_trait]
// impl AsyncRunnable for AsyncTask {
//     async fn run(&self, _queueable: &mut dyn AsyncQueueable) -> Result<(), Error> {
//         Ok(())
//     }
//     // this func is optional
//     // Default task_type is common
//     fn task_type(&self) -> String {
//         "my-task-type".to_string()
//     }

//     // If `uniq` is set to true and the task is already in the storage, it won't be inserted
// again     // The existing record will be returned for for any insertions operaiton
//     fn uniq(&self) -> bool {
//         true
//     }

//     // This will be useful if you would like to schedule tasks.
//     // default value is None (the task is not scheduled, it's just executed as soon as it's
// inserted)     fn cron(&self) -> Option<Scheduled> {
//         let expression = "0/20 * * * Aug-Sep * 2022/1";
//         Some(Scheduled::CronPattern(expression.to_string()))
//     }

//     // the maximum number of retries. Set it to 0 to make it not retriable
//     // the default value is 20
//     fn max_retries(&self) -> i32 {
//         20
//     }

//     // backoff mode for retries
//     fn backoff(&self, attempt: u32) -> u32 {
//         u32::pow(2, attempt)
//     }
// }
use diesel::{Connection as _, connection};
use fang::{
    AsyncQueue, AsyncRunnable, async_trait,
    asynk::async_queue::AsyncQueueable,
    run_migrations_postgres,
    serde::{Deserialize, Serialize},
};

fn do_migration(db_url: &str) -> anyhow::Result<()> {
    let mut connection = PgConnection::establish(db_url)?;
    tracing::info!("Running migrations");
    run_migrations_postgres(&mut connection)
        .map_err(|e| anyhow::anyhow!("run migration err {e}"))?;
    tracing::info!("Migrations done");
    Ok(())
}

pub async fn prepare_task_queue(config: crate::config::TaskQueue) -> anyhow::Result<()> {
    do_migration(&config.db_url);

    let mut queue = AsyncQueue::builder()
        // Postgres database url
        .uri(config.db_url)
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
