import { useMemo, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';

import { PageHeader } from '@/components/layout/PageHeader';
import { TopicForm } from '@/components/domain/TopicForm';
import { TopicTable } from '@/components/domain/Tables';
import { ConfirmDialog, Modal } from '@/components/ui/Modal';
import { Checkbox } from '@/components/ui/Field';
import {
  Button,
  EmptyState,
  Panel,
  SearchInput,
  Toolbar,
  ToolbarSpacer,
} from '@/components/ui/primitives';
import { topicsApi } from '@/api/topics';
import { useAsync } from '@/hooks/useAsync';
import { useCapabilities, useConnection } from '@/hooks/useConnection';
import { useDebounced } from '@/hooks/useDebounced';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';
import type { TopicSummary } from '@/types';

export function Topics() {
  const { id = '' } = useParams();
  const navigate = useNavigate();

  const { profile } = useConnection(id);
  const capabilities = useCapabilities(id);
  const connected = useWorkspace((state) => state.statuses[id]?.state === 'connected');
  const connect = useWorkspace((value) => value.connect);

  const [query, setQuery] = useState('');
  const [includeInternal, setIncludeInternal] = useState(false);
  const [creating, setCreating] = useState(false);
  const [purgeTarget, setPurgeTarget] = useState<TopicSummary | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<TopicSummary | null>(null);
  const [working, setWorking] = useState(false);

  const debouncedQuery = useDebounced(query, 200);

  const topics = useAsync(
    () => topicsApi.list(id, includeInternal),
    [id, includeInternal],
    { enabled: connected },
  );

  const filtered = useMemo(() => {
    const list = topics.data ?? [];
    if (!debouncedQuery.trim()) return list;
    const needle = debouncedQuery.toLowerCase();
    return list.filter((topic) => topic.name.toLowerCase().includes(needle));
  }, [topics.data, debouncedQuery]);

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
        <PageHeader title="Topics" subtitle={profile.name} />
        <Panel>
          <EmptyState
            icon="plug"
            title="Connect to list topics"
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

  if (!capabilities.topics.list) {
    return (
      <div className="page">
        <PageHeader title="Topics" subtitle={profile.name} />
        <Panel>
          <EmptyState
            icon="info"
            title="Topics are not supported by this driver"
            text="The capability report returned by the driver does not expose a topic list."
          />
        </Panel>
      </div>
    );
  }

  return (
    <div className="page page--flush">
      <Toolbar>
        <SearchInput value={query} onChange={setQuery} placeholder="Filter topics…" width={260} />
        <Checkbox checked={includeInternal} onChange={setIncludeInternal} label="Include internal" />
        <span className="tiny dim nowrap">
          {filtered.length} of {topics.data?.length ?? 0}
        </span>
        <ToolbarSpacer />
        {topics.error ? <span className="small text-danger ellipsis">{topics.error}</span> : null}
        <Button size="sm" icon="refresh" onClick={() => void topics.reload()} loading={topics.loading}>
          Refresh
        </Button>
        {capabilities.topics.create ? (
          <Button size="sm" variant="primary" icon="plus" onClick={() => setCreating(true)}>
            New topic
          </Button>
        ) : null}
      </Toolbar>

      <TopicTable
        topics={filtered}
        onOpen={(topic) => navigate(`/c/${id}/topics/${encodeURIComponent(topic.name)}`)}
        onPurge={setPurgeTarget}
        onDelete={setDeleteTarget}
        canPurge={capabilities.topics.purge}
        canDelete={capabilities.topics.delete}
        canPartition={capabilities.topics.partitions}
      />

      <Modal
        open={creating}
        onClose={() => setCreating(false)}
        title="New topic"
        subtitle={profile.name}
        size="lg"
      >
        <TopicForm
          mode="create"
          connectionId={id}
          onCancel={() => setCreating(false)}
          onDone={async (topic) => {
            setCreating(false);
            await topics.reload();
            navigate(`/c/${id}/topics/${encodeURIComponent(topic)}`);
          }}
        />
      </Modal>

      <ConfirmDialog
        open={purgeTarget !== null}
        title="Purge topic"
        message={
          <>
            Every record in <strong>{purgeTarget?.name}</strong> will be deleted. Offsets are kept,
            so consumers will jump to the end.
          </>
        }
        confirmLabel="Purge"
        busy={working}
        onCancel={() => setPurgeTarget(null)}
        onConfirm={async () => {
          if (!purgeTarget) return;
          setWorking(true);
          try {
            await topicsApi.purge(id, purgeTarget.name);
            notifySuccess('Topic purged', purgeTarget.name);
            setPurgeTarget(null);
            await topics.reload();
          } catch (error) {
            notifyError(error, 'Purge failed');
          } finally {
            setWorking(false);
          }
        }}
      />

      <ConfirmDialog
        open={deleteTarget !== null}
        title="Delete topic"
        message={
          <>
            <strong>{deleteTarget?.name}</strong> and all of its data will be removed from the
            broker. This cannot be undone.
          </>
        }
        confirmLabel="Delete"
        busy={working}
        onCancel={() => setDeleteTarget(null)}
        onConfirm={async () => {
          if (!deleteTarget) return;
          setWorking(true);
          try {
            await topicsApi.remove(id, deleteTarget.name);
            notifySuccess('Topic deleted', deleteTarget.name);
            setDeleteTarget(null);
            await topics.reload();
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
