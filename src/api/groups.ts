import { call } from './client';
import type { ConsumerGroupDetail, ConsumerGroupSummary, ResetOffsetsRequest } from '@/types';

export const groupsApi = {
  list: (connectionId: string) =>
    call<ConsumerGroupSummary[]>('list_groups', { connectionId }),

  detail: (connectionId: string, group: string) =>
    call<ConsumerGroupDetail>('get_group', { connectionId, group }),

  remove: (connectionId: string, group: string) =>
    call<void>('delete_group', { connectionId, group }),

  resetOffsets: (connectionId: string, request: ResetOffsetsRequest) =>
    call<void>('reset_group_offsets', { connectionId, request }),
};
