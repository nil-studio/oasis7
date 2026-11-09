// Only explicit fixture bindings enable this sink. No payload/error/identifier is accepted.
const stages=new Set([
  'join.attestation','join.enrollment','join.observation','attestation.text','attestation.decode','attestation.exact','activate.store','activate.restore',
  'current.checks','current.transition','sql.initial_read','sql.restore','sql.current','sql.clock','sql.transition','sql.inventory_read','sql.inventory_write','sql.commit',
  'store.restore','current.before','core.checks.before','strict_crypto','core.checks.final','core.cache.final','core.transition.final','alarm.prearm_or_commit',
  'client.admit','client.claim_source','client.source_config','client.source_verify','client.record_source','client.manifest','client.actor','client.reply',
  'client.pre_decrypt','client.private_read','client.post_decrypt','client.fingerprint','client.claim_lifetime','client.current','client.transport','client.lifetime','client.lifetime_pin','client.lifetime_scope','client.complete',
  'account.revalidate','account.admit','account.live','account.challenge','account.claim_source','account.record_source',
  'live.reserve','live.ticket','live.snapshot','live.attestation','live.observation','live.current',
  'proof.snapshot','proof.selector','proof.configuration','proof.prepare','proof.async_crypto','proof.final_prepare','proof.final_crypto','proof.transition','proof.sql_commit',
  'live.checks','live.transition','live.transaction'
]);
const categories=new Set(['unknown','admission_capacity','admission_invalid','admission_frozen','admission_stale_context','admission_expired','admission_binding_mismatch','admission_unknown_key','admission_signature_rejected','admission_conflict','admission_nonce_reused','admission_missing','admission_invalid_persistence','provision_invalid','provision_forbidden','provision_frozen','provision_conflict','provision_expired','provision_exhausted','provision_missing','provision_invalid_transition','provision_encoding','provision_overflow','binding_mismatch','context_too_large','conflict','invalid_persistence','decode','clock','signature']);
const facts=new Set(['source_observed_at','source_expires_at','execution_root_deadline','lease_until','lifetime_until','lifetime_status_until','proof_until','observation_until']);
const numeric=value=>Number.isSafeInteger(value)&&value>=0?value:null;
function clock(){try{return numeric(Date.now());}catch{return null;}}
export function startRejectionTrace(enabled,kind,started,observed){
  if(enabled!==true)return null;
  return {kind:['activation','client','account','proof','live'].includes(kind)?kind:'unknown',started:numeric(started)??clock(),failed:false,facts:{source_observed_at:numeric(observed)}};
}
export function setRejectionFact(trace,name,value){
  if(!trace||trace.failed||!facts.has(name))return;
  trace.facts[name]=numeric(value);
}
export function failRejectionTrace(trace,stage,category,checkedAt=null){
  if(!trace||trace.failed)return;
  trace.failed=true;
  // Capture before formatting/emitting: later parent errors must not replace this instant.
  const checked=numeric(checkedAt), now=checked??clock(), f=trace.facts;
  const difference=(a,b)=>a!==null&&a!==undefined&&b!==null&&b!==undefined?a-b:'unknown';
  const age=difference(now,f.source_observed_at);
  const result={selection:'first_observed_failure',time_basis:checked===null?'failure_observed':'check_time',kind:trace.kind,stage:stages.has(stage)?stage:'unknown',category:categories.has(category)?category:'unknown',
    elapsed_ms:difference(now,trace.started),source_age_ms:age,source_freshness_remaining_ms:typeof age==='number'&&Number.isSafeInteger(5000-age)?5000-age:'unknown',
    source_remaining_ms:difference(f.source_expires_at,now),execution_root_remaining_ms:difference(f.execution_root_deadline,now),lease_remaining_ms:difference(f.lease_until,now),
    lifetime_remaining_ms:difference(f.lifetime_until,now),lifetime_status_remaining_ms:difference(f.lifetime_status_until,now),proof_remaining_ms:difference(f.proof_until,now),observation_remaining_ms:difference(f.observation_until,now)};
  try{console.error(JSON.stringify({custody_rejection:result}));}catch{/* Diagnostics cannot change the original failure. */}
}
