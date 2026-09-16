import { Badge, Button, IconButton, Progress } from '@/components/ui/primitives';
import { DataTable, type Column } from '@/components/ui/Table';
import { formatBytes, formatNumber } from '@/lib/format';
import type { ConsumerGroupSummary, GroupOffset, TopicSummary } from '@/types';

export function TopicTable({
  topics,
  onOpen,
  onPurge,
  onDelete,
  canPurge,
  canDelete,
  canPartition = true,
}: {
  topics: TopicSummary[];
  onOpen: (topic: TopicSummary) => void;
  onPurge: (topic: TopicSummary) => void;
  onDelete: (topic: TopicSummary) => void;
  canPurge: boolean;
  canDelete: boolean;
  /** `false` hides the partition column for queue based brokers. */
  canPartition?: boolean;
}) {
  const columns: Array<Column<TopicSummary>> = [
    {
      key: 'name',
      header: 'Name',
      render: (topic) => (
        <div className="row gap-6" style={{ minWidth: 0 }}>
          <span className="mono-cell ellipsis" style={{ color: 'var(--text)' }}>
            {topic.name}
          </span>
          {topic.internal ? <Badge tone="neutral">internal</Badge> : null}
          {topic.kind !== 'topic' ? <Badge tone="accent">{topic.kind}</Badge> : null}
        </div>
      ),
      sortValue: (topic) => topic.name,
    },
    ...(canPartition
      ? [
          {
            key: 'partitions',
            header: 'Partitions',
            width: 110,
            align: 'right' as const,
            render: (topic: TopicSummary) => (
              <span className="mono-cell">{formatNumber(topic.partitionCount)}</span>
            ),
            sortValue: (topic: TopicSummary) => topic.partitionCount ?? -1,
          },
        ]
      : []),
    {
      key: 'messages',
      header: 'Messages',
      width: 116,
      align: 'right',
      render: (topic) => <span className="mono-cell">{formatNumber(topic.messageCount)}</span>,
      sortValue: (topic) => topic.messageCount ?? -1,
    },
    {
      key: 'consumers',
      header: 'Consumers',
      width: 110,
      align: 'right',
      render: (topic) => <span className="mono-cell">{formatNumber(topic.consumerCount)}</span>,
      sortValue: (topic) => topic.consumerCount ?? -1,
    },
    {
      key: 'size',
      header: 'Size',
      width: 100,
      align: 'right',
      render: (topic) => <span className="mono-cell dim">{formatBytes(topic.sizeBytes)}</span>,
      sortValue: (topic) => topic.sizeBytes ?? -1,
    },
    {
      key: 'actions',
      header: '',
      width: 110,
      align: 'right',
      render: (topic) => (
        <span
          className="row gap-4"
          style={{ justifyContent: 'flex-end' }}
          onClick={(event) => event.stopPropagation()}
        >
          {canPurge ? (
            <IconButton
              icon="reset"
              label="Purge messages"
              size="sm"
              onClick={() => onPurge(topic)}
            />
          ) : null}
          {canDelete ? (
            <IconButton
              icon="trash"
              label="Delete topic"
              size="sm"
              variant="danger"
              onClick={() => onDelete(topic)}
            />
          ) : null}
        </span>
      ),
    },
  ];

  return (
    <DataTable
      columns={columns}
      rows={topics}
      rowKey={(topic) => topic.name}
      onRowClick={onOpen}
      initialSort={{ key: 'name', direction: 'asc' }}
      emptyTitle="No topics"
      emptyText="Nothing matched the current filter."
    />
  );
}

export function ConsumerGroupTable({
  groups,
  onOpen,
  onDelete,
  canDelete,
}: {
  groups: ConsumerGroupSummary[];
  onOpen: (group: ConsumerGroupSummary) => void;
  onDelete: (group: ConsumerGroupSummary) => void;
  canDelete: boolean;
}) {
  const columns: Array<Column<ConsumerGroupSummary>> = [
    {
      key: 'id',
      header: 'Group',
      render: (group) => <span className="mono-cell">{group.id}</span>,
      sortValue: (group) => group.id,
    },
    {
      key: 'state',
      header: 'State',
      width: 150,
      render: (group) =>
        group.state ? (
          <Badge tone={group.state.toLowerCase() === 'stable' ? 'success' : 'warning'}>
            {group.state}
          </Badge>
        ) : (
          <span className="dim">—</span>
        ),
    },
    {
      key: 'members',
      header: 'Members',
      width: 100,
      align: 'right',
      render: (group) => <span className="mono-cell">{formatNumber(group.memberCount)}</span>,
      sortValue: (group) => group.memberCount,
    },
    {
      key: 'topics',
      header: 'Topics',
      width: 110,
      align: 'right',
      render: (group) => <span className="mono-cell">{formatNumber(group.topics.length)}</span>,
      sortValue: (group) => group.topics.length,
    },
    {
      key: 'lag',
      header: 'Total lag',
      width: 190,
      render: (group) =>
        group.totalLag === null || group.totalLag === undefined ? (
          <span className="dim">—</span>
        ) : (
          <LagBar lag={group.totalLag} />
        ),
      sortValue: (group) => group.totalLag ?? -1,
    },
    {
      key: 'actions',
      header: '',
      width: 60,
      align: 'right',
      render: (group) =>
        canDelete ? (
          <span onClick={(event) => event.stopPropagation()}>
            <IconButton
              icon="trash"
              label="Delete group"
              size="sm"
              variant="danger"
              onClick={() => onDelete(group)}
            />
          </span>
        ) : null,
    },
  ];

  return (
    <DataTable
      columns={columns}
      rows={groups}
      rowKey={(group) => group.id}
      onRowClick={onOpen}
      initialSort={{ key: 'lag', direction: 'desc' }}
      emptyTitle="No consumer groups"
    />
  );
}

