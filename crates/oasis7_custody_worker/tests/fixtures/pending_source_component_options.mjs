// Test-only finite deployment. Call armNativeSource BEFORE the real UV child.
import assert from 'node:assert/strict';
import {resolve,relative,dirname} from 'node:path';
import {rawBrowserRoles} from './custody_browser_session_v2.mjs';
import {configurePendingExecutionComponent} from './pending_execution_component_options.mjs';
const spec=(from,to)=>{const v=relative(dirname(from),to).split('\\').join('/');return v.startsWith('.')?v:'./'+v;};
export function armNativeSource(world3){
  assert.ok(world3?.nativeEnv,'actual World3 protected fixture is mandatory');
  const config=JSON.parse(world3.nativeEnv.OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG);
  assert.ok(config.execution_prepare_pin&&config.execution_dispatch_pin);
  assert.equal(config.source_inspect_pin,undefined,'cannot silently replace a prior source configuration');
  config.source_inspect_pin={...config.execution_prepare_pin,allowed_purpose:'execution_pending_source_native_leaf_v2'};
  world3.nativeEnv.OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG=JSON.stringify(config);
  world3.nativeEnv.OASIS7_CUSTODY_EXECUTION_SOURCE_INSPECT_V2_ENABLED='true';
}
export function configurePendingSourceComponent(raw,root,fenceObjectID,ports){
  configurePendingExecutionComponent(raw,root,fenceObjectID,ports);
  const {owner,identity}=rawBrowserRoles(raw);
  const fence=raw.workers.filter(w=>w.name==='fence');assert.equal(fence.length,1);
  const pins=JSON.parse(owner.bindings.CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS);
  const original=pins.find(p=>p.allowed_purpose==='active_three_roots');assert.ok(original);
  const sourcePins=['execution_pending_source_inspect_dispatch_v2','execution_pending_source_native_leaf_v2'].map(allowed_purpose=>({...original,allowed_purpose}));
  for(const w of [owner,identity])w.bindings={...w.bindings,
    CUSTODY_EXECUTION_PENDING_SOURCE_COMPONENT_V2_TEST:'true',
    CUSTODY_EXECUTION_SOURCE_INSPECT_V2_ENABLED:'true',
    CUSTODY_EXECUTION_SOURCE_INSPECT_ISSUER_PINS:JSON.stringify(sourcePins)};
  // Four is this test deployment's finite lifetime, never a production default.
  identity.bindings.CUSTODY_EXECUTION_SOURCE_INSPECT_ADMISSION_CAPACITY='4';
  identity.durableObjects={...identity.durableObjects,EXECUTION_SOURCE_INSPECT_DISPATCHERS:{className:'ExecutionSourceInspectDispatcher',useSQLite:true}};
  identity.serviceBindings={...identity.serviceBindings,EXECUTION_SOURCE_INSPECT_ACCOUNT_GATE:{name:'owner',entrypoint:'ExecutionSourceInspectAccountGate'}};
  owner.durableObjects={...owner.durableObjects,EXECUTION_SOURCE_INSPECT_DISPATCHERS:{className:'ExecutionSourceInspectDispatcher',scriptName:'identity',useSQLite:true}};
  const floor={issuer_epoch:original.issuer_epoch,config_generation:original.config_generation};
  for(const [prefix,purpose,enabled] of [
    ['CUSTODY_EXECUTION_FENCE_ADMISSION','execution_fence_read_v2',true],
    ['CUSTODY_READ_FENCE_ADMISSION','read_fence_read_v3',false],
  ]){
    const verifier={pin:{...original,allowed_purpose:purpose},floor};
    Object.assign(fence[0].bindings,{[prefix+'_CAPACITY']:'4',[prefix+'_VERIFIER_HISTORY']:JSON.stringify([verifier]),[prefix+'_ACTIVE_VERIFIER']:JSON.stringify(verifier),[prefix+'_ENABLED']:String(enabled)});
  }
  const sourceEntry=identity.modules[0].path;
  const sourceWrapper=resolve(root,'tests/fixtures/pending_source_identity_component.mjs');
  identity.modules=[{type:'ESModule',path:sourceWrapper,contents:`import {ExecutionSourceInspectDispatcher as Parent} from ${JSON.stringify(spec(sourceWrapper,sourceEntry))};\nexport * from ${JSON.stringify(spec(sourceWrapper,sourceEntry))};\nexport {default} from ${JSON.stringify(spec(sourceWrapper,sourceEntry))};\nexport class ExecutionSourceInspectDispatcher extends Parent {async fetch(request){console.warn('fixture.pending_action.source_dispatcher');return super.fetch(request);}}\n`},...identity.modules];
  assert.ok(owner.modules[0].contents&&owner.modules[1]?.path);
  owner.modules[0].contents+=`\nexport {default} from ${JSON.stringify(spec(owner.modules[0].path,owner.modules[1].path))};\n`;
  const ownerEntry=owner.modules[0].path;
  const ownerWrapper=resolve(root,'tests/fixtures/pending_source_owner_component.mjs');
  owner.modules=[{type:'ESModule',path:ownerWrapper,contents:`import {ExecutionSourceInspectAccountGate as ParentGate,AccountAuthority as ParentAccount} from ${JSON.stringify(spec(ownerWrapper,ownerEntry))};\nexport * from ${JSON.stringify(spec(ownerWrapper,ownerEntry))};\nexport {default} from ${JSON.stringify(spec(ownerWrapper,ownerEntry))};\nexport class ExecutionSourceInspectAccountGate extends ParentGate {async claim(raw){console.warn('fixture.pending_action.source_owner');return super.claim(raw);}}\nexport class AccountAuthority extends ParentAccount {async fetch(request){if(new URL(request.url).pathname==='/internal/execution-pending-source-native-claim-v2')console.warn('fixture.pending_action.source_account');return super.fetch(request);}}\n`},...owner.modules];
  // Native physical receipt is read from the actual TCP fixture's safe facts.
}
