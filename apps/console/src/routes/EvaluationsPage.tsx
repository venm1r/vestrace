import React from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';

export interface EvaluationItem {
  id: string;
  runId: string;
  qualityScore: number;
  latencyMs: number;
  evaluator: string;
  status: 'passed' | 'failed' | 'warnings';
  createdAt: string;
}

export const EvaluationsPage: React.FC = () => {
  const [evaluations] = React.useState<EvaluationItem[]>([
    {
      id: 'eval_77b01f92',
      runId: 'run_4092',
      qualityScore: 0.98,
      latencyMs: 340,
      evaluator: 'Automated Integrity Evaluator v1',
      status: 'passed',
      createdAt: '2026-08-02T11:45:00Z',
    },
    {
      id: 'eval_99c20a11',
      runId: 'run_4088',
      qualityScore: 0.82,
      latencyMs: 1250,
      evaluator: 'Context Pack Token Efficiency Checker',
      status: 'warnings',
      createdAt: '2026-08-02T10:12:00Z',
    },
  ]);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-4)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
        <div>
          <h2 style={{ margin: 0, fontSize: '20px', color: 'var(--text-primary)' }}>Evaluations & Benchmark Metrics</h2>
          <p style={{ margin: 'var(--space-1) 0 0 0', fontSize: '14px', color: 'var(--text-secondary)' }}>
            Execution quality scores, latency metrics, OpenTelemetry span analysis, and verification checks.
          </p>
        </div>
        <Button variant="primary" onClick={() => alert('Starting automated Evaluation & Quality Benchmark Suite...')}>
          Run Evaluation Suite
        </Button>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
        {evaluations.map((ev) => (
          <Surface key={ev.id} level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
                <span style={{ fontFamily: 'monospace', fontWeight: 700, color: 'var(--brand-cyan)' }}>Run #{ev.runId}</span>
                <span style={{ fontSize: '12px', color: 'var(--text-secondary)' }}>({ev.evaluator})</span>
              </div>
              <span
                style={{
                  padding: '2px 8px',
                  borderRadius: 'var(--radius-round)',
                  fontSize: '11px',
                  fontWeight: 700,
                  textTransform: 'uppercase',
                  backgroundColor: ev.status === 'passed' ? 'var(--semantic-success)' : 'var(--semantic-warning)',
                  color: '#000',
                }}
              >
                {ev.status.toUpperCase()}
              </span>
            </div>

            <div style={{ fontSize: '13px', color: 'var(--text-secondary)', display: 'flex', gap: 'var(--space-5)' }}>
              <span>Quality Score: <strong style={{ color: 'var(--brand-white)' }}>{(ev.qualityScore * 100).toFixed(0)}%</strong></span>
              <span>Latency: <strong>{ev.latencyMs} ms</strong></span>
            </div>

            <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
              <Button variant="secondary" onClick={() => alert(`Loading full evaluation report for ${ev.id}...`)}>
                View Full Report
              </Button>
              <Button variant="ghost" onClick={() => alert(`Opening OpenTelemetry trace inspector for run ${ev.runId}...`)}>
                Inspect Spans
              </Button>
            </div>
          </Surface>
        ))}
      </div>
    </div>
  );
};
