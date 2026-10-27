use crate::Digest;
use crate::live::{LiveSession, TransferKind, network::Testnet11Client, view::LiveView};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    time::Duration,
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, oneshot};

const MAX_JOBS: u64 = 256;
const MAX_VIEW_BYTES: usize = 60 * 1024;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveAction {
    Reload,
    PrepareFunding,
    SubmitFunding,
    MonitorFunding,
    PreparePayment,
    SubmitPayment,
    MonitorPayment,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ApiError {
    pub kind: String,
    pub message: String,
}
impl ApiError {
    pub(super) fn new(kind: &str, message: &str) -> Self {
        fn bounded(value: &str, maximum: usize) -> String {
            let mut end = value.len().min(maximum);
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            value[..end].to_owned()
        }
        Self {
            kind: bounded(kind, 64),
            message: bounded(message, 512),
        }
    }
    fn infrastructure() -> Self {
        Self::new(
            "infrastructure",
            "The local worker is unavailable. Restart and reload the durable outbox.",
        )
    }
}
impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)
    }
}
impl std::error::Error for ApiError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobView {
    pub id: String,
    pub action: LiveAction,
    pub status: JobStatus,
    pub error: Option<ApiError>,
}

#[derive(Clone, Serialize)]
pub(super) struct Snapshot {
    pub view: Value,
    pub job: Option<JobView>,
    pub busy: bool,
    pub needs_reload: bool,
}
struct Cache {
    revision: Digest,
    snapshot: Snapshot,
}

/// Private backend interface. Only the sealed session implementation is
/// available in production; arbitrary JSON backends exist only in unit tests.
pub(super) trait Backend: Send + 'static {
    fn initialize(&mut self) -> Result<(Digest, Value), ApiError>;
    fn execute(
        &mut self,
        action: LiveAction,
        revision: Digest,
    ) -> Result<(Digest, Value), ApiError>;
}
struct SessionBackend {
    session: LiveSession,
    network: Testnet11Client,
}
impl SessionBackend {
    fn view(&self) -> Result<(Digest, Value), ApiError> {
        let view = LiveView::from_session(&self.session).map_err(|_| ApiError::infrastructure())?;
        Ok((
            self.session.revision(),
            serde_json::to_value(view).map_err(|_| ApiError::infrastructure())?,
        ))
    }
}
impl Backend for SessionBackend {
    fn initialize(&mut self) -> Result<(Digest, Value), ApiError> {
        self.session
            .reload_current(&mut self.network)
            .map_err(action_error)?;
        self.view()
    }
    fn execute(
        &mut self,
        action: LiveAction,
        revision: Digest,
    ) -> Result<(Digest, Value), ApiError> {
        if action != LiveAction::Reload {
            // Check the exact disk/RAM revision before *any* provider or TTY
            // work. Reload instead compares the posted cached revision in the
            // controller so legitimate external appends remain recoverable.
            self.session.require_revision(&revision).map_err(|_| {
                ApiError::new(
                    "revision_conflict",
                    "The durable outbox changed. Reload before another action.",
                )
            })?;
        }
        self.session
            .reload_current(&mut self.network)
            .map_err(action_error)?;
        let result = match action {
            LiveAction::Reload => Ok(()),
            LiveAction::PrepareFunding => self.session.prepare_funding(&mut self.network),
            LiveAction::SubmitFunding => self
                .session
                .submit(&mut self.network, TransferKind::Funding),
            LiveAction::MonitorFunding => self
                .session
                .monitor(&mut self.network, TransferKind::Funding)
                .map(|_| ()),
            LiveAction::PreparePayment => self.session.prepare_payment(&mut self.network),
            LiveAction::SubmitPayment => self
                .session
                .submit(&mut self.network, TransferKind::Payment),
            LiveAction::MonitorPayment => self
                .session
                .monitor(&mut self.network, TransferKind::Payment)
                .map(|_| ()),
        };
        result.map_err(action_error)?;
        self.view()
    }
}
fn action_error(error: anyhow::Error) -> ApiError {
    if let Some(error) = error.downcast_ref::<crate::live::network::Error>() {
        return ApiError::new("provider_observation", &error.message);
    }
    if let Some(error) = error.downcast_ref::<crate::live::wallet::WalletError>() {
        return ApiError::new("wallet_preflight", &error.to_string());
    }
    ApiError::new(
        "live_action",
        "The live action did not complete normally and may have committed public state. Reload the durable outbox before retrying.",
    )
}

