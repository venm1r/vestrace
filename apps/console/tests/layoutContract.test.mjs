import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const cssUrl = new URL('../src/design-system/tokens/theme.css', import.meta.url);
const runsRailUrl = new URL('../src/components/RunsRail.tsx', import.meta.url);
const runInspectorUrl = new URL('../src/components/RunInspector.tsx', import.meta.url);
const runWorkspaceUrl = new URL('../src/components/RunWorkspace.tsx', import.meta.url);
const appLayoutUrl = new URL('../src/shell/AppLayout.tsx', import.meta.url);

test('keeps the wide operations grid dense at the 1440 reference width', async () => {
  const css = await readFile(cssUrl, 'utf8');

  assert.match(css, /--layout-sidebar: 196px;/);
  assert.match(css, /--layout-inspector: 286px;/);
  assert.match(css, /--layout-metrics: 250px;/);
  assert.match(css, /grid-template-columns: 224px minmax\(360px, 1fr\) var\(--layout-inspector\) var\(--layout-metrics\);/);
  assert.doesNotMatch(css, /@media \(max-width: 1439px\)/);
});

test('keeps the field focus outline visible and internal rail metadata bounded', async () => {
  const css = await readFile(cssUrl, 'utf8');

  assert.match(css, /\.field-control:focus-visible \{[^}]*outline: 2px solid var\(--focus-ring\);/);
  assert.match(css, /\.run-rail-timestamp \{[^}]*text-overflow: ellipsis;/);
});

test('uses presentation tones for status chips instead of filter groups', async () => {
  for (const sourceUrl of [runsRailUrl, runInspectorUrl, runWorkspaceUrl]) {
    const source = await readFile(sourceUrl, 'utf8');
    assert.match(source, /status-chip--\$\{status\.tone\}/);
  }
});

test('routes mobile nav links through the focus-restoring close path only in drawer mode', async () => {
  const source = await readFile(appLayoutUrl, 'utf8');

  assert.match(source, /if \(isMobileDrawer\) closeMobileNavigation\(\);/);
});
