//! Shared diagnostic redaction helpers for configured URLs.

const REDACTED_QUERY_VALUE: &str = "REDACTED";

/// Removes URL credentials, fragments, and query values from diagnostic output.
///
/// The returned URL keeps its scheme, host, port, path, query keys, duplicate
/// query-key ordering, and whether the input explicitly contained a path. The
/// original URL remains unchanged for requests. Invalid URLs are replaced by a
/// fixed marker so malformed input cannot leak through parser diagnostics.
#[must_use]
pub fn redact_url_for_diagnostics(value: &str) -> String {
    let Ok(mut url) = url::Url::parse(value) else {
        return "<invalid-url>".to_owned();
    };
    let explicit_path = has_explicit_path(value);
    let query_keys = url
        .query_pairs()
        .map(|(key, _)| key.into_owned())
        .collect::<Vec<_>>();

    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_fragment(None);
    url.set_query(None);
    if !query_keys.is_empty() {
        let mut query = url.query_pairs_mut();
        for key in query_keys {
            query.append_pair(&key, REDACTED_QUERY_VALUE);
        }
    }

    let mut output = url.to_string();
    if !explicit_path && url.path() == "/" {
        if let Some(query_index) = output.find("/?") {
            output.remove(query_index);
        } else if output.ends_with('/') {
            output.pop();
        }
    }
    output
}

fn has_explicit_path(value: &str) -> bool {
    value.split_once("://").is_some_and(|(_, remainder)| {
        remainder
            .split(['?', '#'])
            .next()
            .is_some_and(|authority_and_path| authority_and_path.contains('/'))
    })
}

#[cfg(test)]
mod tests {
    use super::redact_url_for_diagnostics;

    #[test]
    fn redacts_url_secrets_without_changing_request_shape() {
        assert_eq!(
            redact_url_for_diagnostics(
                "https://user:password@example.test/v1?api-version=2026-01-01&key=secret#token"
            ),
            "https://example.test/v1?api-version=REDACTED&key=REDACTED"
        );
        assert_eq!(
            redact_url_for_diagnostics("https://example.test?key=secret&key=second"),
            "https://example.test?key=REDACTED&key=REDACTED"
        );
        assert_eq!(
            redact_url_for_diagnostics("https://example.test"),
            "https://example.test"
        );
        assert_eq!(redact_url_for_diagnostics("not a url"), "<invalid-url>");
    }
}
