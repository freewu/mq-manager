import { useWorkspace } from '@/store/workspace';
import type {
  Capabilities,
  ConnectionProfile,
  ConnectionState,
  ConnectionStatus,
  ProviderDescriptor,
} from '@/types';

/** Everything a page needs to know about the connection in the URL. */
export interface ConnectionContext {
  id: string;
  profile: ConnectionProfile | undefined;
  provider: ProviderDescriptor | undefined;
  status: ConnectionStatus | undefined;
  state: ConnectionState;
  connected: boolean;
  capabilities: Capabilities | undefined;
}

export function useConnection(id: string | undefined): ConnectionContext {
  const profile = useWorkspace((state) =>
    id ? state.profiles.find((entry) => entry.id === id) : undefined,
  );
  const provider = useWorkspace((state) =>
    profile ? state.providers.find((entry) => entry.id === profile.provider) : undefined,
  );
  const status = useWorkspace((state) => (id ? state.statuses[id] : undefined));
  const capabilities = useWorkspace((state) => (id ? state.capabilities[id] : undefined));

  const state: ConnectionState = status?.state ?? 'disconnected';

  return {
    id: id ?? '',
    profile,
    provider,
    status,
    state,
    connected: state === 'connected',
    capabilities,
  };
}

/** Sets every capability to `false` so pages can read flags without guards. */
export const NO_CAPABILITIES: Capabilities = {
  topics: {
    list: false,
    create: false,
    delete: false,
    update: false,
    config: false,
    partitions: false,
    purge: false,
  },
  messages: {
    produce: false,
    browse: false,
    tail: false,
    keys: false,
    headers: false,
    partitions: false,
    timestampSeek: false,
    offsetSeek: false,
    persistent: false,
  },
  groups: {
    list: false,
    describe: false,
    members: false,
    offsets: false,
    lag: false,
    resetOffsets: false,
    delete: false,
  },
  nodes: false,
  metrics: false,
  acl: false,
  schemas: false,
};

export function useCapabilities(id: string | undefined): Capabilities {
  return useWorkspace((state) => (id ? state.capabilities[id] : undefined)) ?? NO_CAPABILITIES;
}

/** Display name of a connection for menus and breadcrumbs. */
export function profileLabel(profile: ConnectionProfile | undefined): string {
  return profile?.name ?? 'Unknown connection';
}
