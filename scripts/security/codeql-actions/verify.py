#!/usr/bin/env python3
"""Fail closed if the Actions compatibility query is no longer a one-query replacement."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent
WRAPPER = ROOT / 'queries/CachePoisoningViaPoisonableStep.ql'
STOCK_PATH = 'Security/CWE-349/CachePoisoningViaPoisonableStep.ql'
RULE_ID = 'actions/cache-poisoning/poisonable-step'
CLI_VERSION = '2.27.4'
SOURCE_SHA = '1be292a1e54696ca543b4ad0f9b4537b9687caa7'
STOCK_SHA256 = '4107ce87661034b7870ab7bfd8a1718203baa044bc126aa144767d769f5878f1'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def output(codeql, *args):
    return subprocess.check_output([codeql, *map(str, args)], text=True)


def queries(codeql, suite):
    paths = json.loads(output(codeql, 'resolve', 'queries', suite, '--format=json'))
    require(isinstance(paths, list) and paths, 'empty or unknown query resolution')
    return {Path(path).resolve() for path in paths}


def verify(codeql, *, initialized_suite=None, profile=None):
    require(json.loads(output(codeql, 'version', '--format=json'))['version'] == CLI_VERSION,
            'CodeQL version changed; revalidate the Actions cache-mode compatibility query')
    libraries = json.loads(output(codeql, 'resolve', 'library-path', '--query=' + str(WRAPPER), '--format=json'))
    packs = {}
    for directory in libraries['libraryPath']:
        manifest = Path(directory) / 'qlpack.yml'
        if not manifest.is_file():
            continue
        raw = manifest.read_text()
        name = re.search(r'^name: (\S+)$', raw, re.M)
        if not name or name[1] not in ('codeql/actions-all', 'codeql/actions-queries'):
            continue
        require(name[1] not in packs, 'ambiguous resolved Actions pack')
        version = {'codeql/actions-all': '0.7.1', 'codeql/actions-queries': '0.6.39'}[name[1]]
        for pattern, expected in ((r'^version: (\S+)$', version),
                                  (r'^  sha: (\S+)$', SOURCE_SHA),
                                  (r'^  cliVersion: (\S+)$', CLI_VERSION)):
            value = re.search(pattern, raw, re.M)
            require(value is not None and value[1] == expected, 'resolved Actions pack changed: ' + name[1])
        packs[name[1]] = Path(directory).resolve()
    require(len(packs) == 2, 'missing resolved Actions packs')
    stock = packs['codeql/actions-queries'] / STOCK_PATH
    require(hashlib.sha256(stock.read_bytes()).hexdigest() == STOCK_SHA256, 'upstream query changed')
    model = packs['codeql/actions-all'] / 'codeql/actions/security/CachePoisoningQuery.qll'
    require(hashlib.sha256(model.read_bytes()).hexdigest() ==
            'd2c19e8d19b23ed620b2d656aff75a4bb56ed399c0492f1adb8e7b1f4fa60280', 'upstream cache scope model changed')
    # The only semantic change is the conservative negative guard; keep all upstream conditions.
    restored = WRAPPER.read_text().split('\n', 1)[1].replace('import LocalCacheMode\n', '', 1)
    restored = restored.replace('  not provenLocalReadOrNone(job) and\n', '', 1)
    require(restored == stock.read_text(), 'wrapper differs beyond the reviewed cache-mode guard')
    suites = {}
    for name, suffix, count in (('default', 'code-scanning', 18), ('extended', 'security-extended', 24)):
        upstream = queries(codeql, packs['codeql/actions-queries'] / ('codeql-suites/actions-' + suffix + '.qls'))
        current = queries(codeql, ROOT / (name + '.qls'))
        require(len(upstream) == count and len(current) == count, 'query count changed: ' + name)
        require(upstream - current == {stock} and current - upstream == {WRAPPER},
                'suite must replace exactly the poisonable-step query: ' + name)
        ids = [re.findall(r'@id\s+(\S+)', path.read_text()) for path in current]
        require(sum(RULE_ID in values for values in ids) == 1, 'duplicate or missing target rule ID: ' + name)
        suites[name] = current
    if initialized_suite is not None:
        require(profile in suites, 'initialized suite needs a known profile')
        require(queries(codeql, initialized_suite) == suites[profile], 'init did not select the reviewed query suite')
    return stock


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codeql', required=True)
    parser.add_argument('--initialized-suite', type=Path)
    parser.add_argument('--profile', choices=('default', 'extended'))
    args = parser.parse_args()
    verify(args.codeql, initialized_suite=args.initialized_suite, profile=args.profile)
    print('Actions compatibility verified: default 18 / extended 24; exactly one query replaced')


if __name__ == '__main__':
    main()
