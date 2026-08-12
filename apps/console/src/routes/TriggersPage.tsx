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

export const TriggersPage: React.FC = () => {
  const { data: triggers, error, loading, reload } = useApiResource(vestraceClient.listTriggers);
  const { notice, notify, dismiss } = useNotice();

  const items = triggers ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Event Triggers & Schedules"
        description="Webhooks, cron schedules, and external event listeners bound to this workspace."
        actions={
          <ActionButton
            icon="bolt"
            onClick={() => notify('info', 'Trigger creation is not implemented in the P0 foundation.')}
          >
            Add Trigger
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
            resourceName="triggers"
            emptyMessage="No triggers are configured in this workspace."
            onRetry={reload}
          />
        </Panel>
      )}

      {items.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {items.map((trigger) => (
            <div
              key={trigger.id}
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
                  style={{ fontSize: '28px', color: 'var(--color-warning)' }}
                >
                  bolt
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
                    {trigger.name}
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
                    <span>
                      Type: <strong>{trigger.type}</strong>
                    </span>
                    <span>
                      Target: <strong style={{ color: 'var(--color-tertiary)' }}>{trigger.target}</strong>
                    </span>
                  </div>
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
                {trigger.status}
              </span>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
