import { useNavigate } from 'react-router-dom';

import { ProviderSummary } from '@/components/domain/Capabilities';
import { Badge, Button, Panel } from '@/components/ui/primitives';
import { useWorkspace } from '@/store/workspace';

const FEATURES: Array<{ icon: string; title: string; text: string }> = [
  {
    icon: 'search',
    title: 'Browse & search',
    text: 'Seek by offset or timestamp, page through partitions and filter payloads.',
  },
  {
    icon: 'activity',
    title: 'Live tail',
    text: 'Stream new messages into the grid without polluting real consumer groups.',
  },
  {
    icon: 'users',
    title: 'Consumer groups',
    text: 'Inspect members, committed offsets and per-partition lag at a glance.',
  },
  {
    icon: 'sliders',
    title: 'Administration',
    text: 'Create topics, tune configuration overrides, reset offsets, purge data.',
  },
];

export function Welcome() {
  const navigate = useNavigate();
  const providers = useWorkspace((state) => state.providers);
  const profiles = useWorkspace((state) => state.profiles);

  return (
    <div className="page">
      <div className="welcome">
        <div className="welcome__hero">
          <span className="brand__mark" style={{ width: 58, height: 58, borderRadius: 16 }}>
            <svg viewBox="0 0 24 24" width="30" height="30" fill="none" stroke="#fff" strokeWidth={2} strokeLinecap="round">
              <path d="M4 7h16" />
              <path d="M4 12h11" />
              <path d="M4 17h7" />
              <circle cx="19" cy="17" r="2" fill="#fff" stroke="none" />
            </svg>
          </span>
          <h1 style={{ fontSize: 26, marginTop: 4 }}>One console for every message broker</h1>
          <p className="muted" style={{ maxWidth: 560, lineHeight: 1.6 }}>
            MQ Manager ships a provider-agnostic core: Kafka is the first driver, and further
            engines plug in without touching the interface. Pick a driver, connect, and the
            screens adapt to what your broker actually supports.
          </p>
          <div className="row gap-8 mt-8">
            <Button variant="primary" icon="plus" onClick={() => navigate('/connections?new=1')}>
              New connection
            </Button>
            <Button icon="plug" onClick={() => navigate('/connections')}>
              Manage connections
            </Button>
          </div>
        </div>

        <div className="feature-grid mb-8">
          {FEATURES.map((feature) => (
            <div className="card" key={feature.title}>
              <div className="bold small">{feature.title}</div>
              <p className="small dim" style={{ marginTop: 4, lineHeight: 1.55 }}>
                {feature.text}
              </p>
            </div>
          ))}
        </div>

        <Panel
          title={`Drivers in this build (${providers.length})`}
          actions={<Badge tone="accent">{profiles.length} connections</Badge>}
        >
          <div className="stack-12">
            {providers.map((provider) => (
              <ProviderSummary
                key={provider.id}
                name={provider.name}
                vendor={provider.vendor}
                description={provider.description}
                accent={provider.accent}
                port={provider.defaultPort}
              />
            ))}
            {providers.length === 0 ? (
              <span className="dim small">No drivers registered — the backend failed to start.</span>
            ) : null}
          </div>
        </Panel>

        <Panel title="Spin up a broker for testing" className="mt-16">
          <div className="stack-8 small muted">
            <span>
              The repository ships a compose file with Kafka, RabbitMQ and RocketMQ. From the
              project root:
            </span>
            <pre className="code-block" data-selectable>
{`just all-up        # start every fixture (see the README for ports)
just kafka-up      # or just one: Kafka (KRaft, no ZooKeeper)
just rabbit-up     # RabbitMQ with the management plugin
just rocket-up     # RocketMQ nameserver + broker
just kafka-seed    # create sample topics and messages`}
            </pre>
          </div>
        </Panel>
      </div>
    </div>
  );
}
