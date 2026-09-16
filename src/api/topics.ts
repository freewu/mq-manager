import { call } from './client';
import type {
  ConfigEntry,
  CreateTopicRequest,
  TopicDetail,
  TopicSummary,
  UpdateTopicRequest,
} from '@/types';

export const topicsApi = {
  list: (connectionId: string, includeInternal = false) =>
    call<TopicSummary[]>('list_topics', { connectionId, includeInternal }),

  detail: (connectionId: string, topic: string) =>
    call<TopicDetail>('get_topic', { connectionId, topic }),

  configs: (connectionId: string, topic: string) =>
    call<ConfigEntry[]>('get_topic_configs', { connectionId, topic }),

  create: (connectionId: string, request: CreateTopicRequest) =>
    call<void>('create_topic', { connectionId, request }),

  update: (connectionId: string, request: UpdateTopicRequest) =>
    call<void>('update_topic', { connectionId, request }),

  remove: (connectionId: string, topic: string) =>
    call<void>('delete_topic', { connectionId, topic }),

  purge: (connectionId: string, topic: string) =>
    call<void>('purge_topic', { connectionId, topic }),
};
