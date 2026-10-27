// Test-only ops caller; no deployment binds this worker or HTTP facade.
export default { async fetch(request,env) {
  const path=new URL(request.url).pathname;
  try {
    if(path==='/stats')return Response.json(await env.STATS.stats());
    const value=path==='/describe'?await env.CONTROL.describe():path==='/activate'?await env.CONTROL.activate(await request.text()):path==='/disable'?await env.CONTROL.disable():await env.READER.read();
    return Response.json(JSON.parse(value));
  } catch { return Response.json({error:'rejected'}, {status:409}); }
} };
