// Exactly one fixed private Actor read. No caller-selected namespace or URL.
export async function fetchTermOwnerSlotQualification(env,name,body,until){
  const at=Date.now();
  if(!Number.isSafeInteger(at)||at<=0||!Number.isSafeInteger(until)||until<=at||until-at>5000
    ||typeof name!=='string'||!(/^[0-9a-f]{64}$/).test(name)||typeof body!=='string'||new TextEncoder().encode(body).byteLength>32768)
    throw new Error('Term qualification unavailable');
  const controller=new AbortController();
  let reader;
  let timer;
  const deadline=new Promise((_,reject)=>{
    timer=setTimeout(()=>{
      controller.abort();
      if(reader)void reader.cancel().catch(()=>{});
      reject(new Error('Term qualification deadline'));
    },until-at);
  });
  const operation=(async()=>{
    const stub=env.ROLE_KEYS.get(env.ROLE_KEYS.idFromName(name));
    const request=new Request('https://role-key.internal/internal/role-key/term-owner-slot-qualification-v2',{
      method:'POST',headers:{'Content-Type':'application/json'},body,redirect:'manual',signal:controller.signal,
    });
    const response=await stub.fetch(request);
    if(response.status!==200||response.redirected||(response.headers.get('Content-Type')??'').split(';')[0].trim().toLowerCase()!=='application/json'||!response.body)
      throw new Error('Term qualification rejected');
    reader=response.body.getReader();
    const parts=[];let length=0;
    try{
      for(;;){
        const part=await reader.read();
        if(part.done)break;
        length+=part.value.byteLength;
        if(length>32768)throw new Error('Term qualification bound');
        parts.push(part.value);
      }
      if(Date.now()>=until)throw new Error('Term qualification deadline');
      const bytes=new Uint8Array(length);let offset=0;
      for(const part of parts){bytes.set(part,offset);offset+=part.byteLength;}
      return new TextDecoder('utf-8',{fatal:true}).decode(bytes);
    }finally{
      if(reader){void reader.cancel().catch(()=>{});reader.releaseLock();reader=undefined;}
    }
  })();
  try{return await Promise.race([operation,deadline]);}
  finally{clearTimeout(timer);controller.abort();}
}
