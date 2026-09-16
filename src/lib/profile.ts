import type { ConnectionProfile, JsonMap, ProviderDescriptor } from '@/types';

/**
 * Picks the field a provider considers its primary address so cards can show a
 * human readable target without the UI knowing any broker specifics.
 */
export function primaryTarget(
  profile: ConnectionProfile | undefined,
  provider: ProviderDescriptor | undefined,
): string {
  if (!profile) return '';
  const field = provider?.fields.find(
    (entry) => entry.kind === 'text' && entry.required && !entry.secret,
  );
  const options: JsonMap = profile.options ?? {};
  const value = field ? options[field.key] : undefined;
  if (typeof value === 'string' && value.trim()) return value.trim();
  if (field && (typeof value === 'number' || typeof value === 'boolean')) return String(value);
  return provider ? provider.name : profile.provider;
}
