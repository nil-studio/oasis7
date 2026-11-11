#!/usr/bin/env python3
"""Read-only, bounded runner evidence; completion is not a network health gate."""
import concurrent.futures
import ipaddress
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import socket
import struct
import time
from urllib.parse import urlsplit

HOSTS = ('azure.archive.ubuntu.com', 'archive.ubuntu.com',
         'security.ubuntu.com', 'packages.microsoft.com')
DNS_CODE = """import json,socket,sys
try:
 rows=socket.getaddrinfo(sys.argv[1],443,type=socket.SOCK_STREAM)
 print(json.dumps({'ok':True,'families':sorted(set(x[0] for x in rows)),
 'addresses':sorted(set(x[4][0] for x in rows))}))
except socket.gaierror as e:
 print(json.dumps({'ok':False,'errno':e.errno}))
"""


def command(argv, timeout):
    started = time.monotonic()
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        return dict(exit=result.returncode, stdout=result.stdout[:4096],
                    elapsed=round(time.monotonic()-started, 3))
    except subprocess.TimeoutExpired:
        return dict(exit=None, timeout=True, elapsed=round(time.monotonic()-started, 3))


def emit(kind, value):
    print(json.dumps(dict(kind=kind, data=value)), flush=True)


def configuration():
    metadata = {}
    for name in ('RUNNER_OS', 'RUNNER_ARCH', 'ImageOS', 'ImageVersion'):
        value = os.environ.get(name, 'unavailable')
        metadata[name] = value if re.fullmatch(r'[a-zA-Z0-9_.-]{1,80}', value) else 'redacted'
    metadata['proxy_variables_present'] = [n for n in
        ('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','http_proxy','https_proxy','all_proxy')
        if os.environ.get(n)]
    emit('runner', metadata)
    path = Path('/etc/resolv.conf')
    servers = []
    if path.exists():
        for line in path.read_text()[:16384].splitlines():
            words = line.split()
            if len(words) >= 2 and words[0] == 'nameserver':
                try:
                    servers.append(str(ipaddress.ip_address(words[1])))
                except ValueError:
                    servers.append('unparseable')
    emit('resolver_nameservers', servers)
    nss = Path('/etc/nsswitch.conf')
    if nss.exists():
        emit('nss_hosts', [l.strip() for l in nss.read_text().splitlines()
                           if l.startswith('hosts:')][:1])
    for filename in ('/etc/apt/apt-mirrors.txt',
                     '/etc/apt/sources.list.d/ubuntu.sources'):
        source = Path(filename)
        urls = []
        if source.exists():
            for word in source.read_text()[:16384].split():
                if word.startswith(('http://','https://','mirror+file:')):
                    parsed = urlsplit(word)
                    urls.append(dict(scheme=parsed.scheme,
                        host=parsed.hostname if parsed.hostname in HOSTS else 'redacted'))
        emit('apt_sources', dict(file=filename, exists=source.exists(), urls=urls))


def dns(host):
    result = command([sys.executable, '-c', DNS_CODE, host], 8)
    raw = result.pop('stdout', '')
    try:
        result['answer'] = json.loads(raw)
    except ValueError:
        result['answer'] = dict(ok=False, diagnostic='no_structured_answer')
    result['host'] = host
    return result


def transport(host):
    paths = {'azure.archive.ubuntu.com':'/ubuntu/dists/noble/InRelease',
             'archive.ubuntu.com':'/ubuntu/dists/noble/InRelease',
             'security.ubuntu.com':'/ubuntu/dists/noble-security/InRelease',
             'packages.microsoft.com':'/ubuntu/24.04/prod/dists/noble/InRelease'}
    curl = shutil.which('curl')
    if not curl:
        return dict(host=host, unavailable='curl')
    result = command([curl, '--disable', '--silent', '--connect-timeout','4','--max-time','8',
        '--max-filesize','1048576','--output',os.devnull,'--write-out',
        '%{http_code} %{time_namelookup} %{time_connect} %{time_appconnect} %{time_total}',
        ('http://' if host == HOSTS[0] else 'https://')+host+paths[host]], 10)
    result['timings'] = result.pop('stdout', '').strip()
    result['host'] = host
    # stderr may contain proxy credentials/configuration; never emit it.
    return result


def raw_dns(item):
    server, host = item
    started = time.monotonic()
    ident = int.from_bytes(os.urandom(2), 'big')
    query = struct.pack('!HHHHHH', ident, 0x0100, 1, 0, 0, 0)
    query += b''.join(bytes([len(p)])+p.encode('ascii') for p in host.split('.'))
    query += b'\x00'+struct.pack('!HH', 1, 1)
    result = dict(server=server, host=host)
    try:
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            sock.settimeout(2)
            sock.connect((server, 53))
            sock.send(query)
            data = sock.recv(4096)
        response_id, flags, questions, answers, _, _ = struct.unpack('!HHHHHH', data[:12])
        result.update(valid_header=response_id == ident and bool(flags & 0x8000),
                      rcode=flags & 15, answers=answers, truncated=bool(flags & 0x0200))
    except (OSError, struct.error) as error:
        result['error_type'] = type(error).__name__
    result['elapsed'] = round(time.monotonic()-started, 3)
    return result


def dns_path_probes():
    servers = ['127.0.0.53', '1.1.1.1', '8.8.8.8']
    upstream = Path('/run/systemd/resolve/resolv.conf')
    if upstream.exists():
        for line in upstream.read_text()[:16384].splitlines():
            fields = line.split()
            if len(fields) >= 2 and fields[0] == 'nameserver':
                try:
                    addr = ipaddress.ip_address(fields[1])
                    if addr.version == 4 and str(addr) not in servers:
                        servers.append(str(addr))
                except ValueError:
                    pass
    servers = servers[:4]
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for result in pool.map(raw_dns, [(s,h) for s in servers for h in HOSTS]):
            emit('explicit_dns_read_only', result)
    # Current public A records observed on the operator host; diagnostic only.
    # These are NOT installed as hosts entries or used for package installation.
    if any(os.environ.get(n) for n in
           ('HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','http_proxy','https_proxy','all_proxy')):
        emit('https_verified_diagnostic_ip', dict(skipped='proxy_environment_present'))
        return  # do not bypass operator proxy policy or misattribute proxy DNS
    curl = shutil.which('curl')
    for host, addr in [('archive.ubuntu.com','185.125.190.81'),
                       ('security.ubuntu.com','185.125.190.81')]:
        if not curl:
            continue
        path = '/ubuntu/dists/noble/InRelease' if host.startswith('archive') else '/ubuntu/dists/noble-security/InRelease'
        result = command([curl, '--disable','--silent','--connect-timeout','4',
            '--max-time','8','--max-filesize','1048576','--output',os.devnull,
            '--resolve', host+':443:'+addr, '--write-out','%{http_code} %{time_connect} %{time_appconnect}',
            'https://'+host+path], 10)
        result.update(host=host, diagnostic_ip=addr)
        result['timings'] = result.pop('stdout','').strip()
        emit('https_verified_diagnostic_ip', result)


def main():
    configuration()
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for value in pool.map(dns, HOSTS):
            emit('dns', value)
        for value in pool.map(transport, HOSTS):
            emit('http_transport', value)
    dns_path_probes()
    emit('collection', dict(completed=True, network_health_not_asserted=True))


if __name__ == '__main__':
    main()
