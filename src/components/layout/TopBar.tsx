import { useLocation } from 'react-router-dom';

import { Badge, Button, ProviderChip, StatusDot } from '@/components/ui/primitives';
import { useConnection } from '@/hooks/useConnection';
import { useStreams } from '@/store/streams';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';

function titleFor(pathname: string): string {
  if (pathname.startsWith('/connections')) return 'Connections';
  if (pathname.startsWith('/settings')) return 'Settings';
  if (pathname.startsWith('/about')) return 'About';
  const match = /^\/c\/([^/]+)(?:\/([^/]+))?/.exec(pathname);
  if (!match) return 'Overview';
  switch (match[2]) {
    case undefined:
      return 'Overview';
    case 'topics':
      return 'Topics';
    case 'groups':
      return 'Consumer groups';
    case 'nodes':
      return 'Nodes';
    default:
      return 'Overview';
  }
}

export function TopBar({ onToggleSidebar }: { onToggleSidebar: () => void }) {
  const { pathname } = useLocation();
  const match = /^\/c\/([^/]+)/.exec(pathname);
  const connectionId = match?.[1];

  const { profile, provider, status, state, connected } = useConnection(connectionId);
  const busy = useWorkspace((value) => (connectionId ? Boolean(value.busy[connectionId]) : false));
  const connect = useWorkspace((value) => value.connect);
  const disconnect = useWorkspace((value) => value.disconnect);
  const refresh = useWorkspace((value) => value.refresh);
  const slots = useStreams((value) => value.slots);
  const providerCount = useWorkspace((value) => value.providers.length);

  const activeStreams = Object.values(slots).filter((slot) => slot.jobId).length;

  return (
    <header className="topbar">
      <Button
        variant="ghost"
        size="sm"
        icon="panel"
        onClick={onToggleSidebar}
        title="Toggle sidebar"
        aria-label="Toggle sidebar"
      />

      <div className="topbar__title">
        <span className="bold">{titleFor(pathname)}</span>
        {profile ? (
          <>
            <span className="dim">·</span>
            <span className="muted ellipsis" style={{ maxWidth: 260 }}>
              {profile.name}
            </span>
          </>
        ) : null}
      </div>

      <span className="grow" />

      {activeStreams > 0 ? (
        <span className="live-pill" title="Live tail jobs running">
          <span className="dot dot--connected" />
          {activeStreams} live
        </span>
      ) : null}

      {connectionId && profile ? (
        <>
          {provider ? (
            <ProviderChip name={provider.name} accent={provider.accent} compact />
          ) : null}

          <span className="row gap-6 small muted nowrap">
            <StatusDot state={state} />
            {state}
          </span>

          {connected && status?.latencyMs != null ? (
            <span className="tiny dim nowrap mono">{status.latencyMs} ms</span>
          ) : null}

          <span className="toolbar__divider" />

          {connected ? (
            <Button
              size="sm"
              icon="plug"
              loading={busy}
              onClick={async () => {
                try {
                  await disconnect(connectionId);
                  notifySuccess('Disconnected', profile.name);
                } catch (error) {
                  notifyError(error, 'Could not disconnect');
                }
              }}
            >
              Disconnect
            </Button>
          ) : (
            <Button
              size="sm"
              variant="primary"
              icon="zap"
              loading={busy}
              onClick={async () => {
                const ok = await connect(connectionId);
                if (ok) notifySuccess('Connected', profile.name);
                else notifyError('Check the broker address and credentials', 'Connection failed');
              }}
            >
              Connect
            </Button>
          )}

          <Button
            size="sm"
            variant="ghost"
            icon="refresh"
            title="Refresh status"
            aria-label="Refresh status"
            onClick={() => void refresh()}
          />
        </>
      ) : (
        <Badge tone="neutral" icon="sliders">
          {providerCount} drivers available
        </Badge>
      )}
    </header>
  );
}
