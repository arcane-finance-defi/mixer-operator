use anyhow::Context as _;
use diesel::{Connection as _, PgConnection};
use fang::{
    AsyncQueue, AsyncQueueable as _, AsyncRunnable, asynk::async_worker_pool::AsyncWorkerPool,
    run_migrations_postgres,
};
use tokio::sync::OnceCell;

use crate::{mixer::MixerClientSender, task::AsyncMixBatchTask};

static MIXER_SENDER: OnceCell<MixerClientSender> = OnceCell::const_new();

fn do_migration(db_url: &str) -> anyhow::Result<()> {
    tracing::info!("Establishing connection to task queue database");
    let mut connection = PgConnection::establish(db_url)?;
    tracing::info!("Running migrations");
    run_migrations_postgres(&mut connection)
        .map_err(anyhow::Error::from_boxed)
        .with_context(|| "run migration error")?;
    tracing::info!("Migrations done");
    Ok(())
}

pub async fn prepare_task_queue(config: &crate::config::TaskQueue) -> anyhow::Result<AsyncQueue> {
    let mut queue: AsyncQueue = AsyncQueue::builder()
        // Postgres database url
        .uri(config.db_url.clone())
        // Max number of connections that are allowed
        .max_pool_size(config.db_max_pool.unwrap_or(3))
        .build();

    if !config.enabled {
        tracing::warn!("Task queue was disabled, you should NOT do it IN PRODUCTION environment!");
        return Ok(queue);
    }

    do_migration(&config.db_url)?;

    // Always connect first in order to perform any operation
    queue.connect().await?;
    tracing::info!("Queue connected to database");

    let mut pool: AsyncWorkerPool<AsyncQueue> = AsyncWorkerPool::builder()
        .number_of_workers(config.workers_max.unwrap_or(1 + 1))
        .queue(queue.clone())
        // .task_type("my_task_type") // use default `common` task type
        .build();

    tracing::info!("Pool created");

    let task = AsyncMixBatchTask::default();
    queue.schedule_task(&task as &dyn AsyncRunnable).await?;
    tracing::info!("Periodic tasks enqueued");
    // tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    // let task = queue.fetch_and_touch_task(None).await?;
    // tracing::warn!("Task - {task:?}");

    pool.start().await;
    tracing::info!("Workers started");

    Ok(queue)
}

pub fn prepare_shared_mixer_client(mixer_sender: MixerClientSender) -> anyhow::Result<()> {
    MIXER_SENDER.set(mixer_sender)?;
    tracing::info!("Shared reference to Miden client set");
    Ok(())
}

pub fn mixer_client_sender() -> anyhow::Result<&'static MixerClientSender> {
    MIXER_SENDER
        .get()
        .ok_or(anyhow::anyhow!("no mixer sender initialized in once cell, it's a bug!"))
}

// #[cfg(test)]
// mod tests {
//     use std::sync::atomic::{AtomicBool, Ordering};

//     use std::time::Duration;
//     use diesel::SqliteConnection;
//     use fang::{
//         async_trait,
//         run_migrations_sqlite,
//         Serialize, Deserialize,
//         AsyncQueueable, FangError, Scheduled
//     };
//     use super::*;
//     use crate::config::TaskQueue as TaskQueueConfig;

//     struct Fixture {
//         config: TaskQueueConfig
//     }

//     impl Default for Fixture {
//         fn default() -> Self {
//         Self {
//             config: TaskQueueConfig {
//                 enabled: true,
//                 db_url: "sqlite::memory:".to_string(),
//                 db_max_pool: Some(1),
//                 workers_max: Some(1),
//                 task_max_retry: Some(2),
//             }
//             }
//         }
//     }

//     impl Fixture {
//         fn config(&self) -> &TaskQueueConfig {
//             &self.config
//         }
//     }

//     #[derive(Default, Serialize, Deserialize)]
//     #[serde(crate = "fang::serde")]
//     pub struct AsyncTestTask {
//         completed: AtomicBool,
//     }

//     #[typetag::serde]
//     #[async_trait]
//     impl AsyncRunnable for AsyncTestTask {
//         async fn run(&self, _queueable: &dyn AsyncQueueable) -> Result<(), FangError> {
//             self.completed.store(true, Ordering::SeqCst);
//             Ok(())
//         }
//         fn cron(&self) -> Option<Scheduled> {
//             //                              s  m h md m wd y
//             let expression = "0/1 * * * * * *";
//             Some(Scheduled::CronPattern(expression.to_string()))
//         }
//         fn uniq(&self) -> bool {
//             true
//         }
//     }

//     #[tokio::test]
//     async fn test_worker_executing() {
//         let fixture = Fixture::default();
//         let config = fixture.config();

//         let mut queue: AsyncQueue = AsyncQueue::builder()
//             .uri(config.db_url.clone())
//             .max_pool_size(config.db_max_pool.expect("pool size"))
//             .build();

//         let mut connection = SqliteConnection::establish(&config.db_url).expect("sqlite conn");
//         run_migrations_sqlite(&mut connection).expect("run migrations");

//         queue.connect().await.expect("queue connect");

//         let mut pool: AsyncWorkerPool<AsyncQueue> = AsyncWorkerPool::builder()
//             .number_of_workers(config.workers_max.unwrap_or(1))
//             .queue(queue.clone())
//             // .task_type("my_task_type")
//             .build();

//         pool.start().await;

//         let task = AsyncTestTask {
//             completed: AtomicBool::new(false)
//         };
//         queue.schedule_task(&task as &dyn AsyncRunnable).await.expect("scheduled task");

//         tokio::time::sleep(Duration::from_millis(5000)).await;
//         assert_eq!(task.completed.load(Ordering::SeqCst), true);
//     }
// }
