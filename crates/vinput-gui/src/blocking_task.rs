//! Plain-thread execution for blocking libraries that create their own runtimes.

use std::fmt;

use iced::Task;

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

/// Builds one Iced task around [`run`] while preserving typed worker failures.
pub(crate) fn perform<T, Message>(
    name: &'static str,
    operation: impl FnOnce() -> T + Send + 'static,
    mapper: impl FnOnce(Result<T, BlockingTaskFailure>) -> Message + Send + 'static,
) -> Task<Message>
where
    T: Send + 'static,
    Message: Send + 'static,
{
    Task::perform(run(name, operation), mapper)
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        time::Duration,
    };

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

    #[tokio::test(flavor = "multi_thread")]
    async fn blocking_http_client_runs_without_nesting_the_parent_runtime() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("loopback request");
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request).expect("read request");
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
                )
                .expect("write response");
        });
        let url = format!("http://{address}/fixture.json");
        let body = run("vinput-gui-blocking-http-test", move || {
            vinput_http::fetch_json_text(&url, Duration::from_secs(2))
        })
        .await
        .expect("plain worker result")
        .expect("HTTP fixture result");
        server.join().expect("loopback server");
        assert_eq!(body, "{\"ok\":true}");
    }
}
