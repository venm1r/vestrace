import React, { useCallback, useState } from 'react';
import { ApiRequestError } from '../sdk/client';

export type Tone = 'info' | 'warning' | 'error' | 'success';

const TONE_COLOR: Record<Tone, string> = {
  info: 'var(--color-tertiary)',
  warning: 'var(--color-warning)',
  error: 'var(--color-error)',
  success: 'var(--color-success)',
};

const TONE_ICON: Record<Tone, string> = {
  info: 'info',
  warning: 'warning',
  error: 'error',
  success: 'check_circle',
};

export interface PageHeaderProps {
  title: string;
  description: string;
  actions?: React.ReactNode;
}

export const PageHeader: React.FC<PageHeaderProps> = ({ title, description, actions }) => (
  <div
    style={{
      display: 'flex',
      justifyContent: 'space-between',
      alignItems: 'flex-start',
      gap: '16px',
      flexWrap: 'wrap',
    }}
  >
    <div style={{ minWidth: '260px', flex: 1 }}>
      <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '24px', fontWeight: 700, color: 'var(--text-primary)', margin: 0 }}>
        {title}
      </h1>
      <p style={{ fontSize: '14px', color: 'var(--text-secondary)', marginTop: '4px', marginBottom: 0 }}>
        {description}
      </p>
    </div>
    {actions}
  </div>
);

export interface ActionButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: string;
  variant?: 'primary' | 'quiet' | 'danger';
}

const ACTION_STYLES: Record<NonNullable<ActionButtonProps['variant']>, React.CSSProperties> = {
  primary: { background: 'var(--color-primary)', color: '#ffffff', border: '1px solid transparent' },
  quiet: {
    background: 'var(--color-surface-container-high)',
    color: 'var(--text-primary)',
    border: '1px solid var(--color-outline)',
  },
  danger: { background: 'transparent', color: 'var(--color-error)', border: '1px solid var(--color-error)' },
};

