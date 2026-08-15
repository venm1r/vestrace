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
  data: unknown;
}

export interface AgUiRunAgentOutput {
  run_id?: string;
  status?: string;
  message?: string;
}

export type AgUiStreamStatus = 'connecting' | 'open' | 'closed';

const AG_UI_BASE = '/api/ag-ui';

async function agUiRequest<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${AG_UI_BASE}${path}`, options);
  if (!response.ok) {
    let message = `AG-UI request failed with status ${response.status}`;
    try {
      const body = (await response.json()) as { message?: string };
      if (body.message) message = body.message;
    } catch {
      /* the error body was not JSON; keep the status-based message */
    }
    throw new Error(message);
  }
  return (await response.json()) as T;
}

export interface AgUiStreamHandlers {
  onEvent: (event: AgUiEvent) => void;
  onStatus?: (status: AgUiStreamStatus) => void;
  onError?: (message: string) => void;
}

export const agUiClient = {
  listEndpoints: (): Promise<AgUiEndpoint[]> => agUiRequest<AgUiEndpoint[]>('/endpoints'),

  runAgent: (input: AgUiRunAgentInput): Promise<AgUiRunAgentOutput> =>
    agUiRequest<AgUiRunAgentOutput>('/run', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(input),
    }),

  /**
   * Opens the AG-UI event stream. The gateway answers 501 in this build;
   * the connection is closed on the first error instead of letting the browser
   * reconnect forever, and the failure is reported to the caller rather than
   * being replaced with fabricated data.
   */
  connectEventStream: (handlers: AgUiStreamHandlers): (() => void) => {
    let source: EventSource;
    try {
      source = new EventSource(`${AG_UI_BASE}/events/stream`);
    } catch (reason: unknown) {
      handlers.onStatus?.('closed');
      handlers.onError?.(reason instanceof Error ? reason.message : 'the AG-UI stream could not be opened');
      return () => {};
    }

    handlers.onStatus?.('connecting');

    source.onopen = () => handlers.onStatus?.('open');

    source.onmessage = (event: MessageEvent<string>) => {
      let data: unknown = event.data;
      try {
        data = JSON.parse(event.data);
      } catch {
        /* not JSON; forward the raw payload */
      }
      handlers.onEvent({ event: event.type || 'message', data });
    };

    source.onerror = () => {
      // EventSource retries indefinitely by default, which turns an
      // unimplemented endpoint into a reconnect loop. Close it instead.
      source.close();
      handlers.onStatus?.('closed');
      handlers.onError?.('the AG-UI event stream is unavailable');
    };

    return () => {
      source.close();
      handlers.onStatus?.('closed');
    };
  },
};
