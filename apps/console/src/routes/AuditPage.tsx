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
  Th,
  rowStyle,
  tableHeadRowStyle,
  tableStyle,
  useNotice,
} from '../shell/PageState';

function formatTimestamp(value: string): string {
  const parsed = new Date(value);
  return Number.isNaN(parsed.getTime()) ? value : parsed.toLocaleString();
}

export const AuditPage: React.FC = () => {
  const { data: events, error, loading, reload } = useApiResource(vestraceClient.listAuditEvents);
  const { notice, notify, dismiss } = useNotice();

  const items = events ?? [];

  const handleExport = () => {
    if (items.length === 0) return;
    const blob = new Blob([JSON.stringify(items, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `vestrace-audit-trail-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
    notify('success', `Exported ${items.length} audit events.`);
  };

  return (
    <PageShell>
      <PageHeader
        title="Audit Log & Trace Registry"
        description="Immutable security event stream, capability authorization logs, and RLS enforcement history."
        actions={
          <ActionButton
            icon="download"
            disabled={items.length === 0}
            onClick={handleExport}
          >
            Export Audit Trail
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Panel>
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={items.length === 0}
          resourceName="audit events"
          emptyMessage="No audit events are recorded for this workspace."
          onRetry={reload}
        />
        {!loading && !error && items.length > 0 && (
          <div className="table-scroll">
            <table style={tableStyle}>
              <thead>
                <tr style={tableHeadRowStyle}>
                  <Th>Timestamp</Th>
                  <Th style={{ padding: '12px 16px' }}>Actor</Th>
                  <Th style={{ padding: '12px 16px' }}>Action</Th>
                  <Th>Target Resource</Th>
                </tr>
              </thead>
              <tbody>
                {items.map((event) => (
                  <tr key={event.id} style={rowStyle}>
                    <td
                      style={{
                        padding: '16px 24px',
                        fontFamily: 'var(--font-mono)',
                        fontSize: '13px',
                        color: 'var(--text-secondary)',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {formatTimestamp(event.timestamp)}
                    </td>
                    <td style={{ padding: '16px', fontWeight: 600, color: 'var(--text-primary)' }}>{event.actor}</td>
                    <td style={{ padding: '16px' }}>
                      <span
                        style={{
                          padding: '4px 8px',
                          background: 'var(--color-surface-container-high)',
                          borderRadius: '4px',
                          fontSize: '12px',
                          fontFamily: 'var(--font-mono)',
                          color: 'var(--color-tertiary)',
                        }}
                      >
                        {event.action}
                      </span>
                    </td>
                    <td style={{ padding: '16px 24px', fontFamily: 'var(--font-mono)', fontSize: '13px' }}>
                      {event.resource}
                    </td>
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
