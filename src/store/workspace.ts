import { create } from 'zustand';

import { errorMessage } from '@/api/client';
import { connectionsApi } from '@/api/connections';
import { onConnectionsEvent } from '@/api/events';
import type {
  Capabilities,
  ConnectionProfile,
  ConnectionState,
  ConnectionStatus,
  ProviderDescriptor,
} from '@/types';

interface WorkspaceState {
  ready: boolean;
  providers: ProviderDescriptor[];
  profiles: ConnectionProfile[];
  statuses: Record<string, ConnectionStatus>;
  capabilities: Record<string, Capabilities>;
  busy: Record<string, boolean>;
  errors: Record<string, string>;

  init: () => Promise<void>;
  refresh: () => Promise<void>;
  provider: (providerId: string) => ProviderDescriptor | undefined;
  profile: (profileId: string) => ConnectionProfile | undefined;
  status: (profileId: string) => ConnectionStatus | undefined;
  stateOf: (profileId: string) => ConnectionState;
  capabilitiesOf: (profileId: string) => Capabilities | undefined;
  isConnected: (profileId: string) => boolean;
  save: (profile: ConnectionProfile) => Promise<ConnectionProfile>;
  remove: (profileId: string) => Promise<void>;
  connect: (profileId: string) => Promise<boolean>;
  disconnect: (profileId: string) => Promise<void>;
}

let listening = false;

export const useWorkspace = create<WorkspaceState>((set, get) => {
  const patchStatus = (profileId: string, patch: Partial<ConnectionStatus>) => {
    set((state) => {
      const current: ConnectionStatus = state.statuses[profileId] ?? {
        profileId,
        providerId: '',
        state: 'disconnected',
      };
      return { statuses: { ...state.statuses, [profileId]: { ...current, ...patch } } };
    });
  };

  return {
    ready: false,
    providers: [],
    profiles: [],
    statuses: {},
    capabilities: {},
    busy: {},
    errors: {},

    async init() {
      if (!listening) {
        listening = true;
        // The backend announces connects / disconnects — keep the sidebar honest.
        void onConnectionsEvent(() => void get().refresh()).catch(() => undefined);
      }

      const [providers, profiles] = await Promise.all([
        connectionsApi.providers(),
        connectionsApi.list(),
      ]);
      set({ providers, profiles, ready: true });
      await get().refresh();
    },

    async refresh() {
      const statuses = await connectionsApi.statuses();
      const next: Record<string, ConnectionStatus> = {};
      for (const status of statuses) next[status.profileId] = status;

      const capabilities: Record<string, Capabilities> = {};
      await Promise.all(
        statuses.map(async (status) => {
          try {
            capabilities[status.profileId] = await connectionsApi.capabilities(status.profileId);
          } catch {
            // A connection that vanished mid-refresh is not an error.
          }
        }),
      );

      set({ statuses: next, capabilities });
    },

    provider: (providerId) => get().providers.find((entry) => entry.id === providerId),
    profile: (profileId) => get().profiles.find((entry) => entry.id === profileId),
    status: (profileId) => get().statuses[profileId],
    stateOf: (profileId) => get().statuses[profileId]?.state ?? 'disconnected',
    capabilitiesOf: (profileId) => get().capabilities[profileId],
    isConnected: (profileId) => get().statuses[profileId]?.state === 'connected',

    async save(profile) {
      const saved = await connectionsApi.save(profile);
      set((state) => ({
        profiles: state.profiles.some((entry) => entry.id === saved.id)
          ? state.profiles.map((entry) => (entry.id === saved.id ? saved : entry))
          : [...state.profiles, saved],
      }));
      return saved;
    },

    async remove(profileId) {
      await connectionsApi.remove(profileId);
      set((state) => {
        const statuses = { ...state.statuses };
        delete statuses[profileId];
        const capabilities = { ...state.capabilities };
        delete capabilities[profileId];
        return {
          profiles: state.profiles.filter((entry) => entry.id !== profileId),
          statuses,
          capabilities,
        };
      });
    },

    async connect(profileId) {
      set((state) => ({
        busy: { ...state.busy, [profileId]: true },
        errors: { ...state.errors, [profileId]: '' },
      }));
      patchStatus(profileId, { state: 'connecting', error: null });

      try {
        const result = await connectionsApi.connect(profileId);
        set((state) => ({
          busy: { ...state.busy, [profileId]: false },
          statuses: { ...state.statuses, [profileId]: result.status },
          capabilities: { ...state.capabilities, [profileId]: result.capabilities },
        }));
        return true;
      } catch (error) {
        const message = errorMessage(error);
        set((state) => ({ busy: { ...state.busy, [profileId]: false }, errors: { ...state.errors, [profileId]: message } }));
        patchStatus(profileId, { state: 'error', error: message });
        return false;
      }
    },

    async disconnect(profileId) {
      try {
        await connectionsApi.disconnect(profileId);
      } finally {
        set((state) => {
          const statuses = { ...state.statuses };
          delete statuses[profileId];
          return { statuses };
        });
      }
    },
  };
});
