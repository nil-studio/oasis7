// Developer-only Rust DTO export. The bootstrap protocol generator is unchanged.
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';
import { declarations } from './schema-to-typescript.mjs';
const worker = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const root = resolve(worker, '../..');
const mode = process.argv[2];
if (!['check', 'write'].includes(mode)) throw new Error('usage: contract.mjs check|write');
const scratch = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-schema-'));
try {
  execFileSync('cargo', ['test', '-p', 'oasis7_custody_core', '--lib', '--locked',
    'http::tests::emit_http_contract', '--', '--exact'], {
    cwd: root, stdio: 'inherit',
    env: { ...process.env, RUSTC_WRAPPER: '', OASIS7_CUSTODY_SCHEMA_OUTPUT: scratch },
  });
  const raw = readFileSync(resolve(scratch, 'http.schema.json'), 'utf8');
  const contract = JSON.parse(raw);
  const ts = declarations({ ServiceMetadata: contract.metadata, ErrorResponse: contract.error, ...contract.native });
  for (const [name, content] of [['http.schema.json', raw], ['http.d.ts', ts]]) {
    const output = resolve(worker, 'contract', name);
    if (mode === 'write') writeFileSync(output, content);
    else if (readFileSync(output, 'utf8') !== content) throw new Error('Stale Rust HTTP contract: ' + name);
  }
} finally { rmSync(scratch, { recursive: true, force: true }); }
