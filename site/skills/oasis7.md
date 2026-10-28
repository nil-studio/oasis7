---
name: oasis7
description: Public raw Markdown for the oasis7 Local Provider real-play workflow. Use it to choose the Rust-direct LetAI preset, a legacy provider diagnostic, or a release-bundle launcher path, and to run scoped parity smoke.
---

# Oasis7

## Overview

`oasis7` is the public, repo-backed workflow for Local Provider gameplay. This
copy is self-contained and does not require the deleted private skill wrapper.
Choose one entry path before installing components or probing services; the
Rust-direct default does not use the legacy Gateway or runtime-agent workspace.

These are operating instructions, not evidence that a fresh machine, a release
bundle, or a real provider has passed acceptance. Match commands to the source
revision or installed bundle being tested and record the actual result.

## When To Use

Use this skill to configure real Local Provider gameplay, start or diagnose
`oasis7_provider_local_bridge`, launch `oasis7_game_launcher`, or run `P0-001`
parity smoke. Do not use it for unrelated provider work, changes to read-only
`third_party/` code, or Viewer-only styling.

## Execution Lanes

Local Provider real-play can run without a Viewer:

- `headless_agent`: smoke, CI, servers, low-spec machines, and no-UI regression.
- `player_parity`: player-feel runs for producers and QA.

The Viewer is optional and is not the authority execution path. A headless pass
does not prove graphical interaction or parity between the two lanes.

## Core Workflow

### 1. Choose the entry path and its prerequisites

| Entry | Use it for | Required prerequisites | Not a prerequisite |
| --- | --- | --- | --- |
| Rust-direct LetAI preset | Normal local real-play from a source checkout / compatible existing build | Repo scripts, usable build or build toolchain, authorized LetAI configuration, available local ports | Provider CLI, `127.0.0.1:18789`, `setup-provider-oasis7-runtime.sh` |
| Legacy CLI diagnostics | Explicit compatibility investigation | The selected CLI/adapter and its own configuration; see section 4 | A Gateway or installed runtime-agent workspace unless that selected CLI actually consumes them |
| Release-bundle launcher | Inspect an installed bundle's launcher wiring | A compatible bundle containing `run-game.sh`, plus a separately configured and healthy provider bridge | Repo build tooling merely to invoke `run-game.sh`; a bundle does not itself prove that the bridge/preset is installed |

Use `cargo` only when the selected repo-backed path builds/runs from source.
Canonical Cargo commands use `env -u RUSTC_WRAPPER cargo ...`; follow the repo's
development wrapper and target preparation rules for local compilation.
The normal local preset uses `48420/48421/48422`; manual bridge examples below
use `127.0.0.1:5841`. Use the selected entry's actual ready output when probing.

### 2. Start Local LetAI Real-Play

For normal local real LetAI gameplay, start with the one-command preset:

```bash
./scripts/run-local-letai-game-test.sh --local-world-playtest
```

The preset normalizes platform credentials into a temporary project token
config, forwards platform user/key and auto-topup configuration, starts the
Rust-direct LetAI provider bridge, and launches the runtime/Viewer/local
standalone chain stack. It does not connect to formal/public testnet. This
path requires an available checkout/build and provider configuration;
these instructions do not certify unattended cold-start on a clean machine.

Pass extra launcher flags after `--` only for diagnostics:

```bash
./scripts/run-local-letai-game-test.sh --local-world-playtest -- --viewer-port 4174 --json-ready
./scripts/run-local-letai-game-test.sh --help-all
```

Direct bridge/launcher binaries are not substitutes for this preset's token,
platform identity, auto-topup, and local-world setup. Manual runs must preserve
the applicable configuration instead of assuming equivalent behavior.

### 3. Credentials, billing, and manual Rust-direct bridge diagnostics

Prefer an explicit `OASIS7_LETAI_CONFIG_PATH` for the authorized local config.
The wrapper also has local fallback locations and token-config inputs; inspect
the selected revision's wrapper for precedence rather than copying another
operator's private filesystem paths.

A project `token_key` / `api_key` authorizes inference. A platform management
`Key` is not that project token; a documentation `Doc` URL is not a model
endpoint. When only platform credentials are available, use the preset or
`ensure-letai-local-token-config.sh` to normalize them first. Generated token
files such as `letai-local-token.env` are secrets: never attach them to public
PRs, evidence, screenshots, or logs.

Real provider calls can incur charges. The local Rust bridge defaults to an
`--auto-topup-usd` amount of `0.1`; on `insufficient_user_quota` it can top up
once per quota-failed chat request, followed by bounded delayed retries. This
requires both `OASIS7_REMOTE_LLM_PLATFORM_KEY` and
`OASIS7_REMOTE_LLM_PLATFORM_USER_ID`; otherwise the quota error remains visible.
Confirm the authorized budget and resolved auto-topup setting before real calls.

