"""Replay the unchanged selector on frozen Git inputs and compare full payloads."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

CANDIDATE = '3929fc500d5994ca0359f00b19ded889fd6b37c0'
MAIN = 'd6f856b3e0b58eda5c73b18e9f14a031f22b5045'
BEFORE = 'a9740c16017aac42892ee9c7baa29061d9f91717'
ROOT = Path.cwd()


def git(*args, cwd=ROOT, env=None):
    return subprocess.check_output(['git', *args], cwd=cwd, env=env, text=True).strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def commands(revision):
    workflow = git('show', revision + ':.github/workflows/rust.yml')
    job = re.search(r'(?ms)^  select:\n(.*?)(?=^  [a-z][a-z0-9-]*:\n)', workflow)[1]
    names = ('Select using trusted baseline Git data', 'Publish workflow scheduling projection',
             'Resolve formal selection with conservative old-baseline fallback',
             'Preserve selection attempt for downstream reruns')
    blocks = re.split(r'(?m)^      - ', job)
    return ['\n'.join(line[10:] for line in
            next(block for block in blocks if f'name: {name}\n' in block)
            .split('        run: |\n', 1)[1].splitlines()) for name in names]


def replay(temp, scripts):
    repo = temp / 'repo'
    git('clone', '--quiet', '--shared', '--no-checkout', str(ROOT), str(repo))
    mirror = temp / 'mirror'
    git('clone', '--quiet', '--shared', '--bare', str(ROOT), str(mirror))
    git('update-ref', 'refs/heads/main', MAIN, cwd=mirror)
    git('remote', 'set-url', 'origin', str(mirror), cwd=repo)
    fixed = {f'GIT_{role}_{key}': value for role in ('AUTHOR', 'COMMITTER')
             for key, value in (('NAME', 'Selector replay'), ('EMAIL', 'test@example.invalid'),
                                ('DATE', '2026-11-25T00:00:00Z'))}
    merge = git('-c', 'commit.gpgsign=false', 'commit-tree', CANDIDATE + '^{tree}',
                '-p', MAIN, '-p', CANDIDATE, '-m', 'Frozen selector PR merge',
                cwd=repo, env=os.environ | fixed)
    cases = (
        ('pr', 'pull_request', 'refs/pull/7030/merge', merge, CANDIDATE, MAIN, MAIN, '', 'pr-fast', MAIN, 'false'),
        ('main', 'push', 'refs/heads/main', MAIN, MAIN, BEFORE, MAIN, '', 'main-fast', BEFORE, 'true'),
        ('schedule', 'schedule', 'refs/heads/main', MAIN, MAIN, '', MAIN, '', 'periodic-full', MAIN, 'true'),
        ('manual', 'workflow_dispatch', 'refs/heads/main', MAIN, MAIN, '', MAIN, CANDIDATE, 'candidate-full', MAIN, 'false'),
        ('branch-dispatch', 'workflow_dispatch', 'refs/heads/candidate', CANDIDATE, CANDIDATE, '', CANDIDATE, '', 'candidate-full', MAIN, 'false'),
        ('invalid-dispatch', 'workflow_dispatch', 'refs/heads/candidate', CANDIDATE, CANDIDATE, '', CANDIDATE, MAIN, None, None, None),
    )
    witness = {}
    for label, event, ref, checkout, source, before, workflow, requested, profile, policy, trusted in cases:
        git('checkout', '--quiet', '--detach', checkout, cwd=repo)
        out = temp / label
        out.mkdir()
        env = os.environ | dict(EVENT_NAME=event, GITHUB_REF=ref, GITHUB_SHA=checkout,
            GITHUB_WORKSPACE=str(repo), SOURCE_SHA=source, SOURCE_REF=requested, BASE_SHA=before,
            WORKFLOW_SHA=workflow, GITHUB_RUN_ID='12345', GITHUB_RUN_ATTEMPT='2',
            MAIN_FAST_BASE_TOKEN='', RUNNER_TEMP=str(out), GITHUB_OUTPUT=str(out / 'github-output'),
            GITHUB_STEP_SUMMARY=str(out / 'summary'), PYTHONDONTWRITEBYTECODE='1')
        result = subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', scripts[0]],
                                cwd=repo, env=env, text=True, capture_output=True)
        if profile is None:
            assert result.returncode != 0 and 'source_ref requires a full dispatch from main' in result.stdout
            assert not (out / 'selection.json').exists() and not (out / 'ci-authority').exists()
            witness[label] = ['rejected', result.returncode, digest(result.stdout.encode())]
            continue
        assert result.returncode == 0, result.stdout + result.stderr
        for script in scripts[1:]:
            subprocess.run(['bash', '-e', '-o', 'pipefail', '-c', script], cwd=repo, env=env, check=True)
        raw = (out / 'github-output').read_bytes()
        output = dict(line.split('=', 1) for line in raw.decode().splitlines())
        selection = (out / 'selection.json').read_bytes()
        expected_source = requested or source
        expected_test = expected_source if event in ('schedule', 'workflow_dispatch') else checkout
        assert (json.loads(selection)['execution_profile'], output['source_sha'], output['tested_sha'],
                output['policy_sha'], output['trusted_source']) == (profile, expected_source, expected_test, policy, trusted)
        assert json.loads(output['plan'])['selection_sha256'] == digest(selection)
        assert output['authority_artifact'] == 'ci-authority-12345-2'
        assert output['selection_artifact'] == 'ci-selection-12345-2'
        authority = {file.relative_to(out / 'ci-authority').as_posix(): digest(file.read_bytes())
                     for file in sorted((out / 'ci-authority').rglob('*')) if file.is_file()}
        assert set(authority) >= {'ci-required-scope.json', 'plan-rust-required-scope.py'}
        witness[label] = dict(selection=digest(selection), authority=authority,
                              outputs=digest(raw), output_keys=sorted(output))
        print(label, json.dumps(witness[label], sort_keys=True))
    return json.dumps(witness, sort_keys=True, separators=(',', ':')).encode()


assert git('rev-parse', 'HEAD^1') == CANDIDATE
assert git('rev-parse', CANDIDATE + '^1') == MAIN
assert git('rev-parse', MAIN + '^1') == BEFORE
scripts = commands(CANDIDATE)
assert scripts == commands(MAIN)
with tempfile.TemporaryDirectory(dir=os.environ['RUNNER_TEMP']) as directory:
    witness = replay(Path(directory), scripts)
with Path(os.environ['GITHUB_OUTPUT']).open('a') as output:
    output.write('witness=' + digest(witness) + '\n')
