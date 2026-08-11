//! Unavailable ASR backend used while a configured backend cannot be loaded.

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionSession,
};

/// Backend placeholder that preserves daemon availability without fabricating recognition.
#[derive(Clone)]
pub struct UnavailableAsrBackend {
    error: String,
}

impl std::fmt::Debug for UnavailableAsrBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("UnavailableAsrBackend")
            .field("error", &"<redacted>")
            .finish()
    }
}

impl UnavailableAsrBackend {
    /// Creates an unavailable backend with a stable diagnostic error.
    #[must_use]
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

impl AsrBackend for UnavailableAsrBackend {
    fn describe(&self) -> BackendDescriptor {
        BackendDescriptor::new("", "", "Unavailable ASR", BackendCapabilities::buffered())
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Err(AsrError::Backend(self.error.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_preserved_failure_reason() {
        let secret = "backend-construction-private-detail";
        let backend = UnavailableAsrBackend::new(secret);
        let debug = format!("{backend:?}");
        assert!(debug.contains("UnavailableAsrBackend"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains(secret));

        let error = backend
            .create_session(RecognitionContext::normal("default", None))
            .err()
            .expect("unavailable backend must reject session creation");
        assert!(error.to_string().contains(secret));
    }
}
