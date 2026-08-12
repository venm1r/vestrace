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
  type = 'button',
  disabled,
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
      type={type}
      disabled={disabled}
      style={{
        backgroundColor: getBackgroundColor(),
        color: 'var(--brand-white)',
        border: '1px solid var(--border-color)',
        borderRadius: 'var(--radius-md)',
        padding: 'var(--space-2) var(--space-4)',
        fontSize: '14px',
        fontWeight: 600,
        cursor: disabled ? 'not-allowed' : 'pointer',
        opacity: disabled ? 0.6 : 1,
        display: 'inline-flex',
        alignItems: 'center',
        gap: 'var(--space-2)',
        whiteSpace: 'nowrap',
        ...style,
      }}
      {...props}
    >
      {children}
    </button>
  );
};