For a deliberate low-level bridge diagnostic:

```bash
./scripts/run-local-letai-provider-bridge.sh
# In another terminal; use the actual bind address if overridden:
./scripts/provider-remote-https/provider-bridge-contract-smoke.sh --base-url http://127.0.0.1:5841
./scripts/run-launcher-stack.sh --agent-provider-lane local
```

The default backend is `rust-direct-letai`. It calls LetAI's chat-completions
API, not the builtin OpenAI Responses preflight. It does not require a provider
CLI, local Gateway, or runtime-agent installation. The bridge runs in the
foreground; the game connects to the loopback bridge, not directly to a Gateway.

Inspect the manual bridge with:

```bash
curl -sS http://127.0.0.1:5841/v1/provider/info | jq .
curl -sS http://127.0.0.1:5841/v1/provider/health | jq .
```

For builtin-LLM diagnostics only, the direct config wrapper requires a config
that already contains an inference-compatible project token:

```bash
./scripts/with-letai-llm-config.sh -- ./scripts/check-active-llm-provider.sh --pretty
./scripts/with-letai-llm-config.sh -- ./scripts/run-launcher-stack.sh --agent-decision-source builtin_llm
```

Do not point that direct wrapper at a platform-only `Key` file. When bypassing
the preset, preserve its project token and the applicable
`OASIS7_REMOTE_LLM_PLATFORM_KEY`, `OASIS7_REMOTE_LLM_PLATFORM_USER_ID`, and
`OASIS7_REMOTE_LLM_AUTO_TOPUP_USD` values without exposing them.

The preset leaves `OASIS7_RUNTIME_AGENT_CHAT_ECHO` disabled. `--chat-echo` is a
receipt/debug option only: a `[local-mock-receipt]` reply is not a real Agent reply.

### 4. Legacy provider diagnostics: select the actual adapter

**LetAI Python adapter compatibility** uses this explicit branch:

```bash
./scripts/run-local-letai-provider-bridge.sh --provider-backend legacy-cli
```

That script selects `scripts/provider-remote-https/letai_provider_cli.py` and
LetAI configuration. It does not turn `127.0.0.1:18789` or an OpenClaw workspace
into prerequisites. `./scripts/check-letai-chat-completions.sh` is available for
this adapter/token-only comparison, not the normal real-play preflight.

**Gateway-backed provider CLI compatibility** is a different diagnostic path.
Only when the selected CLI actually uses the local Gateway and agent workspace,
check/install those components:

```bash
provider_cli_bin="${OASIS7_PROVIDER_CLI_BIN:-openclaw}"
"$provider_cli_bin" --version
curl -sS http://127.0.0.1:18789/health
bash scripts/setup-provider-oasis7-runtime.sh
env -u RUSTC_WRAPPER cargo run -p oasis7_provider_local_bridge --bin oasis7_provider_local_bridge -- --provider-backend legacy-cli --provider-cli-bin "$provider_cli_bin" --provider-agent oasis7_provider_agent
```

The repo-owned lightweight agent defaults are `oasis7_provider_agent`, workspace
`tools/provider/oasis7_provider_workspace`, and model `custom-right-codes/gpt-5.4`.
Prefer that isolated workspace to the user's default workspace for this legacy
route. Match CLI selection to the actual bridge configuration. Gateway health
only diagnoses this route; it is not a general Rust-direct readiness gate.

For no-quota/no-billing plumbing tests, use the deterministic mock instead:

```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_provider_local_bridge --bin oasis7_provider_local_bridge -- --mode mock
```

The mock exposes the loopback HTTP contract without calling a provider CLI or
LLM. It cannot supply real LLM, gameplay, or player-parity evidence.

### 5. Release bundle and advanced manual launcher path

For bundle-first operators, obtain the installer asset for the intended tag and
platform from `https://github.com/eng-cc/oasis7/releases`, then install/extract it.
Verify that the chosen bundle contains `run-game.sh`. Do not treat `latest`, an
asset name, or these instructions as proof of a usable build for every platform.
The bundle is an alternative launcher entry, not a prerequisite of section 2.

Start and verify the selected provider bridge separately before these manual
launcher diagnostics. Preserve the preset's applicable credential and billing
configuration; repo bridge/smoke scripts are not implied to be in the bundle.

Repo source path:

```bash
env -u RUSTC_WRAPPER cargo run -p oasis7_launcher_backend --bin oasis7_game_launcher -- \
  --scenario llm_bootstrap \
  --with-llm \
  --agent-provider-mode provider_loopback_http \
  --agent-provider-url http://127.0.0.1:5841 \
  --agent-provider-connect-timeout-ms 15000 \
  --agent-provider-profile oasis7_p0_low_freq_npc \
  --agent-execution-lane player_parity
```

