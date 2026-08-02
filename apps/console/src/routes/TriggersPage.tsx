import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface TriggerItem {
  id: string;
  name: string;
  triggerType: string;
  targetWorkflow: string;
  enabled: boolean;
  createdAt: string;
}

export const TriggersPage: React.FC = () => {
  const [triggers] = React.useState<TriggerItem[]>([
    {
      id: 'trg_11a9f02c',
      name: 'GitHub Webhook Push Event',
      triggerType: 'webhook.github',
      targetWorkflow: 'Automated Code Review & Security Audit',
      enabled: true,
      createdAt: '2026-08-01T11:00:00Z',
    },
    {
      id: 'trg_99b01c44',
      name: 'Daily Memory Reconciliation Cron',
      triggerType: 'schedule.cron',
      targetWorkflow: 'Memory Ingestion and Knowledge Consolidation',
      enabled: true,
      createdAt: '2026-08-02T00:00:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Triggers & Webhooks</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            External webhook event listeners and scheduled cron trigger bindings.
          </p>
        </div>
        <Button variant="primary" onClick={() => alert('Opening Trigger & Webhook Creation Wizard...')}>
          Create Trigger
        </Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {triggers.map((trg) => (
          <Surface key={trg.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--brand-cyan)' }}>{trg.name}</span>
                <code style={{ fontSize: '12px', color: 'var(--brand-white)' }}>({trg.triggerType})</code>
              </div>
              <span
                style={{
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-round)',
                  fontSize: '11px',
                  fontWeight: 700,
                  textTransform: 'uppercase',
                  backgroundColor: trg.enabled ? 'var(--semantic-success)' : 'var(--semantic-warning)',
                  color: '#000',
                }}
              >
                {trg.enabled ? 'ACTIVE' : 'DISABLED'}
              </span>
            </div>

            <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
              Target Workflow: <strong style={{ color: 'var(--text-primary)' }}>{trg.targetWorkflow}</strong>
            </div>

            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <Button variant="secondary" onClick={() => alert(`Editing binding configuration for trigger ${trg.id}...`)}>
                Edit Binding
              </Button>
              <Button variant="ghost" onClick={() => alert(`Fetching HMAC delivery logs for trigger ${trg.name}...`)}>
                View Delivery Logs
              </Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
