import React from 'react';
import { Surface } from '../design-system/primitives/Surface';

export interface SimpleListViewProps {
  title: string;
  description: string;
}

export const SimpleListView: React.FC<SimpleListViewProps> = ({ title, description }) => {
  return (
    <Surface level={1} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--brand-cyan)' }}>{title}</h2>
      <p style={{ margin: 0, color: 'var(--text-secondary)' }}>{description}</p>
      <div
        style={{
          padding: 'var(--space-6)',
          backgroundColor: 'var(--bg-level-2)',
          borderRadius: 'var(--radius-md)',
          border: '1px dashed var(--border-color)',
          textAlign: 'center',
          color: 'var(--brand-muted)',
          fontSize: '14px',
        }}
      >
        No items found in active workspace scope.
      </div>
    </Surface>
  );
};
