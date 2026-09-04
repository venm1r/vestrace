import { createRequire } from 'node:module';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const RUNTIME_PATH = 'schemas/ag-ui/0.0.58/runtime-schemas.json';
const PROFILE_PATH = 'schemas/ag-ui/vestrace-v1-profile.json';
const ROOT_NAMES = ['RunAgentInputSchema', 'MessageSchema', 'StateSchema', 'ContextSchema', 'ToolSchema', 'InterruptSchema', 'InputContentSchema', 'EventSchemas'];
const UNSUPPORTED_EVENT_TYPES = ['RAW', 'REASONING_ENCRYPTED_VALUE'];

const canonicalize = (value) => Array.isArray(value) ? value.map(canonicalize) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonicalize(value[key])])) : value;
const serialize = (value) => `${JSON.stringify(canonicalize(value), null, 2)}\n`;

export function buildVestraceAgUiProfile(eventTypes) {
  const unsupported = UNSUPPORTED_EVENT_TYPES;
  return {
    event_types: eventTypes.filter((type) => !unsupported.includes(type)),
    profile_id: 'vestrace-v1-ag-ui/0.0.58',
    request_root: 'RunAgentInputSchema',
    surface: {
      context: { root: 'ContextSchema' },
      events: { root: 'EventSchemas' },
      interrupts_resume: {
        interrupt_root: 'InterruptSchema',
        resume_entry: 'ResumeEntrySchema',
      },
      messages: { root: 'MessageSchema' },
      multimedia: {
        input_content_root: 'InputContentSchema',
        types: ['text', 'image', 'audio', 'video', 'binary', 'document'],
      },
      request: {
        required_identity_fields: ['threadId', 'runId'],
        root: 'RunAgentInputSchema',
      },
      state: { root: 'StateSchema' },
      tools: { root: 'ToolSchema' },
    },
    supported_roots: ROOT_NAMES,
    transport: { method: 'POST', streaming: 'SSE' },
    unsupported_event_reason: 'The v1 profile does not accept raw or encrypted reasoning event payloads.',
    unsupported_event_types: unsupported,
  };
}

export async function extractRuntimeSchemas(repoRoot) {
  const consolePackage = resolve(repoRoot, 'apps/console/package.json');
  const requireFromConsole = createRequire(consolePackage);
  const core = await import(pathToFileURL(requireFromConsole.resolve('@ag-ui/core')).href);
  const { zodToJsonSchema } = requireFromConsole('zod-to-json-schema');
  const schemas = {};
  for (const name of ROOT_NAMES) {
    if (!core[name]) throw new Error(`@ag-ui/core public export missing: ${name}`);
    schemas[name] = zodToJsonSchema(core[name], { name, target: 'jsonSchema7', $refStrategy: 'root' });
  }
  const packageLock = JSON.parse(readFileSync(resolve(repoRoot, 'apps/console/package-lock.json'), 'utf8'));
  const packageEntry = packageLock.packages['node_modules/@ag-ui/core'];
  if (!packageEntry || packageEntry.version !== '0.0.58') throw new Error('expected @ag-ui/core 0.0.58 lock entry');
  return {
    profile: buildVestraceAgUiProfile(Object.values(core.EventType)),
    runtime: {
      package: { integrity: packageEntry.integrity, name: '@ag-ui/core', version: packageEntry.version },
      schema_draft: 'http://json-schema.org/draft-07/schema#',
      schemas,
    },
  };
}

export async function checkOrWrite(repoRoot, write) {
  const { runtime, profile } = await extractRuntimeSchemas(repoRoot);
  const outputs = [[RUNTIME_PATH, serialize(runtime)], [PROFILE_PATH, serialize(profile)]];
  for (const [relative, text] of outputs) {
    const file = resolve(repoRoot, relative);
    if (write) { mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, text, 'utf8'); }
    else if (!existsSync(file) || readFileSync(file, 'utf8') !== text) throw new Error(`stale AG-UI generated artifact: ${relative}`);
  }
}

async function main(argv) {
  if (argv.length !== 2 || !['--write', '--check'].includes(argv[0])) throw new Error('usage: node scripts/extract-ag-ui-runtime-schemas.mjs --write|--check <repo-root>');
  await checkOrWrite(resolve(argv[1]), argv[0] === '--write');
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  main(process.argv.slice(2)).catch((error) => { console.error(error.message); process.exitCode = 1; });
}
