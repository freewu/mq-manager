#!/usr/bin/env node
/**
 * Seeds the local Kafka fixture started by `just kafka-up`.
 *
 *   just kafka-seed
 *
 * Everything runs inside the broker container through `docker compose exec`, so
 * no Kafka client is needed on the host. The generated data is meant to look
 * like a small production cluster: a handful of topics with different partition
 * counts, a few consumer groups — some of them deliberately lagging — and JSON
 * payloads the message viewer can pretty print.
 */

import { spawnSync } from 'node:child_process';

const COMPOSE = ['compose'];
const SERVICE = 'kafka';
const BIN = '/opt/kafka/bin';
const BOOTSTRAP = 'localhost:9092';

const script = [];

/** Deterministic pseudo random so reruns look the same. */
let seed = 1337;
const random = () => {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed / 2147483648;
};

const pick = (items) => items[Math.floor(random() * items.length)];

function order(index) {
  return {
    orderId: `ord-${1000 + index}`,
    customerId: `cus-${100 + (index % 40)}`,
    status: pick(['created', 'paid', 'shipped', 'cancelled']),
    total: Number((random() * 480 + 20).toFixed(2)),
    currency: 'EUR',
    items: Array.from({ length: 1 + Math.floor(random() * 3) }, (_, position) => ({
      sku: `SKU-${200 + ((index + position) % 60)}`,
      quantity: 1 + Math.floor(random() * 4),
    })),
    createdAt: new Date(Date.UTC(2024, 4, 1 + (index % 28), 9, index % 60)).toISOString(),
  };
}

function event(index) {
  const kinds = ['page_view', 'add_to_cart', 'checkout_started', 'purchase', 'signup'];
  return {
    kind: kinds[index % kinds.length],
    sessionId: `ses-${9000 + Math.floor(random() * 900)}`,
    userId: `usr-${300 + (index % 70)}`,
    path: pick(['/', '/pricing', '/docs', '/checkout', '/blog/tauri']),
    device: pick(['desktop', 'mobile', 'tablet']),
    latencyMs: Math.floor(random() * 900) + 30,
  };
}

function payment(index) {
  return {
    paymentId: `pay-${5000 + index}`,
    orderId: `ord-${1000 + index}`,
    provider: pick(['stripe', 'adyen', 'paypal']),
    amount: Number((random() * 500 + 10).toFixed(2)),
    result: pick(['authorised', 'captured', 'refunded', 'declined']),
  };
}

const LOG_LEVELS = ['DEBUG', 'INFO', 'WARN', 'ERROR'];
const LOG_MESSAGES = [
  'consumer group rebalanced',
  'produced batch to partition 2',
  'connection to broker 1 established',
  'offset commit failed, retrying',
  'topic metadata refreshed',
  'session expired, rejoining group',
];

function logLine(index) {
  const level = LOG_LEVELS[index % LOG_LEVELS.length];
  return `${new Date(Date.UTC(2024, 4, 2, 8, index % 60, index % 60)).toISOString()} ${level} [mq-manager] ${LOG_MESSAGES[index % LOG_MESSAGES.length]}`;
}

const render = (value) => (typeof value === 'string' ? value : JSON.stringify(value));

const lines = (count, renderer) =>
  Array.from({ length: count }, (_, index) => render(renderer(index))).join('\n');

/* -------------------------------------------------------------------------- */
/* container script                                                           */
/* -------------------------------------------------------------------------- */

script.push(`
set -euo pipefail
KAFKA_BIN=${BIN}
BOOTSTRAP=${BOOTSTRAP}

echo "waiting for the broker to accept connections…"
for attempt in $(seq 1 60); do
  if "$KAFKA_BIN/kafka-broker-api-versions.sh" --bootstrap-server "$BOOTSTRAP" > /dev/null 2>&1; then
    break
  fi
  sleep 1
done
"$KAFKA_BIN/kafka-broker-api-versions.sh" --bootstrap-server "$BOOTSTRAP" > /dev/null

create_topic() {
  "$KAFKA_BIN/kafka-topics.sh" --bootstrap-server "$BOOTSTRAP" \\
    --create --if-not-exists --topic "$1" --partitions "$2" --replication-factor 1 > /dev/null
  echo "  topic $1 ($2 partitions)"
}

produce() {
  "$KAFKA_BIN/kafka-console-producer.sh" --bootstrap-server "$BOOTSTRAP" --topic "$1" > /dev/null
}

consume_some() {
  # Committing offsets for a group is what creates the lag the UI shows.
  "$KAFKA_BIN/kafka-console-consumer.sh" --bootstrap-server "$BOOTSTRAP" \\
    --topic "$1" --group "$2" --from-beginning \\
    --max-messages "$3" --timeout-ms 15000 > /dev/null 2>&1 || true
  echo "  group $2 consumed $3 record(s) of $1"
}

echo "creating topics"
create_topic orders 3
create_topic payments 6
create_topic events 3
create_topic logs 1
create_topic analytics.raw 4
`);

const ROWS = [
  { topic: 'orders', count: 60, render: order },
  { topic: 'payments', count: 24, render: payment },
  { topic: 'events', count: 140, render: event },
  { topic: 'logs', count: 90, render: (index) => logLine(index) },
  { topic: 'analytics.raw', count: 45, render: event },
];

for (const row of ROWS) {
  script.push(`
echo "producing ${row.count} records into ${row.topic}"
produce ${row.topic} <<'MQ_MANAGER_MESSAGES'
${lines(row.count, row.render)}
MQ_MANAGER_MESSAGES
`);
}

script.push(`
echo "creating consumer groups"
consume_some orders demo-consumer 20
consume_some events analytics-service 50
consume_some logs log-shipper 30

echo
echo "topics:"
"$KAFKA_BIN/kafka-topics.sh" --bootstrap-server "$BOOTSTRAP" --list | sed 's/^/  /'
echo
echo "consumer groups:"
"$KAFKA_BIN/kafka-consumer-groups.sh" --bootstrap-server "$BOOTSTRAP" --list | sed 's/^/  /'
`);

/* -------------------------------------------------------------------------- */
/* run                                                                        */
/* -------------------------------------------------------------------------- */

function run(args, options = {}) {
  return spawnSync('docker', [...COMPOSE, ...args], {
    encoding: 'utf8',
    windowsHide: true,
    ...options,
  });
}

const docker = spawnSync('docker', ['--version'], { encoding: 'utf8', windowsHide: true });
if (docker.error || docker.status !== 0) {
  console.error('docker was not found on PATH — install Docker Desktop and retry.');
  process.exit(1);
}

const running = run(['ps', '--status', 'running', '--services']);
if (!(running.stdout ?? '').includes(SERVICE)) {
  console.error(`the "${SERVICE}" service is not running. Start it with:\n\n  just kafka-up\n`);
  process.exit(1);
}

console.log('seeding the local Kafka fixture…\n');

const result = run(['exec', '-T', SERVICE, 'bash', '-c', script.join('\n')], {
  stdio: 'inherit',
});

if (result.status !== 0) {
  console.error('\nseeding failed. Is the broker still starting up? Try `just kafka-up` again.');
  process.exit(result.status ?? 1);
}

console.log(`
Done. Point an MQ Manager connection at localhost:9092:

  topics    orders, payments, events, logs, analytics.raw
  groups    demo-consumer, analytics-service, log-shipper (all with backlog)
`);
