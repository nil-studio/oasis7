// Only fixed private service/Actor transports; no public Actions mount.
const max=32768;
function deadline(until){const now=Date.now();if(!Number.isSafeInteger(now)||now<=0||!Number.isSafeInteger(until)||until<=now||until-now>5000)throw new Error('source inspect deadline');return until-now;}
function bodyBound(raw){if(typeof raw!=='string'||new TextEncoder().encode(raw).byteLength>max)throw new Error('source inspect bound');}
async function limited(action,until){
  const controller=new AbortController();let timer;
  const stopped=new Promise((_,reject)=>{timer=setTimeout(()=>{controller.abort();reject(new Error('source inspect deadline'));},deadline(until));});
  try{const value=await Promise.race([action(controller.signal),stopped]);deadline(until);return value;}
  finally{clearTimeout(timer);controller.abort();}
}
export async function claimPendingSourceNativeLeaf(env,raw,until){
  bodyBound(raw);
  // This real Owner service RPC in turn makes its one fixed Account Actor fetch.
  // Timeout does not assert that an in-flight private RPC was never accepted.
  return limited(async()=>{const value=await env.EXECUTION_SOURCE_INSPECT_ACCOUNT_GATE.claim(raw);bodyBound(value);return value;},until);
}
export async function sendPendingSourceInspectDispatcher(env,name,raw,until){
  bodyBound(raw);if(typeof name!=='string'||!/^[0-9a-f]{64}$/.test(name))throw new Error('source inspect object');
  return limited(async signal=>{
    const namespace=env.EXECUTION_SOURCE_INSPECT_DISPATCHERS;
    const stub=namespace.get(namespace.idFromName(name));
    const response=await stub.fetch(new Request('https://source-inspect.internal/internal/execution-source-inspect-v2',{method:'POST',headers:{'Content-Type':'application/json'},body:raw,redirect:'manual',signal}));
    if(response.status!==200||response.redirected||!response.body||(response.headers.get('Content-Type')??'').split(';')[0].trim().toLowerCase()!=='application/json')throw new Error('source inspection unavailable');
    const reader=response.body.getReader();const chunks=[];let size=0;
    try{for(;;){const part=await reader.read();if(part.done)break;size+=part.value.byteLength;if(size>max)throw new Error('source inspect reply bound');chunks.push(part.value);}
      const all=new Uint8Array(size);let offset=0;for(const chunk of chunks){all.set(chunk,offset);offset+=chunk.byteLength;}return new TextDecoder('utf-8',{fatal:true}).decode(all);
    }finally{void reader.cancel().catch(()=>{});reader.releaseLock();}
  },until);
}
