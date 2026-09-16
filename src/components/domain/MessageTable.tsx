import type { ReactNode } from 'react';

import { Badge, CopyButton, DetailList, Panel } from '@/components/ui/primitives';
import { DataTable, type Column } from '@/components/ui/Table';
import { formatBytes, formatNumber, formatTimestamp, type TimestampMode } from '@/lib/format';
import { compact, prettify } from '@/lib/json';
import type { Message } from '@/types';

function preview(message: Message): string {
  if (message.payload === null || message.payload === undefined || message.payload === '') {
    return '(empty)';
  }
  if (message.encoding === 'base64') return message.payload;
  return compact(message.payload, 180);
}

export function MessageTable({
  messages,
  timestampMode = 'datetime',
  loading = false,
  selectedId,
  onSelect,
  emptyTitle = 'No messages',
  emptyText,
  initialSort = { key: 'offset', direction: 'desc' },
}: {
  messages: Message[];
  timestampMode?: TimestampMode;
  loading?: boolean;
  selectedId?: string;
  onSelect?: (message: Message) => void;
  emptyTitle?: string;
  emptyText?: ReactNode;
  initialSort?: { key: string; direction: 'asc' | 'desc' };
}) {
  const columns: Array<Column<Message>> = [
    {
      key: 'partition',
      header: 'P',
      width: 56,
      align: 'right',
      render: (message) => <span className="mono-cell">{message.partition ?? '—'}</span>,
      sortValue: (message) => message.partition ?? -1,
    },
    {
      key: 'offset',
      header: 'Offset',
      width: 96,
      align: 'right',
      render: (message) => <span className="mono-cell">{formatNumber(message.offset)}</span>,
      sortValue: (message) => message.offset ?? -1,
    },
    {
      key: 'timestamp',
      header: 'Timestamp',
      width: 172,
      render: (message) => (
        <span className="mono-cell dim">{formatTimestamp(message.timestamp, timestampMode)}</span>
      ),
      sortValue: (message) => message.timestamp ?? 0,
    },
    {
      key: 'key',
      header: 'Key',
      width: 160,
      render: (message) =>
        message.key ? (
          <span className="mono-cell ellipsis" title={message.key}>
            {message.key}
          </span>
        ) : (
          <span className="dim">—</span>
        ),
    },
    {
      key: 'payload',
      header: 'Payload',
      render: (message) => (
        <div className="row gap-6" style={{ minWidth: 0 }}>
          <span className="payload-cell ellipsis" title={preview(message)}>
            {preview(message)}
          </span>
          {message.encoding === 'base64' ? <Badge tone="info">b64</Badge> : null}
          {message.headers.length > 0 ? (
            <Badge tone="neutral" title={`${message.headers.length} headers`}>
              h{message.headers.length}
            </Badge>
          ) : null}
        </div>
      ),
    },
    {
      key: 'size',
      header: 'Size',
      width: 84,
      align: 'right',
      render: (message) => <span className="mono-cell dim">{formatBytes(message.size)}</span>,
      sortValue: (message) => message.size,
    },
  ];

  return (
    <DataTable
      columns={columns}
      rows={messages}
      rowKey={(message, index) => message.id || `${message.partition}-${message.offset}-${index}`}
      loading={loading}
      initialSort={initialSort}
      emptyTitle={emptyTitle}
      emptyText={emptyText}
      onRowClick={onSelect}
      isSelected={(message) => message.id === selectedId}
    />
  );
}

export function MessageDrawer({ message }: { message: Message }) {
  const pretty =
    message.encoding === 'base64' ? message.payload ?? '' : prettify(message.payload, true);

  return (
    <div className="stack-16">
      <Panel title="Metadata">
        <DetailList
          items={[
            { label: 'Topic', value: <span className="mono">{message.topic}</span> },
            { label: 'Partition', value: <span className="mono">{message.partition ?? '—'}</span> },
            { label: 'Offset', value: <span className="mono">{formatNumber(message.offset)}</span> },
            {
              label: 'Timestamp',
              value: (
                <span className="mono">
                  {formatTimestamp(message.timestamp, 'datetime')}
                  {message.timestamp ? (
                    <span className="dim"> ({message.timestamp})</span>
                  ) : null}
                </span>
              ),
            },
            { label: 'Producer', value: message.producer ?? '—' },
            { label: 'Size', value: formatBytes(message.size) },
            { label: 'Encoding', value: <Badge tone={message.encoding === 'base64' ? 'info' : 'neutral'}>{message.encoding}</Badge> },
          ]}
        />
      </Panel>

      <Panel
        title="Key"
        actions={<CopyButton value={message.key ?? ''} />}
        bodyClassName="panel__body--tight"
      >
        {message.key ? (
          <div className="code-block code-block--inline mono" data-selectable>
            {message.key}
          </div>
        ) : (
          <span className="dim small">No key</span>
        )}
      </Panel>

      <Panel title="Headers" flush>
        {message.headers.length === 0 ? (
          <div className="panel__body dim small">No headers</div>
        ) : (
          <table className="table">
            <tbody>
              {message.headers.map((header, index) => (
                <tr key={`${header.key}-${index}`}>
                  <td style={{ width: '38%' }} className="mono-cell">
                    {header.key}
                  </td>
                  <td className="mono-cell" style={{ whiteSpace: 'pre-wrap' }} data-selectable>
                    {header.value ?? '—'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </Panel>

      <Panel
        title={`Payload · ${message.encoding === 'base64' ? 'base64' : 'utf-8'}`}
        actions={<CopyButton value={message.payload ?? ''} />}
        bodyClassName="panel__body--tight"
      >
        {message.payload ? (
          <pre className="code-block" data-selectable>
            {pretty}
          </pre>
        ) : (
          <span className="dim small">Empty payload</span>
        )}
      </Panel>
    </div>
  );
}
