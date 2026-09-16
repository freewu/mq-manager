import { useState } from 'react';

import { Banner, Button, Panel } from '@/components/ui/primitives';
import { Field, KeyValueEditor, type KeyValueRow } from '@/components/ui/Field';
import { topicsApi } from '@/api/topics';
import { errorMessage } from '@/api/client';
import { useCapabilities } from '@/hooks/useConnection';
import { notifySuccess } from '@/store/toast';
import type { ConfigEntry, CreateTopicRequest, TopicDetail, UpdateTopicRequest } from '@/types';

const RESERVED = new Set([
  'name',
  'partitionCount',
  'replicationFactor',
  'partitions',
  'options',
]);

function configRows(configs: ConfigEntry[]): KeyValueRow[] {
  return configs
    .filter((config) => !config.readOnly && !RESERVED.has(config.name))
    .map((config) => ({ key: config.name, value: config.value ?? '' }));
}

export function TopicForm({
  mode,
  connectionId,
  detail,
  onDone,
  onCancel,
}: {
  mode: 'create' | 'edit';
  connectionId: string;
  detail?: TopicDetail;
  onDone: (topic: string) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(detail?.summary.name ?? '');
  const [partitionCount, setPartitionCount] = useState(
    String(detail?.summary.partitionCount ?? 3),
  );
  const [replicationFactor, setReplicationFactor] = useState('3');
  const [rows, setRows] = useState<KeyValueRow[]>(() => configRows(detail?.configs ?? []));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Servers without partitions (a queue based broker, for example) neither have
  // a partition count nor a replication factor to ask for.
  const capabilities = useCapabilities(connectionId);
  const supportsPartitions = capabilities.topics.partitions;

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const configs: ConfigEntry[] = rows
        .filter((row) => row.key.trim() !== '')
        .map((row) => ({
          name: row.key.trim(),
          value: row.value,
          source: null,
          readOnly: false,
          sensitive: false,
          isDefault: false,
        }));

      if (mode === 'create') {
        const request: CreateTopicRequest = {
          name: name.trim(),
          partitionCount: supportsPartitions ? Number(partitionCount) || null : null,
          replicationFactor: supportsPartitions ? Number(replicationFactor) || null : null,
          configs,
          options: {},
        };
        await topicsApi.create(connectionId, request);
        notifySuccess('Topic created', request.name);
        onDone(request.name);
      } else {
        const request: UpdateTopicRequest = {
          name: detail?.summary.name ?? name.trim(),
          partitionCount: supportsPartitions ? Number(partitionCount) || null : null,
          configs,
        };
        await topicsApi.update(connectionId, request);
        notifySuccess('Topic updated', request.name);
        onDone(request.name);
      }
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="stack-12">
      {error ? <Banner tone="danger">{error}</Banner> : null}

      <div className="form-grid">
        <Field label="Name" required help="Cannot be changed afterwards.">
          <input
            className="input input--mono"
            value={name}
            disabled={mode === 'edit'}
            autoFocus={mode === 'create'}
            spellCheck={false}
            placeholder="orders"
            onChange={(event) => setName(event.target.value)}
          />
        </Field>

        {supportsPartitions ? (
          <Field
            label="Partitions"
            help={mode === 'edit' ? 'Partitions can only be increased.' : 'Number of partitions.'}
          >
            <input
              className="input input--mono"
              inputMode="numeric"
              value={partitionCount}
              onChange={(event) => setPartitionCount(event.target.value.replace(/[^0-9]/g, ''))}
            />
          </Field>
        ) : null}

        {supportsPartitions && mode === 'create' ? (
          <Field
            label="Replication factor"
            help="Must not exceed the broker count. Ignored by brokers without per-topic replication."
          >
            <input
              className="input input--mono"
              inputMode="numeric"
              value={replicationFactor}
              onChange={(event) => setReplicationFactor(event.target.value.replace(/[^0-9]/g, ''))}
            />
          </Field>
        ) : null}
      </div>

      <Panel title="Configuration overrides" flush>
        <div className="panel__body">
          <KeyValueEditor
            rows={rows}
            onChange={setRows}
            keyPlaceholder={supportsPartitions ? 'cleanup.policy' : 'x-message-ttl'}
            valuePlaceholder={supportsPartitions ? 'compact' : '60000'}
            addLabel="Add override"
            emptyLabel="No overrides — broker defaults apply"
          />
        </div>
      </Panel>

      <div className="form-actions" style={{ marginTop: 0, paddingTop: 0, borderTop: 0 }}>
        <Button variant="ghost" onClick={onCancel} disabled={busy}>
          Cancel
        </Button>
        <Button
          variant="primary"
          icon="check"
          loading={busy}
          disabled={mode === 'create' && name.trim() === ''}
          onClick={submit}
        >
          {mode === 'create' ? 'Create topic' : 'Save changes'}
        </Button>
      </div>
    </div>
  );
}
