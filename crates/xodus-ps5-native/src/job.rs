//! Dedicated native job worker. Terminal snapshots mean all runtime IO has
//! drained and the on-disk job lease has been released. Cancellation preserves
//! partial files; the host may offer its managed cleanup only after terminal.
use std::{future::Future, path::PathBuf, process::ExitCode, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}};
use tokio::{runtime::{Builder, Runtime}, sync::watch};
use xodus_cli::{Client, DownloadProgress, JobLease, JobObserver, TokenManager};

static ACTIVE: AtomicBool = AtomicBool::new(false);
struct Active;
impl Drop for Active { fn drop(&mut self) { ACTIVE.store(false, Ordering::Release); } }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State { Starting, Running, Stopping, Complete, Failed, Cancelled }
impl State {
    pub fn terminal(self) -> bool { matches!(self, Self::Complete | Self::Failed | Self::Cancelled) }
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state: State,
    pub phase: String,
    pub progress: Option<DownloadProgress>,
    pub error: Option<&'static str>,
}
struct Mailbox(Mutex<Snapshot>);
impl Mailbox {
    fn update(&self, f: impl FnOnce(&mut Snapshot)) {
        f(&mut self.0.lock().unwrap_or_else(|p| p.into_inner()));
    }
}
impl JobObserver for Mailbox {
    fn phase(&self, phase: &str) { self.update(|s| { s.phase = phase.into(); s.progress = None; }); }
    fn progress(&self, progress: DownloadProgress) { self.update(|s| s.progress = Some(progress)); }
}

/// No callbacks enter the UI. Poll copies the latest bounded state. Dropping a
/// handle requests cancellation, but does NOT mean cleanup is safe: retain and
/// poll the handle until terminal before deleting files or unloading the library.
pub struct Job {
    mailbox: Arc<Mailbox>,
    cancel: watch::Sender<bool>,
}
impl Job {
    /// Roots are supplied by the host's approved storage policy, never a request.
    /// Setup executes on the worker and may initialize its private credential
    /// backend and verified HTTP client. Only one native job may exist at once.
    pub fn start(
        root: PathBuf, games: PathBuf,
        setup: impl FnOnce() -> Result<(Client, TokenManager), &'static str> + Send + 'static,
    ) -> Result<Self, &'static str> {
        ACTIVE.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map_err(|_| "job-busy")?;
        let active = Active;
        let mailbox = Arc::new(Mailbox(Mutex::new(Snapshot { state: State::Starting, phase: "starting".into(), progress: None, error: None })));
        let (cancel, receiver) = watch::channel(false);
        let sink = mailbox.clone();
        std::thread::Builder::new().name("xodus-job".into()).spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker(root, games, setup, receiver, &sink)))
                .unwrap_or(Err("job-panicked"));
            // worker has dropped the runtime, credentials and lease before here.
            drop(active);
            sink.update(|s| match result {
                Ok(state) => s.state = state,
                Err(code) => { s.state = State::Failed; s.error = Some(code); },
            });
        }).map_err(|_| "job-thread")?;
        Ok(Self { mailbox, cancel })
    }
    pub fn snapshot(&self) -> Snapshot { self.mailbox.0.lock().unwrap_or_else(|p| p.into_inner()).clone() }
    pub fn cancel(&self) {
        self.mailbox.update(|s| {
            if !s.state.terminal() { s.state = State::Stopping; self.cancel.send_replace(true); }
        });
    }
}
impl Drop for Job { fn drop(&mut self) { self.cancel.send_replace(true); } }

fn worker(
    root: PathBuf, games: PathBuf,
    setup: impl FnOnce() -> Result<(Client, TokenManager), &'static str>,
    receiver: watch::Receiver<bool>, mailbox: &Arc<Mailbox>,
) -> Result<State, &'static str> {
    let lease = JobLease::open(&root, &games, Some(mailbox.clone()))?;
    let runtime = Builder::new_multi_thread().worker_threads(2).enable_all().max_blocking_threads(4).build().map_err(|_| "job-runtime")?;
    let result = run_drained(runtime, async {
        let (client, tokens) = setup()?;
        mailbox.update(|s| { if s.state != State::Stopping { s.state = State::Running; } });
        Ok::<_, &'static str>(lease.run(&client, &tokens).await)
    }, receiver);
    match result {
        Some(Ok(code)) => Ok(if code == ExitCode::SUCCESS { State::Complete } else { State::Failed }),
        Some(Err(code)) => Err(code),
        None => { lease.record_cancelled_after_drain()?; Ok(State::Cancelled) },
    }
}

fn run_drained<F: Future>(runtime: Runtime, operation: F, mut cancel: watch::Receiver<bool>) -> Option<F::Output> {
    let result = if *cancel.borrow() { None } else {
        runtime.block_on(async move {
            tokio::select! {
                biased;
                // A job that has finished publication wins over a late cancel.
                result = operation => Some(result),
                _ = cancel.wait_for(|value| *value) => None,
            }
        })
    };
    // A dropped future can leave Tokio file writes on the blocking pool. Runtime
    // destruction waits for them. Never use shutdown_background/timeout here.
    drop(runtime);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs2::FileExt;
    use std::{fs::OpenOptions, sync::mpsc, time::Duration};

    #[test]
    fn cancellation_retains_lease_until_blocking_io_has_drained() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("job-request.json"), br#"{"schema":1,"requestId":"11111111-1111-4111-8111-111111111111","operation":"refresh","market":"GB"}"#).unwrap();
        let lease = JobLease::open(root.path(), root.path(), None).unwrap();
        let contender = OpenOptions::new().write(true).open(root.path().join("job.lock")).unwrap();
        let (cancel, receiver) = watch::channel(false);
        let (started_tx, started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let (finished_tx, finished) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();
            let result = run_drained(runtime, async move {
                tokio::task::spawn_blocking(move || { started_tx.send(()).unwrap(); released.recv().unwrap(); });
                std::future::pending::<()>().await;
            }, receiver);
            assert!(result.is_none());
            lease.record_cancelled_after_drain().unwrap();
            drop(lease);
            finished_tx.send(()).unwrap();
        });
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        cancel.send_replace(true);
        assert!(contender.try_lock_exclusive().is_err());
        assert!(finished.recv_timeout(Duration::from_millis(50)).is_err());
        assert!(!root.path().join("job-result.json").exists());
        release.send(()).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
        contender.try_lock_exclusive().unwrap();
        let record = std::fs::read_to_string(root.path().join("job-result.json")).unwrap();
        assert!(record.contains("cancelled"));
    }

    #[test]
    fn cancelled_before_poll_never_starts_operation() {
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();
        let (_, receiver) = watch::channel(true);
        assert_eq!(run_drained(runtime, async { panic!("must not start"); #[allow(unreachable_code)] 1 }, receiver), None);
    }
}
