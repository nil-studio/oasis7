import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn, execFileSync } from 'node:child_process';
import { once } from 'node:events';
import { connect } from 'node:net';
import { existsSync, readFileSync, writeFileSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { Miniflare, convertV4MiniflareOptions, Response as MiniflareResponse } from 'miniflare';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');

async function authority(directory) {
  const binary = readFileSync(resolve(root, 'artifacts/native-test-binary.txt'), 'utf8').trim();
  const ready = resolve(directory, 'native-ready.json');
  const stop = resolve(directory, 'native-stop');
  const child = spawn(binary, ['serve_native_identity_http_fixture', '--ignored', '--nocapture'], {
    env: { ...Object.fromEntries(Object.entries(process.env).filter(([name]) => !name.startsWith('OASIS7_'))),
      OASIS7_CUSTODY_NATIVE_HTTP_READY: ready, OASIS7_CUSTODY_NATIVE_HTTP_STOP: stop },
    stdio: ['ignore', 'ignore', 'pipe'],
  });
  const processDiagnostic = { panic: false, ioKind: undefined };
  // Never retain or print native stderr: it may contain credentials on a future
  // failure path. Extract only a fixed panic bit and closed IO-kind vocabulary.
  child.stderr.on('data', chunk => {
    const text = String(chunk);
    if (text.includes('panicked')) processDiagnostic.panic = true;
    for (const kind of ['UnexpectedEof', 'WouldBlock', 'ConnectionReset', 'BrokenPipe', 'TimedOut', 'InvalidData', 'Interrupted', 'ConnectionAborted']) {
      if (text.includes(kind)) processDiagnostic.ioKind = kind;
    }
  });
  const diagnostics = () => ({ ...processDiagnostic, exitCode: child.exitCode, signalCode: child.signalCode });
  let exited = false;
  const exit = once(child, 'exit').then(([code]) => { exited = true; return code; });
  const close = async () => {
    writeFileSync(stop, 'stop');
    await Promise.race([exit, delay(5000, undefined, { ref: false })]);
    if (!exited) { child.kill('SIGTERM'); await exit; }
  };
  for (let attempt = 0; attempt < 300; attempt++) {
    if (exited) throw new Error('isolated native authority fixture exited before ready');
    if (existsSync(ready)) {
      try { return { data: JSON.parse(readFileSync(ready, 'utf8')), close, diagnostics }; }
      catch { /* Writer may still be completing its bounded local fixture file. */ }
    }
    await delay(20);
  }
  await close();
  throw new Error('isolated native authority fixture did not become ready');
}

test('native HTTP fixture waits for fragmented request bytes without exiting', { timeout: 30000 }, async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-native-fragment-'));
  let native;
  let socket;
  try {
    native = await authority(directory);
    const address = new URL(native.data.address);
    socket = connect(Number(address.port), address.hostname);
    const reply = new Promise(resolve => {
      const chunks = [];
      socket.on('data', bytes => chunks.push(bytes));
      socket.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
      socket.on('error', () => resolve('transport-error'));
      socket.setTimeout(2000, () => { socket.destroy(); resolve('transport-timeout'); });
    });
    await once(socket, 'connect');
    socket.write('GET /_fixture/revocation-status HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{');
    // Deliberately require a second read on an accepted connection. Darwin may
    // inherit O_NONBLOCK from the polling listener; the fixture must reset it.
    await delay(50);
    socket.write('}');
    const response = await reply;
    await delay(20);
    assert.equal(native.diagnostics().panic, false, JSON.stringify(native.diagnostics()));
    assert.match(response, /^HTTP\/1\.1 200 /);
  } finally {
    socket?.destroy();
    await native?.close();
    rmSync(directory, { recursive: true, force: true });
  }
});
function bindings(data) {
  return {
    CUSTODY_NATIVE_ORIGIN: 'https://localhost', CUSTODY_NATIVE_RP_ID: 'localhost',
    CUSTODY_NATIVE_NAMESPACE: data.namespace, CUSTODY_NATIVE_INSTANCE_EPOCH: data.identity_instance_epoch,
    CUSTODY_NATIVE_ENVIRONMENT: 'test', CUSTODY_NATIVE_AUDIENCE: data.audience,
    CUSTODY_NATIVE_APPROVAL_KEY_ID: data.key_id, CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY: data.public_key,
    CUSTODY_NATIVE_ADAPTER_SECRET: data.adapter_secret,
    ...(data.revocation_poll_secret ? { CUSTODY_NATIVE_REVOCATION_POLL_SECRET: data.revocation_poll_secret, CUSTODY_NATIVE_REVOCATION_ACK_SECRET: data.revocation_ack_secret } : {}),
  };
}
const world = { world_id: 'world-one', genesis_digest: 'genesis-one', branch_id: 'branch-one', reorg_epoch: '0', service_public_key: 'ab'.repeat(32) };
const routes = { environment: 'test', identity_route: 'native-one', world_route: 'world-route-one', world_identity: world.world_id };
function ownerBindings(data) {
  return { CUSTODY_NATIVE_APPROVAL_KEY_ID: data.key_id, CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY: data.public_key, CUSTODY_NATIVE_AUDIENCE: data.audience,
    CUSTODY_REVOCATION_OWNER_KEY_ID: 'owner-source-test', CUSTODY_REVOCATION_OWNER_SIGNING_KEY: Buffer.alloc(32, 92).toString('hex'), CUSTODY_NATIVE_NAMESPACE: data.namespace, CUSTODY_NATIVE_INSTANCE_EPOCH: data.identity_instance_epoch,
    CUSTODY_NATIVE_ENVIRONMENT: 'test', CUSTODY_POLICY_VERSION: '1',
    CUSTODY_BOOTSTRAP_MANAGERS: JSON.stringify([{ principal: { identity_namespace: data.namespace, native_account_id: 'a' }, environments: ['test'] }]),
    CUSTODY_PROVISION_ROUTES: JSON.stringify(routes), CUSTODY_PROVISION_WORLD_IDENTITY: JSON.stringify(world) };
}
function keyBindings(role) {
  return {CUSTODY_PROVISION_KEYS_ENABLED:'true',CUSTODY_ROLE_KEY_ENVIRONMENT:'test',CUSTODY_SERVICE_EPOCH:'1',CUSTODY_ROLE_WRAPPING_KEY_VERSION:'1',CUSTODY_ROLE_KEY_KEK:Buffer.alloc(32,{identity:61,owner:62,executor:63}[role]).toString('hex')};
}
function options(directory, data, bridge, overrides = {}, approval = undefined, keys = false) {
  return { ...convertV4MiniflareOptions({
    workers: [{ name: 'caller', modules: [{ type: 'ESModule', path: resolve(root, 'tests/fixtures/native_identity_caller.mjs') }],
      modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
      serviceBindings: { PUMP: { name: 'identity', entrypoint: 'ScheduledFixture' }, IDENTITY: { name: 'identity', entrypoint: 'IdentityNativeApproval' }, IDENTITY_DEFAULT: 'identity', OWNER_DEFAULT:'owner', ...(approval ? {FENCE_DEFAULT:'fence'} : {}) },
    }, { name: 'identity', modules: [
      { type: 'ESModule', path: resolve(root, 'tests/fixtures/native_revocation_scheduler.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/identity/entry.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/identity/index.js') },
      { type: 'CompiledWasm', path: resolve(root, 'artifacts/identity/index_bg.wasm') },
    ], modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
    bindings: { ...bindings(data), ...(approval ? {CUSTODY_PROVISION_APPROVAL_ENABLED:'true'} : {}), ...(keys ? keyBindings('identity') : {}), ...(keys === 'lost' ? {FIXTURE_DROP_ROLE_RESPONSE:'true'} : {}), ...overrides }, outboundService: bridge,
    serviceBindings: { OWNER_REVOCATIONS: { name: 'owner', entrypoint: 'OwnerRevocations' }, OWNER_MANAGEMENT: { name: 'owner', entrypoint: 'OwnerManagement' }, ...(keys ? {ACCOUNT_KEY_PERMITS:{name:'owner',entrypoint:'IdentityKeyPermits'},RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'}} : {}) },
    durableObjects: { SESSIONS: { className: 'NativeSession', useSQLite: true }, ...(keys ? {ROLE_KEYS:{className:'RoleKeyVault',useSQLite:true}} : {}) },
    }, { name: 'owner', modules: [
      { type: 'ESModule', path: resolve(root, 'artifacts/owner/entry.mjs') },
      { type: 'ESModule', path: resolve(root, 'artifacts/owner/index.js') },
      { type: 'CompiledWasm', path: resolve(root, 'artifacts/owner/index_bg.wasm') },
    ], modulesRoot: root, compatibilityDate: '2026-10-25', cf: false,
      bindings: {...ownerBindings(data), ...(approval ? {CUSTODY_PROVISION_APPROVAL_ENABLED:'true',CUSTODY_SERVICE_EPOCH:'1'} : {}), ...(keys ? keyBindings('owner') : {})},
      serviceBindings: {...(approval ? {IDENTITY_NATIVE_APPROVAL:{name:'identity',entrypoint:'IdentityNativeApproval'},RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'}} : {}), ...(keys ? {ACCOUNT_KEY_PERMITS:{name:'owner',entrypoint:'OwnerKeyPermits'},IDENTITY_PROVISION_KEYS:{name:'identity',entrypoint:'RoleKeyProvision'},OWNER_PROVISION_KEYS:{name:'owner',entrypoint:'RoleKeyProvision'},EXECUTOR_PROVISION_KEYS:{name:'executor',entrypoint:'RoleKeyProvision'}} : {})},
      durableObjects: { ACCOUNTS: { className: 'AccountAuthority', useSQLite: true }, MANAGER_ASSOCIATIONS: { className: 'ManagerAssociations', useSQLite: true }, ...(keys ? {ROLE_KEYS:{className:'RoleKeyVault',useSQLite:true}} : {}) },
    }, ...(keys ? [{name:'executor',modules:[{type:'ESModule',path:resolve(root,'artifacts/executor/entry.mjs')},{type:'ESModule',path:resolve(root,'artifacts/executor/index.js')},{type:'CompiledWasm',path:resolve(root,'artifacts/executor/index_bg.wasm')}],modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,bindings:keyBindings('executor'),serviceBindings:{ACCOUNT_KEY_PERMITS:{name:'owner',entrypoint:'ExecutorKeyPermits'},RECOVERY_FENCE:{name:'fence',entrypoint:'RecoveryFenceReader'}},durableObjects:{ROLE_KEYS:{className:'RoleKeyVault',useSQLite:true}}}] : []), ...(approval ? [approval.startsWith('isolated-positive-fixture') ? {
      name:'fence',modules:[{type:'ESModule',path:resolve(root,'tests/fixtures/provision_fence_fixture.mjs')}],modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,
    } : {
      name:'fence', modules:[{type:'ESModule',path:resolve(root,'artifacts/recovery-fence/entry.mjs')},{type:'ESModule',path:resolve(root,'artifacts/recovery-fence/index.js')},{type:'CompiledWasm',path:resolve(root,'artifacts/recovery-fence/index_bg.wasm')}],
      modulesRoot:root,compatibilityDate:'2026-10-25',cf:false,bindings:{CUSTODY_INITIAL_SERVICE_EPOCH:'1'},durableObjects:{FENCES:{className:'RecoveryFence',useSQLite:true}},
    }] : [])],
  }), resourcePersistencePath: directory, unsafeDevRegistryPath: resolve(directory, 'registry') };
}

function persistedRolePublics(persist,account) {
  return JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
found=[]
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-role-key-slot-v1'").fetchone()
  if row:
   slot=json.loads(row[0])['slot']
   if slot and slot['scope']['context']['identity']['account_id']==sys.argv[2]:
    found.append({'role':slot['scope']['context']['identity']['role'],'public_key':slot['public_ref']['public_key'],'ciphertext_length':len(slot['envelope']['ciphertext'])})
  db.close()
 except sqlite3.Error: pass
print(json.dumps(found))
`,persist,account],{encoding:'utf8'}));}

test('real three-role vault dispatch spends persisted attempts, survives restart and never regenerates keys', {timeout:120000}, async () => {
  const directory = mkdtempSync(resolve(tmpdir(),'oasis7-real-role-provision-'));
  let native; let mf;
  try {
    native = await authority(directory);
    let nativeVerifications = 0;
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin,'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status','/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers = new Headers();
      for (const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) { const value=request.headers.get(name); if(value!==null) headers.set(name,value); }
      const response = await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      return new MiniflareResponse(await response.text(),{status:response.status,headers:{'Content-Type':'application/json'}});
    };
    const persist = resolve(directory,'workerd');
    mf = new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    let identity = await mf.getWorker('identity');
    const headers = {Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft = {api_version:1,client_request_id:'actual-three-roles',plan:{...routes,purpose:'actual isolated key preparation',expires_at:String(Date.now()+180000),transport_limit:'128'}};
    const response = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200); const original=await response.json();
    const request = {approval_request_id:'actual-role-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,native_identity_namespace:native.data.namespace,register_request_id:'actual-register',login_request_id:'actual-login',deadline:draft.plan.expires_at,transport_limit:'128'}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'real UV fixture must approve the explicit 128 transport plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'actual-role-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    const before=await (await identity.fetch(base+'/progress',{headers})).json();
    assert.equal(before.stage,'approved'); assert.equal(before.transport_remaining,'128');
    const drive={api_version:1,operation_id:'actual-role-drive'};
    const advanced=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.equal(advanced.status,200,'actual role dispatch chain must return progress');
    const ready=await advanced.json();
    assert.equal(ready.stage,'role_keys_ready','all three actual vaults must commit before readiness');
    assert.deepEqual(ready.pending_operations,[]);
    assert.equal(ready.transport_remaining,'24','one 8-hop inspection and three 32-hop reservations');
    assert.equal(nativeVerifications,2,'one approval consume and one registered origin inspect');
    assert.doesNotMatch(JSON.stringify(ready),/cookie|signature|verify_nonce|ciphertext|wrapped_dek|session_ref/);
    const publics=persistedRolePublics(persist,original.entry.custody_account_id);
    assert.deepEqual(publics.map(v=>v.role).sort(),['executor','identity','owner']);
    assert.equal(new Set(publics.map(v=>v.public_key)).size,3);
    assert.ok(publics.every(v=>v.ciphertext_length===96));
    await mf.dispose(); mf=new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    identity=await mf.getWorker('identity');
    const retried=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.equal(retried.status,200); assert.deepEqual(await retried.json(),ready);
    assert.equal(nativeVerifications,2,'restart/history cannot renew an inspect or dispatch');
  } finally { await mf?.dispose(); await native?.close(); rmSync(directory,{recursive:true,force:true}); }
});

test('old signed low budget cannot dispatch a role or acquire the new maximum automatically', {timeout:120000}, async () => {
  const directory = mkdtempSync(resolve(tmpdir(),'oasis7-real-role-provision-'));
  let native; let mf;
  try {
    native = await authority(directory);
    let nativeVerifications = 0;
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin,'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status','/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers = new Headers();
      for (const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) { const value=request.headers.get(name); if(value!==null) headers.set(name,value); }
      const response = await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      return new MiniflareResponse(await response.text(),{status:response.status,headers:{'Content-Type':'application/json'}});
    };
    const persist = resolve(directory,'workerd');
    mf = new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    let identity = await mf.getWorker('identity');
    const headers = {Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft = {api_version:1,client_request_id:'actual-three-roles',plan:{...routes,purpose:'actual isolated key preparation',expires_at:String(Date.now()+180000),transport_limit:'24'}};
    const response = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200); const original=await response.json();
    const request = {approval_request_id:'actual-role-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,native_identity_namespace:native.data.namespace,register_request_id:'actual-register',login_request_id:'actual-login',deadline:draft.plan.expires_at,transport_limit:'24'}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'real UV fixture must approve the explicit 128 transport plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'actual-role-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    const before=await (await identity.fetch(base+'/progress',{headers})).json();
    assert.equal(before.stage,'approved'); assert.equal(before.transport_remaining,'24');
    const drive={api_version:1,operation_id:'actual-role-drive'};
    const advanced=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.notEqual(advanced.status,200,'insufficient signed budget must refuse dispatch');
    const progressResponse=await identity.fetch(base+'/progress',{headers});
    assert.equal(progressResponse.status,200);
    const progress=await progressResponse.json();
    assert.equal(progress.stage,'approved');
    assert.equal(progress.transport_remaining,'16','only the source inspection reservation was affordable');
    assert.deepEqual(persistedRolePublics(persist,original.entry.custody_account_id),[]);
    assert.equal(nativeVerifications,2);
    const retried=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.equal(retried.status,200);
    assert.equal((await retried.json()).transport_remaining,'16');
    assert.equal(nativeVerifications,2,'history cannot retry spending or top up budget');
  } finally { await mf?.dispose(); await native?.close(); rmSync(directory,{recursive:true,force:true}); }
});

test('slow Fence response cannot renew the five-second authority window', {timeout:120000}, async () => {
  const directory = mkdtempSync(resolve(tmpdir(),'oasis7-real-role-provision-'));
  let native; let mf;
  try {
    native = await authority(directory);
    let nativeVerifications = 0;
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin,'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status','/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers = new Headers();
      for (const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) { const value=request.headers.get(name); if(value!==null) headers.set(name,value); }
      const response = await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      return new MiniflareResponse(await response.text(),{status:response.status,headers:{'Content-Type':'application/json'}});
    };
    const persist = resolve(directory,'workerd');
    mf = new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    let identity = await mf.getWorker('identity');
    const headers = {Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft = {api_version:1,client_request_id:'actual-three-roles',plan:{...routes,purpose:'actual isolated key preparation',expires_at:String(Date.now()+180000),transport_limit:'128'}};
    const response = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200); const original=await response.json();
    const request = {approval_request_id:'actual-role-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,native_identity_namespace:native.data.namespace,register_request_id:'actual-register',login_request_id:'actual-login',deadline:draft.plan.expires_at,transport_limit:'128'}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'real UV fixture must approve the explicit 128 transport plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'actual-role-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    const before=await (await identity.fetch(base+'/progress',{headers})).json();
    assert.equal(before.stage,'approved'); assert.equal(before.transport_remaining,'128');
    const fence=await mf.getWorker('fence');
    await fence.fetch('https://fixture.test/slow-once');
    const drive={api_version:1,operation_id:'actual-role-drive'};
    const advanced=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.notEqual(advanced.status,200,'a slow successful Fence response cannot restore freshness');
    assert.deepEqual(persistedRolePublics(persist,original.entry.custody_account_id),[]);
    assert.equal(nativeVerifications,1,'stale authority must fail before native inspection is dispatched');
    const progress=await (await identity.fetch(base+'/progress',{headers})).json();
    assert.equal(progress.transport_remaining,'120','failed remote observations retain their reservation');
  } finally { await mf?.dispose(); await native?.close(); rmSync(directory,{recursive:true,force:true}); }
});

test('directory freeze after native inspection prevents actual role dispatch', {timeout:120000}, async () => {
  const directory = mkdtempSync(resolve(tmpdir(),'oasis7-real-role-provision-'));
  let native; let mf;
  try {
    native = await authority(directory);
    let nativeVerifications = 0;
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin,'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status','/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers = new Headers();
      for (const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) { const value=request.headers.get(name); if(value!==null) headers.set(name,value); }
      const response = await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      const responseText=await response.text();
      if(url.pathname.endsWith('/verify') && nativeVerifications===2) {
        const liveIdentity=await mf.getWorker('identity');
        const frozen=await liveIdentity.fetch('https://localhost/v1/manager-revocations',{method:'POST',headers:{Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf},body:JSON.stringify({api_version:1,operation_id:'freeze-after-native-inspection'})});
        assert.equal(frozen.status,200,'real directory revocation must commit before native response returns');
      }
      return new MiniflareResponse(responseText,{status:response.status,headers:{'Content-Type':'application/json'}});
    };
    const persist = resolve(directory,'workerd');
    mf = new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    let identity = await mf.getWorker('identity');
    const headers = {Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft = {api_version:1,client_request_id:'actual-three-roles',plan:{...routes,purpose:'actual isolated key preparation',expires_at:String(Date.now()+180000),transport_limit:'128'}};
    const response = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200); const original=await response.json();
    const request = {approval_request_id:'actual-role-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,native_identity_namespace:native.data.namespace,register_request_id:'actual-register',login_request_id:'actual-login',deadline:draft.plan.expires_at,transport_limit:'128'}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'real UV fixture must approve the explicit 128 transport plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'actual-role-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    const before=await (await identity.fetch(base+'/progress',{headers})).json();
    assert.equal(before.stage,'approved'); assert.equal(before.transport_remaining,'128');
    const drive={api_version:1,operation_id:'actual-role-drive'};
    const advanced=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.notEqual(advanced.status,200,'freeze must win over a successful native inspection');
    assert.deepEqual(persistedRolePublics(persist,original.entry.custody_account_id),[]);
    assert.equal(nativeVerifications,2);
  } finally { await mf?.dispose(); await native?.close(); rmSync(directory,{recursive:true,force:true}); }
});

test('lost committed role response cannot resend after restart and fresh attempt recovers the same key', {timeout:120000}, async () => {
  const directory = mkdtempSync(resolve(tmpdir(),'oasis7-real-role-provision-'));
  let native; let mf;
  try {
    native = await authority(directory);
    let nativeVerifications = 0;
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin,'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status','/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/verify')) nativeVerifications++;
      const headers = new Headers();
      for (const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) { const value=request.headers.get(name); if(value!==null) headers.set(name,value); }
      const response = await fetch(native.data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
      return new MiniflareResponse(await response.text(),{status:response.status,headers:{'Content-Type':'application/json'}});
    };
    const persist = resolve(directory,'workerd');
    mf = new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture','lost'));
    let identity = await mf.getWorker('identity');
    const headers = {Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf};
    const draft = {api_version:1,client_request_id:'actual-three-roles',plan:{...routes,purpose:'actual isolated key preparation',expires_at:String(Date.now()+180000),transport_limit:'256'}};
    const response = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
    assert.equal(response.status,200); const original=await response.json();
    const request = {approval_request_id:'actual-role-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,identity_route:routes.identity_route,world_route:routes.world_route,native_identity_namespace:native.data.namespace,register_request_id:'actual-register',login_request_id:'actual-login',deadline:draft.plan.expires_at,transport_limit:'256'}};
    const completed=await (await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)})).json();
    assert.ok(completed.approval,'real UV fixture must approve the explicit 128 transport plan');
    const base='https://localhost/v1/account-provisions/'+original.entry.provision_id;
    const accepted=await identity.fetch(base+'/approve',{method:'POST',headers,body:JSON.stringify({api_version:1,input:{operation_id:'actual-role-approval-operation',request,approval:completed.approval}})});
    assert.equal(accepted.status,200);
    const before=await (await identity.fetch(base+'/progress',{headers})).json();
    assert.equal(before.stage,'approved'); assert.equal(before.transport_remaining,'256');
    const drive={api_version:1,operation_id:'actual-role-drive'};
    const advanced=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.equal(advanced.status,200,'actual role dispatch chain must return progress');
    const pending=await advanced.json();
    assert.equal(pending.stage,'approved');
    assert.equal(pending.transport_remaining,'216');
    assert.equal(pending.pending_operations.length,1);
    const committedBeforeLoss=persistedRolePublics(persist,original.entry.custody_account_id);
    assert.equal(committedBeforeLoss.length,1);
    assert.equal(committedBeforeLoss[0].role,'identity');
    assert.equal(nativeVerifications,2);
    await mf.dispose(); mf=new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    identity=await mf.getWorker('identity');
    const history=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify(drive)});
    assert.equal(history.status,200); assert.deepEqual(await history.json(),pending);
    assert.equal(nativeVerifications,2,'claimed unknown attempt must never regain send permission');
    const recovered=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify({...drive,operation_id:'fresh-recovery-drive'})});
    assert.equal(recovered.status,200);
    const ready=await recovered.json();
    assert.equal(ready.stage,'role_keys_ready','all three actual vaults must commit before readiness');
    assert.equal(ready.pending_operations.length,1,'unknown original stays visible after a new attempt succeeds');
    assert.equal(ready.transport_remaining,'112','one 8-hop inspection and three 32-hop reservations');
    assert.equal(nativeVerifications,3,'fresh attempt pays for a new source inspection');
    assert.doesNotMatch(JSON.stringify(ready),/cookie|signature|verify_nonce|ciphertext|wrapped_dek|session_ref/);
    const publics=persistedRolePublics(persist,original.entry.custody_account_id);
    assert.deepEqual(publics.map(v=>v.role).sort(),['executor','identity','owner']);
    assert.equal(publics.find(v=>v.role==='identity').public_key,committedBeforeLoss[0].public_key,'fresh attempt must return the original committed public key');
    assert.equal(new Set(publics.map(v=>v.public_key)).size,3);
    assert.ok(publics.every(v=>v.ciphertext_length===96));
    await mf.dispose(); mf=new Miniflare(options(persist,native.data,bridge,{},'isolated-positive-fixture',true));
    identity=await mf.getWorker('identity');
    const retried=await identity.fetch(base+'/continue',{method:'POST',headers,body:JSON.stringify({...drive,operation_id:'fresh-recovery-drive'})});
    assert.equal(retried.status,200); assert.deepEqual(await retried.json(),ready);
    assert.equal(nativeVerifications,3,'restart/history cannot renew an inspect or dispatch');
  } finally { await mf?.dispose(); await native?.close(); rmSync(directory,{recursive:true,force:true}); }
});

test('actual provisioning approval uses native UV proof and Account SQL; independent production fence stays frozen', { timeout: 120000 }, async () => {
  for (const profile of ['production-frozen', 'isolated-positive-fixture', 'isolated-positive-fixture-freeze-during-verify']) {
    const directory = mkdtempSync(resolve(tmpdir(), 'oasis7-provision-approval-chain-'));
    let native; let mf;
    try {
      native = await authority(directory);
      let verifies = 0;
      const bridge = async request => {
        const url = new URL(request.url);
        assert.equal(url.origin, 'https://localhost');
        assert.ok(['/api/public/hosted-account/v2/custody/management-session-status', '/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
        if (url.pathname.endsWith('/verify')) verifies++;
        const headers = new Headers();
        for (const name of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']) {
          const value = request.headers.get(name); if (value !== null) headers.set(name,value);
        }
        const response = await fetch(native.data.address + url.pathname, {method:request.method, headers,
          ...(request.method === 'GET' ? {} : {body:Buffer.from(await request.arrayBuffer())}), redirect:'manual'});
        const text = await response.text();
        if (url.pathname.endsWith('/verify') && profile.endsWith('-freeze-during-verify')) {
          const identity = await mf.getWorker('identity');
          await identity.fetch('https://localhost/v1/manager-revocations',{method:'POST',headers:{Cookie:native.data.cookie,Origin:'https://localhost','Content-Type':'application/json','X-Oasis7-CSRF':native.data.csrf},body:JSON.stringify({api_version:1,operation_id:'freeze-during-verification'})});
        }
        return new MiniflareResponse(text, {status:response.status, headers:{'Content-Type':'application/json'}});
      };
      mf = new Miniflare(options(resolve(directory,'workerd'),native.data,bridge,{},profile));
      const identity = await mf.getWorker('identity');
      const headers = {Cookie:native.data.cookie, Origin:'https://localhost', 'Content-Type':'application/json', 'X-Oasis7-CSRF':native.data.csrf};
      const draft = {api_version:1,client_request_id:'approved-chain',plan:{...routes,purpose:'isolated approval chain',expires_at:String(Date.now()+120000),transport_limit:'3'}};
      const response = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
      assert.equal(response.status,200, 'actual draft must initialize');
      const view = await response.json();
      const request = {approval_request_id:'chain-approval',context:{environment:'test',custody_account_id:view.entry.custody_account_id,audience:native.data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},
        intent:{kind:'provision',provision_id:view.entry.provision_id,purpose:draft.plan.purpose,plan_digest:view.entry.plan_digest,bootstrap_config_version:'1',world_identity:world,
          identity_route:routes.identity_route,world_route:routes.world_route,native_identity_namespace:native.data.namespace,register_request_id:'register-chain-original',login_request_id:'login-chain-original',deadline:draft.plan.expires_at,transport_limit:draft.plan.transport_limit}};
      const proofResponse = await fetch(native.data.address+'/_fixture/approve-intent',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(request)});
      const completed = await proofResponse.json();
      assert.ok(completed.approval, 'real fixture UV completion must produce a native proof; status=' + proofResponse.status + ', code=' + (typeof completed.error_code === 'string' ? completed.error_code : 'unavailable'));
      const body = {api_version:1,input:{operation_id:'approve-chain-original',request,approval:completed.approval}};
      const url = 'https://localhost/v1/account-provisions/'+view.entry.provision_id+'/approve';
      const approve = value => identity.fetch(url,{method:'POST',headers,body:JSON.stringify(value)});
      for (const path of ['/default-approve','/default-fence']) {
        const denied = await mf.dispatchFetch('https://caller.test'+path,{method:'POST',body:JSON.stringify(body)});
        assert.equal(denied.status,409);
        assert.equal((await denied.json()).missing_rpc,true,'default binding must not expose '+path);
      }
      if (profile === 'production-frozen') {
        assert.notEqual((await approve(body)).status,200, 'real independently frozen fence must block consumption');
        assert.equal(verifies,0, 'frozen service cannot begin native consumption');
        const fence = await mf.getWorker('fence');
        assert.equal((await fence.fetch('https://fence.test/internal/recovery-fence')).status,404, 'default ingress cannot proxy private fence reads');
        continue;
      }
      if (profile.endsWith('-freeze-during-verify')) {
        assert.notEqual((await approve(body)).status,200,'a real directory freeze during native verification must block final consumption');
        assert.equal(verifies,1);
        // Read only the isolated workerd SQLite files. Return non-secret state,
        // never the proof, session context, signatures or original private bytes.
        const state = JSON.parse(execFileSync('python3',['-c',`
import json,sqlite3,sys
from pathlib import Path
found=[]
for p in Path(sys.argv[1]).rglob('*.sqlite'):
 try:
  db=sqlite3.connect(p.as_uri()+'?mode=ro',uri=True)
  row=db.execute("SELECT payload FROM custody_records WHERE id='custody-provision-approval-v1'").fetchone()
  if row:
   value=json.loads(row[0]); state=value['state']
   if state['draft']['entry']['custody_account_id']==sys.argv[2]:
    found.append({'stage':state['provision']['stage'],'accepted':state['accepted_operation_id'],'tasks':len(state['role_key_tasks']),'fenced':bool(db.execute("SELECT 1 FROM custody_records WHERE id='custody-account-freeze-v1'").fetchone())})
  db.close()
 except sqlite3.Error: pass
print(json.dumps(found))
`,resolve(directory,'workerd'),view.entry.custody_account_id],{encoding:'utf8'}));
        assert.deepEqual(state,[{stage:'draft',accepted:null,tasks:0,fenced:true}]);
        continue;
      }
      const badOrigin = await identity.fetch(url,{method:'POST',headers:{...headers,Origin:'https://other.invalid'},body:JSON.stringify(body)});
      assert.notEqual(badOrigin.status,200);
      const bad = structuredClone(body); bad.input.request.intent.purpose = 'not-approved';
      assert.notEqual((await approve(bad)).status,200);
      const accepted = await approve(body);
      assert.equal(accepted.status,200, 'complete actual approval chain with explicitly isolated fixture fence');
      const result = await accepted.json();
      assert.deepEqual(result,{api_version:1,provision_id:view.entry.provision_id,custody_account_id:view.entry.custody_account_id,operation_id:body.input.operation_id,stage:'approved'});
      assert.doesNotMatch(JSON.stringify(result),/proof|signature|session_ref|nonce|cookie/);
      assert.equal(verifies,1);
      assert.deepEqual(await (await approve(body)).json(),result, 'lost-response retry returns the original');
      assert.equal(verifies,1,'consumed retry cannot repeat native consumption');
      const repeated = await identity.fetch('https://localhost/v1/account-provisions',{method:'POST',headers,body:JSON.stringify(draft)});
      assert.equal(repeated.status,200); assert.deepEqual(await repeated.json(),view,'draft retry preserves original after approval');
      const conflict = structuredClone(body); conflict.input.operation_id = 'another-operation';
      assert.notEqual((await approve(conflict)).status,200);
      assert.equal(verifies,1,'same proof cannot authorize another operation');
      const fence = await mf.getWorker('fence'); await fence.fetch('https://fixture.test/freeze');
      assert.notEqual((await approve(body)).status,200,'a frozen service cannot use retry as fresh authority');
      await fetch(native.data.address+'/_fixture/revoke-manager',{method:'POST'});
      assert.notEqual((await approve(body)).status,200,'a revoked native manager cannot enter approval');
    } finally { await mf?.dispose(); await native?.close(); rmSync(directory,{recursive:true,force:true}); }
  }
});

// The bridge maps only the fixture's pinned HTTPS authority to loopback HTTP.
// It does not alter the production HTTPS-only configuration or emulate the native authority.
test('production Identity workerd consumes actual native HTTP/SQLite management and private verification', { timeout: 90000 }, async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-native-http-'));
  let native;
  let mf;
  try {
    native = await authority(directory);
    let fault;
    let nativeStatus;
    let bridgeStage;
    let bridgeFailure;
    const track = async (stage, operation) => {
      bridgeStage = stage;
      try { return await operation(); }
      catch (error) {
        const code = value => ['ECONNRESET', 'ECONNREFUSED', 'EPIPE', 'ETIMEDOUT', 'UND_ERR_SOCKET', 'UND_ERR_ABORTED', 'ERR_INVALID_STATE', 'ERR_STREAM_PREMATURE_CLOSE', 'ERR_STREAM_DESTROYED', 'ABORT_ERR'].includes(value) ? value : 'Other';
        bridgeFailure = { name: ['TypeError', 'Error', 'AbortError', 'TimeoutError', 'AssertionError'].includes(error?.name) ? error.name : 'Other', code: code(error?.code), cause: code(error?.cause?.code) };
        throw error;
      }
    };
    const diagnostics = () => JSON.stringify({ native: native.diagnostics(), bridgeStage, bridgeFailure, nativeStatus });
    let foreignAttempts = 0;
    const calls = [];
    const bridge = async request => {
      nativeStatus = undefined;
      bridgeStage = 'start'; bridgeFailure = undefined;
      const url = new URL(request.url);
      if (url.origin !== 'https://localhost') foreignAttempts++;
      assert.equal(url.origin, 'https://localhost');
      assert.ok(['/api/public/hosted-account/v2/custody/management-session-status', '/api/public/hosted-account/v2/custody/approvals/verify'].includes(url.pathname));
      if (url.pathname.endsWith('/management-session-status')) assert.ok(request.headers.get('cookie') === native.data.cookie, 'only current SID cookie may leave Identity');
      calls.push({ path: url.pathname, method: request.method, cookie: request.headers.has('cookie'), origin: request.headers.has('origin') });
      if (fault === 'redirect') return new MiniflareResponse(null, { status: 302, headers: { Location: 'https://other.invalid/steal' } });
      if (fault === 'oversize') return new MiniflareResponse(' '.repeat(32769), { headers: { 'Content-Type': 'application/json' } });
      const body = request.method === 'GET' ? undefined : Buffer.from(await track('request_body', () => request.arrayBuffer()));
      // Rebuild only native semantic headers. The workerd stream is now bytes;
      // Node owns its loopback Host, Content-Length and other transport metadata.
      const nativeHeaders = new Headers();
      bridgeStage = 'native_headers';
      for (const name of ['authorization', 'cookie', 'origin', 'sec-fetch-site', 'x-oasis7-csrf', 'content-type']) {
        const value = request.headers.get(name);
        if (typeof value === 'string') nativeHeaders.set(name, value);
      }
      const response = await track('native_fetch', () => fetch(native.data.address + url.pathname, { method: request.method,
        headers: nativeHeaders, body, redirect: 'manual' }));
      const text = await track('native_body', () => response.text());
      nativeStatus = response.status;
      bridgeStage = 'complete';
      if (fault === 'unknown' || fault === 'identity' || fault === 'signature' || fault === 'correlation') {
        const value = JSON.parse(text);
        if (fault === 'unknown') value.unknown = true;
        if (fault === 'identity') value.identity_namespace = 'other';
        if (fault === 'signature') value.signature = 'A'.repeat(86);
        if (fault === 'correlation') value.verification.verify_nonce = 'wrong-owner-nonce';
        return MiniflareResponse.json(value, { status: response.status });
      }
      return new MiniflareResponse(text, { status: response.status, headers: { 'Content-Type': 'application/json' } });
    };
    mf = new Miniflare(options(resolve(directory, 'workerd'), native.data, bridge));
    const identity = await mf.getWorker('identity');
    const headers = { Cookie: 'unrelated=must-stay-local; ' + native.data.cookie };
    let status = await identity.fetch('https://localhost/v1/management/session-status', { headers });
    assert.equal(status.status, 200, 'management ' + diagnostics());
    const original = await status.json();
    assert.equal(original.account_id, 'a');
    assert.equal(original.identity_namespace, native.data.namespace);
    assert.equal(original.identity_instance_epoch, native.data.identity_instance_epoch);
    assert.ok(!JSON.stringify(original).includes(native.data.cookie));
    for (const invalid of [
      { Cookie: native.data.cookie + '; ' + native.data.cookie },
      { Cookie: native.data.cookie, Origin: 'https://other.invalid' },
      { Cookie: native.data.cookie, Authorization: 'Bearer arbitrary' },
      { Cookie: native.data.cookie, 'Sec-Fetch-Site': 'cross-site' },
    ]) assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers: invalid })).status, 200);
    assert.notEqual((await identity.fetch('https://other.invalid/v1/management/session-status', { headers })).status, 200);
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status?account_id=other', { headers })).status, 200);
    for (const kind of ['redirect', 'oversize', 'unknown', 'identity']) {
      fault = kind;
      assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    }
    fault = undefined;
    assert.equal(foreignAttempts, 0, 'manual redirects must never attempt another origin');
    const before = calls.length;
    let verify = await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) });
    assert.equal(verify.status, 200, 'private verification native dispatches=' + (calls.length - before) + ', ' + diagnostics());
    const signed = await verify.json();
    const dispatchCount = calls.length;
    const defaultRpc = await mf.dispatchFetch('https://caller.test/default-verify', { method: 'POST', body: JSON.stringify(native.data.verify_request) });
    assert.equal(defaultRpc.status, 409);
    assert.equal((await defaultRpc.json()).missing_rpc, true, 'default binding must not expose generated Rust RPC');
    assert.equal(calls.length, dispatchCount);
    assert.equal(signed.verification.verify_nonce, native.data.verify_request.verify_nonce);
    assert.equal(signed.verification.origin_state, 'active');
    assert.equal(calls.length, before + 1);
    assert.deepEqual(calls.at(-1), { path: '/api/public/hosted-account/v2/custody/approvals/verify', method: 'POST', cookie: false, origin: false });
    for (const kind of ['redirect', 'oversize', 'unknown', 'signature', 'correlation']) {
      fault = kind;
      assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) })).status, 409);
    }
    fault = undefined;
    assert.equal(foreignAttempts, 0, 'private verify must never forward a redirect');
    const noDispatch = calls.length;
    const wrongAudience = { ...native.data.verify_request, expected_audience: 'other' };
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(wrongAudience) })).status, 409);
    const wrongIdentity = structuredClone(native.data.verify_request);
    wrongIdentity.approval.proof.identity_namespace = 'other';
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(wrongIdentity) })).status, 409);
    const extra = { ...native.data.verify_request, url: 'https://other.invalid/' };
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(extra) })).status, 409);
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: ' '.repeat(32769) })).status, 409);
    assert.equal(calls.length, noDispatch);
    // The native service-auth verify route remains unavailable through public fetch.
    assert.equal((await identity.fetch('https://localhost/v1/approvals/verify', { method: 'POST', body: '{}' })).status, 404);
    // Real Passkey cookie + original browser CSRF/Origin must authorize the private
    // Identity -> Owner -> per-account SQLite draft initialization, without world calls.
    const draft = { api_version: 1, client_request_id: 'native-authenticated-draft',
      plan: { ...routes, purpose: 'native auth integration only', expires_at: String(Date.now() + 120000), transport_limit: '3' } };
    const writeHeaders = { ...headers, Origin: 'https://localhost', 'Content-Type': 'application/json', 'X-Oasis7-CSRF': native.data.csrf };
    for (const invalid of [
      { ...writeHeaders, 'X-Oasis7-CSRF': '0'.repeat(64) },
      { ...writeHeaders, Origin: 'https://other.invalid' },
    ]) assert.notEqual((await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: invalid, body: JSON.stringify(draft) })).status, 200);
    const initialized = await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: writeHeaders, body: JSON.stringify(draft) });
    assert.equal(initialized.status, 200, 'draft initialization ' + diagnostics());
    const view = await initialized.json();
    assert.equal(view.phase, 'draft_initialized');
    assert.deepEqual(view.entry.manager, { identity_namespace: native.data.namespace, native_account_id: 'a' });
    assert.deepEqual(view.world_identity, world);
    const repeated = await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: writeHeaders, body: JSON.stringify(draft) });
    assert.equal(repeated.status, 200);
    assert.deepEqual(await repeated.json(), view);
    const read = await identity.fetch('https://localhost/v1/account-provisions/' + view.entry.provision_id, { headers });
    assert.equal(read.status, 200);
    assert.deepEqual(await read.json(), view);
    const selectedManager = { ...draft, manager: { native_account_id: 'other' } };
    assert.equal((await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers: writeHeaders, body: JSON.stringify(selectedManager) })).status, 400);
    let configCase = 0;
    for (const invalid of [
      { CUSTODY_NATIVE_ORIGIN: 'http://localhost' },
      { CUSTODY_NATIVE_ORIGIN: 'https://localhost/untrusted-path' },
      { CUSTODY_NATIVE_RP_ID: 'other.invalid' },
      { CUSTODY_NATIVE_NAMESPACE: 'other' },
      { CUSTODY_NATIVE_INSTANCE_EPOCH: '99' },
      { CUSTODY_NATIVE_AUDIENCE: 'other' },
      { CUSTODY_NATIVE_APPROVAL_PUBLIC_KEY: '01' + '00'.repeat(31) },
    ]) {
      const noDispatch = calls.length;
      const invalidWorker = new Miniflare(options(resolve(directory, 'config-' + configCase++), native.data, bridge, invalid));
      try {
        assert.equal((await invalidWorker.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) })).status, 409);
        assert.equal(calls.length, noDispatch, 'configuration/binding rejection must precede native dispatch');
      } finally { await invalidWorker.dispose(); }
    }
    await fetch(native.data.address + '/_fixture/native-key', { method: 'POST' });
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    await fetch(native.data.address + '/_fixture/reset', { method: 'POST' });
    assert.equal((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    await fetch(native.data.address + '/_fixture/expire', { method: 'POST' });
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers })).status, 200);
    const inspect = { ...native.data.verify_request, purpose: 'inspect_origin', verify_nonce: 'owner-origin-inspection' };
    const inspected = await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(inspect) });
    assert.equal(inspected.status, 200);
    assert.notEqual((await inspected.json()).verification.origin_state, 'active');
    assert.equal((await mf.dispatchFetch('https://caller.test/', { method: 'POST', body: JSON.stringify(native.data.verify_request) })).status, 409);
  } finally {
    try { if (mf) await mf.dispose(); }
    finally {
      try { if (native) await native.close(); }
      finally { rmSync(directory, { recursive: true, force: true }); }
    }
  }
});

// Actual native transaction/outbox, scheduled Identity handler, Owner SQLite closure,
// and native acknowledgement. Only the HTTPS-to-loopback transport is a local bridge.
test('revoked native manager reaches Owner closure without its revoked browser session', { timeout: 90000 }, async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'oasis7-custody-native-revocation-'));
  let native; let mf;
  try {
    native = await authority(directory); const calls = [];
    const bridge = async request => {
      const url = new URL(request.url); assert.equal(url.origin, 'https://localhost');
      assert.ok(['management-session-status', 'revocations/poll', 'revocations/ack'].some(path => url.pathname === '/api/public/hosted-account/v2/custody/' + path));
      if (url.pathname.includes('/revocations/')) { assert.equal(request.headers.has('cookie'), false); assert.equal(request.headers.has('origin'), false); }
      calls.push(url.pathname);
      const headers = new Headers(); for (const name of ['authorization', 'cookie', 'origin', 'sec-fetch-site', 'x-oasis7-csrf', 'content-type']) {
        const value = request.headers.get(name); if (typeof value === 'string') headers.set(name, value);
      }
      const body = request.method === 'GET' ? undefined : Buffer.from(await request.arrayBuffer());
      const result = await fetch(native.data.address + url.pathname, { method: request.method, headers, body, redirect: 'manual' });
      return new MiniflareResponse(await result.text(), { status: result.status, headers: { 'Content-Type': 'application/json' } });
    };
    const create = () => new Miniflare(options(resolve(directory, 'workerd'), native.data, bridge));
    mf = create(); let identity = await mf.getWorker('identity');
    const headers = { Cookie: native.data.cookie, Origin: 'https://localhost', 'Content-Type': 'application/json', 'X-Oasis7-CSRF': native.data.csrf };
    const draft = { api_version: 1, client_request_id: 'revocation-original', plan: { ...routes, purpose: 'native revocation integration', expires_at: String(Date.now() + 120000), transport_limit: '3' } };
    const initialized = await identity.fetch('https://localhost/v1/account-provisions', { method: 'POST', headers, body: JSON.stringify(draft) });
    assert.equal(initialized.status, 200); const view = await initialized.json(); assert.equal(view.phase, 'draft_initialized');
    assert.equal((await fetch(native.data.address + '/_fixture/revoke-manager', { method: 'POST' })).status, 200);
    assert.notEqual((await identity.fetch('https://localhost/v1/management/session-status', { headers: { Cookie: native.data.cookie } })).status, 200);
    const poll = async () => {
      const result = await fetch(native.data.address + '/api/public/hosted-account/v2/custody/revocations/poll', { method: 'POST',
        headers: { Authorization: 'Bearer ' + native.data.revocation_poll_secret, 'Content-Type': 'application/json' }, body: JSON.stringify({ api_version: 1, limit: 8 }) });
      assert.equal(result.status, 200); return result.json();
    };
    const pending = await poll(); assert.ok(pending.events.length > 0);
    const outboxStatus = async () => (await fetch(native.data.address + '/_fixture/revocation-status')).json();
    assert.ok((await outboxStatus()).pending > 0);
    assert.equal((await mf.dispatchFetch('https://caller.test/pump', { method: 'POST' })).status, 204);
    assert.deepEqual((await poll()).events, []);
    const confirmed = await outboxStatus(); assert.equal(confirmed.pending, 0); assert.ok(confirmed.acknowledged > 0);
    assert.ok(calls.some(path => path.endsWith('/revocations/ack')));
    await mf.dispose(); mf = create(); identity = await mf.getWorker('identity');
    assert.equal((await mf.dispatchFetch('https://caller.test/pump', { method: 'POST' })).status, 204);
    assert.deepEqual((await poll()).events, []);
    const accountBindings = await mf.getBindings('owner');
    const account = accountBindings.ACCOUNTS.get(accountBindings.ACCOUNTS.idFromName(view.entry.custody_account_id));
    const late = await account.fetch('https://account.test/internal/draft', { method: 'POST', headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ entry: view.entry, plan: view.plan, world_identity: view.world_identity }) });
    assert.equal(late.status, 409, 'confirmed source closure must preserve Account freeze across restart');
  } finally {
    try { if (mf) await mf.dispose(); } finally { try { if (native) await native.close(); } finally { rmSync(directory, { recursive: true, force: true }); } }
  }
});
