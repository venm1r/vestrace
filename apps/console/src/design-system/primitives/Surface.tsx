import React from 'react';

export interface SurfaceProps {
  level?: 1 | 2 | 3 | 4;
  children: React.ReactNode;
  style?: React.CSSProperties;
}

export const Surface: React.FC<SurfaceProps> = ({ level = 1, children, style }) => {
  return (
    <div
      style={{
        backgroundColor: `var(--bg-level-${level})`,
        border: '1px solid var(--border-color)',
        borderRadius: 'var(--radius-lg)',
        padding: 'var(--space-4)',
        boxShadow: level > 1 ? '0 4px 12px rgba(0, 0, 0, 0.15)' : 'none',
        ...style,
      }}
    >
      {children}
    </div>
  );
};
