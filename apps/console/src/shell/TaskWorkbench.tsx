import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface TaskWorkbenchProps {
  title: string;
  status: 'Running' | 'Waiting' | 'Completed' | 'Failed' | 'Unknown';
  stepSummary: string;
}

export const TaskWorkbench: React.FC<TaskWorkbenchProps> = ({ title, status, stepSummary }) => {
  const getStatusColor = () => {
    switch (status) {
      case 'Completed':
        return 'var(--semantic-success)';
      case 'Waiting':
        return 'var(--semantic-warning)';
      case 'Failed':
        return 'var(--semantic-error)';
      case 'Running':
        return 'var(--brand-cyan)';
      default:
        return 'var(--brand-muted)';
    }
  };

  return (
    <Surface level={1} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2 style={{ margin: 0, fontSize: '20px' }}>{title}</h2>
        <span
          style={{
            backgroundColor: getStatusColor(),
            color: '#000',
            padding: 'var(--space-1) var(--space-3)',
            borderRadius: 'var(--radius-round)',
            fontSize: '12px',
            fontWeight: 700,
          }}
        >
          {status}
        </span>
      </div>
      <p style={{ margin: 0, color: 'var(--text-secondary)' }}>{stepSummary}</p>
      <div style={{ display: 'flex', gap: 'var(--space-3)' }}>
        <Button variant="primary">View Task Details</Button>
        <Button variant="ghost">Inspect Evidence</Button>
      </div>
    </Surface>
  );
};
