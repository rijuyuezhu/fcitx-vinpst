//! Shared HTTP client construction and safe transport diagnostics.

use std::{env, fs, io::Read, path::Path};

/// Environment variable containing an additional PEM certificate bundle.
pub const SSL_CERT_FILE_ENV: &str = "SSL_CERT_FILE";

const MAX_EXTRA_CA_BUNDLE_BYTES: u64 = 4 * 1024 * 1024;

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

/// Builds the blocking provider client using the process trust environment.
///
/// Reqwest's built-in `WebPKI` roots remain enabled. When `SSL_CERT_FILE` names a
/// non-empty PEM bundle, every certificate in that bundle is added as an extra
/// trust root. Certificate verification is never disabled.
pub fn blocking_client_from_environment() -> Result<reqwest::blocking::Client, HttpClientError> {
    let certificate_path = env::var_os(SSL_CERT_FILE_ENV)
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from);
    blocking_client_with_extra_ca_path(certificate_path.as_deref())
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

fn blocking_client_with_extra_ca_path(
    certificate_path: Option<&Path>,
) -> Result<reqwest::blocking::Client, HttpClientError> {
    let mut builder = reqwest::blocking::Client::builder();
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
    use std::{fs, io::Write};

    use tempfile::tempdir;

    use super::{HttpClientError, MAX_EXTRA_CA_BUNDLE_BYTES, blocking_client_with_extra_ca_path};

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
}
