#!/usr/bin/env python3
"""Offline SDK capability check; never instantiate a bucket or credential provider."""
import importlib.metadata
import importlib.util
import json
import os
from pathlib import Path
import sys


def no_network(event, _args):
    if event in ('socket.connect', 'socket.getaddrinfo'):
        raise AssertionError('SDK smoke must not access the network')


sys.addaudithook(no_network)
assert os.getuid() == os.getgid() == 1001
assert sys.version_info[:2] == (3, 12)
assert Path(sys.prefix) == Path('/opt/oasis7-oss')
assert not os.access(sys.prefix, os.W_OK)
rows = json.loads(Path(__file__).with_name('artifacts.json').read_text())
expected = {row['name']: row['version'] for row in rows if row['group'] == 'sdk'}
installed = {item.metadata['Name'].lower().replace('_', '-'): item.version
             for item in importlib.metadata.distributions()}
assert installed == expected, (installed, expected)
extension = importlib.util.find_spec('crcmod._crcfunext')
assert extension and extension.origin.endswith('.so'), extension
import crcmod.predefined
assert crcmod.predefined.mkPredefinedCrcFun('crc-32')(b'123456789') == 0xcbf43926
import oss2
from oss2.http import Request
request = Request('GET', 'https://offline.invalid/fixture', headers={})
oss2.StsAuth('smoke-id', 'smoke-secret', 'smoke-token')._sign_request(request, 'smoke-bucket', 'fixture')
assert request.headers['x-oss-security-token'] == 'smoke-token'
assert request.headers['authorization'].startswith('OSS smoke-id:')
assert request.url == 'https://offline.invalid/fixture'
assert not importlib.util.find_spec('pip')
print(json.dumps({'sdk': expected, 'crc_c_extension': True, 'crc_vector': True,
                  'explicit_fake_sts_signature': True, 'network': False}, sort_keys=True))
