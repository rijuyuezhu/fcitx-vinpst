//! Shared HTTP client construction and safe transport diagnostics.

use std::{
    env, fs,
    io::{self, Read},
    path::Path,
    time::Duration,
};

/// Environment variable containing an additional PEM certificate bundle.
pub const SSL_CERT_FILE_ENV: &str = "SSL_CERT_FILE";

const MAX_EXTRA_CA_BUNDLE_BYTES: u64 = 4 * 1024 * 1024;

/// Maximum response-body size accepted from ASR and text providers.
pub const MAX_PROVIDER_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Errors produced while reading a bounded provider response body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResponseBodyError {
    /// The response body stalled beyond the request deadline.
    #[error("response body timed out")]
    TimedOut,
    /// The response stream failed for a non-timeout reason.
    #[error("response body read failed")]
    Read,
    /// The response body exceeded the configured safety limit.
    #[error("response body exceeds the supported size limit")]
    TooLarge,
    /// The response body was not valid UTF-8 text.
    #[error("response body is not valid UTF-8")]
    InvalidUtf8,
}

/// Errors produced while constructing the shared provider HTTP client.
///
/// Messages intentionally omit certificate paths, certificate contents, and
/// lower-level parser errors so provider diagnostics cannot disclose local
/// trust-store details.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HttpClientError {
    /// The configured certificate path could not be inspected.
    #[error("configured SSL_CERT_FILE could not be inspected")]
    CertificateFileInspection,
    /// The configured certificate path does not resolve to a regular file.
    #[error("configured SSL_CERT_FILE is not a regular file")]
    CertificateFileNotRegular,
    /// The configured certificate bundle exceeds the bounded read limit.
    #[error("configured SSL_CERT_FILE exceeds the supported size limit")]
    CertificateFileTooLarge,
    /// The configured certificate bundle could not be read.
    #[error("configured SSL_CERT_FILE could not be read")]
    CertificateFileRead,
    /// The configured certificate bundle is empty or invalid.
    #[error("configured SSL_CERT_FILE is not a valid PEM certificate bundle")]
    CertificateBundleInvalid,
    /// Reqwest rejected the final client configuration.
    #[error("failed to build the provider HTTP client")]
    ClientBuild,
}

/// Errors produced while fetching bounded JSON text over the shared client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JsonTextFetchError {
    /// The shared client could not be built from the trust environment.
    #[error("HTTP client setup failed")]
    Client(#[from] HttpClientError),
    /// The request exceeded its configured deadline.
    #[error("HTTP request timed out")]
    TimedOut,
    /// The remote endpoint could not be reached.
    #[error("HTTP connection failed")]
    ConnectionFailed,
    /// The request failed for another transport reason.
    #[error("HTTP request failed")]
    RequestFailed,
    /// The endpoint returned a non-success status, including redirects.
    #[error("HTTP endpoint returned status {0}")]
    Status(u16),
    /// The bounded response body could not be read as UTF-8.
    #[error("HTTP response body failed: {0}")]
    Body(#[from] ResponseBodyError),
}

/// Builds the blocking provider client using the process trust environment.
///
/// Reqwest's built-in `WebPKI` roots remain enabled. When `SSL_CERT_FILE` names a
/// non-empty PEM bundle, every certificate in that bundle is added as an extra
/// trust root. Certificate verification is never disabled. Redirects are also
/// disabled so provider credentials and POST bodies remain bound to the
/// configured endpoint.
pub fn blocking_client_from_environment() -> Result<reqwest::blocking::Client, HttpClientError> {
    let certificate_path = env::var_os(SSL_CERT_FILE_ENV)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    blocking_client_with_extra_ca_path(certificate_path.as_deref())
}

/// Builds the blocking provider client with an explicit TCP connect deadline.
///
/// Per-request timeouts still bound the complete exchange; this limit only
/// prevents a connect attempt from consuming an otherwise long deadline.
pub fn blocking_client_from_environment_with_connect_timeout(
    connect_timeout: Duration,
) -> Result<reqwest::blocking::Client, HttpClientError> {
    let certificate_path = env::var_os(SSL_CERT_FILE_ENV)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    blocking_client_with_extra_ca_path_and_connect_timeout(
        certificate_path.as_deref(),
        Some(connect_timeout),
    )
}

/// Fetches one bounded JSON text document with redirects disabled.
///
/// Diagnostics intentionally omit the request URL and response body. The shared
/// trust environment and built-in roots remain active.
pub fn fetch_json_text(url: &str, timeout: Duration) -> Result<String, JsonTextFetchError> {
    let client = blocking_client_from_environment()?;
    fetch_json_text_with_client(&client, url, timeout)
}

fn fetch_json_text_with_client(
    client: &reqwest::blocking::Client,
    url: &str,
    timeout: Duration,
) -> Result<String, JsonTextFetchError> {
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(timeout)
        .send()
        .map_err(|error| {
            if error.is_timeout() {
                JsonTextFetchError::TimedOut
            } else if error.is_connect() {
                JsonTextFetchError::ConnectionFailed
            } else {
                JsonTextFetchError::RequestFailed
            }
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(JsonTextFetchError::Status(status.as_u16()));
    }
    read_provider_response_text(response).map_err(JsonTextFetchError::from)
}

/// Returns a stable, URL-free category for a reqwest transport error.
#[must_use]
pub fn reqwest_error_category(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "timed out"
    } else if error.is_connect() {
        "connection failed"
    } else if error.is_redirect() {
        "redirect failed"
    } else if error.is_body() {
        "request or response body failed"
    } else if error.is_decode() {
        "response decode failed"
    } else if error.is_builder() {
        "request build failed"
    } else if error.is_status() {
        "HTTP status failed"
    } else if error.is_request() {
        "request failed"
    } else {
        "transport failed"
    }
}

/// Reads one provider response as UTF-8 while bounding the body size.
///
/// The limit applies to the bytes exposed by reqwest's response reader. Error
/// messages intentionally omit response contents and URLs.
pub fn read_provider_response_text(
    response: reqwest::blocking::Response,
) -> Result<String, ResponseBodyError> {
    read_utf8_bounded(response, MAX_PROVIDER_RESPONSE_BYTES)
}

fn read_utf8_bounded(reader: impl Read, max_bytes: u64) -> Result<String, ResponseBodyError> {
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or(ResponseBodyError::TooLarge)?;
    let mut bytes = Vec::new();
    reader
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            if io_error_is_timeout(&error) {
                ResponseBodyError::TimedOut
            } else {
                ResponseBodyError::Read
            }
        })?;
    let body_len = u64::try_from(bytes.len()).map_err(|_| ResponseBodyError::TooLarge)?;
    if body_len > max_bytes {
        return Err(ResponseBodyError::TooLarge);
    }
    String::from_utf8(bytes).map_err(|_| ResponseBodyError::InvalidUtf8)
}

