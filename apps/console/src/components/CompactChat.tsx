import React, { useEffect, useRef, useState } from 'react';
import { Surface } from '../design-system/primitives/Surface';
import { Button } from '../design-system/primitives/Button';
import {
  agUiClient,
  createAgUiRequestGate,
  type AgUiEvent,
  type AgUiRequestGate,
  type AgUiStreamStatus,
} from '../sdk/agUiClient';

interface ChatMessage {
  id: string;
  sender: string;
  kind: 'operator' | 'gateway';
  text: string;
  timestamp: string;
}

const STREAM_PRESENTATION: Record<AgUiStreamStatus, { label: string; color: string }> = {
  connecting: { label: 'AG-UI stream connecting', color: 'var(--color-warning)' },
  open: { label: 'AG-UI stream active', color: 'var(--color-success)' },
  closed: { label: 'AG-UI stream unavailable', color: 'var(--color-error)' },
};

let messageCounter = 0;

function nextMessageId(): string {
  messageCounter += 1;
  return `ag-ui-event-${messageCounter}`;
}

function formatEventData(data: unknown): string {
  if (typeof data === 'string') return data;
  const serialized = JSON.stringify(data);
  return serialized === undefined ? String(data) : serialized;
}

export const CompactChat: React.FC = () => {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [sending, setSending] = useState(false);
  const [streamStatus, setStreamStatus] = useState<AgUiStreamStatus>('closed');
  const [activeAgentRunId, setActiveAgentRunId] = useState<string | null>(null);
  const [deliveryError, setDeliveryError] = useState<string | null>(null);
  const [streamError, setStreamError] = useState<string | null>(null);
  const disconnectRef = useRef<(() => void) | null>(null);
  const activeStreamRunRef = useRef<string | null>(null);
  const requestGateRef = useRef<AgUiRequestGate | null>(null);
  const logRef = useRef<HTMLDivElement | null>(null);

  if (requestGateRef.current === null) requestGateRef.current = createAgUiRequestGate();

  const append = (message: Omit<ChatMessage, 'id' | 'timestamp'>) =>
    setMessages((previous) => [
      ...previous,
      { ...message, id: nextMessageId(), timestamp: new Date().toLocaleTimeString() },
    ]);

  useEffect(() => () => {
    requestGateRef.current?.invalidate();
    activeStreamRunRef.current = null;
    disconnectRef.current?.();
  }, []);

  useEffect(() => {
    const log = logRef.current;
    if (log) log.scrollTop = log.scrollHeight;
  }, [messages]);

  const handleSend = async () => {
    const message = input.trim();
    if (!message || sending) return;
    const requestGate = requestGateRef.current;
    if (requestGate === null) return;
    const request = requestGate.begin();

    setInput('');
    setSending(true);
    setDeliveryError(null);

    try {
      // The backend creates a separate AG-UI run and ignores a selected console
      // run id, so this request intentionally contains the operator message only.
      const result = await agUiClient.runAgent({ message });
      requestGate.admit(request, () => {
        const returnedRunId = result.run_id;
        if (!returnedRunId) {
          setDeliveryError('AG-UI did not return a run id, so no event stream can be opened.');
          return;
        }

        disconnectRef.current?.();
        activeStreamRunRef.current = returnedRunId;
        setActiveAgentRunId(returnedRunId);
        setMessages([
          {
            id: nextMessageId(),
            sender: 'Operator',
            kind: 'operator',
            text: message,
            timestamp: new Date().toLocaleTimeString(),
          },
        ]);
        setStreamError(null);
        setStreamStatus('connecting');
        disconnectRef.current = agUiClient.connectEventStream(returnedRunId, {
          onStatus: (status) => {
            if (activeStreamRunRef.current === returnedRunId) setStreamStatus(status);
          },
          onEvent: (event: AgUiEvent) => {
            if (activeStreamRunRef.current !== returnedRunId) return;
            append({
              sender: `AG-UI ${event.event}`,
              kind: 'gateway',
              text: formatEventData(event.data),
            });
          },
          onError: (error) => {
            if (activeStreamRunRef.current === returnedRunId) setStreamError(error);
          },
        });
      });
    } catch (reason: unknown) {
      requestGate.admit(request, () => {
        setDeliveryError(reason instanceof Error ? reason.message : 'the instruction could not be delivered');
      });
    } finally {
      requestGate.admit(request, () => setSending(false));
    }
  };

  const presentation = STREAM_PRESENTATION[streamStatus];

  return (
    <Surface level={2} style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: '12px' }}>
        <div>
          <h3 className="type-h3" style={{ margin: 0, color: 'var(--text-primary)' }}>
            Separate AG-UI run
          </h3>
          <p className="type-body-sm" style={{ color: 'var(--text-secondary)', margin: '4px 0 0' }}>
            Sending an instruction starts a separate AG-UI run. It does not continue or mutate the selected run.
          </p>
        </div>
        <span role="status" className="status-chip" style={{ color: presentation.color }}>
          {activeAgentRunId ? presentation.label : 'No AG-UI run'}
        </span>
      </div>

      {activeAgentRunId && (
        <div className="type-code" style={{ color: 'var(--text-secondary)', overflowWrap: 'anywhere' }}>
          Streaming separate run: {activeAgentRunId}
        </div>
      )}
      {deliveryError && <div role="alert" style={{ color: 'var(--color-error)' }}>{deliveryError}</div>}
      {streamError && <div role="alert" style={{ color: 'var(--color-error)' }}>{streamError}</div>}

      <div
        ref={logRef}
        role="log"
        aria-live="polite"
        aria-label="Events for the separate AG-UI run"
        style={{
          display: 'flex',
          flexDirection: 'column',
          gap: 'var(--space-2)',
          minHeight: '120px',
          maxHeight: '220px',
          overflowY: 'auto',
          padding: 'var(--space-2)',
          backgroundColor: 'var(--bg-level-0)',
          borderRadius: '8px',
          border: '1px solid var(--border-color)',
        }}
      >
        {messages.length === 0 ? (
          <div className="type-body-sm" style={{ color: 'var(--text-secondary)' }}>
            No events have been returned for a separate AG-UI run.
          </div>
        ) : (
          messages.map((message) => (
            <div key={message.id} style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
              <div className="type-body-sm" style={{ display: 'flex', justifyContent: 'space-between', gap: '8px', color: 'var(--text-secondary)' }}>
                <strong style={{ color: message.kind === 'operator' ? 'var(--color-tertiary)' : 'var(--text-primary)' }}>
                  {message.sender}
                </strong>
                <span>{message.timestamp}</span>
              </div>
              <div className="type-code" style={{ color: 'var(--text-primary)', overflowWrap: 'anywhere' }}>
                {message.text}
              </div>
            </div>
          ))
        )}
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-2)' }}>
        <label htmlFor="ag-ui-input" style={{ position: 'absolute', left: '-9999px' }}>
          Start a separate AG-UI run
        </label>
        <input
          id="ag-ui-input"
          type="text"
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Enter') {
              event.preventDefault();
              void handleSend();
            }
          }}
          placeholder="Start a separate AG-UI run..."
          className="field-control"
          style={{ flex: 1, minWidth: 0 }}
        />
        <Button variant="primary" onClick={() => void handleSend()} disabled={sending || !input.trim()}>
          {sending ? 'Starting…' : 'Start run'}
        </Button>
      </div>
    </Surface>
  );
};
