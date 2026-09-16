//! Connection options for the RocketMQ driver.
//!
//! A RocketMQ client only needs the nameserver list: brokers are discovered
//! through it. Broker addresses found in route information are used as-is
//! (`host:port`), the default port here only applies to bare host names.

use std::time::Duration;

use super::remoting::socket_addr;
use crate::error::{AppError, AppResult};
use crate::mq::types::ConnectionProfile;

/// RocketMQ nameserver port.
pub const DEFAULT_NAMESERVER_PORT: u16 = 9876;
/// RocketMQ broker listener port.
pub const DEFAULT_BROKER_PORT: u16 = 10911;
/// Default per request timeout.
pub const DEFAULT_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone)]
pub struct RocketMqOptions {
    /// Nameserver endpoints, already normalised to `host:port`.
    pub nameservers: Vec<String>,
    pub timeout: Duration,
}

impl RocketMqOptions {
    pub fn from_profile(profile: &ConnectionProfile) -> AppResult<Self> {
        let raw = profile.option_str("nameserver").unwrap_or_default();
        let default_port =
            profile.option_u64("nameserverPort", DEFAULT_NAMESERVER_PORT as u64) as u16;

        let mut nameservers = Vec::new();
        for entry in raw
            .split([',', ';', '\n', ' '])
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
        {
            let addr = socket_addr(entry, default_port);
            if !nameservers.contains(&addr) {
                nameservers.push(addr);
            }
        }

        if nameservers.is_empty() {
            return Err(AppError::invalid(
                "a RocketMQ nameserver address is required (for example `localhost:9876`)",
            ));
        }

        let timeout = Duration::from_millis(
            profile
                .option_u64("timeoutMs", DEFAULT_TIMEOUT_MS)
                .clamp(500, 120_000),
        );

        Ok(Self {
            nameservers,
            timeout,
        })
    }

    /// The nameservers as one string, used in messages and logs.
    pub fn authority(&self) -> String {
        self.nameservers.join(", ")
    }

    /// First nameserver, the one every metadata request is sent to.
    pub fn primary(&self) -> &str {
        self.nameservers
            .first()
            .map(String::as_str)
            .unwrap_or("localhost:9876")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mq::types::{ConnectionProfile, JsonMap};

    fn profile(options: JsonMap) -> ConnectionProfile {
        ConnectionProfile {
            id: "test".into(),
            name: "test".into(),
            provider: "rocketmq".into(),
            color: None,
            description: None,
            tags: Vec::new(),
            options,
            created_at: 0,
            updated_at: 0,
            last_connected_at: None,
        }
    }

    #[test]
    fn normalises_nameserver_lists() {
        let mut options = JsonMap::new();
        options.insert(
            "nameserver".into(),
            serde_json::json!("ns1:9876, ns2  ;ns3"),
        );
        let parsed = RocketMqOptions::from_profile(&profile(options)).unwrap();
        assert_eq!(parsed.nameservers, vec!["ns1:9876", "ns2:9876", "ns3:9876"]);
        assert_eq!(parsed.primary(), "ns1:9876");
    }

    #[test]
    fn requires_a_nameserver() {
        let error = RocketMqOptions::from_profile(&profile(JsonMap::new())).unwrap_err();
        assert!(error.to_string().contains("nameserver"));
    }

    #[test]
    fn honours_a_custom_default_port() {
        let mut options = JsonMap::new();
        options.insert("nameserver".into(), serde_json::json!("ns1"));
        options.insert("nameserverPort".into(), serde_json::json!(9877));
        let parsed = RocketMqOptions::from_profile(&profile(options)).unwrap();
        assert_eq!(parsed.nameservers, vec!["ns1:9877"]);
    }
}
