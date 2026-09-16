import { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { PageHeader } from '@/components/layout/PageHeader';
import { ConsumerGroupTable } from '@/components/domain/Tables';
import { ConfirmDialog } from '@/components/ui/Modal';
import { Button, EmptyState, Panel, SearchInput, Toolbar, ToolbarSpacer } from '@/components/ui/primitives';
import { groupsApi } from '@/api/groups';
import { useAsync } from '@/hooks/useAsync';
import { useCapabilities, useConnection } from '@/hooks/useConnection';
import { useDebounced } from '@/hooks/useDebounced';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';
import type { ConsumerGroupSummary } from '@/types';

export function Groups() {
  const { id = '' } = useParams();
  const navigate = useNavigate();

  const { profile } = useConnection(id);
  const capabilities = useCapabilities(id);
  const connected = useWorkspace((state) => state.statuses[id]?.state === 'connected');
  const connect = useWorkspace((value) => value.connect);

  const [query, setQuery] = useState('');
  const [deleteTarget, setDeleteTarget] = useState<ConsumerGroupSummary | null>(null);
  const [working, setWorking] = useState(false);
  const debounced = useDebounced(query, 200);

  const groups = useAsync(() => groupsApi.list(id), [id], {
    enabled: connected && capabilities.groups.list,
  });

  const filtered = useMemo(() => {
    const list = groups.data ?? [];
    if (!debounced.trim()) return list;
    const needle = debounced.toLowerCase();
    return list.filter((group) => group.id.toLowerCase().includes(needle));
  }, [groups.data, debounced]);

  const totalLag = useMemo(
    () => filtered.reduce((sum, group) => sum + (group.totalLag ?? 0), 0),
    [filtered],
  );

  if (!profile) {
    return (
      <div className="page">
        <EmptyState icon="alert" title="Connection not found" />
      </div>
    );
  }

  if (!connected) {
    return (
      <div className="page">
        <PageHeader title="Consumer groups" subtitle={profile.name} />
        <Panel>
          <EmptyState
            icon="plug"
            title="Connect to list consumer groups"
            actions={
              <Button variant="primary" icon="zap" onClick={() => void connect(id)}>
                Connect
              </Button>
            }
          />
        </Panel>
      </div>
    );
  }

  if (!capabilities.groups.list) {
    return (
      <div className="page">
        <PageHeader title="Consumer groups" subtitle={profile.name} />
        <Panel>
          <EmptyState
            icon="info"
            title="This driver does not expose consumer groups"
            text="Queues and topics without a consumer-group concept have nothing to show here."
          />
        </Panel>
      </div>
    );
  }

  return (
    <div className="page page--flush">
      <Toolbar>
        <SearchInput value={query} onChange={setQuery} placeholder="Filter groups…" width={260} />
        <span className="tiny dim nowrap">
          {filtered.length} of {groups.data?.length ?? 0}
        </span>
        {totalLag > 0 ? (
          <span className="tiny dim nowrap">total lag {totalLag.toLocaleString('en-US')}</span>
        ) : null}
        <ToolbarSpacer />
        {groups.error ? <span className="small text-danger ellipsis">{groups.error}</span> : null}
        <Button size="sm" icon="refresh" loading={groups.loading} onClick={() => void groups.reload()}>
          Refresh
        </Button>
      </Toolbar>

      <ConsumerGroupTable
        groups={filtered}
        onOpen={(group) => navigate(`/c/${id}/groups/${encodeURIComponent(group.id)}`)}
        onDelete={setDeleteTarget}
        canDelete={capabilities.groups.delete}
      />

      <ConfirmDialog
        open={deleteTarget !== null}
        title="Delete consumer group"
        message={
          <>
            <strong>{deleteTarget?.id}</strong> will be removed. Offsets are lost; an active
            consumer would recreate the group on its next commit.
          </>
        }
        confirmLabel="Delete group"
        busy={working}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={async () => {
          if (!deleteTarget) return;
          setWorking(true);
          try {
            await groupsApi.remove(id, deleteTarget.id);
            notifySuccess('Group deleted', deleteTarget.id);
            setDeleteTarget(null);
            await groups.reload();
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
