// Two fixed physical transports, called only by the private Rust first-token
// consumer. The service RPC still contains its real Account Actor hop.
function deadline(until) {
  const now=Date.now();
  if(!Number.isSafeInteger(now)||now<=0||!Number.isSafeInteger(until)||until<=now||until-now>5000)throw new Error('execution fence deadline');
  return until-now;
}
async function limited(operation,until) {
  let timer;const controller=new AbortController();
  const stop=new Promise((_,reject)=>{timer=setTimeout(()=>{controller.abort();reject(new Error('execution fence deadline'));},deadline(until));});
  try {const result=await Promise.race([operation(controller.signal),stop]);if(Date.now()>=until)throw new Error('execution fence deadline');return result;}
  finally {clearTimeout(timer);controller.abort();}
}
export async function claimExecutionFenceAccountLeaf(env,body,until) {
  if(typeof body!=='string'||new TextEncoder().encode(body).byteLength>32768)throw new Error('execution fence request bound');
  return limited(async()=>{
    const raw=await env.EXECUTION_FENCE_ACCOUNT_GATE.claim(body);
    if(typeof raw!=='string'||new TextEncoder().encode(raw).byteLength>32768)throw new Error('execution fence gate bound');
    return raw;
  },until);
}
export async function readExecutionFenceLeaf(env,body,until) {
  if(typeof body!=='string'||new TextEncoder().encode(body).byteLength>32768)throw new Error('execution fence request bound');
  return limited(async(signal)=>{
    const stub=env.FENCES.get(env.FENCES.idFromName('service-fence-v1'));
    const response=await stub.fetch(new Request('https://fence.internal/internal/execution-fence-read-v2',{method:'POST',headers:{'Content-Type':'application/json'},body,redirect:'manual',signal}));
    if(response.status!==200||response.redirected||(response.headers.get('Content-Type')??'').split(';')[0].trim().toLowerCase()!=='application/json'||!response.body)throw new Error('execution fence read rejected');
    const reader=response.body.getReader();const parts=[];let length=0;
    try {for(;;){const part=await reader.read();if(part.done)break;length+=part.value.byteLength;if(length>32768)throw new Error('execution fence response bound');parts.push(part.value);}
      const bytes=new Uint8Array(length);let at=0;for(const part of parts){bytes.set(part,at);at+=part.byteLength;}return new TextDecoder('utf-8',{fatal:true}).decode(bytes);
    } finally {void reader.cancel().catch(()=>{});reader.releaseLock();}
  },until);
}

// The actual first sender uses only the dedicated read-only class namespace.
export async function sendExecutionFenceDispatcher(env,name,body,until) {
  if(typeof name!=='string'||!/^[0-9a-f]{64}$/.test(name)||typeof body!=='string'||new TextEncoder().encode(body).byteLength>32768)throw new Error('execution fence dispatch bound');
  return limited(async(signal)=>{
    const stub=env.EXECUTION_FENCE_DISPATCHERS.get(env.EXECUTION_FENCE_DISPATCHERS.idFromName(name));
    const response=await stub.fetch(new Request('https://fence-dispatch.internal/internal/execution-fence-dispatch-v2',{method:'POST',headers:{'Content-Type':'application/json'},body,redirect:'manual',signal}));
    if(response.status!==200||response.redirected||(response.headers.get('Content-Type')??'').split(';')[0].trim().toLowerCase()!=='application/json'||!response.body)throw new Error('execution fence dispatcher rejected');
    const reader=response.body.getReader();const parts=[];let length=0;
    try{for(;;){const part=await reader.read();if(part.done)break;length+=part.value.byteLength;if(length>32768)throw new Error('execution fence reply bound');parts.push(part.value);}
      const bytes=new Uint8Array(length);let offset=0;for(const part of parts){bytes.set(part,offset);offset+=part.byteLength;}return new TextDecoder('utf-8',{fatal:true}).decode(bytes);
    }finally{void reader.cancel().catch(()=>{});reader.releaseLock();}
  },until);
}
