# P2P Builtin Wasm 身份共识与跨平台构建方案

历史编辑/阶段边界：原 MIG/T0/Phase 批次尚未逐项匹配唯一实施 Task，故原记录与约束映射保留；文档人工重写、章节/引用检查和完成日期只证明原编辑或测试窗口，不生成当前排期、不代签机制、部署或 QA readiness。当前技术与安全合同继续由本文及配对设计持有；后续治理 Task 不冒充全部原实施者。

- 对应设计文档: `doc/p2p/consensus/builtin-wasm-identity-consensus.design.md`
- 交付事实：Git、PR、实际 CI 与评审记录；Issue / Project 按需跟踪，遵循[开发流程规范](../../engineering/workflow/source-of-truth.md)。

审计轮次: 5（原迁移记录，不代表本次运行验收）
## 1. Executive Summary
- Problem Statement: 解决“不同宿主构建产物 hash 漂移导致门禁不稳定”的生产问题，同时保留节点本地可构建能力。
- Proposed Solution: 将一致性目标从“跨平台产物字节 hash 必须一致”升级为“跨平台模块身份（identity）一致”。
- Success Criteria:
  - SC-1: 在不要求 runtime 节点执行前本机重编源码的前提下，支持多节点验证、产物复用与 identity 一致性校验。

## 2. User Experience & Functionality
- User Personas: builtin 模块维护者、节点运营者与身份校验维护者。
- User Scenarios & Frequency: 模块源码或本地依赖变化时生成/校验身份；节点选择、加载或复用 builtin 工件时验证对应关系。
- User Stories: 维护者能区分源码输入、构建配方、模块身份与实际 wasm 字节摘要，避免把跨平台兼容当作放宽远程工件校验。
- Critical User Flows: `确定模块及源码依赖闭包 -> 生成/核对 receipt 与 identity -> 同步检查 m1/m4/m5 清单 -> bootstrap 绑定 artifact_identity -> 获取/验证或复用工件`。
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 源码身份 | `module_id`、`source_hash` | 对模块及本地 path 依赖闭包计算摘要 | 输入变化后重新核对 identity | 使用共享 `compute_source_hash`；包含适用 Cargo.lock | 不允许消费者自造仅模块目录算法 |
| 构建与工件绑定 | `build_manifest_hash`、`identity_hash`、wasm hash | sync/check 对齐 manifest、receipt 与实际工件 | 一致才可作为该身份的工件；不一致显式失败 | identity 与字节 hash 是不同校验维度 | 不以 identity 一致豁免远程字节 hash 校验 |
| 运行时使用 | `artifact_identity`、本地缓存索引 | bootstrap 绑定身份；materializer 获取/复用工件 | 获取失败按适用 fallback 与错误边界处理 | 物化行为遵循 WASM 主设计 §5.0 | 动作级 release policy 不自动覆盖 builtin materializer |
- Acceptance Criteria:
  - AC-1: 为 builtin wasm 增加 identity manifest（`source_hash`、`build_manifest_hash`、`identity_hash`）。
  - AC-2: 扩展 `sync-m1/m4/m5` 流程：在同步 hash 清单时同时生成/校验 identity manifest。
  - AC-3: 运行时读取 identity manifest，为 bootstrap module manifest 写入可验证的 `artifact_identity`。
  - AC-4: 校验策略升级：`sync --check` 同时验证当前平台产物 hash 与 identity 数据完整性。
  - AC-5: 一致性回归覆盖 `m1/m4/m5` 三套 builtin 模块，避免只校验 `m1` 造成漏检；每次 PR 的适用执行范围由现行 CI/开发规范选择，不由历史命令要求所有普通改动全量检查。
  - AC-6: runtime builtin materializer 支持非 canonical 平台节点的本地回退编译与缓存复用，不再把“hash 必须命中清单”作为唯一可用条件。
- Non-Goals:
  - 修改 wasm ABI 协议版本与 runtime 模块执行语义。
  - 将 identity 共识专题扩展为发布级 Docker builder 设计；当前 canonical build 目标态由 `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md` 管理。
  - 替换现有 `sha256` 算法。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用（本专题不涉及 AI 模型能力改造）。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 源码输入摘要与构建配方摘要组成模块身份；sync/check、bootstrap 和工件消费者对齐身份及实际字节，不把二者混为一个 hash。
