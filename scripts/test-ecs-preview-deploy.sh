#!/usr/bin/env bash
set -euo pipefail
DEPLOY_TEST_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEPLOY_TEST_PYTHON="${OASIS7_DEPLOY_TEST_PYTHON:-python3}"
"$DEPLOY_TEST_PYTHON" -c 'import sys; assert sys.version_info >= (3, 10), "deployment tests require Python >= 3.10"'
DEPLOY_TEST_TEMP="$(mktemp -d "${TMPDIR:-/tmp}/oasis7-ecs-deploy-tests.XXXXXX")"
trap 'rm -rf -- "$DEPLOY_TEST_TEMP"' EXIT
"$DEPLOY_TEST_PYTHON" -m venv "$DEPLOY_TEST_TEMP/venv"
"$DEPLOY_TEST_TEMP/venv/bin/python" -m pip install --disable-pip-version-check --retries 1 --timeout 30 \
  -r "$DEPLOY_TEST_ROOT/scripts/ecs-preview-deploy-test.requirements.txt"
for DEPLOY_TEST_FILE in deploy-ecs-preview.test.py ecs-preview-remote.test.py ecs-preview-probe.test.py; do
  "$DEPLOY_TEST_TEMP/venv/bin/python" "$DEPLOY_TEST_ROOT/scripts/$DEPLOY_TEST_FILE"
done
