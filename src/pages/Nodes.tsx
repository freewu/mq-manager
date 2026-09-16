import { useNavigate, useParams } from 'react-router-dom';

import { PageHeader } from '@/components/layout/PageHeader';
import { DataTable, type Column } from '@/components/ui/Table';
import { Badge, Button, EmptyState, Panel, Toolbar, ToolbarSpacer } from '@/components/ui/primitives';
import { clusterApi } from '@/api/cluster';
import { useAsync } from '@/hooks/useAsync';
import { useCapabilities, useConnection } from '@/hooks/useConnection';
import { useWorkspace } from '@/store/workspace';
import type { NodeInfo } from '@/types';

export function Nodes() {
  const { id = '' } = useParams();
  const navigate = useNavigate();
  const { profile } = useConnection(id);
  const capabilities = useCapabilities(id);
  const connected = useWorkspace((state) => state.statuses[id]?.state === 'connected');
  const connect = useWorkspace((value) => value.connect);

  const nodes = useAsync(() => clusterApi.nodes(id), [id], {
    enabled: connected && capabilities.nodes,
  });

  if (!profile) {
    return (
      <div className="page">
        <EmptyState icon="alert" title="Connection not found" />
      </div>
    );
  }

  if (!connected || !capabilities.nodes) {
    return (
      <div className="page">
        <PageHeader title="Nodes" subtitle={profile.name} />
        <Panel>
          <EmptyState
            icon="plug"
            title={connected ? 'This driver does not expose nodes' : 'Connect to list nodes'}
            actions={
              connected ? (
                <Button onClick={() => navigate(`/c/${id}`)}>Overview</Button>
              ) : (
                <Button variant="primary" icon="zap" onClick={() => void connect(id)}>
                  Connect
                </Button>
              )
            }
          />
        </Panel>
      </div>
    );
  }

  const columns: Array<Column<NodeInfo>> = [
    {
      key: 'id',
      header: 'Node',
      width: 90,
      align: 'right',
      render: (node) => <span className="mono-cell">{node.id}</span>,
      sortValue: (node) => node.id,
    },
    {
      key: 'host',
      header: 'Host',
      render: (node) => <span className="mono-cell">{node.host}</span>,
      sortValue: (node) => node.host,
    },
    {
      key: 'port',
      header: 'Port',
      width: 100,
      align: 'right',
      render: (node) => <span className="mono-cell dim">{node.port}</span>,
      sortValue: (node) => node.port,
    },
    {
      key: 'rack',
      header: 'Rack',
      width: 160,
      render: (node) =>
        node.rack ? <span className="mono-cell dim">{node.rack}</span> : <span className="dim">—</span>,
    },
    {
      key: 'role',
      header: 'Role',
      width: 160,
      render: (node) => (
        <span className="row gap-6">
          {node.isController ? <Badge tone="accent">controller</Badge> : null}
          {node.role ? <Badge tone="neutral">{node.role}</Badge> : null}
          {!node.isController && !node.role ? <span className="dim">broker</span> : null}
        </span>
      ),
    },
  ];

  const controller = (nodes.data ?? []).find((node) => node.isController);

  return (
    <div className="page page--flush">
      <Toolbar>
        <span className="small muted">
          {nodes.data?.length ?? 0} node(s) reported
          {controller ? ` · controller #${controller.id}` : ''}
        </span>
        <ToolbarSpacer />
        {nodes.error ? <span className="small text-danger ellipsis">{nodes.error}</span> : null}
        <Button size="sm" icon="refresh" loading={nodes.loading} onClick={() => void nodes.reload()}>
          Refresh
        </Button>
      </Toolbar>

      <DataTable
        columns={columns}
        rows={nodes.data ?? []}
        rowKey={(node) => String(node.id)}
        loading={nodes.loading}
        initialSort={{ key: 'id', direction: 'asc' }}
        emptyTitle="No nodes"
      />
    </div>
  );
}
