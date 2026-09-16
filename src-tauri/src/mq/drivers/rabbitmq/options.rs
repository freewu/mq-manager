//! Connection options for the RabbitMQ driver.
//!
//! Two endpoints are involved: the AMQP 0-9-1 listener (default `5672`, `5671`
//! for TLS) and the management HTTP API (`15672`) which is what makes a queue
//! browsable from a GUI. Only the former is strictly required.

use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::mq::types::ConnectionProfile;

pub const DEFAULT_AMQP_PORT: u16 = 5672;
pub const DEFAULT_AMQPS_PORT: u16 = 5671;
pub const DEFAULT_MANAGEMENT_PORT: u16 = 15672;
pub const DEFAULT_MANAGEMENT_TLS_PORT: u16 = 15671;

/// Everything the driver needs, resolved from the connection profile.
#[derive(Debug, Clone)]
pub struct RabbitMqOptions {
    pub host: String,
    pub port: u16,
    pub vhost: String,
    pub username: String,
    pub password: String,
    pub use_tls: bool,
    /// Optional explicit management API base url, e.g. `http://rabbit-mgmt:15672`.
    pub management_url: Option<String>,
    pub management_username: String,
    pub management_password: String,
    /// `false` disables the management API entirely (publish-only connections).
    pub management_enabled: bool,
    pub connection_name: String,
    pub heartbeat: u16,
    pub timeout: Duration,
}

impl RabbitMqOptions {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        let host = profile
            .option_str("host")
            .unwrap_or_else(|| "localhost".to_string());
        let use_tls = profile.option_bool("useTls", false);
        let default_port = if use_tls {
            DEFAULT_AMQPS_PORT
        } else {
            DEFAULT_AMQP_PORT
        };
        let port = profile.option_u64("port", default_port as u64) as u16;
        let username = profile
            .option_str("username")
            .unwrap_or_else(|| "guest".to_string());
        let password = profile
            .option_str("password")
            .unwrap_or_else(|| "guest".to_string());
        let vhost = profile
            .option_str("vhost")
            .unwrap_or_else(|| "/".to_string());

        let management_username = profile
            .option_str("managementUsername")
            .unwrap_or_else(|| username.clone());
        let management_password = profile
            .option_str("managementPassword")
            .unwrap_or_else(|| password.clone());

        Ok(Self {
            host,
            port,
            vhost,
            username,
            password,
            use_tls,
            management_url: profile.option_str("managementUrl"),
            management_username,
            management_password,
            management_enabled: profile.option_bool("managementApi", true),
            connection_name: profile
                .option_str("connectionName")
                .unwrap_or_else(|| "mq-manager".to_string()),
            heartbeat: profile.option_u64("heartbeat", 60) as u16,
            timeout: Duration::from_millis(profile.option_u64("timeoutMs", 15_000).max(1_000)),
        })
    }

    /// `host:port`, used for banners and error messages.
    pub fn authority(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }

    /// The `amqp(s)://` URI handed to the AMQP client.
    ///
    /// Built by hand because the WHATWG url parser refuses credentials on
    /// non-special schemes such as `amqp`.
    pub fn amqp_uri(&self) -> String {
        let scheme = if self.use_tls { "amqps" } else { "amqp" };
        let host = if self.host.contains(':') && !self.host.starts_with('[') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let mut uri = format!(
            "{scheme}://{}:{}@{}:{}/{}",
            encode(&self.username),
            encode(&self.password),
            host,
            self.port,
            encode(&self.vhost),
        );
        let mut query: Vec<String> = Vec::new();
        if self.heartbeat > 0 {
            query.push(format!("heartbeat={}", self.heartbeat));
        }
        query.push(format!("connection_timeout={}", self.timeout.as_millis()));
        if !query.is_empty() {
            uri.push('?');
            uri.push_str(&query.join("&"));
        }
        uri
    }

    /// Management API base url, always with a trailing slash.
    pub fn management_base(&self) -> Option<String> {
        if !self.management_enabled {
            return None;
        }
        let explicit = self
            .management_url
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let base = match explicit {
            Some(url) => url.to_string(),
            None => {
                let (scheme, port) = if self.use_tls {
                    ("https", DEFAULT_MANAGEMENT_TLS_PORT)
                } else {
                    ("http", DEFAULT_MANAGEMENT_PORT)
                };
                let host = if self.host.contains(':') && !self.host.starts_with('[') {
                    format!("[{}]", self.host)
                } else {
                    self.host.clone()
                };
                format!("{scheme}://{host}:{port}")
            }
        };
        Some(if base.ends_with('/') {
            base
        } else {
            format!("{base}/")
        })
    }

    /// Credentials for the management API, if any.
    pub fn management_credentials(&self) -> (String, String) {
        (
            self.management_username.clone(),
            self.management_password.clone(),
        )
    }

    pub fn client(&self) -> AppResult<reqwest::Client> {
        // A `reqwest` client with the `rustls-no-provider` feature needs a
        // process wide provider; installing it here (idempotently) keeps the
        // driver usable from tests and from the binary alike.
        crate::install_crypto_provider();
        reqwest::Client::builder()
            .timeout(self.timeout)
            .connect_timeout(self.timeout)
            .user_agent(concat!("mq-manager/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| AppError::broker(format!("could not create HTTP client: {error}")))
    }
}

/// RFC 3986 unreserved characters only — safe for user info and URI path
/// segments (so the default vhost `/` becomes `%2F`).
pub fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_reserved_characters() {
        assert_eq!(encode("/"), "%2F");
        assert_eq!(encode("a b"), "a%20b");
        assert_eq!(encode("keep-._~"), "keep-._~");
    }
}
