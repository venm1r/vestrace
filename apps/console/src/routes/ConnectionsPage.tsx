import React, { useState } from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import { Modal } from '../design-system/primitives/Modal';
import { Button } from '../design-system/primitives/Button';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  useNotice,
} from '../shell/PageState';

export const ConnectionsPage: React.FC = () => {
  const { data: connections, error, loading, reload } = useApiResource(vestraceClient.listConnections);
  const { notice, notify, dismiss } = useNotice();
  const [isModalOpen, setIsModalOpen] = useState(false);

  const items = connections ?? [];

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
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', fontSize: '13px', color: 'var(--text-primary)' }}>
          <p style={{ margin: 0, lineHeight: '1.5' }}>
            External connections and credential brokers in Vestrace are provisioned through environment variables and deployment manifests to guarantee secure custody and prevent cleartext secret leaks.
          </p>
          <div style={{ padding: '12px', borderRadius: 'var(--radius-md)', backgroundColor: 'var(--color-surface-container-lowest)', border: '1px solid var(--color-outline-variant)' }}>
            <div style={{ fontWeight: 600, marginBottom: '6px', color: 'var(--color-tertiary)' }}>Configuration Keys:</div>
            <ul style={{ margin: 0, paddingLeft: '20px', display: 'flex', flexDirection: 'column', gap: '6px', fontFamily: 'var(--font-mono)', fontSize: '12px' }}>
              <li><strong>VESTRACE_DATABASE__URL</strong>: PostgreSQL connection string</li>
              <li><strong>VESTRACE_EMBEDDING__BASE_URL</strong>: Embedding / vector engine endpoint</li>
              <li><strong>VESTRACE_EFFECTS__WEBHOOK__*</strong>: Outbound effect adapters &amp; callbacks</li>
              <li><strong>VESTRACE_SECRETS__MASTER_KEY</strong>: Envelope encryption master key</li>
            </ul>
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: '8px' }}>
            <Button variant="primary" onClick={() => setIsModalOpen(false)}>
              Got it
            </Button>
          </div>
        </div>
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
                      {connection.name}
                    </h2>
                    <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>
                      Type: {connection.type}
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
                  {connection.status}
                </span>
              </div>

              <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
                Ping latency:{' '}
                <strong style={{ color: 'var(--text-primary)', fontFamily: 'var(--font-mono)' }}>
                  {connection.latency}
                </strong>
              </div>

              <ActionButton
                variant="quiet"
                style={{ padding: '8px 14px', fontSize: '13px' }}
                onClick={() =>
                  notify(
                    'info',
                    `Connection tests are not implemented in this build (${connection.name}).`,
                  )
                }
              >
                Test Connection
              </ActionButton>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
