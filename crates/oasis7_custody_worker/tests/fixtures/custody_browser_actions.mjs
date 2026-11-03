import assert from 'node:assert/strict';
export async function browserAction(page,fn,arg) {
  await page.evaluate(({source,arg})=>{window.localUvResult=null;const prior=document.querySelector('#local-uv-action');prior?.remove();const button=document.createElement('button');button.id='local-uv-action';button.textContent='Local fixture operation';button.onclick=async()=>{try{window.localUvResult={ok:true,value:await window.localUvOperation(arg)};}catch{window.localUvResult={ok:false};}};document.body.append(button);}, {source:'fixed handler',arg});
  // Test-only browser function registration; actual credentials.get still runs
  // after a real click and is verified by the Native WebAuthn implementation.
  await page.evaluate(`window.localUvOperation=${fn.toString()}`);
  await page.locator('#local-uv-action').click();await page.waitForFunction(()=>window.localUvResult!==null);
  const result=await page.evaluate(()=>window.localUvResult);assert.equal(result.ok,true,'actual local browser operation failed');return result.value;
}
export async function approveProvision(page,request) {
  return browserAction(page,async request=>{
    const module=await import('/viewer_passkey_account_module.js');
    const capabilities=await(await fetch('/api/public/hosted-account/v2/capabilities')).json();
    const post=async(path,body)=>{const r=await fetch(path,{method:'POST',credentials:'same-origin',headers:{'content-type':'application/json','X-Oasis7-CSRF':capabilities.csrf_token},body:JSON.stringify(body)});if(!r.ok)throw Error('closed phase');return r.json();};
    const started=await post('/api/public/hosted-account/v2/custody/approvals/options',request);
    const credential=await navigator.credentials.get(module.browserPasskeyOptions(started.options));
    const complete=await post('/api/public/hosted-account/v2/custody/approvals/complete',{approval_id:started.approval_id,ceremony_id:started.ceremony_id,operation_id:started.operation_id,intent_digest:started.intent_digest,response:module.serializePasskey(credential)});
    const result=await post(`/v1/account-provisions/${encodeURIComponent(request.intent.provision_id)}/approve`,{api_version:1,input:{operation_id:'local-parent-provision-approval',request,approval:complete.approval}});
    return {stage:result.stage};
  },request);
}

// Instrument the actual browser API; never synthesize a credential or success.
export async function installActualGetFault(page,mode) {
  await page.evaluate(mode=>{
    const original=navigator.credentials.get.bind(navigator.credentials);
    window.localGetFacts={started:0,completed:0,aborted:0,released:0,returned:0,business_started:0,business_settled:0};
    // Install before the confirmation button is rendered. Observe the actual
    // async product click handler's settlement; no fixed delay substitutes for it.
    const add=EventTarget.prototype.addEventListener;
    EventTarget.prototype.addEventListener=function(type,listener,options) {
      if(type==='click'&&this instanceof HTMLButtonElement&&this.textContent==='确认摘要并使用通行密钥批准'&&typeof listener==='function') {
        const actual=listener;
        listener=function(event) {
          window.localGetFacts.business_started++;
          const result=actual.call(this,event);
          Promise.resolve(result).then(()=>{window.localGetFacts.business_settled++;},()=>{window.localGetFacts.business_settled++;});
          return result;
        };
      }
      return add.call(this,type,listener,options);
    };
    navigator.credentials.get=async options=>{
      window.localGetFacts.started++;
      const controller=mode==='abort'?new AbortController():null;
      if(controller)window.localAbortGet=()=>{window.localGetFacts.aborted++;controller.abort();};
      const credential=await original(controller?{...options,signal:controller.signal}:options);
      window.localGetFacts.completed++;
      if(mode==='late')await new Promise(resolve=>{window.localReleaseGet=()=>{window.localGetFacts.released++;resolve();};});
      window.localGetFacts.returned++;return credential;
    };
  },mode);
}
export async function actualGetFacts(page) {
  return page.evaluate(()=>({...window.localGetFacts}));
}
