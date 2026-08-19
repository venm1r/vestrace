import React, { useMemo, useState } from 'react';
import { ModelItem, ProviderItem, vestraceClient } from '../sdk/client';
import { useApiResource } from '../sdk/useApiResource';
import { Modal } from '../design-system/primitives/Modal';
import { Button } from '../design-system/primitives/Button';
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

// The console is English-only, and the backend reports costs in USD. Formatting in
// the viewer's locale renders USD as "15,00 $", which reads as a different currency.
const NUMBER_LOCALE = 'en-US';

const costFormatter = new Intl.NumberFormat(NUMBER_LOCALE, {
  style: 'currency',
  currency: 'USD',
  minimumFractionDigits: 2,
  maximumFractionDigits: 4,
});

const countFormatter = new Intl.NumberFormat(NUMBER_LOCALE);

function formatCost(value: number): string {
  return Number.isFinite(value) ? `${costFormatter.format(value)} / Mtok` : '—';
}

export const ModelsPage: React.FC = () => {
  const { data: initialModels, error, loading, reload } = useApiResource(vestraceClient.listModels);
  const { data: initialProviders } = useApiResource(vestraceClient.listProviders);
  const [models, setModels] = useState<ModelItem[] | null>(null);
  const [providers, setProviders] = useState<ProviderItem[] | null>(null);
  const { notice, notify, dismiss } = useNotice();

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modelName, setModelName] = useState('');
  const [selectedProviderId, setSelectedProviderId] = useState('');
  const [newProviderName, setNewProviderName] = useState('');
  const [newProviderLocality, setNewProviderLocality] = useState<'remote' | 'local'>('remote');
  const [contextWindow, setContextWindow] = useState('128000');
  const [inputCost, setInputCost] = useState('0.15');
  const [outputCost, setOutputCost] = useState('0.60');
  const [submitting, setSubmitting] = useState(false);

  const providerList = providers ?? initialProviders ?? [];
  const items = models ?? initialModels ?? [];

  const providerNames = useMemo(
    () => new Map(providerList.map((provider) => [provider.id, provider.name])),
    [providerList],
  );

  const handleCreateModel = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!modelName.trim()) {
      notify('warning', 'Model name is required.');
      return;
    }

    setSubmitting(true);
    try {
      let providerId = selectedProviderId;
      if (!providerId || providerId === '__new__') {
        if (!newProviderName.trim()) {
          notify('warning', 'Provider name is required.');
          setSubmitting(false);
          return;
        }
        const createdProvider = await vestraceClient.createProvider({
          name: newProviderName.trim(),
          locality: newProviderLocality,
        });
        providerId = createdProvider.id;
        setProviders([...providerList, createdProvider]);
      }

      const newModel = await vestraceClient.createModel({
        provider_id: providerId,
        model_name: modelName.trim(),
        context_window: parseInt(contextWindow, 10) || 128000,
        input_cost_per_mtoken: parseFloat(inputCost) || 0,
        output_cost_per_mtoken: parseFloat(outputCost) || 0,
      });

      setModels([newModel, ...items]);
      setIsModalOpen(false);
      setModelName('');
      setNewProviderName('');
      notify('success', `Model "${newModel.model_name}" was registered.`);
    } catch (err: unknown) {
      const described = describeError(err, 'model registration');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <PageShell>
      <PageHeader
        title="Models & Providers Registry"
        description="Registered model profiles, their provider binding, context windows, and per-million-token pricing."
        actions={
          <ActionButton
            icon="model_training"
            onClick={() => {
              if (providerList.length > 0 && !selectedProviderId) {
                setSelectedProviderId(providerList[0].id);
              }
              setIsModalOpen(true);
            }}
          >
            Register Model
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title="Register Model"
      >
        <form onSubmit={handleCreateModel} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <label
              htmlFor="model-name"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              Model Name *
            </label>
            <input
              id="model-name"
              type="text"
              required
              value={modelName}
              onChange={(e) => setModelName(e.target.value)}
              placeholder="e.g. gpt-4o-mini or claude-3-5-sonnet"
              style={{
                width: '100%',
                padding: '10px 12px',
                borderRadius: 'var(--radius-md)',
                backgroundColor: 'var(--color-surface-container)',
                border: '1px solid var(--color-outline-variant)',
                color: 'var(--brand-white)',
                fontSize: '14px',
                fontFamily: 'inherit',
                boxSizing: 'border-box',
              }}
            />
          </div>

          <div>
            <label
              htmlFor="model-provider"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              Provider *
            </label>
            <select
              id="model-provider"
              value={selectedProviderId}
              onChange={(e) => setSelectedProviderId(e.target.value)}
              style={{
                width: '100%',
                padding: '10px 12px',
                borderRadius: 'var(--radius-md)',
                backgroundColor: 'var(--color-surface-container)',
                border: '1px solid var(--color-outline-variant)',
                color: 'var(--brand-white)',
                fontSize: '14px',
                fontFamily: 'inherit',
                boxSizing: 'border-box',
              }}
            >
              {providerList.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.locality})
                </option>
              ))}
              <option value="__new__">+ Add New Provider...</option>
            </select>
          </div>

          {(selectedProviderId === '__new__' || providerList.length === 0) && (
            <div
              style={{
                padding: '12px',
                borderRadius: 'var(--radius-md)',
                backgroundColor: 'var(--color-surface-container-lowest)',
                border: '1px dashed var(--color-outline-variant)',
                display: 'flex',
                flexDirection: 'column',
                gap: '12px',
              }}
            >
              <div>
                <label
                  htmlFor="new-provider-name"
                  style={{ display: 'block', fontSize: '12px', fontWeight: 600, color: 'var(--text-secondary)', marginBottom: '4px' }}
                >
                  New Provider Name *
                </label>
                <input
                  id="new-provider-name"
                  type="text"
                  value={newProviderName}
                  onChange={(e) => setNewProviderName(e.target.value)}
                  placeholder="e.g. OpenAI or Anthropic"
                  style={{
                    width: '100%',
                    padding: '8px 10px',
                    borderRadius: 'var(--radius-sm)',
                    backgroundColor: 'var(--color-surface-container)',
                    border: '1px solid var(--color-outline-variant)',
                    color: 'var(--brand-white)',
                    fontSize: '13px',
                    boxSizing: 'border-box',
                  }}
                />
              </div>

              <div>
                <label
                  htmlFor="new-provider-locality"
                  style={{ display: 'block', fontSize: '12px', fontWeight: 600, color: 'var(--text-secondary)', marginBottom: '4px' }}
                >
                  Locality
                </label>
                <select
                  id="new-provider-locality"
                  value={newProviderLocality}
                  onChange={(e) => setNewProviderLocality(e.target.value as 'remote' | 'local')}
                  style={{
                    width: '100%',
                    padding: '8px 10px',
                    borderRadius: 'var(--radius-sm)',
                    backgroundColor: 'var(--color-surface-container)',
                    border: '1px solid var(--color-outline-variant)',
                    color: 'var(--brand-white)',
                    fontSize: '13px',
                    boxSizing: 'border-box',
                  }}
                >
                  <option value="remote">Remote (Cloud API)</option>
                  <option value="local">Local (Self-hosted / On-prem)</option>
                </select>
              </div>
            </div>
          )}

          <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: '12px' }}>
            <div>
              <label
                htmlFor="model-context"
                style={{ display: 'block', fontSize: '12px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '4px' }}
              >
                Context Window
              </label>
              <input
                id="model-context"
                type="number"
                value={contextWindow}
                onChange={(e) => setContextWindow(e.target.value)}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface-container)',
                  border: '1px solid var(--color-outline-variant)',
                  color: 'var(--brand-white)',
                  fontSize: '13px',
                  fontFamily: 'var(--font-mono)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="model-input-cost"
                style={{ display: 'block', fontSize: '12px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '4px' }}
              >
                In ($/Mtok)
              </label>
              <input
                id="model-input-cost"
                type="number"
                step="0.01"
                value={inputCost}
                onChange={(e) => setInputCost(e.target.value)}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface-container)',
                  border: '1px solid var(--color-outline-variant)',
                  color: 'var(--brand-white)',
                  fontSize: '13px',
                  fontFamily: 'var(--font-mono)',
                  boxSizing: 'border-box',
                }}
              />
            </div>

            <div>
              <label
                htmlFor="model-output-cost"
                style={{ display: 'block', fontSize: '12px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '4px' }}
              >
                Out ($/Mtok)
              </label>
              <input
                id="model-output-cost"
                type="number"
                step="0.01"
                value={outputCost}
                onChange={(e) => setOutputCost(e.target.value)}
                style={{
                  width: '100%',
                  padding: '8px 10px',
                  borderRadius: 'var(--radius-sm)',
                  backgroundColor: 'var(--color-surface-container)',
                  border: '1px solid var(--color-outline-variant)',
                  color: 'var(--brand-white)',
                  fontSize: '13px',
                  fontFamily: 'var(--font-mono)',
                  boxSizing: 'border-box',
                }}
              />
            </div>
          </div>

          <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '12px', marginTop: '8px' }}>
            <Button
              variant="secondary"
              onClick={() => setIsModalOpen(false)}
              disabled={submitting}
            >
              Cancel
            </Button>
            <Button
              variant="primary"
              type="submit"
              disabled={submitting}
              icon="add"
            >
              {submitting ? 'Registering...' : 'Register'}
            </Button>
          </div>
        </form>
      </Modal>

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
                      {countFormatter.format(model.context_window)}
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
