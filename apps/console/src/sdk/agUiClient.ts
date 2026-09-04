import { buildRequestIdentityHeaders } from './client.ts';

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

export interface AgUiRequestGate {
  begin: () => number;
  invalidate: () => void;
  admit: (request: number, continuation: () => void) => boolean;
}

export interface AgUiRunAgentOutput {
  run_id?: string;
  status?: string;
  message?: string;
}

export type AgUiStreamStatus = 'connecting' | 'open' | 'closed';

const AG_UI_BASE = '/api/ag-ui';

function parseAgUiPayload(payload: unknown): unknown {
  if (typeof payload !== 'string') return payload;
  try {
    return JSON.parse(payload);
  } catch {
    return payload;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

export function createAgUiRequestGate(): AgUiRequestGate {
  let generation = 0;

  return {
    begin: () => {
      generation += 1;
      return generation;
    },
    invalidate: () => {
      generation += 1;
    },
    admit: (request, continuation) => {
      if (request !== generation) return false;
      continuation();
      return true;
    },
  };
}

export function decodeAgUiEvent(event: { type?: string; data: unknown }): AgUiEvent {
  const data = parseAgUiPayload(event.data);
  const eventType = isRecord(data) && typeof data.event_type === 'string' && data.event_type.trim()
    ? data.event_type
    : event.type || 'message';

  return { event: eventType, data };
}

export function describeAgUiStreamError(event: Event | { data?: unknown }): string {
  if (!('data' in event)) return 'the AG-UI event stream is unavailable';

  const data = parseAgUiPayload(event.data);
  if (isRecord(data) && typeof data.message === 'string' && data.message.trim()) return data.message;
  if (typeof data === 'string' && data.trim()) return data;

  const serialized = JSON.stringify(data);
  return serialized === undefined ? 'the AG-UI event stream is unavailable' : serialized;
}

export function buildAgUiEventStreamUrl(runId: string): string {
  return `${AG_UI_BASE}/events/stream?run_id=${encodeURIComponent(runId)}`;
}

async function agUiRequest<T>(path: string, options?: RequestInit): Promise<T> {
  const response = await fetch(`${AG_UI_BASE}${path}`, {
    ...options,
    headers: {
      ...buildRequestIdentityHeaders(),
      ...options?.headers,
    },
  });
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
  connectEventStream: (runId: string, handlers: AgUiStreamHandlers): (() => void) => {
    let source: EventSource;
    try {
      source = new EventSource(buildAgUiEventStreamUrl(runId));
    } catch (reason: unknown) {
      handlers.onStatus?.('closed');
      handlers.onError?.(reason instanceof Error ? reason.message : 'the AG-UI stream could not be opened');
      return () => {};
    }

    handlers.onStatus?.('connecting');

    source.onopen = () => handlers.onStatus?.('open');

    source.onmessage = (event: MessageEvent<string>) => handlers.onEvent(decodeAgUiEvent(event));

    source.onerror = (event) => {
      // EventSource retries indefinitely by default, which turns an
      // unimplemented endpoint into a reconnect loop. Close it instead.
      source.close();
      handlers.onStatus?.('closed');
      handlers.onError?.(describeAgUiStreamError(event));
    };

    return () => {
      source.close();
      handlers.onStatus?.('closed');
    };
  },
};
