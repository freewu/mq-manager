import { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { OffsetTable } from '@/components/domain/Tables';
import { DataTable, type Column } from '@/components/ui/Table';
import { ConfirmDialog, Modal } from '@/components/ui/Modal';
import { Field, Select } from '@/components/ui/Field';
import {
  Badge,
  Button,
  DetailList,
  EmptyState,
  IconButton,
  Metric,
  Panel,
  Toolbar,
  ToolbarSpacer,
} from '@/components/ui/primitives';
import { groupsApi } from '@/api/groups';
import { useAsync } from '@/hooks/useAsync';
import { useCapabilities, useConnection } from '@/hooks/useConnection';
import { formatNumber } from '@/lib/format';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';
import type { GroupMember, GroupOffset, ResetMode } from '@/types';

const offsetKey = (offset: GroupOffset) => `${offset.topic}::${offset.partition ?? -1}`;

export function GroupDetail() {
  const { id = '', group = '' } = useParams();
  const navigate = useNavigate();
  const decoded = decodeURIComponent(group);

  const { profile } = useConnection(id);
  const capabilities = useCapabilities(id);
  const connected = useWorkspace((state) => state.statuses[id]?.state === 'connected');

  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [resetOpen, setResetOpen] = useState(false);
  const [mode, setMode] = useState<ResetMode>('earliest');
  const [offsetValue, setOffsetValue] = useState('0');
  const [working, setWorking] = useState(false);
  const [deleteOpen, setDeleteOpen] = useState(false);

  const detail = useAsync(() => groupsApi.detail(id, decoded), [id, decoded], {
    enabled: connected && capabilities.groups.list,
  });

  const offsets = detail.data?.offsets ?? [];
  const members = detail.data?.members ?? [];
  const summary = detail.data?.summary;

  const totalLag = useMemo(
    () => offsets.reduce((sum, offset) => sum + Math.max(0, offset.lag ?? 0), 0),
    [offsets],
  );

  const topics = useMemo(() => {
    const unique = new Set<string>();
    for (const offset of offsets) unique.add(offset.topic);
    return [...unique];
  }, [offsets]);

  if (!profile || !connected) {
    return (
      <div className="page">
        <EmptyState
          icon="plug"
          title={profile ? 'Not connected' : 'Connection not found'}
          actions={<Button onClick={() => navigate(`/c/${id}/groups`)}>Back to groups</Button>}
        />
      </div>
    );
  }

  const resetPartitions = (): number[] | null => {
    const chosen = offsets.filter((offset) => selected.has(offsetKey(offset)));
    if (chosen.length === 0) return null;
    const partitions = chosen
      .map((offset) => offset.partition)
      .filter((value): value is number => value !== null && value !== undefined);
    return partitions.length === 0 ? null : partitions;
  };

  return (
    <div className="page">
      <Toolbar style={{ margin: '-18px -20px 16px', borderRadius: 0 }}>
        <IconButton icon="arrow-left" label="Back to groups" onClick={() => navigate(`/c/${id}/groups`)} />
        <span className="mono bold grow ellipsis" style={{ fontSize: 14 }}>
          {decoded}
        </span>
        <ToolbarSpacer />
        {selected.size > 0 ? (
          <span className="tiny dim nowrap">{selected.size} partition(s) selected</span>
        ) : null}
        {capabilities.groups.resetOffsets ? (
          <Button
            size="sm"
            icon="reset"
            disabled={selected.size === 0}
            onClick={() => setResetOpen(true)}
          >
            Reset offsets
          </Button>
        ) : null}
        {capabilities.groups.delete ? (
          <Button size="sm" variant="danger" icon="trash" onClick={() => setDeleteOpen(true)}>
            Delete
          </Button>
        ) : null}
        <Button size="sm" variant="ghost" icon="refresh" onClick={() => void detail.reload()}>
          Refresh
        </Button>
      </Toolbar>

      <div className="grid-cards mb-8">
        <Metric
          label="State"
          value={summary?.state ?? '—'}
          sub={summary?.protocolType ? `protocol ${summary.protocolType}` : undefined}
          icon="activity"
        />
        <Metric label="Members" value={formatNumber(summary?.memberCount ?? members.length)} icon="users" />
        <Metric label="Topics" value={formatNumber(topics.length)} icon="layers" />
        <Metric
          label="Total lag"
          value={formatNumber(totalLag)}
          sub={totalLag === 0 ? 'fully caught up' : 'messages behind'}
          icon="bar-chart"
        />
      </div>

      <div className="stack-16">
        {capabilities.groups.members ? (
          <Panel title={`Members (${members.length})`} flush>
            <MemberTable members={members} />
          </Panel>
        ) : null}

        <Panel
          title="Committed offsets"
          actions={
            capabilities.groups.resetOffsets && offsets.length > 0 ? (
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  setSelected((current) =>
                    current.size === offsets.length
                      ? new Set()
                      : new Set(offsets.map((offset) => offsetKey(offset))),
                  )
                }
              >
                {selected.size === offsets.length ? 'Clear selection' : 'Select all'}
              </Button>
            ) : null
          }
          flush
        >
          <OffsetTable
            offsets={offsets}
            canReset={capabilities.groups.resetOffsets}
            onSelect={(offset) =>
              setSelected((current) => {
                const next = new Set(current);
                const key = offsetKey(offset);
                if (next.has(key)) next.delete(key);
                else next.add(key);
                return next;
              })
            }
            selected={(offset) => selected.has(offsetKey(offset))}
          />
        </Panel>

        {summary ? (
          <Panel title="Summary">
            <DetailList
              items={[
                { label: 'Group', value: <span className="mono">{summary.id}</span> },
                { label: 'State', value: summary.state ?? '—' },
                { label: 'Protocol', value: summary.protocolType ?? '—' },
                { label: 'Kind', value: summary.kind ?? '—' },
                { label: 'Members', value: formatNumber(summary.memberCount) },
                {
                  label: 'Topics',
                  value: <span className="mono">{summary.topics.join(', ') || '—'}</span>,
                },
                { label: 'Total lag', value: formatNumber(summary.totalLag ?? totalLag) },
              ]}
            />
          </Panel>
        ) : null}

        {detail.error ? (
          <Panel>
            <EmptyState icon="alert" title="Could not load the group" text={detail.error} />
          </Panel>
        ) : null}
      </div>

      <Modal
        open={resetOpen}
        onClose={() => setResetOpen(false)}
        title="Reset offsets"
        subtitle={`${selected.size} partition(s) in ${decoded}`}
        size="sm"
        footer={
          <>
            <Button variant="ghost" onClick={() => setResetOpen(false)} disabled={working}>
              Cancel
            </Button>
            <Button
              variant="primary"
              icon="reset"
              loading={working}
              onClick={async () => {
                const partitions = resetPartitions();
                const topic = offsets.find((offset) => selected.has(offsetKey(offset)))?.topic;
                if (!topic) return;
                setWorking(true);
                try {
                  await groupsApi.resetOffsets(id, {
                    group: decoded,
                    topic,
                    partitions,
                    mode,
                    offset: mode === 'offset' ? Number(offsetValue) || 0 : null,
                  });
                  notifySuccess('Offsets reset', decoded);
                  setResetOpen(false);
                  setSelected(new Set());
                  await detail.reload();
                } catch (error) {
                  notifyError(error, 'Reset failed');
                } finally {
                  setWorking(false);
                }
              }}
            >
              Apply
            </Button>
          </>
        }
      >
        <div className="stack-12">
          <Field label="Target">
            <Select
              value={mode}
              onChange={(value) => setMode(value as ResetMode)}
              options={[
                { value: 'earliest', label: 'Earliest — replay from the start' },
                { value: 'latest', label: 'Latest — skip to the end' },
                { value: 'offset', label: 'Specific offset' },
              ]}
            />
          </Field>
          {mode === 'offset' ? (
            <Field label="Offset" help="Applied to every selected partition.">
              <input
                className="input input--mono"
                inputMode="numeric"
                value={offsetValue}
                onChange={(event) => setOffsetValue(event.target.value.replace(/[^0-9]/g, ''))}
              />
            </Field>
          ) : null}
          <div className="small dim">
            The offset is committed on behalf of the group. Consumers that are currently running may
            overwrite it on their next commit.
          </div>
        </div>
      </Modal>

      <ConfirmDialog
        open={deleteOpen}
        title="Delete consumer group"
        message={
          <>
            <strong>{decoded}</strong> will be removed from the broker.
          </>
        }
        confirmLabel="Delete group"
        busy={working}
        onCancel={() => setDeleteOpen(false)}
        onConfirm={async () => {
          setWorking(true);
          try {
            await groupsApi.remove(id, decoded);
            notifySuccess('Group deleted', decoded);
            setDeleteOpen(false);
            navigate(`/c/${id}/groups`);
          } catch (error) {
            notifyError(error, 'Delete failed');
          } finally {
            setWorking(false);
          }
        }}
      />
    </div>
  );
}

