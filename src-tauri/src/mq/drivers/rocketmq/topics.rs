//! Cluster, node and topic discovery for RocketMQ.
//!
//! RocketMQ spreads its metadata over two roles: nameservers know which brokers
//! exist and where the queues of a topic live, brokers know the topic
//! configuration. Everything here is therefore assembled from a handful of
//! admin requests instead of one `listTopics` style call.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::protocol::{code, response_code, RemotingCommand};
use super::remoting::is_missing;
use super::{messages, RocketMqConnection};
use crate::error::{AppError, AppResult};
use crate::mq::types::*;

/// Per topic deadline while listing topics.
const ROUTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);
/// Queues described with offsets in `topic_detail` before we stop asking.
const DETAIL_QUEUE_LIMIT: usize = 32;

// ---------------------------------------------------------------------------
// Wire structures
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct QueueData {
    #[serde(default)]
    pub broker_name: String,
    #[serde(default)]
    pub read_queue_nums: i32,
    #[serde(default)]
    pub write_queue_nums: i32,
    #[serde(default)]
    pub perm: i32,
    /// Present in the route payload; nothing on the screens needs it yet.
    #[serde(default)]
    #[allow(dead_code)]
    pub topic_sys_flag: i32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct BrokerData {
    #[serde(default)]
    pub cluster: String,
    #[serde(default)]
    pub broker_name: String,
    #[serde(default)]
    pub broker_addrs: BTreeMap<String, String>,
}

impl BrokerData {
    /// Address of the master (`brokerId == 0`).
    pub fn master(&self) -> Option<&str> {
        self.broker_addrs.get("0").map(String::as_str)
    }

    pub fn slaves(&self) -> Vec<(i32, String)> {
        self.broker_addrs
            .iter()
            .filter_map(|(id, addr)| id.parse::<i32>().ok().map(|id| (id, addr.clone())))
            .filter(|(id, _)| *id != 0)
            .collect()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TopicRouteData {
    #[serde(default)]
    pub order_topic_conf: Option<String>,
    #[serde(default)]
    pub queue_datas: Vec<QueueData>,
    #[serde(default)]
    pub broker_datas: Vec<BrokerData>,
}

impl TopicRouteData {
    pub fn broker(&self, name: &str) -> Option<&BrokerData> {
        self.broker_datas
            .iter()
            .find(|data| data.broker_name == name)
    }

    /// Total number of queues (read and write) the topic is spread over.
    pub fn partition_count(&self) -> u32 {
        self.queue_datas
            .iter()
            .map(|data| data.read_queue_nums.max(data.write_queue_nums).max(0) as u32)
            .sum()
    }

    /// Every `(broker name, master address, queue id)` triple to pull from.
    pub fn queues(&self) -> Vec<(String, String, i32)> {
        let mut queues = Vec::new();
        for queue in &self.queue_datas {
            let Some(broker) = self.broker(&queue.broker_name) else {
                continue;
            };
            let Some(master) = broker.master() else {
                continue;
            };
            let count = queue.read_queue_nums.max(queue.write_queue_nums).max(0);
            for queue_id in 0..count {
                queues.push((queue.broker_name.clone(), master.to_string(), queue_id));
            }
        }
        queues
    }

    pub fn perm(&self) -> i32 {
        self.queue_datas
            .iter()
            .map(|data| data.perm)
            .fold(0, |acc, perm| acc | perm)
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ClusterDto {
    #[serde(default)]
    pub broker_addr_table: BTreeMap<String, BrokerData>,
    #[serde(default)]
    pub cluster_addr_table: BTreeMap<String, Vec<String>>,
}

impl ClusterDto {
    /// Every master broker as `(broker name, address)`, sorted by name.
    pub fn masters(&self) -> Vec<(String, String)> {
        self.broker_addr_table
            .iter()
            .filter_map(|(name, data)| data.master().map(|addr| (name.clone(), addr.to_string())))
            .collect()
    }

    pub fn all(&self) -> Vec<(String, BrokerData)> {
        self.broker_addr_table
            .iter()
            .map(|(name, data)| (name.clone(), data.clone()))
            .collect()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TopicConfigDto {
    #[serde(default)]
    pub topic_name: String,
    #[serde(default)]
    pub read_queue_nums: i32,
    #[serde(default)]
    pub write_queue_nums: i32,
    #[serde(default)]
    pub perm: i32,
    #[serde(default)]
    pub topic_filter_type: Option<String>,
    #[serde(default)]
    pub topic_sys_flag: i32,
    #[serde(default)]
    pub order: bool,
    #[serde(default)]
    pub attributes: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Lookups
// ---------------------------------------------------------------------------

/// Fetch (and cache) the broker table from the nameserver.
pub(super) async fn cluster_dto(connection: &RocketMqConnection) -> AppResult<ClusterDto> {
    if let Some(cached) = connection.cached_cluster() {
        return Ok(cached);
    }
    let response = connection
        .client()
        .request(connection.nameserver(), code::GET_BROKER_CLUSTER_INFO, &[])
        .await?;
    if response.code != response_code::SUCCESS {
        return Err(AppError::broker(format!(
            "could not read the broker cluster info from {}: {}{}",
            connection.nameserver(),
            super::protocol::response_text(response.code),
            remark_of(&response)
        )));
    }
    let cluster: ClusterDto = response.body_json().unwrap_or_default();
    connection.remember_cluster(cluster.clone());
    Ok(cluster)
}

/// Fetch (and cache) the route of a topic.
pub(super) async fn route(
    connection: &RocketMqConnection,
    topic: &str,
) -> AppResult<TopicRouteData> {
    if let Some(cached) = connection.cached_route(topic) {
        return Ok(cached);
    }
    let response = connection
        .client()
        .request(
            connection.nameserver(),
            code::GET_ROUTEINFO_BY_TOPIC,
            &[("topic", topic.to_string())],
        )
        .await?;
    if response.code != response_code::SUCCESS {
        if is_missing(response.code) {
            return Err(AppError::invalid(format!(
                "the topic `{topic}` does not exist on this RocketMQ cluster"
            )));
        }
        return Err(AppError::broker(format!(
            "could not resolve the route of `{topic}`: {}{}",
            super::protocol::response_text(response.code),
            remark_of(&response)
        )));
    }
    let route: TopicRouteData = response.body_json()?;
    if route.broker_datas.is_empty() && route.queue_datas.is_empty() {
        return Err(AppError::broker(format!(
            "the nameserver returned an empty route for `{topic}`"
        )));
    }
    connection.remember_route(topic, route.clone());
    Ok(route)
}

/// All master broker addresses, or an actionable error when the cluster is empty.
pub(super) async fn masters(connection: &RocketMqConnection) -> AppResult<Vec<(String, String)>> {
    let cluster = cluster_dto(connection).await?;
    let masters = cluster.masters();
    if masters.is_empty() {
        return Err(AppError::broker(
            "the nameserver does not know any broker — is at least one broker registered?",
        ));
    }
    Ok(masters)
}

/// The master broker holding a topic, used for per-broker admin requests.
pub(super) async fn topic_masters(
    connection: &RocketMqConnection,
    topic: &str,
) -> AppResult<Vec<String>> {
    let route = route(connection, topic).await?;
    let mut addrs: Vec<String> = route
        .queue_datas
        .iter()
        .filter_map(|queue| route.broker(&queue.broker_name))
        .filter_map(|broker| broker.master().map(str::to_string))
        .collect();
    addrs.sort();
    addrs.dedup();
    if addrs.is_empty() {
        return Err(AppError::broker(format!(
            "no master broker is serving `{topic}`"
        )));
    }
    Ok(addrs)
}

// ---------------------------------------------------------------------------
// Cluster / nodes
// ---------------------------------------------------------------------------

pub(super) async fn cluster_impl(connection: &RocketMqConnection) -> AppResult<ClusterInfo> {
    let cluster = cluster_dto(connection).await?;
    let topic_names = topic_names(connection).await?;
    let brokers = cluster.all();
    let masters = cluster.masters();

    // The broker version is one extra request; the page is fine without it.
    let version = match masters.first() {
        Some((_, addr)) => broker_version(connection, addr).await.ok().flatten(),
        None => None,
    };

    let mut attributes = Vec::new();
    attributes.push(Attribute::new(
        "Nameservers",
        connection.nameserver().to_string(),
    ));
    let mut clusters: Vec<String> = cluster.cluster_addr_table.keys().cloned().collect();
    if clusters.is_empty() {
        clusters = brokers
            .iter()
            .map(|(_, data)| data.cluster.clone())
            .filter(|name| !name.is_empty())
            .collect();
        clusters.sort();
        clusters.dedup();
    }
    if !clusters.is_empty() {
        attributes.push(Attribute::new("Clusters", clusters.join(", ")));
    }
    attributes.push(Attribute::new("Brokers", brokers.len().to_string()));
    attributes.push(Attribute::new(
        "Masters",
        masters
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    ));
    if let Some(version) = &version {
        attributes.push(Attribute::new("Broker version", version.clone()));
    }
    attributes.push(Attribute::new("Topics", topic_names.len().to_string()));
    attributes.push(Attribute::new("Serialization", "JSON (remoting)"));

    let name = clusters
        .first()
        .cloned()
        .unwrap_or_else(|| "RocketMQ".to_string());

    Ok(ClusterInfo {
        id: Some(name.clone()),
        name,
        provider: super::PROVIDER_ID.to_string(),
        version,
        controller_id: None,
        node_count: (masters.len() + 1) as u32,
        topic_count: topic_names.len() as u32,
        // Summing the queues of every topic would need one route lookup per
        // topic; the topic list shows the per topic number instead.
        partition_count: 0,
        consumer_group_count: None,
        attributes,
    })
}

pub(super) async fn nodes_impl(connection: &RocketMqConnection) -> AppResult<Vec<NodeInfo>> {
    let cluster = cluster_dto(connection).await?;
    let mut nodes = Vec::new();

    // Nameservers have no ids in RocketMQ; use negative ones so they sort first.
    for (index, nameserver) in connection.nameservers().iter().enumerate() {
        let (host, port) = split_host(nameserver.as_str(), super::options::DEFAULT_NAMESERVER_PORT);
        nodes.push(NodeInfo {
            id: -((index as i32) + 1),
            host,
            port: port as i32,
            rack: None,
            is_controller: false,
            role: Some("nameserver".into()),
        });
    }

    for (name, data) in cluster.all() {
        for (id, addr) in &data.broker_addrs {
            let Ok(broker_id) = id.parse::<i32>() else {
                continue;
            };
            let (host, port) = split_host(addr.as_str(), super::options::DEFAULT_BROKER_PORT);
            nodes.push(NodeInfo {
                id: broker_id,
                host,
                port: port as i32,
                rack: Some(name.clone()),
                is_controller: broker_id == 0,
                role: Some(if broker_id == 0 { "master" } else { "slave" }.into()),
            });
        }
    }

    if nodes.len() == connection.nameservers().len() {
        return Err(AppError::broker(
            "the nameserver does not know any broker — is at least one broker registered?",
        ));
    }
    Ok(nodes)
}

async fn broker_version(connection: &RocketMqConnection, addr: &str) -> AppResult<Option<String>> {
    let response = connection
        .client()
        .request(addr, code::GET_BROKER_RUNTIME_INFO, &[])
        .await?;
    if response.code != response_code::SUCCESS {
        return Ok(None);
    }
    let info: Value = response.body_json()?;
    Ok(info
        .get("brokerVersionDesc")
        .and_then(Value::as_str)
        .map(str::to_string))
}

// ---------------------------------------------------------------------------
// Topics
// ---------------------------------------------------------------------------

/// Topic names as known by the nameserver.
pub(super) async fn topic_names(connection: &RocketMqConnection) -> AppResult<Vec<String>> {
    let response = connection
        .client()
        .request(
            connection.nameserver(),
            code::GET_ALL_TOPIC_LIST_FROM_NAMESERVER,
            &[],
        )
        .await?;
    if response.code != response_code::SUCCESS {
        return Err(AppError::broker(format!(
            "could not list the topics from {}: {}{}",
            connection.nameserver(),
            super::protocol::response_text(response.code),
            remark_of(&response)
        )));
    }
    // Older brokers answer with a bare array, newer ones with a wrapper object.
    let body: Value = response.body_json()?;
    let list = match body {
        Value::Array(items) => items,
        Value::Object(map) => map
            .get("topicList")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    let mut names: Vec<String> = list
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect();
    names.sort();
    names.dedup();
    Ok(names)
}

pub(super) async fn topics_impl(connection: &RocketMqConnection) -> AppResult<Vec<TopicSummary>> {
    let names = topic_names(connection).await?;

    let mut topics = Vec::with_capacity(names.len());
    for name in &names {
        // Route lookups all go to the nameserver over one connection, so they
        // are serialised anyway; a short deadline keeps a slow broker from
        // blocking the whole page.
        let route = tokio::time::timeout(ROUTE_TIMEOUT, route(connection, name))
            .await
            .ok()
            .and_then(Result::ok);
        topics.push(summary_from_route(name, route.as_ref()));
    }
    Ok(topics)
}

fn summary_from_route(name: &str, route: Option<&TopicRouteData>) -> TopicSummary {
    let mut attributes = Vec::new();
    if let Some(route) = route {
        let brokers: Vec<&str> = route
            .queue_datas
            .iter()
            .map(|queue| queue.broker_name.as_str())
            .collect();
        if !brokers.is_empty() {
            attributes.push(Attribute::new("Brokers", brokers.join(", ")));
        }
        let queues: i32 = route
            .queue_datas
            .iter()
            .map(|queue| queue.read_queue_nums.max(queue.write_queue_nums))
            .sum();
        attributes.push(Attribute::new("Queues", queues.to_string()));
        attributes.push(Attribute::new("Permission", perm_text(route.perm())));
    }
    TopicSummary {
        name: name.to_string(),
        kind: EntityKind::Topic,
        internal: is_system_topic(name),
        partition_count: route.map(TopicRouteData::partition_count),
        message_count: None,
        size_bytes: None,
        consumer_count: None,
        attributes,
    }
}

pub(super) async fn topic_detail_impl(
    connection: &RocketMqConnection,
    topic: &str,
) -> AppResult<TopicDetail> {
    let route = route(connection, topic).await?;
    let mut summary = summary_from_route(topic, Some(&route));

    // Per queue offsets: bounded, because a topic may have hundreds of queues.
    let queues = route.queues();
    let mut partitions = Vec::with_capacity(queues.len());
    for (broker, _, queue_id) in &queues {
        let replicas = route
            .broker(broker)
            .map(|data| {
                let mut ids: Vec<i32> = vec![0];
                ids.extend(data.slaves().into_iter().map(|(id, _)| id));
                ids
            })
            .unwrap_or_default();
        partitions.push(PartitionInfo {
            id: *queue_id,
            leader: Some(0),
            replicas: replicas.clone(),
            isr: replicas,
            offline_replicas: Vec::new(),
            begin_offset: None,
            end_offset: None,
            message_count: None,
        });
    }

    let mut total: i64 = 0;
    let mut counted = 0usize;
    for (partition, (_, addr, _)) in partitions.iter_mut().zip(queues.iter()) {
        if counted >= DETAIL_QUEUE_LIMIT {
            break;
        }
        let (begin, end) = tokio::join!(
            messages::min_offset(connection, addr, topic, partition.id),
            messages::max_offset(connection, addr, topic, partition.id),
        );
        if let (Ok(begin), Ok(end)) = (begin, end) {
            partition.begin_offset = Some(begin);
            partition.end_offset = Some(end);
            partition.message_count = Some((end - begin).max(0));
            total += (end - begin).max(0);
            counted += 1;
        }
    }
    if counted > 0 && counted == partitions.len() {
        summary.message_count = Some(total);
    }

    let configs = topic_config(connection, topic, &route).await;

    let mut attributes = summary.attributes.clone();
    attributes.push(Attribute::new(
        "Ordered",
        route.order_topic_conf.is_some().to_string(),
    ));

    Ok(TopicDetail {
        summary,
        partitions,
        configs,
        attributes,
    })
}

/// Topic configuration straight from the broker, with a route based fallback.
async fn topic_config(
    connection: &RocketMqConnection,
    topic: &str,
    route: &TopicRouteData,
) -> Vec<ConfigEntry> {
    let mut config = None;
    for addr in topic_masters(connection, topic).await.unwrap_or_default() {
        let response = connection
            .client()
            .request(
                addr.as_str(),
                code::GET_TOPIC_CONFIG,
                &[("topic", topic.to_string())],
            )
            .await;
        if let Ok(response) = response {
            if response.code == response_code::SUCCESS {
                if let Ok(parsed) = response.body_json::<TopicConfigDto>() {
                    if !parsed.topic_name.is_empty() {
                        config = Some(parsed);
                        break;
                    }
                }
            }
        }
    }

    let config = config.unwrap_or_else(|| TopicConfigDto {
        topic_name: topic.to_string(),
        read_queue_nums: route
            .queue_datas
            .iter()
            .map(|queue| queue.read_queue_nums)
            .max()
            .unwrap_or(0),
        write_queue_nums: route
            .queue_datas
            .iter()
            .map(|queue| queue.write_queue_nums)
            .max()
            .unwrap_or(0),
        perm: route.perm(),
        topic_filter_type: Some("SINGLE_TAG".into()),
        topic_sys_flag: 0,
        order: route.order_topic_conf.is_some(),
        attributes: BTreeMap::new(),
    });

    let mut configs = vec![
        entry("readQueueNums", config.read_queue_nums.to_string()),
        entry("writeQueueNums", config.write_queue_nums.to_string()),
        entry("perm", perm_text(config.perm)),
        entry(
            "topicFilterType",
            config
                .topic_filter_type
                .clone()
                .unwrap_or_else(|| "SINGLE_TAG".into()),
        ),
        entry("topicSysFlag", config.topic_sys_flag.to_string()),
        entry("order", config.order.to_string()),
    ];
    for (key, value) in &config.attributes {
        configs.push(entry(key, value.clone()));
    }
    configs
}

fn entry(name: &str, value: String) -> ConfigEntry {
    ConfigEntry {
        name: name.to_string(),
        value: Some(value),
        source: Some("broker".into()),
        read_only: false,
        sensitive: false,
        is_default: false,
    }
}

pub(super) async fn create_topic_impl(
    connection: &RocketMqConnection,
    request: CreateTopicRequest,
) -> AppResult<()> {
    let queue_nums = request
        .partition_count
        .map(|count| count as i32)
        .or_else(|| queue_nums(&request.options))
        .unwrap_or(4)
        .clamp(1, 1024);
    let perm = request
        .options
        .get("perm")
        .and_then(Value::as_i64)
        .map(|perm| perm as i32)
        .unwrap_or(6);

    let body = json!({
        "topicName": request.name,
        "readQueueNums": queue_nums,
        "writeQueueNums": queue_nums,
        "perm": perm,
        "topicFilterType": "SINGLE_TAG",
        "topicSysFlag": 0,
        "order": request.options.get("order").and_then(Value::as_bool).unwrap_or(false),
    });
    let body = serde_json::to_vec(&body)
        .map_err(|error| AppError::broker(format!("could not encode the topic config: {error}")))?;

    let targets = if let Some(broker) = request.options.get("brokerName").and_then(Value::as_str) {
        let cluster = cluster_dto(connection).await?;
        let addr = cluster
            .broker_addr_table
            .get(broker)
            .and_then(BrokerData::master)
            .ok_or_else(|| {
                AppError::invalid(format!("unknown broker `{broker}` in this cluster"))
            })?;
        vec![(broker.to_string(), addr.to_string())]
    } else {
        masters(connection).await?
    };

    let mut failures = Vec::new();
    for (name, addr) in &targets {
        let command =
            RemotingCommand::request(code::UPDATE_AND_CREATE_TOPIC, 0, Default::default())
                .with_ext("topic", request.name.clone())
                .with_ext("brokerName", name.clone())
                .with_body(body.clone());
        match connection.client().invoke(addr, command).await {
            Ok(response) if response.code == response_code::SUCCESS => {}
            Ok(response) => failures.push(format!(
                "{name}: {}{}",
                super::protocol::response_text(response.code),
                remark_of(&response)
            )),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }
    if !failures.is_empty() {
        return Err(AppError::broker(format!(
            "could not create `{}`: {}",
            request.name,
            failures.join("; ")
        )));
    }
    connection.invalidate_route(&request.name);
    Ok(())
}

pub(super) async fn update_topic_impl(
    connection: &RocketMqConnection,
    request: UpdateTopicRequest,
) -> AppResult<()> {
    let current = topic_config(
        connection,
        &request.name,
        &route(connection, &request.name).await?,
    )
    .await;
    let mut options = Map::new();
    for config in &request.configs {
        if let Some(value) = &config.value {
            options.insert(config.name.clone(), Value::String(value.clone()));
        }
    }
    if let Some(partitions) = request.partition_count {
        options.insert("readQueueNums".into(), json!(partitions));
        options.insert("writeQueueNums".into(), json!(partitions));
    }
    if options.is_empty() {
        // Nothing to change but the numbers we already have: push them so the
        // broker refreshes its config and validates the topic exists.
        for config in &current {
            if let Some(value) = &config.value {
                options.insert(config.name.clone(), Value::String(value.clone()));
            }
        }
    }
    create_topic_impl(
        connection,
        CreateTopicRequest {
            name: request.name.clone(),
            partition_count: request.partition_count,
            replication_factor: None,
            configs: Vec::new(),
            options,
        },
    )
    .await
}

pub(super) async fn delete_topic_impl(
    connection: &RocketMqConnection,
    topic: &str,
) -> AppResult<()> {
    if is_system_topic(topic) {
        return Err(AppError::invalid(format!(
            "`{topic}` is a RocketMQ system topic and cannot be deleted"
        )));
    }
    let mut failures = Vec::new();

    // The nameserver drops the route, every broker drops the config.
    let response = connection
        .client()
        .request(
            connection.nameserver(),
            code::DELETE_TOPIC_IN_NAMESRV,
            &[("topic", topic.to_string())],
        )
        .await;
    match response {
        Ok(response) if response.code == response_code::SUCCESS => {}
        Ok(response) if is_missing(response.code) => {}
        Ok(response) => failures.push(format!(
            "nameserver: {}{}",
            super::protocol::response_text(response.code),
            remark_of(&response)
        )),
        Err(error) => failures.push(format!("nameserver: {error}")),
    }

    for (name, addr) in masters(connection).await? {
        let response = connection
            .client()
            .request(
                addr.as_str(),
                code::DELETE_TOPIC_IN_BROKER,
                &[("topic", topic.to_string())],
            )
            .await;
        match response {
            Ok(response) if response.code == response_code::SUCCESS => {}
            Ok(response) if is_missing(response.code) => {}
            Ok(response) => failures.push(format!(
                "{name}: {}{}",
                super::protocol::response_text(response.code),
                remark_of(&response)
            )),
            Err(error) => failures.push(format!("{name}: {error}")),
        }
    }

    if !failures.is_empty() {
        return Err(AppError::broker(format!(
            "could not delete `{topic}`: {}",
            failures.join("; ")
        )));
    }
    connection.invalidate_route(topic);
    Ok(())
}

/// RocketMQ has no way to empty a topic while keeping it.

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Topics that belong to RocketMQ itself.
pub(super) fn is_system_topic(name: &str) -> bool {
    if name.starts_with("rmq_sys_")
        || name.starts_with("%RETRY%")
        || name.starts_with("%DLQ%")
        || name.starts_with("RMQ_SYS_")
    {
        return true;
    }
    matches!(
        name,
        "TBW102"
            | "SELF_TEST_TOPIC"
            | "OFFSET_MOVED_EVENT"
            | "SCHEDULE_TOPIC_XXXX"
            | "DefaultCluster"
            | "benchmark"
            | "CID_RMQ_SYS_TRANS"
    )
}

fn perm_text(perm: i32) -> String {
    let read = perm & 0x4 != 0;
    let write = perm & 0x2 != 0;
    match (read, write) {
        (true, true) => "read + write".into(),
        (true, false) => "read only".into(),
        (false, true) => "write only".into(),
        (false, false) => perm.to_string(),
    }
}

fn queue_nums(options: &Map<String, Value>) -> Option<i32> {
    for key in ["readQueueNums", "writeQueueNums", "queueNums", "partitions"] {
        if let Some(value) = options.get(key).and_then(Value::as_i64) {
            return Some(value as i32);
        }
    }
    None
}

fn split_host(addr: &str, default_port: u16) -> (String, u16) {
    match addr.rsplit_once(':') {
        Some((host, port)) => (host.to_string(), port.parse().unwrap_or(default_port)),
        None => (addr.to_string(), default_port),
    }
}

fn remark_of(response: &RemotingCommand) -> String {
    response
        .remark
        .as_deref()
        .filter(|remark| !remark.is_empty())
        .map(|remark| format!(" — {remark}"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_system_topics() {
        assert!(is_system_topic("TBW102"));
        assert!(is_system_topic("rmq_sys_TRANS_HALF_TOPIC"));
        assert!(is_system_topic("%RETRY%group"));
        assert!(!is_system_topic("orders"));
    }

    #[test]
    fn lists_queues_of_a_route() {
        let route = TopicRouteData {
            queue_datas: vec![QueueData {
                broker_name: "broker-a".into(),
                read_queue_nums: 4,
                write_queue_nums: 2,
                perm: 6,
                topic_sys_flag: 0,
            }],
            broker_datas: vec![BrokerData {
                cluster: "DefaultCluster".into(),
                broker_name: "broker-a".into(),
                broker_addrs: [("0".to_string(), "127.0.0.1:10911".to_string())]
                    .into_iter()
                    .collect(),
            }],
            order_topic_conf: None,
        };
        assert_eq!(route.partition_count(), 4);
        assert_eq!(route.queues().len(), 4);
        assert_eq!(route.perm(), 6);
    }

    #[test]
    fn renders_permissions() {
        assert_eq!(perm_text(6), "read + write");
        assert_eq!(perm_text(4), "read only");
        assert_eq!(perm_text(2), "write only");
    }
}
