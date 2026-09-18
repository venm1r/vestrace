import React from 'react';

interface ErrorBoundaryProps {
  children: React.ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * Without a boundary a render failure in any route unmounts the whole tree and
 * leaves a blank page with no indication of what happened.
 */
export class ErrorBoundary extends React.Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo): void {
    console.error('[vestrace-console] render failure', error, info.componentStack);
  }

  private readonly reset = () => this.setState({ error: null });

  render(): React.ReactNode {
    const { error } = this.state;
    if (!error) return this.props.children;

    return (
      <div
        role="alert"
        style={{
          margin: '48px auto',
          maxWidth: '640px',
          background: 'var(--color-surface-container-low)',
          border: '1px solid var(--color-error)',
          borderRadius: '12px',
          padding: '24px',
        }}
      >
        <h1
          style={{
            fontFamily: 'var(--font-display)',
            fontSize: '20px',
            fontWeight: 700,
            color: 'var(--text-primary)',
            marginTop: 0,
          }}
        >
          The console could not render this view
        </h1>
        <p style={{ color: 'var(--text-secondary)', fontSize: '14px' }}>
          The failure was logged to the browser console. Reloading the view is safe; no run state is
          held in the browser.
        </p>
        <pre
          style={{
            background: 'var(--color-surface-container-lowest)',
            border: '1px solid var(--color-outline)',
            borderRadius: '8px',
            padding: '12px',
            fontFamily: 'var(--font-mono)',
            fontSize: '12px',
            color: 'var(--color-error)',
            overflowX: 'auto',
          }}
        >
          {error.message}
        </pre>
        <button
          type="button"
          onClick={this.reset}
          style={{
            background: 'var(--color-primary)',
            color: '#ffffff',
            border: 'none',
            borderRadius: '8px',
            padding: '10px 20px',
            fontSize: '14px',
            fontWeight: 600,
            cursor: 'pointer',
          }}
        >
          Try again
        </button>
      </div>
    );
  }
}
