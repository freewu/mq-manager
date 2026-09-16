use rdkafka::config::ClientConfig;
use std::time::Duration;

use crate::error::{AppError, AppResult};
use crate::mq::types::ConnectionProfile;

pub const SECURITY_PROTOCOLS: &[(&str, &str)] = &[
    ("PLAINTEXT", "PLAINTEXT"),
    ("SSL", "SSL"),
    ("SASL_PLAINTEXT", "SASL_PLAINTEXT"),
    ("SASL_SSL", "SASL_SSL"),
];

pub const SASL_MECHANISMS: &[(&str, &str)] = &[
    ("PLAIN", "PLAIN"),
    ("SCRAM-SHA-256", "SCRAM-SHA-256"),
    ("SCRAM-SHA-512", "SCRAM-SHA-512"),
];

/// Whether this build can speak TLS. Decided at compile time by the `tls` feature.
pub const TLS_SUPPORTED: bool = cfg!(feature = "tls");

#[derive(Debug, Clone)]
pub struct KafkaOptions {
    pub bootstrap_servers: String,
    pub client_id: String,
    pub security_protocol: String,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    pub ssl_ca_location: Option<String>,
    pub ssl_certificate_location: Option<String>,
    pub ssl_key_location: Option<String>,
    pub ssl_key_password: Option<String>,
    pub enable_ssl_verification: bool,
    pub request_timeout_ms: u64,
    pub socket_timeout_ms: u64,
    pub message_max_bytes: u64,
    /// Raw librdkafka properties supplied by the user, they win over everything else.
    pub extra: Vec<(String, String)>,
    /// Human readable target, used for the window title / status bar.
    pub target: String,
}

impl KafkaOptions {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        let bootstrap_servers = profile
            .option_str("bootstrapServers")
            .ok_or_else(|| AppError::invalid("bootstrapServers is required"))?;

        let security_protocol = profile
            .option_str("securityProtocol")
            .unwrap_or_else(|| "PLAINTEXT".to_string())
            .to_ascii_uppercase();

        let uses_sasl = security_protocol.starts_with("SASL");
        let uses_tls = security_protocol.ends_with("SSL");

        if uses_tls && !TLS_SUPPORTED {
            return Err(AppError::invalid(
                "this build was compiled without TLS support — rebuild with `just build --features tls`",
            ));
        }

        let sasl_mechanism = uses_sasl.then(|| {
            profile
                .option_str("saslMechanism")
                .unwrap_or_else(|| "PLAIN".to_string())
                .to_ascii_uppercase()
        });

        Ok(Self {
            bootstrap_servers: bootstrap_servers.clone(),
            client_id: profile
                .option_str("clientId")
                .unwrap_or_else(|| "mq-manager".to_string()),
            security_protocol,
            sasl_mechanism,
            sasl_username: profile.option_str("saslUsername"),
            sasl_password: profile.option_str("saslPassword"),
            ssl_ca_location: profile.option_str("sslCaLocation"),
            ssl_certificate_location: profile.option_str("sslCertificateLocation"),
            ssl_key_location: profile.option_str("sslKeyLocation"),
            ssl_key_password: profile.option_str("sslKeyPassword"),
            enable_ssl_verification: profile.option_bool("enableSslCertificateVerification", true),
            request_timeout_ms: profile.option_u64("requestTimeoutMs", 30_000).max(1_000),
            socket_timeout_ms: profile.option_u64("socketTimeoutMs", 60_000).max(1_000),
            message_max_bytes: profile.option_u64("messageMaxBytes", 1_048_576).max(1_000),
            extra: profile.option_map("additionalProperties"),
            target: bootstrap_servers,
        })
    }

    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms)
    }

    /// Metadata / admin calls get a slightly longer budget than a ping.
    pub fn admin_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms + self.socket_timeout_ms.min(30_000))
    }

    pub fn client_config(&self) -> ClientConfig {
        let mut config = ClientConfig::new();
        config
            .set("bootstrap.servers", &self.bootstrap_servers)
            .set("client.id", &self.client_id)
            .set("security.protocol", &self.security_protocol)
            .set("api.version.request", "true")
            .set("request.timeout.ms", self.request_timeout_ms.to_string())
            .set("socket.timeout.ms", self.socket_timeout_ms.to_string())
            .set("message.max.bytes", self.message_max_bytes.to_string())
            .set(
                "enable.ssl.certificate.verification",
                self.enable_ssl_verification.to_string(),
            );

        if let Some(mechanism) = &self.sasl_mechanism {
            config.set("sasl.mechanism", mechanism);
            if let Some(username) = &self.sasl_username {
                config.set("sasl.username", username);
            }
            if let Some(password) = &self.sasl_password {
                config.set("sasl.password", password);
            }
        }

        if let Some(value) = &self.ssl_ca_location {
            config.set("ssl.ca.location", value);
        }
        if let Some(value) = &self.ssl_certificate_location {
            config.set("ssl.certificate.location", value);
        }
        if let Some(value) = &self.ssl_key_location {
            config.set("ssl.key.location", value);
        }
        if let Some(value) = &self.ssl_key_password {
            config.set("ssl.key.password", value);
        }

        for (key, value) in &self.extra {
            config.set(key, value);
        }

        config
    }
}
