//! Queues and exchanges, mapped onto the neutral “topic” model.
//!
//! AMQP is intentionally split in two: queues store messages, exchanges route
//! them. Both are listed so that the workspace can show the full topology, with
//! [`EntityKind`] telling the UI which is which.

use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};
use crate::mq::types::*;

use super::api::{ExchangeDto, QueueDto};
use super::RabbitMqConnection;

impl RabbitMqConnection {
    // -- cluster ----------------------------------------------------------

    pub(crate) async fn cluster_impl(&self) -> AppResult<ClusterInfo> {
        let api = self.api()?;
        let overview = match api.overview().await {
            Ok(overview) => overview,
            Err(_) => self.overview.clone().unwrap_or_default(),
        };
        let nodes = api.nodes().await.unwrap_or_default();
        let totals = &overview.object_totals;

        let mut attributes = Vec::new();
        attributes.push(Attribute::new("Virtual host", self.vhost()));
        attributes.push(Attribute::new("Queues", totals.queues.to_string()));
        attributes.push(Attribute::new("Exchanges", totals.exchanges.to_string()));
        attributes.push(Attribute::new(
            "Connections",
            totals.connections.to_string(),
        ));
        attributes.push(Attribute::new("Channels", totals.channels.to_string()));
        attributes.push(Attribute::new("Consumers", totals.consumers.to_string()));
        attributes.push(Attribute::new(
            "Messages ready",
            overview.queue_totals.messages_ready.to_string(),
        ));
        attributes.push(Attribute::new(
            "Messages unacknowledged",
            overview.queue_totals.messages_unacknowledged.to_string(),
        ));
        if let Some(stats) = overview.message_stats.as_ref() {
            attributes.push(Attribute::new(
                "Messages published",
                stats.publish.to_string(),
            ));
            attributes.push(Attribute::new(
                "Messages delivered",
                stats.deliver.to_string(),
            ));
            attributes.push(Attribute::new(
                "Messages acknowledged",
                stats.ack.to_string(),
            ));
        }
        if let Some(version) = overview.management_version.as_ref() {
            attributes.push(Attribute::new("Management plugin", version));
        }
        if let Some(node) = overview.node.as_ref() {
            attributes.push(Attribute::new("Connected node", node));
        }
        if let Some(name) = overview.cluster_name.as_ref() {
            attributes.push(Attribute::new("Cluster", name));
        }

        Ok(ClusterInfo {
            id: overview.cluster_name.clone(),
            name: overview
                .cluster_name
                .clone()
                .unwrap_or_else(|| "RabbitMQ".to_string()),
            provider: super::PROVIDER_ID.to_string(),
            version: overview
                .rabbitmq_version
                .clone()
                .or_else(|| overview.product_version.clone()),
            controller_id: None,
            node_count: nodes.len() as u32,
            // `list_topics` returns queues *and* exchanges.
            topic_count: totals.queues + totals.exchanges,
            partition_count: 0,
            consumer_group_count: Some(totals.consumers),
            attributes,
        })
    }

    pub(crate) async fn nodes_impl(&self) -> AppResult<Vec<NodeInfo>> {
        let api = self.api()?;
        let overview = self.overview.clone().unwrap_or_default();
        let mut nodes: Vec<NodeInfo> = api
            .nodes()
            .await?
            .into_iter()
            .enumerate()
            .map(|(index, node)| {
                let mut role = node.kind.clone().unwrap_or_else(|| "disc".to_string());
                if !node.running {
                    role.push_str(" (stopped)");
                }
                if !node.partitions.is_empty() {
                    role.push_str(" (network partition)");
                }
                NodeInfo {
                    id: index as i32,
                    host: node.name.clone(),
                    // AMQP nodes do not expose a single client port.
                    port: 0,
                    rack: None,
                    is_controller: overview.node.as_deref() == Some(node.name.as_str()),
                    role: Some(role),
                }
            })
            .collect();
        nodes.sort_by(|a, b| a.host.cmp(&b.host));
        Ok(nodes)
    }

    // -- topics (queues + exchanges) --------------------------------------

