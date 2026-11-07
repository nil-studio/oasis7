# Offline telemetry fixture

This synthetic input is not a GitHub API response. It tests the explicitly prepared
`oasis7-engineering-telemetry-input/v1` boundary and makes no performance claim.
Run from the repository root:

```sh
python3 scripts/engineering_telemetry.py collect scripts/tests/fixtures/engineering-telemetry/input.json /tmp/telemetry-snapshot.json
# collect exits 1 after writing the snapshot because coverage is partial.
python3 scripts/engineering_telemetry.py report /tmp/telemetry-snapshot.json /tmp/telemetry-report.json /tmp/telemetry-report.md
python3 -m unittest discover -s scripts/tests -p 'test_engineering_telemetry*.py'
```

`collect-source KIND INPUT OUTPUT --reference RELATIVE_PATH --collection-id ID --collected-at UTC --origin
{github,local}` can also project one offline Actions jobs response, process JSONL,
build manifest v2, or compile metrics v2. Use kinds `actions-jobs`, `process-jsonl`,
`build-manifest`, and `compile-metrics`. It records a SHA-256 of the raw input and
returns 1 after writing a partial snapshot; it does not verify a reference or bind
an artifact to a run. The synthetic prepared input instead hashes the canonical
JSON representation of the supplied normalized record list, not original bytes.

Use only curated, non-sensitive inputs. Output is a narrow allowlist, but constrained
identifier fields are not a secret detector. Inputs are limited to 32 MiB; this is
not yet a complete request/time/byte collection budget. Field provenance, complete
source adapters, timing and DAG analysis, live collection and real CI/local
verification remain future work. A report does not issue a GitHub check or establish
a passing execution; execution results remain source values.
