import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const css = readFileSync(new URL('../viewer_terminal_shell.css', import.meta.url), 'utf8');
const host = readFileSync(new URL('../software_safe_src/pixel_world_host.jsx', import.meta.url), 'utf8');

test('retained recovery receipt stays in flow instead of covering the reload button', () => {
  const rule = css.match(/\[data-viewer-overlay="renderer-unavailable"\] > \[data-viewer-overlay="receipt"\]\s*\{([^}]+)\}/)?.[1];
  assert.ok(rule, 'recovery receipts need a scoped layout override');
  assert.match(rule, /position:\s*static\s*;/);
  assert.match(rule, /inset:\s*auto\s*;/);
  assert.match(rule, /z-index:\s*auto\s*;/);
  assert.match(host, /<PixelWorldActionReceipt locale=\{locale\} surface=\{surface\} \/>/);
  assert.match(host, /class="pixel-world-render-unavailable__retry" onClick=\{requestReadyMode\}/);
  assert.doesNotMatch(rule, /display:\s*none|pointer-events:\s*none/);
});
