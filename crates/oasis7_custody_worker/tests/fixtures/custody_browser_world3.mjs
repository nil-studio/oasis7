// Test-only deployment: real public pins and TLS; no consent/result substitutes.
import assert from 'node:assert/strict';
import {rawBrowserRoles} from './custody_browser_session_v2.mjs';
import {createPrivateKey,createPublicKey} from 'node:crypto';
import {createServer} from 'node:https';
import {writeFileSync,readFileSync,chmodSync,mkdirSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
const callbackPath='/api/custody/execution/native-dispatch-claim-v2';
// The seed is fixture Account-only, never a Native or Host key. Native receives public pins.
const seed=Buffer.alloc(32,96).toString('hex');
const key=createPrivateKey({key:Buffer.concat([Buffer.from('302e020100300506032b657004220420','hex'),Buffer.from(seed,'hex')]),format:'der',type:'pkcs8'});
const publicKey=createPublicKey(key).export({format:'der',type:'spki'}).subarray(-32).toString('hex');
export async function world3BrowserFixture(directory,nativeOrigin){
  const tls=resolve(directory,'world3-callback');mkdirSync(tls,{mode:0o700});
  const ca=resolve(tls,'ca.pem'),caKey=resolve(tls,'ca.key'),leaf=resolve(tls,'leaf.pem'),leafKey=resolve(tls,'leaf.key'),csr=resolve(tls,'leaf.csr'),ext=resolve(tls,'leaf.ext');
  const openssl=args=>execFileSync('rtk',['proxy','openssl',...args],{stdio:'ignore'});
  openssl(['req','-x509','-newkey','rsa:2048','-nodes','-keyout',caKey,'-out',ca,'-days','1','-subj','/CN=Oasis7 World3 fixture CA','-addext','basicConstraints=critical,CA:TRUE','-addext','keyUsage=critical,keyCertSign,cRLSign','-addext','subjectKeyIdentifier=hash']);
  openssl(['req','-new','-newkey','rsa:2048','-nodes','-keyout',leafKey,'-out',csr,'-subj','/CN=localhost']);
  writeFileSync(ext,'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\nsubjectKeyIdentifier=hash\nauthorityKeyIdentifier=keyid:always\n',{mode:0o600});
  openssl(['x509','-req','-in',csr,'-CA',ca,'-CAkey',caKey,'-CAcreateserial','-out',leaf,'-days','1','-sha256','-extfile',ext]);
  chmodSync(caKey,0o600);chmodSync(leafKey,0o600);
  let identity;let callbackCount=0;
  const server=createServer({key:readFileSync(leafKey),cert:readFileSync(leaf)},async(req,res)=>{
    try{
      assert.ok(['127.0.0.1','::ffff:127.0.0.1'].includes(req.socket.remoteAddress));
      assert.equal(req.method,'POST');assert.equal(req.url,callbackPath);
      assert.equal(req.headers.cookie,undefined);assert.equal(req.headers['x-oasis7-csrf'],undefined);
      const parts=[];let size=0;for await(const part of req){size+=part.length;assert.ok(size<=128*1024);parts.push(part);}
      assert.ok(identity);callbackCount++;
      // Preserve exact NativeRoot proof headers and raw bytes for the actual Worker verifier.
      const response=await identity(new Request(callbackOrigin+callbackPath,{method:'POST',headers:req.headers,body:Buffer.concat(parts)}));
      res.writeHead(response.status,{'content-type':'application/json','cache-control':'no-store'});res.end(Buffer.from(await response.arrayBuffer()));
    }catch{res.writeHead(503,{'content-type':'application/json'});res.end('{"error_code":"world3_callback_fixture_unavailable"}');}
  });
  server.requestTimeout=5000;server.headersTimeout=5000;
  await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));const callbackOrigin=`https://localhost:${server.address().port}`;
  assert.notEqual(callbackOrigin,nativeOrigin);
  const now=Date.now();
  const issuer={namespace:'browser-account',service_id:'browser-account-service',environment:'test',key_id:'browser-account-machine-v2',issuer_epoch:'1',config_generation:'1',native_audience_origin:nativeOrigin};
  const pin=purpose=>({...issuer,public_key:publicKey,not_before:String(now-1000),not_after:String(now+600000),allowed_purpose:purpose});
  const pins=['consumed_pending','active_three_roots','native_current_attempt','native_term_current_attempt','native_preflight_access_v2','native_execution_prepare_v2','execution_dispatch_claim_v2'].map(pin);
  const config={api_version:2,pending_pin:pins[0],active_pin:pins[1],current_pin:pins[2],term_current_pin:pins[3],preflight_access_pin:pins[4],execution_prepare_pin:pins[5],execution_dispatch_pin:pins[6],execution_identity_origin:callbackOrigin,issuer_floor:{issuer_epoch:'1',config_generation:'1'},native_origin:nativeOrigin};
  return{
    nativeEnv:{OASIS7_CUSTODY_BROWSER_WORLD3:'true',OASIS7_CUSTODY_CLEANUP_MACHINE_V2_ENABLED:'true',OASIS7_CUSTODY_EXECUTION_PREPARE_DISPATCH_V2_ENABLED:'true',OASIS7_CUSTODY_CLEANUP_ACCOUNT_ISSUER_CONFIG:JSON.stringify(config),OASIS7_CUSTODY_CLEANUP_SQLITE_RESTORE_GUARD_PATH:resolve(directory,'native-machine-lineage'),OASIS7_CUSTODY_BROWSER_CALLBACK_CA:ca},
    configure(options,data,bridge){for(const w of Object.values(rawBrowserRoles(options))){
      w.bindings={...w.bindings,CUSTODY_CLEANUP_MACHINE_V2_ENABLED:'true',CUSTODY_EXECUTION_PREPARE_DISPATCH_V2_ENABLED:'true',CUSTODY_ACCOUNT_SERVICE_ISSUER_ENABLED:'true',CUSTODY_ACCOUNT_SERVICE_ISSUER:JSON.stringify(issuer),CUSTODY_ACCOUNT_SERVICE_ISSUER_PINS:JSON.stringify(pins),CUSTODY_NATIVE_BINDING_OWNER_PIN:JSON.stringify(data.managed_binding_owner_pin)};
      if(w.name==='owner'){w.bindings.CUSTODY_ACCOUNT_SERVICE_SIGNING_SEED=seed;w.outboundService=bridge;}
    }return options;},
    setIdentity(handler){identity=handler;},facts(){return{callback_requests:callbackCount,distinct_identity_origin:callbackOrigin!==nativeOrigin};},
    close:()=>new Promise(resolve=>server.close(resolve)),
  };
}
