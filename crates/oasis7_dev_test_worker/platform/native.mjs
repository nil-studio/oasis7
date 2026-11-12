import {readBoundedText, loadRecord, commitRecord} from './store.mjs';
const allowed = new Set(['/session','/capabilities','/entry-executor/control']);
export async function nativeHttp(origin,path,method,cookie,csrf,json,deadline,limit) {
  if (!allowed.has(path) || !((method==='GET' && path!=='/entry-executor/control') || (method==='POST' && path==='/entry-executor/control')) || Date.now() >= deadline) throw new Error('native_deadline_unknown');
  const controller=new AbortController(), timer=setTimeout(()=>controller.abort(),Math.max(0,deadline-Date.now()));
  try {
    const response=await fetch(origin+"/api/public/hosted-account/v2"+path,{method,headers:{'Content-Type':'application/json','Origin':origin,'Cookie':cookie,...(method==='POST'?{'X-Oasis7-Csrf':csrf}: {})},
      body:method==='POST'?json:undefined, redirect:'manual',signal:controller.signal});
    if (response.status!==200) { await response.body?.cancel(); throw new Error('native_response_unavailable'); }
    return await readBoundedText(response,limit,deadline);
  } finally { clearTimeout(timer); }
}
function bytes(hex) { if (!/^[0-9a-f]{64}$/.test(hex)) throw new Error('private_wrapping_unavailable'); return Uint8Array.from(hex.match(/../g),x=>parseInt(x,16)); }
const hex=data=>Array.from(new Uint8Array(data),x=>x.toString(16).padStart(2,'0')).join('');
function decode(hexValue) { if (typeof hexValue!=='string' || hexValue.length%2 || !/^[0-9a-f]+$/.test(hexValue)) throw new Error('private_record_unknown'); return Uint8Array.from(hexValue.match(/../g),x=>parseInt(x,16)); }
async function key(env) { return crypto.subtle.importKey('raw',bytes(env.DEV_TEST_STATE_KEK),{name:'AES-GCM'},false,['encrypt','decrypt']); }
function aad(name) { return new TextEncoder().encode('oasis7/dev-test-worker-private-state/v1:'+name); }
async function decrypt(env,name,raw) {
  if (!raw) return '';
  const record=JSON.parse(raw);
  if (record.version!==1 || typeof record.iv!=='string' || record.iv.length!==24 || typeof record.ciphertext!=='string' || record.ciphertext.length>6*1024*1024) throw new Error('private_record_unknown');
  const plain=await crypto.subtle.decrypt({name:'AES-GCM',iv:decode(record.iv),additionalData:aad(name),tagLength:128},await key(env),decode(record.ciphertext));
  return new TextDecoder('utf-8',{fatal:true}).decode(plain);
}
export async function readNativeRecord(state,env,name) { return decrypt(env,name,loadRecord(state,name)); }
export async function putNativeRecord(state,env,name,plain) {
  if (new TextEncoder().encode(plain).length>2*1024*1024) throw new Error('private_record_bound');
  const old=loadRecord(state,name);
  if (old) { if (await decrypt(env,name,old)!==plain) throw new Error('private_original_conflict'); return; }
  const iv=crypto.getRandomValues(new Uint8Array(12));
  const cipher=await crypto.subtle.encrypt({name:'AES-GCM',iv,additionalData:aad(name),tagLength:128},await key(env),new TextEncoder().encode(plain));
  const wrapped=JSON.stringify({version:1,iv:hex(iv),ciphertext:hex(cipher)});
  // The SQL write must complete before the caller may send a native request.
  commitRecord(state,name,'',wrapped);
}
