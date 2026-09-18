import React from 'react';
import '../tokens/theme.css';

export interface SurfaceProps {
  /** Tonal layer. The spec builds hierarchy from tone and thin borders, not
   *  from shadow, so a higher level is a lighter surface rather than a
   *  heavier drop shadow. */
  level?: 1 | 2 | 3 | 4;
  /** Removes the padding, for containers whose children manage their own —
   *  a table that must reach the container's edges, for instance. */
  flush?: boolean;
  as?: 'div' | 'section' | 'article' | 'aside';
  children: React.ReactNode;
  style?: React.CSSProperties;
}

const LEVEL_BACKGROUND: Record<1 | 2 | 3 | 4, string> = {
  1: 'var(--color-surface-container-low)',
  2: 'var(--color-surface-container)',
  3: 'var(--color-surface-container-high)',
  4: 'var(--color-surface-container-highest)',
};

export const Surface: React.FC<SurfaceProps> = ({
  level = 1,
  flush = false,
  as: Tag = 'div',
  children,
  style,
}) => (
  <Tag
    style={{
      backgroundColor: LEVEL_BACKGROUND[level],
      // Low-contrast borders define containers in dark mode; `outline` is
      // reserved for elements that need to stand out against them.
      border: '1px solid var(--color-outline-variant)',
      borderRadius: 'var(--radius-lg)',
      padding: flush ? 0 : 'var(--space-md)',
      // Only layer 4 — popovers and drawers — earns a shadow.
      boxShadow: level === 4 ? '0 8px 24px rgb(0 0 0 / 35%)' : 'none',
      overflow: 'hidden',
      ...style,
    }}
  >
    {children}
  </Tag>
);
