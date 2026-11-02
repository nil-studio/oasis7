// Dedicated bounded physical-once rows; old Directory/Fence storage is intact.
// No caller-visible HTTP or arbitrary query reaches these exports.
const LIMIT=65536;
const U64=(1n<<64n)-1n;
function integer(s){if(typeof s!=="string"||!/^([1-9][0-9]*)$/.test(s))throw Error('pending Current once unavailable');const n=BigInt(s);if(n>U64)throw Error('pending Current once unavailable');return n;}
function text(s){return typeof s==='string'&&s.length>0&&s.length<=256&&!/[\u0000-\u001f\u007f]/.test(s);}
export function claimExecutionPendingActorV2(state,raw){
 if(typeof raw!=='string'||raw.length>8192)throw Error('pending Current once unavailable');
 const q=JSON.parse(raw),keys=['namespace','service_id','environment','physical_attempt_id','proof_digest','issuer_epoch','config_generation'];
 if(!q||Array.isArray(q)||Object.keys(q).length!==keys.length||keys.some(k=>!Object.hasOwn(q,k))||keys.slice(0,4).some(k=>!text(q[k]))||typeof q.proof_digest!=='string'||!/^([0-9a-f]{64})$/.test(q.proof_digest))throw Error('pending Current once unavailable');
 const epoch=integer(q.issuer_epoch),generation=integer(q.config_generation),storage=state.storage,sql=storage.sql;
 storage.transactionSync(()=>{
  sql.exec('CREATE TABLE IF NOT EXISTS custody_pending_current_floor_v2(namespace TEXT NOT NULL,service_id TEXT NOT NULL,environment TEXT NOT NULL,issuer_epoch TEXT NOT NULL,config_generation TEXT NOT NULL,PRIMARY KEY(namespace,service_id,environment))');
  sql.exec('CREATE TABLE IF NOT EXISTS custody_pending_current_once_v2(namespace TEXT NOT NULL,service_id TEXT NOT NULL,environment TEXT NOT NULL,attempt TEXT NOT NULL,proof_digest TEXT NOT NULL,PRIMARY KEY(namespace,service_id,environment,attempt))');
  const scope=[q.namespace,q.service_id,q.environment];
  const old=sql.exec('SELECT issuer_epoch,config_generation FROM custody_pending_current_floor_v2 WHERE namespace=? AND service_id=? AND environment=?',...scope).toArray();
  if(old.length>1||old.length===1&&(epoch<integer(old[0].issuer_epoch)||generation<integer(old[0].config_generation)))throw Error('pending Current once unavailable');
  if(sql.exec('SELECT attempt FROM custody_pending_current_once_v2 WHERE namespace=? AND service_id=? AND environment=? AND attempt=?',...scope,q.physical_attempt_id).toArray().length!==0)throw Error('pending Current once unavailable');
  const rows=sql.exec('SELECT COUNT(*) AS n FROM custody_pending_current_once_v2').toArray();
  if(rows.length!==1||!Number.isSafeInteger(rows[0].n)||rows[0].n>=LIMIT)throw Error('pending Current once unavailable');
  sql.exec('INSERT INTO custody_pending_current_once_v2(namespace,service_id,environment,attempt,proof_digest) VALUES(?,?,?,?,?)',...scope,q.physical_attempt_id,q.proof_digest);
  sql.exec('INSERT INTO custody_pending_current_floor_v2(namespace,service_id,environment,issuer_epoch,config_generation) VALUES(?,?,?,?,?) ON CONFLICT(namespace,service_id,environment) DO UPDATE SET issuer_epoch=excluded.issuer_epoch,config_generation=excluded.config_generation',...scope,q.issuer_epoch,q.config_generation);
 });
}
