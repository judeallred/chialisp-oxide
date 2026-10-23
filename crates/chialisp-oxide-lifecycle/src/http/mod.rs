//! Local simulator GUI. The session capability prevents cross-origin browser
//! actions; it does not authenticate against other processes on this machine.
mod policy;
mod service;
mod session;

use crate::store::JournalStore;
use anyhow::{Result, ensure};
use hyper::{server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use std::{
    future::Future,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{net::TcpListener, sync::Semaphore, task::JoinSet};

pub(super) const MAX_CONNECTIONS: usize = 8;
pub(super) const MAX_HEADERS: usize = 24;
pub(super) const MAX_HEADER_BYTES: usize = 8192;
pub(super) const MAX_BODY: usize = 2048;
pub(super) const HEADER_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const BODY_TIMEOUT: Duration = Duration::from_secs(2);
pub(super) const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

pub struct LoopbackServer {
    listener: TcpListener,
    shared: Arc<session::Shared>,
    connections: Arc<Semaphore>,
}

impl LoopbackServer {
    /// Port and loopback address are chosen here, never from HTTP input.
    pub async fn bind(store: JournalStore) -> Result<Self> {
        Self::bind_with_recipe(
            store,
            crate::runner::SimulatorRunner::new()?.validated_recipe(),
        )
        .await
    }

    /// The recipe was checked before the listener starts; HTTP cannot supply
    /// tools, filesystem paths, saved receipts or unchecked events.
    pub async fn bind_with_recipe(
        store: JournalStore,
        recipe: crate::runner::ValidatedRecipe,
    ) -> Result<Self> {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).await?;
        let address = listener.local_addr()?;
        ensure!(
            address.ip() == std::net::IpAddr::V4(Ipv4Addr::LOCALHOST),
            "server must bind IPv4 loopback"
        );
        let shared = session::Shared::with_recipe(store, address, recipe)?;
        Ok(Self {
            listener,
            shared: Arc::new(shared),
            connections: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
        })
    }

    /// Plain URL only. No capability is exposed in this or the journal.
    pub fn url(&self) -> &str {
        &self.shared.origin
    }
    pub fn address(&self) -> Result<SocketAddr> {
        Ok(self.listener.local_addr()?)
    }

    pub async fn run_until(self, shutdown: impl Future<Output = ()>) -> Result<()> {
        tokio::pin!(shutdown);
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                _ = tasks.join_next(), if !tasks.is_empty() => (),
                accepted = self.listener.accept() => {
                    let (stream, peer) = accepted?;
                    if !peer.ip().is_loopback() { continue; }
                    let Ok(permit) = self.connections.clone().try_acquire_owned() else { continue; };
                    let shared = self.shared.clone();
                    tasks.spawn(async move {
                        let _permit = permit;
                        let service = service_fn(move |request| service::handle(request, shared.clone()));
                        let mut builder = http1::Builder::new();
                        builder.keep_alive(false).max_headers(MAX_HEADERS).max_buf_size(MAX_HEADER_BYTES)
                            .timer(TokioTimer::new()).header_read_timeout(HEADER_TIMEOUT);
                        // No request/header/response logging: the page contains
                        // an in-memory session capability. Parser/timeout errors
                        // close only this connection and never run a command.
                        let _ = tokio::time::timeout(CONNECTION_TIMEOUT, builder.serve_connection(TokioIo::new(stream), service)).await;
                    });
                }
            }
        }
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
        // A started blocking filesystem operation is not cancellable. It keeps
        // the single operation permit until completion. On timeout/reconnect,
        // clients must reload durable revision before deciding to retry.
        Ok(())
    }
}

#[cfg(test)]
mod tests;
