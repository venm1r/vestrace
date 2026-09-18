import { defineConfig, loadEnv } from 'vite';
import react from '@vitejs/plugin-react';
import { buildRequestIdentityHeaders } from './src/sdk/client.ts';

export function createConsoleViteConfig(env: Record<string, string | undefined>) {
  const agUiIdentityHeaders = buildRequestIdentityHeaders({
    workspaceId: env.VITE_VESTRACE_WORKSPACE_ID ?? '',
    principalId: env.VITE_VESTRACE_PRINCIPAL_ID ?? '',
  });

  return {
    plugins: [react()],
    server: {
      port: 3000,
      host: true,
      proxy: {
        '/api/v1': {
          target: 'http://127.0.0.1:8080',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/v1/, '/v1'),
        },
        // EventSource cannot attach request headers. In Vite development only,
        // inject the same configured identity used by normal API requests.
        '/api/ag-ui': {
          target: 'http://127.0.0.1:8080',
          changeOrigin: true,
          headers: agUiIdentityHeaders,
          rewrite: (path) => path.replace(/^\/api\/ag-ui/, '/ag-ui'),
        },
        '/api/health': {
          target: 'http://127.0.0.1:8080',
          changeOrigin: true,
          rewrite: (path) => path.replace(/^\/api\/health/, '/health'),
        },
      },
    },
  };
}

export default defineConfig(({ mode }) =>
  createConsoleViteConfig(loadEnv(mode, process.cwd(), 'VITE_')),
);
