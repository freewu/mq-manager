import { call } from './client';
import type { AppInfo, AppSettings } from '@/types';

export const appApi = {
  info: () => call<AppInfo>('app_info'),
  getSettings: () => call<AppSettings>('get_settings'),
  saveSettings: (settings: AppSettings) => call<AppSettings>('save_settings', { settings }),
};
