import React, { useCallback, useState } from 'react';
import { ApiRequestError } from '../sdk/client';
import { Button, type ButtonVariant } from '../design-system/primitives/Button';
import { Surface } from '../design-system/primitives/Surface';

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
      gap: 'var(--space-md)',
      flexWrap: 'wrap',
    }}
  >
    <div style={{ minWidth: '240px', flex: 1 }}>
      <h1 className="type-h2" style={{ color: 'var(--brand-white)', margin: 0 }}>
        {title}
      </h1>
      <p
        className="type-body-sm"
        style={{ color: 'var(--color-on-surface-variant)', marginTop: 'var(--space-xs)', marginBottom: 0 }}
      >
        {description}
      </p>
    </div>
    {/* Actions wrap under the title rather than squeezing it on narrow
        screens; `control-row` keeps them on the 8px grid when they do. */}
    {actions && <div className="control-row">{actions}</div>}
  </div>
);

export interface ActionButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: string;
  /** `quiet` is retained as an alias for the primitive's `secondary`, so the
   *  existing call sites keep reading naturally. */
  variant?: 'primary' | 'quiet' | 'danger';
}

const VARIANT_ALIAS: Record<NonNullable<ActionButtonProps['variant']>, ButtonVariant> = {
  primary: 'primary',
  quiet: 'secondary',
  danger: 'danger',
};

/** The page-level action button.
 *
 * A thin alias over the design-system `Button` rather than a second
 * implementation. Until now the console carried two button components with
 * different radii, padding and typography, and the twelve route pages used the
 * one that was *not* part of the design system.
 */
export const ActionButton: React.FC<ActionButtonProps> = ({
  variant = 'primary',
  children,
  ...props
}) => (
  <Button variant={VARIANT_ALIAS[variant]} {...props}>
    {children}
  </Button>
);

/** A page-level container. An alias over `Surface`, for the same reason as
 *  `ActionButton`: one vocabulary, not two. Flush because panels here wrap
 *  tables and lists that manage their own padding. */
export const Panel: React.FC<{ children: React.ReactNode; style?: React.CSSProperties }> = ({
  children,
  style,
}) => (
  <Surface level={1} flush style={style}>
    {children}
  </Surface>
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
 * Distinguishes a reserved-but-unimplemented surface, a rejected identity and
 * a genuine backend failure, which the console previously reported identically.
 */
export function describeError(error: unknown, resourceName: string): DescribedError {
  if (error instanceof ApiRequestError) {
    if (error.isNotImplemented) {
      return {
        tone: 'info',
        // Phrased without the resource name so it reads correctly for both
        // "artifacts" and "the metrics summary".
        title: 'Not available in this build',
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
  <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-lg)' }}>{children}</div>
);

export const Th: React.FC<{ children: React.ReactNode; style?: React.CSSProperties }> = ({
  children,
  style,
}) => (
  <th
    scope="col"
    style={{ padding: 'var(--space-sm) var(--space-lg)', fontWeight: 500, ...style }}
  >
    {children}
  </th>
);

/** Column headers are "system" text, so they take the monospace label step
 *  rather than a shrunken body size. */
export const tableHeadRowStyle: React.CSSProperties = {
  background: 'var(--color-surface-container)',
  borderBottom: '1px solid var(--color-outline-variant)',
  color: 'var(--color-on-surface-variant)',
  fontFamily: 'var(--font-mono)',
  fontSize: 'var(--text-label-size)',
  lineHeight: 'var(--text-label-line)',
  letterSpacing: '0.05em',
  textTransform: 'uppercase',
};

export const tableStyle: React.CSSProperties = {
  width: '100%',
  borderCollapse: 'collapse',
  textAlign: 'left',
  fontSize: 'var(--text-body-sm-size)',
  lineHeight: 'var(--text-body-sm-line)',
};

export const rowStyle: React.CSSProperties = {
  borderBottom: '1px solid var(--color-outline-variant)',
};
