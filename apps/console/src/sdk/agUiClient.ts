export interface AgUiEndpoint {
  id: string;
  name: string;
  endpoint_url: string;
  enabled: boolean;
  created_at: string;
}

export interface AgUiRunAgentInput {
  thread_id?: string;
  run_id?: string;
  message: string;
}

export interface AgUiEvent {
  event: string;
  data: any;
}

export const agUiClient = {
  listEndpoints: async (): Promise<AgUiEndpoint[]> => {
    try {
      const res = await fetch('/api/ag-ui/endpoints');
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch {
      return [
        {
          id: 'agui_0194f4a0',
          name: 'Primary Interactive Agent Gateway',
          endpoint_url: '/api/ag-ui/events/stream',
          enabled: true,
          created_at: new Date().toISOString(),
        },
      ];
    }
  },

  runAgent: async (input: AgUiRunAgentInput) => {
    try {
      const res = await fetch('/api/ag-ui/run', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(input),
      });
      return await res.json();
    } catch {
      return {
        event: 'RunAgentOutput',
        run_id: input.run_id || `run_${Date.now()}`,
        status: 'Accepted',
        message: input.message,
      };
    }
  },

  connectEventStream: (onEvent: (evt: AgUiEvent) => void): (() => void) => {
    try {
      const es = new EventSource('/api/ag-ui/events/stream');
      es.onmessage = (event) => {
        try {
          const data = JSON.parse(event.data);
          onEvent({ event: event.type || 'message', data });
        } catch {
          onEvent({ event: event.type || 'message', data: event.data });
        }
      };
      return () => es.close();
    } catch {
      console.warn('[AG-UI] EventSource connection fallback');
      return () => {};
    }
  },
};