struct Job {
    id: String,
    action: LiveAction,
    revision: Digest,
    _permit: OwnedSemaphorePermit,
}
pub(super) struct Controller {
    sender: Mutex<Option<SyncSender<Job>>>,
    cache: Arc<Mutex<Option<Cache>>>,
    operation: Arc<Semaphore>,
    accepting: AtomicBool,
    next: AtomicU64,
    finished: Mutex<Option<oneshot::Receiver<()>>>,
}
impl Controller {
    pub(super) async fn production(session: LiveSession) -> Result<Self, ApiError> {
        Self::launch(move || {
            // This closure runs on an ordinary thread, never spawn_blocking or
            // a Tokio worker. The client owns a separate private runtime.
            let network = Testnet11Client::new().map_err(|_| ApiError::infrastructure())?;
            Ok(SessionBackend { session, network })
        })
        .await
    }
    pub(super) async fn launch<B: Backend>(
        factory: impl FnOnce() -> Result<B, ApiError> + Send + 'static,
    ) -> Result<Self, ApiError> {
        let (sender, receiver) = sync_channel::<Job>(1);
        let (ready_tx, ready_rx) = oneshot::channel();
        let (finished_tx, finished_rx) = oneshot::channel();
        let cache = Arc::new(Mutex::new(None));
        let worker_cache = cache.clone();
        std::thread::Builder::new()
            .name("oxide-live-gui".into())
            .spawn(move || {
                struct Finished(Option<oneshot::Sender<()>>);
                impl Drop for Finished {
                    fn drop(&mut self) {
                        if let Some(sender) = self.0.take() {
                            let _ = sender.send(());
                        }
                    }
                }
                let _finished = Finished(Some(finished_tx));
                let mut backend = match factory() {
                    Ok(backend) => backend,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                let initial = backend.initialize().and_then(|(revision, view)| {
                    bounded_view(&view)?;
                    let mut cache = worker_cache
                        .lock()
                        .map_err(|_| ApiError::infrastructure())?;
                    *cache = Some(Cache {
                        revision,
                        snapshot: Snapshot {
                            view,
                            job: None,
                            busy: false,
                            needs_reload: false,
                        },
                    });
                    Ok(())
                });
                if ready_tx.send(initial.clone()).is_err() || initial.is_err() {
                    return;
                }
                while let Ok(job) = receiver.recv() {
                    if let Ok(mut cache) = worker_cache.lock()
                        && let Some(cache) = cache.as_mut()
                        && let Some(view) = &mut cache.snapshot.job
                    {
                        view.status = JobStatus::Running;
                    }
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        backend.execute(job.action, job.revision)
                    }))
                    .unwrap_or_else(|_| Err(ApiError::infrastructure()))
                    .and_then(|(revision, view)| {
                        bounded_view(&view)?;
                        Ok((revision, view))
                    });
                    let Ok(mut guard) = worker_cache.lock() else {
                        return;
                    };
                    let Some(cache) = guard.as_mut() else {
                        return;
                    };
                    let (status, error) = match result {
                        Ok((revision, view)) => {
                            cache.revision = revision;
                            cache.snapshot.view = view;
                            cache.snapshot.needs_reload = false;
                            (JobStatus::Succeeded, None)
                        }
                        Err(error) => {
                            cache.snapshot.needs_reload = true;
                            (JobStatus::Failed, Some(error))
                        }
                    };
                    cache.snapshot.busy = false;
                    cache.snapshot.job = Some(JobView {
                        id: job.id.clone(),
                        action: job.action,
                        status,
                        error,
                    });
                    // job owns the permit until after the complete cache update,
                    // regardless of a client timeout or disconnected browser.
                    drop(job);
                    // Publish idle only once its permit is available: a reader
                    // cannot observe busy=false in the brief permit-drop window.
                    drop(guard);
                }
            })
            .map_err(|_| ApiError::infrastructure())?;
        match tokio::time::timeout(STARTUP_TIMEOUT, ready_rx).await {
            Ok(Ok(result)) => result?,
            _ => {
                return Err(ApiError::new(
                    "startup",
                    "The ordinary live worker did not become ready within its startup limit.",
                ));
            }
        }
        Ok(Self {
            sender: Mutex::new(Some(sender)),
            cache,
            operation: Arc::new(Semaphore::new(1)),
            accepting: AtomicBool::new(true),
            next: AtomicU64::new(0),
            finished: Mutex::new(Some(finished_rx)),
        })
    }
    pub(super) fn snapshot(&self) -> Result<Snapshot, ApiError> {
        self.cache
            .lock()
            .map_err(|_| ApiError::infrastructure())?
            .as_ref()
            .map(|cache| cache.snapshot.clone())
            .ok_or_else(ApiError::infrastructure)
    }
    pub(super) fn accept(&self, action: LiveAction, revision: Digest) -> Result<String, ApiError> {
        if !self.accepting.load(Ordering::Acquire) {
            return Err(ApiError::new(
                "shutdown",
                "This server is no longer accepting operations.",
            ));
        }
        let permit = self.operation.clone().try_acquire_owned().map_err(|_| {
            ApiError::new(
                "busy",
                "One live operation is still running. Check its status or the local terminal.",
            )
        })?;
        let sender_guard = self.sender.lock().map_err(|_| ApiError::infrastructure())?;
        let sender = sender_guard.as_ref().ok_or_else(ApiError::infrastructure)?;
        let mut cache = self.cache.lock().map_err(|_| ApiError::infrastructure())?;
        let cache = cache.as_mut().ok_or_else(ApiError::infrastructure)?;
        if cache.revision != revision {
            return Err(ApiError::new(
                "revision_conflict",
                "The cached revision changed. Read current status before retrying.",
            ));
        }
        if cache.snapshot.needs_reload && action != LiveAction::Reload {
            return Err(ApiError::new(
                "reload_required",
                "Reload durable state before another live action.",
            ));
        }
        let next = self
            .next
            .try_update(Ordering::AcqRel, Ordering::Acquire, |id| {
                (id < MAX_JOBS).then_some(id + 1)
            })
            .map_err(|_| {
                ApiError::new(
                    "job_limit",
                    "This server reached its finite operation limit. Restart with fresh checks.",
                )
            })?
            + 1;
        let id = next.to_string();
        cache.snapshot.busy = true;
        cache.snapshot.job = Some(JobView {
            id: id.clone(),
            action,
            status: JobStatus::Queued,
            error: None,
        });
        let job = Job {
            id: id.clone(),
            action,
            revision,
            _permit: permit,
        };
        let sent = sender.try_send(job);
        if sent.is_err() {
            cache.snapshot.busy = false;
            cache.snapshot.needs_reload = true;
            if let Some(job) = &mut cache.snapshot.job {
                job.status = JobStatus::Failed;
                job.error = Some(ApiError::infrastructure());
            }
            return Err(ApiError::infrastructure());
        }
        Ok(id)
    }
    pub(super) async fn shutdown(&self) {
        self.accepting.store(false, Ordering::Release);
        if let Ok(mut sender) = self.sender.lock() {
            sender.take();
        }
        let finished = self
            .finished
            .lock()
            .ok()
            .and_then(|mut finished| finished.take());
        if let Some(finished) = finished {
            let _ = finished.await;
        }
    }
}
fn bounded_view(view: &Value) -> Result<(), ApiError> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|size| *size <= MAX_VIEW_BYTES)
                .ok_or_else(|| std::io::Error::other("public view bound"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Count(0), view).map_err(|_| {
        ApiError::new(
            "view_limit",
            "The complete public view exceeded its local bound. No truncated view was accepted.",
        )
    })
}
