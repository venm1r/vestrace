import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface WorkflowItem {
  id: string;
  name: string;
  stepsCount: number;
  mode: 'Direct' | 'Guided' | 'Workflow';
  createdAt: string;
}

export const WorkflowsPage: React.FC = () => {
  const [workflows] = React.useState<WorkflowItem[]>([
    {
      id: 'wf_10a9f81c',
      name: 'Automated Code Review & Security Audit',
      stepsCount: 5,
      mode: 'Workflow',
      createdAt: '2026-08-01T12:00:00Z',
    },
    {
      id: 'wf_448a02d3',
      name: 'Memory Ingestion and Knowledge Consolidation',
      stepsCount: 3,
      mode: 'Guided',
      createdAt: '2026-08-02T08:30:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Workflows Registry</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            Versioned multi-step workflow graphs, planning modes, and execution rules.
          </p>
        </div>
        <Button variant="primary">Create Workflow</Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {workflows.map((wf) => (
          <Surface key={wf.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <h3 style={{ margin: 0, fontSize: '16px', color: 'var(--brand-cyan)' }}>{wf.name}</h3>
              <span
                style={{
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-round)',
                  fontSize: '11px',
                  fontWeight: 700,
                  backgroundColor: 'var(--bg-level-1)',
                  color: 'var(--brand-white)',
                  border: '1px solid var(--border-color)',
                }}
              >
                {wf.mode.toUpperCase()} MODE
              </span>
            </div>
            <div style={{ fontSize: '13px', color: 'var(--text-secondary)', display: 'flex', gap: 'var(--space-4)' }}>
              <span>ID: <code style={{ color: 'var(--brand-white)' }}>{wf.id}</code></span>
              <span>Steps: {wf.stepsCount}</span>
            </div>
            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <Button variant="secondary">View Graph</Button>
              <Button variant="ghost">Execute Plan</Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
