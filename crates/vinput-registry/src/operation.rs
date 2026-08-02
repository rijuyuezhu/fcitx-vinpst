//! Cooperative progress and cancellation for long-running registry operations.

use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use thiserror::Error;

/// Progress emitted by a long-running registry operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryOperationProgress {
    /// Preparing validated paths and metadata.
    Preparing,
    /// Resolving the live registry catalog before an install.
    ResolvingRegistry,
    /// Downloading one registry asset.
    Downloading {
        /// Bytes written to the temporary asset file.
        downloaded_bytes: u64,
        /// Expected bytes from registry metadata or the HTTP response, when known.
        total_bytes: Option<u64>,
    },
    /// Verifying the downloaded asset checksum policy.
    VerifyingChecksum,
    /// Extracting a verified archive into a temporary tree.
    Extracting {
        /// Archive entries processed so far.
        processed_entries: u64,
        /// Regular-file bytes extracted so far.
        extracted_bytes: u64,
    },
    /// Writing validated runtime metadata into the staged tree.
    WritingMetadata,
    /// Atomically publishing the prepared tree into managed storage.
    Publishing,
    /// The operation completed successfully.
    Completed,
}

/// Cooperative cancellation marker for a registry operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("registry operation cancelled")]
pub struct RegistryOperationCancelled;

type ProgressReporter = dyn Fn(RegistryOperationProgress) + Send + Sync + 'static;

/// Shared cooperative control for one long-running registry operation.
#[derive(Clone)]
pub struct RegistryOperationControl {
    cancelled: Arc<AtomicBool>,
    reporter: Arc<ProgressReporter>,
}

impl RegistryOperationControl {
    /// Creates a control with a typed progress reporter.
    pub fn new(reporter: impl Fn(RegistryOperationProgress) + Send + Sync + 'static) -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            reporter: Arc::new(reporter),
        }
    }

    /// Requests cooperative cancellation.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    /// Fails when cancellation has been requested.
    pub fn check_cancelled(&self) -> Result<(), RegistryOperationCancelled> {
        if self.is_cancelled() {
            Err(RegistryOperationCancelled)
        } else {
            Ok(())
        }
    }

    /// Emits typed progress unless cancellation has already been requested.
    pub fn report(&self, progress: RegistryOperationProgress) {
        if !self.is_cancelled() {
            (self.reporter)(progress);
        }
    }
}

impl Default for RegistryOperationControl {
    fn default() -> Self {
        Self::new(|_| {})
    }
}

impl fmt::Debug for RegistryOperationControl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RegistryOperationControl")
            .field("cancelled", &self.is_cancelled())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[test]
    fn control_reports_progress_and_cancels_cooperatively() {
        let progress = Arc::new(Mutex::new(Vec::new()));
        let recorded = Arc::clone(&progress);
        let control = RegistryOperationControl::new(move |event| {
            recorded.lock().expect("progress lock").push(event);
        });

        control.report(RegistryOperationProgress::Preparing);
        assert_eq!(
            *progress.lock().expect("progress lock"),
            vec![RegistryOperationProgress::Preparing]
        );
        assert!(control.check_cancelled().is_ok());

        control.cancel();
        assert!(control.is_cancelled());
        assert_eq!(control.check_cancelled(), Err(RegistryOperationCancelled));
        control.report(RegistryOperationProgress::Completed);
        assert_eq!(progress.lock().expect("progress lock").len(), 1);
    }
}
