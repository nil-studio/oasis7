// Test-only TLS receiver: public CA trusted by isolated H; no TLS bypass.
import assert from 'node:assert/strict';
import {createServer} from 'node:https';
import {execFileSync} from 'node:child_process';
import {readFileSync,chmodSync} from 'node:fs';
import {join} from 'node:path';
export async function hostControlHttps(directory){
 const key=join(directory,'localhost.key'),cert=join(directory,'localhost.crt');
 execFileSync('rtk',['proxy','openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',cert,'-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost'],{stdio:'ignore'});chmodSync(key,0o600);
 let runtime=null,drop=null;const observations=[];
 const server=createServer({key:readFileSync(key),cert:readFileSync(cert)},async(req,res)=>{
  try{
   assert.ok(['127.0.0.1','::ffff:127.0.0.1','::1'].includes(req.socket.remoteAddress));
   assert.equal(req.method,'POST');assert.equal(req.headers.host,'localhost');
   assert.ok(req.url.startsWith('/v1/')&&!req.url.includes('?')&&!req.url.includes('#'));
   const chunks=[];let size=0;for await(const b of req){size+=b.length;assert.ok(size<=65536);chunks.push(b);}const body=Buffer.concat(chunks);
   assert.ok(runtime);const headers=new Headers();
   for(let i=0;i<req.rawHeaders.length;i+=2){const n=req.rawHeaders[i].toLowerCase();if(!['connection','content-length','host','user-agent','accept-encoding'].includes(n))headers.append(n,req.rawHeaders[i+1]);}
   const call=req.url.startsWith('/v1/client-enrollments')?runtime.publicEnrollmentFetch:runtime.publicClientFetch;
   const reply=await call(req.url,{method:'POST',headers,body});const bytes=Buffer.from(await reply.arrayBuffer());
   let decoded=null;try{decoded=JSON.parse(body);}catch{}
   observations.push({path:req.url,request_id:decoded?.request_id??null,body:Buffer.from(body),status:reply.status});
   if(drop&&req.url.endsWith('/heartbeat')&&decoded?.request_id===drop){drop=null;req.socket.destroy();return;}
   res.writeHead(reply.status,{'content-type':'application/json'});res.end(bytes);
  }catch{res.writeHead(503,{'content-type':'application/json'});res.end('{"error_code":"host_control_fixture_rejected"}');}
 });await new Promise(resolve=>server.listen(0,'0.0.0.0',resolve));
 return {port:server.address().port,bind(v){runtime=v;},dropHeartbeat(id){assert.equal(drop,null);drop=id;},facts(){return observations.map(v=>({...v,body:Buffer.from(v.body)}));},async close(){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}};
}
