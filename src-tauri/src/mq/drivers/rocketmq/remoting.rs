//! Minimal RocketMQ remoting client over TCP.
//!
//! One serialized connection is kept per broker/nameserver address: the
//! remoting protocol has no request multiplexing on the client side that we can
//! rely on, so requests to the same address are sent one at a time. A failed
//! read drops the socket, which keeps a timed-out request from desynchronising
//! the next one.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex as AsyncMutex;

use super::protocol::{response_code, RemotingCommand};
use crate::error::{AppError, AppResult};

/// Timeout for the initial TCP handshake.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

pub struct RemotingClient {
    connections: Mutex<HashMap<String, Arc<AsyncMutex<TcpStream>>>>,
    opaque: AtomicI32,
    timeout: Duration,
}

impl RemotingClient {
    pub fn new(timeout: Duration) -> Self {
        Self {
            connections: Mutex::new(HashMap::new()),
            opaque: AtomicI32::new(1),
            timeout,
        }
    }

    fn next_opaque(&self) -> i32 {
        // Wrap well before i32::MAX; RocketMQ compares opaques for equality.
        let next = self.opaque.fetch_add(1, Ordering::Relaxed);
        if next > 1_000_000 {
            self.opaque.store(1, Ordering::Relaxed);
            1
        } else {
            next
        }
    }

    /// Drop every pooled connection (used when the profile is closed).
    pub fn clear(&self) {
        self.connections.lock().clear();
    }

    /// Send a request and wait for the matching response.
    pub async fn invoke(
        &self,
        addr: &str,
        mut command: RemotingCommand,
    ) -> AppResult<RemotingCommand> {
        let opaque = self.next_opaque();
        command.opaque = opaque;
        let stream = self.stream(addr).await?;
        // Hold the per-address lock for the whole exchange.
        let mut socket = stream.lock().await;
        let frame = command.encode_frame()?;

        let exchange = async {
            socket.write_all(&frame).await?;
            socket.flush().await?;
            loop {
                let mut prefix = [0u8; 4];
                socket.read_exact(&mut prefix).await?;
                let len = RemotingCommand::frame_len(prefix)?;
                let mut payload = vec![0u8; len];
                socket.read_exact(&mut payload).await?;
                let response = RemotingCommand::decode(&payload)?;
                if response.flag & 1 == 1 {
                    // A response.
                    if response.opaque == opaque {
                        return Ok::<RemotingCommand, AppError>(response);
                    }
                    return Err(AppError::broker(
                        "the broker answered a different request on this connection",
                    ));
                }
                // The broker asked us something (heartbeat notification and
                // friends). We are a management client: refuse politely and go
                // back to waiting for our own response.
                let refusal = RemotingCommand {
                    code: response_code::REQUEST_CODE_NOT_SUPPORTED,
                    language: "OTHER".into(),
                    version: response.version,
                    opaque: response.opaque,
                    flag: 1,
                    remark: Some("not supported by mq-manager".into()),
                    ext_fields: Default::default(),
                    body: Vec::new(),
                };
                let _ = socket.write_all(&refusal.encode_frame()?).await;
                let _ = socket.flush().await;
            }
        };

        match tokio::time::timeout(self.timeout, exchange).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(error)) => {
                self.drop_connection(addr);
                Err(error)
            }
            Err(_) => {
                self.drop_connection(addr);
                Err(AppError::Timeout(self.timeout.as_millis() as u64))
            }
        }
    }

    /// Convenience wrapper: build a request from a code plus header fields.
    pub async fn request(
        &self,
        addr: &str,
        code: i32,
        header: &[(&str, String)],
    ) -> AppResult<RemotingCommand> {
        let mut command = RemotingCommand::request(code, 0, Default::default());
        for (key, value) in header {
            command.ext_fields.insert((*key).to_string(), value.clone());
        }
        self.invoke(addr, command).await
    }

    /// Check that `addr` accepts TCP connections.
    pub async fn touch(&self, addr: &str) -> AppResult<()> {
        self.stream(addr).await.map(|_| ())
    }

    fn drop_connection(&self, addr: &str) {
        self.connections.lock().remove(addr);
    }

    async fn stream(&self, addr: &str) -> AppResult<Arc<AsyncMutex<TcpStream>>> {
        if let Some(stream) = self.connections.lock().get(addr).cloned() {
            return Ok(stream);
        }
        let socket = tokio::time::timeout(CONNECT_TIMEOUT, TcpStream::connect(addr))
            .await
            .map_err(|_| AppError::Timeout(CONNECT_TIMEOUT.as_millis() as u64))?
            .map_err(|error| {
                AppError::broker(format!("could not reach the RocketMQ node {addr}: {error}"))
            })?;
        let _ = socket.set_nodelay(true);
        let stream = Arc::new(AsyncMutex::new(socket));
        self.connections
            .lock()
            .insert(addr.to_string(), Arc::clone(&stream));
        Ok(stream)
    }
}

impl std::fmt::Debug for RemotingClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RemotingClient")
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// Split `host:port` (or `host` with a default port) into a socket address.
pub fn socket_addr(host: &str, default_port: u16) -> String {
    let host = host.trim();
    if host
        .rsplit_once(':')
        .map(|(_, port)| port.parse::<u16>().is_ok())
        == Some(true)
    {
        return host.to_string();
    }
    format!("{host}:{default_port}")
}

/// `true` when a response code means "the object is absent" rather than "broken".
pub fn is_missing(response_code: i32) -> bool {
    matches!(
        response_code,
        response_code::TOPIC_NOT_EXIST
            | response_code::QUERY_NOT_FOUND
            | response_code::SUBSCRIPTION_GROUP_NOT_EXIST
            | response_code::CONSUMER_NOT_ONLINE
            | response_code::PULL_NOT_FOUND
            | response_code::NO_MESSAGE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_socket_addresses() {
        assert_eq!(socket_addr("10.0.0.1", 9876), "10.0.0.1:9876");
        assert_eq!(socket_addr("10.0.0.1:10911", 9876), "10.0.0.1:10911");
        assert_eq!(socket_addr(" [::1] ", 9876), "[::1]:9876");
        assert_eq!(
            socket_addr("broker.internal", 10911),
            "broker.internal:10911"
        );
    }
}
