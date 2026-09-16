/**
 * TypeScript mirror of `src-tauri/src/mq/types.rs`.
 *
 * Every provider speaks this model, so the UI never contains broker specific
 * code — it reads `capabilities` to decide what to render.
 *
 * Keep the shapes in sync: the Rust side always serialises `camelCase`.
 */

export type JsonMap = Record<string, unknown>;

// --- dynamic connection form ----------------------------------------------

export type FieldKind =
  | 'text'
  | 'password'
  | 'number'
  | 'boolean'
  | 'select'
  | 'textarea'
  | 'keyValue';

export interface FieldOption {
  value: string;
  label: string;
  hint?: string | null;
}

export interface FieldCondition {
  key: string;
  equals: string[];
}

export interface ConnectionField {
  key: string;
  label: string;
  kind: FieldKind;
  help?: string | null;
  placeholder?: string | null;
  required: boolean;
  default?: unknown;
  options: FieldOption[];
  group: string;
  advanced: boolean;
  condition?: FieldCondition | null;
  secret: boolean;
}

// --- capabilities ---------------------------------------------------------

export interface TopicCapabilities {
  list: boolean;
  create: boolean;
  delete: boolean;
  update: boolean;
  config: boolean;
  partitions: boolean;
  purge: boolean;
}

export interface MessageCapabilities {
  produce: boolean;
  browse: boolean;
  tail: boolean;
  keys: boolean;
  headers: boolean;
  partitions: boolean;
  timestampSeek: boolean;
  offsetSeek: boolean;
  persistent: boolean;
}

export interface GroupCapabilities {
  list: boolean;
  describe: boolean;
  members: boolean;
  offsets: boolean;
  lag: boolean;
  resetOffsets: boolean;
  delete: boolean;
}

export interface Capabilities {
  topics: TopicCapabilities;
  messages: MessageCapabilities;
  groups: GroupCapabilities;
  nodes: boolean;
  metrics: boolean;
  acl: boolean;
  schemas: boolean;
}

// --- providers & connections ---------------------------------------------

export interface ProviderDescriptor {
  id: string;
  name: string;
  description: string;
  vendor: string;
  accent: string;
  docsUrl?: string | null;
  defaultPort: number | null;
  driverVersion: string;
  capabilities: Capabilities;
  fields: ConnectionField[];
}

export interface ConnectionProfile {
  id: string;
  name: string;
  provider: string;
  color?: string | null;
  description?: string | null;
  tags: string[];
  options: JsonMap;
  createdAt: number;
  updatedAt: number;
  lastConnectedAt?: number | null;
}

export type ConnectionState = 'disconnected' | 'connecting' | 'connected' | 'error';

export interface ConnectionStatus {
  profileId: string;
  providerId: string;
  displayName?: string | null;
  state: ConnectionState;
  error?: string | null;
  cluster?: ClusterInfo | null;
  connectedAt?: number | null;
  latencyMs?: number | null;
}

export interface ConnectResult {
  status: ConnectionStatus;
  capabilities: Capabilities;
  provider: ProviderDescriptor;
}

// --- cluster --------------------------------------------------------------

export interface Attribute {
  label: string;
  value: string;
  mono: boolean;
}

export interface ClusterInfo {
  id?: string | null;
  name: string;
  provider: string;
  version?: string | null;
  controllerId?: number | null;
  nodeCount: number;
  topicCount: number;
  partitionCount: number;
  consumerGroupCount?: number | null;
  attributes: Attribute[];
}

export interface NodeInfo {
  id: number;
  host: string;
  port: number;
  rack?: string | null;
  isController: boolean;
  role?: string | null;
}

// --- topics ---------------------------------------------------------------

export type EntityKind = 'topic' | 'queue' | 'exchange';

export interface TopicSummary {
  name: string;
  kind: EntityKind;
  internal: boolean;
  partitionCount?: number | null;
  messageCount?: number | null;
  sizeBytes?: number | null;
  consumerCount?: number | null;
  attributes: Attribute[];
}

export interface PartitionInfo {
  id: number;
  leader?: number | null;
  replicas: number[];
  isr: number[];
  offlineReplicas: number[];
  beginOffset?: number | null;
  endOffset?: number | null;
  messageCount?: number | null;
}

export interface ConfigEntry {
  name: string;
  value?: string | null;
  source?: string | null;
  readOnly: boolean;
  sensitive: boolean;
  isDefault: boolean;
}

