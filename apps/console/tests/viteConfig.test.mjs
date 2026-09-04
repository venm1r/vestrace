import assert from 'node:assert/strict';
import test from 'node:test';

test('configures the AG-UI dev proxy with the configured request identity', async () => {
  const { createConsoleViteConfig } = await import('../vite.config.ts');

  assert.equal(typeof createConsoleViteConfig, 'function');
  const config = createConsoleViteConfig({
    VITE_VESTRACE_WORKSPACE_ID: '11111111-1111-1111-1111-111111111111',
    VITE_VESTRACE_PRINCIPAL_ID: '22222222-2222-2222-2222-222222222222',
  });
  const headers = config.server.proxy['/api/ag-ui'].headers;
  assert.deepEqual(headers, {
    'x-workspace-id': '11111111-1111-1111-1111-111111111111',
    'x-principal-id': '22222222-2222-2222-2222-222222222222',
  });
});
