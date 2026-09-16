import { PageHeader } from '@/components/layout/PageHeader';
import { Field, Select, Toggle } from '@/components/ui/Field';
import { Button, DetailList, Panel } from '@/components/ui/primitives';
import { useAppInfo } from '@/store/app';
import { notifySuccess } from '@/store/toast';
import { useSettings } from '@/store/settings';
import type { AppSettings } from '@/types';

const DEFAULTS: AppSettings = {
  theme: 'dark',
  pageSize: 50,
  defaultMessageLimit: 200,
  prettyJson: true,
  confirmDestructive: true,
  autoConnect: false,
  timestampFormat: 'datetime',
};

export function Settings() {
  const settings = useSettings((state) => state.settings);
  const update = useSettings((state) => state.update);
  const info = useAppInfo((state) => state.info);

  const patch = (values: Partial<AppSettings>) => {
    void update(values).catch(() => undefined);
  };

  return (
    <div className="page">
      <PageHeader
        title="Settings"
        subtitle="Stored next to your connections in the user configuration directory."
      />

      <div className="split">
        <Panel title="Messages">
          <div className="stack-16">
            <Field
              label="Default fetch size"
              help="How many messages a browse request asks for before hitting the limit."
            >
              <Select
                value={String(settings.defaultMessageLimit)}
                onChange={(value) => patch({ defaultMessageLimit: Number(value) })}
                options={[
                  { value: '50', label: '50' },
                  { value: '200', label: '200' },
                  { value: '500', label: '500' },
                  { value: '1000', label: '1 000' },
                ]}
              />
            </Field>

            <Field label="Timestamp format" help="Used by every message grid.">
              <Select
                value={settings.timestampFormat}
                onChange={(value) => patch({ timestampFormat: value })}
                options={[
                  { value: 'datetime', label: 'Date and time' },
                  { value: 'time', label: 'Time only' },
                  { value: 'epoch', label: 'Epoch milliseconds' },
                ]}
              />
            </Field>

            <Toggle
              checked={settings.prettyJson}
              onChange={(value) => patch({ prettyJson: value })}
              label="Pretty print JSON payloads"
              help="Only affects display; the stored bytes are never rewritten."
            />
          </div>
        </Panel>

        <Panel title="Behaviour">
          <div className="stack-16">
            <Field label="Table page size" help="Rows rendered per page in long grids.">
              <Select
                value={String(settings.pageSize)}
                onChange={(value) => patch({ pageSize: Number(value) })}
                options={[
                  { value: '25', label: '25' },
                  { value: '50', label: '50' },
                  { value: '100', label: '100' },
                  { value: '250', label: '250' },
                ]}
              />
            </Field>

            <Toggle
              checked={settings.confirmDestructive}
              onChange={(value) => patch({ confirmDestructive: value })}
              label="Confirm destructive actions"
              help="Topic deletion, purges and offset resets always ask first in this build."
            />

            <Toggle
              checked={settings.autoConnect}
              onChange={(value) => patch({ autoConnect: value })}
              label="Connect on start-up"
              help="Opens every stored connection when the app launches."
            />
          </div>
        </Panel>
      </div>

      <div className="split mt-16">
        <Panel title="Workspace">
          <DetailList
            items={[
              { label: 'Version', value: info?.version ?? '—' },
              { label: 'Tauri', value: info?.tauriVersion ?? '—' },
              { label: 'Platform', value: info ? `${info.os} ${info.arch}` : '—' },
              { label: 'TLS builds', value: info?.tlsSupported ? 'enabled' : 'disabled' },
              {
                label: 'Workspace file',
                value: <span className="mono small">{info?.workspacePath ?? '—'}</span>,
              },
            ]}
          />
        </Panel>

        <Panel title="Maintenance">
          <div className="stack-12">
            <p className="small muted">
              Settings are written immediately. Resetting only touches preferences — connections and
              their credentials stay where they are.
            </p>
            <div className="row gap-8">
              <Button
                icon="reset"
                onClick={() => {
                  patch(DEFAULTS);
                  notifySuccess('Settings restored');
                }}
              >
                Restore defaults
              </Button>
            </div>
          </div>
        </Panel>
      </div>
    </div>
  );
}
