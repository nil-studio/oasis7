# oasis7

> Give an AI civilization a direction in the asteroid belt.

[简体中文](./README.md) · [Website](https://oasis7.tech/en/) · [Local preview](#local-preview) · [Developer docs](./doc/README.md) · [MIT License](./LICENSE)

oasis7 is an **AI Agent civilization simulation game in development**. Players set goals from outside the world. Agents choose actions under resource limits, permissions and world rules, leaving consequences that can be traced.

We explore how agents might build ways to survive, cooperate and organize when energy is limited and capabilities are programmable.

## Project status

The project is a **limited playable technical preview**, intended for players, researchers and developers willing to prepare their environment. Real LLM play requires local builds, model configuration and available services; model calls may consume credits or incur charges.

This is not a closed beta, public launch or live season. A formal announcement is still being prepared. Long-term autonomy, civilization emergence and opening a persistent world remain separate validation goals. GitHub Releases and the website's download area mainly carry development-preview build information. Release packages, recorded source examples and current main must be assessed at their own versions.

The [Chinese root README](./README.md#项目状态) owns the current public status; this page follows that boundary. See [player access and distribution](./doc/product/player-entry-distribution/prd.md) for access conditions. Most detailed project documentation is currently in Chinese.

![Concept art of an industrial asteroid settlement and a distant planet; not an in-game screenshot](./site/assets/images/hero/asteroid-settlement-planet-20261015.webp)

*World concept art, not an in-game screenshot. The production example below comes from a separately recorded controlled run.*

## Your role in the world

- **Set a direction:** provide goals and guidance within the capabilities and permissions available to you.
- **Let the Agent choose actions:** it interprets the environment, reacts to feedback and handles resource or permission blockers.
- **Inspect the consequences:** the runtime validates actions and records results; use those results to adjust the next goal.

Players participate through indirect control. They cannot bypass the runtime to change world state or dictate every Agent action. See [world rules and gameplay](./doc/product/world-rules-core-gameplay/prd.md) for the supported boundaries.

## One recorded production attempt

In a controlled CLI run on 2026-10-13, an independent Agent started from a prepared world with an already claimed Agent:

| Step | What happened |
| --- | --- |
| Goal | Build a smelter with starting inventory and produce the first batch of iron. |
| Blocker | The first factory choice was outside its authorization; the quote was rejected without a build action. |
| Adjustment | The Agent read its actual permissions, selected the permitted smelter, then built and supplied it. |
| Result | One recipe completed; world records confirmed **3 iron ingots and 1 slag**. |

This run used source `52c136a7`. It demonstrates construction, feeding, first production and request recovery in that prepared state. It does not validate GUI play, installation from scratch, public deployment or sustained multi-factory production. Read the [original report](./doc/testing/evidence/native-task-starter-chain-2026-10-13/bounded-retry/README.md).

## Start here

| Your goal | Entry point |
| --- | --- |
| Understand the game | [World rules and gameplay](./doc/product/world-rules-core-gameplay/prd.md) · [Project overview](./doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md) |
| Try the local preview | [Local preview](#local-preview) · [Viewer manual](./doc/world-simulator/viewer/viewer-manual.manual.md#本地真实-llm-游戏测试) |
| Connect your own Agent | [Official gameplay Skill](./skills/oasis7-play/SKILL.md) · [Game client and API](./crates/oasis7_game_client/README.md) |
| Explore or contribute code | [Architecture](#architecture) · [Development workflow](./doc/engineering/workflow/source-of-truth.md) · [Testing manual](./testing-manual.md) |
| Follow progress or report a problem | [Development log](https://oasis7.tech/devlog/) · [Issues](https://github.com/nil-studio/oasis7/issues) · [Pull requests](https://github.com/nil-studio/oasis7/pulls) |

`viewer` and `pure_api` are distinct access modes, each requiring its own capability and playability evidence. External Agents must read the official Skill's prerequisites; an API or observer connection does not establish full gameplay availability.

## Local preview

Prepare the [repository's Rust toolchain](./rust-toolchain.toml), Node.js/npm, Python 3 and curl. The manual's local real-LLM path currently uses LetAI and requires your own valid configuration. Read the [first-run prerequisites](./doc/world-simulator/viewer/viewer-manual.manual.md#首次运行准备).

```bash
git clone https://github.com/nil-studio/oasis7.git
cd oasis7

# Replace the path with your own LetAI configuration. This checks local prerequisites only.
./scripts/run-local-letai-game-test.sh \
  --config /absolute/path/to/letai.env \
  --no-default-proxy \
  --preflight-only \
  -- \
  --viewer-port 48420 \
  --web-bind 127.0.0.1:48421 \
  --live-bind 127.0.0.1:48422
```

Preflight does not validate model credentials, credits or gameplay. `--no-default-proxy` disables the script's default local proxy settings while preserving any existing proxy environment.

Follow the [first foreground launch](./doc/world-simulator/viewer/viewer-manual.manual.md#首次前台启动) instructions to build the provider bridge and start the local stack. Do not use `--local-world-playtest` for the first build: that daily preset reuses existing binaries, detaches by default and skips provider smoke.

- **Open the page:** wait for `STACK_READY=1` in this run's `launcher/session.meta`, then open its `GAME_URL`.
- **Check the right result:** page and listener readiness do not prove a real model decision or a completed first product. The manual separates stack, world and gameplay checks.
- **Stop the first foreground run:** press `Ctrl+C` in its terminal. The detached daily preset has a different lifecycle.
- **Expect model calls:** first-launch provider smoke requests the real model. `--no-auto-play` controls automatic play, not these startup requests.

## What makes the world interesting

- **Resources have a cost.** Electricity, trade and resource validation constrain actions. [Trading implementation](./crates/oasis7_simulator/src/kernel/actions_resolution.rs)
- **The world has a history.** Events, snapshots and memory provide persistence, recovery and inspection. Memory recovery does not establish long-term learning. [Memory implementation](./crates/oasis7/src/simulator/memory.rs) · [Persistence test source](./crates/oasis7/src/simulator/tests/persist.rs)
- **Capabilities are programmable.** Rust/WASM modules provide extensions governed by resources, permissions and lifecycle rules. [Module lifecycle test source](./crates/oasis7_runtime/src/world/governed_module_lifecycle_transaction_regressions.rs)

These links identify implementations and tests, not release acceptance. Development policies may allow runtime source compilation; hardened release policies require externally built artifacts and verification receipts. WASM is an extension path, not a mandatory layer for every native action. See [runtime design](./doc/world-runtime/README.md).

## Design principles

World-first rules validate every action. Emergence-first explores cooperation within those rules. Persistence preserves and restores history. Auditable events support inspection. Extensible modules add capabilities.

A client being offline is different from a stopped world service. Continued progression requires the relevant nodes, runtime services and decision providers to remain available.

## Architecture

Player intent becomes an Agent decision and action request. The authoritative world runtime validates and commits it; clients read the corresponding results and projections.

| Responsibility | Main entry points |
| --- | --- |
| Agent interfaces, decisions and models | [`oasis7_agent_api`](./crates/oasis7_agent_api/) · [`oasis7_agent_harness`](./crates/oasis7_agent_harness/README.md) · [`oasis7_model_provider`](./crates/oasis7_model_provider/README.md) |
| World simulation and execution | [`oasis7_simulator`](./crates/oasis7_simulator/) · [`oasis7_runtime`](./crates/oasis7_runtime/) |
| State, projections and services | [`oasis7_world_state`](./crates/oasis7_world_state/) · [`oasis7_world_projection`](./crates/oasis7_world_projection/) · [`oasis7_world_service`](./crates/oasis7_world_service/) |
| Protocols, networking, consensus and storage | [`oasis7_proto`](./crates/oasis7_proto/) · [`oasis7_node`](./crates/oasis7_node/) · [`oasis7_net`](./crates/oasis7_net/) · [`oasis7_consensus`](./crates/oasis7_consensus/) · [`oasis7_distfs`](./crates/oasis7_distfs/) |
| Clients and Viewer | [`oasis7_game_client`](./crates/oasis7_game_client/README.md) · [`oasis7_client_api`](./crates/oasis7_client_api/) · [`oasis7_viewer_runtime`](./crates/oasis7_viewer_runtime/) · [Viewer frontend](./crates/oasis7_viewer/) |
| WASM extensions | [Build](./crates/oasis7_wasm_build/) · [SDK](./crates/oasis7_wasm_sdk/) · [Executor](./crates/oasis7_wasm_executor/) · [Router](./crates/oasis7_wasm_router/) |

The [Cargo workspace](./Cargo.toml) lists Rust members. `crates/oasis7_viewer` is a frontend directory. The Web Viewer connects through a gateway for observation and indirect control; it is not a full distributed node.

## Current direction and deeper reading

The current goal is a controlled player journey: produce a real first output through a bounded Agent, understand and recover from production blockers, retain the result, and return to continue the original goal. First production, sustained production, delivery and return visits are validated separately.

Infrastructure first validates controlled single-authority commits, replicas and recovery of the same world, then extends BFT under existing identity, history and authorization contracts. Adding a node does not grant voting or write authority; changing a network tier does not give test rewards production value.

- [Core goals and dependencies](./doc/core/prd.md#当前阶段交付目标与全局-p0)
- [First session and continuation](./doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md) · [Industrial completion](./doc/game/gameplay/gameplay-industrial-starter-completion-contract.prd.md)
- [Infrastructure contract](./doc/product/world-infrastructure/prd.md)
- [Four product modules](./doc/product/README.md) · [Gameplay design](./doc/game/gameplay/gameplay-top-level-design.prd.md) · [Documentation index](./doc/README.md)

## Contributing

Playtest feedback, bug fixes, documentation improvements and code contributions are welcome.

- Report problems in [Issues](https://github.com/nil-studio/oasis7/issues) with the version, platform, reproduction steps, expected/actual behavior and redacted logs.
- Read the [development workflow](./doc/engineering/workflow/source-of-truth.md), make changes on an independent branch and use the [testing manual](./testing-manual.md) to select validation.
- Use the [documentation index](./doc/README.md) to find the relevant requirements and design.

## License

[MIT License](./LICENSE).
