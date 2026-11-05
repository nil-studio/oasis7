# Preview provider 原生测试附属产物设计

- 状态：active
- 专业 owner：`qa_engineer`

## 问题与范围

真实 provider preview 验收需要同一候选源码构建的 local provider WASM、metadata 和官方 receipt。原生编译、归档和测试消费必须保持同一来源，不能依赖开发机缓存或占位模块。本文定义该测试附属产物的准备、验证和交付，不承接业务 consumer 的世界授权或完成判定。

## 上游需求承接

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29。沿用[CI 分级执行设计](ci-tiered-execution.design.md)的共享原生产物与 full-tier 验收合同；不声明新要求或改变可信 BASE 的选择权限。

### 2.1 需求承接与分配表

| 上游要求 | 分配内容 | 本设计 | 负责范围 | 限制 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；[共享原生产物](ci-tiered-execution.prd.md#ci-formal-process) | 同源 provider 附件准备与归档消费 | [产物合同](#preview-provider-native-design) | CI 维护者与 QA | 产品适用性：N/A；reason=测试附属产物；scope=原生测试 bundle，不改变发布权限 |
| Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；[完整分层验收](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full) | 实际 full-tier 消费者的材料准备 | [产物合同](#preview-provider-native-design) | CI 维护者与 QA | 产品适用性：N/A；reason=CI 验收接线；scope=provider 消费者，不代签 hosted 容量或性能 |

<a id="preview-provider-native-design"></a>

## 设计条款

ordinary expanded／full producer 在实际 native 库存包含启用 `test_tier_full` 的 `oasis7_chain_runtime` 测试消费者时准备材料；仅含其他包的隔离编排夹具不启动模块构建。使用官方 `scripts/build-wasm-module.sh --candidate-build`，按同 checkout 构建固定的 `module.runtime.local-test-provider`。`ci-tests.sh` 的 `full`、`full-standard`、`full-extended`、`full-core` CLI 在进入对应测试前使用同一准备入口。

复用 `ci-test-artifacts.py prepare-provider` 与 `sync_builtin_wasm_identity --verify-provider-artifact`。只读 checker 使用官方 SourceRule2 完整闭包、builder recipe／execution 和 receipt 验证 API，允许 locked Cargo metadata 解析，不启动模块执行或身份同步。WASM、metadata、官方 receipt、native checker 及其必要动态库绑定 native bundle 的 tested/source SHA、checkout tree、固定文件角色与 hash。元数据与 receipt 的身份字段、WASM hash 和长度均核对。

pack／load 验证完整材料、当前 source-input v2 和身份后，才向实际测试进程注入 `OASIS7_PREVIEW_TEST_PROVIDER_WASM` 与 `OASIS7_PREVIEW_TEST_PROVIDER_METADATA`。load 先清除继承的旧变量。缺失、损坏、跨 source 或路径越界直接失败；不回退 placeholder、其他 checkout 或本机缓存材料。归档只包含所选 native 闭包及其显式 provider 附件。

## 验证

`ci-formal-process.test.py` 的 provider 合同覆盖来源身份、固定文件角色、完整性、路径和 symlink 拒绝，以及官方 checker 失败时禁止导出环境。其 mock 只证明编排；真实官方构建、原生 checker 与 pack／load 仍由实际材料链验证。已有 native feature 选择与归档消费回归继续执行，后续 full-tier 业务 consumer 在材料缺失时失败而非 skip。`ci-workflow.test.py` 核对原 full-tier viewer、严格 wasmtime profile 与失败传播，前置准备不替代这些检查。

## 决策与边界

本能力复用现有 CI 入口，不修改 producer recipe、发布身份、ignored runner 登记、required gate 或 secret／外部写权限。纯 Python 合同、可控 fixture 或合法归档不能代替真实 consumer 执行及业务结果；实际模型调用、signer/provider/witness 隔离与故障域验收继续由各自领域合同判定。

### 11.1 验证映射表

| 上游要求 | 本设计 | 验收内容 | 验证来源 | 实际记录 | 限制 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7；[共享原生产物](ci-tiered-execution.prd.md#ci-formal-process) | [产物合同](#preview-provider-native-design) | 来源、完整性、路径拒绝与环境导出 | [原生归档合同](../../../scripts/ci-formal-process.test.py) | 当前 PR 固定候选的官方构建、checker、pack/load 与测试记录 | mock 不代替真实 consumer |
| Trace refs: O7-REQ-a34ef03b-c8ee-4988-af23-67ad20e65e29；[完整分层验收](ci-tiered-execution.prd.md#ci-fast-pr-periodic-full) | [产物合同](#preview-provider-native-design) | full-tier CLI 接线和失败传播 | [工作流合同](../../../scripts/ci-workflow.test.py) | 当前 PR 的 Linux 合同与 required CI | 本地合同不代签 hosted 完整覆盖和业务结果 |
