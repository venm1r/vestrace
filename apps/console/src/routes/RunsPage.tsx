import React, { useEffect, useState } from 'react';
import { RunItem, RunStatus, vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import {
  ActionButton,
  NoticeBanner,
  PageHeader,
  PageShell,
  Panel,
  ResourceState,
  Th,
  describeError,
  rowStyle,
  tableHeadRowStyle,
  tableStyle,
  useNotice,
} from '../shell/PageState';

const STATUS_COLOR: Record<RunStatus, string> = {
  created: 'var(--text-secondary)',
  preparing: 'var(--color-tertiary)',
  running: 'var(--color-tertiary)',
  waiting_for_input: 'var(--color-warning)',
  waiting_for_approval: 'var(--color-warning)',
  waiting_for_dependency: 'var(--color-warning)',
  paused: 'var(--color-warning)',
  paused_policy_changed: 'var(--color-warning)',
  succeeded: 'var(--color-success)',
  succeeded_with_warnings: 'var(--color-warning)',
  partial: 'var(--color-warning)',
  failed: 'var(--color-error)',
  cancelled: 'var(--text-secondary)',
  expired: 'var(--text-secondary)',
};

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

export const RunsPage: React.FC = () => {
  const { data, error, loading, reload } = useApiResource(vestraceClient.listRuns);
  const [runs, setRuns] = useState<RunItem[]>([]);
  const [creating, setCreating] = useState(false);
  const { notice, notify, dismiss } = useNotice();

  useEffect(() => {
    setRuns(data ?? []);
  }, [data]);

  const createRun = async () => {
    if (creating) return;
    setCreating(true);
    try {
      const newRun = await vestraceClient.createRun({
        title: 'Manual Operator Execution Trigger',
      });
      setRuns((current) => [newRun, ...current]);
      notify('success', `Run record ${newRun.id} was persisted.`);
    } catch (reason: unknown) {
      const described = describeError(reason, 'run creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setCreating(false);
    }
  };

  return (
    <PageShell>
      <PageHeader
        title="Run Records"
        description="Persisted run records available in this build."
        actions={
          <ActionButton onClick={createRun} disabled={creating} icon="add">
            {creating ? 'Creating...' : 'Create Run'}
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Panel>
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={runs.length === 0}
          resourceName="run records"
          emptyMessage="No run records exist in this workspace."
          onRetry={reload}
        />
        {!loading && !error && runs.length > 0 && (
          <div className="table-scroll">
            <table style={tableStyle}>
              <caption style={{ position: 'absolute', left: '-9999px' }}>
                Persisted run records for the active workspace
              </caption>
              <thead>
                <tr style={tableHeadRowStyle}>
                  <Th>Title &amp; ID</Th>
                  <Th style={{ padding: '12px 16px' }}>Status</Th>
                  <Th style={{ padding: '12px 16px' }}>Version</Th>
                  <Th style={{ padding: '12px 16px' }}>Created</Th>
                  <Th>Updated</Th>
                </tr>
              </thead>
              <tbody>
                {runs.map((run) => (
                  <tr key={run.id} style={rowStyle}>
                    <td style={{ padding: '16px 24px' }}>
                      <div style={{ fontWeight: 600, color: 'var(--text-primary)' }}>{run.title}</div>
                      <div
                        style={{
                          fontFamily: 'var(--font-mono)',
                          fontSize: '12px',
                          color: 'var(--text-secondary)',
                          marginTop: '2px',
                        }}
                      >
                        {run.id}
                      </div>
                    </td>
                    <td style={{ padding: '16px' }}>
                      <span
                        style={{
                          padding: '4px 10px',
                          borderRadius: '4px',
                          fontSize: '12px',
                          fontWeight: 600,
                          textTransform: 'uppercase',
                          background: 'var(--color-surface-container-high)',
                          color: STATUS_COLOR[run.status] ?? 'var(--text-secondary)',
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {run.status.replace(/_/g, ' ')}
                      </span>
                    </td>
                    <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>{run.version}</td>
                    <td style={{ padding: '16px', whiteSpace: 'nowrap' }}>{formatTimestamp(run.created_at)}</td>
                    <td style={{ padding: '16px 24px', whiteSpace: 'nowrap' }}>{formatTimestamp(run.updated_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>
    </PageShell>
  );
};
