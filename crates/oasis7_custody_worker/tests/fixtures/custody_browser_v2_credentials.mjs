// Test-only consumer of a closed same-scope signer. No key/proof/token artifacts.
import assert from 'node:assert/strict';

function credentials(reply,generation,purposes) {
  assert.equal(reply.status,'claimed');assert.equal(reply.api_version,2);
  assert.equal(reply.credentials.length,purposes.length);
  const found=new Map();
  for(const item of reply.credentials){
    assert.ok(purposes.includes(item.purpose));assert.equal(item.generation,String(generation));
    assert.equal(typeof item.token,'string');assert.equal(item.token.length,43);
    const bytes=Buffer.from(item.token,'base64url');
    assert.equal(bytes.length,32);assert.ok(bytes.toString('base64url')===item.token);
    assert.ok(!found.has(item.purpose));found.set(item.purpose,item.token);
  }
  return found;
}
function history(reply,purposes,generation) {
  assert.equal(reply.status,'historical');assert.equal(reply.api_version,2);
  assert.equal(reply.credentials.length,purposes.length);
  for(const item of reply.credentials){assert.ok(purposes.includes(item.purpose));
    assert.equal(item.generation,String(generation));assert.ok(!Object.hasOwn(item,'token'));}
}
function rootsUnchanged(before,after){
  for(const p of ['execution','reconcile_read','termination_plan']) {
    assert.equal(after.purposes[p].transport,before.purposes[p].transport);
    assert.equal(after.purposes[p].business,before.purposes[p].business);
  }
  assert.equal(after.execution_book_absent,true);assert.equal(after.old_v1_sessions,0);
}
export async function exerciseV2PublicCredentials({cycle,counters,setStage,lostReply}){
  const before=counters();
  assert.equal(before.claims,0);assert.equal(before.execution_book_absent,true);
  setStage('v2-public-real-claim');
  const first=await cycle({label:'first',purposes:['execute','read'],replace:false,expectedGeneration:null,discardReply:lostReply});
  const after=counters();
  assert.equal(after.claims,1);assert.equal(after.host,before.host-2);assert.equal(after.native,before.native-4);
  assert.equal(after.sources,before.sources+2);assert.equal(after.requests,before.requests-1);
  rootsUnchanged(before,after);
  if(lostReply){
    assert.equal(first.discarded_committed_reply,true);
    setStage('v2-public-lost-reply-history');
    const saved=counters();const historical=await first.replay();const replayed=counters();
    history(historical,['execution','reconcile_read'],1);
    assert.equal(replayed.claims,saved.claims);assert.equal(replayed.sources,saved.sources);
    assert.equal(replayed.native,saved.native);assert.equal(replayed.host,saved.host-1);
    assert.equal(replayed.requests,saved.requests);rootsUnchanged(saved,replayed);
    return {actual_v2_claim:true,committed_reply_discarded:true,history_no_token:true,history_native_http:0};
  }
  const initial=credentials(first.reply,1,['execution','reconcile_read']);
  setStage('v2-public-read-replacement');
  const replace=await cycle({label:'read-replace',purposes:['read'],replace:true,expectedGeneration:'1',discardReply:false});
  const next=credentials(replace.reply,2,['reconcile_read']);
  assert.ok(next.get('reconcile_read')!==initial.get('reconcile_read'));
  const replaced=counters();assert.equal(replaced.claims,2);assert.equal(replaced.host,after.host-2);
  assert.equal(replaced.native,after.native-4);assert.equal(replaced.requests,after.requests-1);
  assert.equal(replaced.purposes.execution.history,1);assert.equal(replaced.purposes.reconcile_read.history,2);
  rootsUnchanged(after,replaced);
  setStage('v2-public-retired-history-only');
  const old=await first.replay();history(old,['execution','reconcile_read'],1);
  const final=counters();assert.equal(final.purposes.reconcile_read.generation,'2');
  assert.equal(final.native,replaced.native);assert.equal(final.sources,replaced.sources);
  assert.equal(final.host,replaced.host-1);rootsUnchanged(replaced,final);
  return {actual_v2_claim:true,canonical_43_tokens:true,actual_read_replacement:true,history_no_token:true,
    no_purpose_budget_borrow:true,execution_book_absent:true};
}