- Integration Points:
  - [共享源码摘要算法](../../../crates/oasis7_wasm_build/src/lib.rs)：`compute_source_hash` 及其本地依赖遍历。
  - [receipt / identity 同步校验](../../../crates/oasis7_distfs/src/bin/sync_builtin_wasm_identity.rs)。
  - [SC-3：receipt 的源码闭包合同](../../world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md#sr2-obligation-sc-3)。
  - [builtin materialization 边界](../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md#50-builtin-工件-materialization)。
- Edge Cases & Error Handling: 缺 identity、receipt 不匹配、源码依赖变化、远程工件 hash 不匹配必须分别可识别；不能把历史“有 hash 无 identity”的迁移状态默认为有效身份。
- Non-Functional Requirements: source hash 输入集合与遍历/归一化规则由共享算法维护；模块依赖的 SDK/ABI 变化不能漏入身份计算。
- Security & Privacy: 远程工件仍须验证实际字节；本地回退和缓存不授予发布签名或生产 release authorization。

当前 `source_hash` 输入包含模块及其本地 path dependency closure、适用的 Cargo.lock 和共享算法规定的路径/文件摘要。不能按“仅模块 crate + Cargo.lock”重实现。此处是源码与 SC-3 的静态对照，不表示本次已复算 hash、构建或执行跨宿主验证。

下节原文记录早期方案与迁移接口，保留其当时的定义、路径和命令；其中旧 source 输入边界及统一门禁命令不覆盖上述当前合同，也不重新触发历史迁移。

### 原文技术约束（保真）
#### 接口 / 数据
- 现有 hash 清单继续保留：
  - `crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.sha256`
  - `crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.sha256`
  - `crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.sha256`
- 新增 identity manifest：
  - `crates/oasis7_runtime/src/world/artifacts/m1_builtin_modules.identity.json`
  - `crates/oasis7_runtime/src/world/artifacts/m4_builtin_modules.identity.json`
  - `crates/oasis7_runtime/src/world/artifacts/m5_builtin_modules.identity.json`
- runtime 本地缓存索引（新增）：
  - ` .distfs/builtin_wasm/module_hash_index.txt`
  - 语义：记录 `module_id -> 最近一次可验证加载成功的 wasm_hash`，用于非 canonical 平台复用本地构建产物，避免每次启动重复编译。
- identity manifest 每个模块至少包含：
  - `module_id`
  - `source_hash`：模块源码输入集摘要（基于模块 crate 目录 + Cargo.lock）
  - `build_manifest_hash`：构建配方摘要（toolchain/target/build-std/canonicalizer）
  - `identity_hash`：`sha256("<module_id>:<source_hash>:<build_manifest_hash>")`
- 运行时接口：
  - `runtime/m{1,4,5}_builtin_wasm_artifact.rs` 增加读取 identity manifest 的辅助函数。
  - `runtime/world/bootstrap_{power,economy,gameplay}.rs` 改为使用 identity manifest 生成 `ModuleArtifactIdentity`。
  - `runtime/builtin_wasm_materializer.rs` 在“远程拉取失败 + 本地编译回退”路径允许落地非清单 hash（仅本地编译路径），并写入模块 hash 索引。
- 门禁接口：
  - `scripts/ci-tests.sh required` 统一执行：
    - `./scripts/sync-m1-builtin-wasm-artifacts.sh --check`
    - `./scripts/sync-m4-builtin-wasm-artifacts.sh --check`
    - `./scripts/sync-m5-builtin-wasm-artifacts.sh --check`

## 5. Risks & Roadmap
- Phased Rollout（历史迁移阶段，非待办或当前开发准入）:
  - M1：设计文档、GitHub Issue/Project task truth落地。
  - M2：`sync-m1` 主脚本扩展 identity manifest 生成与校验，并对 `m4/m5` 复用。
  - M3：runtime 接入 identity manifest，替换 bootstrap 占位 identity 逻辑。
  - M4：测试补齐与 required 回归通过。
  - M5：项目文档与 devlog 收口。
  - M6：全系统迁移（CI/pre-commit/runtime fallback）与过时文档归档完成。
- Technical Risks:
  - `source_hash` 必须覆盖模块及本地 path 依赖闭包，并复用共享算法与适用 Cargo.lock；缩成仅模块目录会漏掉共享 SDK/ABI 变化，自行扩大为整个仓库则会引入无关抖动。
  - 旧版本清单迁移窗口内可能出现“有 hash 无 identity”状态，需提供兼容提示与一次性同步脚本。
  - 若开发者在非标准环境覆盖构建参数，`build_manifest_hash` 会变化并触发门禁，需要清晰错误提示。
  - 非 canonical 平台允许本地回退编译后，必须保证远程拉取路径仍然严格校验 hash 清单，避免把“跨平台兼容”退化成“任意字节可执行”。

## 6. Validation & Decision Record
- Test Plan & Traceability（验证要求，不是本次执行结果）:
| 合同 | 验证入口 / 方法 | 负向与回归范围 |
| --- | --- | --- |
| AC-1 / AC-2 / AC-4 | 共享源码摘要测试、receipt/identity 同步校验与适用 `sync-m1/m4/m5 --check` | 本地依赖变化、缺字段、receipt/hash 不一致不得被接受 |
| AC-3 | bootstrap artifact identity 与 manifest 绑定回归 | 缺失/错配 identity 不得伪装成有效绑定 |
| AC-5 | 适用 CI 计划及三套 builtin 一致性检查 | 不漏受影响模块；未选中的检查不伪报运行成功 |
| AC-6 | builtin materializer 获取、hash 验证、缓存与 fallback 回归 | 远程 hash 不匹配、本地缓存失效及物化失败；不代签发布策略 |

实际命令、source/build、结果及未覆盖范围记录在对应 PR/CI；文档治理通过不等于上述技术验收。跨宿主构建和发布证据仍按 WASM 主合同执行。

### 历史迁移 provenance（不承载机制验收）
原 `PRD-P2P-MIG-057-001` / `T0~Tn` 的文档治理与章节完整性检查仅证明迁移时的文档格式；不再作为当前功能矩阵或技术测试占位。原任务标识保留，不新建迁移工作项。

- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-PRD-P2P-MIG-057-001 | 逐篇阅读后人工重写 | 直接重命名 | 保证语义保真和可审计性。 |

## 原文约束点映射（内容保真）
- 原“目标” -> 第 1 章。
- 原“范围” -> 第 2 章。
- 原“接口/数据、里程碑、风险” -> 第 4~6 章。
