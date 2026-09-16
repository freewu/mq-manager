import { useState } from 'react';

import { Banner, Button, Panel } from '@/components/ui/primitives';
import { Field, KeyValueEditor, Select, type KeyValueRow } from '@/components/ui/Field';
import { JsonEditor } from '@/components/ui/JsonEditor';
import { messagesApi } from '@/api/messages';
import { errorMessage } from '@/api/client';
import { byteLength } from '@/lib/format';
import { rowsToObject } from '@/lib/json';
import { notifySuccess } from '@/store/toast';
import type { MessageHeader, PayloadEncoding, ProduceRequest, ProduceResult } from '@/types';

export interface ProduceFormProps {
  connectionId: string;
  topic: string;
  partitions?: number[] | null;
  supportsKeys: boolean;
  supportsHeaders: boolean;
  supportsPartitions: boolean;
  /** Called after a successful produce so the page can refresh counts. */
  onProduced?: (result: ProduceResult) => void;
}

const HEADER_ENCODINGS: Array<{ value: PayloadEncoding; label: string }> = [
  { value: 'utf8', label: 'UTF-8' },
  { value: 'base64', label: 'Base64' },
];

export function ProduceForm({
  connectionId,
  topic,
  partitions,
  supportsKeys,
  supportsHeaders,
  supportsPartitions,
  onProduced,
}: ProduceFormProps) {
  const [payload, setPayload] = useState('{\n  "hello": "world"\n}');
  const [key, setKey] = useState('');
  const [encoding, setEncoding] = useState<PayloadEncoding>('utf8');
  const [partition, setPartition] = useState<string>('');
  const [headers, setHeaders] = useState<KeyValueRow[]>([]);
  const [sending, setSending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [last, setLast] = useState<ProduceResult | null>(null);

  const send = async () => {
    setSending(true);
    setError(null);
    try {
      const headerList: MessageHeader[] = Object.entries(rowsToObject(headers)).map(
        ([headerKey, value]) => ({ key: headerKey, value, encoding: 'utf8' }),
      );

      const request: ProduceRequest = {
        topic,
        key: supportsKeys && key !== '' ? key : null,
        payload,
        headers: supportsHeaders ? headerList : [],
        partition: supportsPartitions && partition !== '' ? Number(partition) : null,
        timestamp: null,
        encoding,
        options: {},
      };

      const result = await messagesApi.produce(connectionId, request);
      setLast(result);
      notifySuccess(
        'Message produced',
        `partition ${result.partition} · offset ${result.offset} · ${result.elapsedMs} ms`,
      );
      onProduced?.(result);
    } catch (caught) {
      setError(errorMessage(caught));
    } finally {
      setSending(false);
    }
  };

  return (
    <div className="stack-12">
      {error ? <Banner tone="danger">{error}</Banner> : null}

      <Panel title={`Produce to ${topic}`}>
        <div className="stack-12">
          <div className="form-grid">
            {supportsKeys ? (
              <Field label="Key" help="Optional, decides the target partition.">
                <input
                  className="input input--mono"
                  value={key}
                  spellCheck={false}
                  onChange={(event) => setKey(event.target.value)}
                />
              </Field>
            ) : null}

            {supportsPartitions ? (
              <Field label="Partition" help="Leave empty to let the broker choose.">
                <Select
                  value={partition}
                  onChange={setPartition}
                  options={[
                    { value: '', label: 'Automatic' },
                    ...(partitions ?? []).map((id) => ({ value: String(id), label: `Partition ${id}` })),
                  ]}
                />
              </Field>
            ) : null}

            <Field label="Encoding">
              <Select
                value={encoding}
                onChange={(value) => setEncoding(value as PayloadEncoding)}
                options={HEADER_ENCODINGS}
              />
            </Field>
          </div>

          <Field label="Payload">
            <JsonEditor
              value={payload}
              onChange={setPayload}
              encoding={encoding}
              label="Payload"
              rows={9}
            />
          </Field>

          {supportsHeaders ? (
            <Field label="Headers">
              <KeyValueEditor
                rows={headers}
                onChange={setHeaders}
                addLabel="Add header"
                emptyLabel="No headers"
              />
            </Field>
          ) : null}

          <div className="row-between">
            <span className="tiny dim">{byteLength(payload)} bytes</span>
            <Button variant="primary" icon="send" onClick={send} loading={sending}>
              Send
            </Button>
          </div>

          {last ? (
            <div className="row gap-8 small muted">
              <span className="badge badge--success">sent</span>
              <span className="mono">
                partition {last.partition} · offset {last.offset}
              </span>
            </div>
          ) : null}
        </div>
      </Panel>

      <Panel title="Notes">
        <div className="stack-8 small muted">
          <span>
            Payloads are sent unchanged; switch the encoding to <span className="mono">base64</span>{' '}
            when you need to deliver raw bytes.
          </span>
          {supportsHeaders ? null : <span>This driver does not support message headers.</span>}
        </div>
      </Panel>
    </div>
  );
}