export interface TopicDetail {
  summary: TopicSummary;
  partitions: PartitionInfo[];
  configs: ConfigEntry[];
  attributes: Attribute[];
}

export interface CreateTopicRequest {
  name: string;
  partitionCount?: number | null;
  replicationFactor?: number | null;
  configs: ConfigEntry[];
  options: JsonMap;
}

export interface UpdateTopicRequest {
  name: string;
  partitionCount?: number | null;
  configs: ConfigEntry[];
}

// --- messages -------------------------------------------------------------

export type PayloadEncoding = 'utf8' | 'base64';

export interface MessageHeader {
  key: string;
  value?: string | null;
  encoding: PayloadEncoding;
}

export interface Message {
  id: string;
  topic: string;
  partition?: number | null;
  offset?: number | null;
  timestamp?: number | null;
  producer?: string | null;
  key?: string | null;
  keyEncoding: PayloadEncoding;
  payload?: string | null;
  encoding: PayloadEncoding;
  headers: MessageHeader[];
  size: number;
}

export interface ProduceRequest {
  topic: string;
  key?: string | null;
  payload?: string | null;
  headers: MessageHeader[];
  partition?: number | null;
  timestamp?: number | null;
  encoding: PayloadEncoding;
  options: JsonMap;
}

export interface ProduceResult {
  topic: string;
  partition: number;
  offset: number;
  elapsedMs: number;
}

/** Discriminated union mirroring the Rust `SeekPosition` enum. */
export type SeekPosition =
  | { mode: 'beginning' }
  | { mode: 'end' }
  | { mode: 'offset'; offset: number }
  | { mode: 'timestamp'; timestamp: number };

export interface BrowseRequest {
  topic: string;
  partitions?: number[] | null;
  start: SeekPosition;
  limit: number;
  timeoutMs?: number | null;
  keyword?: string | null;
}

export interface BrowseResult {
  messages: Message[];
  truncated: boolean;
  scanned: number;
  elapsedMs: number;
  watermarks: PartitionInfo[];
}

export interface StreamRequest {
  topic: string;
  partitions?: number[] | null;
  start: SeekPosition;
  maxMessages: number;
  idleTimeoutMs?: number | null;
}

// --- consumer groups ------------------------------------------------------

export interface ConsumerGroupSummary {
  id: string;
  state?: string | null;
  protocolType?: string | null;
  memberCount: number;
  topics: string[];
  totalLag?: number | null;
  kind?: string | null;
}

export interface MemberAssignment {
  topic: string;
  partitions: number[];
}

export interface GroupMember {
  id: string;
  clientId?: string | null;
  clientHost?: string | null;
  assignments: MemberAssignment[];
}

export interface GroupOffset {
  topic: string;
  partition?: number | null;
  currentOffset?: number | null;
  endOffset?: number | null;
  beginOffset?: number | null;
  lag?: number | null;
  metadata?: string | null;
  node?: number | null;
}

export interface ConsumerGroupDetail {
  summary: ConsumerGroupSummary;
  members: GroupMember[];
  offsets: GroupOffset[];
}

export type ResetMode = 'earliest' | 'latest' | 'offset';

export interface ResetOffsetsRequest {
  group: string;
  topic: string;
  partitions?: number[] | null;
  mode: ResetMode;
  offset?: number | null;
}

// --- jobs -----------------------------------------------------------------

export type JobState = 'starting' | 'running' | 'stopping' | 'stopped' | 'failed';

export interface JobInfo {
  id: string;
  kind: string;
  connectionId: string;
  target: string;
  state: JobState;
  error?: string | null;
  startedAt: number;
  received: number;
}

// --- app ------------------------------------------------------------------

export interface AppSettings {
  theme: string;
  pageSize: number;
  defaultMessageLimit: number;
  prettyJson: boolean;
  confirmDestructive: boolean;
  autoConnect: boolean;
  timestampFormat: string;
}

export interface AppInfo {
  name: string;
  version: string;
  tauriVersion: string;
  os: string;
  arch: string;
  tlsSupported: boolean;
  workspacePath: string;
  providers: string[];
}

export interface ApiErrorPayload {
  kind: string;
  message: string;
}

// --- backend events -------------------------------------------------------

export interface MessageBatchEvent {
  jobId: string;
  messages: Message[];
}

export interface JobEvent {
  jobId: string;
  state: JobState;
  error?: string | null;
  idle: boolean;
  received: number;
}

export interface ConnectionsEvent {
  reason: string;
  profileId?: string | null;
}
