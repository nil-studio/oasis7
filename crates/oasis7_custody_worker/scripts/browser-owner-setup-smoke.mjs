#!/usr/bin/env node
// Actual local-only Native SQLite + workerd + Chrome UV. No production claims.
import assert from 'node:assert/strict';
import {mkdtempSync,chmodSync,readFileSync,writeFileSync,existsSync} from 'node:fs';
import {tmpdir} from 'node:os';import {resolve,dirname,join} from 'node:path';import {fileURLToPath} from 'node:url';
import {spawn} from 'node:child_process';import {setTimeout as delay} from 'node:timers/promises';import {createRequire} from 'node:module';
import {Miniflare,Response as MiniflareResponse} from 'miniflare';
import {worldAuthority} from '../tests/fixtures/world_runtime_support.mjs';
import {browserUvRoleOptions} from '../tests/fixtures/role_runtime_native_support.mjs';
import {browserHttps,normalizeBrowserPath} from '../tests/fixtures/custody_browser_https.mjs';
import {approveProvision,installActualGetFault,actualGetFacts} from '../tests/fixtures/custody_browser_actions.mjs';
import {sessionV2BrowserOptions,acceptSessionV2,withActualHostDescriptor,rawBrowserRoles} from '../tests/fixtures/custody_browser_session_v2.mjs';
import {world3BrowserFixture} from '../tests/fixtures/custody_browser_world3.mjs';
import {actualHostDriver} from '../tests/fixtures/custody_browser_host_driver.mjs';
import {exerciseV2PublicCredentials} from '../tests/fixtures/custody_browser_v2_credentials.mjs';
import {signFixtureRecoveryActivation} from '../tests/fixtures/fence_activation_signer.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');const repo=resolve(root,'../..');
const caseIndex=process.argv.indexOf('--case');const faultCase=caseIndex<0?'positive':process.argv[caseIndex+1];
assert.ok(['positive','lost-complete','lost-owner-approve','wrong-ids','wrong-csrf','cancel-get','cancel-expiry-recover','logout-late-get','account-switch-late-get','session-v2-activation','session-v2-claim','session-v2-claim-lost-reply','session-v2-host-control','session-v2-execution-nonce-five','session-v2-world3-ready'].includes(faultCase),'closed --case selector');
const runtimeDiagnostic={missing_module:false,missing_owner_management_export:false,duplicate_module:false,entrypoint_class:false,module_syntax:false,options_validation:false};
function classifyRuntime(text){
  if(typeof text!=='string')return;
  runtimeDiagnostic.missing_module ||= /No such module|Cannot find module|Unable to resolve|Could not resolve/.test(text);
  runtimeDiagnostic.missing_owner_management_export ||= text.includes('OwnerManagement')&&/export/i.test(text);
  runtimeDiagnostic.duplicate_module ||= /Duplicate module|Multiple modules.*same|already.*module/i.test(text);
  runtimeDiagnostic.entrypoint_class ||= /entrypoint|WorkerEntrypoint/i.test(text)&&/constructor|class|extend/i.test(text);
  runtimeDiagnostic.module_syntax ||= /SyntaxError|syntax error|unexpected token|already been declared/i.test(text);
  runtimeDiagnostic.options_validation ||= /MiniflareOptions|validation|ZodError/i.test(text);
}
const nativeStartupDiagnostic={exited:false,exit_code:null,signal:null,stderr_truncated:false,panic:false,world3_configuration:false,public_fixture_ca:false,fixture_origin:false,fixture_panic_line:null};
let nativeStderr='';
function classifyNativeStartup(chunk){
  const text=chunk.toString('utf8');
  const remaining=65536-nativeStderr.length;
  if(text.length>remaining)nativeStartupDiagnostic.stderr_truncated=true;
  if(remaining>0)nativeStderr+=text.slice(0,remaining);
  nativeStartupDiagnostic.panic ||= /panicked at/.test(nativeStderr);
  nativeStartupDiagnostic.world3_configuration ||= nativeStderr.includes('fixed World3 machine pins before MFA');
  nativeStartupDiagnostic.public_fixture_ca ||= nativeStderr.includes('public fixture CA bytes')||nativeStderr.includes('explicit fixture CA');
  nativeStartupDiagnostic.fixture_origin ||= nativeStderr.includes('explicit HTTPS loopback origin');
  const line=nativeStderr.match(/custody_browser_uv_fixture\.rs:(\d+):\d+/);
  if(line&&Number.isSafeInteger(Number(line[1])))nativeStartupDiagnostic.fixture_panic_line=Number(line[1]);
}
let moduleDiagnostic=null;let nonceFiveDiagnostic=null;
const directory=mkdtempSync(join(tmpdir(),'oasis7-browser-uv-'));chmodSync(directory,0o700);
let world,https,native,mf,browser,context,page,hostDriver,world3;let stage='startup';let exited=false;let ownerContinueDiagnostic=null;let sessionContinueDiagnostic=null;const enrollmentDiagnostics={};const enrollmentActorStages=Object.fromEntries(['received','host_absent','host_ambiguous','host_expected','host_unexpected','transfer_encoding_present','content_encoding_present','trailer_present','length_expected','length_unexpected','status_200','status_409','status_other','status_threw'].map(k=>[k,0]));const enrollmentProofStages=Object.fromEntries(['read','decode','physical','prepare','precrypto','finalinventory','finalprepare','verifyReceipt','stateProve','update'].map(k=>[k,0]));const enrollmentPopErrors=Object.fromEntries(['Bounds','Target','Header','Binding','Digest','Time','Signature','Crypto'].map(k=>[k,0]));
const world3Ready=faultCase==='session-v2-world3-ready';
const credentialProfileStages=Object.fromEntries(['first_commit','missing','credential_five','wrong','inspection_error','no_unique_source'].map(key=>[key,0]));const credentialSourceStages=Object.fromEntries(['admit','claim_source','record_source','observe','claim_lifetime','complete'].flatMap(action=>['entered','returned','threw'].map(stage=>[action+'.'+stage,0])));const nonceFive=faultCase==='session-v2-execution-nonce-five';const noncePorts={public:0,provider:0,realm:0,owner:0,account:0};const sessionInspectionStages=Object.fromEntries(['entered','status_200','status_409','status_other','threw'].map(key=>[key,0]));const sessionPreflightStages=Object.fromEntries(['describe_preflight','claim_source','record_source','get_permit','claim_step','reserve_source_read','record_access','record_lifetime','publish_owner_read','activate'].flatMap(action=>['entered','returned','threw'].map(stage=>[action+'.'+stage,0])));const ownerPrepareStages={received_v2:0,returned_text:0,returned_nontext:0,closed_v2:0,exact_original:0,valid_context:0,rpc_failed:0};
const sessionV2=['session-v2-activation','session-v2-claim','session-v2-claim-lost-reply','session-v2-host-control','session-v2-execution-nonce-five','session-v2-world3-ready'].includes(faultCase);
const recovering=faultCase==='cancel-expiry-recover', cancelling=faultCase==='cancel-get'||recovering;let recoveryFacts=null;
const ready=resolve(directory,'native-ready.json'),stop=resolve(directory,'native-stop');
try {
  stage='world';world=await worldAuthority(directory);
  https=await browserHttps(directory,resolve(repo,'crates/oasis7_viewer/software_safe_src'));
  const origin=https.origin;
  if(world3Ready)world3=await world3BrowserFixture(directory,origin);
  stage='native';const binary=readFileSync(resolve(root,'artifacts/native-test-binary.txt'),'utf8').trim();
  native=spawn('rtk',['proxy',binary,'serve_custody_browser_uv_fixture','--ignored','--nocapture','--test-threads=1'],{
    env:{PATH:process.env.PATH,...Object.fromEntries(Object.entries(process.env).filter(([k])=>!k.startsWith('OASIS7_'))),OASIS7_CUSTODY_BROWSER_READY:ready,OASIS7_CUSTODY_BROWSER_STOP:stop,
      OASIS7_CUSTODY_BROWSER_ORIGIN:origin,OASIS7_CUSTODY_BROWSER_SESSION_V2:sessionV2?'true':'false',OASIS7_CUSTODY_BROWSER_WORLD_TRUST:JSON.stringify(world.data),...(world3?.nativeEnv??{})},stdio:['ignore','ignore','pipe']});native.stderr.on('data',classifyNativeStartup);native.once('exit',(code,signal)=>{exited=true;nativeStartupDiagnostic.exited=true;nativeStartupDiagnostic.exit_code=Number.isInteger(code)?code:null;nativeStartupDiagnostic.signal=['SIGABRT','SIGTERM','SIGKILL','SIGSEGV'].includes(signal)?signal:null;});
  for(let i=0;!existsSync(ready);i++){assert.ok(!exited&&i<1500,'Native browser fixture readiness failed');await delay(20);}
  const data=JSON.parse(readFileSync(ready,'utf8'));https.setNative(data.address);
  stage='browser-launch';const modules=process.env.OASIS7_PLAYWRIGHT_NODE_MODULES;
  const require=modules?createRequire(join(modules,'local-browser-uv.cjs')):createRequire(import.meta.url);browser=await require('playwright').chromium.launch({channel:'chrome',headless:!process.argv.includes('--headed')});
  context=await browser.newContext({ignoreHTTPSErrors:true,viewport:{width:1280,height:960}});page=await context.newPage();
  const cdp=await context.newCDPSession(page);await cdp.send('WebAuthn.enable');const {authenticatorId}=await cdp.send('WebAuthn.addVirtualAuthenticator',{options:{protocol:'ctap2',transport:'internal',hasResidentKey:true,hasUserVerification:true,isUserVerified:true,automaticPresenceSimulation:true}});
  stage='browser-register';await page.goto(origin+'/');await page.getByRole('button',{name:'免邮箱创建账号',exact:true}).click();await page.getByRole('heading',{name:'账号与安全',exact:true}).waitFor();
  const manager=await page.evaluate(async()=>{const r=await fetch('/api/public/hosted-account/v2/session');const s=await r.json();if(s.authentication.method!=='webauthn')throw Error('actual UV required');return s.account.account_id;});
  const capability=await page.evaluate(async()=>fetch('/api/public/hosted-account/v2/capabilities').then(r=>r.json()));
  const cookies=await context.cookies(origin);const sid=cookies.find(c=>c.name==='__Host-oasis7_sid');assert.ok(sid,'actual browser SID required');
  const sourceHeaders={Cookie:`${sid.name}=${sid.value}`,Origin:origin,'Content-Type':'application/json','X-Oasis7-CSRF':capability.csrf_token};
  stage='manager-bootstrap';const enrolled=await fetch(data.address+'/_fixture/browser-manager',{method:'POST',headers:sourceHeaders,body:'{}'});assert.equal(enrolled.status,200,'fixed actual browser manager bootstrap failed');
  assert.equal((await enrolled.json()).manager_account_id,manager);
  const nativeCounts={};const bridge=async request=>{
    const url=new URL(request.url);assert.equal(url.origin,origin,'fixed local Native origin');assert.ok(url.pathname.startsWith('/api/public/hosted-account/v2/'));
    const nativePath=normalizeBrowserPath(url.pathname);nativeCounts[nativePath]=(nativeCounts[nativePath]??0)+1;
    const headers=new Headers();for(const key of ['authorization','cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type']){const v=request.headers.get(key);if(v!==null)headers.set(key,v);}
    const response=await fetch(data.address+url.pathname,{method:request.method,headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())}),redirect:'manual'});
    return new MiniflareResponse(await response.arrayBuffer(),{status:response.status,headers:response.headers});
  };
  stage='workerd';const runtimeData={...data,world:world.data.world,adapter_secret:'34'.repeat(32),revocation_poll_secret:'53'.repeat(32),revocation_ack_secret:'54'.repeat(32)};
  const makeRoleOptions=(actualHostDescriptor=null)=>browserUvRoleOptions(resolve(directory,'workerd'),runtimeData,bridge,manager,origin,resolve(root,'tests/fixtures/custody_browser_provider_adapter.mjs'),raw=>{
    const roles=rawBrowserRoles(raw);
    raw.unsafeProxySharedSecret=https.proxySecret;
    if(sessionV2)sessionV2BrowserOptions(raw,root,origin);
    if(actualHostDescriptor)withActualHostDescriptor(raw,actualHostDescriptor);
    if(nonceFive){
      raw.handleStructuredLogs=record=>{const name=Object.keys(noncePorts).find(n=>record.message.trim()===`fixture.tool_nonce.${n}`);if(name)noncePorts[name]++;};
      for(const w of Object.values(roles)){
        w.bindings={...w.bindings,CUSTODY_EXECUTION_PREPARE_DISPATCH_V2_ENABLED:'true'};
        const file=w.name==='identity'?'execution_nonce_five_browser_identity.mjs':'tool_nonce_owner_runtime.mjs';
        w.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures',file)},...w.modules];
      }
    }
    if(sessionV2){
      const previous=raw.handleStructuredLogs;
      raw.handleStructuredLogs=record=>{classifyRuntime(record?.message);previous?.(record);const message=typeof record?.message==='string'?record.message.trim():'';for(const [prefix,counts] of [['fixture.credential_profile.',credentialProfileStages],['fixture.credential_source.',credentialSourceStages],['fixture.session_inspection.',sessionInspectionStages],['fixture.session_preflight.',sessionPreflightStages],['fixture.enrollment_actor.',enrollmentActorStages],['enrollment.prove.',enrollmentProofStages],['enrollment.prove.http_pop.',enrollmentPopErrors]]){const key=Object.keys(counts).find(k=>message===prefix+k);if(key)counts[key]++;}const name=Object.keys(ownerPrepareStages).find(n=>record.message.trim()===`fixture.owner_v2_prepare.${n}`);if(name)ownerPrepareStages[name]++;};
      // The observer imports this wrapper for every Session case, not only
      // NonceFive. Register its dependency once without selecting a new entry.
      const nonceWrapper=resolve(root,'tests/fixtures/tool_nonce_owner_runtime.mjs');
      if(!roles.owner.modules.some(module=>module.path===nonceWrapper))roles.owner.modules.push({type:'ESModule',path:nonceWrapper});
      assert.equal(roles.owner.modules.filter(module=>module.path===resolve(root,'tests/fixtures/native_owner_result_loss.mjs')).length,1,'observer base dependency registered exactly once');
      roles.owner.modules=[{type:'ESModule',path:resolve(root,'tests/fixtures/owner_v2_prepare_downstream_observer.mjs')},...roles.owner.modules];
    }
    if(process.env.OASIS7_TEST_SESSION_OWNER_DIAGNOSTIC==='true'){
      const ownerRoot=resolve(root,'artifacts/owner')+'/';
      const harnessRoot=resolve(root,'artifacts/approval-harness')+'/';let wrappers=0;
      roles.owner.modules=roles.owner.modules.map(module=>{
        if(module.path.startsWith(ownerRoot))return {...module,path:harnessRoot+module.path.slice(ownerRoot.length)};
        if(module.path===resolve(root,'tests/fixtures/native_owner_result_loss.mjs')){
          const contents=readFileSync(module.path,'utf8'),marker='../../artifacts/owner/entry.mjs';
          assert.equal(contents.split(marker).length-1,2,'exact actual Owner wrapper imports must select current diagnostic artifact');wrappers++;
          return {...module,contents:contents.replaceAll(marker,'../../artifacts/approval-harness/entry.mjs')};
        }
        return module;
      });
      assert.equal(wrappers,1,'actual Owner wrapper must be redirected once');
      const previous=raw.handleStructuredLogs;
      raw.handleStructuredLogs=record=>{previous?.(record);let value;try{value=JSON.parse(record?.message);}catch{return;}
        const bits=value?.session_source_predicates;const allowed=["session_present", "record_present", "recorded_context_valid", "input_digest_available", "source_present", "state_not_frozen", "current_not_frozen", "fence_epoch_valid", "authority_equal", "source_version_two", "approved_input_equal", "source_unclaimed", "before_source_expiry", "guards_pass", "native_request_build_valid", "request_encoding_valid", "journal_append_valid", "post_claim_persistence_valid"];if(Array.isArray(bits)&&bits.every(v=>Array.isArray(v)&&v.length===2&&allowed.includes(v[0])&&typeof v[1]==='boolean'))console.error(JSON.stringify({source_claim_predicates:bits}));
        const f=value?.activation_failure;const stages=['source.observe','sql.initial_read','sql.restore','sql.current','sql.clock','source.core.admission','source.core.provision','source.core.binding','source.core.context_bound','source.core.conflict','source.core.persistence','sql.transition','sql.inventory_read','sql.inventory_write'];
        if(f&&stages.includes(f.stage))console.error(JSON.stringify({session_source_failure:{stage:f.stage,elapsed_ms:Number.isSafeInteger(f.elapsed_ms)&&f.elapsed_ms>=0?f.elapsed_ms:null}}));
      };
    }
    if(world3)world3.configure(raw,data,bridge);
    moduleDiagnostic=Object.fromEntries(Object.entries(roles).map(([name,worker])=>{const paths=worker.modules.map(m=>m.path).filter(p=>typeof p==='string');return[name,{registered_modules:paths.length,duplicate_paths:paths.length-new Set(paths).size}];}));
    return raw;
  });
  const roleOptions=makeRoleOptions();
  mf=new Miniflare(roleOptions);
  world3?.setIdentity(async request=>mf.dispatchFetch(request.url,{method:request.method,headers:request.headers,body:Buffer.from(await request.arrayBuffer())}));
  https.setIdentity(async request=>mf.dispatchFetch(request.url,{method:request.method,headers:request.headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())})}));
  const controller=await mf.getWorker('fence-controller');const description=await(await controller.fetch('https://fixture.test/describe',{method:'POST'})).json();
  assert.equal((await controller.fetch('https://fixture.test/activate',{method:'POST',body:JSON.stringify(signFixtureRecoveryActivation(description,'browser-fixture-bootstrap'))})).status,200,'real Fence activation');
  stage='parent-precondition';
  const draft={api_version:1,client_request_id:'browser-parent',plan:{environment:'test',identity_route:'native-one',world_route:'world-route-one',world_identity:world.data.world.world_id,purpose:'Local browser precondition fixture',expires_at:String(Date.now()+((recovering||sessionV2)?360000:180000)),transport_limit:'400'}};
  const identity=await mf.getWorker('identity');const created=await identity.fetch(origin+'/v1/account-provisions',{method:'POST',headers:sourceHeaders,body:JSON.stringify(draft)});assert.equal(created.status,200,'parent draft failed');const original=await created.json();
  const request={approval_request_id:'browser-parent-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},
    intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world.data.world,identity_route:'native-one',world_route:'world-route-one',native_identity_namespace:data.namespace,register_request_id:'browser-parent-register',login_request_id:'browser-parent-login',deadline:draft.plan.expires_at,transport_limit:'400'}};
  await approveProvision(page,request);
  const continued=await identity.fetch(origin+`/v1/account-provisions/${original.entry.provision_id}/continue`,{method:'POST',headers:sourceHeaders,body:JSON.stringify({api_version:1,operation_id:'browser-parent-participant'})});assert.equal(continued.status,200,'parent participant continuation failed');assert.equal((await continued.json()).stage,'participant_bound','actual parent fixture must reach participant_bound');
  if(sessionV2) {
    const hostCase=faultCase==='session-v2-host-control';
    if(hostCase) hostDriver=actualHostDriver({directory,fixtureSource:resolve(root,'tests/fixtures'),port:Number(new URL(origin).port)});
    const liveIdentity={fetch:(...args)=>https.clientFetch(...args)};
    const hostEnrollment=hostCase?async({target})=>{
      stage='v2-actual-host-identity';const descriptor=await hostDriver.initialize({target,origin});
      await mf.setOptions(makeRoleOptions(descriptor));
      const retained=await liveIdentity.fetch(origin+`/v1/account-provisions/${encodeURIComponent(original.entry.provision_id)}/progress`,{headers:sourceHeaders});
      assert.equal(retained.status,200,'actual Account draft/invite must survive local allowlist reconfiguration');
      stage='v2-actual-host-enroll';return hostDriver.rpc('enroll',{});
    }:undefined;
    const facts=await acceptSessionV2({page,identity:liveIdentity,origin,sourceHeaders,parent:original,world:world.data.world,persist:resolve(directory,'workerd'),setStage:value=>{stage=value;},nativeCounts,hostEnrollment,world3Ready,expectedAudience:data.audience,onOwnerContinue:value=>{ownerContinueDiagnostic=value;},onSessionContinue:value=>{sessionContinueDiagnostic=value;},onEnrollment:value=>{if(["invite","enroll","prove"].includes(value.phase))enrollmentDiagnostics[value.phase]={status:Number.isInteger(value.status)?value.status:null,api1:value.api1===true,status_proved:value.status_proved===true,descriptor_match:value.descriptor_match===true};},
      onActivated:hostCase?args=>hostDriver.exercise(args):nonceFive?args=>exerciseExecutionNonceFive(args,noncePorts):['session-v2-activation','session-v2-world3-ready'].includes(faultCase)?undefined:args=>exerciseV2PublicCredentials({...args,lostReply:faultCase==='session-v2-claim-lost-reply'})});
    const nativeFacts=await(await fetch(data.address+'/_fixture/browser-facts')).json();
    if(world3Ready){assert.equal(nativeFacts.world3_enabled,true);assert.equal(nativeFacts.protected_storage_ready,true);assert.equal(world3.facts().callback_requests,0,'activation does not pretend to execute Native45');}
    assert.equal(nativeFacts.bypass_attempts,0);assert.equal(https.facts().bypass_attempts,0);
    const sum=suffix=>Object.entries(https.facts().counts).filter(([key])=>key.endsWith(suffix)).reduce((n,[,v])=>n+v,0);
    assert.equal(sum('/custody/approvals/complete'),3,'parent, OwnerV2 and SessionV2 each use a separate actual custody UV');
    assert.equal(sum('/owner-setups/approve'),1);assert.equal(sum('/test-sessions/approve'),1);
    writeFileSync(resolve(directory,'safe-evidence.json'),JSON.stringify({status:'passed',case:faultCase,...facts,mfa_bypass_attempts:0,http_counts:https.facts().counts,native_counts:nativeCounts},null,2),{mode:0o600});
    console.log(JSON.stringify({status:'passed',case:faultCase,evidence_directory:directory,kind:'local_native_workerd_browser_uv_v2'}));
  } else {
  stage='mounted-refresh';await page.evaluate(()=>window.surface.refresh());stage='metadata-load';await page.getByRole('button',{name:'加载本人可见父项',exact:true}).click();
  stage='metadata-select';
  await page.getByRole('button',{name:'为此父项准备批准摘要',exact:true}).waitFor();await page.getByLabel(/本次私有目标/).fill('Public local fixture: Move business 0 only');
  const delayed=['logout-late-get','account-switch-late-get'].includes(faultCase);
  await installActualGetFault(page,delayed?'late':cancelling?'abort':'count');
  stage='plan-prepare';await page.getByRole('button',{name:'为此父项准备批准摘要',exact:true}).click();await page.getByRole('button',{name:'确认摘要并使用通行密钥批准',exact:true}).waitFor();
  await page.screenshot({path:resolve(directory,'desktop.png'),fullPage:true});await page.setViewportSize({width:390,height:844});await page.screenshot({path:resolve(directory,'narrow.png'),fullPage:true});
  const beforeOwner=https.facts().counts;
  const phaseCount=(counts,suffix)=>Object.entries(counts).filter(([path])=>path.endsWith(suffix)).reduce((n,[,count])=>n+count,0);
  const baseOptions=phaseCount(beforeOwner,'/custody/approvals/options'),baseComplete=phaseCount(beforeOwner,'/custody/approvals/complete'),baseApprove=phaseCount(beforeOwner,'/owner-setups/approve');
  if(['lost-complete','lost-owner-approve','wrong-ids','wrong-csrf'].includes(faultCase))https.armFault(faultCase);
  if(cancelling)await cdp.send('WebAuthn.setAutomaticPresenceSimulation',{authenticatorId,enabled:false});
  stage='owner-uv-and-approve';await page.getByRole('button',{name:'确认摘要并使用通行密钥批准',exact:true}).click();
  if(['lost-complete','lost-owner-approve','wrong-ids','wrong-csrf'].includes(faultCase)) {
    stage='explicit-original-retry';await page.getByRole('button',{name:'显式重送同一原件',exact:true}).waitFor();
    const intermediate=https.facts().fault;
    assert.equal(intermediate.calls,1,'one injected attempt before explicit retry');
    if(faultCase.startsWith('lost-')){assert.equal(intermediate.committed_reply_dropped,true,'actual 200 commit must precede withheld response body');assert.equal(intermediate.downstream_statuses[0],502,'browser must receive fixed gateway uncertainty');}
    else {assert.equal(intermediate.native_error_codes[0],faultCase==='wrong-ids'?'invalid_request':'csrf_failed','exact Native route rejection');assert.equal(intermediate.upstream_statuses[0],faultCase==='wrong-ids'?400:403);}
    await page.getByRole('button',{name:'显式重送同一原件',exact:true}).click();
    await page.getByText('OwnerSetup 已批准；Execution 仍关闭，本页没有 continue 操作。',{exact:true}).waitFor();
    const retry=https.facts().fault;assert.equal(retry.calls,2);assert.equal(retry.identical_original,true,'retry must equal original browser bytes');assert.equal(retry.upstream_statuses[1],200);if(faultCase.startsWith('lost-'))assert.deepEqual(retry.downstream_statuses,[502,200],'actual successful body withheld via gateway502, then explicit retry200');
  }else if(cancelling) {
    stage='actual-browser-abort';await page.waitForFunction(()=>window.localGetFacts?.started===1&&typeof window.localAbortGet==='function');await page.evaluate(()=>window.localAbortGet());
    await page.getByText('已停止本地通行密钥交互；服务端挑战没有被撤销，请等待原期限或由原账号核清。',{exact:true}).first().waitFor();
    const get=await actualGetFacts(page);assert.equal(get.started,1);assert.equal(get.completed,0);assert.equal(get.aborted,1);
    if(recovering) {
      const setupId=()=>page.locator('dl.custody-approval-summary dt').evaluateAll(nodes=>{
        const label=nodes.find(node=>node.textContent==='Setup 操作');if(!label?.nextElementSibling)throw Error('fixed summary field absent');return label.nextElementSibling.textContent;
      });
      const firstSetup=await setupId();
      const rawExpiry=await page.getByText(/^原挑战截止时间（Unix 毫秒）：/).textContent();
      const expiry=rawExpiry.match(/^原挑战截止时间（Unix 毫秒）：([1-9][0-9]{0,19})$/)?.[1];
      assert.ok(expiry&&BigInt(expiry)<=18446744073709551615n,'actual Native expiry must be canonical u64');
      assert.equal(await page.evaluate(expiry=>BigInt(Date.now())<BigInt(expiry),expiry),true,'first discard must precede actual expiry');
      const discard=()=>page.getByRole('button',{name:'挑战到期后丢弃未提交原件',exact:true}).click();
      stage='cancel-discard-before-expiry';const beforeEarly=https.facts().counts;await discard();
      await page.getByText('原挑战尚未到期，不能丢弃；请等待已显示的挑战期限后再操作。',{exact:true}).waitFor();
      assert.deepEqual(https.facts().counts,beforeEarly,'premature discard must send zero requests');
      assert.ok((await setupId())===firstSetup,'premature discard must preserve original summary');
      await page.screenshot({path:resolve(directory,'cancelled-narrow.png'),fullPage:true});
      stage='cancel-wait-actual-native-expiry';
      // Wait on the real displayed Native expiry. No JSON rewrite or clock override.
      await page.waitForFunction(expiry=>BigInt(Date.now())>=BigInt(expiry),expiry,{timeout:180000,polling:250});
      stage='cancel-discard-after-expiry';const beforeDiscard=https.facts().counts;await discard();
      await page.getByText('已丢弃本页未提交原件；没有撤销服务端挑战或发送新请求。请选择父项另行准备，并重新完成通行密钥批准。',{exact:true}).waitFor();
      assert.deepEqual(https.facts().counts,beforeDiscard,'expired discard must send zero requests');
      await page.getByRole('button',{name:'为此父项准备批准摘要',exact:true}).waitFor();
      await page.screenshot({path:resolve(directory,'recovered-parent-narrow.png'),fullPage:true});
      stage='cancel-expiry-explicit-new-prepare';await page.getByRole('button',{name:'为此父项准备批准摘要',exact:true}).click();
      await page.getByRole('button',{name:'确认摘要并使用通行密钥批准',exact:true}).waitFor();
      assert.ok((await setupId())!==firstSetup,'new explicit prepare must mint a new setup original');
      assert.equal(phaseCount(https.facts().counts,'/custody/approvals/complete'),baseComplete,'cancelled original must never complete');
      assert.equal(phaseCount(https.facts().counts,'/owner-setups/approve'),baseApprove,'cancelled original must never enter Owner approve');
      await cdp.send('WebAuthn.setAutomaticPresenceSimulation',{authenticatorId,enabled:true});
      stage='cancel-expiry-fresh-actual-uv';await page.getByRole('button',{name:'确认摘要并使用通行密钥批准',exact:true}).click();
      await page.getByText('OwnerSetup 已批准；Execution 仍关闭，本页没有 continue 操作。',{exact:true}).waitFor();
      await page.waitForFunction(()=>window.localGetFacts?.business_settled===2);
      await page.screenshot({path:resolve(directory,'recovered-approved-narrow.png'),fullPage:true});
      const recovered=await actualGetFacts(page);assert.equal(recovered.started,2);assert.equal(recovered.completed,1);assert.equal(recovered.aborted,1);assert.equal(recovered.returned,1);
      assert.equal(phaseCount(https.facts().counts,'/custody/approvals/complete')-baseComplete,1,'only fresh assertion completes');
      assert.equal(phaseCount(https.facts().counts,'/owner-setups/approve')-baseApprove,1,'only new original enters Owner approve');
      recoveryFacts={actual_native_expiry_waited:true,premature_discard_zero_io:true,expired_discard_zero_io:true,new_setup_original:true,fresh_actual_get:true};
    }

  }else if(delayed) {
    stage='actual-get-awaiting-scope-change';await page.waitForFunction(()=>window.localGetFacts?.completed===1&&typeof window.localReleaseGet==='function');
    await page.getByRole('button',{name:'退出登录',exact:true}).click();await page.getByRole('heading',{name:'进入 Oasis7',exact:true}).waitFor();
    if(faultCase==='account-switch-late-get') {
      await page.getByRole('button',{name:'免邮箱创建账号',exact:true}).click();await page.getByRole('heading',{name:'账号与安全',exact:true}).waitFor();
      const changed=await page.evaluate(async()=>fetch('/api/public/hosted-account/v2/session').then(r=>r.json()).then(s=>s.account.account_id));assert.ok(changed!==manager,'actual browser account must switch');
    }
    await page.evaluate(()=>window.localReleaseGet());
    await page.waitForFunction(()=>window.localGetFacts?.released===1&&window.localGetFacts?.returned===1&&window.localGetFacts?.business_started===1&&window.localGetFacts?.business_settled===1);
    assert.equal(phaseCount(https.facts().counts,'/custody/approvals/complete'),baseComplete,'late actual assertion must not enter complete');
    assert.equal(phaseCount(https.facts().counts,'/owner-setups/approve'),baseApprove,'late actual assertion must not enter Owner');
  }else await page.getByText('OwnerSetup 已批准；Execution 仍关闭，本页没有 continue 操作。',{exact:true}).waitFor();
  const getFacts=await actualGetFacts(page);assert.equal(getFacts.started,recovering?2:1,'only explicit expired recovery may allocate a fresh actual get');
  assert.equal(phaseCount(https.facts().counts,'/custody/approvals/options')-baseOptions,recovering?2:1,'only explicit expired recovery may allocate a fresh custody challenge');
  if(faultCase==='cancel-get'){assert.equal(phaseCount(https.facts().counts,'/custody/approvals/complete'),baseComplete);assert.equal(phaseCount(https.facts().counts,'/owner-setups/approve'),baseApprove);}
  const facts=https.facts();assert.equal(facts.bypass_attempts,0);assert.ok(!Object.keys(facts.counts).some(path=>path.includes('/owner-setups/continue')),'OwnerSetup must never continue');
  const nativeFacts=await(await fetch(data.address+'/_fixture/browser-facts')).json();assert.equal(nativeFacts.bypass_attempts,0);assert.equal(nativeFacts.manager_configured,true);
  const evidence={kind:'local_native_workerd_browser_uv',status:'passed',case:faultCase,fault:https.facts().fault,cancel_recovery:recoveryFacts,actual_get:getFacts,software_uv:true,hardware:'not_tested',production:'not_tested',parent_precondition:'actual fixture provision/participant, outside OwnerSetup UI acceptance',mfa_bypass_attempts:0,owner_setup_continue_calls:0,http_counts:facts.counts,native_counts:nativeCounts};
  writeFileSync(resolve(directory,'safe-evidence.json'),JSON.stringify(evidence,null,2),{mode:0o600});console.log(JSON.stringify({status:'passed',case:faultCase,evidence_directory:directory,kind:evidence.kind}));
  }
}catch(error) {
  await page?.screenshot({path:resolve(directory,'failure.png'),fullPage:true}).catch(()=>{});
  const counts=https?.facts().counts??{};
  const sum=suffix=>Object.entries(counts).filter(([key])=>key.endsWith(suffix)).reduce((n,[,value])=>n+value,0);
  const kind=['AssertionError','TimeoutError'].includes(error?.name)?error.name:'Error';
  classifyRuntime(error?.message);classifyRuntime(error?.cause?.message);
  const startupCode=['ERR_RUNTIME_FAILURE','ERR_VALIDATION','ERR_MODULE_PARSE','ERR_MODULE_NOT_FOUND','ERR_NO_MODULES','ERR_BUNDLING'].includes(error?.code)?error.code:'unclassified';

  const credentialRaw=error?.custodyCredentialDiagnostic;
  const credentialDiagnostic=credentialRaw&&['challenge','claim'].includes(credentialRaw.kind)?{kind:credentialRaw.kind,...Object.fromEntries(['http_status','native_verify_delta','native_lifetime_delta','native_total_delta'].filter(k=>Number.isSafeInteger(credentialRaw[k])&&credentialRaw[k]>=0&&credentialRaw[k]<=9999).map(k=>[k,credentialRaw[k]]))}:null;
  const raw=error?.custodyPrepareDiagnostic;
  const knownCodes=['custody_original_pending','custody_plan_invalid','custody_parent_invalid','custody_session_changed','custody_configuration_changed','custody_csrf_unavailable','custody_outcome_unknown','custody_response_invalid','custody_request_rejected','custody_prepare_invalid','custody_scope_unavailable','custody_disposed'];
  const booleanFields=['response_json_object','request_id_match','context_present','account_match','environment_match','audience_match','policy_positive','security_positive','security_is_zero','service_epoch_positive','intent_kind_match','plan_structurally_equal','scope_structurally_equal'];
  const diagnostic=raw&&['viewer_prepare_original_owner_v2','viewer_prepare_original_session_v2'].includes(raw.phase)?{
    phase:raw.phase,code:knownCodes.includes(raw.code)?raw.code:'unclassified',http_status:Number.isInteger(raw.http_status)&&raw.http_status>=100&&raw.http_status<=599?raw.http_status:null,
    fields:Object.fromEntries(booleanFields.filter(k=>typeof raw[k]==='boolean').map(k=>[k,raw[k]])),
  }:null;
  console.error(JSON.stringify({status:'failed',case:faultCase,stage,error_kind:kind,evidence_directory:directory,native_startup_diagnostic:stage==='native'?nativeStartupDiagnostic:null,nonce_five_diagnostic:nonceFiveDiagnostic,credential_diagnostic:credentialDiagnostic,nonce_ports:{...noncePorts},credential_source_stages:credentialSourceStages,credential_profile_stages:credentialProfileStages,prepare_diagnostic:diagnostic,owner_continue_diagnostic:ownerContinueDiagnostic,session_continue_diagnostic:sessionContinueDiagnostic,enrollment_diagnostic:enrollmentDiagnostics,enrollment_actor_stages:enrollmentActorStages,enrollment_proof_stages:enrollmentProofStages,enrollment_pop_errors:enrollmentPopErrors,owner_prepare_stages:ownerPrepareStages,session_preflight_stages:sessionPreflightStages,session_inspection_stages:sessionInspectionStages,...(stage==='workerd'?{startup:{code:startupCode,classes:runtimeDiagnostic,modules:moduleDiagnostic}}:{}),http_phase_statuses:https?.facts().phase_statuses??{},
    phase_counts:{options:sum('/custody/approvals/options'),complete:sum('/custody/approvals/complete'),prepare:sum('/owner-setups/prepare'),approve:sum('/owner-setups/approve')}}));process.exitCode=1;
}
finally {await hostDriver?.close();await context?.close();await browser?.close();await mf?.dispose();if(native&&!exited){writeFileSync(stop,'stop');for(let i=0;!exited&&i<250;i++)await delay(20);if(!exited)native.kill('SIGTERM');}await https?.close();await world3?.close();await world?.close();}

