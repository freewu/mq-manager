import { useMemo, useState } from 'react';

import { Button, Banner } from '@/components/ui/primitives';
import { Checkbox, Field, KeyValueEditor, Select, Toggle } from '@/components/ui/Field';
import { Icon } from '@/components/ui/Icon';
import { connectionsApi } from '@/api/connections';
import { objectToRows, rowsToObject } from '@/lib/json';
import { cn } from '@/lib/cn';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';
import type {
  ConnectionField,
  ConnectionProfile,
  JsonMap,
  ProviderDescriptor,
} from '@/types';

const SWATCHES = ['#98d98e', '#8b6cf0', '#3ecf8e', '#efb45c', '#f2606a', '#5cc2ea', '#e879b9'];

/** Builds the `options` object a provider expects from its field defaults. */
function defaultOptions(provider: ProviderDescriptor | undefined): JsonMap {
  const options: JsonMap = {};
  for (const field of provider?.fields ?? []) {
    if (field.default !== null && field.default !== undefined) {
      options[field.key] = field.default;
    } else if (field.kind === 'keyValue') {
      options[field.key] = {};
    }
  }
  return options;
}

function newProfile(providerId: string, provider: ProviderDescriptor | undefined): ConnectionProfile {
  const now = Date.now();
  return {
    id: crypto.randomUUID(),
    name: '',
    provider: providerId,
    color: provider?.accent ?? SWATCHES[0],
    description: '',
    tags: [],
    options: defaultOptions(provider),
    createdAt: now,
    updatedAt: now,
    lastConnectedAt: null,
  };
}

function initialDraft(
  profile: ConnectionProfile | null | undefined,
  initialProviderId: string | undefined,
  providers: ProviderDescriptor[],
): ConnectionProfile {
  if (profile) {
    const provider = providers.find((entry) => entry.id === profile.provider);
    return {
      ...profile,
      options: { ...defaultOptions(provider), ...profile.options },
      tags: profile.tags ?? [],
    };
  }
  const providerId = initialProviderId ?? providers[0]?.id ?? '';
  const provider = providers.find((entry) => entry.id === providerId);
  const draft = newProfile(providerId, provider);
  draft.name = provider ? `My ${provider.name}` : 'New connection';
  return draft;
}

function visible(field: ConnectionField, options: JsonMap): boolean {
  if (!field.condition) return true;
  const current = options[field.condition.key];
  const asString = current === null || current === undefined ? '' : String(current);
  return field.condition.equals.includes(asString);
}

export interface ConnectionFormProps {
  profile?: ConnectionProfile | null;
  initialProviderId?: string;
  onSaved: (profile: ConnectionProfile) => void;
  onCancel: () => void;
}