/** Lag is unbounded, so the bar scales against a log-ish reference. */
export function LagBar({ lag, max }: { lag: number; max?: number }) {
  const reference = max ?? Math.max(lag, 10);
  const tone = lag === 0 ? 'var(--success)' : lag > 10_000 ? 'var(--danger)' : 'var(--accent)';
  return (
    <div className="lag-bar">
      <div className="lag-bar__track" title={`${formatNumber(lag)} messages behind`}>
        <div
          className="lag-bar__fill"
          style={{
            width: `${Math.min(100, (lag / reference) * 100)}%`,
            background: tone,
          }}
        />
      </div>
      <span className="lag-bar__value">{formatNumber(lag)}</span>
    </div>
  );
}

export function OffsetTable({
  offsets,
  onSelect,
  selected,
  canReset,
}: {
  offsets: GroupOffset[];
  onSelect?: (offset: GroupOffset) => void;
  selected?: (offset: GroupOffset) => boolean;
  canReset?: boolean;
}) {
  const maxLag = offsets.reduce((max, offset) => Math.max(max, offset.lag ?? 0), 0);

  const columns: Array<Column<GroupOffset>> = [
    {
      key: 'topic',
      header: 'Topic',
      render: (offset) => <span className="mono-cell">{offset.topic}</span>,
      sortValue: (offset) => offset.topic,
    },
    {
      key: 'partition',
      header: 'P',
      width: 60,
      align: 'right',
      render: (offset) => <span className="mono-cell">{offset.partition ?? '—'}</span>,
      sortValue: (offset) => offset.partition ?? -1,
    },
    {
      key: 'current',
      header: 'Committed',
      width: 116,
      align: 'right',
      render: (offset) => <span className="mono-cell">{formatNumber(offset.currentOffset)}</span>,
      sortValue: (offset) => offset.currentOffset ?? -1,
    },
    {
      key: 'begin',
      header: 'Earliest',
      width: 110,
      align: 'right',
      render: (offset) => <span className="mono-cell dim">{formatNumber(offset.beginOffset)}</span>,
      sortValue: (offset) => offset.beginOffset ?? -1,
    },
    {
      key: 'end',
      header: 'Latest',
      width: 110,
      align: 'right',
      render: (offset) => <span className="mono-cell dim">{formatNumber(offset.endOffset)}</span>,
      sortValue: (offset) => offset.endOffset ?? -1,
    },
    {
      key: 'lag',
      header: 'Lag',
      width: 190,
      render: (offset) =>
        offset.lag === null || offset.lag === undefined ? (
          <span className="dim">—</span>
        ) : (
          <LagBar lag={offset.lag} max={maxLag} />
        ),
      sortValue: (offset) => offset.lag ?? -1,
    },
    {
      key: 'metadata',
      header: 'Metadata',
      render: (offset) =>
        offset.metadata ? <span className="tiny dim ellipsis">{offset.metadata}</span> : <span className="dim">—</span>,
    },
  ];

  return (
    <DataTable
      columns={columns}
      rows={offsets}
      rowKey={(offset, index) => `${offset.topic}-${offset.partition ?? index}`}
      onRowClick={canReset ? onSelect : undefined}
      isSelected={selected}
      initialSort={{ key: 'lag', direction: 'desc' }}
      emptyTitle="No committed offsets"
      emptyText="This group has not committed anything yet."
    />
  );
}

/** Reusable “N of M” progress row for long running fetches. */
export function CoverageBar({ done, total }: { done: number; total: number }) {
  if (total <= 0) return null;
  return (
    <div className="row gap-8">
      <Progress value={done} max={total} />
      <span className="tiny dim mono nowrap">
        {done}/{total}
      </span>
    </div>
  );
}

export function ReadOnlyHint() {
  return (
    <Button size="sm" variant="ghost" disabled icon="info">
      read-only driver
    </Button>
  );
}
