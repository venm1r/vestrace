import React, { useState } from 'react';
import { GovernedModelItem, vestraceClient } from '../sdk/client';
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
export const ModelsPage: React.FC = () => {
  const { data: initialModels, error, loading, reload } = useApiResource(vestraceClient.listModels);
  const { data: connections } = useApiResource(vestraceClient.listConnections);
  const { data: defaultModel, reload: reloadDefault } = useApiResource(() =>
    vestraceClient.getWorkspaceModelDefault('chat'),
  );
  const [models, setModels] = useState<GovernedModelItem[] | null>(null);
  const { notice, notify, dismiss } = useNotice();

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [modelName, setModelName] = useState('');
  const [wireModelId, setWireModelId] = useState('');
  const [selectedConnectionId, setSelectedConnectionId] = useState('');
  const [contextWindow, setContextWindow] = useState('128000');
  const [inputCost, setInputCost] = useState('0.15');
  const [outputCost, setOutputCost] = useState('0.60');
  const [submitting, setSubmitting] = useState(false);
  const [settingDefaultId, setSettingDefaultId] = useState<string | null>(null);

  const connectionList = connections ?? [];
  const items = models ?? initialModels ?? [];

  const handleCreateModel = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!modelName.trim() || !wireModelId.trim()) {
      notify('warning', 'Model name and wire model id are required.');
      return;
    }
    if (!selectedConnectionId) {
      notify('warning', 'Select a Connection this Model runs against.');
      return;
    }
    const connection = connectionList.find((c) => c.id === selectedConnectionId);
    if (!connection || !connection.revision_id) {
      notify('warning', 'The selected Connection has no published revision yet.');
      return;
    }

    setSubmitting(true);
    try {
      const modelId = crypto.randomUUID();
      await vestraceClient.createModelRevision(
        modelId,
        {
          model_id: modelId,
          provider_id: crypto.randomUUID(),
          model_name: modelName.trim(),
          context_window: parseInt(contextWindow, 10) || 128000,
          input_cost_per_mtoken: parseFloat(inputCost) || 0,
          output_cost_per_mtoken: parseFloat(outputCost) || 0,
          revision_id: crypto.randomUUID(),
          connection_id: connection.id,
          connection_revision_id: connection.revision_id,
          wire_model_id: wireModelId.trim(),
          kind: 'chat',
          execution_guard_id: crypto.randomUUID(),
          expected_head_version: 0,
        },
        { requestId: crypto.randomUUID() },
      );

      setModels(null);
      reload();
      setIsModalOpen(false);
      setModelName('');
      setWireModelId('');
      notify('success', `Model "${modelName.trim()}" was published.`);
    } catch (err: unknown) {
      const described = describeError(err, 'model publication');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  const handleSetDefault = async (modelId: string) => {
    setSettingDefaultId(modelId);
    try {
      await vestraceClient.setWorkspaceModelDefault(
        modelId,
        {
          default_id: crypto.randomUUID(),
          model_id: modelId,
          purpose: 'chat',
          required_capabilities: ['chat.completions'],
          expected_version: defaultModel?.version ?? 0,
        },
        { requestId: crypto.randomUUID() },
      );
      reloadDefault();
      notify('success', `Model ${modelId} is now the workspace's chat default.`);
    } catch (err: unknown) {
      const described = describeError(err, 'setting the workspace default');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSettingDefaultId(null);
    }
  };

  return (
    <PageShell>
      <PageHeader
        title="Models & Providers Registry"
        description="Registered models as the governed API serves them: revision, state, qualification and any blockers."
        actions={
          <ActionButton
            icon="model_training"
            onClick={() => setIsModalOpen(true)}
          >
            Register Model
          </ActionButton>
        }
      />

      <Panel>
        <div style={{ fontSize: '13px', color: 'var(--text-secondary)' }}>
          Current chat default:{' '}
          <strong style={{ color: 'var(--text-primary)', fontFamily: 'var(--font-mono)' }}>
            {defaultModel?.model_id ?? 'none configured'}
          </strong>
        </div>
      </Panel>

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
            <label htmlFor="model-wire-id" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Wire Model Id *
            </label>
            <input
              id="model-wire-id"
              type="text"
              required
              value={wireModelId}
              onChange={(e) => setWireModelId(e.target.value)}
              placeholder="e.g. ternary-bonsai-27b"
              className="field-control"
              style={{ width: '100%', boxSizing: 'border-box', fontFamily: 'var(--font-mono)' }}
            />
          </div>

          <div>
            <label htmlFor="model-connection" style={{ display: 'block', fontSize: '13px', fontWeight: 600, marginBottom: '6px' }}>
              Connection *
            </label>
            <select
              id="model-connection"
              value={selectedConnectionId}
              onChange={(e) => setSelectedConnectionId(e.target.value)}
              className="field-control"
              style={{ width: '100%' }}
            >
              <option value="">Select a Connection...</option>
              {connectionList.map((c) => (
                <option key={c.id} value={c.id} disabled={!c.revision_id}>
                  {c.id} ({c.qualification_state ?? 'unqualified'})
                </option>
              ))}
            </select>
          </div>

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
                  <Th>Model</Th>
                  <Th style={{ padding: '12px 16px' }}>Revision</Th>
                  <Th style={{ padding: '12px 16px' }}>State</Th>
                  <Th style={{ padding: '12px 16px' }}>Qualification</Th>
                  <Th style={{ padding: '12px 16px' }}>Blockers</Th>
                  <Th style={{ padding: '12px 16px' }}>Default</Th>
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
                        <span className="material-symbols-outlined" style={{ fontSize: '18px' }}>
                          model_training
                        </span>
                        <span style={{ fontFamily: 'var(--font-mono)', fontSize: '12px' }}>
                          {model.id}
                        </span>
                      </div>
                    </td>
                    <td style={{ padding: '16px', fontFamily: 'var(--font-mono)', fontSize: '12px' }}>
                      {model.revision_id ?? (
                        <span style={{ color: 'var(--text-secondary)' }}>none</span>
                      )}
                    </td>
                    <td style={{ padding: '16px' }}>{model.state}</td>
                    <td style={{ padding: '16px' }}>
                      {model.qualification_state ?? (
                        <span style={{ color: 'var(--text-secondary)' }}>unqualified</span>
                      )}
                    </td>
                    <td style={{ padding: '16px' }}>
                      {model.blockers.length === 0 ? (
                        <span style={{ color: 'var(--text-secondary)' }}>none</span>
                      ) : (
                        model.blockers.join(', ')
                      )}
                    </td>
                    <td style={{ padding: '16px' }}>
                      {defaultModel?.model_id === model.id ? (
                        <span style={{ color: 'var(--color-success)' }}>chat default</span>
                      ) : (
                        <ActionButton
                          variant="quiet"
                          style={{ padding: '4px 10px', fontSize: '12px' }}
                          disabled={settingDefaultId === model.id}
                          onClick={() => handleSetDefault(model.id)}
                        >
                          {settingDefaultId === model.id ? 'Setting...' : 'Set as default'}
                        </ActionButton>
                      )}
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
