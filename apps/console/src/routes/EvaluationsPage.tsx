import React from 'react';
import { vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  useNotice,
} from '../shell/PageState';

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

function statusColor(status: string | null): string {
  switch (status?.toLowerCase()) {
    case 'passed':
    case 'succeeded':
    case 'completed':
      return 'var(--color-success)';
    case 'failed':
      return 'var(--color-error)';
    case 'running':
    case 'pending':
      return 'var(--color-warning)';
    default:
      return 'var(--text-secondary)';
  }
}

export const EvaluationsPage: React.FC = () => {
  const { data: evaluations, error, loading, reload } = useApiResource(vestraceClient.listEvaluations);
  const { notice, notify, dismiss } = useNotice();

  const items = evaluations ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Evaluations"
        description="Recorded evaluation runs, their reported status, and the score persisted for each one."
        actions={
          <ActionButton
            icon="analytics"
            onClick={() =>
              notify('info', 'Triggering evaluation suites from the console is not implemented in this build.')
            }
          >
            Run Eval Suite
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      {(loading || error || items.length === 0) && (
        <Panel>
          <ResourceState
            loading={loading}
            error={error}
            isEmpty={items.length === 0}
            resourceName="evaluations"
            emptyMessage="No evaluations are recorded in this workspace."
            onRetry={reload}
          />
        </Panel>
      )}

      {items.length > 0 && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '12px' }}>
          {items.map((evaluation) => (
            <div
              key={evaluation.id}
              style={{
                background: 'var(--color-surface-container-low)',
                border: '1px solid var(--color-outline)',
                borderRadius: '8px',
                padding: '16px 24px',
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: '16px',
                flexWrap: 'wrap',
              }}
            >
              <div style={{ display: 'flex', alignItems: 'center', gap: '16px', minWidth: 0 }}>
                <span
                  className="material-symbols-outlined"
                  aria-hidden="true"
                  style={{ fontSize: '28px', color: statusColor(evaluation.status) }}
                >
                  fact_check
                </span>
                <div style={{ minWidth: 0 }}>
                  <h2
                    style={{
                      fontFamily: 'var(--font-display)',
                      fontSize: '16px',
                      fontWeight: 600,
                      color: 'var(--text-primary)',
                      margin: 0,
                    }}
                  >
                    {evaluation.name}
                  </h2>
                  <div style={{ fontSize: '13px', color: 'var(--text-secondary)', marginTop: '2px' }}>
                    Recorded: {formatTimestamp(evaluation.created_at)}
                  </div>
                  {evaluation.summary && (
                    <div style={{ fontSize: '13px', color: 'var(--text-secondary)', marginTop: '4px' }}>
                      {evaluation.summary}
                    </div>
                  )}
                </div>
              </div>

              <div style={{ display: 'flex', alignItems: 'center', gap: '20px' }}>
                <div style={{ textAlign: 'right' }}>
                  <div
                    style={{
                      fontFamily: 'var(--font-display)',
                      fontSize: '20px',
                      fontWeight: 700,
                      color: evaluation.score === null ? 'var(--text-secondary)' : 'var(--text-primary)',
                    }}
                  >
                    {evaluation.score === null ? '—' : evaluation.score}
                  </div>
                  <span style={{ fontSize: '11px', color: 'var(--text-secondary)', textTransform: 'uppercase' }}>
                    Score
                  </span>
                </div>

                <span
                  style={{
                    padding: '2px 8px',
                    borderRadius: '4px',
                    fontSize: '12px',
                    fontWeight: 600,
                    textTransform: 'uppercase',
                    background: 'var(--color-surface-container-high)',
                    color: statusColor(evaluation.status),
                    whiteSpace: 'nowrap',
                  }}
                >
                  {evaluation.status ?? 'unknown'}
                </span>
              </div>
            </div>
          ))}
        </div>
      )}
    </PageShell>
  );
};
