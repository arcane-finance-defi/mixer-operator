use anyhow::anyhow;
use futures::{Future, FutureExt, future::BoxFuture};
use tracing::Instrument;

pub type TaskHandle = BoxFuture<'static, (String, anyhow::Result<()>)>;

fn into_task_handle(
    name: String,
    join_handle: tokio::task::JoinHandle<anyhow::Result<()>>,
) -> TaskHandle {
    async move {
        let result = match join_handle.await {
            Ok(result) => result,
            Err(error) => Err(if let Ok(panic) = error.try_into_panic() {
                if let Some(msg) = panic.downcast_ref::<String>() {
                    anyhow!("tokio task {name} panicked: {msg}")
                } else if let Some(msg) = panic.downcast_ref::<&str>() {
                    anyhow!("tokio task {name} panicked: {msg}")
                } else {
                    anyhow!("tokio task {name} panicked")
                }
            } else {
                anyhow!("tokio task {name} cancelled")
            }),
        };
        (name, result)
    }
    .boxed()
}

pub fn named<F>(name: impl Into<String>, fut: F) -> TaskHandle
where
    F: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let name = name.into();
    let join_handle = tokio::spawn(fut.in_current_span());
    into_task_handle(name, join_handle)
}

pub fn named_blocking<F>(name: impl Into<String>, fun: F) -> TaskHandle
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    let name = name.into();
    let join_handle = tokio::task::spawn_blocking(fun);
    into_task_handle(name, join_handle)
}
