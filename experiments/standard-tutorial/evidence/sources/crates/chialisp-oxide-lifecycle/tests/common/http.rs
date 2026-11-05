//! Controlled HTTP client for tests. Never print response bodies/headers: the
//! bootstrap page deliberately carries an in-memory session capability.
use bytes::Bytes;
use http_body_util::{BodyExt, Full, Limited};
use hyper::{HeaderMap, Method, Request, StatusCode, client::conn::http1};
use hyper_util::rt::TokioIo;
use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpStream;

pub async fn send(
    address: SocketAddr,
    method: Method,
    path: &str,
    host: &str,
    headers: &[(&str, &str)],
    body: Vec<u8>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let result = tokio::time::timeout(Duration::from_secs(10), async {
        let stream = TcpStream::connect(address)
            .await
            .expect("test loopback connect failed");
        let transport = http1::handshake(TokioIo::new(stream)).await;
        assert!(transport.is_ok(), "test HTTP handshake failed");
        let (mut sender, connection) = transport.unwrap();
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
        let request = builder.body(Full::new(Bytes::from(body)));
        assert!(request.is_ok(), "test request construction failed");
        let response = sender.send_request(request.unwrap()).await;
        assert!(response.is_ok(), "test HTTP exchange failed");
        let (parts, body) = response.unwrap().into_parts();
        let collected = Limited::new(body, 64 * 1024).collect().await;
        assert!(collected.is_ok(), "test response read failed");
        assert!(task.await.is_ok(), "test transport task failed");
        (
            parts.status,
            parts.headers,
            collected.unwrap().to_bytes().to_vec(),
        )
    })
    .await;
    assert!(result.is_ok(), "test HTTP exchange exceeded its deadline");
    result.unwrap()
}

pub fn capability(page: &[u8]) -> String {
    let text = std::str::from_utf8(page).expect("bootstrap HTML must be UTF-8");
    let (_, suffix) = text
        .split_once("name=\"oxide-session\" content=\"")
        .expect("bootstrap capability metadata missing");
    let (capability, _) = suffix
        .split_once('"')
        .expect("bootstrap capability metadata incomplete");
    assert!(
        capability.len() == 64 && capability.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "invalid bootstrap capability shape"
    );
    capability.to_owned()
}