async function exerciseExecutionNonceFive({cycle,counters,setStage},ports){
  assert.equal(typeof cycle,'function');assert.equal(typeof cycle.executionNonce,'function');
  setStage('nonce-five-genuine-claim');
  let first,cycleError;
  try{first=await cycle({label:'nonce-five',purposes:['execute'],replace:false,expectedGeneration:null,discardReply:false,onPhase:phase=>setStage('nonce-five-'+phase)});}catch(error){cycleError=error;}
  // Wait only for the already committed SQL observer log, not another HTTP.
  const profileUntil=Date.now()+2000;
  while(credentialProfileStages.first_commit===0&&credentialProfileStages.inspection_error===0&&credentialProfileStages.no_unique_source===0&&Date.now()<profileUntil)await delay(10);
  setStage('nonce-five-new-paid-admit-profile');
  assert.equal(credentialProfileStages.inspection_error,0,'actual Account SQL profile inspection must succeed');
  assert.equal(credentialProfileStages.no_unique_source,0,'actual paid first Admit must create exactly one Source');
  assert.equal(credentialProfileStages.first_commit,1,'actual paid first Admit commit is the prerequisite');
  assert.equal(credentialProfileStages.credential_five,1,'new fixed Credential receiver requires its Core factory five-oracle profile');
  if(cycleError)throw cycleError;
  setStage('nonce-five-claim-status');
  nonceFiveDiagnostic={status_claimed:first.reply?.status==='claimed',api2:first.reply?.api_version===2,credentials_array:Array.isArray(first.reply?.credentials),credential_count:Array.isArray(first.reply?.credentials)?first.reply.credentials.length:null};
  assert.equal(first.reply.status,'claimed');
  setStage('nonce-five-claim-version');assert.equal(first.reply.api_version,2);
  setStage('nonce-five-claim-count');assert.equal(first.reply.credentials.length,1);const credential=first.reply.credentials[0];
  nonceFiveDiagnostic.purpose_execution=credential.purpose==='execution';nonceFiveDiagnostic.generation_one=credential.generation==='1';
  setStage('nonce-five-claim-purpose');assert.equal(credential.purpose,'execution');
  setStage('nonce-five-claim-generation');assert.equal(credential.generation,'1');
  // The genuine credential cycle sent precisely two prior public client HTTPs.
  // Wait for only their existing log delivery, never send another request.
  const priorUntil=Date.now()+2000;
  while((ports.provider!==2||ports.realm!==2)&&Date.now()<priorUntil)await delay(10);
  setStage('nonce-five-claim-limiter-prefix');
  assert.deepEqual(ports,{public:0,provider:2,realm:2,owner:0,account:0},'genuine claim limiter prefix must be observed before nonce measurement');
  setStage('nonce-five-claim-no-book');
  const before=counters(),previous={...ports};assert.equal(before.execution_book_absent,true);
  setStage('nonce-five-actual-public');await cycle.executionNonce(credential.token);
  const after=counters();assert.equal(after.purposes.execution.transport,before.purposes.execution.transport-5);
  assert.deepEqual(after.purposes.reconcile_read,before.purposes.reconcile_read);
  for(const k of ['host','native','requests','sources','claims'])assert.equal(after[k],before[k]);
  assert.equal(after.execution_book_absent,true);
  // Wait for structured counter delivery only, never retry business HTTP.
  const until=Date.now()+2000;
  while(Object.keys(ports).some(k=>ports[k]-previous[k]!==1)&&Date.now()<until)await delay(10);
  for(const k of Object.keys(ports))assert.equal(ports[k]-previous[k],1,`actual ${k} physical leaf exactly once`);
  return {actual_execution_nonce_five:true,actual_five_physical_hops:true,no_book_seed:true,no_source_native_world_io:true,no_control_or_read_borrow:true};
}
