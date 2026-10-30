// Dedicated local test RP. No routes to arbitrary services or synthetic MFA.
import assert from 'node:assert/strict';
import {createServer} from 'node:https';
import {readFileSync,chmodSync} from 'node:fs';
import {resolve} from 'node:path';
import {execFileSync} from 'node:child_process';
export async function browserHttps(directory,viewerRoot) {
  const key=resolve(directory,'localhost.key'),cert=resolve(directory,'localhost.crt');
  execFileSync('rtk',['proxy','openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',cert,'-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost'],{stdio:'ignore'});chmodSync(key,0o600);
  let native=null,identity=null,bypass=0;
  const counts={};
  const files=new Set(['viewer_passkey_account_module.js','viewer_custody_owner_setup_panel.js','viewer_custody_owner_setup_client.js','viewer_account_email_recovery.js','viewer_imported_key.js','viewer_auth_crypto.js','software_safe_constants.js']);
  const server=createServer({key:readFileSync(key),cert:readFileSync(cert)},async(req,res)=>{
    try {
      assert.ok(req.socket.remoteAddress==='127.0.0.1'||req.socket.remoteAddress==='::ffff:127.0.0.1');
      const path=new URL(req.url,'https://localhost').pathname;
      if(path==='/_fixture/approve-intent'){bypass++;res.writeHead(403);res.end('forbidden');return;}
      if(path==='/'&&req.method==='GET'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><meta charset=utf-8><title>Local custody UI</title><style>body{font:16px system-ui;padding:16px;max-width:720px;margin:auto}.stack{display:grid;gap:12px}button,textarea{font:inherit;max-width:100%}dd{margin:0 0 8px;overflow-wrap:anywhere}</style><main id=account class=stack></main><script type=module>import{mountPasskeyAccount}from"/viewer_passkey_account_module.js";window.surface=mountPasskeyAccount(document.querySelector("#account"));</script>');return;}
      if(files.has(path.slice(1))&&req.method==='GET'){res.setHeader('Content-Type','text/javascript');res.end(readFileSync(resolve(viewerRoot,path.slice(1))));return;}
      if(!path.startsWith('/api/public/hosted-account/v2/')&&!path.startsWith('/v1/')){res.writeHead(404);res.end();return;}
      const chunks=[];let size=0;for await(const chunk of req){size+=chunk.length;if(size>65536)throw new Error('bounded local request');chunks.push(chunk);}const body=Buffer.concat(chunks);
      const headers=new Headers();for(const [name,value]of Object.entries(req.headers)){
        if(['cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type','accept'].includes(name)&&typeof value==='string')headers.set(name,value);
      }
      counts[req.method+' '+path]=(counts[req.method+' '+path]??0)+1;
      let response;
      if(path.startsWith('/api/')){assert.ok(native);response=await fetch(native+path,{method:req.method,headers,...(req.method==='GET'?{}:{body}),redirect:'manual'});}
      else {assert.ok(identity);response=await identity(new Request(origin+path,{method:req.method,headers,...(req.method==='GET'?{}:{body})}));}
      res.statusCode=response.status;for(const name of ['content-type','cache-control']){const value=response.headers.get(name);if(value)res.setHeader(name,value);}
      const cookies=response.headers.getSetCookie?.()??[];if(cookies.length)res.setHeader('Set-Cookie',cookies);
      res.end(Buffer.from(await response.arrayBuffer()));
    }catch {res.writeHead(503,{'content-type':'application/json'});res.end('{"error_code":"local_fixture_unavailable"}');}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`https://localhost:${server.address().port}`;
  return {origin,setNative(address){native=address;},setIdentity(handler){identity=handler;},facts(){return{counts:{...counts},bypass_attempts:bypass};},close:()=>new Promise(resolve=>server.close(resolve))};
}
