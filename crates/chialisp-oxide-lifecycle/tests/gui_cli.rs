//! Real CLI process and real loopback HTTP; never dump HTML/session headers.
#[path = "common/http.rs"]
mod client;
use hyper::{Method, StatusCode};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader},
    net::SocketAddr,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test]
async fn serve_cli_prints_only_plain_url_and_gui_flow_uses_the_saved_runner() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .canonicalize()
        .unwrap()
        .join("serve.lifecycle.json");
    let mut process = Process(
        Command::new(env!("CARGO_BIN_EXE_chialisp-oxide-lifecycle"))
            .args(["simulator", "serve", "--journal"])
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let stdout = process.0.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        if reader.read_line(&mut line).is_ok() {
            let _ = tx.send(line);
        }
    });
    let line = rx
        .recv_timeout(Duration::from_secs(15))
        .expect("server did not announce a plain loopback URL");
    reader.join().unwrap();
    let origin = line.trim();
    assert!(
        origin.starts_with("http://127.0.0.1:") && origin.split_whitespace().count() == 1,
        "server output was not a plain loopback URL"
    );
    let address: SocketAddr = origin
        .strip_prefix("http://")
        .unwrap()
        .parse()
        .expect("announced loopback address invalid");
    assert!(address.ip().is_loopback());
    let (status, _, page) =
        client::send(address, Method::GET, "/", &address.to_string(), &[], vec![]).await;
    assert_eq!(status, StatusCode::OK);
    let capability = client::capability(&page);
    assert!(!line.contains(&capability), "capability was printed by CLI");
    for revision in 0..7 {
        let (status, _, body) = client::send(
            address,
            Method::POST,
            "/api/commands",
            &address.to_string(),
            &[
                ("origin", origin),
                ("x-oxide-session", &capability),
                ("content-type", "application/json"),
            ],
            serde_json::to_vec(&json!({"revision":revision,"command":"advance"})).unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        if revision == 6 {
            let state: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(state["view"]["stage"], "confirmed");
            assert_eq!(state["view"]["children"].as_array().unwrap().len(), 1);
            assert_eq!(state["view"]["checks"]["live_network"], "unavailable");
        }
    }
    let saved = chialisp_oxide_lifecycle::store::JournalStore::new(&path)
        .unwrap()
        .load()
        .unwrap();
    assert_eq!(
        saved.journal.stage(),
        chialisp_oxide_lifecycle::Stage::Confirmed
    );
    assert!(
        !String::from_utf8_lossy(&std::fs::read(&path).unwrap()).contains(&capability),
        "capability was persisted"
    );
    assert!(process.0.try_wait().unwrap().is_none());
}
