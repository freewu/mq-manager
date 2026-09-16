import { call } from './client';
import type {
  Capabilities,
  ConnectResult,
  ConnectionProfile,
  ConnectionStatus,
  ProviderDescriptor,
} from '@/types';

export const connectionsApi = {
  providers: () => call<ProviderDescriptor[]>('list_providers'),

  list: () => call<ConnectionProfile[]>('list_connections'),

  statuses: () => call<ConnectionStatus[]>('connection_statuses'),

  capabilities: (id: string) => call<Capabilities>('connection_capabilities', { id }),

  save: (profile: ConnectionProfile) => call<ConnectionProfile>('save_connection', { profile }),

  test: (profile: ConnectionProfile) => call<ConnectResult>('test_connection', { profile }),

  connect: (id: string) => call<ConnectResult>('connect', { id }),

  disconnect: (id: string) => call<void>('disconnect', { id }),

  /** Closes the connection (if any) and drops the stored profile. */
  remove: (id: string) => call<void>('remove_connection', { id }),
};
