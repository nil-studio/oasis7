# Builtin Wasm 身份共识设计

- 对应需求：[Builtin identity PRD](builtin-wasm-identity-consensus.prd.md)。
- 生命周期：active；本专题不以文档 active 表示构建或部署验收。

<a id="1-设计定位"></a>
## 1. 问题、目标与非目标

跨宿主 wasm 字节可以不同，但模块源码闭包、构建配方和工件身份必须可验证地绑定。identity 一致不能豁免远程实际 wasm hash 校验，也不能选择 governing world version。本设计不改变 ABI、运行语义、密码学算法或生产发布授权。

## 审读基线与当前差距

- Owner role：`runtime_engineer`、`blockchain_ops_engineer`；验证由 `qa_engineer` 独立核对。
- 本次文档/源码审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`，日期 `2026-10-15`。当前实现说明与历史阶段记录分别保留；本次不执行签名、部署、恢复或构建，不推导新的运行完成态。
- 产品关系只承接下表明确列出的局部义务；功能整体仍为部分覆盖，现有产品实现状态与原验收层级不变。

## 2. 上游与职责

专业 PRD 的 AC-1..6 拥有 manifest、sync/check、bootstrap 和 materializer 合同；WASM deterministic-build 主设计拥有生产 builder、release policy 与 materialization 边界。runtime/bootstrap 消费 identity；构建工具维护源码摘要；QA 验证适用模块与平台。

<a id="builtin-artifact-version-binding"></a>
## 3. 工件身份与版本边界

DWE-004 只在可验证工件身份子范围由本设计承接：bootstrap 的 artifact_identity 必须绑定选定模块的 manifest，不能由当前机器构建字节、缓存索引或客户端版本自行选择 governing version。最终选版仍由 canonical execution block 与 manifest activation boundary 决定；identity 不是 activation/finality 证明。历史重放、升级切换和工件长期可用性仍需 WASM/runtime 主合同及同候选组合验证。

<a id="builtin-identity-contract"></a>
<a id="2-设计结构"></a>
## 4. 结构、接口与数据合同

`module_id`、`source_hash`、`build_manifest_hash` 与 `identity_hash` 区分源码、配方与模块身份，实际 wasm hash 另行校验。当前 source_hash 必须复用 `oasis7_wasm_build::compute_source_hash`，覆盖模块及本地 path dependency closure 和适用 Cargo.lock，不重实现旧的“仅模块 crate”算法。sync 工具读取 build receipt、hash manifest 和模块映射，生成/核对三套 m1/m4/m5 identity manifest；bootstrap 再写入 ModuleArtifactIdentity。

<a id="3-关键接口-入口"></a>
## 5. 主流程与拒绝路径

1. 确定模块、源码依赖闭包与配方；变更依赖/lockfile 后重新计算身份。
2. sync/check 将 receipt 的 source/build/wasm 摘要与当前输入和 manifest 对照；缺失或错配必须显式失败，不能用旧 identity 填空。
3. bootstrap 绑定 artifact_identity；materializer 按主设计先获取并校验远程字节。
4. 获取失败时，只有适用合同允许的本地 fallback 才可编译/缓存；不得把远程 hash 不匹配解释为任意工件可执行或 production release 授权。

## 6. 状态、事务与持久化

identity manifest 和 build receipt 是构建事实；`.distfs/builtin_wasm/module_hash_index.txt` 记录 module_id 到最近可验证加载成功 wasm_hash 的本地缓存关系。缓存命中只证明该获取路径可复用，不证明治理激活或世界提交。输入变化、manifest/receipt 不一致与缓存失效需要重新核对，失败不能伪造成功绑定。本文不新增缓存格式或事务原子性保证；写入与失效行为须按 materializer 源码/回归验证。

<a id="4-约束与边界"></a>
## 7. 部署、安全与兼容

远程工件保持字节 hash 校验，本地 fallback 与 production 工件政策分开；动作级 release policy 不自动覆盖 builtin materializer。旧“有 hash 无 identity”迁移记录不等于有效绑定，也不重启旧 Phase/MIG 排期。本文不授予 signer custody、安装/激活权限或放宽 ABI 兼容。

## 8. 决策与剩余风险

选择共享 source-hash 算法避免漏掉 SDK/ABI path dependencies；选择 identity 与 wasm hash 双维度避免将跨宿主差异误判为任意字节授权。跨平台构建、m1/m4/m5 完整回归、bootstrap 与治理激活组合仍须实际验证；静态测试定义不是通过证据。

<a id="5-设计演进计划"></a>
## 9. 历史演进说明

旧设计的“先补专题互链、再按 Project 拆解实现测试”是历史规划，不是当前实施任务或完成证明。旧设计提出的可信签名来源、共识与治理身份一致及旧工件迁移边界仍是约束；本次不声称仅靠 identity manifest 已实现独立签名/门限/共识快照治理机制。具体发布签名 authority 由 WASM 主合同承担。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | 工件 identity 不得替代 canonical block 选版与 activation 证明。 | [工件与版本](#builtin-artifact-version-binding) | world-runtime / WASM build owner | 仅 artifact identity 子范围；完整 activation、历史 replay、工件长期可用与客户端组合未验证。 |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | 三套模块保留 source/build/identity 与实际 wasm hash 的完整性；bootstrap、fallback 遵循各自边界。 | [identity contract](#builtin-identity-contract) | build/sync owner / runtime / QA | 配对 PRD AC-1..6 的工件身份校验仅支撑 DWE-004 可验证工件子范围；不代签产品整体选版或发布。 |

### 11.1 验证映射表

以下是验证要求与已有入口的对应，不是本次执行结果。源码/测试定义只提供静态核对；实际验收须在同候选、适用 feature/config 与环境下运行，并保留命令、提交、输出、失败样例和未覆盖范围。文档检查通过不证明运行、部署或 release readiness。

| 上游 requirement / acceptance | 本设计条款 | 验证目标 / oracle | 验证方法 / 精确来源 | 证据产物 | 未覆盖边界 / residual risk |
| --- | --- | --- | --- | --- | --- |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | [工件与版本](#builtin-artifact-version-binding) | 本地缓存/回退不能证明版本已激活；两个测试只验证 fetch miss compile/cache 和 cached hash 在 manifest mismatch 时复用，不证明远程 hash 或 identity 校验。 | 执行 `env -u RUSTC_WRAPPER cargo test -p oasis7 --lib --features test_tier_full materializer_`；结合 [builtin materializer tests](../../../crates/oasis7_runtime/src/tests/builtin_wasm_materializer.rs) 的 fetch-miss/cache 用例核对工件获取子范围；完整版本激活需 QA 另按 DWE-004 组合验收。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 仅 artifact identity 子范围；完整 activation、历史 replay、工件长期可用与客户端组合未验证。 |
| [REQ-DWE-004](../../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-004)；Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc | [identity contract](#builtin-identity-contract) | 依赖变化改变源码身份；receipt/manifest 错配失败；不漏适用模块。 | 依 [testing manual](../../../testing-manual.md) 的构建环境执行 `env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_build source_hash` 与 `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --features sync-builtin-wasm-identity --bin sync_builtin_wasm_identity sync_source_hash_tracks_local_path_dependency_changes`；适用时分别运行 `./scripts/sync-m1-builtin-wasm-artifacts.sh --check`、`./scripts/sync-m4-builtin-wasm-artifacts.sh --check`、`./scripts/sync-m5-builtin-wasm-artifacts.sh --check`，记录实际选中范围。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 配对 PRD AC-1..6 的工件身份校验仅支撑 DWE-004 可验证工件子范围；不代签产品整体选版或发布。 |

## 精确源码与断言定位

- [共享 source hash 与回归](../../../crates/oasis7_wasm_build/src/lib.rs)：`source_hash_changes_when_local_path_dependency_changes`、`source_hash_changes_when_workspace_ancestor_lockfile_changes`，分别证明 fixture 的依赖和 lockfile 变化进入摘要。
- [sync identity 与回归](../../../crates/oasis7_distfs/src/bin/sync_builtin_wasm_identity.rs)：`sync_source_hash_tracks_local_path_dependency_changes`；不是三套真实工件重建通过。
- [materializer 回归](../../../crates/oasis7_runtime/src/tests/builtin_wasm_materializer.rs)：`materializer_fetch_miss_falls_back_to_compile_and_caches_blob`、`materializer_reuses_cached_module_hash_when_manifest_hashes_do_not_match` 只覆盖本地 fixture 获取/缓存，不覆盖 bootstrap activation、全部远程失败或生产发布。
