"""Secret-free, fixed read-only MCP transport for the isolated Codex acceptance runtime."""
import json, subprocess, sys
calls=0
for line in sys.stdin:
    request={}
    try:
        request=json.loads(line)
        identifier=request.get("id")
        method=request.get("method")
        if identifier is None: continue
        if method=="initialize":
            result={"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"oasis7-local-acceptance","version":"1"}}
        elif method=="ping": result={}
        elif method=="tools/list":
            result={"tools":[{"name":"dev_test_capabilities","description":"Read the permitted local test service capabilities.","inputSchema":{"type":"object","properties":{},"additionalProperties":False},"annotations":{"readOnlyHint":True,"destructiveHint":False}}]}
        elif method=="tools/call" and request.get("params",{}).get("name")=="dev_test_capabilities" and request["params"].get("arguments",{})=={}:
            if calls >= 5: raise ValueError("tool budget exhausted")
            calls += 1
            call=subprocess.run(["/opt/oasis7-tools/oasis7_game_client","dev-test-local-call","--socket","/run/oasis7-public/tools.sock"],input=json.dumps({"profile":"dev-test-accounts-v1","audience":"oasis7-dev-test-tools","method":"capabilities"}),capture_output=True,text=True,timeout=3)
            if call.returncode: raise ValueError("local tool unavailable")
            value=json.loads(call.stdout)
            result={"content":[{"type":"text","text":json.dumps(value)}],"structuredContent":value,"isError":value.get("status")!="ok"}
        else: raise ValueError("unsupported tool request")
        response={"jsonrpc":"2.0","id":identifier,"result":result}
    except Exception:
        response={"jsonrpc":"2.0","id":request.get("id") if isinstance(request,dict) else None,"error":{"code":-32602,"message":"local tool request rejected"}}
    print(json.dumps(response),flush=True)
