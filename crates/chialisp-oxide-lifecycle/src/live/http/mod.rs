//! Explicit live loopback GUI. HTTP can select only fixed actions over the
//! process-local fresh recipe; it cannot provide keys, programs, paths or fees.
mod service;
mod worker;

use crate::http::{
    CONNECTION_TIMEOUT, HEADER_TIMEOUT, MAX_CONNECTIONS, MAX_HEADER_BYTES, MAX_HEADERS,
};
use crate::live::LiveSession;
use anyhow::{Result, ensure};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};
pub use worker::{ApiError, JobStatus, JobView, LiveAction};

struct Shared {
    origin: String,
    host: String,
    capability: String,
    worker: worker::Controller,
}

pub struct LiveLoopbackServer {
    listener: TcpListener,
    shared: Arc<Shared>,
    connections: Arc<Semaphore>,
}

impl LiveLoopbackServer {
    /// A private fresh LiveSession was built by the CLI before this call. The
    /// ordinary worker validates current durable intent before serving a page.
    pub async fn bind(session: LiveSession) -> Result<Self> {
        let worker = worker::Controller::production(session).await?;
        Self::with_worker(worker).await
    }
    async fn with_worker(worker: worker::Controller) -> Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
        let address = listener.local_addr()?;
        ensure!(
            address.ip() == Ipv4Addr::LOCALHOST,
            "live GUI must use IPv4 loopback"
        );
        let mut bytes = [0; 32];
        getrandom::fill(&mut bytes)?;
        let shared = Shared {
            origin: format!("http://{address}"),
            host: address.to_string(),
            capability: hex::encode(bytes),
            worker,
        };
        Ok(Self {
            listener,
            shared: Arc::new(shared),
            connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        })
    }
    /// The plain URL contains no capability or wallet material.
    pub fn url(&self) -> &str {
        &self.shared.origin
    }
    pub fn address(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }
    pub async fn run_until(self, shutdown: impl Future<Output = ()>) -> Result<()> {
        tokio::pin!(shutdown);
        let mut tasks = JoinSet::new();
        let outcome = loop {
            tokio::select! {
                _ = &mut shutdown => break Ok(()),
                _ = tasks.join_next(), if !tasks.is_empty() => (),
                accepted = self.listener.accept() => {
                    let (stream, peer) = match accepted {
                        Ok(value) => value,
                        Err(error) => break Err(error.into()),
                    };
                    if !peer.ip().is_loopback() { continue; }
                    let Ok(permit) = self.connections.clone().try_acquire_owned() else { continue; };
                    let shared = self.shared.clone();
                    tasks.spawn(async move {
                        let _permit = permit;
                        let service = service_fn(move |request| service::handle(request, shared.clone()));
                        let mut builder = http1::Builder::new();
                        builder.keep_alive(false).max_headers(MAX_HEADERS).max_buf_size(MAX_HEADER_BYTES)
                            .timer(TokioTimer::new()).header_read_timeout(HEADER_TIMEOUT);
                        // Never log headers, response HTML or request bodies:
                        // the page contains a session-local capability.
                        let _ = tokio::time::timeout(CONNECTION_TIMEOUT,
                            builder.serve_connection(TokioIo::new(stream), service)).await;
                    });
                }
            }
        };
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        // This awaits a notification; it never joins a blocking thread on Tokio.
        // A started signing/network job must finish or be cancelled locally.
        self.shared.worker.shutdown().await;
        outcome
    }
}

#[cfg(test)]
mod tests;
