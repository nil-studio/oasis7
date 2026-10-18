# oasis7: Builtin Wasm Docker Canonical Gate

- 对应设计文档: `doc/testing/ci/ci-builtin-wasm-docker-canonical-gate.design.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

审计轮次: 2

## 1. Executive Summary
- Problem Statement: builtin wasm 发布链路已经切到 Docker canonical build，但 testing 侧专题仍保留 keyed multi-runner、旧 required check 与旧 workflow 叙事，导致现行 Linux-only canonical gate 与历史 host-native 多 runner 对账口径混杂。
- Proposed Solution: 将专题目标收敛为 Docker canonical gate：发布清单只保留 `linux-x86_64` 单 canonical token，GitHub-hosted gate 统一走 `.github/workflows/wasm-determinism-gate.yml`，required checks 与 release evidence 围绕 canonical Docker 输出组织；外部 Docker-capable macOS summary 只补充跨宿主产物证据，不属于 `test_tier_full`。
- Success Criteria:
  - SC-1: `m1/m4/m5` hash manifest 仅包含单 canonical token：`linux-x86_64=<sha256>`。
  - SC-2: `sync-m*-builtin-wasm-artifacts.sh` 默认按 Docker canonical 平台校验，并拒绝 legacy / mixed 输入。
  - SC-3: `.github/workflows/wasm-determinism-gate.yml` 是现行 builtin wasm hash/evidence 独立 gate，并在 PR/push 上先按 changed paths 规划命中的 `m1/m4/m5` module set。planner 成功且 scope 为 partial/all 时，collect/verify 仅展开选中模块；`scope=skip` 时仅执行 planner，collect/verify 在 job 层跳过，不产生该次模块产物校验通过结论。
  - SC-4: `Wasm Determinism Gate` 的 verify 矩阵按选中模块展开，以下为对应模块的 job 名称，不表示每次 PR 都执行全部三项：
    - `Wasm Determinism Gate / verify-wasm-determinism (m1)`
    - `Wasm Determinism Gate / verify-wasm-determinism (m4)`
    - `Wasm Determinism Gate / verify-wasm-determinism (m5)`
    当前保护以 GitHub 原生配置的名称与 App 来源绑定为准；仓库文档不能证明保护已启用或与选择器一致。planner、被选中的 collect/verify 失败、取消、缺失或意外跳过都不能视为验证通过；未选中模块的 job 跳过只说明本次不执行。策略变更按开发流程规范授权、操作并回读，不自动创建或解除保护。
  - SC-5: `test_tier_required` / `test_tier_full` 是 `scripts/ci-tests.sh` 的测试层级；它们不执行也不替代独立的 `wasm-determinism-gate` hash / receipt-evidence 检查。额外导入的 macOS summary 只补跨宿主产物证据，不代表 `test_tier_full` 或 `./scripts/ci-tests.sh full` 的结果。
  - SC-6: `source_hash` 仅基于可追踪源码与模块级 lockfile 输入，不再依赖 workspace 根 `Cargo.lock`。
  - SC-7: 本地默认只读校验，manifest/identity 写入路径限定非 CI 的显式授权流程。

## 2. User Experience & Functionality
- User Personas:
  - CI 维护者：需要在 PR 阶段自动拦截 canonical Docker 输出漂移。
  - 发布工程维护者：需要 manifest / identity / release evidence 的写入来源可审计且不可被 CI 或开发机误覆盖。
  - 模块开发者：需要在本地快速执行 `--check` 获取 canonical 结果。
- User Scenarios & Frequency:
  - PR 门禁：涉及 wasm 构建链路改动时，每次 PR 触发 canonical summary / release evidence gate；经 planner 判为无关的改动只执行范围规划，不实际收集 summaries；不能仅按扩展名认定文档必然不影响构建输入。
  - 发布更新：仅在发布流程由受控节点执行 manifest / identity 写入。
  - 本地调试：开发者高频执行 `sync-m*/--check` 进行只读校验。
- User Stories:
  - PRD-TESTING-CI-WASMHARD-001: As a CI 维护者, I want builtin wasm manifests and sync flow bound to a single canonical Docker output, so that legacy drift paths are removed.
  - PRD-TESTING-CI-WASMHARD-002: As a 发布工程维护者, I want identity and release evidence bound to stable tracked inputs plus build receipts, so that trust no longer depends on host-native builds.
  - PRD-TESTING-CI-WASMHARD-003: As a 仓库管理员, I want required checks and evidence automation aligned to `wasm-determinism-gate`, so that policy drift is prevented.
- Critical User Flows:
  1. Flow-WASMHARD-001: `PR 触发 -> planner 基于 changed paths 推导命中的 module set -> wasm-determinism-gate 仅对命中的 m1/m4/m5 收集 canonical summary -> 汇总 release evidence -> required check 放行/阻断`
  2. Flow-WASMHARD-002: `发布触发 -> 受控节点执行 sync 写入 -> single canonical manifest + identity 更新 -> 合规提交`
  3. Flow-WASMHARD-003: `本地执行 sync -> 默认仅 --check -> 若请求写入且未显式授权则拒绝并提示策略`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| canonical hash manifest | `module_id`, `linux-x86_64=<sha256>` | `sync --check` 校验 canonical token 合法性与平台完备性 | `raw -> validated -> pass/fail` | 发布清单仅允许单 canonical token | 所有人可读，写入受策略约束 |
| sync strict 模式 | `legacy_tokens`, `keyed_tokens`, `current_platform` | 检测 legacy / mixed 时直接失败 | `checking -> rejected/accepted` | 仅 canonical token 允许进入后续流程 | 本地默认只读，写入需显式授权 |
| identity / receipt 输入收敛 | 源码白名单、模块 lockfile、build receipt、hash manifest token | 计算 `source_hash`、`identity_hash` 与 release evidence | `collecting -> hashing -> emitted` | 输入路径排序稳定，忽略未跟踪文件 | 由构建脚本统一执行 |
| canonical summary / evidence 对账 | `runner`, `canonical_platform`, `module_hashes`, `receipt_evidence`, `module_set`, `scope` | planner 先产出 scope，runner 仅对命中的 module set 导出摘要；汇总脚本执行差异比较并生成 evidence | `planned -> generated -> uploaded -> reconciled` | 对选中 `module_id` 完整对齐 canonical 输出；无关范围仅 planner 成功，不形成模块校验结果 | CI workflow 自动执行 |
- Acceptance Criteria:
  - AC-1: `m1/m4/m5_builtin_modules.sha256` 全量迁移到单 canonical token，且不含 legacy / 双平台 token。
  - AC-2: sync 脚本在 check / sync 两种模式均拒绝 legacy 或 mixed 输入。
  - AC-3: 本地执行不带显式写入授权时，sync 脚本拒绝写入并提供修复提示。
  - AC-4: `.github/workflows/wasm-determinism-gate.yml` 与摘要 / 证据脚本按 changed paths 选择真实 module set；无关范围只运行 planner，选中范围实际完成收集与校验，范围规划或必要执行失败不能作为通过证据。
  - AC-6: identity `source_hash` 移除 workspace 根 `Cargo.lock` 依赖，并仅使用可追踪稳定输入。
- Non-Goals:
  - 不变更 runtime 对 builtin wasm manifest 的消费协议。
  - 不恢复 `darwin-arm64` 作为发布级 canonical token。
  - 不调整业务模块的 wasm 功能逻辑。
  - 不把 `scripts/ci-tests.sh required/full` 重新定义为 builtin m1/m4/m5 manifest/hash gate；这些校验只由独立 `wasm-determinism-gate` 与发布级 WASM authority 拥有。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题为构建与 CI 治理）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 通过“single canonical token + sync 严格策略 + identity 输入白名单 + canonical summary/release evidence + required check 治理”形成闭环，消除 builtin wasm 发布链路的宿主漂移来源。
- Integration Points:
  - `scripts/plan-wasm-determinism-scope.sh`
  - `scripts/sync-m1-builtin-wasm-artifacts.sh`
  - `scripts/sync-m4-builtin-wasm-artifacts.sh`
  - `scripts/sync-m5-builtin-wasm-artifacts.sh`
  - `scripts/ci-m1-wasm-summary.sh`
  - `scripts/ci-verify-m1-wasm-summaries.py`
  - `scripts/wasm-release-evidence-report.sh`
  - `crates/oasis7_distfs/src/bin/sync_builtin_wasm_identity.rs`
  - `.github/workflows/wasm-determinism-gate.yml`
  - `crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.sha256`
  - `crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.sha256`
- Edge Cases & Error Handling:
  - git diff base 不可解析：planner 必须回退为 `m1,m4,m5` 全量运行，不能静默漏跑。
  - GitHub-hosted macOS 无 Docker daemon：独立 gate 只跑 Linux canonical runner；可导入真实 Docker-capable `darwin-arm64` summary 补充跨宿主产物证据，该证据不属于 `test_tier_full` 测试层级。
  - 当前平台不在 canonical 列表：`--check` 直接失败并提示 `OASIS7_WASM_CANONICAL_PLATFORMS`。
  - manifest 含重复平台 token：严格失败并报告 `module_id + platform`。
  - runner 缺摘要或摘要重复：汇总脚本失败并列出缺失 / 重复 runner。
  - 目标安装边界：命中 WASM scope 的 Rust job 在当前默认 toolchain 下安装 `wasm32-unknown-unknown`；full/determinism job在 pinned WASM toolchain 下安装同一 target。该事实不表示所有 CI job 或所有 toolchain 都预装 target。
- Non-Functional Requirements:
  - NFR-WASMHARD-1: canonical manifest 与 identity 计算在同一 commit、同一 builder image digest 下 100% 可复现。
  - NFR-WASMHARD-2: canonical summary / evidence 对账失败信息须包含 `runner/module_id/hash`，单次运行内可定位。
  - NFR-WASMHARD-3: 新增治理链路不改变 `scripts/ci-tests.sh required/full` 的职责边界。
  - NFR-WASMHARD-4: 本地默认路径无写权限时不会修改任何 tracked manifest 文件。
- NFR-WASMHARD-5: planner 确认不涉及 builtin wasm 输入的 PR 不执行 summary collect；选中模块使用既有 job 名称，未选中模块不为生成空成功结果而分配 runner。文档、共享配置或脚本是否相关按实际消费者判断。
- NFR-WASMHARD-6: Linux canonical evidence只证明实际选中并成功校验的 canonical Docker 产物；跨宿主 closure 还需要真实 Docker-capable `darwin-arm64` summary，属于独立跨宿主产物证据而非 `ci-tests full` 结果，不能由 Linux-only pass 外推。
- Security & Privacy: 仅处理模块源码路径、hash token、receipt evidence 与 CI metadata；不处理敏感业务数据。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (T1): 专题 PRD / 项目文档对齐到 Docker canonical gate 口径。
  - v1.1 (T2/T3/T6/T7): 完成 manifest strict 化与 identity / receipt 输入收敛。
  - v2.0 (T4/T5/T8): 接入 `wasm-determinism-gate` required checks 与发布策略收口。
- Technical Risks:
  - 风险-1: 外部 macOS summary 若生成环境不受控，会影响跨宿主产物证据可信度。
  - 风险-2: receipt / evidence schema 继续演化时，testing 专题需要同步回写。
  - 风险-3: 本地写入策略收紧后，团队需迁移到受控节点更新流程。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-CI-WASMHARD-001 | T1/T2/T3 | 独立 `wasm-determinism-gate`，不属于 `ci-tests` tier | `sync-m4/m5 --check` + canonical token schema 校验 | builtin wasm manifest 稳定性 |
| PRD-TESTING-CI-WASMHARD-002 | T1/T6/T7 | 独立 hash / receipt-evidence 验证，不属于 `ci-tests` tier | identity / receipt evidence 对账 + source 输入白名单验证 | identity hash 可复现性 |
| PRD-TESTING-CI-WASMHARD-003 | T1/T4/T5/T8/T9 | 独立 workflow context 与 required-check policy | `wasm-determinism-gate` planner + job-skip/selected-collect 分流验证 + 失败/缺失结果边界核对 + 实际 required-check 配置验证 | 发布门禁与策略治理 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-WASMHARD-001 | 发布清单收敛到单 canonical token | 长期保留双平台 token | Docker canonical build 已固定发布 hash 空间。 |
| DEC-WASMHARD-002 | 独立 gate 统一收敛到 `wasm-determinism-gate` | 继续维持多份旧 workflow 文档并行 | 现行执行入口必须唯一。 |
| DEC-WASMHARD-003 | GitHub-hosted 默认只跑 Linux，macOS 作为外部跨宿主产物证据（不属于 `ci-tests full`） | 恢复 GitHub-hosted macOS 默认门禁 | 当前 GitHub-hosted macOS 无 Docker daemon。 |
