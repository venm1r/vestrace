import React, { useEffect, useRef, useState } from 'react';
import { useNavigate, useParams } from 'react-router-dom';
import type { RunItem } from '../sdk/client';
import { vestraceClient } from '../sdk/client';
import { RunInspector } from '../components/RunInspector';
import { RunsRail } from '../components/RunsRail';
import { RunWorkspace } from '../components/RunWorkspace';
import { useApiResource } from '../sdk/useApiResource';
import { filterRuns, resolveSelectedRun, type RunFilter } from './runWorkspaceModel';
import { NoticeBanner, ResourceState, describeError, useNotice } from '../shell/PageState';

export const RunsPage: React.FC = () => {
  const { runId } = useParams();
  const navigate = useNavigate();
  const { data, error, loading, reload } = useApiResource(vestraceClient.listRuns);
  const [runs, setRuns] = useState<RunItem[]>([]);
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState<RunFilter>('all');
  const [creating, setCreating] = useState(false);
  const locallyCreatedRuns = useRef(new Map<string, RunItem>());
  const { notice, notify, dismiss } = useNotice();

  useEffect(() => {
    if (data === null) return;

    setRuns(() => {
      const returnedIds = new Set(data.map(({ id }) => id));
      for (const id of returnedIds) locallyCreatedRuns.current.delete(id);
      const pendingCreates = [...locallyCreatedRuns.current.values()];
      return [...pendingCreates, ...data.filter(({ id }) => !locallyCreatedRuns.current.has(id))];
    });
  }, [data]);

  const visibleRuns = filterRuns(runs, query, filter);
  const selectedRun = resolveSelectedRun(runs, runId);

  const selectRun = (run: RunItem) => {
    navigate(`/runs/${encodeURIComponent(run.id)}`);
  };

  const createRun = async () => {
    if (creating) return;

    setCreating(true);
    try {
      const newRun = await vestraceClient.createRun({
        title: 'Manual Operator Execution Trigger',
      });
      locallyCreatedRuns.current.set(newRun.id, newRun);
      setRuns((current) => [newRun, ...current.filter(({ id }) => id !== newRun.id)]);
      navigate(`/runs/${encodeURIComponent(newRun.id)}`);
      reload();
      notify('success', `Run record ${newRun.id} was persisted.`);
    } catch (reason: unknown) {
      const described = describeError(reason, 'run creation');
      notify('error', `${described.title}: ${described.detail}`);
    } finally {
      setCreating(false);
    }
  };

  return (
    <div className="runs-workspace">
      <RunsRail
        runs={runs}
        visibleRuns={visibleRuns}
        query={query}
        filter={filter}
        selectedRunId={selectedRun?.id}
        loading={loading}
        creating={creating}
        onQueryChange={setQuery}
        onFilterChange={setFilter}
        onSelectRun={selectRun}
        onCreateRun={createRun}
        onReload={reload}
      />

      <section className="run-center" aria-label="Selected run workspace">
        <NoticeBanner notice={notice} onDismiss={dismiss} />
        <ResourceState
          loading={loading}
          error={error}
          isEmpty={runs.length === 0}
          resourceName="run records"
          emptyMessage="No run records exist in this workspace."
          onRetry={reload}
        />
        {!loading && !error && selectedRun && <RunWorkspace run={selectedRun} />}
      </section>

      {!loading && !error && selectedRun && <RunInspector run={selectedRun} />}
    </div>
  );
};
