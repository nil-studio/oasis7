# oasis7 Compile Metrics V2 warm/no-op check design

- Testing module authority: [`doc/testing/prd.md`](../prd.md)
- Current performance coverage authority: [`performance-coverage-gap-matrix-2026-06-09.md`](performance-coverage-gap-matrix-2026-06-09.md)
- Approval and acceptance source: [GitHub Issue #3606](https://github.com/eng-cc/oasis7/issues/3606)
- Implementation surfaces: [`scripts/ci-compile-metrics.sh`](../../../scripts/ci-compile-metrics.sh), [`scripts/ci-compile-metrics-gate.py`](../../../scripts/ci-compile-metrics-gate.py), [`scripts/ci-compile-metrics-contract.test.sh`](../../../scripts/ci-compile-metrics-contract.test.sh), and [`.github/workflows/compile-metrics.yml`](../../../.github/workflows/compile-metrics.yml)

This document records the technical contract and current behavior for the
approved first Compile Metrics V2 slice: an opt-in warm/no-op `cargo check`
measurement. It is a design authority, not a task ledger. Task status,
execution evidence, review returns, and mutable acceptance state remain in the
actual PR/CI and any existing task (Issue/Project are optional).

## 1. Purpose and scope

The current compile-metrics harness answers a cold-build question: how much
dependency closure and wall-clock work is required to check a selected Cargo
surface from an isolated run-owned target directory outside the checkout? The
first V2 slice adds one
deliberately narrower question:

> After the cold check has completed, how long does an identical no-op check
> take when it reuses the same checkout's run-owned check target directory
> outside the checkout?

The warm metric is useful for detecting the cost of the ordinary developer or
agent inner loop when no source has changed. It does not claim to measure a
touched-source incremental rebuild, and it does not by itself prove that any
optimization made compilation faster.

The slice is intentionally bounded to:

- one optional `cargo check` immediately after the existing cold check;
- the same selected package, feature mode, offline/locked flags, normalized
  environment, and per-checkout run-owned check target directory outside the
  checkout as the cold check;
- current-only or current-versus-baseline JSON/Markdown reporting;
- an optional, explicitly supplied warm regression threshold;
- contract tests for command identity, target isolation, source immutability,
  schema identity, and fail-closed gate behavior.

The existing cold metrics, launcher `wasmtime` invariant, required-gate
planner, and default workflow behavior remain unchanged.

## 2. Current facts and evidence boundary

These facts describe the current repository behavior. The warm option, manual
workflow input, and optional threshold validation are implemented; the
following sections specify their contract and boundaries.

| Surface | Current behavior | Consequence for V2 |
| --- | --- | --- |
| `ci-compile-metrics.sh` | Measures one offline/locked cold `cargo check` per checkout by default. With `--measure-warm-check`, it immediately measures an identical no-op check after the cold check. Optional release build and binary size remain separate. It records commit OIDs, package identity, feature mode, and metric values. | `cargo_check_seconds` remains the cold value; warm duration and its identity bit are recorded separately. |
| Cargo target isolation | Each current/baseline checkout has a separate run-owned external check target. Cold and warm checks reuse that target within the same checkout; release builds use a separate target. `CARGO_HOME` may be shared for registry/source data. | Reuse stays within one checkout; current and baseline never share target outputs. |
| Measurement environment | Cargo calls normalize `CARGO_INCREMENTAL=0`, development/test debug settings, and an unset `RUSTC_WRAPPER`. Timed check/build commands run `--offline --locked` after host-target prefetch. | The warm check must use the same normalization and must not introduce network or wrapper-dependent timing. |
| `.github/workflows/compile-metrics.yml` | `workflow_dispatch` runs a three-platform matrix. `baseline_ref` is optional. `measure_warm_check` defaults to `false`; the warm threshold defaults to empty and is rejected unless warm measurement is enabled. | Warm measurement remains manual and opt-in. It does not add an automatic PR trigger or required check. |
| `ci-compile-metrics-gate.py` | Validates V2 warm identity/value fields and metric rows fail-closed. It evaluates a warm threshold only when explicitly supplied with a baseline; without a baseline, the summary reports that the threshold was skipped. | Warm reporting has no default threshold; selected thresholds retain fail-closed validation and comparison. |
| Performance matrix | Compile metrics remain `on-demand`; warm/no-op collection is opt-in and report-only unless a caller explicitly supplies a warm threshold. | Keep this manual path separate from required-gate and do not infer a default threshold. |

The current harness has no touched-source incremental scenario. A warm/no-op
result must never be presented as evidence for `touch-proto`, `touch-node`,
`touch-launcher-ui`, or `touch-wasm-abi`; those are future scenarios described
in Section 11.

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Testing technical specifications](../prd.md#4-technical-specifications) | Keep compile measurements scoped to their selected surface and distinguish manual performance evidence from required-gate or release claims. | [Warm measurement rollout](#9-rollout-and-rollback) | `qa_engineer`; compile-metrics workflow maintainers | No touched-source incremental claim, live hosted-runner baseline, or release-readiness claim follows from a warm/no-op sample. |

## 3. Non-goals and invariants

### 3.1 Non-goals

This slice does not:

- change Cargo dependencies, feature definitions, package boundaries, or
  runtime behavior;
- enable `sccache`, `RUSTC_WRAPPER`, incremental compilation, or debug info in
  the cold metric;
- change the required-gate changed-path planner or add an automatic pull
  request trigger;
- add a default warm threshold, claim a stable SLO, or make warm timing a
  release-blocking signal by default;
- split libp2p features, add workspace dependency governance, add a fast
  linker profile, or introduce `cargo-nextest`;
- measure a source mutation, a touched-file rebuild, a release-link warm path,
  or a cross-checkout shared target;
- copy task status, mutable evidence, or a task execution ledger into this
  document.

### 3.2 Invariants

1. Cold `cargo check` remains the first timed check and retains its current
   command, per-checkout run-owned target directory outside the checkout,
   environment, and gate semantics.
2. Warm measurement is opt-in and is never silently substituted for a missing
   cold measurement.
3. Warm and cold checks for one checkout use the same run-owned check target
   directory outside the checkout, while current and baseline check targets
   remain different.
4. Current and baseline warm-mode identity must match before a comparison can
   be used for threshold evaluation.
5. A source/worktree mutation during measurement fails the run closed; it is
   not converted into a successful warm result.
6. A missing, malformed, non-finite, or semantically inconsistent warm field
   cannot satisfy an explicitly requested warm threshold.

## 4. Target contract: option and workflow inputs

### 4.1 Harness option

Add the opt-in CLI flag:

```text
--measure-warm-check
```

When absent, the harness preserves the current one-check behavior and emits a
warm-disabled result according to Section 6. When present, each measured
checkout runs the warm check after its cold check.

The option applies to both check-only surfaces and release-build surfaces. A
release-build surface still measures the warm check around `cargo check`; its
release build remains a separate cold operation in its own per-checkout
run-owned release target directory outside the checkout.

### 4.2 Manual workflow inputs

The manual workflow exposes the same choice with these inputs:

| Input | Type/default | Contract |
| --- | --- | --- |
| `measure_warm_check` | boolean / `false` | Passes the opt-in flag to the harness for current and baseline measurements. |
| `max_cargo_check_warm_regression_pct` | string / empty | Optional non-negative finite percentage. It is passed to the gate only when supplied and warm measurement is enabled. |

The existing cold threshold inputs retain their names, defaults, and meaning.
The warm threshold has no default value. Supplying a warm threshold while
`measure_warm_check` is false is a configuration error and must fail before a
gate can report success; silently ignoring that threshold would make the
workflow claim ambiguous.

Before baseline fetch, measurement, or gate evaluation, the workflow runs an
unconditional input-validation step for every `workflow_dispatch` invocation;
it is not guarded by whether a baseline ref was supplied. The step validates
the choice input, parses every non-empty cold or warm threshold as a finite,
non-negative number, and rejects a supplied warm threshold when
`measure_warm_check` is false. The same validation is not deferred to the
baseline-conditional gate step, because otherwise invalid or contradictory
inputs could appear successful on a current-only run.

When no baseline is provided, the workflow may still collect and publish the
current warm value. As with current cold-only runs, no regression threshold is
evaluated without baseline metrics. The gate invocation therefore has a
current-only path after measurement (or an equivalent explicit no-baseline
step), and emits a visible `SKIP` result naming the selected threshold(s) that
were not evaluated. The summary must repeat that no-baseline distinction; it
must not look like a passing threshold verdict.

## 5. Measurement order and cold/warm isolation

For each checkout (`current`, then `baseline` when requested), the harness
follows this order:

```text
host-target prefetch (not timed)
  -> dependency-tree query (not timed)
  -> cold cargo check in the per-checkout run-owned <label>-check-target
     directory outside the checkout (timed)
  -> identical warm/no-op cargo check in that same external target directory
     (timed)
  -> optional release cargo build in the separate per-checkout run-owned
     <label>-release-target directory outside the checkout (timed)
  -> binary-size/readout and JSON serialization
```

The warm check is immediately after the cold check and before the release
build. This keeps release-link work from becoming an accidental input to the
warm signal. The warm command must have the same argument vector as the cold
command, including:

```text
cargo check --offline --locked -p <package> [--no-default-features]
```

The exact command is executed through the same normalized environment as the
cold command:

- `CARGO_TARGET_DIR` is the existing run-owned check target directory allocated
  to this checkout outside the checkout itself;
- `CARGO_HOME` is the caller/workflow Cargo home, as for the cold check;
- `CARGO_INCREMENTAL=0`;
- `CARGO_PROFILE_DEV_DEBUG=0`;
- `CARGO_PROFILE_TEST_DEBUG=0`;
- `RUSTC_WRAPPER` is unset.

The word *warm* refers only to reusing artifacts produced by the immediately
preceding cold check in that same per-checkout run-owned target directory. It
does not mean a shared target,
registry write, network request, changed source tree, or warm release link.
Current and baseline run-owned check target directories remain isolated even
when they share
`CARGO_HOME`; otherwise the comparison could measure cross-checkout cache
contamination rather than no-op behavior.

Before and after each checkout measurement, the harness records a source-state
fingerprint covering tracked content, tracked mode/type, symlink targets, and
untracked paths. The declared measurement output subtree (which the workflow
places under `output/compile-metrics/`) is excluded as harness-owned output;
target directories are outside the checkout. The output path must not alias a
tracked source path or a symlink into one. No other path is excluded. Any
difference fails the measurement and preserves the relevant log; it must not
produce a successful metrics JSON by pretending that the command was a no-op.

## 6. Metrics schema and identity

The V2 change extends the existing metrics payload; it does not redefine
existing cold fields.

### 6.1 V2 schema and backward compatibility

V2 metrics and comparison payloads carry `schema_version: 2`. The harness
always emits this version, including when warm measurement is disabled. The
gate may accept an unversioned legacy V1 cold-only payload for backward
compatibility only when no warm flag, warm field, warm row, or warm threshold
is requested; legacy artifacts must still satisfy the existing cold identity,
provenance, row, and arithmetic rules. A payload with an unknown version, or
any warm request against a payload without V2 fields, fails closed. Version is
a compatibility check, not a substitute for measurement identity.

### 6.2 Per-checkout metrics

Each `current.metrics.json` and `baseline.metrics.json` contains:

| Field | Type/condition | Meaning |
| --- | --- | --- |
| `schema_version` | integer `2` for V2; omitted only by accepted legacy V1 cold-only artifacts | Payload contract version. |
| `cargo_check_seconds` | finite non-negative number | Existing cold check wall-clock duration. |
| `warm_check_enabled` | boolean | Whether the warm check was requested for this measurement. This is part of measurement identity. |
| `cargo_check_warm_seconds` | finite non-negative number when enabled; JSON `null` when disabled | Timed no-op check immediately following the cold check. `null`, not `0`, represents disabled measurement. |

For every V2 per-checkout payload, `warm_check_enabled` is present and a
boolean, and `cargo_check_warm_seconds` is present with the exact invariant
`false -> null` and `true -> finite, non-negative number`. A disabled result
must never use `0` as a sentinel, and an enabled result must never use `null`
or a missing field. The comparison envelope repeats the V2 version and must
bind its `current` and `baseline` payloads to those same field values.

All existing fields remain present and retain their current meaning, including
`package`, `binary`, `check_only`, `no_default_features`, `commit_oid`,
`package_count`, dependency-presence flags, release timing, and binary size.

### 6.3 Measurement identity

The identity used to compare current and baseline is extended from:

```text
package, binary, check_only, no_default_features
```

to:

```text
package, binary, check_only, no_default_features, warm_check_enabled
```

Current and baseline must match every field exactly. A warm-enabled current
measurement paired with a warm-disabled baseline is not comparable, even if
both payloads contain a numeric-looking field. Missing `warm_check_enabled` or
`cargo_check_warm_seconds` in a V2 warm comparison is a schema failure, not a
request to infer a default.

The identity bit prevents a false comparison between a cold-only run and a run
whose `cargo_check_seconds` was followed by a warm observation. It also keeps
future identity extensions explicit instead of relying on the presence of a
metric row as an implicit mode signal.

### 6.4 Comparison and summary

`comparison.json` retains the existing `measurement_identity` envelope and
includes the new `warm_check_enabled` field. The gate owns a static
schema-known metric set, independent of caller-selected thresholds:

```text
KNOWN_METRICS = {
  package_count,
  cargo_check_seconds,
  cargo_build_release_seconds,
  release_binary_bytes,
  cargo_check_warm_seconds,
}
```

Threshold flags form a separate optional `SELECTED_THRESHOLDS` subset of that
set. A threshold flag cannot make an otherwise unknown row supported, and the
absence of a threshold cannot make a known report-only row invalid. Every
`metric_rows` entry must name one static known metric at most once. When a
baseline exists, each row's `baseline` and `current` values must equal the
same-named numeric fields in the `baseline` and `current` payloads, including
the warm row; the gate must reject a row that disagrees with either payload or
uses a row to smuggle a value for a null/disabled field. `delta` and `percent`
remain derived values and must satisfy the existing arithmetic rules.

When a baseline exists and both measurements have warm mode enabled,
`metric_rows` includes exactly one warm row in addition to any other measured
known metrics:

```json
{
  "metric": "cargo_check_warm_seconds",
  "baseline": 1.0,
  "current": 1.0,
  "delta": 0.0,
  "percent": 0.0
}
```

The example values are illustrative only; real values must be finite and
non-negative. When warm mode is disabled, the per-checkout field is `null` and
no warm row is emitted. The summary reports whether warm measurement was
enabled and, when enabled, prints the current warm duration and the paired
comparison row where applicable. It must continue to label
`cargo_check_seconds` as **cold**.

## 7. Gate and threshold behavior

The existing cold gate remains authoritative and is evaluated exactly as it is
today. The warm metric is report-only unless the caller supplies
`--max-cargo-check-warm-regression-pct` (wired from the manual workflow input).

| Warm mode | Warm threshold | Baseline | Result |
| --- | --- | --- | --- |
| disabled | empty | any | Existing cold/current-only behavior; no warm row. |
| enabled | empty | absent | Publish current warm report; no regression gate. |
| enabled | empty | present | Publish paired warm row; no warm threshold gate. |
| enabled | supplied | absent | Publish current warm report and emit an explicit no-baseline `SKIP` naming the warm threshold as not evaluated. |
| enabled | supplied | present | Validate and enforce the warm row using the existing regression formula. |
| disabled | supplied | any | Fail configuration closed; the supplied threshold has no valid measurement. |

Evaluation order is normative:

1. The unconditional workflow input validator runs first, including when no
   baseline is requested.
2. The gate validates the current payload and current side of the comparison
   envelope: accepted schema/version, current identity, current commit
   provenance, and V2 warm-field invariants. This stage does not require a
   paired comparison row.
3. If no baseline exists, emit the explicit current-only `SKIP` and summary
   wording, then stop. Do not enter paired row/percentage validation and do
   not require a baseline payload, warm row, or comparison row for this
   current-only result.
4. Only when a baseline exists does the gate validate the baseline payload and
   full identity/provenance match, then validate known metric rows and their
   payload bindings, followed by percentage arithmetic and selected
   thresholds. A missing warm row is rejected only in this baseline-present
   warm comparison path, never as part of the current-only `SKIP` path.

The warm threshold parser accepts only a finite, non-negative number. With a
baseline greater than zero, the percentage is:

```text
((current - baseline) / baseline) * 100
```

For `cargo_check_warm_seconds`, a zero baseline makes percentage evaluation
impossible. If no warm threshold is selected, the warm row is report-only and
may carry `percent: null`, rendered as `n/a`; the gate accepts that one
metric-specific non-evaluable result without inventing a percentage. If the
warm threshold is selected, the same zero-baseline row is a deterministic
failure because the requested threshold cannot be evaluated. Other known
metrics retain their existing percentage requirements unless a later,
metric-specific contract changes them.

For a baseline-present comparison, before threshold evaluation, the gate must
reject all of the following:

- missing or mismatched `measurement_identity`, including the warm bit;
- missing, duplicate, unsupported, or non-array `metric_rows` (support is the
  static known-metric set, not the selected threshold set);
- a missing warm row when baseline-present measurements both enable warm mode;
- non-finite, negative, or arithmetically inconsistent warm values;
- a row whose `baseline` or `current` value does not equal the corresponding
  payload field, or a warm row emitted for a disabled/null warm field;
- fabricated `delta` or `percent` values;
- stale or mismatched current/baseline commit provenance.

The workflow passes selected thresholds to the gate even when no baseline is
available, so the gate can emit the explicit current-only `SKIP`. The actual
regression gate remains non-blocking only for the absence of a baseline; input
validation and contradictory warm-mode configuration remain unconditional
failures.

The launcher-only `wasmtime` absence check and existing cold thresholds remain
unchanged. This slice does not add warm timing to required-gate or change the
required-gate planner's capability mapping.

## 8. Failure semantics and evidence

The harness and gate fail closed at the earliest point that makes the result
untrustworthy.

| Failure | Required behavior | Evidence boundary |
| --- | --- | --- |
| Cold check fails | Do not run or report a successful warm check. Return failure and preserve the cold log. | No metrics claim is valid. |
| Warm check fails | Return failure; do not fall back to `cargo_check_seconds` or emit a successful warm value. Preserve both check logs when available. | The run is incomplete, not a warm pass. |
| Warm check uses a different command, target, or environment | Contract test and implementation validation fail. | The result is not comparable. |
| Source/worktree fingerprint changes | Return failure and identify the changed checkout/path state. | No-op evidence is invalid. |
| Current/baseline identity mismatch | Abort comparison before threshold evaluation. | Existing cold rows must not mask the mismatch. |
| Warm row/schema/arithmetic is malformed | Gate returns deterministic failure. | Caller-authored or stale JSON cannot satisfy the gate. |
| Warm threshold supplied while warm mode is disabled, or any malformed threshold input | Unconditional workflow input validation fails before measurement/gate success. | No threshold is silently ignored. |
| Baseline fetch or ref resolution fails | Preserve existing baseline failure behavior; do not silently run a current-only result under a requested comparison. | No paired comparison exists. |
| No baseline requested | Publish current-only metrics and emit a visible gate `SKIP`; when a threshold was selected, name that threshold as not evaluated. | Current-only evidence is not a regression verdict. |

The workflow's `always()` summary/artifact steps may expose partial logs, but a
missing or incomplete summary is not a pass. Operators must inspect the first
measurement failure before debugging downstream summary or gate steps; a
missing `comparison.json` is normally a consequence, not the root failure.

## 9. Rollout and rollback

产品适用性：N/A；reason=本节仅规定开发编译量测工具的选择、运行记录和回退，不定义玩家体验或运行时性能目标；scope=warm/no-op measurement rollout。真实 hosted 量测、专业准确性及原有验证义务仍须独立满足。

### Phase A: contract implementation (implemented; still opt-in)

The CLI option, workflow inputs, schema fields, gate option, and contract tests
are implemented. Default workflow triggers and cold thresholds remain
unchanged. Current-only warm measurements validate command identity and
artifact shape; they do not make a release or merge claim from a warm number.

### Phase B: paired report-only observation

Run the manual workflow with a fixed baseline ref and warm mode enabled across
the existing platform matrix and selected surfaces. Compare warm values only
within the same run's current/baseline identity and runner family. Collect
noise and failure signatures before choosing any threshold. Existing cold
regression results remain the primary gate.

### Phase C: explicit threshold trials

When an owner has a supported warm baseline and noise boundary, pass an
explicit warm threshold for a bounded manual run. A warm threshold is a real
gate for that invocation, but it is not a repository default and does not
change required-gate coverage. A threshold trial must be reviewed as a
measurement-policy decision by `qa_engineer`; repository health owns the
contract/documentation alignment, not release go/no-go.

### Rollback

Disable `measure_warm_check` and omit the warm threshold. The cold-only path
continues to provide the existing metrics and gate. A failed warm observation
does not justify weakening or removing the cold gate.

Automatic PR dispatch, multi-run statistical gating, fixed performance
hardware, and touched-source scenarios require separate scope and evidence.

## 10. Test strategy and acceptance

The implementation must extend
`scripts/ci-compile-metrics-contract.test.sh` without relying on absolute
timing expectations. The contract tests should assert invocation identity,
ordering, output schema, and fail-closed behavior using the existing fake Cargo
and Git surfaces.

Required cases:

1. **Default compatibility:** without `--measure-warm-check`, exactly one cold
   check is invoked; cold metrics, release behavior, and existing identity
   remain valid; warm mode is false and the warm duration is `null`.
2. **Legacy cold compatibility:** an unversioned V1 cold-only payload remains
   accepted only without warm fields, rows, or thresholds; a warm request or
   warm row against a legacy payload fails closed, while every new harness
   payload is explicitly V2.
3. **Warm check-only invocation and artifact reuse:** with the option, exactly
   two check calls occur; their command arguments are identical, both are
   `--offline --locked`, the second follows the first, and both use the same
   per-checkout run-owned target directory outside the checkout. Fake Cargo
   writes a marker into that target on the first check and requires the second
   check to observe it, proving artifact reuse rather than only path equality.
4. **Warm release invocation:** a release surface has two check calls in the
   same external per-checkout target, followed by the existing release build
   in a different external per-checkout target; the warm check does not become
   a release-build metric, and fake Cargo markers prove those targets differ.
5. **Environment normalization:** every current and baseline Cargo call,
   including both checks, records incremental/debug/wrapper values normalized
   exactly as the cold contract requires.
6. **Baseline isolation:** warm current and warm baseline checks each reuse
   only their own external per-checkout target; current and baseline targets
   remain distinct; the shared Cargo home behavior remains unchanged.
7. **Cold-failure short-circuit:** a fake Cargo failure on the first check
   prevents the warm check, release build, and successful warm JSON; the cold
   log is preserved.
8. **Warm-failure evidence:** a fake Cargo failure on the second check returns
   failure, prevents the release build and successful warm value, and preserves
   both check logs when available.
9. **No source mutation:** fake commands that change tracked content, mode,
   symlink state, or an untracked path cause deterministic measurement failure;
   unchanged state succeeds. Every mutation case runs only in a disposable
   measurement copy/worktree, removes that disposable worktree through Git
   before directory cleanup, and asserts that the canonical task worktree's
   source fingerprint is byte-for-byte unchanged.
10. **Output-path safety:** an output path equal to, inside, or symlinked into
    a tracked source path is rejected; an external output path succeeds. This
    test must not use the canonical task worktree as a mutation fixture.
11. **Schema, identity, and row binding:** V2 warm fields obey
    `false -> null` and `true -> finite non-negative`; current/baseline warm
    identity mismatches, missing or unknown schema versions, stale provenance,
    missing fields, and fabricated rows fail closed. A known report row whose
    values disagree with the corresponding current/baseline payload fields
    fails even when no threshold is selected, while unknown rows fail even if a
    threshold flag names them.
12. **Warm report-only and zero-baseline behavior:** enabled warm metrics with
    no warm threshold produce a report; a zero-baseline warm row renders
    `percent: null` as `n/a` and does not block. The same row with an explicit
    warm threshold fails closed.
13. **Workflow input validation and wiring:** the manual YAML declares a
    boolean warm opt-in defaulting false and an empty warm threshold default;
    an unconditional validator rejects malformed thresholds and
    threshold-with-warm-disabled regardless of baseline presence. Tests also
    prove the opt-in reaches current/baseline harness calls and the selected
    warm threshold reaches the gate.
14. **Visible no-baseline skip:** with a warm threshold and no baseline, the
    workflow first performs unconditional input and current-payload validation,
    then invokes the current-only gate path. The gate/summary visibly say
    `SKIP` and name the threshold as not evaluated before any paired
    row/percentage validation; no warm row is required without a baseline.
    This is not reported as a passing regression verdict. A baseline-present
    warm comparison separately rejects a missing warm row.
15. **Warm threshold behavior:** an explicit finite threshold passes within the
    threshold and fails above it; missing rows, malformed numbers, fabricated
    arithmetic, and invalid CLI thresholds fail closed according to Section 7.
16. **Summary labels:** summaries explicitly distinguish cold
    `cargo_check_seconds`, warm enabled/disabled state, warm duration, paired
    warm rows, zero-baseline `n/a`, and no-baseline threshold `SKIP`.
17. **Cold preservation:** the existing package, cold check, release, binary,
    launcher `wasmtime`, and no-baseline gate cases remain green.

Acceptance for this design slice is therefore:

- the opt-in contract and manual workflow surface are unambiguous;
- warm and cold command/external-target/environment identity is testable;
- the V2 JSON/summary/gate schema distinguishes warm-enabled from
  warm-disabled and remains explicit about accepted legacy cold payloads;
- static known metrics, selected thresholds, and row/payload binding are
  independent and fail closed when inconsistent;
- explicit warm threshold use cannot be silently ignored;
- no-baseline threshold handling is visibly `SKIP`, and zero-baseline
  report-only warm rows are metric-specific `n/a`;
- cold reproducibility and required-gate coverage are unchanged; and
- source mutation, identity drift, malformed payloads, and unavailable
  baselines fail closed.

The relevant local verification for the implementation phase is:

```bash
bash ./scripts/ci-compile-metrics-contract.test.sh
./scripts/doc-governance-check.sh
git diff --check
```

The contract test exercises the implemented warm path, including command
identity, opt-in defaults, threshold validation, report-only output, and
failure behavior. Run it alongside the documentation governance check when
updating this authority.

## 11. Future touch-based incremental metrics

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Testing technical specifications](../prd.md#4-technical-specifications) | [Warm measurement rollout](#9-rollout-and-rollback) | Verify that the default run records cold-only metrics, opt-in mode records a separate no-op warm check, and only an explicit supported threshold can evaluate a paired warm comparison. | Run `bash ./scripts/ci-compile-metrics-contract.test.sh` ([contract test](../../../scripts/ci-compile-metrics-contract.test.sh)); its fake-Cargo fixtures cover the disabled default, command/target identity, report-only output, selected threshold, and failure behavior on this candidate. | The implementation PR and actual checks for the tested source; Issue #4065 is historical provenance, not a required task or receipt. | Does not establish hosted-runner performance, platform noise bounds, or release readiness. |

Warm/no-op is a prerequisite observation, not the final developer inner-loop
metric. A later Compile Metrics V2 slice may add these independent scenarios:

```text
cold-check
no-op-check
touch-proto
touch-node
touch-launcher-ui
touch-wasm-abi
```

Each touch scenario must use a clean, deterministic source fixture or copied
checkout, apply exactly one documented source mutation, and use its own
run-owned target directory outside the fixture checkout. It must not reuse the
no-op target or mutate the canonical task worktree. The fixture identity,
mutation kind, package/feature identity, target triple, toolchain, and source
commit must become explicit measurement identity fields before a touch result
can be compared.

Touch metrics need their own baseline/noise study and domain review. A
`touch-proto` or `touch-wasm-abi` contract may require the matching runtime or
WASM specialist to confirm that the fixture represents a meaningful boundary;
`qa_engineer` owns any decision to promote a touch threshold into a release or
required gate. No touch threshold, fixture, or automatic trigger is implied by
this warm/no-op design.

### 11.2 State/projection extraction: fixed-profile build experiments

Issue [#4948](https://github.com/nil-studio/oasis7/issues/4948) additionally requires real `cargo build` experiments for the state/projection boundary. These are manual build observations, separate from the implemented V2 `cargo check` schema and its thresholds. The no-op check above does not satisfy this obligation. Record results in the implementation PR or attached run artifacts; this design contains no claimed timing result.

Use functionally equivalent immutable baseline/candidate revisions containing only the extraction and necessary supporting changes. Freeze the native Rust toolchain, host target, `ecs` profile, default feature resolution, flags, Cargo configuration, runner image, jobs (choose 1 for serial unit attribution), dependency input identities and cache state. Record exact manifest/lock identities and `rustc -vV`. Do not compare the historical ECS timing sample to a different current profile or machine as a percentage improvement. Keep three conditions separate: cold dependencies, warm dependencies with workspace outputs absent, and warm workspace outputs. Provision these conditions in disposable measurement runners or repository-approved measurement fixtures; never clear an active development target or change the everyday `CARGO_TARGET_DIR` to manufacture a cold result.

For the production native root set, the canonical timed command is:

```bash
env -u RUSTC_WRAPPER CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build --locked --timings --profile ecs --message-format=json-render-diagnostics -p oasis7_chain_runtime -p oasis7_launcher_backend -p oasis7_viewer_runtime --bin oasis7_chain_runtime --bin oasis7_game_launcher --bin oasis7_viewer_live
```

Preserve JSON stdout, rendered compiler diagnostics, Cargo timing HTML and an external wall-clock/resource report outside the scanned checkout. Identify each compiler artifact by package ID, target name/kind, resolved features and profile, then record its `fresh` value. Keep binary package source identity validation separate from Cargo freshness. Timing HTML distinguishes frontend/codegen where available; unavailable fields are unknown rather than zero. If LLVM inspection is needed to establish concrete codec ownership, retain its exact invocation/profile and actual unit/symbol evidence separately; moving a type alias does not establish code-generation consolidation.

Each touched-source scenario starts from its own independently seeded fixture with the same completed production build. Run an unchanged identical rebuild first and record its check/link overhead. Then apply exactly one reviewed, deterministic source mutation that preserves behavior and rerun the identical command. Never accumulate mutations across scenarios or edit the canonical task worktree. Freeze and retain the patch bytes and pre/post input digests, not just an mtime `touch`. Include equivalent mutations at the corresponding baseline owner when comparing revisions.

| Scenario | Required observation |
| --- | --- |
| Same-source no-op build | Valid units remain Fresh; separately report metadata/check/link overhead. |
| Runtime execution orchestration only | State and projection libraries remain Fresh when their actual inputs are unchanged; real execution adapters rebuild as needed. |
| Service reader authentication/construction only | State and projection libraries remain Fresh; applicable permission tests still execute separately. |
| Viewer presentation/read adapter only | State and projection libraries remain Fresh when unchanged; do not infer that all Viewer paths lost Runtime dependencies. |
| State schema or required reducer change | State and its real downstream dependency closure rebuild; necessary invalidation is correct behavior. |
| Added, removed and renamed source input | Retain three separate patches and the cache rejection/rebuild evidence; path-set changes conservatively miss the workspace overlay. |
| Cross-run ECS producer/consumer | Retain successful trusted main native producer and compatible reader links, request/restored key identity, payload validation, true unit Fresh/rebuilt and final package identity. |

For every sample preserve immutable source/head IDs, scenario patch, exact command/environment, cache provenance, wall time, unit identities/Fresh/rebuilt, available frontend/codegen time and rebuild closure. For cross-run cache samples also record raw/unique/compressed bytes and restore/save duration; calculate net benefit including transfer overhead only from comparable runs. Cache-hit, successful build, test execution and package verification are distinct facts. A compile-only harness or surviving executable is not test execution evidence. Report regressions and missing measurements explicitly; architecture acceptance and real remote workspace cache consumption remain independent completion items. This manual protocol does not add a default gate, threshold or automatic workflow trigger.

### 11.3 Executable ECS native build and Viewer experiment

[`scripts/ci-ecs-build-metrics.py`](../../../scripts/ci-ecs-build-metrics.py) implements the full three-binary native closure and Viewer row of the protocol above. It is independent of the V2 check schema. The existing [ECS package workflow](../../../.github/workflows/ecs-packages.yml) exposes an opt-in `measure_builds` input and a required explicit `measurement_baseline` when enabled. The normal Web/native assembly still runs and validates the complete deliverable; the separate measurement job does not publish or deploy a release. No automatic PR trigger, timing threshold or cache writer is added.

Resolve baseline and current revisions to immutable commits before any measurement. The workflow fetches only the requested baseline; the harness never fetches or changes the canonical task checkout. It creates separate detached checkouts and empty run-owned external targets, prefetches locked dependencies outside the timing interval, and executes offline builds with Rust 1.96.0, `ecs`, native host, default features, fixed jobs and incremental disabled. Current and baseline never share compiled targets. For this one Viewer scenario, each revision independently runs a full seed build, the identical unchanged build, and the identical build after the fixed Viewer comment patch. This is the per-scenario sequence allowed in section 11.2, not cumulative mutation or reuse of a target from another scenario.

The patch targets the compiled `world_view_source.rs` source at both revisions. Missing source, unexpected source changes, a failed Cargo command, rebuilt no-op units or an unchanged Viewer target cannot yield a successful comparison. Preserve the patch, before/after digests, command/environment/toolchain identities, binary hashes, per-unit Fresh/rebuilt data, wall time, diagnostics and available Cargo timing HTML outside both checkouts. Restore the owned mutation and remove only harness-owned worktrees on exit. A real minimal Rust workspace exercises this path in the harness contract tests; it proves the harness behavior, not production performance.

Report the actual state/projection freshness and all rebuilt downstream units for the Viewer sample. A comparison of different functional revisions is an observation of those revisions, not proof that extraction alone caused the difference. Single sequential samples retain runner/order variance; they do not establish a stable percentage gain. Local warm-target measurements, validated local overlays and hosted cross-run cache consumption remain separate evidence. Package identity and normal tests remain separate from timing success.

## 12. Residual risk and deferred-debt trigger

The warm signal still inherits hosted-runner CPU, filesystem, scheduler, and
Cargo metadata variance. The per-checkout run-owned target directory outside
the checkout is cold before the first check, but the Cargo registry/source
cache may be shared; the metric is therefore about the second check's no-op
path, not a hermetic machine benchmark.

Build scripts, environment changes, or toolchain behavior can invalidate
artifacts even when the source fingerprint is unchanged. That is useful
diagnostic information but does not make the metric a touched-source measure.
The existing three-platform workflow is also expensive, so this slice keeps
warm collection manual and opt-in.

Evaluate the affected scope and record any necessary follow-up in the current PR or an existing task when any of these triggers is observed; create an Issue only when tracking needs it:

- warm results are being used to support an incremental/touched-source claim;
- an owner wants a default warm threshold or required-gate enforcement;
- repeated platform-specific warm failures indicate runner or target
  contamination rather than source behavior;
- a source mutation is needed to represent a meaningful development loop; or
- enough paired samples exist to justify multi-run statistics, fixed runners,
  trend storage, or automatic changed-path dispatch.

Those triggers are technical-debt signals, not permission to broaden this
slice in place.

## 范围、结构边界与风险

本设计范围是 cold/warm/no-op 编译量测及其结果解释；结构、采样身份、缓存条件、rollout 与验证沿用下文各节。主要风险是把不同候选、缓存或 runner 条件的样本混比，或把 no-op 改善当作触源码增量、hosted 性能或发布就绪证明；必须保留原始测量及未证明范围。
