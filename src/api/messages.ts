import { call } from './client';
import type { BrowseRequest, BrowseResult, JobInfo, ProduceRequest, ProduceResult, StreamRequest } from '@/types';

export const messagesApi = {
  produce: (connectionId: string, request: ProduceRequest) =>
    call<ProduceResult>('produce_message', { connectionId, request }),

  browse: (connectionId: string, request: BrowseRequest) =>
    call<BrowseResult>('browse_messages', { connectionId, request }),

  /** Returns the job id; batches arrive on the `mq://messages` event. */
  startStream: (connectionId: string, request: StreamRequest) =>
    call<string>('start_stream', { connectionId, request }),

  stopStream: (jobId: string) => call<JobInfo>('stop_stream', { jobId }),

  jobs: () => call<JobInfo[]>('list_jobs'),
};
