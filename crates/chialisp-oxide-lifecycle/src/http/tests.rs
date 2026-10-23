use super::*;
#[path = "../../tests/common/http.rs"]
pub(super) mod client;
use crate::{CoinRecord, Stage, store::JournalStore};
use hyper::{Method, StatusCode};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    task::JoinHandle,
};

struct Fixture {
    _directory: tempfile::TempDir,
    path: std::path::PathBuf,
    address: SocketAddr,
    origin: String,
    connections: Arc<Semaphore>,
    operations: Arc<Semaphore>,
    stop: oneshot::Sender<()>,
    task: JoinHandle<Result<()>>,
}

impl Fixture {
    async fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let path = directory
            .path()
            .canonicalize()
            .unwrap()
            .join("gui.lifecycle.json");
        let server = LoopbackServer::bind(JournalStore::new(&path).unwrap())
            .await
            .unwrap();
        let address = server.address().unwrap();
        let origin = server.url().to_owned();
        let connections = server.connections.clone();
        let operations = server.shared.operation.clone();
        let (stop, rx) = oneshot::channel();
        let task = tokio::spawn(server.run_until(async {
            let _ = rx.await;
        }));
        Self {
            _directory: directory,
            path,
            address,
            origin,
            connections,
            operations,
            stop,
            task,
        }
    }
    async fn bootstrap(&self) -> String {
        let (status, headers, page) = client::send(
            self.address,
            Method::GET,
            "/",
            &self.address.to_string(),
            &[],
            vec![],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(headers.get("cache-control").unwrap(), "no-store");
        assert!(
            headers
                .get("content-security-policy")
                .unwrap()
                .to_str()
                .unwrap()
                .contains("frame-ancestors 'none'")
        );
        assert!(headers.get("access-control-allow-origin").is_none());
        assert!(
            std::str::from_utf8(&page)
                .unwrap()
                .contains("WIP prototype")
        );
        client::capability(&page)
    }
    async fn state(&self, capability: &str) -> Value {
        let (status, _, body) = client::send(
            self.address,
            Method::GET,
            "/api/state",
            &self.address.to_string(),
            &[("x-oxide-session", capability)],
            vec![],
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        serde_json::from_slice(&body).expect("public state JSON invalid")
    }
    async fn command(&self, capability: &str, revision: u32, command: &str) -> (StatusCode, Value) {
        let (status, _, body) = client::send(
            self.address,
            Method::POST,
            "/api/commands",
            &self.address.to_string(),
            &[
                ("origin", &self.origin),
                ("x-oxide-session", capability),
                ("content-type", "application/json"),
            ],
            serde_json::to_vec(&json!({"revision":revision,"command":command})).unwrap(),
        )
        .await;
        (
            status,
            serde_json::from_slice(&body).expect("command response JSON invalid"),
        )
    }
    async fn finish(self) {
        let _ = self.stop.send(());
        assert!(self.task.await.unwrap().is_ok());
    }
}

#[tokio::test]
async fn http_same_trusted_flow_confirms_exact_children_and_restores_checkpoint() {
    let fixture = Fixture::new().await;
    let capability = fixture.bootstrap().await;
    let mut state = fixture.state(&capability).await;
    assert_eq!(state["view"]["checks"]["fresh_cargo_build"], "unavailable");
    assert_eq!(
        state["view"]["artifact_origin"],
        "retained_optimized_rust_artifact"
    );
    assert_eq!(
        state["view"]["typed_controls"][0]["code"],
        "AssertMyAmountFailed"
    );
    for revision in 0..7 {
        let (status, next) = fixture.command(&capability, revision, "advance").await;
        assert_eq!(status, StatusCode::OK);
        state = next;
    }
    assert_eq!(state["view"]["stage"], "confirmed");
    let children = state["view"]["children"].as_array().unwrap();
    assert_eq!(children.len(), 1);
    let coin: CoinRecord = serde_json::from_value(children[0]["coin"].clone()).unwrap();
    assert_eq!(coin.amount, 1000);
    assert_eq!(coin.puzzle_hash, [0x42; 32]);
    assert_eq!(children[0]["coin_id"], hex::encode(coin.id()));
    let (status, state) = fixture.command(&capability, 7, "reorg_unspent").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state["view"]["stage"], "ready_to_submit");
    let (status, _) = fixture.command(&capability, 8, "refresh").await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = fixture.command(&capability, 9, "advance").await;
    assert_eq!(status, StatusCode::OK);
    let (status, state) = fixture.command(&capability, 10, "advance").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(state["view"]["stage"], "confirmed");
    let saved = JournalStore::new(&fixture.path).unwrap().load().unwrap();
    assert_eq!(saved.journal.stage(), Stage::Confirmed);
    assert!(
        !String::from_utf8_lossy(&std::fs::read(&fixture.path).unwrap()).contains(&capability),
        "capability was persisted"
    );
    fixture.finish().await;
}

