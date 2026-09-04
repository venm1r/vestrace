import React from 'react';
import type { RunItem } from '../sdk/client';
import {
  getRunStatusPresentation,
  type RunFilter,
} from '../routes/runWorkspaceModel';

interface RunsRailProps {
  runs: readonly RunItem[];
  visibleRuns: readonly RunItem[];
  query: string;
  filter: RunFilter;
  selectedRunId?: string;
  loading: boolean;
  creating: boolean;
  onQueryChange: (query: string) => void;
  onFilterChange: (filter: RunFilter) => void;
  onSelectRun: (run: RunItem) => void;
  onCreateRun: () => void;
  onReload: () => void;
}

function shortenRunId(id: string): string {
  return id.length <= 18 ? id : `${id.slice(0, 9)}…${id.slice(-6)}`;
}

function formatTimestamp(value: string): string {
  const timestamp = new Date(value);
  return Number.isNaN(timestamp.getTime()) ? value : timestamp.toLocaleString();
}

export const RunsRail: React.FC<RunsRailProps> = ({
  runs,
  visibleRuns,
  query,
  filter,
  selectedRunId,
  loading,
  creating,
  onQueryChange,
  onFilterChange,
  onSelectRun,
  onCreateRun,
  onReload,
}) => (
  <aside className="runs-rail" aria-label="Runs">
    <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: '8px', padding: '12px' }}>
      <div>
        <h1 className="type-h2" style={{ margin: 0 }}>
          Runs
        </h1>
        <div className="type-body-sm" style={{ color: 'var(--text-secondary)', marginTop: '2px' }}>
          {runs.length} total · {visibleRuns.length} visible
        </div>
      </div>
      <button type="button" className="button-primary" onClick={onCreateRun} disabled={creating}>
        {creating ? 'Creating…' : 'Create run'}
      </button>
    </div>

    <div style={{ display: 'grid', gap: '8px', padding: '0 12px 12px' }}>
      <label className="type-label" htmlFor="runs-search">
        Search runs
      </label>
      <input
        id="runs-search"
        className="field-control"
        value={query}
        onChange={(event) => onQueryChange(event.target.value)}
        placeholder="Title or ID"
        type="search"
      />
      <label className="type-label" htmlFor="runs-status-filter">
        Status filter
      </label>
      <div className="control-row">
        <select
          id="runs-status-filter"
          className="field-control"
          value={filter}
          onChange={(event) => onFilterChange(event.target.value as RunFilter)}
          style={{ flex: 1 }}
        >
          <option value="all">All returned statuses</option>
          <option value="active">Active</option>
          <option value="waiting">Waiting</option>
          <option value="finished">Finished</option>
        </select>
        <button type="button" className="button-ghost" onClick={onReload} disabled={loading}>
          Reload
        </button>
      </div>
    </div>

    <div aria-live="polite" style={{ display: 'grid', gap: '4px', padding: '0 8px 12px' }}>
      {!loading && visibleRuns.length === 0 && runs.length > 0 && (
        <p className="type-body-sm" style={{ color: 'var(--text-secondary)', margin: '8px 4px' }}>
          No returned runs match this filter.
        </p>
      )}
      {visibleRuns.map((run) => {
        const status = getRunStatusPresentation(run.status);
        const isSelected = run.id === selectedRunId;

        return (
          <button
            key={run.id}
            type="button"
            onClick={() => onSelectRun(run)}
            aria-pressed={isSelected}
            style={{
              display: 'grid',
              gap: '6px',
              width: '100%',
              padding: '10px',
              border: `1px solid ${isSelected ? 'var(--brand-blue)' : 'var(--color-outline-variant)'}`,
              borderRadius: 'var(--radius-sm)',
              background: isSelected ? 'var(--color-primary-container)' : 'var(--color-surface-container-low)',
              color: 'var(--text-primary)',
              cursor: 'pointer',
              textAlign: 'left',
            }}
          >
            <span style={{ overflow: 'hidden', fontSize: '13px', fontWeight: 600, textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
              {run.title}
            </span>
            <span className="type-code" style={{ color: 'var(--text-secondary)' }} title={run.id}>
              {shortenRunId(run.id)}
            </span>
            <span className="run-rail-meta">
              <span className={`status-chip status-chip--${status.tone}`} title={status.label}>{status.label}</span>
              <time className="type-body-sm run-rail-timestamp" dateTime={run.updated_at} title={formatTimestamp(run.updated_at)}>
                {formatTimestamp(run.updated_at)}
              </time>
            </span>
          </button>
        );
      })}
    </div>
  </aside>
);
