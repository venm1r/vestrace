import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface TaskWorkbenchProps {
  title: string;
  status: 'Running' | 'Waiting' | 'Completed' | 'Failed' | 'Unknown';
  stepSummary: string;
}

export const TaskWorkbench: React.FC<TaskWorkbenchProps> = ({ title, status, stepSummary }) => {
  const [detailsOpen, setDetailsOpen] = React.useState(false);
  const [evidenceOpen, setEvidenceOpen] = React.useState(false);

  const getStatusColor = () => {
    switch (status) {
      case 'Completed': return 'var(--semantic-success)';
      case 'Waiting': return 'var(--semantic-warning)';
      case 'Failed': return 'var(--semantic-error)';
      case 'Running': return 'var(--brand-cyan)';
      default: return 'var(--brand-muted)';
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

      {/* Dynamic Task Details */}
      {detailsOpen && (
        <div style={{ padding: 'var(--space-3)', backgroundColor: 'var(--bg-level-2)', borderRadius: 'var(--radius-md)', fontSize: '13px', display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
          <div><strong>Run ID:</strong> <code style={{ color: 'var(--brand-cyan)' }}>run_7f81a4b9</code></div>
          <div><strong>Execution Mode:</strong> Direct Step Pipeline</div>
          <div><strong>Step 1:</strong> Pre-flight database check (Completed)</div>
          <div><strong>Step 2:</strong> Validating Row-Level Security Isolation Policies (In Progress)</div>
        </div>
      )}

      {/* Dynamic Evidence Findings */}
      {evidenceOpen && (
        <div style={{ padding: 'var(--space-3)', backgroundColor: 'var(--bg-level-2)', borderRadius: 'var(--radius-md)', fontSize: '13px', display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
          <div style={{ color: 'var(--semantic-success)' }}>✓ RLS session variable vestrace.workspace_id set correctly</div>
          <div style={{ color: 'var(--semantic-success)' }}>✓ Audit log entry aud_90f81a2c generated</div>
          <div style={{ color: 'var(--brand-cyan)' }}>ℹ 3 SQL migrations validated with zero warnings</div>
        </div>
      )}

      <div style={{ display: 'flex', gap: 'var(--space-3)' }}>
        <Button variant="primary" onClick={() => setDetailsOpen(!detailsOpen)}>
          {detailsOpen ? 'Hide Task Details' : 'View Task Details'}
        </Button>
        <Button variant="ghost" onClick={() => setEvidenceOpen(!evidenceOpen)}>
          {evidenceOpen ? 'Hide Evidence' : 'Inspect Evidence'}
        </Button>
      </div>
    </Surface>
  );
};
