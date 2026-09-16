import { CapabilityList } from '@/components/domain/Capabilities';
import { PageHeader } from '@/components/layout/PageHeader';
import { Badge, DetailList, Panel, ProviderChip } from '@/components/ui/primitives';
import { useAppInfo } from '@/store/app';
import { useWorkspace } from '@/store/workspace';

export function About() {
  const info = useAppInfo((state) => state.info);
  const providers = useWorkspace((state) => state.providers);

  return (
    <div className="page">
      <PageHeader
        title="About"
        subtitle="A provider agnostic message queue manager. Kafka is the first driver, not the last."
      />

      <div className="split">
        <Panel title="Build">
          <DetailList
            items={[
              { label: 'Application', value: info?.name ?? 'MQ Manager' },
              { label: 'Version', value: info?.version ?? '—' },
              { label: 'Tauri', value: info?.tauriVersion ?? '—' },
              { label: 'Platform', value: info ? `${info.os} · ${info.arch}` : '—' },
              {
                label: 'TLS',
                value: info ? (
                  <Badge tone={info.tlsSupported ? 'success' : 'warning'}>
                    {info.tlsSupported ? 'supported' : 'not compiled in'}
                  </Badge>
                ) : (
                  '—'
                ),
              },
              { label: 'Drivers', value: info?.providers.join(', ') ?? '—' },
              {
                label: 'Workspace',
                value: <span className="mono small">{info?.workspacePath ?? '—'}</span>,
              },
            ]}
          />
        </Panel>

        <Panel title="Design notes">
          <div className="stack-8 small muted" style={{ lineHeight: 1.6 }}>
            <p>
              Everything above the driver layer is broker neutral: the interface renders
              <span className="mono"> capabilities</span> advertised by the active provider, so a
              queue without partitions or a broker without consumer groups simply hides those
              screens.
            </p>
            <p>
              Live tail sessions use a dedicated consumer with a throwaway group id and autocommit
              disabled — inspecting a topic never disturbs real consumers.
            </p>
            <p>
              Credentials stay on this machine, inside the workspace file listed above. Nothing is
              sent anywhere else.
            </p>
          </div>
        </Panel>
      </div>

      <div className="stack-12 mt-16">
        {providers.map((provider) => (
          <Panel
            key={provider.id}
            title={
              <span className="row gap-8">
                <ProviderChip name={provider.name} accent={provider.accent} />
                <span className="muted small">{provider.vendor}</span>
              </span>
            }
            actions={<Badge tone="neutral">v{provider.driverVersion}</Badge>}
          >
            <div className="stack-12">
              <p className="small muted">{provider.description}</p>
              <CapabilityList capabilities={provider.capabilities} />
            </div>
          </Panel>
        ))}
      </div>
    </div>
  );
}
