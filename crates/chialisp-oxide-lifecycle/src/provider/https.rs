//! Fixed-origin read-only HTTPS. No arbitrary URL, secret header or push API.
use super::*;
use std::time::Duration;

/// Synchronous owner of a private async client/runtime. Construct, call and
/// drop outside an entered Tokio runtime; an ordinary dedicated thread is
/// suitable. GUI integration must retain its operation permit until it ends.
pub struct Testnet11Https {
    client: reqwest::Client,
    runtime: Option<tokio::runtime::Runtime>,
    origin: reqwest::Url,
}
impl Testnet11Https {
    pub fn new() -> Result<Self> {
        outside_runtime()?;
        Self::build(
            reqwest::Url::parse(TESTNET11_ORIGIN)
                .map_err(|_| Error::new(ErrorKind::Transport, "invalid fixed provider origin"))?,
            true,
            Duration::from_secs(5),
            Duration::from_secs(15),
        )
    }
    fn build(origin: reqwest::Url, https: bool, idle: Duration, total: Duration) -> Result<Self> {
        outside_runtime()?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| Error::new(ErrorKind::Transport, "provider runtime unavailable"))?;
        let client = runtime.block_on(async {
            reqwest::Client::builder()
                .use_rustls_tls()
                .https_only(https)
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .no_proxy()
                .connect_timeout(idle)
                .read_timeout(idle)
                .timeout(total)
                .pool_max_idle_per_host(1)
                .user_agent("chialisp-oxide-lifecycle/WIP-read-only")
                .build()
                .map_err(transport_error)
        })?;
        Ok(Self {
            client,
            runtime: Some(runtime),
            origin,
        })
    }
}
impl Drop for Testnet11Https {
    fn drop(&mut self) {
        // A caller may move an idle backend into an async context. A normal
        // Runtime drop there can panic; asynchronous shutdown never blocks it.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}
fn outside_runtime() -> Result<()> {
    if tokio::runtime::Handle::try_current().is_ok() {
        return Err(Error::new(
            ErrorKind::Transport,
            "synchronous provider requires an ordinary thread outside an entered async runtime",
        ));
    }
    Ok(())
}
fn transport_error(error: reqwest::Error) -> Error {
    Error::new(
        if error.is_timeout() {
            ErrorKind::Timeout
        } else {
            ErrorKind::Transport
        },
        if error.is_timeout() {
            "provider HTTP deadline exceeded"
        } else {
            "provider HTTP transport failed"
        },
    )
}
impl Transport for Testnet11Https {
    fn post(&mut self, request: &Request) -> Result<Response> {
        outside_runtime()?;
        if request.body().len() > MAX_REQUEST {
            return Err(Error::new(
                ErrorKind::RequestLimit,
                "provider request byte limit",
            ));
        }
        let url = self
            .origin
            .join(request.endpoint().path())
            .map_err(|_| Error::new(ErrorKind::Transport, "fixed endpoint URL"))?;
        self.runtime
            .as_ref()
            .ok_or_else(|| Error::new(ErrorKind::Transport, "provider runtime unavailable"))?
            .block_on(async {
                let mut response = self
                    .client
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .header(reqwest::header::ACCEPT, "application/json")
                    .body(request.body().to_vec())
                    .send()
                    .await
                    .map_err(transport_error)?;
                if response
                    .content_length()
                    .is_some_and(|length| length > MAX_BODY as u64)
                {
                    return Err(Error::new(
                        ErrorKind::BodyLimit,
                        "provider declared oversized response",
                    ));
                }
                let status = response.status().as_u16();
                let mut body = BodyCollector::default();
                // Never call json(), text(), bytes() or unrestricted body collect.
                // reqwest's read and total deadlines remain attached during chunks.
                while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
                    body.push(&chunk)?;
                }
                Ok(Response {
                    status,
                    body: body.finish()?,
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn request() -> Request {
        Request {
            endpoint: Endpoint::GetNetworkInfo,
            body: b"{}".to_vec(),
        }
    }
    fn read_request(stream: &mut std::net::TcpStream) {
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = vec![];
        let mut buffer = [0; 1024];
        loop {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0);
            request.extend_from_slice(&buffer[..count]);
            assert!(request.len() <= 8192);
            if let Some(index) = request.windows(4).position(|value| value == b"\r\n\r\n")
                && request.len() >= index + 4 + 2
            {
                break;
            }
        }
        assert!(request.starts_with(b"POST /get_network_info HTTP/1.1\r\n"));
    }
    fn server(response: Vec<u8>, delay: Duration) -> (reqwest::Url, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin =
            reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            read_request(&mut stream);
            thread::sleep(delay);
            let _ = stream.write_all(&response);
        });
        (origin, handle)
    }
    fn local(origin: reqwest::Url) -> Testnet11Https {
        // Private cfg(test) origin only. Production construction accepts none.
        Testnet11Https::build(
            origin,
            false,
            Duration::from_secs(5),
            Duration::from_secs(15),
        )
        .unwrap()
    }
    fn short_deadline(origin: reqwest::Url) -> Testnet11Https {
        // Short timeouts belong only to deliberate timeout controls.
        Testnet11Https::build(
            origin,
            false,
            Duration::from_millis(200),
            Duration::from_millis(600),
        )
        .unwrap()
    }
    #[test]
    fn actual_http_chunked_body_without_content_length_is_bounded() {
        let (origin,handle)=server(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n".to_vec(),Duration::ZERO);
        let mut client = local(origin);
        let response = client.post(&request()).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.body.as_bytes(), b"{\"a\":1}");
        handle.join().unwrap();
        let mut wire =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n".to_vec();
        for _ in 0..17 {
            wire.extend_from_slice(b"1000\r\n");
            wire.extend_from_slice(&[b'x'; 4096]);
            wire.extend_from_slice(b"\r\n");
        }
        wire.extend_from_slice(b"0\r\n\r\n");
        let (origin, handle) = server(wire, Duration::ZERO);
        assert_eq!(
            local(origin).post(&request()).err().unwrap().kind,
            ErrorKind::BodyLimit
        );
        handle.join().unwrap();
    }
    #[test]
    fn actual_http_status_and_redirect_are_not_followed() {
        for status in [302, 413, 429, 503] {
            let wire=format!("HTTP/1.1 {status} Status\r\nLocation: http://127.0.0.1:1/never\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{{}}").into_bytes();
            let (origin, handle) = server(wire, Duration::ZERO);
            let response = local(origin).post(&request()).unwrap();
            assert_eq!(response.status, status);
            handle.join().unwrap();
        }
    }
    #[test]
    fn actual_http_read_deadline_and_declared_body_limit_fail() {
        let (origin, handle) = server(
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_vec(),
            Duration::from_millis(350),
        );
        assert_eq!(
            short_deadline(origin).post(&request()).err().unwrap().kind,
            ErrorKind::Timeout
        );
        handle.join().unwrap();
        let (origin, handle) = server(
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                MAX_BODY + 1
            )
            .into_bytes(),
            Duration::ZERO,
        );
        assert_eq!(
            local(origin).post(&request()).err().unwrap().kind,
            ErrorKind::BodyLimit
        );
        handle.join().unwrap();
    }
    #[test]
    fn entered_runtime_is_rejected_without_network_or_nested_runtime_panic() {
        let client = Testnet11Https::new().unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            assert_eq!(
                Testnet11Https::new().err().unwrap().kind,
                ErrorKind::Transport
            );
            let mut client = client;
            assert_eq!(
                client.post(&request()).err().unwrap().kind,
                ErrorKind::Transport
            );
            drop(client);
        });
    }
    #[test]
    fn actual_http_slow_body_and_continuous_chunks_keep_read_and_total_deadlines() {
        for continuous in [false, true] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin =
                reqwest::Url::parse(&format!("http://{}/", listener.local_addr().unwrap()))
                    .unwrap();
            let handle = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                if continuous {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").unwrap();
                    for _ in 0..25 {
                        if stream.write_all(b"1\r\nx\r\n").is_err() {
                            break;
                        }
                        thread::sleep(Duration::from_millis(50));
                    }
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n",
                        )
                        .unwrap();
                    thread::sleep(Duration::from_millis(350));
                    let _ = stream.write_all(b"{}");
                }
            });
            assert_eq!(
                short_deadline(origin).post(&request()).err().unwrap().kind,
                ErrorKind::Timeout
            );
            handle.join().unwrap();
        }
    }
}
