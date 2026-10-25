// Developer-only Rust DTO export. The bootstrap protocol generator is unchanged.
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
const worker = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const root = resolve(worker, '../..');
const mode = process.argv[2];
if (!['check', 'write'].includes(mode)) throw new Error('usage: contract.mjs check|write');
const scratch = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-schema-'));
function type(schema) {
  if (schema.$ref) return schema.$ref.split('/').at(-1);
  if (schema.enum) return schema.enum.map(value => JSON.stringify(value)).join(' | ');
  if (schema.type === 'string') return 'string';
  if (schema.type === 'boolean') return 'boolean';
  if (schema.type === 'object' && schema.additionalProperties === false) {
    return '{\n' + Object.entries(schema.properties).map(([key, value]) =>
      '  ' + JSON.stringify(key) + (schema.required?.includes(key) ? '' : '?') + ': ' + type(value) + ';').join('\n') + '\n}';
  }
  throw new Error('Unsupported schema construct; extend exporter with rejection tests');
}
try {
  execFileSync('cargo', ['test', '-p', 'oasis7_custody_core', '--lib', '--locked',
    'http::tests::emit_http_contract', '--', '--exact'], {
    cwd: root, stdio: 'inherit',
    env: { ...process.env, RUSTC_WRAPPER: '', OASIS7_CUSTODY_SCHEMA_OUTPUT: scratch },
  });
  const raw = readFileSync(resolve(scratch, 'http.schema.json'), 'utf8');
  const contract = JSON.parse(raw);
  const definitions = new Map();
  for (const schema of [contract.metadata, contract.error]) {
    for (const [name, value] of Object.entries(schema.$defs ?? {})) {
      if (definitions.has(name) && JSON.stringify(definitions.get(name)) !== JSON.stringify(value))
        throw new Error('Conflicting schema definition');
      definitions.set(name, value);
    }
  }
  const ts = '// Generated from actual Rust handler DTOs. Do not edit.\n' +
    [...definitions].map(([name, schema]) => 'export type ' + name + ' = ' + type(schema) + ';\n').join('') +
    [['ServiceMetadata', contract.metadata], ['ErrorResponse', contract.error]]
      .map(([name, schema]) => 'export type ' + name + ' = ' + type(schema) + ';\n').join('');
  for (const [name, content] of [['http.schema.json', raw], ['http.d.ts', ts]]) {
    const output = resolve(worker, 'contract', name);
    if (mode === 'write') writeFileSync(output, content);
    else if (readFileSync(output, 'utf8') !== content) throw new Error('Stale Rust HTTP contract: ' + name);
  }
} finally { rmSync(scratch, { recursive: true, force: true }); }
