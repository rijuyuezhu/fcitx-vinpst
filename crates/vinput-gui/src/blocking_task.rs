//! Plain-thread execution for blocking libraries that create their own runtimes.

use std::fmt;

/// Failure to start or receive a result from a plain blocking worker thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlockingTaskFailure {
    /// The operating system refused to create the worker thread.
    Spawn,
    /// The worker stopped before sending its result, including panic termination.
    Stopped,
}

impl fmt::Display for BlockingTaskFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Spawn => "blocking worker could not start",
            Self::Stopped => "blocking worker stopped unexpectedly",
        })
    }
}

/// Runs one blocking operation outside the Tokio runtime context.
///
/// `tokio::task::spawn_blocking` still enters the Tokio runtime context. That is
/// incompatible with blocking libraries such as `reqwest::blocking` and
/// `zbus::blocking`, which may create and drive an internal runtime. A dedicated
/// standard thread plus a Tokio oneshot keeps the UI asynchronous without
/// nesting runtimes.
pub(crate) async fn run<T>(
    name: &'static str,
    operation: impl FnOnce() -> T + Send + 'static,
) -> Result<T, BlockingTaskFailure>
where
    T: Send + 'static,
{
    let (sender, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name(name.to_owned())
        .spawn(move || {
            let _ = sender.send(operation());
        })
        .map_err(|_| BlockingTaskFailure::Spawn)?;
    receiver.await.map_err(|_| BlockingTaskFailure::Stopped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn worker_does_not_inherit_the_tokio_runtime_context() {
        let outside_runtime = run("vinput-gui-runtime-boundary-test", || {
            tokio::runtime::Handle::try_current().is_err()
        })
        .await
        .expect("plain worker result");
        assert!(outside_runtime);
    }
}
