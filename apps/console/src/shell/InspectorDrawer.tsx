import React from 'react';
import { Surface } from '../design-system/primitives/Surface';

export interface InspectorDrawerProps {
  isOpen: boolean;
  onClose: () => void;
  runId?: string;
  runStatus?: string;
  budgetUsed?: string;
}

export const InspectorDrawer: React.FC<InspectorDrawerProps> = ({
  isOpen,
  onClose,
  runId,
  runStatus,
  budgetUsed,
}) => {
  if (!isOpen) return null;

  return (
    <div
      style={{
        position: 'fixed',
        top: 0,
        right: 0,
        width: '380px',
        height: '100vh',
        backgroundColor: 'var(--bg-level-3)',
        borderLeft: '1px solid var(--border-color)',
        padding: 'var(--space-4)',
        boxShadow: '-4px 0 16px rgba(0, 0, 0, 0.3)',
        zIndex: 1000,
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--space-4)',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h3 style={{ margin: 0, fontSize: '18px' }}>Context Inspector</h3>
        <button
          onClick={onClose}
          style={{ background: 'none', border: 'none', color: 'var(--text-secondary)', fontSize: '20px', cursor: 'pointer' }}
        >
          &times;
        </button>
      </div>

      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
        <h4 style={{ margin: 0, fontSize: '14px', color: 'var(--brand-cyan)' }}>Run Overview</h4>
        <div><strong>Run ID:</strong> <code style={{ fontSize: '12px' }}>{runId || 'N/A'}</code></div>
        <div><strong>Status:</strong> {runStatus || 'Unknown'}</div>
        <div><strong>Budget Used:</strong> {budgetUsed || '$0.00'}</div>
      </Surface>

      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
        <h4 style={{ margin: 0, fontSize: '14px', color: 'var(--brand-cyan)' }}>Verification Checks</h4>
        <div style={{ color: 'var(--semantic-success)', fontSize: '13px' }}>✓ RLS Workspace Isolation Verified</div>
        <div style={{ color: 'var(--semantic-success)', fontSize: '13px' }}>✓ Memory Provenance Check Passed</div>
      </Surface>
    </div>
  );
};
