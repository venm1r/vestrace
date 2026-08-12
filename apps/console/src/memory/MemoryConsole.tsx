import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface MemoryConsoleProps {
  memoryId: string;
  kind: string;
  status: string;
  /** Confidence in the 0..1 range. */
  confidence: number;
  content: string;
  revisionNumber: number;
  onViewProvenance?: (memoryId: string) => void;
  onHardPurge?: (memoryId: string) => void;
}

function formatConfidence(confidence: number): string {
  if (!Number.isFinite(confidence)) return 'unknown';
  const clamped = Math.min(Math.max(confidence, 0), 1);
  return `${(clamped * 100).toFixed(0)}%`;
}

export const MemoryConsole: React.FC<MemoryConsoleProps> = ({
  memoryId,
  kind,
  status,
  confidence,
  content,
  revisionNumber,
  onViewProvenance,
  onHardPurge,
}) => {
  return (
    <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: '12px' }}>
        <h3 style={{ margin: 0, fontSize: '16px', fontFamily: 'var(--font-mono)', wordBreak: 'break-all' }}>
          Memory: {memoryId}
        </h3>
        <span style={{ fontSize: '12px', color: 'var(--brand-cyan)', fontWeight: 600, whiteSpace: 'nowrap' }}>
          {kind.toUpperCase()} | {status}
        </span>
      </div>

      <div
        style={{
          padding: 'var(--space-2)',
          backgroundColor: 'var(--bg-level-0)',
          borderRadius: 'var(--radius-sm)',
        }}
      >
        <p
          style={{
            margin: 0,
            fontSize: '14px',
            fontFamily: 'var(--font-mono)',
            whiteSpace: 'pre-wrap',
            wordBreak: 'break-word',
          }}
        >
          {content}
        </p>
      </div>

      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          fontSize: '12px',
          color: 'var(--text-secondary)',
        }}
      >
        <span>Revision #{revisionNumber}</span>
        <span>Confidence: {formatConfidence(confidence)}</span>
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
        <Button
          variant="secondary"
          disabled={!onViewProvenance}
          onClick={() => onViewProvenance?.(memoryId)}
        >
          View Provenance
        </Button>
        <Button variant="danger" disabled={!onHardPurge} onClick={() => onHardPurge?.(memoryId)}>
          Hard Purge
        </Button>
      </div>
    </Surface>
  );
};
