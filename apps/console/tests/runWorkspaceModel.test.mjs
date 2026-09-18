import assert from 'node:assert/strict';
import test from 'node:test';

import {
  filterRuns,
  getRunGroup,
  getRunStatusPresentation,
  groupRuns,
  resolveSelectedRun,
} from '../src/routes/runWorkspaceModel.ts';

const runs = [
  {
    id: 'run-alpha',
    title: 'Alpha deployment',
    status: 'running',
    version: 3,
    created_at: '2026-08-25T08:00:00Z',
    updated_at: '2026-08-25T08:10:00Z',
  },
  {
    id: 'beta-42',
    title: 'Beta verification',
    status: 'waiting_for_approval',
    version: 1,
    created_at: '2026-08-25T09:00:00Z',
    updated_at: '2026-08-25T09:05:00Z',
  },
  {
    id: 'run-final',
    title: 'Final report',
    status: 'succeeded',
    version: 2,
    created_at: '2026-08-25T10:00:00Z',
    updated_at: '2026-08-25T10:15:00Z',
  },
  {
    id: 'run-future',
    title: 'Future state',
    status: 'future_status',
    version: 1,
    created_at: '2026-08-25T11:00:00Z',
    updated_at: '2026-08-25T11:05:00Z',
  },
];

test('filters by title or id without case sensitivity', () => {
  assert.deepEqual(filterRuns(runs, 'ALPHA', 'all').map(({ id }) => id), ['run-alpha']);
  assert.deepEqual(filterRuns(runs, 'BETA-42', 'all').map(({ id }) => id), ['beta-42']);
});

test('filters the selected group without sorting and trims the query', () => {
  assert.deepEqual(
    filterRuns(runs, '  ', 'waiting').map(({ id }) => id),
    ['beta-42'],
  );
  assert.deepEqual(
    filterRuns(runs, 'future', 'active').map(({ id }) => id),
    [],
  );
});

test('groups known and unknown statuses without dropping server order', () => {
  assert.deepEqual(
    Object.fromEntries(
      Object.entries(groupRuns(runs)).map(([group, groupedRuns]) => [
        group,
        groupedRuns.map(({ id }) => id),
      ]),
    ),
    {
      active: ['run-alpha'],
      waiting: ['beta-42'],
      finished: ['run-final'],
      other: ['run-future'],
    },
  );
});

test('resolves URL selection without inventing a run', () => {
  assert.equal(resolveSelectedRun(runs, 'beta-42')?.id, 'beta-42');
  assert.equal(resolveSelectedRun(runs, 'missing')?.id, 'run-alpha');
  assert.equal(resolveSelectedRun([], 'missing'), null);
});

test('places every declared status and an unknown value deterministically', () => {
  const expected = {
    created: 'active',
    preparing: 'active',
    running: 'active',
    waiting_for_input: 'waiting',
    waiting_for_approval: 'waiting',
    waiting_for_dependency: 'waiting',
    paused: 'waiting',
    paused_policy_changed: 'waiting',
    succeeded: 'finished',
    succeeded_with_warnings: 'finished',
    partial: 'finished',
    failed: 'finished',
    cancelled: 'finished',
    expired: 'finished',
  };

  for (const [status, group] of Object.entries(expected)) {
    assert.equal(getRunGroup(status), group, status);
  }

  assert.equal(getRunGroup('future_status'), 'other');
  assert.deepEqual(getRunStatusPresentation('future_status'), {
    label: 'Unknown',
    tone: 'neutral',
  });
});

test('presents every declared status with its truthful semantic tone', () => {
  const expected = {
    created: 'neutral',
    preparing: 'active',
    running: 'active',
    waiting_for_input: 'warning',
    waiting_for_approval: 'warning',
    waiting_for_dependency: 'warning',
    paused: 'warning',
    paused_policy_changed: 'warning',
    succeeded: 'success',
    succeeded_with_warnings: 'warning',
    partial: 'warning',
    failed: 'danger',
    cancelled: 'neutral',
    expired: 'neutral',
  };

  for (const [status, tone] of Object.entries(expected)) {
    assert.equal(getRunStatusPresentation(status).tone, tone, status);
  }
  assert.equal(getRunStatusPresentation('future_status').tone, 'neutral');
});
