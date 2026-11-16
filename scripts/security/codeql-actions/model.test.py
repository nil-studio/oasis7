#!/usr/bin/env python3
"""Run the real Actions extractor/query against temporary workflows; never execute them."""
import argparse
import json
from pathlib import Path
import subprocess
import tempfile

from verify import RULE_ID, WRAPPER, verify

STEPS = '''    steps:
      - uses: actions/checkout@v4
        with:
          ref: ${{ inputs.ref }}
          persist-credentials: false
      - run: npm install
'''
HEADER = '''name: cache-mode fixture
on:
  workflow_dispatch:
    inputs:
      ref:
        type: string
permissions:
  contents: read
'''


def fixture(workflow='', job='', *, trigger=None):
    header = HEADER if trigger is None else HEADER.replace(
        '  workflow_dispatch:\n    inputs:\n      ref:\n        type: string\n', '  ' + trigger + ':\n')
    return header + workflow + 'jobs:\n  check:\n    runs-on: ubuntu-latest\n' + job + STEPS


def fixtures():
    # Expected findings are independent of the predicate implementation.
    cases = {
        'default': (fixture(), True),
        'workflow-read': (fixture('cache-mode: read\n'), False),
        'workflow-none': (fixture('cache-mode: none\n'), False),
        'job-read': (fixture(job='    cache-mode: read\n'), False),
        'job-none': (fixture(job='    cache-mode: none\n'), False),
        'job-write-overrides-read': (fixture('cache-mode: read\n', '    cache-mode: write\n'), True),
        'job-read-overrides-write': (fixture('cache-mode: write\n', '    cache-mode: read\n'), False),
        'job-expression': (fixture('cache-mode: read\n', "    cache-mode: ${{ inputs.mode }}\n"), True),
        'job-unknown': (fixture('cache-mode: read\n', '    cache-mode: unexpected\n'), True),
        'job-nonscalar': (fixture('cache-mode: read\n', '    cache-mode: [read]\n'), True),
        'job-duplicate': (fixture('cache-mode: read\n', '    cache-mode: read\n    cache-mode: write\n'), True),
        'workflow-duplicate': (fixture('cache-mode: read\ncache-mode: write\n'), True),
        'job-value-alias': (fixture('x-mode: &mode read\ncache-mode: read\n', '    cache-mode: *mode\n'), True),
        'workflow-value-alias': (fixture('x-mode: &mode read\ncache-mode: *mode\n'), True),
        'job-key-alias': (fixture('x-key: &key cache-mode\ncache-mode: read\n', '    *key: write\n'), True),
        'job-merge': (fixture('x-job: &defaults {cache-mode: write}\ncache-mode: read\n', '    <<: *defaults\n'), True),
        'job-merge-and-direct': (fixture('x-job: &defaults {cache-mode: write}\n', '    <<: *defaults\n    cache-mode: read\n'), True),
        'workflow-merge': (fixture('x-workflow: &defaults {cache-mode: read}\n<<: *defaults\n'), True),
        # Cache mode must not expand the original default-branch event/cache scope.
        'pr-merge-scope': (fixture('cache-mode: write\n', trigger='pull_request'), False),
        'target-original-scope': (fixture('cache-mode: write\n', trigger='pull_request_target'), False),
    }
    for mode, expected in (('read', False), ('write', True)):
        shared = HEADER + 'jobs:\n  first:\n    runs-on: ubuntu-latest\n    cache-mode: read\n'
        shared += STEPS.replace('steps:', 'steps: &shared', 1)
        shared += '  second:\n    runs-on: ubuntu-latest\n    cache-mode: ' + mode + '\n    steps: *shared\n'
        cases['shared-steps-' + mode] = (shared, expected)
    return cases


def findings(path):
    sarif = json.loads(path.read_text())
    results = [result for run in sarif['runs'] for result in run.get('results', [])]
    if any(result['ruleId'] != RULE_ID for result in results):
        raise AssertionError('unexpected rule in isolated regression scan')
    return {Path(result['locations'][0]['physicalLocation']['artifactLocation']['uri']).stem for result in results}


def run(codeql, *args):
    subprocess.run([codeql, *map(str, args)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--codeql', required=True)
    parser.add_argument('--initialized-suite', type=Path)
    parser.add_argument('--profile', choices=('default', 'extended'))
    args = parser.parse_args()
    stock = verify(args.codeql, initialized_suite=args.initialized_suite, profile=args.profile)
    cases = fixtures()
    with tempfile.TemporaryDirectory(prefix='oasis7-codeql-cache-mode-') as directory:
        root = Path(directory)
        source = root / 'source'
        workflows = source / '.github/workflows'
        workflows.mkdir(parents=True)
        for name, (content, _) in cases.items():
            (workflows / (name + '.yml')).write_text(content)
        database = root / 'database'
        run(args.codeql, 'database', 'create', database, '--language=actions', '--source-root=' + str(source),
            '--build-mode=none', '--threads=2', '--ram=3072')
        for name, query in (('upstream', stock), ('local', WRAPPER)):
            report = root / (name + '.sarif')
            run(args.codeql, 'database', 'analyze', database, query, '--format=sarif-latest',
                '--output=' + str(report), '--threads=2', '--ram=3072')
            expected = {key for key, (_, alert) in cases.items()
                        if (alert if name == 'local' else key not in ('pr-merge-scope', 'target-original-scope'))}
            actual = findings(report)
            if actual != expected:
                raise AssertionError(f'{name}: missing={sorted(expected - actual)}, unexpected={sorted(actual - expected)}')
            print(f'{name}: {len(actual)} expected workflow findings', flush=True)
    print(f'PASS: {len(cases)} real Actions extraction fixtures; read/none removed, write/unknown retained')


if __name__ == '__main__':
    main()
