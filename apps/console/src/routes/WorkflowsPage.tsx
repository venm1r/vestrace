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

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

export const WorkflowsPage: React.FC = () => {
  const { data: workflows, error, loading, reload } = useApiResource(vestraceClient.listWorkflows);
  const { notice, notify, dismiss } = useNotice();

  const items = workflows ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Workflow Definitions"
        description="Registered workflow definitions and the revision currently published for each one."
        actions={
          <ActionButton
            icon="add"
            onClick={() =>
              notify('info', 'Workflow authoring from the console is not implemented in the P0 foundation.')
            }
          >
            Create Workflow
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
            resourceName="workflows"
            emptyMessage="No workflows are defined in this workspace."
            onRetry={reload}
          />
        </Panel>
      )}

      {items.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {items.map((workflow) => (
            <div
              key={workflow.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '8px',
                padding: '16px 24px',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: '16px',
                flexWrap: 'wrap',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '16px', minWidth: 0 }}>
                <span
                  className="material-symbols-outlined"
                  aria-hidden="true"
                  style={{ fontSize: '28px', color: 'var(--color-tertiary)' }}
                >
                  account_tree
                </span>
                <div style={{ minWidth: 0 }}>
                  <h2
                    style={{
                      fontFamily: 'var(--font-display)',
                      fontSize: '16px',
                      fontWeight: 600,
                      color: 'var(--text-primary)',
                      margin: 0,
                    }}
                  >
                    {workflow.name}
                  </h2>
                  <div
                    style={{
                      fontSize: '13px',
                      color: 'var(--text-secondary)',
                      marginTop: '2px',
                      display: 'flex',
                      gap: '16px',
                      flexWrap: 'wrap',
                    }}
                  >
                    <span style={{ fontFamily: 'var(--font-mono)', fontSize: '12px', wordBreak: 'break-all' }}>
                      {workflow.id}
                    </span>
                    <span>Created: {formatTimestamp(workflow.created_at)}</span>
                  </div>
                </div>
              </div>

              <span
                style={{
                  padding: '2px 8px',
                  borderRadius: '4px',
                  fontSize: '12px',
                  fontWeight: 600,
                  background: 'var(--color-surface-container-high)',
                  color: 'var(--color-tertiary)',
                  whiteSpace: 'nowrap',
                }}
              >
                Revision {workflow.current_revision}
              </span>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
