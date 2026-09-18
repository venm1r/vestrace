import React, { useState } from 'react';
import { ConnectionAuthMode, ConnectionKind, ConnectionTransportPolicy, vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import { usePolledQualification } from '../sdk/useQualificationPolling';
import { Modal } from '../design-system/primitives/Modal';
import { Button } from '../design-system/primitives/Button';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  describeError,
  useNotice,
} from '../shell/PageState';

export const ConnectionsPage: React.FC = () => {
  const { data: connections, error, loading, reload } = useApiResource(vestraceClient.listConnections);
  const { notice, notify, dismiss } = useNotice();
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [name, setName] = useState('');
  const [preset, setPreset] = useState<'lm_studio' | 'openai_compatible'>('lm_studio');
  const [runtimeBaseUrl, setRuntimeBaseUrl] = useState('http://host.docker.internal:12345/v1');
  const [submitting, setSubmitting] = useState(false);
  const [testingId, setTestingId] = useState<string | null>(null);
  const [chatRevisionInput, setChatRevisionInput] = useState('');
  const [embeddingRevisionInput, setEmbeddingRevisionInput] = useState('');
  const { item: testedConnection, polling: testingQualification } = usePolledQualification(
    vestraceClient.listConnections,
    testingId,
  );

  const items = connections ?? [];

  const handleCreate = async (event: React.FormEvent) => {
    event.preventDefault();
    if (!name.trim()) {
      notify('warning', 'Connection name is required.');
      return;
    }
    setSubmitting(true);
    try {
      const kind: ConnectionKind = preset === 'lm_studio' ? 'l_m_studio_local' : 'open_ai_chat_completions_v1';
      const authMode: ConnectionAuthMode = preset === 'lm_studio' ? 'none' : 'bearer';
      const transportPolicy: ConnectionTransportPolicy =
        preset === 'lm_studio' ? { kind: 'loopback_only' } : { kind: 'remote_https' };
      const credentialSlotId = authMode === 'none' ? null : crypto.randomUUID();

      await vestraceClient.createConnection(
        {
          connection_id: crypto.randomUUID(),
          connector_id: crypto.randomUUID(),
          name: name.trim(),
          revision_id: crypto.randomUUID(),
          execution_guard_id: crypto.randomUUID(),
          kind,
          logical_base_url: runtimeBaseUrl,
          runtime_base_url: runtimeBaseUrl,
          adapter_profile_revision: 'openai-chat-completions/v1',
          transport_policy: transportPolicy,
          auth_mode: authMode,
          credential_slot_id: credentialSlotId,
          expected_head_version: 0,
        },
        { requestId: crypto.randomUUID() },
      );

      reload();
      setIsModalOpen(false);
      setName('');
      notify('success', `Connection "${name.trim()}" was created.`);
    } catch (err: unknown) {
      const described = describeError(err, 'connection creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  const handleTest = async (connectionId: string) => {
    const connection = items.find((c) => c.id === connectionId);
    if (!connection?.revision_id) {
      notify('warning', 'This connection has no published revision yet.');
      return;
    }
    if (!connection.no_auth_binding_revision_id) {
      notify('warning', 'This connection has no no-auth binding yet — only the no-auth branch is supported here.');
      return;
    }
    if (!chatRevisionInput.trim() || !embeddingRevisionInput.trim()) {
      notify('warning', 'Paste a chat and an embedding Model revision id to qualify against.');
      return;
    }
    try {
      await vestraceClient.requestConnectionQualification(
        connectionId,
        {
          job_id: crypto.randomUUID(),
          target_binding_id: crypto.randomUUID(),
          connection_id: connectionId,
          connection_revision_id: connection.revision_id,
          target: { branch: 'no_auth', binding_revision_id: connection.no_auth_binding_revision_id },
          chat_model_revision_id: chatRevisionInput.trim(),
          embedding_model_revision_id: embeddingRevisionInput.trim(),
        },
        { requestId: crypto.randomUUID() },
      );
      setTestingId(connectionId);
      notify('info', `Qualification requested for ${connectionId}.`);
    } catch (err: unknown) {
      const described = describeError(err, 'connection qualification');
      notify('error', `${described.title}: ${described.detail}`);
    }
  };

  return (
    <PageShell>
      <PageHeader
        title="External Connections"
        description="Credential brokers, PostgreSQL pools, vector databases, and LLM provider gateways."
        actions={
          <ActionButton
            icon="cable"
            onClick={() => setIsModalOpen(true)}
          >
            Add Connection
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title="Configure External Connection"
      >
        <form onSubmit={handleCreate} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <label htmlFor="connection-name" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Name *
            </label>
            <input
              id="connection-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. LM Studio (local)"
              className="field-control"
              style={{ width: '100%', boxSizing: 'border-box' }}
            />
          </div>
          <div>
            <label htmlFor="connection-preset" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Provider *
            </label>
            <select
              id="connection-preset"
              value={preset}
              onChange={(e) => {
                const next = e.target.value as typeof preset;
                setPreset(next);
                setRuntimeBaseUrl(
                  next === 'lm_studio' ? 'http://host.docker.internal:12345/v1' : 'https://api.example.com/v1',
                );
              }}
              className="field-control"
              style={{ width: '100%' }}
            >
              <option value="lm_studio">LM Studio (local, no credential)</option>
              <option value="openai_compatible">Remote OpenAI-compatible (bearer credential)</option>
            </select>
          </div>
          <div>
            <label htmlFor="connection-url" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Runtime base URL *
            </label>
            <input
              id="connection-url"
              type="text"
              required
              value={runtimeBaseUrl}
              onChange={(e) => setRuntimeBaseUrl(e.target.value)}
              className="field-control"
              style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'var(--font-mono)' }}
            />
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px', marginTop: '8px' }}>
            <Button variant="secondary" onClick={() => setIsModalOpen(false)} disabled={submitting}>
              Cancel
            </Button>
            <Button variant="primary" type="submit" disabled={submitting} icon="add">
              {submitting ? 'Creating...' : 'Create'}
            </Button>
          </div>
        </form>
      </Modal>

      {(loading || error || items.length === 0) && (
        <Panel>
          <ResourceState
            loading={loading}
            error={error}
            isEmpty={items.length === 0}
            resourceName="connections"
            emptyMessage="No external connections are configured in this workspace."
            onRetry={reload}
          />
        </Panel>
      )}

      {items.length > 0 && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '16px' }}>
          {items.map((connection) => (
            <div
              key={connection.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '12px',
                padding: '20px',
                display: 'flex',
                flexDirection: 'column',
                gap: '16px',
              }}
            >
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: '12px' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: '12px', minWidth: 0 }}>
                  <span
                    className="material-symbols-outlined"
                    aria-hidden="true"
                    style={{ fontSize: '32px', color: 'var(--color-tertiary)' }}
                  >
                    hub
                  </span>
                  <div style={{ minWidth: 0 }}>
                    <h2
                      style={{
                        fontFamily: 'var(--font-display)',
                        fontSize: '18px',
                        fontWeight: 600,
                        color: 'var(--text-primary)',
                        margin: 0,
                      }}
                    >
                      {connection.id}
                    </h2>
                    <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>
                      Revision: {connection.revision_id ?? 'none'}
                    </span>
                  </div>
                </div>

                <span
                  style={{
                    padding: '2px 8px',
                    borderRadius: '4px',
                    fontSize: '12px',
                    fontWeight: 600,
                    textTransform: 'uppercase',
                    background: 'var(--color-surface-container-high)',
                    color: 'var(--color-tertiary)',
                    whiteSpace: 'nowrap',
                  }}
                >
                  {connection.state}
                </span>
              </div>

              <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
                Qualification:{' '}
                <strong style={{ color: 'var(--text-primary)', fontFamily: 'var(--font-mono)' }}>
                  {connection.qualification_state ?? 'unqualified'}
                </strong>
                {connection.blockers.length > 0 && (
                  <span style={{ display: 'block', marginTop: '4px' }}>
                    Blockers: {connection.blockers.join(', ')}
                  </span>
                )}
              </div>

              <div style={{ display: 'flex', flexDirection: 'column', gap: '6px' }}>
                {testingId === connection.id ? (
                  <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>
                    Qualification:{' '}
                    <strong style={{ color: 'var(--text-primary)' }}>
                      {testingQualification
                        ? 'checking...'
                        : (testedConnection?.qualification_state ?? 'unknown')}
                    </strong>
                  </span>
                ) : (
                  <>
                    <input
                      type="text"
                      value={chatRevisionInput}
                      onChange={(e) => setChatRevisionInput(e.target.value)}
                      placeholder="Chat Model revision id"
                      className="field-control"
                      style={{ fontSize: '12px', fontFamily: 'var(--font-mono)' }}
                    />
                    <input
                      type="text"
                      value={embeddingRevisionInput}
                      onChange={(e) => setEmbeddingRevisionInput(e.target.value)}
                      placeholder="Embedding Model revision id"
                      className="field-control"
                      style={{ fontSize: '12px', fontFamily: 'var(--font-mono)' }}
                    />
                  </>
                )}
                <ActionButton
                  variant="quiet"
                  style={{ padding: '8px 14px', fontSize: '13px' }}
                  onClick={() => void handleTest(connection.id)}
                >
                  Test Connection
                </ActionButton>
              </div>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
