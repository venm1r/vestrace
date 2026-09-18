import { deflateSync } from 'node:zlib';
import { existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const OUTPUT_PATH = 'tests/fixtures/openai-q1/marker.png';
const TEXT = 'VESTRACE_Q1_IMAGE';
const SCALE = 4;
const PADDING = 16;
const GLYPHS = {
  A: ['01110','10001','10001','11111','10001','10001','10001'], C: ['01111','10000','10000','10000','10000','10000','01111'], E: ['11111','10000','10000','11110','10000','10000','11111'], G: ['01111','10000','10000','10111','10001','10001','01111'], I: ['11111','00100','00100','00100','00100','00100','11111'], M: ['10001','11011','10101','10101','10001','10001','10001'], Q: ['01110','10001','10001','10001','10101','10010','01101'], R: ['11110','10001','10001','11110','10100','10010','10001'], S: ['01111','10000','10000','01110','00001','00001','11110'], T: ['11111','00100','00100','00100','00100','00100','00100'], V: ['10001','10001','10001','10001','10001','01010','00100'], _: ['00000','00000','00000','00000','00000','00000','11111'], '1': ['00100','01100','00100','00100','00100','00100','01110'],
};

function crc32(bytes) { let crc = 0xffffffff; for (const byte of bytes) { crc ^= byte; for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0); } return (crc ^ 0xffffffff) >>> 0; }
function u32(value) { const bytes = Buffer.alloc(4); bytes.writeUInt32BE(value); return bytes; }
function chunk(type, data) { const name = Buffer.from(type, 'ascii'); return Buffer.concat([u32(data.length), name, data, u32(crc32(Buffer.concat([name, data]))) ]); }

export function generateMarkerPng() {
  const width = PADDING * 2 + (TEXT.length * 6 - 1) * SCALE;
  const height = PADDING * 2 + 7 * SCALE;
  const pixels = Buffer.alloc(width * height * 3, 255);
  for (let characterIndex = 0; characterIndex < TEXT.length; characterIndex += 1) {
    const glyph = GLYPHS[TEXT[characterIndex]];
    if (!glyph) throw new Error(`missing glyph: ${TEXT[characterIndex]}`);
    for (let row = 0; row < glyph.length; row += 1) for (let column = 0; column < glyph[row].length; column += 1) if (glyph[row][column] === '1') for (let dy = 0; dy < SCALE; dy += 1) for (let dx = 0; dx < SCALE; dx += 1) {
      const x = PADDING + (characterIndex * 6 + column) * SCALE + dx;
      const y = PADDING + row * SCALE + dy;
      pixels.fill(0, (y * width + x) * 3, (y * width + x + 1) * 3);
    }
  }
  const scanlines = Buffer.alloc((width * 3 + 1) * height);
  for (let y = 0; y < height; y += 1) { scanlines[y * (width * 3 + 1)] = 0; pixels.copy(scanlines, y * (width * 3 + 1) + 1, y * width * 3, (y + 1) * width * 3); }
  return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]), chunk('IHDR', Buffer.concat([u32(width), u32(height), Buffer.from([8,2,0,0,0])])), chunk('IDAT', deflateSync(scanlines, { level: 9 })), chunk('IEND', Buffer.alloc(0))]);
}

function main(argv) {
  if (argv.length !== 2 || !['--write', '--check'].includes(argv[0])) throw new Error('usage: node scripts/generate-openai-q1-marker.mjs --write|--check <repo-root>');
  const output = resolve(argv[1], OUTPUT_PATH);
  const generated = generateMarkerPng();
  if (argv[0] === '--write') { mkdirSync(dirname(output), { recursive: true }); writeFileSync(output, generated); return; }
  if (!existsSync(output) || !readFileSync(output).equals(generated)) throw new Error('q1 marker fixture differs from deterministic generator');
  statSync(output);
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try { main(process.argv.slice(2)); } catch (error) { console.error(error.message); process.exitCode = 1; }
}
