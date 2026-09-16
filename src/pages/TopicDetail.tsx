import { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { MessageDrawer, MessageTable } from '@/components/domain/MessageTable';
import { ProduceForm } from '@/components/domain/ProduceForm';
import { TopicForm } from '@/components/domain/TopicForm';
import { DataTable, type Column } from '@/components/ui/Table';
import { Tabs } from '@/components/ui/Tabs';
import { ConfirmDialog, Modal } from '@/components/ui/Modal';
import { Field, Select } from '@/components/ui/Field';
import {
  Badge,
  Button,
  DetailList,
  EmptyState,
  IconButton,
  Panel,
  Spinner,
  Toolbar,
  ToolbarSpacer,
} from '@/components/ui/primitives';
import { topicsApi } from '@/api/topics';
import { messagesApi } from '@/api/messages';
import { useAsync } from '@/hooks/useAsync';
import { useCapabilities, useConnection } from '@/hooks/useConnection';
import { formatBytes, formatNumber } from '@/lib/format';
import { notifyError, notifySuccess } from '@/store/toast';
import { streamKey, useStreams } from '@/store/streams';
import { useWorkspace } from '@/store/workspace';
import type {
  BrowseResult,
  ConfigEntry,
  Message,
  PartitionInfo,
  SeekPosition,
} from '@/types';

type SeekMode = 'beginning' | 'end' | 'offset' | 'timestamp';

export function TopicDetail() {
  const { id = '', topic = '' } = useParams();
  const navigate = useNavigate();
  const decoded = decodeURIComponent(topic);

  const { profile } = useConnection(id);
  const capabilities = useCapabilities(id);
  const connected = useWorkspace((state) => state.statuses[id]?.state === 'connected');

  const [tab, setTab] = useState('messages');
  const [editing, setEditing] = useState(false);
  const [purgeOpen, setPurgeOpen] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [working, setWorking] = useState(false);

  const detail = useAsync(() => topicsApi.detail(id, decoded), [id, decoded], {
    enabled: connected && capabilities.topics.list,
  });

  const partitions = detail.data?.partitions ?? [];
  const partitionIds = partitions.map((partition) => partition.id);

  if (!profile || !connected) {
    return (
      <div className="page">
        <EmptyState
          icon="plug"
          title={profile ? 'Not connected' : 'Connection not found'}
          text={profile ? 'Reconnect to inspect this topic.' : undefined}
          actions={<Button onClick={() => navigate('/connections')}>Connections</Button>}
        />
      </div>
    );
  }

  return (
    <div className="page page--flush">
      <Toolbar>
        <IconButton icon="arrow-left" label="Back to topics" onClick={() => navigate(`/c/${id}/topics`)} />
        <div className="row gap-8" style={{ minWidth: 0 }}>
          <span className="mono bold ellipsis" style={{ fontSize: 14 }}>
            {decoded}
          </span>
          {detail.data?.summary.internal ? <Badge tone="neutral">internal</Badge> : null}
          {detail.data?.summary.kind && detail.data.summary.kind !== 'topic' ? (
            <Badge tone="accent">{detail.data.summary.kind}</Badge>
          ) : null}
          {detail.loading ? <Spinner /> : null}
        </div>
        <ToolbarSpacer />
        <Button size="sm" icon="refresh" onClick={() => void detail.reload()}>
          Refresh
        </Button>
        {capabilities.topics.update ? (
          <Button size="sm" icon="edit" onClick={() => setEditing(true)}>
            Configure
          </Button>
        ) : null}
        {capabilities.topics.purge ? (
          <Button size="sm" variant="ghost" icon="reset" onClick={() => setPurgeOpen(true)}>
            Purge
          </Button>
        ) : null}
        {capabilities.topics.delete ? (
          <Button size="sm" variant="danger" icon="trash" onClick={() => setDeleteOpen(true)}>
            Delete
          </Button>
        ) : null}
      </Toolbar>

      <Tabs
        active={tab}
        onChange={setTab}
        tabs={[
          { id: 'messages', label: 'Messages', icon: 'search' },
          {
            id: 'tail',
            label: 'Live tail',
            icon: 'activity',
            disabled: !capabilities.messages.tail,
          },
          { id: 'produce', label: 'Produce', icon: 'send', disabled: !capabilities.messages.produce },
          { id: 'partitions', label: 'Partitions', icon: 'grid', count: partitions.length || undefined },
          { id: 'config', label: 'Configuration', icon: 'sliders', count: detail.data?.configs.length },
          { id: 'overview', label: 'Overview', icon: 'info' },
        ]}
      />

      {tab === 'messages' ? (
        <BrowsePanel
          connectionId={id}
          topic={decoded}
          partitions={partitionIds}
          watermarks={detail.data?.partitions ?? []}
          canSeekTimestamp={capabilities.messages.timestampSeek}
          canSeekOffset={capabilities.messages.offsetSeek}
          canSeekPartitions={capabilities.messages.partitions}
        />
      ) : null}

      {tab === 'tail' ? (
        <TailPanel
          connectionId={id}
          topic={decoded}
          partitions={partitionIds}
          canSeekTimestamp={capabilities.messages.timestampSeek}
        />
      ) : null}

      {tab === 'produce' ? (
        <div className="page" style={{ overflowY: 'auto' }}>
          <ProduceForm
            connectionId={id}
            topic={decoded}
            partitions={partitionIds}
            supportsKeys={capabilities.messages.keys}
            supportsHeaders={capabilities.messages.headers}
            supportsPartitions={capabilities.messages.partitions}
            onProduced={() => void detail.reload()}
          />
        </div>
      ) : null}

      {tab === 'partitions' ? (
        <PartitionsPanel partitions={partitions} loading={detail.loading} />
      ) : null}

      {tab === 'config' ? (
        <ConfigPanel configs={detail.data?.configs ?? []} canEdit={capabilities.topics.config} onEdit={() => setEditing(true)} />
      ) : null}

      {tab === 'overview' ? (
        <div className="page" style={{ overflowY: 'auto' }}>
          <Panel title="Summary">
            {detail.data ? (
              <DetailList
                items={[
                  { label: 'Name', value: <span className="mono">{detail.data.summary.name}</span> },
                  { label: 'Kind', value: detail.data.summary.kind },
                  { label: 'Internal', value: detail.data.summary.internal ? 'yes' : 'no' },
                  { label: 'Partitions', value: formatNumber(detail.data.summary.partitionCount) },
                  { label: 'Messages', value: formatNumber(detail.data.summary.messageCount) },
                  { label: 'Size', value: formatBytes(detail.data.summary.sizeBytes) },
                  { label: 'Consumers', value: formatNumber(detail.data.summary.consumerCount) },
                  ...detail.data.attributes.map((attribute) => ({
                    label: attribute.label,
                    value: attribute.mono ? <span className="mono">{attribute.value}</span> : attribute.value,
                  })),
                ]}
              />
            ) : (
              <span className="dim small">Loading…</span>
            )}
          </Panel>
        </div>
      ) : null}

      <Modal open={editing} onClose={() => setEditing(false)} title="Topic configuration" size="lg">
        <TopicForm
          mode="edit"
          connectionId={id}
          detail={detail.data ?? undefined}
          onCancel={() => setEditing(false)}
          onDone={async () => {
            setEditing(false);
            await detail.reload();
          }}
        />
      </Modal>

      <ConfirmDialog
        open={purgeOpen}
        title="Purge topic"
        message={
          <>
            Delete every record in <strong>{decoded}</strong>? Committed offsets stay where they are.
          </>
        }
        confirmLabel="Purge"
        busy={working}
        onCancel={() => setPurgeOpen(false)}
        onConfirm={async () => {
          setWorking(true);
          try {
            await topicsApi.purge(id, decoded);
            notifySuccess('Topic purged', decoded);
            setPurgeOpen(false);
            await detail.reload();
          } catch (error) {
            notifyError(error, 'Purge failed');
          } finally {
            setWorking(false);
          }
        }}
      />

      <ConfirmDialog
        open={deleteOpen}
        title="Delete topic"
        message={
          <>
            <strong>{decoded}</strong> will be removed permanently.
          </>
        }
        confirmLabel="Delete"
        busy={working}
        onCancel={() => setDeleteOpen(false)}
        onConfirm={async () => {
          setWorking(true);
          try {
            await topicsApi.remove(id, decoded);
            notifySuccess('Topic deleted', decoded);
            setDeleteOpen(false);
            navigate(`/c/${id}/topics`);
          } catch (error) {
            notifyError(error, 'Delete failed');
          } finally {
            setWorking(false);
          }
        }}
      />

      {detail.error ? (
        <div className="page">
          <Panel>
            <EmptyState
              icon="alert"
              title="Could not load the topic"
              text={detail.error}
              actions={<Button onClick={() => void detail.reload()}>Retry</Button>}
            />
          </Panel>
        </div>
      ) : null}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/* Browse                                                                     */
/* -------------------------------------------------------------------------- */

function toSeek(mode: SeekMode, offset: string, timestamp: string): SeekPosition | null {
  switch (mode) {
    case 'beginning':
      return { mode: 'beginning' };
    case 'end':
      return { mode: 'end' };
    case 'offset': {
      const value = Number(offset);
      return Number.isFinite(value) ? { mode: 'offset', offset: value } : null;
    }
    case 'timestamp': {
      const value = Date.parse(timestamp);
      return Number.isNaN(value) ? null : { mode: 'timestamp', timestamp: value };
    }
    default:
      return null;
  }
}

function BrowsePanel({
  connectionId,
  topic,
  partitions,
  watermarks,
  canSeekTimestamp,
  canSeekOffset,
  canSeekPartitions,
}: {
  connectionId: string;
  topic: string;
  partitions: number[];
  watermarks: PartitionInfo[];
  canSeekTimestamp: boolean;
  canSeekOffset: boolean;
  canSeekPartitions: boolean;
}) {
  const [mode, setMode] = useState<SeekMode>('end');
  const [offset, setOffset] = useState('0');
  const [timestamp, setTimestamp] = useState(() => new Date().toISOString().slice(0, 16));
  const [limit, setLimit] = useState('200');
  const [partition, setPartition] = useState('');
  const [keyword, setKeyword] = useState('');
  const [advanced, setAdvanced] = useState(false);
  const [result, setResult] = useState<BrowseResult | null>(null);
  const [selected, setSelected] = useState<Message | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const totals = useMemo(() => {
    const messages = watermarks.reduce(
      (sum, partitionInfo) =>
        sum +
        (partitionInfo.endOffset !== null && partitionInfo.beginOffset !== null
          ? Math.max(0, (partitionInfo.endOffset ?? 0) - (partitionInfo.beginOffset ?? 0))
          : 0),
      0,
    );
    return messages;
  }, [watermarks]);

  const fetch = async () => {
    const start = toSeek(mode, offset, timestamp);
    if (!start) {
      setError('The seek position is invalid.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const value = await messagesApi.browse(connectionId, {
        topic,
        partitions: canSeekPartitions && partition !== '' ? [Number(partition)] : null,
        start,
        limit: Math.max(1, Math.min(10_000, Number(limit) || 200)),
        timeoutMs: 15_000,
        keyword: keyword.trim() === '' ? null : keyword.trim(),
      });
      setResult(value);
      setSelected(value.messages[0] ?? null);
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : 'Browse failed');
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Toolbar>
        <div className="btn-group">
          <button
            type="button"
            className={`btn btn--sm ${mode === 'end' ? 'btn--active' : ''}`}
            onClick={() => setMode('end')}
          >
            Latest
          </button>
          <button
            type="button"
            className={`btn btn--sm ${mode === 'beginning' ? 'btn--active' : ''}`}
            onClick={() => setMode('beginning')}
          >
            Earliest
          </button>
          {canSeekOffset ? (
            <button
              type="button"
              className={`btn btn--sm ${mode === 'offset' ? 'btn--active' : ''}`}
              onClick={() => setMode('offset')}
            >
              Offset
            </button>
          ) : null}
          {canSeekTimestamp ? (
            <button
              type="button"
              className={`btn btn--sm ${mode === 'timestamp' ? 'btn--active' : ''}`}
              onClick={() => setMode('timestamp')}
            >
              Time
            </button>
          ) : null}
        </div>

        {mode === 'offset' ? (
          <input
            className="input input--mono"
            style={{ width: 110 }}
            value={offset}
            inputMode="numeric"
            onChange={(event) => setOffset(event.target.value.replace(/[^0-9]/g, ''))}
          />
        ) : null}

        {mode === 'timestamp' ? (
          <input
            className="input input--mono"
            style={{ width: 190 }}
            type="datetime-local"
            value={timestamp}
            onChange={(event) => setTimestamp(event.target.value)}
          />
        ) : null}

        <Select
          value={limit}
          onChange={setLimit}
          className="input"
          options={[
            { value: '50', label: '50' },
            { value: '200', label: '200' },
            { value: '500', label: '500' },
            { value: '2000', label: '2000' },
          ]}
        />

        <Button size="sm" variant={advanced ? 'outline' : 'ghost'} icon="filter" onClick={() => setAdvanced((value) => !value)}>
          Filters
        </Button>

        <ToolbarSpacer />
        {totals > 0 ? <span className="tiny dim nowrap">{formatNumber(totals)} retained</span> : null}
        <Button size="sm" variant="primary" icon="search" loading={busy} onClick={fetch}>
          Fetch
        </Button>
      </Toolbar>

      {advanced ? (
        <Toolbar>
          {canSeekPartitions ? (
            <Field label="Partition">
              <Select
                value={partition}
                onChange={setPartition}
                className="input"
                options={[
                  { value: '', label: 'All partitions' },
                  ...partitions.map((value) => ({ value: String(value), label: `Partition ${value}` })),
                ]}
              />
            </Field>
          ) : null}
          <Field label="Payload contains" help="Server side filter, case sensitive.">
            <input
              className="input"
              style={{ width: 240 }}
              value={keyword}
              onChange={(event) => setKeyword(event.target.value)}
              placeholder="order-42"
            />
          </Field>
          {error ? <span className="small text-danger">{error}</span> : null}
        </Toolbar>
      ) : null}

      {result ? (
        <div className="row gap-12 small dim" style={{ padding: '8px 14px', borderBottom: '1px solid var(--border-soft)' }}>
          <span>
            <strong className="muted">{formatNumber(result.messages.length)}</strong> messages
          </span>
          <span>scanned {formatNumber(result.scanned)}</span>
          <span>{result.elapsedMs} ms</span>
          {result.truncated ? <Badge tone="warning">limit reached</Badge> : null}
        </div>
      ) : null}

      <MessageTable
        messages={result?.messages ?? []}
        loading={busy}
        selectedId={selected?.id}
        onSelect={setSelected}
        emptyTitle={result ? 'No messages matched' : 'Nothing fetched yet'}
        emptyText={
          result
            ? 'Try an earlier offset, a different partition, or drop the filter.'
            : 'Pick a seek position and hit Fetch.'
        }
      />

      <Modal
        open={selected !== null}
        onClose={() => setSelected(null)}
        variant="right"
        title="Message detail"
        subtitle={selected ? `partition ${selected.partition} · offset ${selected.offset}` : undefined}
      >
        {selected ? <MessageDrawer message={selected} /> : null}
      </Modal>
    </>
  );
}

/* -------------------------------------------------------------------------- */
/* Live tail                                                                  */
/* -------------------------------------------------------------------------- */

function TailPanel({
  connectionId,
  topic,
  partitions,
  canSeekTimestamp,
}: {
  connectionId: string;
  topic: string;
  partitions: number[];
  canSeekTimestamp: boolean;
}) {
  const key = streamKey(connectionId, topic);
  const slot = useStreams((state) => state.slots[key]);
  const start = useStreams((state) => state.start);
  const stop = useStreams((state) => state.stop);
  const clear = useStreams((state) => state.clear);
  const togglePause = useStreams((state) => state.togglePause);

  const [mode, setMode] = useState<SeekMode>('end');
  const [timestamp, setTimestamp] = useState(() => new Date().toISOString().slice(0, 16));
  const [maxMessages, setMaxMessages] = useState('1000');
  const [partition, setPartition] = useState('');
  const [selected, setSelected] = useState<Message | null>(null);

  const running = Boolean(slot?.jobId);

  const launch = () => {
    const position: SeekPosition =
      mode === 'timestamp' && canSeekTimestamp
        ? { mode: 'timestamp', timestamp: Date.parse(timestamp) || Date.now() }
        : mode === 'beginning'
          ? { mode: 'beginning' }
          : { mode: 'end' };

    void start(connectionId, topic, {
      topic,
      partitions: partition === '' ? null : [Number(partition)],
      start: position,
      maxMessages: Math.max(1, Number(maxMessages) || 1000),
      idleTimeoutMs: 15_000,
    });
  };

  return (
    <>
      <Toolbar>
        <div className="btn-group">
          <button
            type="button"
            className={`btn btn--sm ${mode === 'end' ? 'btn--active' : ''}`}
            onClick={() => setMode('end')}
          >
            New only
          </button>
          <button
            type="button"
            className={`btn btn--sm ${mode === 'beginning' ? 'btn--active' : ''}`}
            onClick={() => setMode('beginning')}
          >
            From start
          </button>
          {canSeekTimestamp ? (
            <button
              type="button"
              className={`btn btn--sm ${mode === 'timestamp' ? 'btn--active' : ''}`}
              onClick={() => setMode('timestamp')}
            >
              From time
            </button>
          ) : null}
        </div>

        {mode === 'timestamp' ? (
          <input
            className="input input--mono"
            style={{ width: 190 }}
            type="datetime-local"
            value={timestamp}
            onChange={(event) => setTimestamp(event.target.value)}
          />
        ) : null}

        <Select
          value={maxMessages}
          onChange={setMaxMessages}
          className="input"
          options={[
            { value: '100', label: '100 msgs' },
            { value: '1000', label: '1 000 msgs' },
            { value: '10000', label: '10 000 msgs' },
            { value: '100000', label: 'unlimited' },
          ]}
        />

        <Select
          value={partition}
          onChange={setPartition}
          className="input"
          options={[
            { value: '', label: 'All partitions' },
            ...partitions.map((value) => ({ value: String(value), label: `Partition ${value}` })),
          ]}
        />

        <ToolbarSpacer />

        {slot ? (
          <Badge tone={slot.error ? 'danger' : running ? 'success' : 'neutral'}>
            {slot.error ?? (running ? (slot.paused ? 'paused' : 'streaming') : slot.state)}
          </Badge>
        ) : null}

        <Button size="sm" variant="ghost" icon="trash" onClick={() => clear(key)} disabled={!slot}>
          Clear
        </Button>
        <Button
          size="sm"
          variant="ghost"
          icon={slot?.paused ? 'play' : 'pause'}
          onClick={() => togglePause(key)}
          disabled={!running}
        >
          {slot?.paused ? 'Resume' : 'Pause'}
        </Button>
        {running ? (
          <Button size="sm" variant="danger" icon="square" onClick={() => void stop(key)}>
            Stop
          </Button>
        ) : (
          <Button size="sm" variant="primary" icon="play" onClick={launch}>
            Start tail
          </Button>
        )}
      </Toolbar>

      <div
        className="row gap-12 small dim"
        style={{ padding: '8px 14px', borderBottom: '1px solid var(--border-soft)' }}
      >
        <span>
          received <strong className="muted">{formatNumber(slot?.received ?? 0)}</strong>
        </span>
        <span>buffered {formatNumber(slot?.messages.length ?? 0)}</span>
        <span className="row gap-6">
          <span className={running ? 'dot dot--connected' : 'dot'} />
          {running ? 'consumer attached, group untouched' : 'idle'}
        </span>
      </div>

      <MessageTable
        messages={slot?.messages ?? []}
        selectedId={selected?.id}
        onSelect={setSelected}
        initialSort={{ key: 'offset', direction: 'asc' }}
        emptyTitle={running ? 'Waiting for new messages' : 'Tail is not running'}
        emptyText="A dedicated consumer is created for this session; your real consumer groups are never joined."
      />

      <Modal
        open={selected !== null}
        onClose={() => setSelected(null)}
        variant="right"
        title="Message detail"
        subtitle={selected ? `partition ${selected.partition} · offset ${selected.offset}` : undefined}
      >
        {selected ? <MessageDrawer message={selected} /> : null}
      </Modal>
    </>
  );
}

/* -------------------------------------------------------------------------- */
/* Partitions + configuration                                                 */
/* -------------------------------------------------------------------------- */

function PartitionsPanel({ partitions, loading }: { partitions: PartitionInfo[]; loading: boolean }) {
  const columns: Array<Column<PartitionInfo>> = [
    {
      key: 'id',
      header: 'Partition',
      width: 100,
      align: 'right',
      render: (partition) => <span className="mono-cell">{partition.id}</span>,
      sortValue: (partition) => partition.id,
    },
    {
      key: 'leader',
      header: 'Leader',
      width: 90,
      align: 'right',
      render: (partition) =>
        partition.leader === null || partition.leader === undefined ? (
          <span className="text-danger">none</span>
        ) : (
          <span className="mono-cell">{partition.leader}</span>
        ),
      sortValue: (partition) => partition.leader ?? -1,
    },
    {
      key: 'replicas',
      header: 'Replicas',
      render: (partition) => (
        <span className="row gap-4 wrap">
          {partition.replicas.map((replica) => (
            <Badge key={replica} mono tone="neutral">
              {replica}
            </Badge>
          ))}
        </span>
      ),
    },
    {
      key: 'isr',
      header: 'In sync',
      render: (partition) => (
        <span className="row gap-4 wrap">
          {partition.isr.map((replica) => (
            <Badge key={replica} mono tone="success">
              {replica}
            </Badge>
          ))}
          {partition.offlineReplicas.map((replica) => (
            <Badge key={`offline-${replica}`} mono tone="danger">
              {replica}†
            </Badge>
          ))}
        </span>
      ),
    },
    {
      key: 'begin',
      header: 'Earliest',
      width: 110,
      align: 'right',
      render: (partition) => <span className="mono-cell dim">{formatNumber(partition.beginOffset)}</span>,
      sortValue: (partition) => partition.beginOffset ?? -1,
    },
    {
      key: 'end',
      header: 'Latest',
      width: 110,
      align: 'right',
      render: (partition) => <span className="mono-cell">{formatNumber(partition.endOffset)}</span>,
      sortValue: (partition) => partition.endOffset ?? -1,
    },
    {
      key: 'count',
      header: 'Messages',
      width: 110,
      align: 'right',
      render: (partition) => <span className="mono-cell">{formatNumber(partition.messageCount)}</span>,
      sortValue: (partition) => partition.messageCount ?? -1,
    },
  ];

  return (
    <DataTable
      columns={columns}
      rows={partitions}
      rowKey={(partition) => String(partition.id)}
      loading={loading}
      initialSort={{ key: 'id', direction: 'asc' }}
      emptyTitle="No partition metadata"
    />
  );
}

function ConfigPanel({
  configs,
  canEdit,
  onEdit,
}: {
  configs: ConfigEntry[];
  canEdit: boolean;
  onEdit: () => void;
}) {
  const [query, setQuery] = useState('');

  const rows = useMemo(() => {
    const list = configs.filter((config) => config.value !== null && config.value !== undefined);
    if (!query.trim()) return list;
    const needle = query.toLowerCase();
    return list.filter((config) => config.name.toLowerCase().includes(needle));
  }, [configs, query]);

  const columns: Array<Column<ConfigEntry>> = [
    {
      key: 'name',
      header: 'Name',
      render: (config) => <span className="mono-cell">{config.name}</span>,
      sortValue: (config) => config.name,
    },
    {
      key: 'value',
      header: 'Value',
      render: (config) => (
        <span className="mono-cell ellipsis" title={config.value ?? ''}>
          {config.sensitive ? '••••••' : config.value}
        </span>
      ),
    },
    {
      key: 'source',
      header: 'Source',
      width: 160,
      render: (config) =>
        config.source ? <Badge tone="neutral">{config.source}</Badge> : <span className="dim">—</span>,
    },
    {
      key: 'flags',
      header: '',
      width: 150,
      render: (config) => (
        <span className="row gap-4" style={{ justifyContent: 'flex-end' }}>
          {config.isDefault ? <Badge tone="neutral">default</Badge> : <Badge tone="accent">override</Badge>}
          {config.readOnly ? <Badge tone="warning">read only</Badge> : null}
        </span>
      ),
    },
  ];

  return (
    <>
      <Toolbar>
        <input
          className="input input--search"
          style={{ width: 240 }}
          placeholder="Filter configuration…"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
        <span className="tiny dim nowrap">
          {rows.length} of {configs.length}
        </span>
        <ToolbarSpacer />
        {canEdit ? (
          <Button size="sm" icon="edit" onClick={onEdit}>
            Edit overrides
          </Button>
        ) : null}
      </Toolbar>
      <DataTable
        columns={columns}
        rows={rows}
        rowKey={(config) => config.name}
        initialSort={{ key: 'name', direction: 'asc' }}
        emptyTitle="No configuration reported"
        emptyText="Drivers that do not expose topic configuration show nothing here."
      />
    </>
  );
}
