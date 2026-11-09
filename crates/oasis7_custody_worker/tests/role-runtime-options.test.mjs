import assert from 'node:assert/strict';
import test from 'node:test';
import {resolve} from 'node:path';
import {convertV4MiniflareOptions} from 'miniflare';
import {roleRuntimeOptions} from './fixtures/role_runtime_native_support.mjs';

// Configuration-only checks: no Miniflare instance, fixture process or HTTP request.
test('actual multi-worker role options disable shared Cloudflare metadata fetching',()=>{
  const persist=resolve('unused-role-options-persistence');
  const data={namespace:'fixture-namespace',identity_instance_epoch:'0',audience:'fixture-audience',
    key_id:'fixture-key',public_key:'ab'.repeat(32),adapter_secret:'synthetic-adapter',
    revocation_poll_secret:'synthetic-poll',revocation_ack_secret:'synthetic-ack',
    managed_binding_owner_pin:{}};
  let bridgeCalls=0;
  const options=roleRuntimeOptions(persist,data,()=>{bridgeCalls++;throw new Error('configuration must not invoke the bridge');},
    'native-participant-owner-setup-real-fence-test-session',raw=>{
      // The converter reads module bytes. Supply inert bytes so this contract
      // needs no compiled Wasm, starts no runtime and keeps every role/binding.
      for(const worker of raw.workers)worker.modules=worker.modules.map(module=>({...module,
        contents:module.type==='CompiledWasm'?new Uint8Array([0,97,115,109,1,0,0,0]):'export default {}'}));
      return raw;
    });
  assert.equal(options.cf,false,'cf is a shared Miniflare option, not a per-worker option');
  assert.equal(options.workers.length,6,'retain all real-fence fixture roles and proxies');
  assert.equal(options.resourcePersistencePath,persist);
  assert.equal(options.unsafeDevRegistryPath,resolve(persist,'registry'));
  assert.equal(bridgeCalls,0);
});

test('Miniflare conversion does not promote worker-only cf flags to the shared option',()=>{
  const worker={name:'configuration-only',modules:true,script:'export default {}',
    compatibilityDate:'2026-10-25',cf:false};
  const misplaced=convertV4MiniflareOptions({workers:[worker]});
  assert.equal(misplaced.cf,undefined,'the old per-worker placement leaves default fetching enabled');
  const isolated=convertV4MiniflareOptions({cf:false,workers:[worker]});
  assert.equal(isolated.cf,false,'the shared opt-out must survive the actual locked converter');
  assert.equal(isolated.workers.length,misplaced.workers.length);
});