    pub(crate) async fn topics_impl(&self) -> AppResult<Vec<TopicSummary>> {
        let api = self.api()?;
        let vhost = self.vhost();
        let (queues, exchanges) = tokio::try_join!(api.queues(vhost), api.exchanges(vhost))?;
        self.remember_exchanges(exchanges.iter().map(|exchange| exchange.name.clone()));

        let mut items: Vec<TopicSummary> = Vec::with_capacity(queues.len() + exchanges.len());
        items.extend(queues.iter().map(queue_summary));
        items.extend(
            exchanges
                .iter()
                .filter(|exchange| !exchange.name.is_empty())
                .map(exchange_summary),
        );
        items.sort_by(|a, b| {
            entity_rank(a.kind)
                .cmp(&entity_rank(b.kind))
                .then_with(|| a.name.cmp(&b.name))
        });
        Ok(items)
    }

    pub(crate) async fn topic_detail_impl(&self, topic: &str) -> AppResult<TopicDetail> {
        let api = self.api()?;
        let vhost = self.vhost();

        // A name can be both a queue and an exchange; queues win because they
        // are what messages are usually inspected through.
        if let Some(queue) = api.queue_optional(vhost, topic).await? {
            let bindings = api.queue_bindings(vhost, topic).await.unwrap_or_default();
            let mut attributes = Vec::new();
            attributes.push(Attribute::new("Kind", "queue"));
            if let Some(state) = queue.state.as_ref() {
                attributes.push(Attribute::new("State", state));
            }
            if let Some(node) = queue.node.as_ref() {
                attributes.push(Attribute::new("Node", node));
            }
            if let Some(leader) = queue.leader.as_ref() {
                attributes.push(Attribute::new("Leader", leader));
            }
            if !queue.members.is_empty() {
                attributes.push(Attribute::new("Members", queue.members.join(", ")));
            }
            attributes.push(Attribute::new(
                "Messages",
                queue.messages.unwrap_or_default().to_string(),
            ));
            attributes.push(Attribute::new(
                "Messages ready",
                queue.messages_ready.unwrap_or_default().to_string(),
            ));
            attributes.push(Attribute::new(
                "Messages unacknowledged",
                queue
                    .messages_unacknowledged
                    .unwrap_or_default()
                    .to_string(),
            ));
            attributes.extend(binding_attributes(&bindings, "Exchange", "Routing key"));
            if let Some(consumers) = queue.consumers {
                attributes.push(Attribute::new("Consumers", consumers.to_string()));
            }
            if let Some(memory) = queue.memory {
                attributes.push(Attribute::new("Memory", human_bytes(memory)));
            }
            if let Some(policy) = queue.policy.as_ref() {
                attributes.push(Attribute::new("Policy", policy));
            }

            Ok(TopicDetail {
                summary: queue_summary(&queue),
                partitions: Vec::new(),
                configs: queue_configs(&queue),
                attributes,
            })
        } else if let Some(exchange) = api.exchange_optional(vhost, topic).await? {
            let bindings = api
                .exchange_bindings(vhost, topic)
                .await
                .unwrap_or_default();
            let mut attributes = vec![Attribute::new("Kind", "exchange")];
            attributes.extend(binding_attributes(&bindings, "Destination", "Routing key"));
            Ok(TopicDetail {
                summary: exchange_summary(&exchange),
                partitions: Vec::new(),
                configs: exchange_configs(&exchange),
                attributes,
            })
        } else {
            Err(AppError::invalid(format!(
                "`{topic}` is neither a queue nor an exchange in vhost `{}`",
                vhost
            )))
        }
    }

    // -- mutations --------------------------------------------------------

    pub(crate) async fn create_topic_impl(&self, request: CreateTopicRequest) -> AppResult<()> {
        let api = self.api()?;
        let vhost = self.vhost();
        if request.name.trim().is_empty() {
            return Err(AppError::invalid(
                "the queue/exchange name must not be empty",
            ));
        }
        let name = request.name.trim();

        if is_exchange_request(&request.options) {
            let options = request_options(&request.options, &request.configs);
            let body = exchange_body(&options);
            api.declare_exchange(vhost, name, body).await?;
        } else {
            let options = request_options(&request.options, &request.configs);
            let body = queue_body(&options);
            api.declare_queue(vhost, name, body).await?;
        }
        tracing::info!(target: "mq_manager::rabbitmq", vhost, name, "declared");
        Ok(())
    }

