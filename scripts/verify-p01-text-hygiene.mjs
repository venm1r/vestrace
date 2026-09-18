import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { CREATED_TEXT_PATHS } from './p01-scope.mjs';

const canonicalize = (value) => Array.isArray(value) ? value.map(canonicalize) : value && typeof value === 'object' ? Object.fromEntries(Object.keys(value).sort().map((key) => [key, canonicalize(value[key])])) : value;

export function validateText(bytes, path) {
  if (bytes.subarray(0, 3).equals(Buffer.from([0xef, 0xbb, 0xbf]))) throw new Error(`BOM: ${path}`);
  let text; try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); } catch { throw new Error(`invalid UTF-8: ${path}`); }
  if (text.includes('\uFFFD') || text.includes('\r') || !text.endsWith('\n') || /[ \t]\n/.test(text)) throw new Error(`noncanonical text: ${path}`);
  if (path.endsWith('.json')) { const canonical = JSON.stringify(canonicalize(JSON.parse(text)), null, 2) + '\n'; if (text !== canonical) throw new Error(`noncanonical JSON: ${path}`); }
}
export function verifyP01TextHygiene(repoRoot) { for (const path of CREATED_TEXT_PATHS) validateText(readFileSync(resolve(repoRoot, path)), path); return []; }
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) { try { if (process.argv.length !== 4 || process.argv[2] !== '--check') throw new Error('usage: node scripts/verify-p01-text-hygiene.mjs --check <repo-root>'); verifyP01TextHygiene(resolve(process.argv[3])); } catch (error) { console.error(error.message); process.exitCode = 1; } }
