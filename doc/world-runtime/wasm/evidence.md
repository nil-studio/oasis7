# WASM 平台稳定证据与验证入口

本页索引可重复执行的 WASM 平台验证面和其证据边界。它不保存任务状态、待办、
候选发布结论、CI run 实时状态或 blocker；这些可变事实由 Git、PR、实际 CI
和评审记录承接，Issue / Project 按需使用。

## ABI 与执行器

| 稳定契约 | 实现锚点 | 最低验证 |
| --- | --- | --- |
| 执行器纯 Rust helper、limits/count、FixedSandbox 与 fallback | `crates/oasis7_wasm_executor`（`default=[]`） | `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_executor`；不选中真实 WASM 调用测试 |
| 真实 WASM 成功调用、CBOR 输出及坏 CBOR → `InvalidOutput` | `crates/oasis7_wasm_executor/src/tests.rs` 的 `cfg(feature="wasmtime")` 测试 | `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_executor --features wasmtime` |
| `wasm-1` ABI 编解码与 optional/default 兼容 | `crates/oasis7_wasm_abi`、[wasm-interface.md](wasm-interface.md) | `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_abi`；逐项核对现有兼容回归，执行器 feature 开启不自动证明全部 ABI 合同 |
| fuel fallback、epoch interruption、memory growth、输出限制和权限失败 | `crates/oasis7_wasm_executor/src/lib.rs` | 执行器的 out-of-fuel、interrupt、memory-growth 与 invalid-output 回归 |
| SHA-256 artifact 校验与 compiled-cache compatibility domain | `crates/oasis7_wasm_executor`、runtime module persistence | serialized cache round-trip、corruption-as-miss 与 hash-mismatch rejection 回归 |

资源计费的世界规则、实际扣减和审计事件由 runtime authority 定义；这里仅证明
executor 对 manifest limits/capabilities 的安全执行边界。

## 构建、identity 与发布证据

| 稳定契约 | 实现锚点 | 最低验证 |
| --- | --- | --- |
| publishable artifact 只由 digest-pinned Docker canonical builder 生成 | `docker/wasm-builder/Dockerfile`、`scripts/build-wasm-module.sh`、`tools/wasm_build_suite` | Docker wrapper 成功或明确失败；不得退回 host-native publish path |
| receipt 绑定 source closure、build manifest、builder digest、canonical platform、canonicalizer 和 wasm hash | `tools/wasm_build_suite`、`crates/oasis7_distfs/src/bin/sync_builtin_wasm_identity.rs` | builtin sync/check 与 receipt/identity 一致性回归 |
| release manifest 写单一 `linux-x86_64=<sha256>` canonical token；legacy 多 token 仅读兼容 | sync scripts、identity writer、release-manifest consumer | `scripts/sync-m1-builtin-wasm-artifacts.sh --check`、对应 m4/m5 check |
| production `ReleaseSecurityPolicy` 默认拒绝动作级 `CompileModuleArtifactFromSource`；不是所有 builtin 物化路径的总开关 | `ReleaseSecurityPolicy`、`module_source_compiler`、chain runtime status | module release acceptance 的 hardened-policy / receipt-mismatch 阻断用例，只证明受该 policy 管理的路径 |
| builtin materializer 的 binary 获取、SHA-256 校验、缓存与 source-compile fallback 另有边界 | `crates/oasis7_wasm_materializer/src/lib.rs` 与 runtime 错误适配 `crates/oasis7_runtime/src/builtin_wasm_materializer.rs`、WASM 主设计 §5.0 | 物化路径自身的获取失败、hash 不匹配、缓存与 fallback 回归；不能用动作 gate 的结果代签 |

当前路径划分以[主设计 §5.0](wasm-deterministic-build-pipeline.design.md#50-builtin-工件-materialization)为准：builtin materializer 不接收 identity、receipt 或 release policy，本地候选不可用后仍有 fetch 与源码回退路径；外围 artifact selection / release-gated pipeline 承担对应身份及发布校验。主设计 §2 的 2026-03-18 现状表是有日期的记录，其“关闭 builtin fallback”摘要不能扩为当前所有物化路径的结论。binary-only 是发布目标，文档修订不改变运行策略，也不证明任一线上配置已执行或已关闭 fallback。

跨宿主 determinism 只能由同一候选的 `linux-x86_64` 与 Docker-capable
`darwin-arm64` summaries 及其 receipt evidence 共同证明。Linux-only stable gate
只证明稳定基线，不能代替 cross-host closure；node-side proof payload 才是
attestation 提交的生产证据载体。

### Historical nightly build-std provenance

早期 builtin WASM 曾使用 pinned `nightly-2025-12-11`、`rust-src`、
`wasm32-unknown-unknown`、`OASIS7_WASM_BUILD_STD=1`、`std,panic_abort`
以及路径/custom-section 归一化来抑制宿主预编译 `std` 引起的 hash 漂移。
该方案当时通过 m1/m4 artifact 同步检查与 required-tier 回归，但不定义当前
发布级 builder、runtime ABI、DistFS 协议、hash 算法或 manifest 格式。
nightly 升级/不可用及首次构建成本属于历史风险；当前 publishable policy
已由 digest-pinned Docker canonical builder、receipt 和 cross-host evidence
完全取代。

## SDK 与观测

| 稳定契约 | 实现锚点 | 最低验证 |
| --- | --- | --- |
| SDK 默认 `no_std`，生命周期导出与 Canonical-CBOR wire 单一来源 | `crates/oasis7_wasm_sdk`、`crates/oasis7_builtin_wasm_modules/*` | `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_sdk`；wasm32 target 可用时的 check |
| codec failure 显式可观测，builtin 不复制 parallel wire schema | `oasis7_wasm_sdk::wire`、builtin call sites | wire round-trip / corrupt-input tests 与 builtin duplicate-definition scan |
| build/executor/router metrics 为 bounded local snapshot，不进入 consensus/world state/replay | `tools/wasm_build_suite`、`oasis7_wasm_executor`、`oasis7_wasm_router`、chain status payload | build-suite、executor/router、chain-runtime observability tests 与 monitor shell tests |
| module-local observe 复用 shared metrics，默认不暴露无界 module payload | `tools/wasm_module_observe`、`scripts/oasis7-wasm-module-observe.sh` | observe runner test、代表 module spec 与 wrapper shell check |

wasm32 target 缺失必须被明确记录为 skipped 或 failed，不能作为通过证据。计时
instrumentation 的降级不得阻断模块执行，也不能代签功能正确性或发布就绪。

## 兼容性与证据边界

- 新 ABI/schema/wire 字段必须 optional 或 serde-default compatible，并覆盖旧
  module 解码；删除字段或改变语义是破坏性 ABI 迁移，需单独版本化和迁移计划。
- identity manifest、artifact hash、builder receipt 和 release proof 必须彼此
  对齐；CI artifact/report 本身不等于生产 release authorization。
- 本页的命令是平台级回归入口，不构成 `test_tier_required`、`test_tier_full`、
  集成、长期运行或发布就绪结论。具体候选证据仍需绑定实际提交/构建、
  适用场景及对应 CI/测试记录；未执行或不适用必须分别说明。