    /// AMQP arguments are immutable: the broker rejects the re-declaration when
    /// they differ. We simply re-declare and surface the broker's message.
    pub(crate) async fn update_topic_impl(&self, request: UpdateTopicRequest) -> AppResult<()> {
        let api = self.api()?;
        let vhost = self.vhost();
        let name = request.name.trim().to_string();

        let options = request_options(&Map::new(), &request.configs);
        if api.queue_optional(vhost, &name).await?.is_some() {
            api.declare_queue(vhost, &name, queue_body(&options)).await
        } else if api.exchange_optional(vhost, &name).await?.is_some() {
            api.declare_exchange(vhost, &name, exchange_body(&options))
                .await
        } else {
            Err(AppError::invalid(format!("`{name}` does not exist")))
        }
    }

    pub(crate) async fn delete_topic_impl(&self, topic: &str) -> AppResult<()> {
        let api = self.api()?;
        let vhost = self.vhost();
        let name = topic.trim();

        // Exchanges the connection was told about are deleted as exchanges;
        // everything else is treated as a queue (and vice versa as a fallback).
        if api.queue_optional(vhost, name).await?.is_some() {
            api.delete_queue(vhost, name).await
        } else if api.exchange_optional(vhost, name).await?.is_some() {
            api.delete_exchange(vhost, name).await
        } else {
            Err(AppError::invalid(format!("`{name}` does not exist")))
        }
    }

    pub(crate) async fn purge_topic_impl(&self, topic: &str) -> AppResult<()> {
        let api = self.api()?;
        let vhost = self.vhost();
        let name = topic.trim();
        if api.queue_optional(vhost, name).await?.is_none() {
            return Err(AppError::invalid(format!(
                "`{name}` is not a queue — only queues hold messages"
            )));
        }
        api.purge_queue(vhost, name).await
    }
}

// ---------------------------------------------------------------------------
// Mapping helpers
// ---------------------------------------------------------------------------

fn entity_rank(kind: EntityKind) -> u8 {
    match kind {
        EntityKind::Topic => 0,
        EntityKind::Queue => 1,
        EntityKind::Exchange => 2,
    }
}

fn kind_label(kind: EntityKind) -> &'static str {
    match kind {
        EntityKind::Topic => "topic",
        EntityKind::Queue => "queue",
        EntityKind::Exchange => "exchange",
    }
}

fn queue_summary(queue: &QueueDto) -> TopicSummary {
    let mut attributes = vec![
        Attribute::new("Kind", kind_label(EntityKind::Queue)),
        Attribute::new(
            "Queue type",
            queue.kind.clone().unwrap_or_else(|| "classic".to_string()),
        ),
        Attribute::new("Durable", yes_no(queue.durable)),
        Attribute::new("Auto delete", yes_no(queue.auto_delete)),
    ];
    if let Some(state) = queue.state.as_ref() {
        attributes.push(Attribute::new("State", state));
    }
    if let Some(ready) = queue.messages_ready {
        attributes.push(Attribute::new("Ready", ready.to_string()));
    }
    if let Some(unacked) = queue.messages_unacknowledged {
        attributes.push(Attribute::new("Unacked", unacked.to_string()));
    }
    TopicSummary {
        name: queue.name.clone(),
        kind: EntityKind::Queue,
        internal: queue.internal,
        partition_count: None,
        message_count: queue.messages,
        size_bytes: queue.message_bytes,
        consumer_count: queue.consumers,
        attributes,
    }
}

fn exchange_summary(exchange: &ExchangeDto) -> TopicSummary {
    let mut attributes = vec![
        Attribute::new("Kind", kind_label(EntityKind::Exchange)),
        Attribute::new(
            "Exchange type",
            exchange
                .kind
                .clone()
                .unwrap_or_else(|| "direct".to_string()),
        ),
        Attribute::new("Durable", yes_no(exchange.durable)),
        Attribute::new("Auto delete", yes_no(exchange.auto_delete)),
    ];
    if let Some(stats) = exchange.message_stats.as_ref() {
        attributes.push(Attribute::new("Published in", stats.publish_in.to_string()));
        attributes.push(Attribute::new(
            "Published out",
            stats.publish_out.to_string(),
        ));
    }
    TopicSummary {
        name: exchange.name.clone(),
        kind: EntityKind::Exchange,
        internal: exchange.internal,
        partition_count: None,
        message_count: None,
        size_bytes: None,
        consumer_count: None,
        attributes,
    }
}

