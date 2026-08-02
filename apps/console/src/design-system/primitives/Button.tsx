import React from 'react';
import '../tokens/theme.css';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
  children: React.ReactNode;
}

export const Button: React.FC<ButtonProps> = ({
  variant = 'primary',
  children,
  style,
  ...props
}) => {
  const getBackgroundColor = () => {
    switch (variant) {
      case 'primary':
        return 'var(--brand-blue)';
      case 'secondary':
        return 'var(--bg-level-2)';
      case 'ghost':
        return 'transparent';
      case 'danger':
        return 'var(--semantic-error)';
    }
  };

  return (
    <button
      style={{
        backgroundColor: getBackgroundColor(),
        color: 'var(--brand-white)',
        border: '1px solid var(--border-color)',
        borderRadius: 'var(--radius-md)',
        padding: 'var(--space-2) var(--space-4)',
        fontSize: '14px',
        fontWeight: 600,
        cursor: 'pointer',
        display: 'inline-flex',
        alignItems: 'center',
        gap: 'var(--space-2)',
        ...style,
      }}
      {...props}
    >
      {children}
    </button>
  );
};
