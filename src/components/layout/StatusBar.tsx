import { useLocation } from 'react-router-dom';

import { StatusDot } from '@/components/ui/primitives';
import { useConnection } from '@/hooks/useConnection';
import { formatNumber } from '@/lib/format';
import { useStreams } from '@/store/streams';

export function StatusBar({ version }: { version: string }) {
  const { pathname } = useLocation();
  const connectionId = /^\/c\/([^/]+)/.exec(pathname)?.[1];
  const { status, provider, state } = useConnection(connectionId);
  const slots = useStreams((value) => value.slots);

  const streams = Object.values(slots);
  const active = streams.filter((slot) => slot.jobId).length;
  const received = streams.reduce((total, slot) => total + slot.received, 0);

  return (
    <footer className="statusbar">
      <span className="statusbar__item">
        <StatusDot state={state} />
        {provider ? `${provider.name} · ${state}` : 'No connection selected'}
      </span>

      {status?.cluster ? (
        <span className="statusbar__item">
          {status.cluster.name}
          {status.cluster.version ? ` · ${status.cluster.version}` : ''}
        </span>
      ) : null}

      {status?.cluster ? (
        <span className="statusbar__item">
          {formatNumber(status.cluster.topicCount)} topics · {formatNumber(status.cluster.partitionCount)}{' '}
          partitions
        </span>
      ) : null}

      {status?.latencyMs != null ? (
        <span className="statusbar__item mono">{status.latencyMs} ms</span>
      ) : null}

      <span className="grow" />

      <span className="statusbar__item">
        {active > 0 ? `Tailing ${active} topic${active === 1 ? '' : 's'}` : 'No live tail'}
        {received > 0 ? ` · ${formatNumber(received)} received` : ''}
      </span>

      <span className="statusbar__item">v{version}</span>
    </footer>
  );
}
