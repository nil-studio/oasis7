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
import {signFixtureRecoveryActivation} from '../tests/fixtures/fence_activation_signer.mjs';
const root=resolve(dirname(fileURLToPath(import.meta.url)),'..');const repo=resolve(root,'../..');
const caseIndex=process.argv.indexOf('--case');const faultCase=caseIndex<0?'positive':process.argv[caseIndex+1];
assert.ok(['positive','lost-complete','lost-owner-approve','wrong-ids','wrong-csrf','cancel-get','cancel-expiry-recover','logout-late-get','account-switch-late-get'].includes(faultCase),'closed --case selector');
const directory=mkdtempSync(join(tmpdir(),'oasis7-browser-uv-'));chmodSync(directory,0o700);
let world,https,native,mf,browser,context,page;let stage='startup';let exited=false;
const recovering=faultCase==='cancel-expiry-recover', cancelling=faultCase==='cancel-get'||recovering;let recoveryFacts=null;
const ready=resolve(directory,'native-ready.json'),stop=resolve(directory,'native-stop');
try {
  stage='world';world=await worldAuthority(directory);
  https=await browserHttps(directory,resolve(repo,'crates/oasis7_viewer/software_safe_src'));
  const origin=https.origin;
  stage='native';const binary=readFileSync(resolve(root,'artifacts/native-test-binary.txt'),'utf8').trim();
  native=spawn('rtk',['proxy',binary,'serve_custody_browser_uv_fixture','--ignored','--nocapture','--test-threads=1'],{
    env:{PATH:process.env.PATH,...Object.fromEntries(Object.entries(process.env).filter(([k])=>!k.startsWith('OASIS7_'))),OASIS7_CUSTODY_BROWSER_READY:ready,OASIS7_CUSTODY_BROWSER_STOP:stop,
      OASIS7_CUSTODY_BROWSER_ORIGIN:origin,OASIS7_CUSTODY_BROWSER_WORLD_TRUST:JSON.stringify(world.data)},stdio:'ignore'});native.once('exit',()=>{exited=true;});
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
  mf=new Miniflare(browserUvRoleOptions(resolve(directory,'workerd'),runtimeData,bridge,manager,origin,resolve(root,'tests/fixtures/custody_browser_provider_adapter.mjs')));
  https.setIdentity(async request=>mf.dispatchFetch(request.url,{method:request.method,headers:request.headers,...(request.method==='GET'?{}:{body:Buffer.from(await request.arrayBuffer())})}));
  const controller=await mf.getWorker('fence-controller');const description=await(await controller.fetch('https://fixture.test/describe',{method:'POST'})).json();
  assert.equal((await controller.fetch('https://fixture.test/activate',{method:'POST',body:JSON.stringify(signFixtureRecoveryActivation(description,'browser-fixture-bootstrap'))})).status,200,'real Fence activation');
  stage='parent-precondition';
  const draft={api_version:1,client_request_id:'browser-parent',plan:{environment:'test',identity_route:'native-one',world_route:'world-route-one',world_identity:world.data.world.world_id,purpose:'Local browser precondition fixture',expires_at:String(Date.now()+(recovering?360000:180000)),transport_limit:'400'}};
  const identity=await mf.getWorker('identity');const created=await identity.fetch(origin+'/v1/account-provisions',{method:'POST',headers:sourceHeaders,body:JSON.stringify(draft)});assert.equal(created.status,200,'parent draft failed');const original=await created.json();
  const request={approval_request_id:'browser-parent-approval',context:{environment:'test',custody_account_id:original.entry.custody_account_id,audience:data.audience,policy_version:'1',security_version:'0',service_epoch:'1'},
    intent:{kind:'provision',provision_id:original.entry.provision_id,purpose:draft.plan.purpose,plan_digest:original.entry.plan_digest,bootstrap_config_version:'1',world_identity:world.data.world,identity_route:'native-one',world_route:'world-route-one',native_identity_namespace:data.namespace,register_request_id:'browser-parent-register',login_request_id:'browser-parent-login',deadline:draft.plan.expires_at,transport_limit:'400'}};
  await approveProvision(page,request);
  const continued=await identity.fetch(origin+`/v1/account-provisions/${original.entry.provision_id}/continue`,{method:'POST',headers:sourceHeaders,body:JSON.stringify({api_version:1,operation_id:'browser-parent-participant'})});assert.equal(continued.status,200,'parent participant continuation failed');assert.equal((await continued.json()).stage,'participant_bound','actual parent fixture must reach participant_bound');
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
}catch(error) {
  await page?.screenshot({path:resolve(directory,'failure.png'),fullPage:true}).catch(()=>{});
  const counts=https?.facts().counts??{};
  const sum=suffix=>Object.entries(counts).filter(([key])=>key.endsWith(suffix)).reduce((n,[,value])=>n+value,0);
  const kind=['AssertionError','TimeoutError'].includes(error?.name)?error.name:'Error';
  console.error(JSON.stringify({status:'failed',case:faultCase,stage,error_kind:kind,evidence_directory:directory,
    phase_counts:{options:sum('/custody/approvals/options'),complete:sum('/custody/approvals/complete'),prepare:sum('/owner-setups/prepare'),approve:sum('/owner-setups/approve')}}));process.exitCode=1;
}
finally {await context?.close();await browser?.close();await mf?.dispose();if(native&&!exited){writeFileSync(stop,'stop');for(let i=0;!exited&&i<250;i++)await delay(20);if(!exited)native.kill('SIGTERM');}await https?.close();await world?.close();}
