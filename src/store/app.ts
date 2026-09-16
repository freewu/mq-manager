import { create } from 'zustand';

import { appApi } from '@/api/app';
import type { AppInfo } from '@/types';

interface AppInfoState {
  info: AppInfo | null;
  load: () => Promise<void>;
}

export const useAppInfo = create<AppInfoState>((set) => ({
  info: null,
  async load() {
    try {
      set({ info: await appApi.info() });
    } catch {
      // The About / Settings pages simply render placeholders when unavailable.
    }
  },
}));
