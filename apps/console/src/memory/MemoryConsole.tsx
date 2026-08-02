import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface MemoryConsoleProps {
  memoryId: string;
  kind: string;
  status: string;
  confidence: number;
  content: String;
  revisionNumber: number;
}

export const MemoryConsole: React.FC<MemoryConsoleProps> = ({
  memoryId,
  kind,
  status,
  confidence,
  content,
  revisionNumber,
}) => {
  return (
    <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <h3 style={{ margin: 0, fontSize: '16px', fontFamily: 'monospace' }}>Memory: {memoryId}</h3>
        <span style={{ fontSize: '12px', color: 'var(--brand-cyan)', fontWeight: 600 }}>
          {kind.toUpperCase()} | {status}
        </span>
      </div>

      <div style={{ padding: 'var(--space-2)', backgroundColor: 'var(--bg-level-0)', borderRadius: 'var(--radius-sm)' }}>
        <p style={{ margin: 0, fontSize: '14px', fontFamily: 'monospace' }}>{content}</p>
      </div>

      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', fontSize: '12px', color: 'var(--text-secondary)' }}>
        <span>Revision #{revisionNumber}</span>
        <span>Confidence: {(confidence * 100).toFixed(0)}%</span>
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)', marginTop: 'var(--space-2)' }}>
        <Button variant="secondary">View Provenance</Button>
        <Button variant="danger">Hard Purge</Button>
      </div>
    </Surface>
  );
};
