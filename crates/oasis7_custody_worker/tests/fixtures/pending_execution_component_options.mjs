// Actual signed-caller component only. Configuration occurs on raw V4 once.
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve,relative,dirname} from 'node:path';
import {rawBrowserRoles} from './custody_browser_session_v2.mjs';
const spec=(from,to)=>{const value=relative(dirname(from),to).split('\\').join('/');return value.startsWith('.')?value:'./'+value;};
export function configurePendingExecutionComponent(raw,root,fenceObjectID,ports){
 const previous=raw.handleStructuredLogs;raw.handleStructuredLogs=record=>{previous?.(record);const message=typeof record?.message==='string'?record.message.trim():'';for(const key of Object.keys(ports))if(message===`fixture.pending_action.${key}`||(['provider','realm'].includes(key)&&message===`fixture.tool_nonce.${key}`))ports[key]++;};
 const roles=rawBrowserRoles(raw);const owner=roles.owner,identity=roles.identity;
 const fence=raw.workers.filter(w=>w.name==='fence');assert.equal(fence.length,1);const f=fence[0];
 const pins=JSON.parse(owner.bindings.CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS);
 assert.ok(pins.length>0&&owner.bindings.CUSTODY_ACCOUNT_SERVICE_SIGNING_SEED,'actual World3 issuer initialized before component');
 const newPins=['execution_pending_current_leaf_v2','execution_fence_dispatch_v2','execution_fence_read_v2'].map(allowed_purpose=>({...pins[0],allowed_purpose}));
 const bindings={CUSTODY_EXECUTION_PENDING_START_V2_ENABLED:'true',CUSTODY_EXECUTION_PENDING_COMPONENT_V2_TEST:'true',CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS:JSON.stringify([...pins,...newPins])};
 for(const worker of [owner,identity,f])worker.bindings={...worker.bindings,...bindings,...(fenceObjectID?{CUSTODY_EXECUTION_FENCE_OBJECT_ID:fenceObjectID}:{})};
 owner.bindings.CUSTODY_EXECUTION_DIRECTORY_ADMISSION_CAPACITY='4';
 // A test deployment's finite four-admission lifetime, never a production default.
 f.bindings.CUSTODY_EXECUTION_FENCE_ADMISSION_CAPACITY='4';
 f.durableObjects={...f.durableObjects,EXECUTION_FENCE_DISPATCHERS:{className:'ExecutionFenceDispatcher',useSQLite:true}};
 f.serviceBindings={...f.serviceBindings,EXECUTION_FENCE_ACCOUNT_GATE:{name:'owner',entrypoint:'ExecutionFenceAccountGate'}};
 owner.durableObjects={...owner.durableObjects,EXECUTION_FENCE_DISPATCHERS:{className:'ExecutionFenceDispatcher',scriptName:'fence',useSQLite:true}};
 const ownerWasm=resolve(root,'artifacts/approval-harness/index.js');
 assert.ok(owner.modules.some(m=>m.path===ownerWasm),'real approval-harness artifact alias is mandatory');
 const ownerEntry=owner.modules[0].path;const wrapper=resolve(root,'tests/fixtures/pending_execution_owner_component.mjs');
 const contents=`import RustWorker from ${JSON.stringify(spec(wrapper,ownerWasm))};\nimport {OwnerNativePermits as Parent,AccountAuthority as ParentAccount,ManagerAssociations as ParentDirectory,ExecutionFenceAccountGate as ParentGate} from ${JSON.stringify(spec(wrapper,ownerEntry))};\nexport * from ${JSON.stringify(spec(wrapper,ownerEntry))};\nexport class OwnerNativePermits extends Parent {async pendingExecutionFirst(raw){console.warn('fixture.pending_action.owner');return new RustWorker(this.ctx,this.env).owner_pending_execution_component_v2(this.env,raw);}}\nexport class AccountAuthority extends ParentAccount {async fetch(request){const p=new URL(request.url).pathname;if(p==='/internal/execution-pending-component-v2')console.warn('fixture.pending_action.account');if(p==='/internal/execution-pending-fence-read-claim-v2')console.warn('fixture.pending_action.causal_account');return super.fetch(request);}}\nexport class ManagerAssociations extends ParentDirectory {async fetch(request){if(new URL(request.url).pathname.includes('execution-pending-directory'))console.warn('fixture.pending_action.directory');return super.fetch(request);}}\nexport class ExecutionFenceAccountGate extends ParentGate {async claim(raw){console.warn('fixture.pending_action.causal_owner');return super.claim(raw);}}\n`;
 owner.modules=[{type:'ESModule',path:wrapper,contents},...owner.modules];
 const fenceEntry=f.modules[0].path;const fenceWrapper=resolve(root,'tests/fixtures/pending_execution_fence_component.mjs');
 f.modules=[{type:'ESModule',path:fenceWrapper,contents:`import {RecoveryFence as ParentFence,ExecutionFenceDispatcher as ParentDispatcher} from ${JSON.stringify(spec(fenceWrapper,fenceEntry))};\nexport * from ${JSON.stringify(spec(fenceWrapper,fenceEntry))};\nexport {default} from ${JSON.stringify(spec(fenceWrapper,fenceEntry))};\nexport class ExecutionFenceDispatcher extends ParentDispatcher {async fetch(request){console.warn('fixture.pending_action.dispatcher');return super.fetch(request);}}\nexport class RecoveryFence extends ParentFence {async fetch(request){if(new URL(request.url).pathname==='/internal/execution-fence-read-v2')console.warn('fixture.pending_action.fences');return super.fetch(request);}}\n`},...f.modules];
 // Keep the exact trusted physical-provider adapter and all existing observers.
 const oldRoot=resolve(root,'artifacts/identity')+'/',newRoot=resolve(root,'artifacts/native-harness')+'/';
 let adapter=0;
 identity.modules=identity.modules.map(module=>{
  if(module.path.startsWith(oldRoot))return {...module,path:newRoot+module.path.slice(oldRoot.length)};
  if(module.path===resolve(root,'tests/fixtures/custody_browser_provider_adapter.mjs')){
   let contents=module.contents??readFileSync(module.path,'utf8');
   assert.equal(contents.split('../../artifacts/identity/').length-1,2);contents=contents.replaceAll('../../artifacts/identity/','../../artifacts/native-harness/');
   const marker='  return new RustWorker(ctx,env).fetch(trusted);';assert.ok(contents.includes(marker));adapter++;
   contents=contents.replace(marker,`  if(env.CUSTODY_EXECUTION_PENDING_COMPONENT_V2_TEST==='true'&&request.method==='POST'&&new URL(request.url).pathname.endsWith('/client/actions')){\n    console.warn('fixture.pending_action.public');\n    try{return new Response(await new RustWorker(ctx,env).identity_pending_execution_component_v2(env,trusted),{headers:{'Content-Type':'application/json'}});}catch{return Response.json({error_code:'component_unavailable'},{status:409});}\n  }\n`+marker);
   return {...module,contents};
  }
  return module;
 });assert.equal(adapter,1,'exact trusted provider bridge only');
}
