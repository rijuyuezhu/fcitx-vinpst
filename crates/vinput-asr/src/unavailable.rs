//! Unavailable ASR backend used while a configured backend cannot be loaded.

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionSession,
};

/// Backend placeholder that preserves daemon availability without fabricating recognition.
#[derive(Debug, Clone)]
pub struct UnavailableAsrBackend {
    error: String,
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
