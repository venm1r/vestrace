import React, { useState } from 'react';
import { EvaluationItem, vestraceClient } from '../sdk/client';
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
  describeError,
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
  const { data: initialEvaluations, error, loading, reload } = useApiResource(vestraceClient.listEvaluations);
  const { data: models } = useApiResource(vestraceClient.listModels);
  const [evaluations, setEvaluations] = useState<EvaluationItem[] | null>(null);
  const { notice, notify, dismiss } = useNotice();

  const [isModalOpen, setIsModalOpen] = useState(false);
  const [name, setName] = useState('');
  const [selectedModelId, setSelectedModelId] = useState('');
  const [summary, setSummary] = useState('');
  const [submitting, setSubmitting] = useState(false);

  const items = evaluations ?? initialEvaluations ?? [];
  const modelList = models ?? [];

  const handleRunEvaluation = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      notify('warning', 'Evaluation suite name is required.');
      return;
    }
    setSubmitting(true);
    try {
      const res = await vestraceClient.createEvaluation({
        name: name.trim(),
        model_id: selectedModelId || undefined,
        summary: summary.trim() || undefined,
      });
      const newEval: EvaluationItem = {
        id: res.evaluation_id,
        name: name.trim(),
        model_id: selectedModelId || null,
        status: 'pending',
        score: null,
        summary: summary.trim() || null,
        created_at: new Date().toISOString(),
      };
      setEvaluations([newEval, ...items]);
      setIsModalOpen(false);
      setName('');
      setSummary('');
      notify('success', `Evaluation suite "${newEval.name}" was scheduled.`);
    } catch (err: unknown) {
      const described = describeError(err, 'evaluation creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <PageShell>
      <PageHeader
        title="Evaluations"
        description="Recorded evaluation runs, their reported status, and the score persisted for each one."
        actions={
          <ActionButton
            icon="analytics"
            onClick={() => setIsModalOpen(true)}
          >
            Run Eval Suite
          </ActionButton>
        }
      />

      <NoticeBanner notice={notice} onDismiss={dismiss} />

      <Modal
        isOpen={isModalOpen}
        onClose={() => setIsModalOpen(false)}
        title="Run Evaluation Suite"
      >
        <form onSubmit={handleRunEvaluation} style={{ display: 'flex', flexDirection: 'column', gap: '16px' }}>
          <div>
            <label
              htmlFor="eval-name"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              Suite Name *
            </label>
            <input
              id="eval-name"
              type="text"
              required
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="e.g. Cognitive Consistency Benchmark"
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

          {modelList.length > 0 && (
            <div>
              <label
                htmlFor="eval-model"
                style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
              >
                Target Model
              </label>
              <select
                id="eval-model"
                value={selectedModelId}
                onChange={(e) => setSelectedModelId(e.target.value)}
                style={{
                  width: '100%',
                  padding: '10px 12px',
                  borderRadius: 'var(--radius-md)',
                  backgroundColor: 'var(--color-surface-container)',
                  border: '1px solid var(--color-outline-variant)',
                  color: 'var(--brand-white)',
                  fontSize: '14px',
                  boxSizing: 'border-box',
                }}
              >
                <option value="">(No specific model bound)</option>
                {modelList.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.model_name}
                  </option>
                ))}
              </select>
            </div>
          )}

          <div>
            <label
              htmlFor="eval-summary"
              style={{ display: 'block', fontSize: '13px', fontWeight: 600, color: 'var(--text-primary)', marginBottom: '6px' }}
            >
              Summary / Notes
            </label>
            <input
              id="eval-summary"
              type="text"
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
              placeholder="e.g. Evaluates deterministic step transitions and provenance records"
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
              icon="play_arrow"
            >
              {submitting ? 'Scheduling...' : 'Run Suite'}
            </Button>
          </div>
        </form>
      </Modal>

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
