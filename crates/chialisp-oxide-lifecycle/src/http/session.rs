use crate::{
    runner::{Command, ValidatedRecipe},
    store::{CommitError, JournalStore},
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::SocketAddr,
    sync::{Arc, Mutex},
};
use tokio::sync::Semaphore;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandRequest {
    pub revision: u32,
    pub command: Intent,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Intent {
    Advance,
    Refresh,
    ReorgUnspent,
}

#[derive(Clone, Serialize)]
pub(super) struct ApiError {
    pub kind: &'static str,
    pub message: &'static str,
}
impl ApiError {
    pub fn new(kind: &'static str, message: &'static str) -> Self {
        Self { kind, message }
    }
}

pub(super) struct Shared {
    pub origin: String,
    pub host: String,
    pub capability: String,
    pub operation: Arc<Semaphore>,
    state: Mutex<State>,
}

struct State {
    store: JournalStore,
    recipe: ValidatedRecipe,
    last_error: Option<ApiError>,
}

impl Shared {
    pub fn with_recipe(
        store: JournalStore,
        address: SocketAddr,
        recipe: ValidatedRecipe,
    ) -> Result<Self> {
        // Every existing journal is replayed through the actual trusted SDK
        // runner before exposing the session. A new file is created, never an
        // unrelated existing file overwritten.
        if store.path().try_exists()? {
            recipe.resume(store.load()?.journal)?;
        } else {
            store.create(recipe.start()?.journal())?;
        }
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes)?;
        Ok(Self {
            origin: format!("http://{address}"),
            host: address.to_string(),
            capability: hex::encode(bytes),
            operation: Arc::new(Semaphore::new(1)),
            state: Mutex::new(State {
                store,
                recipe,
                last_error: None,
            }),
        })
    }

    pub async fn operate(
        self: Arc<Self>,
        command: Option<CommandRequest>,
    ) -> std::result::Result<Value, ApiError> {
        let permit = self.operation.clone().try_acquire_owned().map_err(|_| {
            ApiError::new(
                "busy",
                "Another journal operation is running. Reload state before retrying.",
            )
        })?;
        tokio::task::spawn_blocking(move || {
            // Keep this permit inside the blocking operation even if the HTTP
            // connection times out; there is never a queue of filesystem jobs.
            let _permit = permit;
            let mut state = self
                .state
                .lock()
                .map_err(|_| ApiError::new("infrastructure", "Session state is unavailable."))?;
            let result = state.operate(command);
            if let Err(error) = &result {
                state.last_error = Some(error.clone());
            }
            result
        })
        .await
        .map_err(|_| {
            ApiError::new(
                "infrastructure",
                "Journal operation did not complete normally. Reload durable state.",
            )
        })?
    }
}

