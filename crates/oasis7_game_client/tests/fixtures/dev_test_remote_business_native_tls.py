"""Manual, explicitly authorized native read-window fixture; never run in CI.
Requires the trusted owner to resume the existing fixed149 world and approve a new
600s same-grant read-only window. Uses existing private configuration internally;
performs only open B and lookup B4, expected usage 50 -> 52/64. No model/Action.
Source binary d749; this fixture records the tested source/dependency combination.
"""
import pathlib,json,subprocess,os,time,base64,hashlib,http.server,http.client,ssl,threading,socket
r=pathlib.Path('/tmp/oasis7-5534-real-374048cc827e');t=r/'tls-5539';v=json.loads((t/'run-input.json').read_text());w=r/'write-service-5536';p=w/'policy.json';b=r/'bin-5536/oasis7_dev_test_sessions-0e0eb8f83';c=json.loads(p.read_text());counts=lambda:sum(json.loads(f.read_text())['used'] for f in (w/'usage').glob('*.json'));assert counts()==50
# Explicit parent-authorized new read-only window; stable accounting binding retained.
expiry=int(time.time())+600
for g in c['grants']:g['expires_at_unix']=expiry;g['methods']=['describe','task','capabilities','lookup']
for cred in c['service']['credentials']:cred['expires_at_unix']=expiry
c['service']['max_request_bytes']=8192;c['service']['timeout_ms']=30000
candidate=t/'candidate.json';candidate.write_text(json.dumps(c));candidate.chmod(0o600)
assert subprocess.run([str(b),'--apply',str(p),str(candidate)],capture_output=True).returncode==0
c=json.loads(p.read_text());token=next(x['token'] for x in c['service']['credentials'] if x['caller_id']=='bounded-write-tester');env=os.environ.copy();env['SSL_CERT_FILE']=json.loads((r/'runtime-private.json').read_text())['environment']['SSL_CERT_FILE'];accounts=json.loads((r/'read-5535-result.json').read_text())['accounts'];unlocks={a['account_ref']:base64.b64encode((r/'unlock.bin').read_bytes()).decode() for a in accounts}
def movements():
 e=env.copy();e['PYTHONPATH']='/tmp/oasis7-5536-observer';return json.loads(subprocess.run(['python3','/tmp/oasis7-5536-observer/world_observer.py',str(r)],env=e,capture_output=True,check=True,timeout=30).stdout)['movement_events']
prior=movements();assert len(prior)==2
class Proxy(http.server.BaseHTTPRequestHandler):
 def log_message(self,*a):pass
 def do_POST(self):
  body=self.rfile.read(int(self.headers['Content-Length']));h={'Host':v['origin'][8:],'Content-Type':'application/json','Accept':'application/json, text/event-stream','MCP-Protocol-Version':'2025-11-25'}
  if 'Authorization' in self.headers:h['Authorization']=self.headers['Authorization']
  conn=http.client.HTTPConnection('127.0.0.1',v['http_port'],timeout=35);conn.request('POST','/mcp',body,h);reply=conn.getresponse();data=reply.read();conn.close();self.send_response(reply.status);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
proxy=http.server.HTTPServer(('127.0.0.1',v['tls_port']),Proxy)
ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
ctx.minimum_version=ssl.TLSVersion.TLSv1_2
ctx.load_cert_chain(t/'cert.pem',t/'key.pem')
proxy.socket=ctx.wrap_socket(proxy.socket,server_side=True)
assert not pathlib.Path(c['service']['socket']).exists();daemon=subprocess.Popen([str(b),'--serve',str(p)],stdin=subprocess.PIPE,stderr=(t/'backend-private.log').open('ab'),env=env);daemon.stdin.write(json.dumps(unlocks).encode());daemon.stdin.close();remote=None;thread=None
try:
 until=time.monotonic()+10
 while not pathlib.Path(c['service']['socket']).exists():assert daemon.poll() is None and time.monotonic()<until;time.sleep(.05)
 remote=subprocess.Popen([str(r/'bin-5539/remote-tools-d749'),str(p),v['origin'],'127.0.0.1:'+str(v['http_port']),'8'],stderr=(t/'remote-private.log').open('ab'),env=env);time.sleep(.2);assert remote.poll() is None
 thread=threading.Thread(target=proxy.serve_forever);thread.start()
 def request(value,auth=True):
  conn=http.client.HTTPSConnection('localhost',v['tls_port'],context=ssl.create_default_context(cafile=str(t/'cert.pem')),timeout=35);headers={'Content-Type':'application/json'}
  if auth:headers['Authorization']='Bearer '+token
  conn.request('POST','/mcp',json.dumps(value),headers);reply=conn.getresponse();body=reply.read();conn.close();assert token.encode() not in body;return reply.status,json.loads(body) if body else None
 discovery={'jsonrpc':'2.0','id':'discovery','method':'tools/list'};assert request(discovery,False)[0]==401;assert counts()==50;assert request(discovery)[0]==200;assert counts()==50
 def tool(args):
  status,result=request({'jsonrpc':'2.0','id':'tls-5539','method':'tools/call','params':{'name':'dev_test_accounts_call','arguments':args}});assert status==200;return result['result']['structuredContent']
 opened=tool({'method':'open','grant_ref':'B-write','expires_at_unix':int(time.time())+180});assert opened['status']=='ok';answer=tool({'method':'lookup','session_ref':opened['result']['session_ref'],'operation':{'request_id':'B4-remote-5539-lookup','arguments':{'kind':'lookup','original_request_id':'B4-move'}}});assert answer['status']=='ok';obs=answer['result']['observation'];obs=obs.get('operation',obs);assert obs['outcome']['status']=='committed' and obs['outcome']['receipt']['commit_record']['action_id']=='action:5';assert counts()==52 and movements()==prior
 result=dict(v,state='passed',used=52,movement_events_unchanged=2,original_result=answer,window_started=expiry-600,window_expires=expiry);(t/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({'state':'passed','used':52,'action_id':'action:5','movement_events':2}))
finally:
 if thread:proxy.shutdown();thread.join()
 proxy.server_close()
 if remote:remote.terminate();remote.wait(timeout=10)
 daemon.terminate();daemon.wait(timeout=30);pathlib.Path(c['service']['socket']).unlink(missing_ok=True)
