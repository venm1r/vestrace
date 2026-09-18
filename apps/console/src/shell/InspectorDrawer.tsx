import React, { useEffect } from 'react';
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
  useEffect(() => {
    if (!isOpen) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, [isOpen, onClose]);

  if (!isOpen) return null;

  return (
    <div
      role="dialog"
      aria-modal="false"
      aria-label="Context inspector"
      style={{
        position: 'fixed',
        top: 0,
        right: 0,
        width: 'min(380px, 100vw)',
        height: '100%',
        backgroundColor: 'var(--bg-level-3)',
        borderLeft: '1px solid var(--border-color)',
        padding: 'var(--space-4)',
        boxShadow: '-4px 0 16px rgba(0, 0, 0, 0.3)',
        zIndex: 1000,
        display: 'flex',
        flexDirection: 'column',
        gap: 'var(--space-4)',
        overflowY: 'auto',
      }}
    >
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h2 style={{ margin: 0, fontSize: '18px', color: 'var(--text-primary)' }}>Context Inspector</h2>
        <button
          type="button"
          onClick={onClose}
          aria-label="Close context inspector"
          style={{
            background: 'none',
            border: 'none',
            color: 'var(--text-secondary)',
            fontSize: '20px',
            cursor: 'pointer',
            lineHeight: 1,
          }}
        >
          &times;
        </button>
      </div>

      <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
        <h3 style={{ margin: 0, fontSize: '14px', color: 'var(--brand-cyan)' }}>Run Overview</h3>
        <div>
          <strong>Run ID:</strong>{' '}
          <code style={{ fontSize: '12px', wordBreak: 'break-all' }}>{runId ?? 'not selected'}</code>
        </div>
        <div>
          <strong>Status:</strong> {runStatus ?? 'unknown'}
        </div>
        <div>
          <strong>Budget used:</strong> {budgetUsed ?? 'not reported'}
        </div>
      </Surface>
    </div>
  );
};
