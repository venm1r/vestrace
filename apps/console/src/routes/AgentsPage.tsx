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

export const AgentsPage: React.FC = () => {
  const { data: agents, error, loading, reload } = useApiResource(vestraceClient.listAgents);
  const { notice, notify, dismiss } = useNotice();

  const items = agents ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Agent Registry"
        description="Registered agent definitions: name, purpose, and the system prompt bound to each agent."
        actions={
          <ActionButton
            icon="smart_toy"
            onClick={() =>
              notify('info', 'Agent registration from the console is not implemented in the P0 foundation.')
            }
          >
            Register Agent
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
            resourceName="agents"
            emptyMessage="No agents are registered in this workspace."
            onRetry={reload}
          />
        </Panel>
      )}

      {items.length > 0 && (
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(300px, 1fr))', gap: '16px' }}>
          {items.map((agent) => (
            <div
              key={agent.id}
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
              <div style={{ display: 'flex', alignItems: 'center', gap: '12px' }}>
                <span
                  className="material-symbols-outlined"
                  aria-hidden="true"
                  style={{ fontSize: '32px', color: 'var(--color-tertiary)' }}
                >
                  smart_toy
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
                    {agent.name}
                  </h2>
                  <span
                    style={{
                      fontSize: '12px',
                      color: 'var(--text-secondary)',
                      fontFamily: 'var(--font-mono)',
                      wordBreak: 'break-all',
                    }}
                  >
                    {agent.id}
                  </span>
                </div>
              </div>

              <p style={{ fontSize: '13px', color: 'var(--text-secondary)', margin: 0 }}>
                {agent.description || 'No description was recorded for this agent.'}
              </p>

              <details>
                <summary style={{ fontSize: '13px', color: 'var(--color-tertiary)', cursor: 'pointer' }}>
                  System prompt
                </summary>
                <pre
                  style={{
                    marginTop: '8px',
                    marginBottom: 0,
                    background: 'var(--color-surface-container-lowest)',
                    border: '1px solid var(--color-outline)',
                    borderRadius: '8px',
                    padding: '12px',
                    fontFamily: 'var(--font-mono)',
                    fontSize: '12px',
                    color: 'var(--color-on-surface)',
                    whiteSpace: 'pre-wrap',
                    wordBreak: 'break-word',
                    maxHeight: '220px',
                    overflowY: 'auto',
                  }}
                >
                  {agent.system_prompt || '(empty)'}
                </pre>
              </details>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
