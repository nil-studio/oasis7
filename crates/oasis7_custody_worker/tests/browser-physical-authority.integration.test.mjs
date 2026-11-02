// Transport precondition only, not a Core/MFA/paid-current fixture.
import assert from 'node:assert/strict';
import test from 'node:test';
import {mkdtempSync,rmSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';import {resolve} from 'node:path';
import {request as httpsRequest} from 'node:https';
import {connect as tlsConnect} from 'node:tls';
import {browserHttps} from './fixtures/custody_browser_https.mjs';
function rawRequest(origin,ca,headers){return new Promise((resolve,reject)=>{const q=httpsRequest(new URL('/v1/client-enrollments/probe/prove',origin),{method:'POST',headers,setHost:false,ca,servername:'localhost',rejectUnauthorized:true,agent:false,lookup:(_h,o,cb)=>o?.all?cb(null,[{address:'127.0.0.1',family:4}]):cb(null,'127.0.0.1',4)},r=>{r.resume();r.on('end',()=>resolve(r.statusCode));});q.setTimeout(3000,()=>q.destroy());q.on('error',reject);q.end();});}
function rawTls(origin,ca,target,fields,body=Buffer.alloc(0)){
 return new Promise((resolve,reject)=>{
  let done=false;const socket=tlsConnect({host:'127.0.0.1',port:Number(new URL(origin).port),servername:'localhost',ca,rejectUnauthorized:true},()=>{socket.write(Buffer.from('POST '+target+' HTTP/1.1\r\n'+fields.join('\r\n')+'\r\n\r\n'));socket.write(body);});
  const chunks=[];let size=0;
  const finish=()=>{if(done)return;done=true;const first=Buffer.concat(chunks).toString('latin1').split('\r\n',1)[0];const match=/^HTTP\/1\.[01] ([0-9]{3})/.exec(first);socket.destroy();resolve(match?Number(match[1]):null);};
  socket.setTimeout(3000,()=>{socket.destroy();if(!done){done=true;reject(Error('bounded raw TLS probe expired'));}});
  socket.on('data',b=>{size+=b.length;if(size>8192){socket.destroy();if(!done){done=true;reject(Error('bounded raw TLS reply'));}return;}chunks.push(b);if(Buffer.concat(chunks).includes(Buffer.from('\r\n\r\n')))finish();});
  socket.on('end',finish);socket.on('error',()=>{if(!done){done=true;reject(Error('raw TLS probe unavailable'));}});
 });
}
test('real TLS preserves physical wrong authority and rejects absent or duplicate Host',{timeout:20000},async()=>{
 const dir=mkdtempSync(resolve(tmpdir(),'oasis7-physical-authority-'));let server;
 try{
  server=await browserHttps(dir,dir);let calls=0;const expected=new URL(server.origin).host;
  server.setIdentity(async request=>{
   calls++;const host=new URL(request.url).host;
   assert.equal(request.headers.get('host'),host,'actual physical authority remains exact');
   assert.equal(request.headers.get('MF-Proxy-Shared-Secret'),server.proxySecret,'only internal proxy secret survives');
   const good=host===expected;
   return new Response(JSON.stringify({physical_expected:good}),{status:good?200:400});
  });
  const good=await server.clientFetch(server.origin+'/v1/client-enrollments/probe/prove',{method:'POST',headers:{'content-type':'application/json','MF-Proxy-Shared-Secret':'caller-forgery'},body:'{}'});
  assert.equal(good.status,200);assert.equal((await good.json()).physical_expected,true);
  const wrong=await server.clientFetch(server.origin+'/v1/client-enrollments/probe/prove',{method:'POST',headers:{host:'wrong.invalid','content-type':'application/json'},body:'{}'});
  assert.equal(wrong.status,400);assert.equal((await wrong.json()).physical_expected,false,'wrong Host cannot be normalized into expected');
  const prior=calls;const ca=readFileSync(resolve(dir,'localhost.crt'));
  assert.equal(await rawRequest(server.origin,ca,{'content-type':'application/json','content-length':'0'}),400);
  assert.equal(await rawRequest(server.origin,ca,['Host',expected,'Host','wrong.invalid','Content-Length','0']),400);
  assert.equal(calls,prior,'absent and duplicate physical Host never reach the Worker bridge');
  const framed=calls;
  for(const fields of [
    ['Host: '+expected,'Transfer-Encoding: chunked'],
    ['Host: '+expected,'Content-Encoding: gzip','Content-Length: 0'],
    ['Host: '+expected,'Trailer: x-fixture-tail','Content-Length: 0'],
    ['Host: '+expected,'Content-Length: 00'],
    ['Host: '+expected,'Content-Length: 0','Content-Length: 0'],
  ])assert.equal(await rawTls(server.origin,ca,'/v1/client-enrollments/probe/prove',fields),400);
  assert.equal(await rawTls(server.origin,ca,'/v1/client-enrollments/probe/prove',['Host: '+expected,'Content-Length: 1'],Buffer.from([255])),400,'invalid UTF8 must not be decoded into another signed body');
  for(const target of ['/\\'+expected+'/v1/client-enrollments/probe/prove','/v1/../v1/client-enrollments/probe/prove'])assert.equal(await rawTls(server.origin,ca,target,['Host: wrong.invalid','Content-Length: 0']),400,'request target cannot replace physical authority or be silently normalized');
  assert.equal(calls,framed,'bad framing/UTF8/target never reach the identity bridge');
 }finally{if(server)await server.close();rmSync(dir,{recursive:true,force:true});}
});