#[tokio::test]
async fn http_origin_host_session_and_closed_commands_refuse_without_mutation() {
    let fixture = Fixture::new().await;
    let capability = fixture.bootstrap().await;
    let before = std::fs::read(&fixture.path).unwrap();
    let host = fixture.address.to_string();
    let body = serde_json::to_vec(&json!({"revision":0,"command":"advance"})).unwrap();
    for (path, method, host, headers, expected) in [
        (
            "/api/state",
            Method::GET,
            host.as_str(),
            vec![],
            StatusCode::FORBIDDEN,
        ),
        (
            "/api/state",
            Method::GET,
            host.as_str(),
            vec![("x-oxide-session", "wrong")],
            StatusCode::FORBIDDEN,
        ),
        (
            "/",
            Method::GET,
            "evil.example",
            vec![],
            StatusCode::FORBIDDEN,
        ),
        (
            "/",
            Method::GET,
            host.as_str(),
            vec![("origin", "http://evil.example")],
            StatusCode::FORBIDDEN,
        ),
        (
            "/",
            Method::GET,
            host.as_str(),
            vec![("sec-fetch-site", "cross-site")],
            StatusCode::FORBIDDEN,
        ),
        (
            "/api/commands",
            Method::POST,
            host.as_str(),
            vec![
                ("x-oxide-session", capability.as_str()),
                ("content-type", "application/json"),
            ],
            StatusCode::FORBIDDEN,
        ),
        (
            "/api/commands",
            Method::POST,
            host.as_str(),
            vec![
                ("x-oxide-session", capability.as_str()),
                ("origin", "null"),
                ("content-type", "application/json"),
            ],
            StatusCode::FORBIDDEN,
        ),
        (
            "/api/commands",
            Method::OPTIONS,
            host.as_str(),
            vec![("origin", "http://evil.example")],
            StatusCode::FORBIDDEN,
        ),
    ] {
        let payload = if method == Method::POST {
            body.clone()
        } else {
            vec![]
        };
        let (status, response_headers, _) =
            client::send(fixture.address, method, path, host, &headers, payload).await;
        assert_eq!(status, expected);
        assert!(
            response_headers
                .get("access-control-allow-origin")
                .is_none()
        );
    }
    for invalid in [
        json!({"revision":0,"command":"advance","path":"/tmp/unrelated"}),
        json!({"revision":true,"command":"advance"}),
        json!({"revision":0,"event":{"event":"checked"}}),
        json!({"revision":0,"command":"push_mainnet"}),
    ] {
        let (status, _, _) = client::send(
            fixture.address,
            Method::POST,
            "/api/commands",
            &host,
            &[
                ("origin", &fixture.origin),
                ("x-oxide-session", &capability),
                ("content-type", "application/json"),
            ],
            serde_json::to_vec(&invalid).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
    fixture.finish().await;
}

#[tokio::test]
async fn http_session_capabilities_are_independent_and_never_public_state() {
    let first = Fixture::new().await;
    let second = Fixture::new().await;
    let first_capability = first.bootstrap().await;
    let second_capability = second.bootstrap().await;
    assert!(
        first_capability != second_capability,
        "two sessions reused a capability"
    );
    let (status, _, _) = client::send(
        second.address,
        Method::GET,
        "/api/state",
        &second.address.to_string(),
        &[("x-oxide-session", &first_capability)],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let state = second.state(&second_capability).await.to_string();
    assert!(
        !state.contains(&second_capability),
        "public API reflected a capability"
    );
    first.finish().await;
    second.finish().await;
}

async fn raw(address: SocketAddr, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut bytes = Vec::new();
    let result = tokio::time::timeout(
        CONNECTION_TIMEOUT + Duration::from_secs(1),
        stream.take(64 * 1024).read_to_end(&mut bytes),
    )
    .await;
    assert!(
        result.is_ok(),
        "raw HTTP connection did not close within its cap"
    );
    bytes
}

#[tokio::test]
async fn http_header_body_and_read_time_caps_do_not_run_commands() {
    let fixture = Fixture::new().await;
    let capability = fixture.bootstrap().await;
    let before = std::fs::read(&fixture.path).unwrap();
    let mut headers = format!("GET / HTTP/1.1\r\nHost: {}\r\n", fixture.address);
    for index in 0..MAX_HEADERS {
        headers.push_str(&format!("X-Test-{index}: value\r\n"));
    }
    headers.push_str("\r\n");
    assert!(
        String::from_utf8_lossy(&raw(fixture.address, headers.as_bytes()).await)
            .starts_with("HTTP/1.1 431")
    );
    let oversized = format!(
        "GET / HTTP/1.1\r\nHost: {}\r\nX-Padding: {}\r\n\r\n",
        fixture.address,
        "a".repeat(MAX_HEADER_BYTES)
    );
    assert!(
        String::from_utf8_lossy(&raw(fixture.address, oversized.as_bytes()).await)
            .starts_with("HTTP/1.1 431")
    );
    let (status, _, _) = client::send(
        fixture.address,
        Method::POST,
        "/api/commands",
        &fixture.address.to_string(),
        &[
            ("origin", &fixture.origin),
            ("x-oxide-session", &capability),
            ("content-type", "application/json"),
        ],
        vec![b'a'; MAX_BODY + 1],
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    let chunked = format!(
        "POST /api/commands HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nX-Oxide-Session: {}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n{:x}\r\n{}\r\n0\r\n\r\n",
        fixture.address,
        fixture.origin,
        capability,
        MAX_BODY + 1,
        "a".repeat(MAX_BODY + 1)
    );
    assert!(
        String::from_utf8_lossy(&raw(fixture.address, chunked.as_bytes()).await)
            .starts_with("HTTP/1.1 413")
    );
    let incomplete = format!(
        "POST /api/commands HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nX-Oxide-Session: {}\r\nContent-Type: application/json\r\nContent-Length: 20\r\n\r\n{{",
        fixture.address, fixture.origin, capability
    );
    assert!(
        String::from_utf8_lossy(&raw(fixture.address, incomplete.as_bytes()).await)
            .starts_with("HTTP/1.1 408")
    );
    let _closed = raw(fixture.address, b"GET / HTTP/1.1\r\n").await;
    assert_eq!(std::fs::read(&fixture.path).unwrap(), before);
    fixture.finish().await;
}

#[tokio::test]
async fn http_connections_and_operations_are_capped_and_revisions_serialize_races() {
    let fixture = Fixture::new().await;
    let capability = fixture.bootstrap().await;
    let mut held = Vec::new();
    for _ in 0..MAX_CONNECTIONS {
        let mut stream = TcpStream::connect(fixture.address).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        held.push(stream);
    }
    let acquired = tokio::time::timeout(Duration::from_secs(1), async {
        while fixture.connections.available_permits() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await;
    assert!(
        acquired.is_ok(),
        "test connections were not admitted before header timeout"
    );
    let mut extra = TcpStream::connect(fixture.address).await.unwrap();
    let mut one = [0u8; 1];
    let rejected = tokio::time::timeout(Duration::from_secs(1), extra.read(&mut one)).await;
    assert!(
        matches!(rejected, Ok(Ok(0) | Err(_))),
        "connection cap did not close the extra connection"
    );
    drop(held);
    assert!(
        tokio::time::timeout(Duration::from_secs(3), async {
            while fixture.connections.available_permits() != MAX_CONNECTIONS {
                tokio::task::yield_now().await;
            }
        })
        .await
        .is_ok()
    );
    let (first, second) = tokio::join!(
        fixture.command(&capability, 0, "advance"),
        fixture.command(&capability, 0, "advance")
    );
    let mut statuses = [first.0.as_u16(), second.0.as_u16()];
    statuses.sort();
    assert_eq!(statuses, [200, 409]);
    let permit = fixture.operations.clone().acquire_owned().await.unwrap();
    let (status, _, _) = client::send(
        fixture.address,
        Method::GET,
        "/api/state",
        &fixture.address.to_string(),
        &[("x-oxide-session", &capability)],
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    drop(permit);
    assert_eq!(fixture.state(&capability).await["view"]["revision"], 1);
    fixture.finish().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn selected_recipe_http_and_cli_core_share_exact_journal_and_recipient() {
    use crate::{
        fresh::{PaymentMode, reference_recipe_control},
        runner::Command,
    };
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("selected.lifecycle.json");
    let recipe = reference_recipe_control([0x27; 32], PaymentMode::Off).unwrap();
    let server =
        LoopbackServer::bind_with_recipe(JournalStore::new(&path).unwrap(), recipe.clone())
            .await
            .unwrap();
    let address = server.address().unwrap();
    let origin = server.url().to_owned();
    let (stop, rx) = tokio::sync::oneshot::channel();
    let task = tokio::spawn(server.run_until(async {
        let _ = rx.await;
    }));
    let (status, _, page) =
        client::send(address, Method::GET, "/", &address.to_string(), &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    let capability = client::capability(&page);
    for revision in 0..3 {
        let (status, _, body) = client::send(
            address,
            Method::POST,
            "/api/commands",
            &address.to_string(),
            &[
                ("x-oxide-session", &capability),
                ("origin", &origin),
                ("content-type", "application/json"),
            ],
            serde_json::to_vec(&serde_json::json!({"revision":revision,"command":"advance"}))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["view"]["destination_hex"], "27".repeat(32));
        assert_eq!(
            value["view"]["checks"]["fresh_cargo_build"],
            "unavailable_test_control"
        );
    }
    let _ = stop.send(());
    assert!(task.await.unwrap().is_ok());
    let store = JournalStore::new(&path).unwrap();
    let stored = store.load().unwrap();
    let mut pin = stored.byte_digest;
    let mut runner = recipe.resume(stored.journal).unwrap();
    while runner.journal().stage() != crate::Stage::Confirmed {
        runner.execute(Command::Advance).unwrap();
        pin = store.commit(Some(pin), runner.journal()).unwrap();
    }
    assert_eq!(
        runner.view().unwrap()["children"][0]["coin"]["puzzle_hash"],
        serde_json::json!(vec![0x27; 32])
    );
    assert_eq!(store.load().unwrap().byte_digest, pin);
}
