// Test-only barrier around two exact Core-registered Root status nonces.
export function freshAccessStatusBarrier(nonces,mode){
  const pending=new Set([nonces.policy_session,nonces.authorize_session]);
  if(pending.size!==2)throw new Error('fixed status originals must be distinct');
  const facts={policy_status:0,authorize_status:0,policy_completed:0,policy_http_status:null,policy_rejected:false,authorize_after_policy:false,released:null,elapsed_ms:0};
  let started=null,timer,resolve;
  const ready=new Promise(done=>{resolve=done;});
  function release(reason){if(facts.released!==null)return;facts.released=reason;facts.elapsed_ms=Date.now()-started;clearTimeout(timer);resolve();}
  return {facts,after(path,status){if(path==='/participant/policy-original'){facts.policy_http_status=status;if(status===200){facts.policy_completed++;if(mode==='reject_policy'){facts.policy_rejected=true;throw new Error('fixed test rejects completed policy response');}}}},async before(path,request){
    if(path==='/participant/authorize'){facts.authorize_after_policy=facts.policy_completed===1;if(!facts.authorize_after_policy)throw new Error('fixed policy completion must precede authorize');}
    if(path!=='/custody/managed-bindings/session-status'||!pending.has(request.nonce))return;
    const key=request.nonce===nonces.policy_session?'policy_status':'authorize_status';
    facts[key]++;if(facts[key]!==1)throw new Error('fixed status must not be retried');
    if(started===null){started=Date.now();timer=setTimeout(()=>release('bounded_timeout'),5100);}
    if(mode!=='over_window'&&facts.policy_status===1&&facts.authorize_status===1)release('independent_statuses_started');
    await ready;
  }};
}
