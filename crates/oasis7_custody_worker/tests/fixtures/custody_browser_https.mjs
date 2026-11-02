// Dedicated local test RP. No routes to arbitrary services or synthetic MFA.
import assert from 'node:assert/strict';
import {createServer,request as httpsRequest} from 'node:https';
import {randomBytes} from 'node:crypto';
import {readFileSync,chmodSync} from 'node:fs';
import {resolve} from 'node:path';
import {execFileSync} from 'node:child_process';
export function normalizeBrowserPath(path) {
  return path.replace(/\/(account-provisions|client-enrollments|ceremonies|operations|credentials|sessions)\/[^/]+/g, '/$1/:id');
}
export async function browserHttps(directory,viewerRoot) {
  const key=resolve(directory,'localhost.key'),cert=resolve(directory,'localhost.crt');
  execFileSync('rtk',['proxy','openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',key,'-out',cert,'-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost'],{stdio:'ignore'});chmodSync(key,0o600);
  const proxySecret=randomBytes(32).toString('hex');
  let native=null,identity=null,bypass=0;
  const counts={};const phaseStatuses={};let fault=null;
  function armFault(kind) {
    assert.ok(['lost-complete','lost-owner-approve','wrong-ids','wrong-csrf'].includes(kind));
    assert.equal(fault,null);fault={kind,original:null,calls:0,identical_original:null,statuses:[],committed_reply_dropped:false,native_error_codes:[],downstream_statuses:[]};
  }
  function faultFacts() {return fault?{kind:fault.kind,calls:fault.calls,identical_original:fault.identical_original,
    upstream_statuses:[...fault.statuses],native_error_codes:[...fault.native_error_codes],downstream_statuses:[...fault.downstream_statuses],committed_reply_dropped:fault.committed_reply_dropped}:null;}

  const files=new Set(['viewer_passkey_account_module.js','viewer_custody_owner_setup_panel.js','viewer_custody_owner_setup_client.js','viewer_account_email_recovery.js','viewer_imported_key.js','viewer_auth_crypto.js','software_safe_constants.js']);
  const server=createServer({key:readFileSync(key),cert:readFileSync(cert)},async(req,res)=>{
    try {
      assert.ok(req.socket.remoteAddress==='127.0.0.1'||req.socket.remoteAddress==='::ffff:127.0.0.1');
      const hostValues=[];for(let i=0;i<req.rawHeaders.length;i+=2)if(req.rawHeaders[i].toLowerCase()==='host')hostValues.push(req.rawHeaders[i+1]);
      if(hostValues.length!==1||typeof hostValues[0]!=='string'||hostValues[0].length===0||hostValues[0].trim()!==hostValues[0]){res.writeHead(400,{'content-type':'application/json','connection':'close'});res.end('{"error_code":"local_fixture_physical_authority"}');return;}
      const physicalAuthority=hostValues[0];let physicalOrigin;
      try{const actual=new URL('https://'+physicalAuthority);if(actual.host!==physicalAuthority||actual.username||actual.password||actual.pathname!=='/'||actual.search||actual.hash)throw Error();physicalOrigin=actual.origin;}catch{res.writeHead(400,{'content-type':'application/json','connection':'close'});res.end('{"error_code":"local_fixture_physical_authority"}');return;}
      if(typeof req.url!=='string'||!req.url.startsWith('/')||req.url.startsWith('//')||req.url.includes('#')){res.writeHead(400);res.end();return;}
      const receivedUrl=new URL(req.url,physicalOrigin);
      if(req.url.includes('\\')||receivedUrl.origin!==physicalOrigin||receivedUrl.href!==physicalOrigin+req.url){res.writeHead(400,{'connection':'close'});res.end();return;}
      const values=name=>{const found=[];for(let i=0;i<req.rawHeaders.length;i+=2)if(req.rawHeaders[i].toLowerCase()===name)found.push(req.rawHeaders[i+1]);return found;};
      const lengths=values('content-length');
      if(['transfer-encoding','content-encoding','trailer'].some(name=>values(name).length>0)||lengths.length>1||lengths.length===1&&(!/^(0|[1-9][0-9]*)$/.test(lengths[0])||BigInt(lengths[0])>65536n)){res.writeHead(400,{'content-type':'application/json','connection':'close'});res.end('{"error_code":"local_fixture_framing"}');return;}
      const path=receivedUrl.pathname;
      if(path==='/_fixture/approve-intent'){bypass++;res.writeHead(403);res.end('forbidden');return;}
      if(path==='/'&&req.method==='GET'){res.setHeader('Content-Type','text/html');res.end('<!doctype html><meta charset=utf-8><title>Local custody UI</title><style>body{font:16px system-ui;padding:16px;max-width:720px;margin:auto}.stack{display:grid;gap:12px}button,textarea{font:inherit;max-width:100%}dd{margin:0 0 8px;overflow-wrap:anywhere}</style><main id=account class=stack></main><script type=module>import{mountPasskeyAccount}from"/viewer_passkey_account_module.js";window.surface=mountPasskeyAccount(document.querySelector("#account"));</script>');return;}
      if(files.has(path.slice(1))&&req.method==='GET'){res.setHeader('Content-Type','text/javascript');res.end(readFileSync(resolve(viewerRoot,path.slice(1))));return;}
      if(!path.startsWith('/api/public/hosted-account/v2/')&&!path.startsWith('/v1/')){res.writeHead(404);res.end();return;}
      const chunks=[];let size=0;for await(const chunk of req){size+=chunk.length;if(size>65536)throw new Error('bounded local request');chunks.push(chunk);}const body=Buffer.concat(chunks);
      let decoded;try{decoded=new TextDecoder('utf-8',{fatal:true}).decode(body);}catch{res.writeHead(400,{'connection':'close'});res.end();return;}
      if(!Buffer.from(decoded,'utf8').equals(body)||lengths.length===1&&BigInt(body.length)!==BigInt(lengths[0])){res.writeHead(400,{'content-type':'application/json','connection':'close'});res.end('{"error_code":"local_fixture_framing"}');return;}
      const headers=new Headers();for(const [name,value]of Object.entries(req.headers)){
        if(['cookie','origin','sec-fetch-site','x-oasis7-csrf','content-type','accept','content-digest','oasis7-token-digest','oasis7-enrollment','oasis7-request-id','oasis7-test-session','oasis7-binding','signature-input','signature'].includes(name)&&typeof value==='string')headers.set(name,value);
      }
      const countKey=req.method+' '+normalizeBrowserPath(path);counts[countKey]=(counts[countKey]??0)+1;
      const selected=fault && (fault.kind==='lost-owner-approve'?path.endsWith('/owner-setups/approve'):path.endsWith('/custody/approvals/complete'));
      let forwardedBody=body;
      if(selected) {
        fault.calls++;
        if(fault.calls===1)fault.original=Buffer.from(body);
        else fault.identical_original=body.equals(fault.original);
        if(fault.calls===1&&fault.kind==='wrong-ids') {const altered=JSON.parse(body);altered.ceremony_id='fixture-wrong-ceremony';forwardedBody=Buffer.from(JSON.stringify(altered));}
        if(fault.calls===1&&fault.kind==='wrong-csrf')headers.set('X-Oasis7-CSRF','fixture-wrong-csrf');
      }

      let response;
      if(path.startsWith('/api/')){assert.ok(native);response=await fetch(native+path,{method:req.method,headers,...(req.method==='GET'?{}:{body:forwardedBody}),redirect:'manual'});}
      else {assert.ok(identity);
        // No external MF-* header survives the allowlist above. Only this
        // actual TLS receiver may invoke Miniflare's authenticated proxy mode.
        // Its URL uses the physically received authority, including a wrong
        // authority; it never substitutes the signed/expected public origin.
        headers.set('host',physicalAuthority);headers.set('MF-Proxy-Shared-Secret',proxySecret);
        response=await identity(new Request(receivedUrl,{method:req.method,headers,...(req.method==='GET'?{}:{body:forwardedBody})}));}
      const responseBytes=Buffer.from(await response.arrayBuffer());
      const phase=path.endsWith('/custody/approvals/options')?'native_options':path.endsWith('/custody/approvals/complete')?'native_complete':path.endsWith('/owner-setups/prepare')?'owner_prepare':path.endsWith('/test-sessions/prepare')?'session_prepare':null;
      if(phase&&Number.isInteger(response.status)&&response.status>=100&&response.status<=599)phaseStatuses[phase]=response.status;

      if(selected) {
        fault.statuses.push(response.status);
        if(fault.calls===1&&['wrong-ids','wrong-csrf'].includes(fault.kind)) {
          // Consume only the fixed, nonsecret route error code. Never retain or
          // print raw Native response fields/body or use arbitrary 4xx as proof.
          let code=null;try {const value=JSON.parse(responseBytes);if(['invalid_request','csrf_failed'].includes(value.error_code))code=value.error_code;}catch {}
          fault.native_error_codes.push(code);
          const expected=fault.kind==='wrong-ids'?'invalid_request':'csrf_failed';
          assert.ok(code===expected,'exact Native rejection classification required');
        }
        if(fault.calls===1&&['lost-complete','lost-owner-approve'].includes(fault.kind)) {
          assert.equal(response.status,200,'drop only actual committed successful reply');fault.committed_reply_dropped=true;
          // The actual successful upstream body is withheld. A fixed gateway
          // failure makes uncertainty visible without empty-connection retries;
          // this is not claimed equivalent to a TCP packet-loss experiment.
          responseBytes.fill(0);fault.downstream_statuses.push(502);
          res.writeHead(502,{'content-type':'application/json','cache-control':'no-store','connection':'close'});
          res.end('{"error_code":"local_fixture_reply_lost"}');return;
        }
      }
      if(selected)fault.downstream_statuses.push(response.status);
      res.statusCode=response.status;for(const name of ['content-type','cache-control']){const value=response.headers.get(name);if(value)res.setHeader(name,value);}
      const cookies=response.headers.getSetCookie?.()??[];if(cookies.length)res.setHeader('Set-Cookie',cookies);
      res.end(responseBytes);
    }catch {res.writeHead(503,{'content-type':'application/json'});res.end('{"error_code":"local_fixture_unavailable"}');}
  });
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const origin=`https://localhost:${server.address().port}`;
  async function clientFetch(input,init){
    const request=new Request(input,init);const target=new URL(request.url);
    if(target.origin!==origin||!['GET','POST'].includes(request.method))throw Error('fixed local HTTPS target required');
    const bytes=request.method==='GET'?null:Buffer.from(await request.arrayBuffer());
    if(bytes&&bytes.length>65536)throw Error('bounded local HTTPS request');
    const headers=Object.fromEntries(request.headers);
    if(headers['transfer-encoding']||bytes&&headers['content-length']!==undefined&&headers['content-length']!==String(bytes.length))throw Error('local HTTPS framing mismatch');
    if(bytes)headers['content-length']=String(bytes.length);
    return new Promise((resolve,reject)=>{
      // One real TLS request with only this fixture's public cert as trust.
      // Host derives from the actual URL unless the caller explicitly supplies
      // another physical Host, which the receiver must preserve and reject.
      const outgoing=httpsRequest(target,{method:request.method,headers,ca:readFileSync(cert),servername:'localhost',rejectUnauthorized:true,agent:false,lookup:(_name,options,callback)=>options?.all?callback(null,[{address:'127.0.0.1',family:4}]):callback(null,'127.0.0.1',4)},incoming=>{
        const chunks=[];let length=0;
        incoming.on('data',chunk=>{length+=chunk.length;if(length>131072){outgoing.destroy(Error('bounded local HTTPS response'));return;}chunks.push(chunk);});
        incoming.on('end',()=>{const responseHeaders=new Headers();for(let i=0;i<incoming.rawHeaders.length;i+=2)responseHeaders.append(incoming.rawHeaders[i],incoming.rawHeaders[i+1]);resolve(new Response(Buffer.concat(chunks),{status:incoming.statusCode,headers:responseHeaders}));});
        incoming.on('error',()=>reject(Error('local HTTPS response unavailable')));
      });
      outgoing.setTimeout(20000,()=>outgoing.destroy(Error('local HTTPS request expired')));
      outgoing.on('error',()=>reject(Error('local HTTPS request unavailable')));
      outgoing.end(bytes);
    });
  }
  return {origin,proxySecret,clientFetch,armFault,setNative(address){native=address;},setIdentity(handler){identity=handler;},facts(){return{counts:{...counts},phase_statuses:{...phaseStatuses},bypass_attempts:bypass,fault:faultFacts()};},close:()=>new Promise(resolve=>server.close(()=>{fault?.original?.fill(0);resolve();}))};
}