fn queue_configs(queue: &QueueDto) -> Vec<ConfigEntry> {
    let mut configs = vec![
        config(
            "type",
            queue.kind.clone().unwrap_or_else(|| "classic".into()),
            true,
        ),
        config("durable", yes_no(queue.durable), true),
        config("auto_delete", yes_no(queue.auto_delete), true),
        config("exclusive", yes_no(queue.exclusive), true),
        config("internal", yes_no(queue.internal), true),
    ];
    if let Some(state) = queue.state.as_ref() {
        configs.push(config("state", state.clone(), true));
    }
    if let Some(node) = queue.node.as_ref() {
        configs.push(config("node", node.clone(), true));
    }
    if let Some(policy) = queue.policy.as_ref() {
        configs.push(config("policy", policy.clone(), false));
    }
    for (key, value) in &queue.arguments {
        configs.push(config(key, render_value(value), false));
    }
    configs
}

fn exchange_configs(exchange: &ExchangeDto) -> Vec<ConfigEntry> {
    let mut configs = vec![
        config(
            "type",
            exchange.kind.clone().unwrap_or_else(|| "direct".into()),
            true,
        ),
        config("durable", yes_no(exchange.durable), true),
        config("auto_delete", yes_no(exchange.auto_delete), true),
        config("internal", yes_no(exchange.internal), true),
    ];
    for (key, value) in &exchange.arguments {
        configs.push(config(key, render_value(value), false));
    }
    configs
}

fn config(name: &str, value: String, read_only: bool) -> ConfigEntry {
    ConfigEntry {
        name: name.to_string(),
        value: Some(value),
        source: Some(if read_only {
            "broker".into()
        } else {
            "argument".into()
        }),
        read_only,
        sensitive: false,
        is_default: false,
    }
}

fn binding_attributes(bindings: &[Value], target_label: &str, key_label: &str) -> Vec<Attribute> {
    let mut attributes = Vec::new();
    for binding in bindings {
        let target = binding
            .get(if target_label == "Exchange" {
                "source"
            } else {
                "destination"
            })
            .and_then(Value::as_str)
            .unwrap_or_default();
        if target.is_empty() {
            // The default exchange has no bindings worth showing.
            continue;
        }
        let routing_key = binding
            .get("routing_key")
            .and_then(Value::as_str)
            .unwrap_or_default();
        attributes.push(Attribute::new(
            target_label,
            format!(
                "{target} → {key_label}: {}",
                if routing_key.is_empty() {
                    "(none)"
                } else {
                    routing_key
                }
            ),
        ));
    }
    if attributes.is_empty() {
        attributes.push(Attribute::new(target_label, "none"));
    }
    attributes
}

// -- request bodies -------------------------------------------------------

fn is_exchange_request(options: &JsonMap) -> bool {
    option_str(options, "entity")
        .map(|entity| entity.eq_ignore_ascii_case("exchange"))
        .unwrap_or(false)
}

fn queue_body(options: &JsonMap) -> Value {
    let mut arguments = Map::new();
    // Queue type is an argument in AMQP (`x-queue-type`), not a field.
    if let Some(kind) = option_str(options, "type") {
        if matches!(kind.as_str(), "classic" | "quorum" | "stream") && kind != "classic" {
            arguments.insert("x-queue-type".into(), Value::String(kind));
        }
    }
    for (option, argument) in [
        ("maxLength", "x-max-length"),
        ("maxLengthBytes", "x-max-length-bytes"),
        ("messageTtl", "x-message-ttl"),
        ("expires", "x-expires"),
        ("deadLetterExchange", "x-dead-letter-exchange"),
        ("deadLetterRoutingKey", "x-dead-letter-routing-key"),
        ("maxPriority", "x-max-priority"),
        ("singleActiveConsumer", "x-single-active-consumer"),
        ("streamMaxLengthBytes", "x-stream-max-segment-size-bytes"),
        ("leaderLocator", "queue-leader-locator"),
        ("overflow", "x-overflow"),
    ] {
        if let Some(value) = argument_value(options, option) {
            arguments.insert(argument.into(), value);
        }
    }
    merge_arguments(options, &mut arguments);

    serde_json::json!({
        "durable": option_bool(options, "durable", true),
        "auto_delete": option_bool(options, "autoDelete", false),
        "internal": option_bool(options, "internal", false),
        "arguments": Value::Object(arguments),
    })
}