export function ConnectionForm({
  profile,
  initialProviderId,
  onSaved,
  onCancel,
}: ConnectionFormProps) {
  const providers = useWorkspace((state) => state.providers);
  const save = useWorkspace((state) => state.save);

  const [draft, setDraft] = useState<ConnectionProfile>(() =>
    initialDraft(profile, initialProviderId, providers),
  );
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [saving, setSaving] = useState(false);
  const [testing, setTesting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  const provider = providers.find((entry) => entry.id === draft.provider);

  const patch = (values: Partial<ConnectionProfile>) =>
    setDraft((current) => ({ ...current, ...values }));

  const patchOption = (key: string, value: unknown) =>
    setDraft((current) => ({ ...current, options: { ...current.options, [key]: value } }));

  const groups = useMemo(() => {
    const fields = (provider?.fields ?? []).filter((field) => visible(field, draft.options));
    const normal = fields.filter((field) => !field.advanced || showAdvanced);
    const order: string[] = [];
    for (const field of normal) {
      if (!order.includes(field.group)) order.push(field.group);
    }
    return order.map((group) => ({
      group,
      fields: normal.filter((field) => field.group === group),
    }));
  }, [provider, draft.options, showAdvanced]);

  const hasAdvanced = (provider?.fields ?? []).some((field) => field.advanced);

  const switchProvider = (providerId: string) => {
    const next = providers.find((entry) => entry.id === providerId);
    // Options are provider specific — swapping the driver resets them.
    patch({
      provider: providerId,
      options: defaultOptions(next),
      color: next?.accent ?? draft.color,
    });
  };

  const validate = (): string | null => {
    if (!draft.name.trim()) return 'Give the connection a name.';
    for (const field of provider?.fields ?? []) {
      if (!field.required || !visible(field, draft.options)) continue;
      const value = draft.options[field.key];
      if (value === undefined || value === null || String(value).trim() === '') {
        return `${field.label} is required.`;
      }
    }
    return null;
  };

  const submit = async () => {
    const problem = validate();
    if (problem) {
      setFormError(problem);
      return;
    }
    setFormError(null);
    setSaving(true);
    try {
      const saved = await save({
        ...draft,
        name: draft.name.trim(),
        updatedAt: Date.now(),
      });
      notifySuccess(profile ? 'Connection updated' : 'Connection created', saved.name);
      onSaved(saved);
    } catch (error) {
      notifyError(error, 'Could not save the connection');
    } finally {
      setSaving(false);
    }
  };

  const test = async () => {
    setTesting(true);
    setFormError(null);
    try {
      const result = await connectionsApi.test({ ...draft, name: draft.name.trim() || 'test' });
      const cluster = result.status?.cluster;
      notifySuccess('Connection works', cluster ? `${cluster.name} · ${cluster.nodeCount} nodes` : undefined);
    } catch (error) {
      notifyError(error, 'Connection test failed');
    } finally {
      setTesting(false);
    }
  };

  if (providers.length === 0) {
    return <Banner tone="warning">No drivers are registered in this build.</Banner>;
  }

  return (
    <div className="stack-16">
      {formError ? <Banner tone="danger">{formError}</Banner> : null}

      <div className="fieldset">
        <div className="fieldset__legend">Connection</div>
        <div className="form-grid" style={{ marginTop: 12 }}>
          <Field label="Name" required>
            <input
              className="input"
              value={draft.name}
              autoFocus
              placeholder="Local Kafka"
              onChange={(event) => patch({ name: event.target.value })}
            />
          </Field>

          <Field label="Driver" required help={provider?.description}>
            <Select
              value={draft.provider}
              onChange={switchProvider}
              options={providers.map((entry) => ({ value: entry.id, label: entry.name }))}
            />
          </Field>

          <Field label="Colour" help="Shown in the sidebar and cards.">
            <div className="row gap-8" style={{ height: 31 }}>
              {SWATCHES.map((swatch) => (
                <button
                  key={swatch}
                  type="button"
                  aria-label={swatch}
                  onClick={() => patch({ color: swatch })}
                  style={{
                    width: 20,
                    height: 20,
                    borderRadius: '50%',
                    background: swatch,
                    border: draft.color === swatch ? '2px solid #fff' : '2px solid transparent',
                    boxShadow: draft.color === swatch ? '0 0 0 1px var(--border-strong)' : 'none',
                  }}
                />
              ))}
            </div>
          </Field>

          <Field label="Tags" help="Comma separated, used for grouping.">
            <input
              className="input"
              value={draft.tags.join(', ')}
              placeholder="dev, staging"
              onChange={(event) =>
                patch({
                  tags: event.target.value
                    .split(',')
                    .map((tag) => tag.trim())
                    .filter(Boolean),
                })
              }
            />
          </Field>

          <Field label="Description" full>
            <input
              className="input"
              value={draft.description ?? ''}
              placeholder="Optional notes about this broker"
              onChange={(event) => patch({ description: event.target.value })}
            />
          </Field>
        </div>
      </div>

      {groups.map(({ group, fields }) => (
        <div className="fieldset" key={group}>
          <div className="fieldset__legend">{group}</div>
          <div className="form-grid" style={{ marginTop: 12 }}>
            {fields.map((field) => (
              <FieldControl
                key={field.key}
                field={field}
                value={draft.options[field.key]}
                onChange={(value) => patchOption(field.key, value)}
              />
            ))}
          </div>
        </div>
      ))}

      {hasAdvanced ? (
        <Checkbox
          checked={showAdvanced}
          onChange={setShowAdvanced}
          label="Show advanced settings"
        />
      ) : null}

      <div className="form-actions">
        <span className="grow small dim">
          Driver: {provider?.vendor} · v{provider?.driverVersion}
        </span>
        <Button variant="ghost" onClick={onCancel} disabled={saving || testing}>
          Cancel
        </Button>
        <Button icon="zap" onClick={test} loading={testing} disabled={saving}>
          Test
        </Button>
        <Button variant="primary" icon="check" onClick={submit} loading={saving} disabled={testing}>
          {profile ? 'Save changes' : 'Create connection'}
        </Button>
      </div>
    </div>
  );
}

function FieldControl({
  field,
  value,
  onChange,
}: {
  field: ConnectionField;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const [revealed, setRevealed] = useState(false);
  const full = field.kind === 'textarea' || field.kind === 'keyValue';

  const secretToggle = field.secret ? (
    <button
      type="button"
      className="btn btn--ghost btn--icon btn--sm"
      onClick={() => setRevealed((current) => !current)}
      aria-label={revealed ? 'Hide value' : 'Show value'}
      title={revealed ? 'Hide' : 'Show'}
    >
      <Icon name="eye" size={12} />
    </button>
  ) : undefined;

  switch (field.kind) {
    case 'boolean':
      return (
        <div className={cn('field', full && 'form-grid--full')} style={{ justifyContent: 'center' }}>
          <Toggle
            checked={value === true || value === 'true'}
            onChange={onChange}
            label={field.label}
            help={field.help ?? undefined}
          />
        </div>
      );

    case 'number':
      return (
        <Field label={field.label} help={field.help} required={field.required}>
          <input
            className="input input--mono"
            inputMode="numeric"
            value={value === undefined || value === null ? '' : String(value)}
            placeholder={field.placeholder ?? ''}
            onChange={(event) => {
              const raw = event.target.value.trim();
              onChange(raw === '' ? null : Number(raw));
            }}
          />
        </Field>
      );

    case 'select':
      return (
        <Field label={field.label} help={field.help} required={field.required}>
          <Select
            value={value === undefined || value === null ? '' : String(value)}
            onChange={onChange}
            options={field.options.map((option) => ({
              value: option.value,
              label: option.hint ? `${option.label} — ${option.hint}` : option.label,
            }))}
          />
        </Field>
      );

    case 'textarea':
      return (
        <Field label={field.label} help={field.help} required={field.required} full action={secretToggle}>
          <textarea
            className="textarea textarea--mono"
            value={typeof value === 'string' ? value : ''}
            placeholder={field.placeholder ?? ''}
            spellCheck={false}
            onChange={(event) => onChange(event.target.value)}
          />
        </Field>
      );

    case 'keyValue':
      return (
        <Field label={field.label} help={field.help} full>
          <KeyValueEditor
            rows={objectToRows(value)}
            onChange={(rows) => onChange(rowsToObject(rows))}
            addLabel="Add property"
            emptyLabel="No extra properties"
          />
        </Field>
      );

    default:
      return (
        <Field label={field.label} help={field.help} required={field.required} action={secretToggle}>
          <input
            className={cn('input', field.kind === 'password' && 'input--mono')}
            type={field.secret && !revealed ? 'password' : 'text'}
            value={typeof value === 'string' ? value : value === undefined || value === null ? '' : String(value)}
            placeholder={field.placeholder ?? ''}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => onChange(event.target.value)}
          />
        </Field>
      );
  }
}
