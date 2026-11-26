#!/usr/bin/env python3
"""Frozen real two-job proposal plus hostile mutations for trusted BASE migration."""
import importlib.util
import re
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('workflow_contract_authority', ROOT / 'scripts/ci-workflow.test.py')
authority = importlib.util.module_from_spec(spec)
spec.loader.exec_module(authority)

# Exact two job bodies captured from the first complete container proposal.
PROPOSAL = r'''name: Frozen actual portable job proposal

jobs:
  browser-archive-cache:
    cache-mode: write
    permissions:
      contents: read
      actions: read
      packages: read
    if: >-
      github.ref == 'refs/heads/main' && github.workflow_sha == github.sha &&
      (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'))
    runs-on: ubuntu-24.04
    timeout-minutes: 6
    container:
      image: ghcr.io/nil-studio/oasis7/ci-runtime@sha256:3984caa42d82963f93af752bf79e3a07838f2c94279cfbb2deeb08f715cd62c9
      credentials:
        username: ${{ github.actor }}
        password: ${{ github.token }}
      options: --user 1001:1001 --init
      volumes:
        - ${{ github.workspace }}:/home/runner/work/oasis7/oasis7
        - /home/runner/work/_temp:/home/runner/work/_temp
    defaults:
      run:
        shell: bash
        working-directory: /home/runner/work/oasis7/oasis7
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          ref: ${{ github.sha }}
          persist-credentials: false
      - name: Verify canonical portable cache container
        run: python3 scripts/ci-portable-cache-job.py browser
      - name: Verify current main cache source
        shell: bash
        run: |
          test "$GITHUB_REF" = refs/heads/main
          test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"
          test "$(git rev-parse HEAD)" = "$GITHUB_SHA"
      - name: Identify exact browser archives
        id: browser-meta
        run: python3 scripts/ci-browser-deps.py metadata --browser agent --github-output "$GITHUB_OUTPUT" --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Restore exact CJK font archive
        id: browser-archive-noto
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: restore
          contract: browser-archive
          component: fonts-noto-cjk
          key: ${{ steps.browser-meta.outputs.fonts_noto_cjk_key }}
      - name: Fetch or verify pinned CJK font archive
        id: prepare-browser-noto
        run: python3 scripts/ci-browser-deps.py fetch --component fonts-noto-cjk --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Admit bounded CJK font archive cache
        id: browser-admission-noto
        if: steps.browser-archive-noto.outputs.cache-hit != 'true'
        env:
          GH_TOKEN: ${{ github.token }}
          BROWSER_CACHE_KEY: ${{ steps.browser-meta.outputs.fonts_noto_cjk_key }}
        run: python3 scripts/ci-browser-deps.py admit-save --component fonts-noto-cjk --key "$BROWSER_CACHE_KEY" --github-output "$GITHUB_OUTPUT" --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Save verified CJK font archive
        if: >-
          success() && steps.prepare-browser-noto.outcome == 'success' &&
          steps.browser-archive-noto.outputs.cache-hit != 'true' &&
          steps.browser-admission-noto.outcome == 'success' && steps.browser-admission-noto.outputs.save == 'true'
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: save
          contract: browser-archive
          component: fonts-noto-cjk
          key: ${{ steps.browser-meta.outputs.fonts_noto_cjk_key }}
          admitted: ${{ steps.browser-admission-noto.outputs.save }}
          upload-bound: ${{ steps.browser-admission-noto.outputs.upload_bound_bytes }}
      - name: Restore exact Freefont archive
        id: browser-archive-freefont
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: restore
          contract: browser-archive
          component: fonts-freefont-ttf
          key: ${{ steps.browser-meta.outputs.fonts_freefont_ttf_key }}
      - name: Fetch or verify pinned Freefont archive
        id: prepare-browser-freefont
        run: python3 scripts/ci-browser-deps.py fetch --component fonts-freefont-ttf --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Admit bounded Freefont archive cache
        id: browser-admission-freefont
        if: steps.browser-archive-freefont.outputs.cache-hit != 'true'
        env:
          GH_TOKEN: ${{ github.token }}
          BROWSER_CACHE_KEY: ${{ steps.browser-meta.outputs.fonts_freefont_ttf_key }}
        run: python3 scripts/ci-browser-deps.py admit-save --component fonts-freefont-ttf --key "$BROWSER_CACHE_KEY" --github-output "$GITHUB_OUTPUT" --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Save verified Freefont archive
        if: >-
          success() && steps.prepare-browser-freefont.outcome == 'success' &&
          steps.browser-archive-freefont.outputs.cache-hit != 'true' &&
          steps.browser-admission-freefont.outcome == 'success' && steps.browser-admission-freefont.outputs.save == 'true'
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: save
          contract: browser-archive
          component: fonts-freefont-ttf
          key: ${{ steps.browser-meta.outputs.fonts_freefont_ttf_key }}
          admitted: ${{ steps.browser-admission-freefont.outputs.save }}
          upload-bound: ${{ steps.browser-admission-freefont.outputs.upload_bound_bytes }}
      - name: Restore exact Chrome archive
        id: browser-archive-chrome
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: restore
          contract: browser-archive
          component: chrome
          key: ${{ steps.browser-meta.outputs.chrome_key }}
      - name: Fetch or verify pinned Chrome archive
        id: prepare-browser-chrome
        run: python3 scripts/ci-browser-deps.py fetch --component chrome --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Admit bounded Chrome archive cache
        id: browser-admission-chrome
        if: steps.browser-archive-chrome.outputs.cache-hit != 'true'
        env:
          GH_TOKEN: ${{ github.token }}
          BROWSER_CACHE_KEY: ${{ steps.browser-meta.outputs.chrome_key }}
        run: python3 scripts/ci-browser-deps.py admit-save --component chrome --key "$BROWSER_CACHE_KEY" --github-output "$GITHUB_OUTPUT" --cache-base /home/runner/work/_temp/oasis7-browser-archives
      - name: Save verified Chrome archive
        if: >-
          success() && steps.prepare-browser-chrome.outcome == 'success' &&
          steps.browser-archive-chrome.outputs.cache-hit != 'true' &&
          steps.browser-admission-chrome.outcome == 'success' && steps.browser-admission-chrome.outputs.save == 'true'
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: save
          contract: browser-archive
          component: chrome
          key: ${{ steps.browser-meta.outputs.chrome_key }}
          admitted: ${{ steps.browser-admission-chrome.outputs.save }}
          upload-bound: ${{ steps.browser-admission-chrome.outputs.upload_bound_bytes }}

  worker-build-cache:
    cache-mode: write
    permissions:
      contents: read
      actions: read
      packages: read
    if: >-
      github.ref == 'refs/heads/main' && github.workflow_sha == github.sha &&
      (github.event_name == 'schedule' || (github.event_name == 'workflow_dispatch' && inputs.run_mode == 'full'))
    runs-on: ubuntu-24.04
    timeout-minutes: 12
    container:
      image: ghcr.io/nil-studio/oasis7/ci-rust-build@sha256:0b13d0b5ba9051617eb6ab77a5af324f066af79bb2753d664b780535c2fe13a3
      credentials:
        username: ${{ github.actor }}
        password: ${{ github.token }}
      options: --user 1001:1001 --init --tmpfs /opt/oasis7-mold:uid=1001,gid=1001,mode=0755,exec
      volumes:
        - ${{ github.workspace }}:/home/runner/work/oasis7/oasis7
        - /home/runner/work/_temp:/home/runner/work/_temp
        - /home/runner/work/_temp:/home/runner/.cache/oasis7/worker-build
    defaults:
      run:
        shell: bash
        working-directory: /home/runner/work/oasis7/oasis7
    env:
      HOME: /home/runner/work/_temp/portable-worker-home
      CARGO_HOME: /home/runner/work/_temp/portable-worker-home/.cargo
    steps:
      - uses: actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1 # v7.0.1
        with:
          ref: ${{ github.sha }}
          persist-credentials: false
      - name: Verify canonical portable cache container
        run: python3 scripts/ci-portable-cache-job.py worker
      - name: Verify current main cache source
        shell: bash
        run: |
          set -euo pipefail
          test "$GITHUB_REF" = refs/heads/main
          test "$GITHUB_WORKFLOW_SHA" = "$GITHUB_SHA"
          test "$(git rev-parse HEAD)" = "$GITHUB_SHA"
      - name: Configure original pinned mold without writing its auxiliary cache
        run: python3 .github/actions/setup-linux-mold/setup.py --install-dir /opt/oasis7-mold/3.0.0-v1
      - name: Identify pinned Worker build tool
        id: worker-meta
        run: python3 scripts/ci-worker-build.py metadata --github-output "$GITHUB_OUTPUT" --root /home/runner/.cache/oasis7/worker-build/0.7.5
      - name: Restore exact Worker build tool
        id: worker-tool-cache
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: restore
          contract: worker-build
          component: worker-build
          key: worker-build-cli-v1-${{ runner.os }}-${{ runner.arch }}-${{ env.RUST_TOOLCHAIN }}-0.7.5-${{ steps.worker-meta.outputs.definition_sha }}
      - name: Prepare verified Worker build tool
        id: prepare-worker
        shell: bash
        timeout-minutes: 11
        env:
          TOOL_CACHE_HIT: ${{ steps.worker-tool-cache.outputs.cache-hit }}
        run: |
          if [[ "$TOOL_CACHE_HIT" == true ]]; then
            python3 scripts/ci-worker-build.py check --root /home/runner/.cache/oasis7/worker-build/0.7.5
          else
            python3 scripts/ci-worker-build.py ensure --root /home/runner/.cache/oasis7/worker-build/0.7.5
          fi
      - name: Admit bounded Worker tool cache
        id: worker-tool-admission
        if: steps.worker-tool-cache.outputs.cache-hit != 'true'
        env:
          GH_TOKEN: ${{ github.token }}
          TOOL_CACHE_KEY: worker-build-cli-v1-${{ runner.os }}-${{ runner.arch }}-${{ env.RUST_TOOLCHAIN }}-0.7.5-${{ steps.worker-meta.outputs.definition_sha }}
        run: python3 scripts/ci-worker-build.py admit-save --key "$TOOL_CACHE_KEY" --github-output "$GITHUB_OUTPUT" --root /home/runner/.cache/oasis7/worker-build/0.7.5
      - name: Save verified Worker build tool
        if: >-
          success() && steps.prepare-worker.outcome == 'success' &&
          steps.worker-tool-cache.outputs.cache-hit != 'true' &&
          steps.worker-tool-admission.outcome == 'success' && steps.worker-tool-admission.outputs.save == 'true'
        uses: ./.github/actions/portable-cache-transport
        with:
          operation: save
          contract: worker-build
          component: worker-build
          key: worker-build-cli-v1-${{ runner.os }}-${{ runner.arch }}-${{ env.RUST_TOOLCHAIN }}-0.7.5-${{ steps.worker-meta.outputs.definition_sha }}
          admitted: ${{ steps.worker-tool-admission.outputs.save }}
          upload-bound: ${{ steps.worker-tool-admission.outputs.upload_bound_bytes }}

'''


