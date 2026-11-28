#!/usr/bin/env python3
"""Exercise Provider transport and its original byte-level checker interface."""
import hashlib
from importlib import import_module
from io import BytesIO
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

handoff = import_module('ci-trusted-full-provider')


class ProviderHandoffTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.directory = Path(temporary.name).resolve()
        self.checkout = self.directory / 'checkout'
        self.checkout.mkdir()
        self.git('init', '-q')
        (self.checkout / 'source.txt').write_text('provider fixture\n')
        self.git('add', 'source.txt')
        self.git('-c', 'user.name=fixture', '-c', 'user.email=fixture@example.invalid', 'commit', '-qm', 'fixture')
        self.source = self.git('rev-parse', 'HEAD').strip()
        self.bundle = self.directory / 'original'
        provider = self.bundle / handoff.ProviderArtifact.DIRECTORY
        provider.mkdir(parents=True)
        self.marker = self.directory / 'checker-invocations'
        module = handoff.ProviderArtifact.MODULE
        files = {}
        for kind, path, body in (
            ('wasm', provider / (module + '.wasm'), b'fixture wasm'),
            ('metadata', provider / (module + '.metadata.json'), b'{}'),
            ('receipt', provider / (module + '.build-receipt.json'), b'{}'),
            ('checker', provider / 'sync_builtin_wasm_identity',
             f'#!/bin/sh\ntest "$1" = --verify-provider-artifact || exit 7\nprintf "+\\n" >> "{self.marker}"\n'.encode()),
        ):
            path.write_bytes(body)
            path.chmod(0o755 if kind == 'checker' else 0o644)
            files[kind] = {'path': path.relative_to(self.bundle).as_posix(), 'sha256': hashlib.sha256(body).hexdigest()}
        library = provider / 'lib/fixture.so'
        library.parent.mkdir()
        library.write_bytes(b'fixture library')
        library.chmod(0o755)
        self.record = dict(handoff.ProviderArtifact.identity(self.checkout), files=files, libraries=[
            {'path': library.relative_to(self.bundle).as_posix(), 'sha256': hashlib.sha256(library.read_bytes()).hexdigest()}
        ])
        (self.bundle / handoff.RECORD).write_text(json.dumps(self.record))
        (self.bundle / 'environment.sh').write_text('exit 91\n')
        self.archive = self.directory / 'provider.tar'
        self.sha = handoff.pack(self.checkout, self.bundle, self.source, '711', '1', self.archive)

    def git(self, *args):
        return subprocess.check_output(['git', *args], text=True, cwd=self.checkout)

    def restore(self, **overrides):
        options = dict(root=self.checkout, archive=self.archive, target=self.directory / 'out', source=self.source,
                       run='711', current_attempt='2', producer_attempt='1', artifact_id='55', expected_digest=self.sha)
        return handoff.consume(**(options | overrides))

    def rewrite(self, change, name='modified.tar'):
        updated = self.directory / name
        with tarfile.open(self.archive) as source, tarfile.open(updated, 'w', format=tarfile.USTAR_FORMAT) as target:
            rows = [(member, source.extractfile(member).read()) for member in source]
            for member, contents in change(rows):
                member.size = len(contents)
                target.addfile(member, BytesIO(contents))
        return dict(archive=updated, expected_digest=hashlib.sha256(updated.read_bytes()).hexdigest())

    def test_current_or_previous_attempt_same_run_validates_original_checker(self):
        for attempt in ('1', '2'):
            target = self.directory / f'out-{attempt}'
            result = self.restore(current_attempt=attempt, target=target)
            self.assertEqual(set(result), set(handoff.ProviderArtifact.VARIABLES))
            self.assertFalse((target / 'environment.sh').exists())
            self.assertEqual((target / self.record['libraries'][0]['path']).stat().st_mode & 0o777, 0o755)
        self.assertEqual(self.marker.read_text().splitlines(), ['+', '+', '+'])

    def test_wrong_run_attempt_tree_id_and_outer_digest_do_not_run_checker(self):
        (self.checkout / 'source.txt').write_text('other tree\n')
        cases = (
            ({}, 'tree'), ({'run': '712'}, 'run'), ({'producer_attempt': '3'}, 'future attempt'),
            ({'producer_attempt': '2'}, 'origin attempt'), ({'artifact_id': '55,56'}, 'numeric'),
            ({'source': '0' * 40}, 'trusted checkout'), ({'expected_digest': '0' * 64}, 'SHA-256 mismatch'),
        )
        for index, (overrides, reason) in enumerate(cases):
            with self.subTest(reason=reason), self.assertRaisesRegex(ValueError, reason):
                self.restore(target=self.directory / f'out-{index}', **overrides)
        self.assertEqual(self.marker.read_text().splitlines(), ['+'])

    def test_narrow_inventory_members_and_hashes_are_required_before_checker(self):
        receipt = self.record['files']['receipt']['path']
        cases = (
            (lambda rows: [item for item in rows if item[0].name != receipt], 'inventory'),
            (lambda rows: [*rows, (self.regular('other'), b'additional')], 'inventory'),
            (lambda rows: [*rows, (self.regular('../outside'), b'escape')], 'archive member'),
            (lambda rows: [*rows, (self.regular('link', tarfile.SYMTYPE), b'link')], 'archive member'),
            (lambda rows: [*rows, rows[-1]], 'archive member'),
            (lambda rows: [(member, b'changed' if member.name == receipt else data) for member, data in rows], 'hash mismatch'),
        )
        for index, (change, error) in enumerate(cases):
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                self.restore(target=self.directory / f'out-{index}', **self.rewrite(change, f'fixture-{index}.tar'))
        self.assertEqual(self.marker.read_text().splitlines(), ['+'])

    @staticmethod
    def regular(name, kind=tarfile.REGTYPE):
        member = tarfile.TarInfo(name)
        member.type = kind
        return member

    def test_bad_checker_mode_and_trailing_archive_rejected(self):
        checker = self.record['files']['checker']['path']

        def change_mode(rows):
            for member, _ in rows:
                if member.name == checker:
                    member.mode = 0o644
            return rows

        with self.assertRaisesRegex(ValueError, 'mode mismatch'):
            self.restore(**self.rewrite(change_mode))
        updated = self.directory / 'trailing.tar'
        updated.write_bytes(self.archive.read_bytes() + b'unlisted')
        with self.assertRaisesRegex(ValueError, 'trailing content'):
            self.restore(target=self.directory / 'trailing', archive=updated,
                         expected_digest=hashlib.sha256(updated.read_bytes()).hexdigest())
        self.assertEqual(self.marker.read_text().splitlines(), ['+'])

    def test_shell_never_rebuilds_after_rejected_handoff(self):
        driver = (Path(__file__).resolve().parent / 'ci-tests.sh').read_text()
        original = 'prepare_full_provider_artifact() {'
        command = original + driver.split(original, 1)[1].split('\n}\n', 1)[0] + '\n}\n'
        for use_handoff, expected, status in ((True, 'ci-trusted-full-provider.py consume', 17),
                                               (False, 'ci-test-artifacts.py prepare-provider', 18)):
            script = command + r'''
run() {
  printf '%s %s\n' "${2##*/}" "$3"
  case "$3" in
    consume) return 17 ;;
    prepare-provider) return 18 ;;
  esac
}
repo_root=$1
driver_dir=$repo_root
ci_python=python3
if [[ $2 == yes ]]; then
  export OASIS7_TRUSTED_FULL_PROVIDER_ARCHIVE=/absent SOURCE_SHA=sha
  export GITHUB_RUN_ID=711 GITHUB_RUN_ATTEMPT=2 OASIS7_TRUSTED_FULL_PROVIDER_ARTIFACT_ID=55
  export OASIS7_TRUSTED_FULL_PROVIDER_ATTEMPT=1 OASIS7_TRUSTED_FULL_PROVIDER_SHA256=digest
fi
prepare_full_provider_artifact
'''
            result = subprocess.run(['bash', '-c', script, 'test', str(self.checkout), 'yes' if use_handoff else 'no'],
                                    capture_output=True, text=True, check=False)
            self.assertEqual(result.returncode, status, result.stderr)
            self.assertEqual(result.stdout.splitlines(), [expected])

    def test_prod_consumer_rejects_prs_and_mismatched_workflow(self):
        trusted = {
            'GITHUB_ACTIONS': 'true', 'GITHUB_REPOSITORY': handoff.REPOSITORY,
            'GITHUB_REF': 'refs/heads/main', 'GITHUB_WORKFLOW_REF': handoff.WORKFLOW,
            'GITHUB_SHA': self.source, 'GITHUB_WORKFLOW_SHA': self.source,
            'GITHUB_JOB': 'full-regression', 'GITHUB_EVENT_NAME': 'schedule',
            'RUNNER_ENVIRONMENT': 'github-hosted', 'RUNNER_OS': 'Linux', 'RUNNER_ARCH': 'X64',
        }
        handoff.trusted_consumer(trusted, self.source)
        for name, changed in [('GITHUB_EVENT_NAME', 'pull_request'), ('GITHUB_WORKFLOW_SHA', '1' * 40),
                              ('GITHUB_JOB', 'candidate-full-regression'), ('GITHUB_REF', 'refs/pull/9/merge')]:
            with self.subTest(name=name), self.assertRaises(ValueError):
                handoff.trusted_consumer(trusted | {name: changed}, self.source)


if __name__ == '__main__':
    unittest.main()
