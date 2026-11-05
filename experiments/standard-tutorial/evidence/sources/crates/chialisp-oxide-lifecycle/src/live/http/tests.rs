//! Actual loopback HTTP and ordinary worker controls. No real wallet, source
//! validation, network node or signature is supplied by these private fixtures.
use super::worker::{ApiError, Backend, Controller, LiveAction};
use super::*;
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{HeaderMap, Method, Request, StatusCode, client::conn::http1};
use serde_json::{Value, json};
use std::{
    sync::{
        Condvar, Mutex,
        atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::oneshot,
    task::JoinHandle,
};

#[derive(Default)]
struct Control {
    durable: AtomicU8,
    calls: AtomicUsize,
    started: AtomicBool,
    fail_after_commit: AtomicBool,
    panic_next: AtomicBool,
    huge_next: AtomicBool,
    block: Mutex<bool>,
    changed: Condvar,
}
impl Control {
    fn release(&self) {
        *self.block.lock().unwrap() = false;
        self.changed.notify_all();
    }
}
struct FixtureBackend {
    control: Arc<Control>,
    revision: u8,
}
impl FixtureBackend {
    fn view(&self) -> (crate::Digest, Value) {
        let revision = [self.revision; 32];
        (
            revision,
            json!({"revision":hex::encode(revision),"scope":"constructed GUI fixture, no fresh or live execution credit",
            "amount_mojos":"18446744073709551615"}),
        )
    }
}
impl Backend for FixtureBackend {
    fn initialize(&mut self) -> Result<(crate::Digest, Value), ApiError> {
        assert!(
            tokio::runtime::Handle::try_current().is_err(),
            "backend must be outside Tokio"
        );
        Ok(self.view())
    }
    fn execute(
        &mut self,
        action: LiveAction,
        revision: crate::Digest,
    ) -> Result<(crate::Digest, Value), ApiError> {
        assert!(
            tokio::runtime::Handle::try_current().is_err(),
            "backend must be outside Tokio"
        );
        if action != LiveAction::Reload
            && (revision != [self.revision; 32]
                || self.control.durable.load(Ordering::Acquire) != self.revision)
        {
            return Err(ApiError::new(
                "revision_conflict",
                "constructed disk revision changed before network/signing",
            ));
        }
        self.control.calls.fetch_add(1, Ordering::AcqRel);
        self.control.started.store(true, Ordering::Release);
        let guard = self.control.block.lock().unwrap();
        let (guard, timeout) = self
            .control
            .changed
            .wait_timeout_while(guard, Duration::from_secs(12), |blocked| *blocked)
            .unwrap();
        assert!(
            !timeout.timed_out() || !*guard,
            "fixture worker gate exceeded deadline"
        );
        drop(guard);
        if self.control.panic_next.swap(false, Ordering::AcqRel) {
            panic!("constructed worker panic");
        }
        if action == LiveAction::Reload {
            self.revision = self.control.durable.load(Ordering::Acquire);
        } else {
            self.revision += 1;
            self.control.durable.store(self.revision, Ordering::Release);
        }
        if self.control.fail_after_commit.swap(false, Ordering::AcqRel) {
            return Err(ApiError::new(
                "storage_committed_durability_unknown",
                "constructed postcommit uncertainty; reload actual saved state",
            ));
        }
        if self.control.huge_next.swap(false, Ordering::AcqRel) {
            return Ok(([self.revision; 32], json!({"huge":"x".repeat(70*1024)})));
        }
        Ok(self.view())
    }
}
struct Fixture {
    address: SocketAddr,
    origin: String,
    capability: String,
    shared: Arc<Shared>,
    control: Arc<Control>,
    stop: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<anyhow::Result<()>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.control.release();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}
impl Fixture {
    async fn new() -> Self {
        let control = Arc::new(Control::default());
        let backend = FixtureBackend {
            control: control.clone(),
            revision: 0,
        };
        let worker = Controller::launch(move || Ok(backend)).await.unwrap();
        let server = LiveLoopbackServer::with_worker(worker).await.unwrap();
        let address = server.address().unwrap();
        let origin = server.url().to_owned();
        let shared = server.shared.clone();
        let (stop, rx) = oneshot::channel();
        let task = tokio::spawn(server.run_until(async {
            let _ = rx.await;
        }));
        let (status, _, page) =
            send(address, Method::GET, "/", &address.to_string(), &[], vec![]).await;
        assert_eq!(status, StatusCode::OK);
        let page = std::str::from_utf8(&page).unwrap();
        let capability = page
            .split_once("name=\"oxide-session\" content=\"")
            .unwrap()
            .1
            .split_once('"')
            .unwrap()
            .0
            .to_owned();
        assert_eq!(capability.len(), 64);
        Self {
            address,
            origin,
            capability,
            shared,
            control,
            stop: Some(stop),
            task: Some(task),
        }
    }
    async fn state(&self) -> Value {
        let (status, _, bytes) = self.request(Method::GET, "/api/state", vec![], &[]).await;
        assert_eq!(status, StatusCode::OK);
        serde_json::from_slice(&bytes).unwrap()
    }
    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
        extra: &[(&str, &str)],
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut headers = vec![("x-oxide-session", self.capability.as_str())];
        headers.extend_from_slice(extra);
        send(
            self.address,
            method,
            path,
            &self.address.to_string(),
            &headers,
            body,
        )
        .await
    }
    async fn action(&self, action: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        let state = self.state().await;
        self.action_at(action, state["view"]["revision"].as_str().unwrap())
            .await
    }
    async fn action_at(&self, action: &str, revision: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        self.request(
            Method::POST,
            "/api/action",
            serde_json::to_vec(&json!({"action":action,"revision":revision})).unwrap(),
            &[
                ("origin", &self.origin),
                ("content-type", "application/json"),
            ],
        )
        .await
    }
    async fn settled(&self) -> Value {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let state = self.state().await;
                if state["busy"] == false {
                    return state;
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .expect("fixture job did not settle")
    }
    async fn started(&self) {
        tokio::time::timeout(Duration::from_secs(2), async {
            while !self.control.started.load(Ordering::Acquire) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
    async fn close(&mut self) {
        self.control.release();
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = self.task.take() {
            tokio::time::timeout(Duration::from_secs(3), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }
}
async fn send(
    address: SocketAddr,
    method: Method,
    path: &str,
    host: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let stream = TcpStream::connect(address).await.unwrap();
        let (mut sender, connection) = http1::handshake(TokioIo::new(stream)).await.unwrap();
        let task = tokio::spawn(async move {
            let _ = connection.await;
        });
        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .header("host", host);
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let response = sender
            .send_request(builder.body(Full::new(Bytes::from(body))).unwrap())
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        let bytes = Limited::new(body, 64 * 1024)
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();
        task.await.unwrap();
        (parts.status, parts.headers, bytes)
    })
    .await
    .expect("fixture HTTP deadline")
}
async fn raw(address: SocketAddr, wire: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream.write_all(wire.as_bytes()).await.unwrap();
    let mut bytes = vec![];
    let _ = tokio::time::timeout(
        Duration::from_secs(7),
        stream.take(64 * 1024).read_to_end(&mut bytes),
    )
    .await;
    bytes
}

#[tokio::test]
async fn bootstrap_assets_and_cache_are_bounded_and_no_store() {
    let mut f = Fixture::new().await;
    for path in ["/", "/app.js", "/app.css"] {
        let (status, headers, bytes) = f.request(Method::GET, path, vec![], &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert!(bytes.len() <= 64 * 1024);
        assert_eq!(headers["cache-control"], "no-store");
        assert_eq!(headers["x-frame-options"], "DENY");
        assert_eq!(headers["referrer-policy"], "no-referrer");
        assert_eq!(headers["x-content-type-options"], "nosniff");
        assert!(
            headers["content-security-policy"]
                .to_str()
                .unwrap()
                .contains("form-action 'none'")
        );
    }
    let state = f.state().await;
    assert_eq!(state["view"]["amount_mojos"], "18446744073709551615");
    assert_eq!(state["busy"], false);
    assert_eq!(state["job"], Value::Null);
    assert!(
        !serde_json::to_string(&state)
            .unwrap()
            .contains(&f.capability)
    );
    f.close().await;
}
#[tokio::test]
async fn only_closed_routes_and_methods_are_exposed() {
    let mut f = Fixture::new().await;
    for (method, path, status) in [
        (Method::GET, "/api/secret", 404),
        (Method::GET, "/api/action", 405),
        (Method::POST, "/api/state", 405),
        (Method::PUT, "/api/action", 405),
    ] {
        assert_eq!(
            f.request(method, path, vec![], &[]).await.0.as_u16(),
            status
        );
    }
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[tokio::test]
async fn wrong_host_origin_and_fetch_site_cannot_read_state() {
    let mut f = Fixture::new().await;
    assert_eq!(
        send(
            f.address,
            Method::GET,
            "/api/state",
            "localhost",
            &[("x-oxide-session", &f.capability)],
            vec![]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    for headers in [
        vec![("origin", "https://other.example")],
        vec![("sec-fetch-site", "cross-site")],
    ] {
        assert_eq!(
            f.request(Method::GET, "/api/state", vec![], &headers)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    f.close().await;
}
#[tokio::test]
async fn capability_is_required_and_duplicate_capabilities_fail() {
    let mut f = Fixture::new().await;
    assert_eq!(
        send(
            f.address,
            Method::GET,
            "/api/state",
            &f.address.to_string(),
            &[],
            vec![]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            f.address,
            Method::GET,
            "/api/state",
            &f.address.to_string(),
            &[("x-oxide-session", "wrong")],
            vec![]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.request(
            Method::GET,
            "/api/state",
            vec![],
            &[("x-oxide-session", &f.capability)]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    f.close().await;
}
#[tokio::test]
async fn post_requires_exact_explicit_origin() {
    let mut f = Fixture::new().await;
    for headers in [
        vec![("content-type", "application/json")],
        vec![
            ("origin", "https://other.example"),
            ("content-type", "application/json"),
        ],
    ] {
        assert_eq!(
            f.request(Method::POST, "/api/action", b"{}".to_vec(), &headers)
                .await
                .0,
            StatusCode::FORBIDDEN
        );
    }
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[tokio::test]
async fn encoded_and_non_json_post_bodies_are_refused() {
    let mut f = Fixture::new().await;
    for headers in [
        vec![
            ("origin", f.origin.as_str()),
            ("content-type", "text/plain"),
        ],
        vec![
            ("origin", f.origin.as_str()),
            ("content-type", "application/json"),
            ("content-encoding", "gzip"),
        ],
    ] {
        assert_eq!(
            f.request(Method::POST, "/api/action", b"{}".to_vec(), &headers)
                .await
                .0,
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }
    f.close().await;
}
#[tokio::test]
async fn malformed_duplicate_unknown_and_secret_fields_never_queue_actions() {
    let mut f = Fixture::new().await;
    let revision = "00".repeat(32);
    for body in [
        "{".to_owned(),
        "{}".to_owned(),
        format!(r#"{{"action":"reload","action":"reload","revision":"{revision}"}}"#),
        format!(r#"{{"action":"run_shell","revision":"{revision}"}}"#),
        format!(r#"{{"action":"reload","revision":"{revision}","secret":"never-read"}}"#),
        format!(r#"{{"action":"reload","revision":"{revision}","path":"outside"}}"#),
        format!(r#"{{"action":"reload","revision":"{revision}","revision":"{revision}"}}"#),
    ] {
        assert_eq!(
            f.request(
                Method::POST,
                "/api/action",
                body.as_bytes().to_vec(),
                &[("origin", &f.origin), ("content-type", "application/json")]
            )
            .await
            .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[tokio::test]
async fn malformed_and_stale_cached_revision_reject_before_worker() {
    let mut f = Fixture::new().await;
    for revision in ["00", &"GG".repeat(32), &"AA".repeat(32)] {
        assert_eq!(
            f.action_at("reload", revision).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        f.action_at("reload", &"11".repeat(32)).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[tokio::test]
async fn declared_and_streamed_oversized_bodies_are_refused() {
    let mut f = Fixture::new().await;
    let bytes = vec![b' '; 2049];
    assert_eq!(
        f.request(
            Method::POST,
            "/api/action",
            bytes,
            &[("origin", &f.origin), ("content-type", "application/json")]
        )
        .await
        .0,
        StatusCode::PAYLOAD_TOO_LARGE
    );
    let payload = " ".repeat(2049);
    let wire = format!(
        "POST /api/action HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nX-Oxide-Session: {}\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\n\r\n801\r\n{payload}\r\n0\r\n\r\n",
        f.address, f.origin, f.capability
    );
    let reply = raw(f.address, &wire).await;
    assert!(reply.starts_with(b"HTTP/1.1 413"));
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[tokio::test]
async fn get_bodies_queries_and_absolute_targets_are_refused() {
    let mut f = Fixture::new().await;
    assert_eq!(
        f.request(Method::GET, "/api/state", b"x".to_vec(), &[])
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        f.request(Method::GET, "/api/state?extra=1", vec![], &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    let absolute = format!("http://{}/api/state", f.address);
    assert_eq!(
        f.request(Method::GET, &absolute, vec![], &[]).await.0,
        StatusCode::FORBIDDEN
    );
    f.close().await;
}
#[tokio::test]
async fn accepted_job_is_immediate_and_busy_worker_does_not_block_cache() {
    let mut f = Fixture::new().await;
    *f.control.block.lock().unwrap() = true;
    let start = Instant::now();
    let (status, _, body) = f.action("prepare_funding").await;
    assert_eq!(status, StatusCode::ACCEPTED);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert_eq!(
        serde_json::from_slice::<Value>(&body).unwrap()["job_id"],
        "1"
    );
    f.started().await;
    assert_eq!(f.state().await["busy"], true);
    assert_eq!(f.action("submit_funding").await.0, StatusCode::CONFLICT);
    assert_eq!(f.control.calls.load(Ordering::Acquire), 1);
    f.control.release();
    assert_eq!(f.settled().await["job"]["status"], "succeeded");
    f.close().await;
}
#[tokio::test]
async fn job_survives_original_http_connection_lifetime_without_duplicate_queue() {
    let mut f = Fixture::new().await;
    *f.control.block.lock().unwrap() = true;
    assert_eq!(f.action("prepare_payment").await.0, StatusCode::ACCEPTED);
    f.started().await;
    tokio::time::sleep(CONNECTION_TIMEOUT + Duration::from_millis(100)).await;
    assert_eq!(f.state().await["job"]["status"], "running");
    assert_eq!(f.action("prepare_payment").await.0, StatusCode::CONFLICT);
    assert_eq!(f.control.calls.load(Ordering::Acquire), 1);
    f.control.release();
    f.settled().await;
    f.close().await;
}
#[tokio::test]
async fn lost_post_response_still_runs_one_job_and_cache_recovers_its_identity() {
    let mut f = Fixture::new().await;
    *f.control.block.lock().unwrap() = true;
    let body = json!({"action":"submit_funding","revision":"00".repeat(32)}).to_string();
    let wire = format!(
        "POST /api/action HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nX-Oxide-Session: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        f.address,
        f.origin,
        f.capability,
        body.len()
    );
    let mut stream = TcpStream::connect(f.address).await.unwrap();
    stream.write_all(wire.as_bytes()).await.unwrap();
    f.started().await;
    drop(stream);
    assert_eq!(f.state().await["job"]["id"], "1");
    assert_eq!(f.control.calls.load(Ordering::Acquire), 1);
    f.control.release();
    f.settled().await;
    assert_eq!(f.control.calls.load(Ordering::Acquire), 1);
    f.close().await;
}
#[tokio::test]
async fn changed_durable_revision_fails_before_backend_network_and_explicit_reload_recovers() {
    let mut f = Fixture::new().await;
    f.control.durable.store(7, Ordering::Release);
    assert_eq!(f.action("submit_funding").await.0, StatusCode::ACCEPTED);
    let state = f.settled().await;
    assert_eq!(state["job"]["error"]["kind"], "revision_conflict");
    assert_eq!(state["needs_reload"], true);
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    assert_eq!(f.action("prepare_funding").await.0, StatusCode::CONFLICT);
    assert_eq!(f.action("reload").await.0, StatusCode::ACCEPTED);
    let state = f.settled().await;
    assert_eq!(state["view"]["revision"], "07".repeat(32));
    assert_eq!(state["needs_reload"], false);
    f.close().await;
}
#[tokio::test]
async fn postcommit_error_keeps_last_validated_cache_until_explicit_reload() {
    let mut f = Fixture::new().await;
    f.control.fail_after_commit.store(true, Ordering::Release);
    assert_eq!(f.action("submit_payment").await.0, StatusCode::ACCEPTED);
    let state = f.settled().await;
    assert_eq!(state["view"]["revision"], "00".repeat(32));
    assert_eq!(state["needs_reload"], true);
    assert_eq!(f.control.durable.load(Ordering::Acquire), 1);
    assert_eq!(f.action("submit_payment").await.0, StatusCode::CONFLICT);
    assert_eq!(f.action("reload").await.0, StatusCode::ACCEPTED);
    assert_eq!(f.settled().await["view"]["revision"], "01".repeat(32));
    f.close().await;
}
#[tokio::test]
async fn worker_panic_is_inconclusive_releases_permit_and_requires_reload() {
    let mut f = Fixture::new().await;
    f.control.panic_next.store(true, Ordering::Release);
    assert_eq!(f.action("monitor_payment").await.0, StatusCode::ACCEPTED);
    let state = f.settled().await;
    assert_eq!(state["job"]["status"], "failed");
    assert_eq!(state["needs_reload"], true);
    assert_eq!(f.action("reload").await.0, StatusCode::ACCEPTED);
    assert_eq!(f.settled().await["job"]["status"], "succeeded");
    f.close().await;
}
#[tokio::test]
async fn oversized_complete_view_never_replaces_cache_or_gets_truncated() {
    let mut f = Fixture::new().await;
    f.control.huge_next.store(true, Ordering::Release);
    assert_eq!(f.action("monitor_funding").await.0, StatusCode::ACCEPTED);
    let state = f.settled().await;
    assert_eq!(state["job"]["error"]["kind"], "view_limit");
    assert_eq!(state["view"]["revision"], "00".repeat(32));
    assert_eq!(state["needs_reload"], true);
    assert_eq!(f.action("reload").await.0, StatusCode::ACCEPTED);
    f.settled().await;
    f.close().await;
}
#[tokio::test]
async fn shutdown_waits_without_cancelling_started_work_or_blocking_tokio() {
    let mut f = Fixture::new().await;
    *f.control.block.lock().unwrap() = true;
    f.action("prepare_funding").await;
    f.started().await;
    f.stop.take().unwrap().send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!f.task.as_ref().unwrap().is_finished());
    assert_eq!(f.control.calls.load(Ordering::Acquire), 1);
    f.control.release();
    f.close().await;
}
#[tokio::test]
async fn cached_get_polling_never_starts_provider_or_signing_jobs() {
    let mut f = Fixture::new().await;
    for _ in 0..20 {
        assert_eq!(f.state().await["job"], Value::Null);
    }
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[tokio::test]
async fn operation_ids_are_decimal_strings_and_finitely_bounded() {
    let mut f = Fixture::new().await;
    for _ in 0..256 {
        f.shared.worker.accept(LiveAction::Reload, [0; 32]).unwrap();
        while f.shared.worker.snapshot().unwrap().busy {
            tokio::task::yield_now().await;
        }
    }
    assert_eq!(f.state().await["job"]["id"], "256");
    assert_eq!(f.action("reload").await.0, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(f.control.calls.load(Ordering::Acquire), 256);
    f.close().await;
}
#[tokio::test]
async fn incomplete_action_body_expires_without_enqueuing() {
    let mut f = Fixture::new().await;
    let wire = format!(
        "POST /api/action HTTP/1.1\r\nHost: {}\r\nOrigin: {}\r\nX-Oxide-Session: {}\r\nContent-Type: application/json\r\nContent-Length: 10\r\n\r\n{{",
        f.address, f.origin, f.capability
    );
    let reply = raw(f.address, &wire).await;
    assert!(reply.starts_with(b"HTTP/1.1 408"));
    assert_eq!(f.control.calls.load(Ordering::Acquire), 0);
    f.close().await;
}
#[test]
fn error_strings_have_utf8_safe_byte_bounds() {
    let error = ApiError::new(&"é".repeat(100), &"🦀".repeat(200));
    assert!(error.kind.len() <= 64 && error.message.len() <= 512);
    serde_json::to_string(&error).unwrap();
}
