import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type { ConnectionsEvent, JobEvent, MessageBatchEvent } from '@/types';

export const EVENT_MESSAGES = 'mq://messages';
export const EVENT_JOB = 'mq://job';
export const EVENT_CONNECTIONS = 'mq://connections';

/** Subscribe to live-tail batches. Returns the unlisten function. */
export function onMessageBatch(handler: (event: MessageBatchEvent) => void): Promise<UnlistenFn> {
  return listen<MessageBatchEvent>(EVENT_MESSAGES, (event) => handler(event.payload));
}

export function onJobEvent(handler: (event: JobEvent) => void): Promise<UnlistenFn> {
  return listen<JobEvent>(EVENT_JOB, (event) => handler(event.payload));
}

export function onConnectionsEvent(
  handler: (event: ConnectionsEvent) => void,
): Promise<UnlistenFn> {
  return listen<ConnectionsEvent>(EVENT_CONNECTIONS, (event) => handler(event.payload));
}
