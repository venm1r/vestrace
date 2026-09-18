import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { buildVestraceAgUiProfile, checkOrWrite } from '../../../scripts/extract-ag-ui-runtime-schemas.mjs';

const require = createRequire(new URL('../package.json', import.meta.url));
const packageJson = require('./package.json');
const unsupported = ['RAW', 'REASONING_ENCRYPTED_VALUE'];
const assertParses = (schema, value, label) => assert.equal(schema.safeParse(value).success, true, label);
const assertRejects = (schema, value, label) => assert.equal(schema.safeParse(value).success, false, label);

test('pinned public AG-UI schemas cover the complete Vestrace request and event surface', async () => {
  assert.equal(packageJson.dependencies['@ag-ui/core'], '0.0.58');
  assert.equal(packageJson.dependencies['@ag-ui/client'], '0.0.58');
  assert.equal(packageJson.devDependencies['zod-to-json-schema'], '3.25.2');
  const core = await import('@ag-ui/core');
  const text = { type: 'text', text: 'hello' };
  const image = { type: 'image', source: { type: 'data', value: 'aW1hZ2U=', mimeType: 'image/png' } };
  const audio = { type: 'audio', source: { type: 'data', value: 'YXVkaW8=', mimeType: 'audio/wav' } };
  const video = { type: 'video', source: { type: 'data', value: 'dmlkZW8=', mimeType: 'video/mp4' } };
  const binary = { type: 'binary', data: 'YmluYXJ5', filename: 'payload.bin', mimeType: 'application/octet-stream' };
  const document = { type: 'document', source: { type: 'data', value: 'ZG9jdW1lbnQ=', mimeType: 'application/pdf' } };
  const multimedia = [text, image, audio, video, binary, document];
  for (const input of multimedia) {
    assertParses(core.InputContentSchema, input, 'InputContentSchema accepts ' + input.type);
    assertParses(core.MessageSchema, { content: [input], id: 'user-' + input.type, role: 'user' }, 'MessageSchema accepts user ' + input.type);
  }
  const runInput = {
    context: [{ description: 'grounding', value: 'fixture' }],
    messages: [{ content: multimedia, id: 'user-1', role: 'user' }],
    resume: [{ interruptId: 'interrupt-1', payload: { approved: true }, status: 'resolved' }],
    runId: 'run-1',
    state: { phase: 'test' },
    threadId: 'thread-1',
    tools: [{ description: 'fixture tool', name: 'fixture_tool', parameters: { type: 'object' } }],
  };
  assertParses(core.RunAgentInputSchema, runInput, 'minimal complete RunAgentInput');
  for (const identity of ['threadId', 'runId']) {
    const mutation = { ...runInput };
    delete mutation[identity];
    assertRejects(core.RunAgentInputSchema, mutation, 'RunAgentInput requires ' + identity);
  }
  assertParses(core.MessageSchema, runInput.messages[0], 'user message root');
  assertParses(core.StateSchema, runInput.state, 'state root');
  assertParses(core.ContextSchema, runInput.context[0], 'context root');
  assertParses(core.ToolSchema, runInput.tools[0], 'tool root');
  assertParses(core.InterruptSchema, { id: 'interrupt-1', reason: 'approval required', responseSchema: { type: 'object' } }, 'interrupt root');
  assertParses(core.RunAgentInputSchema, runInput, 'resume through RunAgentInput');
  const events = {
    TEXT_MESSAGE_START: { messageId: 'message-1', role: 'assistant' }, TEXT_MESSAGE_CONTENT: { delta: 'hello', messageId: 'message-1' }, TEXT_MESSAGE_END: { messageId: 'message-1' }, TEXT_MESSAGE_CHUNK: { delta: 'hello', messageId: 'message-1', role: 'assistant' },
    TOOL_CALL_START: { toolCallId: 'call-1', toolCallName: 'fixture_tool' }, TOOL_CALL_ARGS: { delta: '{"value":true}', toolCallId: 'call-1' }, TOOL_CALL_END: { toolCallId: 'call-1' }, TOOL_CALL_CHUNK: { delta: '{"value":true}', toolCallId: 'call-1', toolCallName: 'fixture_tool' }, TOOL_CALL_RESULT: { content: 'result', messageId: 'message-2', toolCallId: 'call-1' },
    THINKING_START: { title: 'thinking' }, THINKING_END: {}, THINKING_TEXT_MESSAGE_START: {}, THINKING_TEXT_MESSAGE_CONTENT: { delta: 'thinking' }, THINKING_TEXT_MESSAGE_END: {},
    STATE_SNAPSHOT: { snapshot: { phase: 'snapshot' } }, STATE_DELTA: { delta: [{ op: 'replace', path: '/phase', value: 'delta' }] }, MESSAGES_SNAPSHOT: { messages: [{ content: 'snapshot', id: 'message-3', role: 'assistant' }] },
    ACTIVITY_SNAPSHOT: { activityType: 'progress', content: { progress: 1 }, messageId: 'activity-1' }, ACTIVITY_DELTA: { activityType: 'progress', messageId: 'activity-1', patch: [{ op: 'replace', path: '/progress', value: 2 }] }, CUSTOM: { name: 'vestrace.fixture', value: { ok: true } },
    RUN_STARTED: { runId: 'run-1', threadId: 'thread-1' }, RUN_FINISHED: { runId: 'run-1', threadId: 'thread-1' }, RUN_ERROR: { message: 'fixture error' }, STEP_STARTED: { stepName: 'fixture-step' }, STEP_FINISHED: { stepName: 'fixture-step' },
    REASONING_START: { messageId: 'reasoning-1' }, REASONING_MESSAGE_START: { messageId: 'reasoning-1', role: 'reasoning' }, REASONING_MESSAGE_CONTENT: { delta: 'visible reasoning', messageId: 'reasoning-1' }, REASONING_MESSAGE_END: { messageId: 'reasoning-1' }, REASONING_MESSAGE_CHUNK: { delta: 'visible reasoning', messageId: 'reasoning-1' }, REASONING_END: { messageId: 'reasoning-1' },
  };
  assert.deepEqual(Object.keys(events), Object.values(core.EventType).filter((type) => !unsupported.includes(type)));
  for (const [type, event] of Object.entries(events)) assertParses(core.EventSchemas, { type, ...event }, 'EventSchemas accepts ' + type);
  assertRejects(core.InputContentSchema, { ...image, type: 'not-a-media-type' }, 'multimedia discriminator mutation');
  assertRejects(core.InputContentSchema, { ...image, source: { ...image.source, type: 'blob' } }, 'multimedia source mutation');
  assertRejects(core.MessageSchema, { content: 'missing id', role: 'user' }, 'message required ID mutation');
  assertRejects(core.ToolSchema, { description: 'fixture', parameters: {} }, 'tool required name mutation');
  assertRejects(core.EventSchemas, { ...events.TEXT_MESSAGE_START, type: 'NOT_AN_AG_UI_EVENT' }, 'event discriminator mutation');
  assertRejects(core.EventSchemas, { type: 'TEXT_MESSAGE_START', role: 'assistant' }, 'event required ID mutation');
  assertRejects(core.EventSchemas, { type: 'TOOL_CALL_ARGS', toolCallId: 'call-1' }, 'tool argument mutation');
  assertRejects(core.EventSchemas, { type: 'STATE_DELTA', delta: { op: 'replace' } }, 'state patch shape mutation');
  assertParses(core.EventSchemas, { type: 'RAW' }, 'RAW remains an upstream public event');
  assertParses(core.EventSchemas, { encryptedValue: 'ciphertext', entityId: 'reasoning-1', subtype: 'message', type: 'REASONING_ENCRYPTED_VALUE' }, 'encrypted reasoning remains an upstream public event');
  const profile = JSON.parse(readFileSync(new URL('../../../schemas/ag-ui/vestrace-v1-profile.json', import.meta.url), 'utf8'));
  const runtime = JSON.parse(readFileSync(new URL('../../../schemas/ag-ui/0.0.58/runtime-schemas.json', import.meta.url), 'utf8'));
  assert.equal(profile.request_root, 'RunAgentInputSchema'); assert.equal(runtime.package.version, '0.0.58');
  assert.deepEqual(profile.event_types, Object.keys(events)); assert.deepEqual(profile.unsupported_event_types, unsupported);
  assert.deepEqual(profile.surface, {
    context: { root: 'ContextSchema' }, events: { root: 'EventSchemas' },
    interrupts_resume: { interrupt_root: 'InterruptSchema', resume_entry: 'ResumeEntrySchema' },
    messages: { root: 'MessageSchema' },
    multimedia: { input_content_root: 'InputContentSchema', types: ['text', 'image', 'audio', 'video', 'binary', 'document'] },
    request: { root: 'RunAgentInputSchema', required_identity_fields: ['threadId', 'runId'] },
    state: { root: 'StateSchema' }, tools: { root: 'ToolSchema' },
  });
  assert.deepEqual(buildVestraceAgUiProfile(Object.values(core.EventType)), profile);
  await checkOrWrite(fileURLToPath(new URL('../../../', import.meta.url)), false);
});