impl State {
    fn operate(&mut self, command: Option<CommandRequest>) -> std::result::Result<Value, ApiError> {
        let stored = self.store.load().map_err(|_| {
            ApiError::new(
                "storage_read",
                "The bounded saved journal could not be read.",
            )
        })?;
        let mut runner = self.recipe.resume(stored.journal).map_err(|_| {
            ApiError::new(
                "replay_failed",
                "Saved events do not match the trusted native/SDK simulator replay.",
            )
        })?;
        if let Some(request) = command {
            if request.revision != runner.journal().revision() {
                return Err(ApiError::new(
                    "revision_conflict",
                    "Saved revision changed. Reload state before retrying.",
                ));
            }
            let command = match request.command {
                Intent::Advance => Command::Advance,
                Intent::Refresh => Command::Refresh,
                Intent::ReorgUnspent => Command::ReorgUnspent,
            };
            runner.execute(command).map_err(|_| {
                ApiError::new(
                    "simulator_command",
                    "This trusted simulator action is unavailable at the current stage.",
                )
            })?;
            match self
                .store
                .commit(Some(stored.byte_digest), runner.journal())
            {
                Ok(_) => self.last_error = None,
                Err(CommitError::BeforeCommit(_)) => {
                    return Err(ApiError::new(
                        "storage_before_commit",
                        "The candidate was not committed. Reload durable state before retrying.",
                    ));
                }
                Err(CommitError::CommittedDurabilityUnknown(_)) => {
                    // Replacement occurred. Reload/admit the actual committed
                    // candidate instead of retaining a fictitious old revision.
                    let reread = self.store.load().map_err(|_| ApiError::new("storage_committed_durability_unknown", "Journal replacement occurred; durability is unknown. Reload before retrying."))?;
                    let _actual = self.recipe.resume(reread.journal).map_err(|_| ApiError::new("storage_committed_durability_unknown", "Journal replacement occurred; its current state could not be replayed."))?;
                    return Err(ApiError::new(
                        "storage_committed_durability_unknown",
                        "Journal replacement occurred; durability is unknown. Reload before retrying.",
                    ));
                }
            }
        }
        let view = runner.view().map_err(|_| {
            ApiError::new(
                "infrastructure",
                "Public simulator state could not be rendered.",
            )
        })?;
        Ok(json!({"view":view,"last_error":self.last_error.as_ref()}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::{CONNECTION_TIMEOUT, LoopbackServer, tests::client};
    use hyper::{Method, StatusCode};
    use std::{sync::mpsc, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpStream,
        sync::oneshot,
    };

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn connection_timeout_does_not_cancel_started_operation_or_allow_duplicate_advance() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("timeout.lifecycle.json");
        let server = LoopbackServer::bind(JournalStore::new(&path).unwrap())
            .await
            .unwrap();
        let shared = server.shared.clone();
        let address = server.address().unwrap();
        let origin = server.url().to_owned();
        let (stop, rx) = oneshot::channel();
        let task = tokio::spawn(server.run_until(async {
            let _ = rx.await;
        }));
        let (status, _, page) =
            client::send(address, Method::GET, "/", &address.to_string(), &[], vec![]).await;
        assert_eq!(status, StatusCode::OK);
        let capability = client::capability(&page);
        let (locked_tx, locked_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let blocked = shared.clone();
        let blocker = std::thread::spawn(move || {
            let _guard = blocked.state.lock().unwrap();
            let _ = locked_tx.send(());
            let _ = release_rx.recv_timeout(Duration::from_secs(10));
        });
        assert!(locked_rx.recv_timeout(Duration::from_secs(1)).is_ok());
        let body = r#"{"revision":0,"command":"advance"}"#;
        let wire = format!(
            "POST /api/commands HTTP/1.1\r\nHost: {address}\r\nOrigin: {origin}\r\nX-Oxide-Session: {capability}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(wire.as_bytes()).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(1), async {
                while shared.operation.available_permits() != 0 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .is_ok()
        );
        let mut response = Vec::new();
        let closed = tokio::time::timeout(
            CONNECTION_TIMEOUT + Duration::from_secs(1),
            stream.take(4096).read_to_end(&mut response),
        )
        .await;
        assert!(closed.is_ok(), "HTTP connection exceeded its lifetime");
        assert_eq!(
            shared.operation.available_permits(),
            0,
            "started operation permit was released by HTTP timeout"
        );
        let (status, _, _) = client::send(
            address,
            Method::GET,
            "/api/state",
            &address.to_string(),
            &[("x-oxide-session", &capability)],
            vec![],
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT);
        let _ = release_tx.send(());
        blocker.join().unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(3), async {
                while shared.operation.available_permits() != 1 {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .is_ok()
        );
        assert_eq!(
            JournalStore::new(&path)
                .unwrap()
                .load()
                .unwrap()
                .journal
                .revision(),
            1
        );
        let (status, _, _) = client::send(
            address,
            Method::POST,
            "/api/commands",
            &address.to_string(),
            &[
                ("x-oxide-session", &capability),
                ("origin", &origin),
                ("content-type", "application/json"),
            ],
            body.as_bytes().to_vec(),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "old revision advanced a second time after timeout"
        );
        let _ = stop.send(());
        assert!(task.await.unwrap().is_ok());
    }
}
