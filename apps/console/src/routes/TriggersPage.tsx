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

export const TriggersPage: React.FC = () => {
  const { data: triggers, error, loading, reload } = useApiResource(vestraceClient.listTriggers);
  const { notice, dismiss } = useNotice();
  const [isModalOpen, setIsModalOpen] = useState(false);

  const items = triggers ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Event Triggers & Schedules"
        description="Webhooks, cron schedules, and external event listeners bound to this workspace."
        actions={
          <ActionButton
            icon="bolt"
            onClick={() => setIsModalOpen(true)}
          >
            Add Trigger
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title="Configure Workspace Trigger"
      >
        <div style={{ display: 'flex', flexDirection: 'column', gap: '16px', fontSize: '13px', color: 'var(--text-primary)' }}>
          <p style={{ margin: 0, lineHeight: '1.5' }}>
            Triggers bind incoming external webhook events, cron schedules, or domain outbox changes to autonomous workflow execution runs.
          </p>
          <div style={{ padding: '12px', borderRadius: 'var(--radius-md)', backgroundColor: 'var(--color-surface-container-lowest)', border: '1px solid var(--color-outline-variant)' }}>
            <div style={{ fontWeight: 600, marginBottom: '6px', color: 'var(--color-tertiary)' }}>Trigger Binding Types:</div>
            <ul style={{ margin: 0, paddingLeft: '20px', display: 'flex', flexDirection: 'column', gap: '6px', fontSize: '12px' }}>
              <li><strong>Webhook Listener</strong>: dispatches HTTP payloads directly to a workflow start step</li>
              <li><strong>Cron Schedule</strong>: triggers periodic health inspections, evaluations, or runs</li>
              <li><strong>Outbox Topic Subscription</strong>: listens to domain events (e.g. <code>memory.created</code>, <code>run.succeeded</code>)</li>
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