fn io_error_is_timeout(error: &io::Error) -> bool {
    if matches!(
        error.kind(),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
    ) {
        return true;
    }
    let Some(cause) = error.get_ref() else {
        return false;
    };
    if error_cause_is_timeout(cause) {
        return true;
    }
    let mut source = cause.source();
    while let Some(cause) = source {
        if error_cause_is_timeout(cause) {
            return true;
        }
        source = cause.source();
    }
    false
}

fn error_cause_is_timeout(cause: &(dyn std::error::Error + 'static)) -> bool {
    cause.downcast_ref::<io::Error>().is_some_and(|error| {
        matches!(
            error.kind(),
            io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock
        )
    }) || cause
        .downcast_ref::<reqwest::Error>()
        .is_some_and(reqwest::Error::is_timeout)
}

fn blocking_client_with_extra_ca_path(
    certificate_path: Option<&Path>,
) -> Result<reqwest::blocking::Client, HttpClientError> {
    blocking_client_with_extra_ca_path_and_connect_timeout(certificate_path, None)
}

fn blocking_client_with_extra_ca_path_and_connect_timeout(
    certificate_path: Option<&Path>,
    connect_timeout: Option<Duration>,
) -> Result<reqwest::blocking::Client, HttpClientError> {
    let mut builder =
        reqwest::blocking::Client::builder().redirect(reqwest::redirect::Policy::none());
    if let Some(connect_timeout) = connect_timeout {
        builder = builder.connect_timeout(connect_timeout);
    }
    if let Some(certificate_path) = certificate_path {
        let certificate_file = fs::File::open(certificate_path)
            .map_err(|_| HttpClientError::CertificateFileInspection)?;
        let metadata = certificate_file
            .metadata()
            .map_err(|_| HttpClientError::CertificateFileInspection)?;
        if !metadata.is_file() {
            return Err(HttpClientError::CertificateFileNotRegular);
        }
        if metadata.len() > MAX_EXTRA_CA_BUNDLE_BYTES {
            return Err(HttpClientError::CertificateFileTooLarge);
        }
        let capacity = usize::try_from(metadata.len())
            .map_err(|_| HttpClientError::CertificateFileTooLarge)?;
        let mut pem_bundle = Vec::with_capacity(capacity);
        certificate_file
            .take(MAX_EXTRA_CA_BUNDLE_BYTES + 1)
            .read_to_end(&mut pem_bundle)
            .map_err(|_| HttpClientError::CertificateFileRead)?;
        if pem_bundle.len() as u64 > MAX_EXTRA_CA_BUNDLE_BYTES {
            return Err(HttpClientError::CertificateFileTooLarge);
        }
        let certificates = reqwest::Certificate::from_pem_bundle(&pem_bundle)
            .map_err(|_| HttpClientError::CertificateBundleInvalid)?;
        if certificates.is_empty() {
            return Err(HttpClientError::CertificateBundleInvalid);
        }
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    builder.build().map_err(|_| HttpClientError::ClientBuild)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::{self, Cursor, Read, Write},
        net::TcpListener,
        thread,
        time::Duration,
    };

    use tempfile::tempdir;

    use super::{
        HttpClientError, JsonTextFetchError, MAX_EXTRA_CA_BUNDLE_BYTES, ResponseBodyError,
        blocking_client_with_extra_ca_path, fetch_json_text_with_client, read_utf8_bounded,
    };

    fn serve_once(response: Vec<u8>) -> String {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind HTTP fixture");
        let address = listener.local_addr().expect("HTTP fixture address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept HTTP fixture");
            let mut request = [0_u8; 2048];
            let _ = stream.read(&mut request);
            stream.write_all(&response).expect("write HTTP fixture");
        });
        format!("http://{address}/notification.json")
    }

    #[test]
    fn bounded_json_fetch_accepts_success_and_rejects_redirect_status() {
        let client = blocking_client_with_extra_ca_path(None).expect("HTTP fixture client");
        let body = br#"{"id":"notice"}"#;
        let response = format!(
            "HTTP/1.1 200 OK
Content-Length: {}
Connection: close

",
            body.len()
        )
        .into_bytes()
        .into_iter()
        .chain(body.iter().copied())
        .collect();
        let url = serve_once(response);
        assert_eq!(
            fetch_json_text_with_client(&client, &url, Duration::from_secs(1))
                .expect("fetch JSON fixture"),
            r#"{"id":"notice"}"#
        );

        let redirect = serve_once(
            b"HTTP/1.1 302 Found
Location: http://127.0.0.1:9/blocked
Content-Length: 0
Connection: close

"
            .to_vec(),
        );
        assert_eq!(
            fetch_json_text_with_client(&client, &redirect, Duration::from_secs(1))
                .expect_err("redirect must remain a non-success status"),
            JsonTextFetchError::Status(302)
        );
    }

    struct FailingReader {
        kind: io::ErrorKind,
    }

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(self.kind, "fixture reader failure"))
        }
    }

    struct NestedTimeoutReader;

    impl Read for NestedTimeoutReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other(io::Error::new(
                io::ErrorKind::TimedOut,
                "nested fixture timeout",
            )))
        }
    }

    #[test]
    fn default_client_keeps_builtin_trust_configuration() {
        blocking_client_with_extra_ca_path(None).expect("build default provider client");
    }

    #[test]
    fn extra_ca_path_errors_do_not_disclose_paths_or_contents() {
        let directory = tempdir().unwrap();
        let missing = directory.path().join("missing-secret-name.pem");
        let error = blocking_client_with_extra_ca_path(Some(&missing)).unwrap_err();
        assert_eq!(error, HttpClientError::CertificateFileInspection);
        assert!(!error.to_string().contains("missing-secret-name"));

        let error = blocking_client_with_extra_ca_path(Some(directory.path())).unwrap_err();
        assert_eq!(error, HttpClientError::CertificateFileNotRegular);
        assert!(
            !error
                .to_string()
                .contains(&directory.path().display().to_string())
        );

        let invalid = directory.path().join("invalid-secret-name.pem");
        fs::write(&invalid, b"private fixture contents").unwrap();
        let error = blocking_client_with_extra_ca_path(Some(&invalid)).unwrap_err();
        assert_eq!(error, HttpClientError::CertificateBundleInvalid);
        assert!(!error.to_string().contains("private fixture contents"));
        assert!(!error.to_string().contains("invalid-secret-name"));
    }

    #[test]
    fn extra_ca_bundle_read_is_bounded() {
        let directory = tempdir().unwrap();
        let oversized = directory.path().join("oversized.pem");
        let mut file = fs::File::create(&oversized).unwrap();
        file.write_all(b"x").unwrap();
        file.set_len(MAX_EXTRA_CA_BUNDLE_BYTES + 1).unwrap();

        assert_eq!(
            blocking_client_with_extra_ca_path(Some(&oversized)).unwrap_err(),
            HttpClientError::CertificateFileTooLarge
        );
    }

    #[test]
    fn provider_response_body_accepts_the_limit_and_rejects_the_next_byte() {
        assert_eq!(read_utf8_bounded(Cursor::new(b"four"), 4).unwrap(), "four");
        assert_eq!(
            read_utf8_bounded(Cursor::new(b"five!"), 4).unwrap_err(),
            ResponseBodyError::TooLarge
        );
    }

    #[test]
    fn provider_response_body_rejects_invalid_utf8() {
        assert_eq!(
            read_utf8_bounded(Cursor::new([0xff]), 4).unwrap_err(),
            ResponseBodyError::InvalidUtf8
        );
    }

    #[test]
    fn provider_response_body_classifies_timeout_and_read_failures() {
        assert_eq!(
            read_utf8_bounded(
                FailingReader {
                    kind: io::ErrorKind::TimedOut,
                },
                4,
            )
            .unwrap_err(),
            ResponseBodyError::TimedOut
        );
        assert_eq!(
            read_utf8_bounded(NestedTimeoutReader, 4).unwrap_err(),
            ResponseBodyError::TimedOut
        );
        assert_eq!(
            read_utf8_bounded(
                FailingReader {
                    kind: io::ErrorKind::ConnectionReset,
                },
                4,
            )
            .unwrap_err(),
            ResponseBodyError::Read
        );
    }
}
