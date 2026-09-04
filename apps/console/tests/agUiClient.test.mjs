import assert from 'node:assert/strict';
import test from 'node:test';

test('scopes the event stream to the created AG-UI run', async () => {
  const { buildAgUiEventStreamUrl } = await import('../src/sdk/agUiClient.ts');

  assert.equal(typeof buildAgUiEventStreamUrl, 'function');
  assert.equal(
    buildAgUiEventStreamUrl('run/with space'),
    '/api/ag-ui/events/stream?run_id=run%2Fwith%20space',
  );
});

test('rejects a resolved request continuation after its lifecycle is invalidated', async () => {
  const { createAgUiRequestGate } = await import('../src/sdk/agUiClient.ts');

  assert.equal(typeof createAgUiRequestGate, 'function');

  const gate = createAgUiRequestGate();
  const request = gate.begin();
  let resolveResult;
  const controlledResult = new Promise((resolve) => {
    resolveResult = resolve;
  });
  let streamConnections = 0;

  const continuation = controlledResult.then(() =>
    gate.admit(request, () => {
      streamConnections += 1;
    }),
  );

  gate.invalidate();
  resolveResult();
  assert.equal(await continuation, false);
  assert.equal(streamConnections, 0);
});

test('uses the AG-UI event_type from a normal SSE payload', async () => {
  const { decodeAgUiEvent } = await import('../src/sdk/agUiClient.ts');

  assert.equal(typeof decodeAgUiEvent, 'function');
  assert.deepEqual(
    decodeAgUiEvent({
      type: 'message',
      data: '{"event_type":"RUN_FINISHED","run_id":"agent-run-1"}',
    }),
    {
      event: 'RUN_FINISHED',
      data: { event_type: 'RUN_FINISHED', run_id: 'agent-run-1' },
    },
  );
});

test('preserves named SSE error payloads while transport errors stay unavailable', async () => {
  const { describeAgUiStreamError } = await import('../src/sdk/agUiClient.ts');

  assert.equal(typeof describeAgUiStreamError, 'function');
  assert.equal(
    describeAgUiStreamError({ data: '{"message":"upstream quota exceeded"}' }),
    'upstream quota exceeded',
  );
  assert.equal(
    describeAgUiStreamError({}),
    'the AG-UI event stream is unavailable',
  );
});

test('sends standard request identity headers with an AG-UI POST', async () => {
  const { agUiClient } = await import('../src/sdk/agUiClient.ts');
  const originalFetch = globalThis.fetch;
  let observedHeaders;
  globalThis.fetch = async (_url, options) => {
    observedHeaders = new Headers(options?.headers);
    return new Response(JSON.stringify({ run_id: 'agent-run-1' }), { status: 200 });
  };

  try {
    await agUiClient.runAgent({ message: 'start' });
    assert.equal(observedHeaders.has('x-workspace-id'), true);
    assert.equal(observedHeaders.has('x-principal-id'), true);
  } finally {
    globalThis.fetch = originalFetch;
  }
});