fn exchange_body(options: &JsonMap) -> Value {
    let mut arguments = Map::new();
    merge_arguments(options, &mut arguments);
    let kind = option_str(options, "type").unwrap_or_else(|| "direct".to_string());
    serde_json::json!({
        "type": kind,
        "durable": option_bool(options, "durable", true),
        "auto_delete": option_bool(options, "autoDelete", false),
        "internal": option_bool(options, "internal", false),
        "arguments": Value::Object(arguments),
    })
}

/// Options that are not part of `KNOWN_OPTIONS` are passed through as raw AMQP
/// arguments, so `x-` arguments work without a driver specific UI.
fn merge_arguments(options: &JsonMap, arguments: &mut Map<String, Value>) {
    if let Some(Value::Object(raw)) = options.get("arguments") {
        for (key, value) in raw {
            arguments.insert(key.clone(), value.clone());
        }
    }
    for (key, value) in options {
        if key.starts_with("x-") {
            arguments.insert(key.clone(), value.clone());
        }
    }
}

fn argument_value(options: &JsonMap, key: &str) -> Option<Value> {
    let value = options.get(key)?;
    // Numbers arrive as strings from the key/value editor.
    Some(match value {
        Value::String(text) => match text.parse::<i64>() {
            Ok(number) if key != "deadLetterExchange" && key != "deadLetterRoutingKey" => {
                Value::Number(number.into())
            }
            _ => match text.parse::<bool>() {
                Ok(flag) if !matches!(key, "deadLetterExchange" | "deadLetterRoutingKey") => {
                    Value::Bool(flag)
                }
                _ => Value::String(text.clone()),
            },
        },
        other => other.clone(),
    })
}

/// Merge the driver specific `options` map with any `configs` rows a generic
/// form sent. Explicit options always win.
fn request_options(options: &JsonMap, configs: &[ConfigEntry]) -> JsonMap {
    let mut merged = options.clone();
    for entry in configs {
        if entry.read_only || merged.contains_key(&entry.name) {
            continue;
        }
        let Some(value) = entry.value.as_ref() else {
            continue;
        };
        merged.insert(entry.name.clone(), Value::String(value.clone()));
    }
    merged
}

// -- small helpers --------------------------------------------------------

fn option_str(options: &JsonMap, key: &str) -> Option<String> {
    match options.get(key) {
        Some(Value::String(value)) => {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Some(Value::Number(number)) => Some(number.to_string()),
        Some(Value::Bool(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn option_bool(options: &JsonMap, key: &str, fallback: bool) -> bool {
    match options.get(key) {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => match value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => fallback,
        },
        _ => fallback,
    }
}

fn yes_no(value: bool) -> String {
    if value { "yes" } else { "no" }.to_string()
}

fn render_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn human_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_type_becomes_an_argument() {
        let mut options = JsonMap::new();
        options.insert("type".into(), Value::String("quorum".into()));
        options.insert("maxLength".into(), Value::String("1000".into()));
        options.insert("x-custom".into(), Value::String("yes".into()));
        let body = queue_body(&options);
        assert_eq!(body["arguments"]["x-queue-type"], "quorum");
        assert_eq!(body["arguments"]["x-max-length"], 1000);
        assert_eq!(body["arguments"]["x-custom"], "yes");
        assert_eq!(body["durable"], true);
    }

    #[test]
    fn queue_type_default_is_not_sent() {
        let mut options = JsonMap::new();
        options.insert("type".into(), Value::String("classic".into()));
        let body = queue_body(&options);
        assert!(body["arguments"].as_object().unwrap().is_empty());
    }

    #[test]
    fn exchanges_are_detected() {
        let mut options = JsonMap::new();
        options.insert("entity".into(), Value::String("EXCHANGE".into()));
        assert!(is_exchange_request(&options));
        assert!(!is_exchange_request(&JsonMap::new()));
    }
}