export const ActionButton: React.FC<ActionButtonProps> = ({
  icon,
  variant = 'primary',
  children,
  style,
  disabled,
  ...props
}) => (
  <button
    type="button"
    disabled={disabled}
    style={{
      ...ACTION_STYLES[variant],
      borderRadius: '8px',
      padding: '10px 18px',
      fontSize: '14px',
      fontWeight: 600,
      cursor: disabled ? 'not-allowed' : 'pointer',
      opacity: disabled ? 0.45 : 1,
      display: 'inline-flex',
      alignItems: 'center',
      gap: '8px',
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

export const Panel: React.FC<{ children: React.ReactNode; style?: React.CSSProperties }> = ({
  children,
  style,
}) => (
  <div
    style={{
      background: 'var(--color-surface-container-low)',
      border: '1px solid var(--color-outline)',
      borderRadius: '12px',
      overflow: 'hidden',
      ...style,
    }}
  >
    {children}
  </div>
);

export interface StatusMessageProps {
  tone: Tone;
  title: string;
  detail?: string;
  action?: React.ReactNode;
  role?: 'alert' | 'status';
}

export const StatusMessage: React.FC<StatusMessageProps> = ({
  tone,
  title,
  detail,
  action,
  role = 'status',
}) => (
  <div
    role={role}
    style={{
      display: 'flex',
      alignItems: 'flex-start',
      gap: '12px',
      padding: '20px 24px',
      color: 'var(--text-secondary)',
    }}
  >
    <span className="material-symbols-outlined" aria-hidden="true" style={{ color: TONE_COLOR[tone] }}>
      {TONE_ICON[tone]}
    </span>
    <div style={{ flex: 1, minWidth: 0 }}>
      <div style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{title}</div>
      {detail && (
        <div style={{ fontSize: '13px', marginTop: '4px', wordBreak: 'break-word' }}>{detail}</div>
      )}
      {action && <div style={{ marginTop: '12px' }}>{action}</div>}
    </div>
  </div>
);

export interface DescribedError {
  tone: Tone;
  title: string;
  detail: string;
  retryable: boolean;
}

/**
 * Distinguishes a deliberately unimplemented P0 surface, a rejected identity and
 * a genuine backend failure, which the console previously reported identically.
 */
export function describeError(error: unknown, resourceName: string): DescribedError {
  if (error instanceof ApiRequestError) {
    if (error.isNotImplemented) {
      return {
        tone: 'info',
        title: `${resourceName} is not available in the P0 foundation`,
        detail: error.body.message,
        retryable: false,
      };
    }
    if (error.isIdentityRejected) {
      return {
        tone: 'warning',
        title: 'The backend rejected the request identity',
        detail: `${error.body.message}. Set VITE_VESTRACE_WORKSPACE_ID and VITE_VESTRACE_PRINCIPAL_ID to a provisioned workspace/principal pair and rebuild the console.`,
        retryable: false,
      };
    }
    if (error.status === 0) {
      return {
        tone: 'error',
        title: 'The Vestrace API could not be reached',
        detail: error.body.message,
        retryable: true,
      };
    }
    if (error.status === 403) {
      return {
        tone: 'warning',
        title: 'The policy engine denied this request',
        detail: error.body.message,
        retryable: false,
      };
    }
    return {
      tone: 'error',
      title: `Loading ${resourceName} failed (HTTP ${error.status})`,
      detail: error.body.message,
      retryable: true,
    };
  }

  return {
    tone: 'error',
    title: `Loading ${resourceName} failed`,
    detail: error instanceof Error ? error.message : String(error ?? 'unknown error'),
    retryable: true,
  };
}

export interface ResourceStateProps {
  loading: boolean;
  error: unknown;
  isEmpty: boolean;
  resourceName: string;
  emptyMessage: string;
  onRetry?: () => void;
}

/**
 * Renders the loading / error / empty state for a resource, or `null` when the
 * caller should render the data itself. The page chrome always stays mounted.
 */
export const ResourceState: React.FC<ResourceStateProps> = ({
  loading,
  error,
  isEmpty,
  resourceName,
  emptyMessage,
  onRetry,
}) => {
  if (loading) {
    return <StatusMessage tone="info" title={`Loading ${resourceName}...`} />;
  }

  if (error) {
    const described = describeError(error, resourceName);
    return (
      <StatusMessage
        tone={described.tone}
        role={described.tone === 'error' ? 'alert' : 'status'}
        title={described.title}
        detail={described.detail}
        action={
          described.retryable && onRetry ? (
            <ActionButton variant="quiet" icon="refresh" onClick={onRetry}>
              Retry
            </ActionButton>
          ) : undefined
        }
      />
    );
  }

  if (isEmpty) {
    return <StatusMessage tone="info" title={emptyMessage} />;
  }

  return null;
};

export interface Notice {
  tone: Tone;
  message: string;
}

/**
 * Non-blocking replacement for `window.alert`, which froze the UI thread and
 * could not be styled or read by assistive technology in context.
 */
export function useNotice() {
  const [notice, setNotice] = useState<Notice | null>(null);
  const notify = useCallback((tone: Tone, message: string) => setNotice({ tone, message }), []);
  const dismiss = useCallback(() => setNotice(null), []);
  return { notice, notify, dismiss };
}

export const NoticeBanner: React.FC<{ notice: Notice | null; onDismiss: () => void }> = ({
  notice,
  onDismiss,
}) => {
  if (!notice) return null;

  return (
    <div
      role="status"
      aria-live="polite"
      style={{
        display: 'flex',
        alignItems: 'flex-start',
        gap: '12px',
        padding: '12px 16px',
        borderRadius: '8px',
        border: `1px solid ${TONE_COLOR[notice.tone]}`,
        background: 'var(--color-surface-container)',
        color: 'var(--text-primary)',
        fontSize: '14px',
      }}
    >
      <span className="material-symbols-outlined" aria-hidden="true" style={{ color: TONE_COLOR[notice.tone] }}>
        {TONE_ICON[notice.tone]}
      </span>
      <div style={{ flex: 1, minWidth: 0 }}>{notice.message}</div>
      <button
        type="button"
        onClick={onDismiss}
        aria-label="Dismiss notification"
        style={{
          background: 'transparent',
          border: 'none',
          color: 'var(--text-secondary)',
          cursor: 'pointer',
          lineHeight: 1,
        }}
      >
        <span className="material-symbols-outlined" aria-hidden="true">
          close
        </span>
      </button>
    </div>
  );
};

export const PageShell: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <div style={{ display: 'flex', flexDirection: 'column', gap: '24px' }}>{children}</div>
);

export const Th: React.FC<{ children: React.ReactNode; style?: React.CSSProperties }> = ({
  children,
  style,
}) => (
  <th scope="col" style={{ padding: '12px 24px', fontWeight: 600, ...style }}>
    {children}
  </th>
);

export const tableHeadRowStyle: React.CSSProperties = {
  background: 'var(--color-surface-container)',
  borderBottom: '1px solid var(--color-outline)',
  color: 'var(--text-secondary)',
  fontSize: '12px',
  textTransform: 'uppercase',
};

export const tableStyle: React.CSSProperties = {
  width: '100%',
  borderCollapse: 'collapse',
  textAlign: 'left',
  fontSize: '14px',
};

export const rowStyle: React.CSSProperties = {
  borderBottom: '1px solid var(--color-surface-container-high)',
};
