use anyhow::Context;
use diesel::{Connection as _, PgConnection};
use fang::{AsyncQueue, asynk::async_worker_pool::AsyncWorkerPool, run_migrations_postgres};
use tokio::sync::OnceCell;

use crate::mixer::MixerClientSender;

static MIXER_SENDER: OnceCell<MixerClientSender> = OnceCell::const_new();

fn do_migration(db_url: &str) -> anyhow::Result<()> {
    let mut connection = PgConnection::establish(db_url)?;
    tracing::info!("Running migrations");
    run_migrations_postgres(&mut connection)
        .map_err(anyhow::Error::from_boxed)
        .with_context(|| "run migration error")?;
    tracing::info!("Migrations done");
    Ok(())
}

pub async fn prepare_task_queue(
    config: &crate::config::TaskQueue,
    mixer_sender: MixerClientSender,
) -> anyhow::Result<AsyncQueue> {
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
        // .task_type("my_task_type")
        .build();

    tracing::info!("Pool created");

    pool.start().await;
    tracing::info!("Workers started");

    MIXER_SENDER.set(mixer_sender)?;

    Ok(queue)
}

pub fn mixer_client_sender() -> anyhow::Result<&'static MixerClientSender> {
    Ok(MIXER_SENDER
        .get()
        .ok_or(anyhow::anyhow!("no mixer sender initialized in once cell, it's a bug!"))?)
}
