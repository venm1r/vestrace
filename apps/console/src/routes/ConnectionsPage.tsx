import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
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

  const items = connections ?? [];

  return (
    <PageShell>
      <PageHeader
        title="External Connections"
        description="Credential brokers, PostgreSQL pools, vector databases, and LLM provider gateways."
        actions={
          <ActionButton
            icon="cable"
            onClick={() => notify('info', 'Connection creation is not implemented in this build.')}
          >
            Add Connection
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

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
