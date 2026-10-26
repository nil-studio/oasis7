// Synthetic offline keys and an independent manifest encoder, exclusively for local tests.
import assert from 'node:assert/strict';
import {createPrivateKey,createPublicKey,sign} from 'node:crypto';
export const fixtureRecoveryKeys=[81,82].map(seed=>createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.alloc(32,seed)]),format:'der',type:'pkcs8'}));
export const fixtureRecoveryPolicy={api_version:1,environment:'test',authority_id:'isolated-offline-recovery',quorum:2,keys:fixtureRecoveryKeys.map((key,i)=>({key_id:`offline-${i}`,public_key:createPublicKey(key).export({type:'spki',format:'der'}).subarray(-32).toString('hex')}))};
function head(major,length) {
  if(length<24)return Buffer.from([(major<<5)|length]);
  if(length<256)return Buffer.from([(major<<5)|24,length]);
  assert.ok(length<65536);return Buffer.from([(major<<5)|25,length>>8,length&255]);
}
export function encodeFixtureRecoveryCbor(value) {
  const cbor=encodeFixtureRecoveryCbor;
  if(typeof value==='string'){const b=Buffer.from(value);return Buffer.concat([head(3,b.length),b]);}
  if(typeof value==='number'){assert.ok(Number.isInteger(value)&&value>=0&&value<24);return head(0,value);}
  if(Array.isArray(value))return Buffer.concat([head(4,value.length),...value.map(cbor)]);
  const entries=Object.entries(value).map(([k,v])=>[cbor(k),cbor(v)]).sort((a,b)=>a[0].length-b[0].length||Buffer.compare(a[0],b[0]));
  return Buffer.concat([head(5,entries.length),...entries.flat()]);
}
export function signFixtureRecoveryActivation(description,operation='activation-one') {
  const ledger=description.ledger;
  const manifest={api_version:1,environment:fixtureRecoveryPolicy.environment,authority_id:fixtureRecoveryPolicy.authority_id,object_id:ledger.object_id,operation_id:operation,policy_digest:description.policy_digest,previous_state_digest:description.state_digest,evidence_digest:'ab'.repeat(32),from_epoch:ledger.fence.epoch,to_epoch:String(BigInt(ledger.fence.epoch)+1n),issued_at:String(Date.now()-10),expires_at:String(Date.now()+60000)};
  const bytes=encodeFixtureRecoveryCbor(['oasis7.custody.recovery-activation.v1',manifest]);
  return {manifest,signatures:fixtureRecoveryKeys.map((key,i)=>({key_id:`offline-${i}`,signature:sign(null,bytes,key).toString('base64url')}))};
}
