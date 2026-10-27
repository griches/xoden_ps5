//! In-process notifications. Deliberately excludes URLs, paths and credentials.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadProgress {
    pub sequence: u64,
    pub phase: &'static str,
    pub paused: bool,
    pub completed_bytes: u64,
    pub total_bytes: Option<u64>,
    pub bytes_per_second: u64,
}

/// Callbacks run on the engine/reporter workers. Implementations must return
/// promptly and must not panic. Native hosts should copy into their UI mailbox.
pub trait JobObserver: Send + Sync {
    fn phase(&self, _phase: &str) {}
    fn diagnostic(&self, _code: &'static str) {}
    fn failure_code(&self) -> Option<&'static str> { None }
    fn progress(&self, _progress: DownloadProgress) {}
}
pub type Observer = std::sync::Arc<dyn JobObserver>;
