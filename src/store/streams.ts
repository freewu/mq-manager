import { create } from 'zustand';

import { errorMessage } from '@/api/client';
import { onJobEvent, onMessageBatch } from '@/api/events';
import { messagesApi } from '@/api/messages';
import type { JobState, Message, StreamRequest } from '@/types';

/** How many tailed messages are kept in memory per topic. */
const BUFFER_LIMIT = 2000;

export interface StreamSlot {
  key: string;
  jobId: string | null;
  connectionId: string;
  topic: string;
  state: JobState | 'idle';
  messages: Message[];
  received: number;
  paused: boolean;
  error?: string;
  startedAt: number;
}

interface StreamState {
  slots: Record<string, StreamSlot>;
  start: (connectionId: string, topic: string, request: StreamRequest) => Promise<void>;
  stop: (key: string) => Promise<void>;
  clear: (key: string) => void;
  togglePause: (key: string) => void;
  slot: (key: string) => StreamSlot | undefined;
}

export const streamKey = (connectionId: string, topic: string) => `${connectionId}::${topic}`;

let listening = false;

function ensureListeners() {
  if (listening) return;
  listening = true;

  void onMessageBatch(({ jobId, messages }) => {
    useStreams.setState((state) => {
      const entry = Object.entries(state.slots).find(([, slot]) => slot.jobId === jobId);
      if (!entry) return state;
      const [key, slot] = entry;
      if (slot.paused) return state;
      const merged = [...slot.messages, ...messages];
      return {
        slots: {
          ...state.slots,
          [key]: {
            ...slot,
            messages:
              merged.length > BUFFER_LIMIT ? merged.slice(merged.length - BUFFER_LIMIT) : merged,
          },
        },
      };
    });
  }).catch(() => undefined);

  void onJobEvent(({ jobId, state, error, idle, received }) => {
    useStreams.setState((current) => {
      const entry = Object.entries(current.slots).find(([, slot]) => slot.jobId === jobId);
      if (!entry) return current;
      const [key, slot] = entry;
      return {
        slots: {
          ...current.slots,
          [key]: {
            ...slot,
            state: idle ? 'idle' : state,
            error: error ?? undefined,
            received,
            jobId: state === 'stopped' || state === 'failed' ? null : slot.jobId,
          },
        },
      };
    });
  }).catch(() => undefined);
}

export const useStreams = create<StreamState>((set, get) => ({
  slots: {},

  async start(connectionId, topic, request) {
    ensureListeners();
    const key = streamKey(connectionId, topic);
    const existing = get().slots[key];
    if (existing?.jobId) return;

    const startedAt = Date.now();
    try {
      const jobId = await messagesApi.startStream(connectionId, request);
      set((state) => ({
        slots: {
          ...state.slots,
          [key]: {
            key,
            jobId,
            connectionId,
            topic,
            state: 'running',
            messages: existing?.messages ?? [],
            received: 0,
            paused: false,
            startedAt,
          },
        },
      }));
    } catch (error) {
      set((state) => ({
        slots: {
          ...state.slots,
          [key]: {
            key,
            jobId: null,
            connectionId,
            topic,
            state: 'failed',
            messages: existing?.messages ?? [],
            received: 0,
            paused: false,
            error: errorMessage(error),
            startedAt,
          },
        },
      }));
    }
  },

  async stop(key) {
    const slot = get().slots[key];
    if (!slot?.jobId) {
      set((state) => ({ slots: { ...state.slots, [key]: { ...slot, state: 'stopped' } } }));
      return;
    }
    try {
      await messagesApi.stopStream(slot.jobId);
    } finally {
      set((state) => ({
        slots: {
          ...state.slots,
          [key]: { ...slot, jobId: null, state: 'stopped' },
        },
      }));
    }
  },

  clear(key) {
    set((state) => {
      const slot = state.slots[key];
      if (!slot) return state;
      return { slots: { ...state.slots, [key]: { ...slot, messages: [], received: 0 } } };
    });
  },

  togglePause(key) {
    set((state) => {
      const slot = state.slots[key];
      if (!slot) return state;
      return { slots: { ...state.slots, [key]: { ...slot, paused: !slot.paused } } };
    });
  },

  slot: (key) => get().slots[key],
}));