Release bundle path:

```bash
./run-game.sh \
  --scenario llm_bootstrap \
  --with-llm \
  --agent-provider-mode provider_loopback_http \
  --agent-provider-url http://127.0.0.1:5841 \
  --agent-provider-connect-timeout-ms 15000 \
  --agent-provider-profile oasis7_p0_low_freq_npc \
  --agent-execution-lane player_parity
```

Use the bridge's actual URL if overridden. The manual examples use
`provider_loopback_http`, a `15000` ms connect timeout, and profile
`oasis7_p0_low_freq_npc`; confirm support in the selected source/bundle.

### 6. Run scoped parity smoke

The no-UI regression lane is:

```bash
bash scripts/provider-parity-p0.sh \
  --provider-only \
  --samples 1 \
  --ticks 4 \
  --timeout-ms 15000 \
  --agent-provider-url http://127.0.0.1:5841 \
  --agent-provider-connect-timeout-ms 15000 \
  --agent-provider-profile oasis7_p0_low_freq_npc \
  --execution-mode headless_agent
```

The `P0-001` sample target is `status=passed`, `goal_completed=true`, and
`invalid_action_count=0`. Record the tested source/build/Skill, selected provider
backend, execution lane, environment, actual artifacts, and uncovered scope.
A passing sample is not full builtin/Local Provider parity, a graphical player
run, or a release go decision. Missing prerequisites remain blocked/unknown;
legacy results do not certify a new candidate. Gameplay self-test requirements
remain in `doc/testing/manual/agent-runtime-self-playtest.manual.md`.

### 7. Chain key safety

Outside the local preset, manual Local Provider launchers use their configured
chain startup path unless `--chain-disable` is explicitly selected. Confirm the
actual profile before starting; prefer disposable local trial state and do not
connect a diagnostic run to a formal/public network unintentionally.

Node private keys must never enter Git, issues, evidence, screenshots, shared
shell history, or CI logs. Public keys are environment identity material, not
secrets; label them by environment. Preserve recovery and cleanup boundaries for
the selected chain storage profile.

## Debug Checklist

First identify the route, source/build, configured backend, and actual bind URL.
Then inspect the applicable branch:

1. **Rust-direct preset:** inspect preset readiness/errors, resolved sanitized
   configuration, bridge `/v1/provider/info` and `/v1/provider/health`, then
   inference-token, upstream quota, and billing errors. Do not install or probe
   the legacy Gateway as a repair for this route.
2. **Legacy CLI:** inspect the selected adapter and its configuration. Probe
   `127.0.0.1:18789/health` and check the runtime-agent installation only for a
   Gateway/workspace-backed CLI, not the LetAI Python adapter.
3. **Manual bundle/launcher:** verify `run-game.sh` / executable identity,
   provider mode, URL, profile, timeout, and the separately started bridge.
4. **Parity:** inspect `output/provider_parity/*`, exit status, lane, and actual
   world/receipt results; retain failed attempts and distinguish mock output.

No health probe alone proves player authority, `agent_chat`, `prompt_control`,
end-to-end playability, or a fresh acceptance window. Use the current dual-mode
contract and matching evidence to decide what the tested candidate proves.
Changing the onboarding route does not establish `agent_chat` or `prompt_control`
as end-to-end player-authority paths or lift their existing contract limitations.

## Repo Anchors

- Normal local preset: `scripts/run-local-letai-game-test.sh`.
- Rust-direct / legacy adapter selection: `scripts/run-local-letai-provider-bridge.sh`.
- Bridge: `crates/oasis7_provider_local_bridge/src/bin/oasis7_provider_local_bridge.rs`.
- Launcher: `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs`.
- Legacy workspace installer/policy: `scripts/setup-provider-oasis7-runtime.sh` and `tools/provider/oasis7_provider_workspace/AGENTS.md`.
- Parity harness: `scripts/provider-parity-p0.sh`.
- Dual-mode contract/verdict: `doc/world-simulator/llm/provider-agent-dual-mode.prd.md` and `doc/world-simulator/llm/provider-agent-dual-mode-contract.md`.
- Historical parity provenance: `doc/testing/provider-agent-parity-p0-t4-closure-2026-03-17.md`; not a current candidate's verdict.
- Delivery facts: the applicable PR, actual CI/review and test records; Issue/Project tracking is optional under the repository workflow.

## Output Expectations

Give exact commands for the chosen route. State which process supplies each
actual port; mention `18789` only when the selected legacy route consumes it.
Distinguish source builds from bundles, workspace/profile from documentation,
mock from real LLM evidence, and a bounded smoke from current gameplay/release
acceptance. Never expose credentials or turn unknown results into PASS.
