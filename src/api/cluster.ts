import { call } from './client';
import type { ClusterInfo, NodeInfo } from '@/types';

export const clusterApi = {
  info: (connectionId: string) => call<ClusterInfo>('cluster_info', { connectionId }),
  nodes: (connectionId: string) => call<NodeInfo[]>('list_nodes', { connectionId }),
  ping: (connectionId: string) => call<number>('ping', { connectionId }),
};
