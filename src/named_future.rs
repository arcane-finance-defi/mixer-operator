use futures::{Future, FutureExt, future::BoxFuture};
use tracing::Instrument;

pub type NamedJoinHandle = BoxFuture<'static, (String, anyhow::Result<()>)>;

// #[tracing::instrument(skip(fut))] // TODO
pub fn spawn_named<F>(name: String, fut: F) -> NamedJoinHandle
where
    F: Future<Output = anyhow::Result<()>> + Send + 'static,
{
    let join_handle = tokio::spawn(fut.in_current_span());
    async move {
        let result = match join_handle.await {
            Ok(result) => result,
            Err(error) => {
                if let Ok(panic) = error.try_into_panic() {
                    if let Some(str) = panic.downcast_ref::<String>() {
                        Err(anyhow::anyhow!("task {name} has panicked: {str}"))
                    } else if let Some(str) = panic.downcast_ref::<&str>() {
                        Err(anyhow::anyhow!("task {name} has panicked: {str}"))
                    } else {
                        Err(anyhow::anyhow!("task {name} has panicked"))
                    }
                } else {
                    Err(anyhow::anyhow!("task {name} was cancelled"))
                }
            }
        };
        (name, result)
    }
    .boxed()
}

pub fn spawn_blocking_named<F>(name: String, fun: F) -> NamedJoinHandle
where
    F: FnOnce() -> anyhow::Result<()> + Send + 'static,
{
    let join_handle = tokio::task::spawn_blocking(fun);
    async move {
        let result = match join_handle.await {
            Ok(result) => result,
            Err(error) => {
                if let Ok(panic) = error.try_into_panic() {
                    if let Some(str) = panic.downcast_ref::<String>() {
                        Err(anyhow::anyhow!("task {name} has panicked: {str}"))
                    } else if let Some(str) = panic.downcast_ref::<&str>() {
                        Err(anyhow::anyhow!("task {name} has panicked: {str}"))
                    } else {
                        Err(anyhow::anyhow!("task {name} has panicked"))
                    }
                } else {
                    Err(anyhow::anyhow!("task {name} was cancelled"))
                }
            }
        };
        (name, result)
    }
    .boxed()
}
