import { Badge, Button, ProviderChip, StatusDot } from '@/components/ui/primitives';
import { Icon } from '@/components/ui/Icon';
import { formatRelative } from '@/lib/format';
import { primaryTarget } from '@/lib/profile';
import type { ConnectionProfile, ConnectionStatus, ProviderDescriptor } from '@/types';

export function ConnectionCard({
  profile,
  provider,
  status,
  busy,
  onOpen,
  onConnect,
  onDisconnect,
  onEdit,
  onDelete,
}: {
  profile: ConnectionProfile;
  provider: ProviderDescriptor | undefined;
  status: ConnectionStatus | undefined;
  busy: boolean;
  onOpen: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const state = status?.state ?? 'disconnected';
  const accent = profile.color ?? provider?.accent ?? 'var(--accent)';

  return (
    <article className="card card--interactive" onClick={onOpen}>
      <div className="row-between gap-8" style={{ alignItems: 'flex-start' }}>
        <div className="row gap-8" style={{ minWidth: 0 }}>
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: 3,
              background: accent,
              marginTop: 6,
              flex: '0 0 auto',
            }}
          />
          <div style={{ minWidth: 0 }}>
            <div className="bold ellipsis" style={{ fontSize: 13.5 }}>
              {profile.name}
            </div>
            <div className="tiny dim mono ellipsis">{primaryTarget(profile, provider)}</div>
          </div>
        </div>
        <span className="row gap-6 small muted nowrap">
          <StatusDot state={state} />
          {state}
        </span>
      </div>

      {profile.description ? (
        <p className="small muted" style={{ marginTop: 10, lineHeight: 1.5 }}>
          {profile.description}
        </p>
      ) : null}

      <div className="row gap-6 wrap" style={{ marginTop: 10 }}>
        {provider ? <ProviderChip name={provider.name} accent={provider.accent} /> : null}
        {profile.tags.map((tag) => (
          <Badge key={tag} outline>
            {tag}
          </Badge>
        ))}
      </div>

      <div className="row-between gap-8" style={{ marginTop: 12 }}>
        <span className="tiny dim">
          {status?.cluster
            ? `${status.cluster.nodeCount} nodes · ${status.cluster.topicCount} topics`
            : `last used ${formatRelative(profile.lastConnectedAt)}`}
        </span>
        <span className="row gap-4" onClick={(event) => event.stopPropagation()}>
          {state === 'connected' ? (
            <Button size="sm" icon="plug" loading={busy} onClick={onDisconnect}>
              Disconnect
            </Button>
          ) : (
            <Button size="sm" variant="primary" icon="zap" loading={busy} onClick={onConnect}>
              Connect
            </Button>
          )}
          <Button size="sm" variant="ghost" icon="edit" onClick={onEdit} aria-label="Edit" title="Edit" />
          <Button
            size="sm"
            variant="ghost"
            icon="trash"
            onClick={onDelete}
            aria-label="Delete"
            title="Delete"
          />
        </span>
      </div>

      {state === 'error' && status?.error ? (
        <div className="row gap-6 small text-danger" style={{ marginTop: 8 }}>
          <Icon name="alert" size={13} />
          <span className="ellipsis" title={status.error}>
            {status.error}
          </span>
        </div>
      ) : null}
    </article>
  );
}
