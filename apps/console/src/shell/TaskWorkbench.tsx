import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export type TaskStatus = 'Running' | 'Waiting' | 'Completed' | 'Failed' | 'Unknown';

export interface TaskDetail {
  label: string;
  value: string;
}

export interface TaskEvidence {
  tone: 'pass' | 'info' | 'fail';
  text: string;
}

export interface TaskWorkbenchProps {
  title: string;
  status: TaskStatus;
  stepSummary: string;
  /** Details supplied by the caller; the workbench never invents run data. */
  details?: TaskDetail[];
  /** Verification findings supplied by the caller. */
  evidence?: TaskEvidence[];
}

const STATUS_COLOR: Record<TaskStatus, string> = {
  Completed: 'var(--semantic-success)',
  Waiting: 'var(--semantic-warning)',
  Failed: 'var(--semantic-error)',
  Running: 'var(--brand-cyan)',
  Unknown: 'var(--brand-muted)',
};

const EVIDENCE_COLOR: Record<TaskEvidence['tone'], string> = {
  pass: 'var(--semantic-success)',
  info: 'var(--brand-cyan)',
  fail: 'var(--semantic-error)',
};

const panelStyle: React.CSSProperties = {
  padding: 'var(--space-3)',
  backgroundColor: 'var(--bg-level-2)',
  borderRadius: 'var(--radius-md)',
  fontSize: '13px',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-2)',
};

export const TaskWorkbench: React.FC<TaskWorkbenchProps> = ({
  title,
  status,
  stepSummary,
  details = [],
  evidence = [],
}) => {
  const [detailsOpen, setDetailsOpen] = React.useState(false);
  const [evidenceOpen, setEvidenceOpen] = React.useState(false);

  return (
    <Surface level={1} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '12px' }}>
        <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>{title}</h2>
        <span
          style={{
            backgroundColor: STATUS_COLOR[status],
            color: '#04121f',
            padding: 'var(--space-1) var(--space-3)',
            borderRadius: 'var(--radius-round)',
            fontSize: '12px',
            fontWeight: 700,
            whiteSpace: 'nowrap',
          }}
        >
          {status}
        </span>
      </div>
      <p style={{ margin: 0, color: 'var(--text-secondary)' }}>{stepSummary}</p>

      {detailsOpen && (
        <div style={panelStyle}>
          {details.length === 0 ? (
            <span style={{ color: 'var(--text-secondary)' }}>No task details were provided.</span>
          ) : (
            details.map((detail) => (
              <div key={detail.label}>
                <strong>{detail.label}:</strong> {detail.value}
              </div>
            ))
          )}
        </div>
      )}

      {evidenceOpen && (
        <div style={panelStyle}>
          {evidence.length === 0 ? (
            <span style={{ color: 'var(--text-secondary)' }}>No verification evidence was provided.</span>
          ) : (
            evidence.map((item) => (
              <div key={item.text} style={{ color: EVIDENCE_COLOR[item.tone] }}>
                {item.text}
              </div>
            ))
          )}
        </div>
      )}

      <div style={{ display: 'flex', gap: 'var(--space-3)', flexWrap: 'wrap' }}>
        <Button
          variant="primary"
          aria-expanded={detailsOpen}
          onClick={() => setDetailsOpen((open) => !open)}
        >
          {detailsOpen ? 'Hide Task Details' : 'View Task Details'}
        </Button>
        <Button
          variant="ghost"
          aria-expanded={evidenceOpen}
          onClick={() => setEvidenceOpen((open) => !open)}
        >
          {evidenceOpen ? 'Hide Evidence' : 'Inspect Evidence'}
        </Button>
      </div>
    </Surface>
  );
};
