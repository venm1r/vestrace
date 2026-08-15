import React from 'react';
import '../tokens/theme.css';

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger';

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  /** Material Symbols ligature rendered before the label. */
  icon?: string;
  children: React.ReactNode;
}

/** Colour pairs, always taken together.
 *
 * `primary` is `#b5c8e4` — a light tone — so its text must be `on-primary`
 * (`#1f3147`), not white. Pairing a light surface with white text is the single
 * easiest way to produce unreadable buttons, which is why background and
 * foreground are defined here as one unit rather than chosen separately.
 */
const VARIANTS: Record<ButtonVariant, React.CSSProperties> = {
  primary: {
    backgroundColor: 'var(--color-primary)',
    color: 'var(--color-on-primary)',
    border: '1px solid var(--color-primary-fixed)',
  },
  secondary: {
    backgroundColor: 'var(--color-surface-container-high)',
    color: 'var(--color-on-surface)',
    border: '1px solid var(--color-outline-variant)',
  },
  ghost: {
    backgroundColor: 'transparent',
    color: 'var(--color-on-surface-variant)',
    border: '1px solid var(--color-outline-variant)',
  },
  // Outline until hovered, as the spec requires of destructive actions: a
  // filled red button invites the click it is warning about.
  danger: {
    backgroundColor: 'transparent',
    color: 'var(--color-error)',
    border: '1px solid var(--color-error)',
  },
};

export const Button: React.FC<ButtonProps> = ({
  variant = 'primary',
  icon,
  children,
  style,
  type = 'button',
  disabled,
  ...props
}) => (
  <button
    type={type}
    disabled={disabled}
    style={{
      ...VARIANTS[variant],
      // 8px: the spec's default radius for buttons, inputs and cards.
      borderRadius: 'var(--radius-md)',
      padding: 'var(--space-sm) var(--space-md)',
      // Buttons carry a label, which is monospace in this system.
      fontFamily: 'var(--font-mono)',
      fontSize: 'var(--text-label-size)',
      lineHeight: 'var(--text-label-line)',
      fontWeight: 500,
      letterSpacing: '0.05em',
      cursor: disabled ? 'not-allowed' : 'pointer',
      opacity: disabled ? 0.45 : 1,
      display: 'inline-flex',
      alignItems: 'center',
      justifyContent: 'center',
      gap: 'var(--space-sm)',
      whiteSpace: 'nowrap',
      minHeight: '36px',
      transition: 'background-color 150ms ease, color 150ms ease',
      ...style,
    }}
    {...props}
  >
    {icon && (
      <span className="material-symbols-outlined" aria-hidden="true" style={{ fontSize: '18px' }}>
        {icon}
      </span>
    )}
    {children}
  </button>
);
