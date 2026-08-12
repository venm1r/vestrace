import React, { useMemo } from 'react';
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

const costFormatter = new Intl.NumberFormat(undefined, {
  style: 'currency',
  currency: 'USD',
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

function formatCost(value: number): string {
  return Number.isFinite(value) ? `${costFormatter.format(value)} / Mtok` : '—';
}

export const ModelsPage: React.FC = () => {
  const { data: models, error, loading, reload } = useApiResource(vestraceClient.listModels);
  const { data: providers } = useApiResource(vestraceClient.listProviders);
  const { notice, notify, dismiss } = useNotice();

  const providerNames = useMemo(
    () => new Map((providers ?? []).map((provider) => [provider.id, provider.name])),
    [providers],
  );

  const items = models ?? [];

  return (
    <PageShell>
      <PageHeader
        title="Models & Providers Registry"
        description="Registered model profiles, their provider binding, context windows, and per-million-token pricing."
        actions={
          <ActionButton
            icon="extension"
            onClick={() =>
              notify('info', 'Model registration from the console is not implemented in the P0 foundation.')
            }
          >
            Register Model
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Panel>
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={items.length === 0}
          resourceName="models"
          emptyMessage="No models are registered in this workspace."
          onRetry={reload}
        />
        {!loading && !error && items.length > 0 && (
          <div className="table-scroll">
            <table style={tableStyle}>
              <thead>
                <tr style={tableHeadRowStyle}>
                  <Th>Model Name</Th>
                  <Th style={{ padding: '12px 16px' }}>Provider</Th>
                  <Th style={{ padding: '12px 16px' }}>Context Window</Th>
                  <Th style={{ padding: '12px 16px' }}>Input Cost</Th>
                  <Th style={{ padding: '12px 16px' }}>Output Cost</Th>
                </tr>
              </thead>
              <tbody>
                {items.map((model) => (
                  <tr key={model.id} style={rowStyle}>
                    <td style={{ padding: '16px 24px' }}>
                      <div
                        style={{
                          fontWeight: 600,
                          color: 'var(--text-primary)',
                          display: 'flex',
                          alignItems: 'center',
                          gap: '8px',
                        }}
                      >
                        <span
                          className="material-symbols-outlined"
                          aria-hidden="true"
                          style={{ color: 'var(--color-tertiary)' }}
                        >
                          extension
                        </span>
                        {model.model_name}
                      </div>
                    </td>
                    <td style={{ padding: '16px' }}>
                      {providerNames.get(model.provider_id) ?? (
                        <span style={{ fontFamily: 'var(--font-mono)', fontSize: '12px', color: 'var(--text-secondary)' }}>
                          {model.provider_id}
                        </span>
                      )}
                    </td>
                    <td style={{ padding: '16px', fontFamily: 'var(--font-mono)' }}>
                      {model.context_window.toLocaleString()}
                    </td>
                    <td style={{ padding: '16px', fontFamily: 'var(--font-mono)', whiteSpace: 'nowrap' }}>
                      {formatCost(model.input_cost_per_mtoken)}
                    </td>
                    <td style={{ padding: '16px', fontFamily: 'var(--font-mono)', whiteSpace: 'nowrap' }}>
                      {formatCost(model.output_cost_per_mtoken)}
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
