import { Badge, ProviderChip } from '@/components/ui/primitives';
import { Icon } from '@/components/ui/Icon';
import type { Capabilities } from '@/types';

/** Compact capability report used on the About page and connection drawers. */
export function CapabilityList({ capabilities }: { capabilities: Capabilities }) {
  const entries: Array<{ label: string; on: boolean }> = [
    { label: 'list topics', on: capabilities.topics.list },
    { label: 'create topic', on: capabilities.topics.create },
    { label: 'edit topic', on: capabilities.topics.update },
    { label: 'delete topic', on: capabilities.topics.delete },
    { label: 'topic config', on: capabilities.topics.config },
    { label: 'partitions', on: capabilities.topics.partitions },
    { label: 'purge', on: capabilities.topics.purge },
    { label: 'produce', on: capabilities.messages.produce },
    { label: 'browse', on: capabilities.messages.browse },
    { label: 'live tail', on: capabilities.messages.tail },
    { label: 'keys', on: capabilities.messages.keys },
    { label: 'headers', on: capabilities.messages.headers },
    { label: 'timestamp seek', on: capabilities.messages.timestampSeek },
    { label: 'offset seek', on: capabilities.messages.offsetSeek },
    { label: 'durable log', on: capabilities.messages.persistent },
    { label: 'list groups', on: capabilities.groups.list },
    { label: 'group detail', on: capabilities.groups.describe },
    { label: 'members', on: capabilities.groups.members },
    { label: 'offsets', on: capabilities.groups.offsets },
    { label: 'lag', on: capabilities.groups.lag },
    { label: 'reset offsets', on: capabilities.groups.resetOffsets },
    { label: 'delete group', on: capabilities.groups.delete },
    { label: 'nodes', on: capabilities.nodes },
    { label: 'metrics', on: capabilities.metrics },
    { label: 'ACL', on: capabilities.acl },
    { label: 'schemas', on: capabilities.schemas },
  ];

  return (
    <div className="row gap-6 wrap">
      {entries.map((entry) => (
        <Badge
          key={entry.label}
          tone={entry.on ? 'success' : 'neutral'}
          icon={entry.on ? 'check' : 'x'}
          title={entry.on ? 'Supported' : 'Not supported by this driver'}
        >
          {entry.label}
        </Badge>
      ))}
    </div>
  );
}

/** One-line driver summary, used in the connection picker. */
export function ProviderSummary({
  name,
  vendor,
  description,
  accent,
  port,
}: {
  name: string;
  vendor: string;
  description: string;
  accent: string;
  port: number | null;
}) {
  return (
    <div className="row gap-8" style={{ alignItems: 'flex-start' }}>
      <ProviderChip name={name} accent={accent} />
      <div style={{ minWidth: 0 }}>
        <div className="small">{description}</div>
        <div className="tiny dim row gap-6">
          <span>{vendor}</span>
          {port ? (
            <>
              <span>·</span>
              <span className="mono">default port {port}</span>
            </>
          ) : null}
        </div>
      </div>
    </div>
  );
}

export function CapabilityHint({ ok, children }: { ok: boolean; children: string }) {
  if (ok) return null;
  return (
    <div className="row gap-6 tiny dim">
      <Icon name="info" size={12} />
      {children}
    </div>
  );
}
