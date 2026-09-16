import { useNavigate, useParams } from 'react-router-dom';

import { CapabilityList } from '@/components/domain/Capabilities';
import { PageHeader } from '@/components/layout/PageHeader';
import { LagBar } from '@/components/domain/Tables';
import { Banner, Badge, Button, DetailList, EmptyState, Metric, Panel, ProviderChip, Spinner } from '@/components/ui/primitives';
import { clusterApi } from '@/api/cluster';
import { groupsApi } from '@/api/groups';
import { topicsApi } from '@/api/topics';
import { useAsync } from '@/hooks/useAsync';
import { useConnection } from '@/hooks/useConnection';
import { formatBytes, formatNumber, formatRelative } from '@/lib/format';
import { primaryTarget } from '@/lib/profile';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';

export function Overview() {
  const { id = '' } = useParams();
  const navigate = useNavigate();
  const { profile, provider, status, state, connected, capabilities } = useConnection(id);
  const connect = useWorkspace((value) => value.connect);
  const busy = useWorkspace((value) => Boolean(value.busy[id]));

  const cluster = useAsync(() => clusterApi.info(id), [id], { enabled: connected });
  const topics = useAsync(() => topicsApi.list(id, false), [id], { enabled: connected });

  if (!profile) {
    return (
      <div className="page">
        <EmptyState
          icon="alert"
          title="Connection not found"
          text="It may have been deleted in another window."
          actions={
            <Button onClick={() => navigate('/connections')}>Back to connections</Button>
          }
        />
      </div>
    );
  }

  const info = cluster.data ?? status?.cluster ?? undefined;
  const largest = [...(topics.data ?? [])]
    .filter((topic) => !topic.internal)
    .sort((left, right) => (right.sizeBytes ?? 0) - (left.sizeBytes ?? 0))
    .slice(0, 6);

  return (
    <div className="page">
      <PageHeader
        title={profile.name}
        subtitle={
          <span className="row gap-8">
            {provider ? <ProviderChip name={provider.name} accent={provider.accent} /> : null}
            <span className="mono dim">{primaryTarget(profile, provider)}</span>
          </span>
        }
        actions={
          <>
            {connected ? (
              <Button icon="layers" onClick={() => navigate(`/c/${id}/topics`)}>
                Topics
              </Button>
            ) : null}
            {connected ? (
              <Button icon="users" onClick={() => navigate(`/c/${id}/groups`)}>
                Groups
              </Button>
            ) : null}
            <Button
              variant="ghost"
              icon="refresh"
              onClick={() => {
                void cluster.reload();
                void topics.reload();
              }}
              disabled={!connected}
            >
              Refresh
            </Button>
          </>
        }
      />

      {!connected ? (
        <Panel>
          <EmptyState
            icon="plug"
            title={`Not connected (${state})`}
            text="Open the connection to load cluster metadata, topics and consumer groups."
            actions={
              <Button
                variant="primary"
                icon="zap"
                loading={busy}
                onClick={async () => {
                  const ok = await connect(id);
                  if (ok) notifySuccess('Connected', profile.name);
                  else notifyError('Check the driver configuration', 'Connection failed');
                }}
              >
                Connect
              </Button>
            }
          />
        </Panel>
      ) : null}

      {connected && cluster.error ? (
        <Banner tone="danger" actions={<Button size="sm" onClick={() => void cluster.reload()}>Retry</Button>}>
          {cluster.error}
        </Banner>
      ) : null}

      {connected ? (
        <div className="stack-16">
          <div className="grid-cards">
            <Metric
              label="Brokers"
              value={info ? formatNumber(info.nodeCount) : '—'}
              sub={info?.controllerId != null ? `controller #${info.controllerId}` : undefined}
              icon="server"
            />
            <Metric
              label="Topics"
              value={info ? formatNumber(info.topicCount) : '—'}
              sub={info?.name}
              icon="layers"
            />
            <Metric
              label="Partitions"
              value={info ? formatNumber(info.partitionCount) : '—'}
              sub="across all topics"
              icon="grid"
            />
            <Metric
              label="Consumer groups"
              value={info?.consumerGroupCount != null ? formatNumber(info.consumerGroupCount) : '—'}
              sub={status?.latencyMs != null ? `metadata in ${status.latencyMs} ms` : undefined}
              icon="users"
            />
          </div>

          <div className="split">
            <Panel
              title="Cluster"
              actions={cluster.loading ? <Spinner /> : <Badge tone="success">{state}</Badge>}
            >
              {info ? (
                <DetailList
                  items={[
                    { label: 'Name', value: info.name },
                    {
                      label: 'Cluster id',
                      value: info.id ? <span className="mono">{info.id}</span> : '—',
                    },
                    { label: 'Version', value: info.version ?? '—' },
                    { label: 'Provider', value: info.provider },
                    {
                      label: 'Controller',
                      value: info.controllerId != null ? `node ${info.controllerId}` : '—',
                    },
                    { label: 'Address', value: <span className="mono">{primaryTarget(profile, provider)}</span> },
                    {
                      label: 'Connected',
                      value: status?.connectedAt ? formatRelative(status.connectedAt) : '—',
                    },
                    ...info.attributes.map((attribute) => ({
                      label: attribute.label,
                      value: attribute.mono ? (
                        <span className="mono">{attribute.value}</span>
                      ) : (
                        attribute.value
                      ),
                    })),
                  ]}
                />
              ) : (
                <div className="row gap-8 dim small">
                  <Spinner />
                  Loading cluster metadata…
                </div>
              )}
            </Panel>

            <Panel
              title="Largest topics"
              actions={
                <Button
                  size="sm"
                  variant="ghost"
                  icon="chevron-right"
                  onClick={() => navigate(`/c/${id}/topics`)}
                >
                  All topics
                </Button>
              }
            >
              {topics.loading ? (
                <div className="row gap-8 dim small">
                  <Spinner />
                  Loading topics…
                </div>
              ) : largest.length === 0 ? (
                <span className="dim small">No topics reported by this broker.</span>
              ) : (
                <table className="table">
                  <tbody>
                    {largest.map((topic) => (
                      <tr
                        key={topic.name}
                        style={{ cursor: 'pointer' }}
                        onClick={() => navigate(`/c/${id}/topics/${encodeURIComponent(topic.name)}`)}
                      >
                        <td className="mono-cell">{topic.name}</td>
                        <td className="table__num dim">
                          {formatNumber(topic.partitionCount)} p
                        </td>
                        <td className="table__num dim">{formatNumber(topic.messageCount)}</td>
                        <td className="table__num">{formatBytes(topic.sizeBytes)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </Panel>
          </div>

          <Panel title="Lag hotspots" actions={<span className="tiny dim">from the group list</span>}>
            <GroupLagPreview connectionId={id} />
          </Panel>

          <Panel title="What this driver supports">
            {capabilities ? (
              <CapabilityList capabilities={capabilities} />
            ) : (
              <span className="dim small">Capabilities are reported after a successful connect.</span>
            )}
          </Panel>

          {topics.error ? <Banner tone="warning">{topics.error}</Banner> : null}
        </div>
      ) : null}
    </div>
  );
}

function GroupLagPreview({ connectionId }: { connectionId: string }) {
  const navigate = useNavigate();
  const groups = useWorkspace((state) => state.capabilities[connectionId]?.groups.list ?? false);

  const list = useAsync(() => groupsApi.list(connectionId), [connectionId], { enabled: groups });

  if (!groups) return <span className="dim small">This driver does not expose consumer groups.</span>;
  if (list.loading) {
    return (
      <div className="row gap-8 dim small">
        <Spinner />
        Loading groups…
      </div>
    );
  }

  const ranked = [...(list.data ?? [])]
    .filter((group) => (group.totalLag ?? 0) > 0)
    .sort((left, right) => (right.totalLag ?? 0) - (left.totalLag ?? 0))
    .slice(0, 5);

  if (ranked.length === 0) {
    return <span className="dim small">Every consumer group is caught up.</span>;
  }

  return (
    <div className="stack-8">
      {ranked.map((group) => (
        <div
          key={group.id}
          className="row gap-12"
          style={{ cursor: 'pointer' }}
          onClick={() => navigate(`/c/${connectionId}/groups/${encodeURIComponent(group.id)}`)}
        >
          <span className="mono-cell grow ellipsis">{group.id}</span>
          <LagBar lag={group.totalLag ?? 0} />
        </div>
      ))}
    </div>
  );
}
