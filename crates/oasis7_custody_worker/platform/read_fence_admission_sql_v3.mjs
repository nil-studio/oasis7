// Shared ordinary+paid inventory. No request chooses a profile, capacity or schema.
const profiles=Object.freeze([
 {profile:'execution_v2',prefix:'execution_fence_admission_v2',birth:'execution-fence-read-birth-v2'},
 {profile:'reconcile_read_v3',prefix:'read_fence_admission_v3',birth:'read-fence-read-birth-v3'},
]);
function schemas(p){return ['header','floor','once'].map(kind=>({kind,name:`${p.prefix}_${kind}`,sql:`CREATE TABLE ${p.prefix}_${kind} (id TEXT PRIMARY KEY, payload TEXT NOT NULL)`}));}
function exactUnsigned(v){if(typeof v!=='string'||! /^(0|[1-9][0-9]*)$/.test(v))throw Error('noncanonical fence count');return BigInt(v);}
function limit(v){const n=exactUnsigned(v);if(n===0n||n>4294967295n)throw Error('fence server capacity unavailable');return Number(n)+1;}
function inventory(state,capacities,rowLimit,business=true){
 const sql=state.storage.sql;
 if(!Number.isSafeInteger(rowLimit)||rowLimit<=65536)throw Error('fence row bound unavailable');
 const knownTables=4+profiles.length*3,catalogueProbe=knownTables*2+1;
 const catalogue=sql.exec("SELECT type,name,sql FROM sqlite_master WHERE type IN ('table','trigger','view','index') ORDER BY type,name LIMIT ?",catalogueProbe).toArray();
 const tables=catalogue.filter(r=>r.type==='table').map(r=>r.name),has=n=>tables.includes(n);
 const normalize=value=>typeof value==='string'?value.replace(/\s+/g,'').toLowerCase():null;
 const businessSchemas={custody_records:'CREATE TABLE custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)',custody_audit:'CREATE TABLE custody_audit (record_id TEXT NOT NULL,sequence TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(record_id,sequence))'};
 for(const [name,expected] of Object.entries(businessSchemas))if(has(name)&&normalize(catalogue.find(r=>r.type==='table'&&r.name===name)?.sql)!==normalize(expected))throw Error('fence business schema mismatch');
 // 2 business + exactly 2 compiled known birth keys + 1 unknown overflow probe.
 const records=has('custody_records')?sql.exec(`SELECT id,CASE WHEN id IN (?,?) THEN substr(payload,1,?) ELSE ${business?'substr(payload,1,524289)':'NULL'} END AS payload FROM custody_records ORDER BY id LIMIT 5`,profiles[0].birth,profiles[1].birth,rowLimit+1).toArray():[];
 const audit=!business?null:has('custody_audit')?sql.exec('SELECT CAST(COUNT(*) AS TEXT) AS count FROM custody_audit').one().count:'0';
 const audit_rows=!business?null:has('custody_audit')?sql.exec('SELECT record_id,sequence,substr(payload,1,2049) AS payload FROM custody_audit ORDER BY record_id,sequence LIMIT 130').toArray():[];
 const admissions=profiles.map((p,i)=>{
  const declarations=schemas(p);const present=declarations.filter(t=>has(t.name));
  if(present.length!==0&&present.length!==3)throw Error('partial fence admission schema');
  for(const d of present)if(catalogue.find(r=>r.type==='table'&&r.name===d.name)?.sql!==d.sql)throw Error('fence admission schema mismatch');
  if(present.length===0)return {profile:p.profile,header:null,floor:null,count:null,once:null};
  const max=limit(capacities[i]);const name=kind=>`${p.prefix}_${kind}`;
  return {profile:p.profile,header:sql.exec(`SELECT id,substr(payload,1,?) AS payload FROM ${name('header')} LIMIT 2`,rowLimit+1).toArray(),floor:sql.exec(`SELECT id,substr(payload,1,?) AS payload FROM ${name('floor')} LIMIT 2`,rowLimit+1).toArray(),count:sql.exec(`SELECT CAST(COUNT(*) AS TEXT) AS count FROM ${name('once')}`).one().count,once:sql.exec(`SELECT id,substr(payload,1,?) AS payload FROM ${name('once')} ORDER BY id LIMIT ?`,rowLimit+1,max).toArray()};
 });
 return JSON.stringify({records,catalogue,audit_count:audit,audit_rows,admissions});
}
export function readFenceAdmissionInventoryV3(state,capacities,rowLimit){return inventory(state,capacities,rowLimit,false);}
// The Rust callback validates every profile and may return only canonical business
// bytes; a missing business row may be initialized only for a truly empty store.
export function readFenceOrdinaryInventoryV3(state,capacities,rowLimit,initial,project){
 return state.storage.transactionSync(()=>{
  let raw=inventory(state,capacities,rowLimit);const before=JSON.parse(raw);
  let business=project(raw);if(typeof business!=='string')throw Error('fence projection unavailable');
  if(before.records.length===0&&before.catalogue.every(r=>(r.type==='table'&&(r.name==='_cf_METADATA'||r.name==='custody_records'))||(r.type==='index'&&r.sql===null&&r.name==='sqlite_autoindex_custody_records_1'))){
   state.storage.sql.exec('CREATE TABLE IF NOT EXISTS custody_records (id TEXT PRIMARY KEY, payload TEXT NOT NULL)');
   state.storage.sql.exec("INSERT INTO custody_records(id,payload) VALUES('recovery-fence-v1',?)",initial);
   raw=inventory(state,capacities,rowLimit);business=project(raw);
  }
  return business;
 });
}
export function transitionFenceOrdinaryV3(state,capacities,rowLimit,project,transition){
 return state.storage.transactionSync(()=>{
  const business=project(inventory(state,capacities,rowLimit));const result=transition(business);
  if(!Array.isArray(result)||result.length!==4||result.some(v=>typeof v!=='string'))throw Error('invalid fence transition');
  const [ledger,fence,sequence,audit]=result;
  if(ledger!==''){
   state.storage.sql.exec('CREATE TABLE IF NOT EXISTS custody_audit (record_id TEXT NOT NULL,sequence TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(record_id,sequence))');
   state.storage.sql.exec("INSERT INTO custody_audit(record_id,sequence,payload) VALUES('recovery-fence-ledger-v1',?,?)",sequence,audit);
   state.storage.sql.exec("INSERT INTO custody_records(id,payload) VALUES('recovery-fence-ledger-v1',?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload",ledger);
   state.storage.sql.exec("UPDATE custody_records SET payload=? WHERE id='recovery-fence-v1'",fence);
  }
  return project(inventory(state,capacities,rowLimit));
 });
}
function consumeProfile(state,capacities,rowLimit,step,index){
 return state.storage.transactionSync(()=>{
  const before=inventory(state,capacities,rowLimit,false);const next=step(before);
  if(!Array.isArray(next)||next.length!==7||next.some(v=>typeof v!=='string'))throw Error('invalid closed fence admission');
  const [birth,header,floor,id,claim,notBefore,expires]=next;const now=Date.now();
  if(!Number.isSafeInteger(now)||now<0||BigInt(now)<exactUnsigned(notBefore)||BigInt(now)>=exactUnsigned(expires))throw Error('fence original deadline unavailable');const p=profiles[index];const inv=JSON.parse(before),saved=inv.admissions[index];
  if(!/^[0-9a-f]{64}$/.test(id))throw Error('invalid fence once key');
  if(saved.header===null){for(const d of schemas(p))state.storage.sql.exec(d.sql);state.storage.sql.exec('INSERT INTO custody_records(id,payload) VALUES(?,?)',p.birth,birth);}
  else if(inv.records.find(r=>r.id===p.birth)?.payload!==birth)throw Error('immutable fence birth changed');
  state.storage.sql.exec(`INSERT INTO ${p.prefix}_once(id,payload) VALUES(?,?)`,id,claim);
  for(const [kind,payload] of [['header',header],['floor',floor]])state.storage.sql.exec(`INSERT INTO ${p.prefix}_${kind}(id,payload) VALUES('singleton',?) ON CONFLICT(id) DO UPDATE SET payload=excluded.payload`,payload);
  return inventory(state,capacities,rowLimit,false);
 });
}

export function consumeReadFenceAdmissionV3(state,capacities,rowLimit,step){return consumeProfile(state,capacities,rowLimit,step,1);}
export function consumeExecutionFenceAdmissionV3(state,capacities,rowLimit,step){return consumeProfile(state,capacities,rowLimit,step,0);}
