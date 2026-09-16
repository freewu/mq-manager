import { useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';

import { ConnectionForm } from '@/components/domain/ConnectionForm';
import { ConnectionCard } from '@/components/domain/ConnectionCard';
import { CapabilityList } from '@/components/domain/Capabilities';
import { PageHeader } from '@/components/layout/PageHeader';
import { ConfirmDialog, Modal } from '@/components/ui/Modal';
import { Badge, Button, EmptyState, Panel, ProviderChip } from '@/components/ui/primitives';
import { notifyError, notifySuccess } from '@/store/toast';
import { useWorkspace } from '@/store/workspace';
import type { ConnectionProfile } from '@/types';

export function Connections() {
  const navigate = useNavigate();
  const [searchParams, setSearchParams] = useSearchParams();

  const providers = useWorkspace((state) => state.providers);
  const profiles = useWorkspace((state) => state.profiles);
  const statuses = useWorkspace((state) => state.statuses);
  const busy = useWorkspace((state) => state.busy);
  const connect = useWorkspace((state) => state.connect);
  const disconnect = useWorkspace((state) => state.disconnect);
  const remove = useWorkspace((state) => state.remove);

  const [editing, setEditing] = useState<ConnectionProfile | null>(null);
  const [creating, setCreating] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<ConnectionProfile | null>(null);
  const [deleting, setDeleting] = useState(false);
  const [showDrivers, setShowDrivers] = useState(false);

  useEffect(() => {
    if (searchParams.get('new') === '1') {
      setCreating(true);
      setSearchParams({}, { replace: true });
    }
    // Only react to the query string; setSearchParams is stable enough here.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchParams]);

  const closeForm = () => {
    setCreating(false);
    setEditing(null);
  };

  return (
    <div className="page">
      <PageHeader
        title="Connections"
        subtitle="Each connection stores credentials for one broker and can be kept open in parallel."
        actions={
          <>
            <Button variant="ghost" icon="sliders" onClick={() => setShowDrivers((value) => !value)}>
              {showDrivers ? 'Hide drivers' : 'Show drivers'}
            </Button>
            <Button variant="primary" icon="plus" onClick={() => setCreating(true)}>
              New connection
            </Button>
          </>
        }
      />

      {showDrivers ? (
        <div className="stack-12 mb-8">
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
                <div className="row gap-8 tiny dim">
                  <span>{provider.fields.length} configuration fields</span>
                  {provider.defaultPort ? (
                    <>
                      <span>·</span>
                      <span className="mono">default port {provider.defaultPort}</span>
                    </>
                  ) : null}
                </div>
              </div>
            </Panel>
          ))}
        </div>
      ) : null}

      {profiles.length === 0 ? (
        <Panel>
          <EmptyState
            icon="plug"
            title="No connections yet"
            text="A connection points at one broker cluster. Credentials are stored locally in your user configuration directory and never leave the machine."
            actions={
              <Button variant="primary" icon="plus" onClick={() => setCreating(true)}>
                Create the first connection
              </Button>
            }
          />
        </Panel>
      ) : (
        <div className="grid-cards">
          {profiles.map((profile) => (
            <ConnectionCard
              key={profile.id}
              profile={profile}
              provider={providers.find((entry) => entry.id === profile.provider)}
              status={statuses[profile.id]}
              busy={Boolean(busy[profile.id])}
              onOpen={() => navigate(`/c/${profile.id}`)}
              onConnect={async () => {
                const ok = await connect(profile.id);
                if (ok) notifySuccess('Connected', profile.name);
                else notifyError('See the card for details', 'Connection failed');
              }}
              onDisconnect={() => void disconnect(profile.id)}
              onEdit={() => setEditing(profile)}
              onDelete={() => setPendingDelete(profile)}
            />
          ))}
        </div>
      )}

      <Modal
        open={creating || editing !== null}
        onClose={closeForm}
        variant="right"
        title={editing ? `Edit ${editing.name}` : 'New connection'}
        subtitle="Fields come from the selected driver."
      >
        <ConnectionForm
          profile={editing}
          onSaved={(saved) => {
            closeForm();
            navigate(`/c/${saved.id}`);
          }}
          onCancel={closeForm}
        />
      </Modal>

      <ConfirmDialog
        open={pendingDelete !== null}
        title="Delete connection"
        message={
          <>
            <strong>{pendingDelete?.name}</strong> will be disconnected and removed from this
            machine. Broker data is not touched.
          </>
        }
        confirmLabel="Delete"
        busy={deleting}
        onCancel={() => setPendingDelete(null)}
        onConfirm={async () => {
          if (!pendingDelete) return;
          setDeleting(true);
          try {
            await remove(pendingDelete.id);
            notifySuccess('Connection deleted', pendingDelete.name);
            setPendingDelete(null);
          } catch (error) {
            notifyError(error, 'Could not delete the connection');
          } finally {
            setDeleting(false);
          }
        }}
      />
    </div>
  );
}
