import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface RunListItem {
  id: string;
  title: string;
  status: 'Running' | 'Waiting' | 'Completed' | 'Failed' | 'Cancelled';
  version: number;
  stepsCount: number;
  createdAt: string;
}

export const RunsPage: React.FC = () => {
  const [runs] = React.useState<RunListItem[]>([
    {
      id: 'run_4092',
      title: 'Durable Task Execution #4092: RLS Isolation Audit',
      status: 'Running',
      version: 4,
      stepsCount: 5,
      createdAt: '2026-08-02T12:00:00Z',
    },
    {
      id: 'run_4088',
      title: 'Automated Code Review & Security Analysis',
      status: 'Completed',
      version: 7,
      stepsCount: 7,
      createdAt: '2026-08-02T09:30:00Z',
    },
    {
      id: 'run_4081',
      title: 'Vector Embedding Space Reindex',
      status: 'Waiting',
      version: 2,
      stepsCount: 3,
      createdAt: '2026-08-01T18:15:00Z',
    },
  ]);

  const getStatusBadge = (status: RunListItem['status']) => {
    switch (status) {
      case 'Completed': return { bg: 'var(--semantic-success)', fg: '#000' };
      case 'Waiting': return { bg: 'var(--semantic-warning)', fg: '#000' };
      case 'Failed': return { bg: 'var(--semantic-error)', fg: '#fff' };
      case 'Running': return { bg: 'var(--brand-cyan)', fg: '#000' };
      default: return { bg: 'var(--bg-level-1)', fg: 'var(--text-primary)' };
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Agent Runs & Executions</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            Filter and inspect all durable AgentRun instances, step transitions, and execution histories.
          </p>
        </div>
        <Button variant="primary">New Agent Run</Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {runs.map((r) => {
          const badge = getStatusBadge(r.status);
          return (
            <Surface key={r.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                  <span style={{ fontWeight: 700, fontSize: '16px', color: 'var(--brand-cyan)' }}>{r.title}</span>
                  <code style={{ fontSize: '12px', color: 'var(--brand-white)' }}>({r.id})</code>
                </div>
                <span
                  style={{
                    backgroundColor: badge.bg,
                    color: badge.fg,
                    padding: '2px 8px',
                    borderRadius: 'var(--radius-round)',
                    fontSize: '11px',
                    fontWeight: 700,
                    textTransform: 'uppercase',
                  }}
                >
                  {r.status}
                </span>
              </div>

              <div style={{ fontSize: '13px', color: 'var(--text-secondary)', display: 'flex', gap: 'var(--space-5)' }}>
                <span>Run Version: <strong>v{r.version}</strong></span>
                <span>Steps: <strong>{r.stepsCount} steps</strong></span>
                <span>Created: {r.createdAt}</span>
              </div>

              <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
                <Button variant="secondary">Open Task Workspace</Button>
                <Button variant="ghost">Replay Steps</Button>
              </div>
            </Surface>
          );
        })}
      </div>
    </div>
  );
};