function MemberTable({ members }: { members: GroupMember[] }) {
  const columns: Array<Column<GroupMember>> = [
    {
      key: 'id',
      header: 'Member',
      render: (member) => (
        <span className="mono-cell ellipsis" title={member.id}>
          {member.id}
        </span>
      ),
    },
    {
      key: 'client',
      header: 'Client',
      width: 200,
      render: (member) => <span className="mono-cell dim">{member.clientId ?? '—'}</span>,
    },
    {
      key: 'host',
      header: 'Host',
      width: 190,
      render: (member) => <span className="mono-cell dim">{member.clientHost ?? '—'}</span>,
    },
    {
      key: 'assignment',
      header: 'Assignment',
      render: (member) => (
        <span className="row gap-6 wrap">
          {member.assignments.length === 0 ? (
            <span className="dim">—</span>
          ) : (
            member.assignments.map((assignment) => (
              <Badge key={assignment.topic} mono tone="accent">
                {assignment.topic}[{assignment.partitions.join(',')}]
              </Badge>
            ))
          )}
        </span>
      ),
    },
  ];

  return (
    <DataTable
      columns={columns}
      rows={members}
      rowKey={(member) => member.id}
      emptyTitle="No active members"
      emptyText="The group has committed offsets but nothing is consuming right now."
    />
  );
}