class PortableContainerContract(unittest.TestCase):
    def jobs(self):
        return authority.workflow_jobs(PROPOSAL)

    def test_both_actual_proposed_jobs_retain_all_original_save_guards(self):
        jobs = self.jobs()
        authority.browser_archive_cache_writer(jobs['browser-archive-cache'])
        authority.worker_tool_cache_writer(jobs['worker-build-cache'])

    def test_image_mount_uid_role_and_scope_mutations_fail_closed(self):
        for kind, job in self.jobs().items():
            mode = 'browser' if kind == 'browser-archive-cache' else 'worker'
            checks = [('options: --user 1001:1001 --init', 'options: --user 0:0 --init'),
                      ('${{ github.workspace }}:/home/runner/work/oasis7/oasis7', '/:/home/runner/work/oasis7/oasis7'),
                      ('/home/runner/work/_temp:/home/runner/work/_temp', '/home/runner/work/_temp:/home/runner'),
                      ('/home/runner/work/_temp:/home/runner/work/_temp', '${{ runner.temp }}:/home/runner/work/_temp'),
                      ('password: ${{ github.token }}', 'password: untrusted'),
                      ('refs/heads/main', 'refs/heads/other'),
                      ('steps.' + ('browser-admission-noto' if mode == 'browser' else 'worker-tool-admission') + '.outputs.upload_bound_bytes', 'untrusted.bound'),
                      ('contract: ' + ('browser-archive' if mode == 'browser' else 'worker-build'), 'contract: foreign')]
            for before, after in checks:
                self.assertIn(before, job)
                with self.subTest(kind=kind, before=before), self.assertRaises(AssertionError):
                    changed = job.replace(before, after)
                    if mode == 'browser':
                        authority.browser_archive_cache_writer(changed)
                    else:
                        authority.worker_tool_cache_writer(changed)

    def test_preflight_and_mold_cannot_move_after_tool_io(self):
        for kind, job in self.jobs().items():
            checker = authority.browser_archive_cache_writer if kind == 'browser-archive-cache' else authority.worker_tool_cache_writer
            steps = re.findall(r'^      - .*?(?=^      - |\Z)', job, re.M | re.S)
            names = ['Verify canonical portable cache container']
            if kind == 'worker-build-cache':
                names.append('Configure original pinned mold without writing its auxiliary cache')
            for name in names:
                step = next(step for step in steps if '- name: ' + name + '\n' in step)
                remaining = job.replace(step, '', 1)
                first_step = remaining.replace('    steps:\n', '    steps:\n' + step, 1)
                for changed in (remaining + step, first_step):
                    with self.subTest(kind=kind, name=name), self.assertRaises(AssertionError):
                        checker(changed)

    def test_original_prepare_admission_and_exact_key_negatives_are_preserved(self):
        for kind, job in self.jobs().items():
            mode = 'browser' if kind == 'browser-archive-cache' else 'worker'
            checker = authority.browser_archive_cache_writer if mode == 'browser' else authority.worker_tool_cache_writer
            admission = 'browser-admission-noto' if mode == 'browser' else 'worker-tool-admission'
            for before, after in [('success()', 'always()'),
                                  ('steps.' + admission + ".outputs.save == 'true'", 'true'),
                                  ('admitted: ${{ steps.' + admission + '.outputs.save }}', 'admitted: true'),
                                  ('operation: restore', 'operation: lookup'),
                                  ('${{ steps.' + ('browser-meta.outputs.fonts_noto_cjk_key' if mode == 'browser' else 'worker-meta.outputs.definition_sha') + ' }}', 'foreign-key')]:
                self.assertIn(before, job)
                with self.subTest(kind=kind, before=before), self.assertRaises(AssertionError):
                    checker(job.replace(before, after))


if __name__ == '__main__':
    unittest.main()
