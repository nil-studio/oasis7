#!/usr/bin/env python3
"""Deterministic M component, not a paid/real model or Skill-compliance proof.
It genuinely reads the exact formal Skill bytes supplied by the trusted launcher,
then enters the real five-method CLI under M. No key, token or private config.
"""
import hashlib, json, subprocess, sys
payload = sys.stdin.buffer.read(1024 * 1024 + 1)
if len(payload) > 1024 * 1024:
    raise SystemExit(2)
request = json.loads(payload)
if set(request) != {'skill_text','skill_sha256','socket','resource_id'}:
    raise SystemExit(2)
text = request['skill_text']
if hashlib.sha256(text.encode()).hexdigest() != request['skill_sha256']:
    raise SystemExit(2)
if 'Custody native: closed Control-only profile' not in text:
    raise SystemExit(2)
if '<client> custody-control-call --socket <approved-custody-socket>' not in text:
    raise SystemExit(2)
methods = ['initialize','heartbeat','tools_list','request_finish','request_abort']
if any(method not in text for method in methods):
    raise SystemExit(2)
replies = []
for index, method in enumerate(methods):
    body = {'api_version':1,'resource_id':request['resource_id'],
        'request_id':'actual-host-v2-control-'+str(index),'method':method}
    result = subprocess.run(['/artifacts/host-cli','custody-control-call','--socket',request['socket']],
        input=json.dumps(body).encode(),stdout=subprocess.PIPE,stderr=subprocess.DEVNULL,
        timeout=35,check=False)
    if result.returncode != 0 or len(result.stdout) > 32768:
        raise SystemExit(2)
    reply = json.loads(result.stdout)
    if reply['status'] != 'accepted' or reply['request_id'] != body['request_id']:
        raise SystemExit(2)
    if reply['close_requested'] != method.startswith('request_'):
        raise SystemExit(2)
    replies.append({'method':method,'accepted':True})
print(json.dumps({'kind':'deterministic_component_not_real_model','formal_skill_bytes_loaded':True,
    'replies':replies}))
