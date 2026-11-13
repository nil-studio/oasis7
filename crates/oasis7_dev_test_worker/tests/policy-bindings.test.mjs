import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFileSync, mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {resolve, dirname} from 'node:path';
import {fileURLToPath} from 'node:url';
import {Miniflare, convertV4MiniflareOptions} from 'miniflare';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const encoded = JSON.parse(readFileSync(resolve(root, 'tests/fixtures/policy-large-bindings.json')));
const policyBindings = Object.fromEntries(Object.entries(encoded).filter(([,v]) => v !== null));
const token = '56'.repeat(32);

test('real Rust Wasm reconstructs deployable multi-account policy without resetting SQLite usage', {timeout:60000}, async () => {
  const persist = mkdtempSync(resolve(tmpdir(), 'oasis7-worker-policy-'));
  let nativeCalls = 0;
  const make = (policy) => new Miniflare({
    ...convertV4MiniflareOptions({
      name:'dev-test-policy', modulesRoot:root, compatibilityDate:'2026-10-25', cf:false,
      modules:[
        {type:'ESModule', path:resolve(root,'build/entry.mjs')},
        {type:'ESModule', path:resolve(root,'build/index.js')},
        {type:'CompiledWasm', path:resolve(root,'build/index_bg.wasm')},
      ],
      durableObjects:{DEV_TEST_ACCOUNTS:{className:'DevTestAccounts',useSQLite:true}},
      bindings:{...policy, DEV_TEST_TOKEN:token, DEV_TEST_STATE_KEK:'18'.repeat(32)},
      outboundService:() => { nativeCalls++; throw new Error('unexpected native call'); },
    }),
    resourcePersistencePath:persist, unsafeDevRegistryPath:resolve(persist,'registry'),
  });
  const invoke = (mf, method, args) => mf.dispatchFetch('https://worker.example.test/mcp', {
    method:'POST', headers:{'Content-Type':'application/json', Authorization:'Bearer '+token},
    body:JSON.stringify({jsonrpc:'2.0', id:1, method, ...(args ? {params:args} : {})}),
  });
  const accounts = mf => invoke(mf, 'tools/call', {name:'dev_test_accounts_call', arguments:{method:'accounts'}});
  const manifest = JSON.parse(policyBindings.DEV_TEST_POLICY_MANIFEST);
  assert.ok(manifest.parts > 1);
  const raw = Array.from({length:manifest.parts}, (_,i) => policyBindings[`DEV_TEST_POLICY_PART_${String(i+1).padStart(2,'0')}`]).join('');
  assert.ok(Buffer.byteLength(raw) > 5120, 'fixture must reproduce the actual single-Secret deployment failure');
  assert.ok(Object.values(policyBindings).every(v => Buffer.byteLength(v) <= 4096));
  const partial = {...policyBindings}; delete partial.DEV_TEST_POLICY_PART_02;
  let mf = make(partial);
  try {
    assert.equal((await accounts(mf)).status, 503, 'incomplete deployment must fail before admission');
    await mf.dispose(); mf = make(policyBindings);
    const accepted = await accounts(mf);
    assert.equal(accepted.status, 200);
    const reply = await accepted.json();
    assert.equal(reply.result.structuredContent.status, 'ok');
    assert.equal(reply.result.structuredContent.value.accounts.length, 2);
    await mf.dispose(); mf = make(policyBindings);
    const exhausted = await (await accounts(mf)).json();
    assert.equal(exhausted.result.structuredContent.code, 'call_limit_exhausted');
    assert.equal((await invoke(mf, 'ping')).status, 200);
    await mf.dispose(); mf = make({...policyBindings, DEV_TEST_POLICY:raw});
    assert.equal((await accounts(mf)).status, 503, 'mixed representations cannot choose an old policy');
    await mf.dispose(); mf = make({...policyBindings, DEV_TEST_POLICY:{invalid:'non-string binding'}});
    assert.equal((await accounts(mf)).status, 503, 'a malformed inline binding is present, not absent');
    assert.equal(nativeCalls, 0);
  } finally {
    await mf.dispose(); rmSync(persist, {recursive:true, force:true});
  }
});
