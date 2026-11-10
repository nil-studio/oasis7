"""Loopback trusted TLS proxy + real remote binary + private synthetic UDS backend.
No external connector/model and no world requests. Pass a built remote-tools binary.
"""
import http.client, http.server, json, os, pathlib, shutil, socket, ssl, subprocess, sys, tempfile, threading, time
binary=str(pathlib.Path(sys.argv[1]).resolve())
root=pathlib.Path(tempfile.mkdtemp(prefix='.remote-tls-',dir=str(pathlib.Path.home())));root.chmod(0o700)
backend_path=root/'backend.sock'; policy=root/'policy.json'; token='synthetic-private-token-01234567890123456789'
records=[];stopping=False
backend=socket.socket(socket.AF_UNIX);backend.bind(str(backend_path));backend_path.chmod(0o600);backend.listen();backend.settimeout(.1)
goal=dict(content=dict(goal_text='full goal\n'+'x'*2048,soft_constraints=['constraint A','constraint B']),source=dict(task_fence={'agent_id':'A'},active_goal_reference={'goal_ref':'synthetic'},owner_public_key='synthetic-owner',read_commit={'position':7},activation_commit={'position':2}))
def serve_backend():
    while not stopping:
        try: stream,_=backend.accept()
        except socket.timeout:continue
        with stream:
            body=b''
            while chunk:=stream.recv(4096):body+=chunk
            request=json.loads(body);assert request.pop('credential')==token;records.append(request)
            if request.get('operation',{}).get('request_id')=='lost-move-A':continue
            stream.sendall(json.dumps(dict(status='ok',result=dict(echo=request,active_goal=goal))).encode())
backend_thread=threading.Thread(target=serve_backend);backend_thread.start()
probe=socket.socket();probe.bind(('127.0.0.1',0));port=probe.getsockname()[1];probe.close()
class Proxy(http.server.BaseHTTPRequestHandler):
    def log_message(self,*args):pass
    def do_POST(self):
        body=self.rfile.read(int(self.headers['Content-Length']))
        connection=http.client.HTTPConnection('127.0.0.1',port,timeout=3)
        headers={'Host':origin.removeprefix('https://'),'Content-Type':'application/json','MCP-Protocol-Version':'2025-11-25','Accept':'application/json, text/event-stream'}
        for key in ['Authorization','Origin']:
            if key in self.headers:headers[key]=self.headers[key]
        connection.request('POST','/mcp',body,headers);response=connection.getresponse();data=response.read();connection.close()
        self.send_response(response.status);self.send_header('Content-Length',str(len(data)));self.end_headers();self.wfile.write(data)
proxy=http.server.HTTPServer(('127.0.0.1',0),Proxy);origin='https://localhost:'+str(proxy.server_port)
subprocess.run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-keyout',str(root/'key.pem'),'-out',str(root/'cert.pem'),'-days','1','-subj','/CN=localhost','-addext','subjectAltName=DNS:localhost'],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.minimum_version=ssl.TLSVersion.TLSv1_2
context.load_cert_chain(root/'cert.pem',root/'key.pem')
proxy.socket=context.wrap_socket(proxy.socket,server_side=True)
proxy_thread=threading.Thread(target=proxy.serve_forever);proxy_thread.start()
config=dict(service=dict(profile='dev-test-accounts-v1',socket=str(backend_path),usage_directory=str(root/'usage'),grant_id='fixture',max_request_bytes=65536,timeout_ms=1000,max_calls=100,credentials=[dict(caller_id='connector',token=token,expires_at_unix=int(time.time())+120,revoked=False)]),catalog_directory=str(root/'catalog'),grants=[],resources={})
def save():policy.write_text(json.dumps(config));policy.chmod(0o600)
save();process=subprocess.Popen([binary,str(policy),origin,'127.0.0.1:'+str(port),'100'],stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
def request(value,authenticated=True,origin_header=None):
    client=http.client.HTTPSConnection('localhost',proxy.server_port,context=ssl.create_default_context(cafile=str(root/'cert.pem')),timeout=4)
    headers={'Content-Type':'application/json'}
    if authenticated:headers['Authorization']='Bearer '+token
    if origin_header:headers['Origin']=origin_header
    client.request('POST','/mcp',json.dumps(value),headers);reply=client.getresponse();body=reply.read();client.close()
    assert token.encode() not in body
    return reply.status,json.loads(body) if body else None
def tool(args):return dict(jsonrpc='2.0',id='rpc-A',method='tools/call',params=dict(name='dev_test_accounts_call',arguments=args))
try:
    import select
    assert select.select([process.stderr],[],[],5)[0], 'binary startup timed out'
    assert b'remote_endpoint_listening' in process.stderr.readline(), 'binary rejected trusted fixture configuration'
    discovery=dict(jsonrpc='2.0',id=1,method='tools/list')
    assert (observed:=request(discovery,False))[0]==401 and not records, observed
    assert request(discovery,True,'https://evil.example')[0]==403 and not records
    status,value=request(discovery);assert status==200 and value['result']['tools'][0]['name']=='dev_test_accounts_call' and not records
    for method in ['accounts','open','close','describe','task','capabilities','context','move','lookup']:
        args=dict(method=method,session_ref='A')
        if method in ['context','move','lookup']:
            operation={'kind':method}
            if method=='move':operation.update(context_request_id='context-A',to=[1,0,0])
            if method=='lookup':operation.update(original_request_id='move-A')
            args['operation']=dict(request_id=method+'-A',arguments=operation)
        status,value=request(tool(args));assert status==200,value
        business=value['result']['structuredContent'];assert business['result']['echo']==args and business['result']['active_goal']==goal
    assert len(records)==9
    for field in ['credential','url','path','caller_id']:
        status,value=request(tool(dict(method='accounts',**{field:'forbidden'})));assert value.get('error') and len(records)==9
    lost=dict(method='move',session_ref='A',operation=dict(request_id='lost-move-A',arguments=dict(kind='move',context_request_id='context-A',to=[1,0,0])))
    status,value=request(tool(lost));unknown=value['result']['structuredContent'];assert unknown['status']=='unknown' and unknown['original_request_id']=='lost-move-A' and len(records)==10
    config['service']['credentials'][0]['revoked']=True;save();assert request(discovery)[0]==401 and len(records)==10
    print('PASS: local trusted TLS, nine closed tools, exact IDs/body/full goal, unauth/origin/secret denial, current revocation, received lost reply unknown; synthetic backend only')
finally:
    process.terminate();process.wait(timeout=3);proxy.shutdown();proxy.server_close();proxy_thread.join();stopping=True;backend_thread.join();backend.close();shutil.rmtree(root)
