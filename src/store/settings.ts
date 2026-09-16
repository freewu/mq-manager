import { create } from 'zustand';

import { appApi } from '@/api/app';
import type { AppSettings } from '@/types';

interface SettingsState {
  settings: AppSettings;
  loaded: boolean;
  load: () => Promise<AppSettings>;
  update: (patch: Partial<AppSettings>) => Promise<void>;
}

export const useSettings = create<SettingsState>((set, get) => ({
  settings: {
    theme: 'dark',
    pageSize: 50,
    defaultMessageLimit: 200,
    prettyJson: true,
    confirmDestructive: true,
    autoConnect: false,
    timestampFormat: 'datetime',
  },
  loaded: false,

  async load() {
    try {
      const settings = await appApi.getSettings();
      set({ settings, loaded: true });
      return settings;
    } catch {
      set({ loaded: true });
      return get().settings;
    }
  },

  async update(patch) {
    const next = { ...get().settings, ...patch };
    set({ settings: next });
    const saved = await appApi.saveSettings(next);
    set({ settings: saved });
  },
}));
