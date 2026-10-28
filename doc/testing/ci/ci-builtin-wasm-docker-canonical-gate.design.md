# oasis7: Builtin Wasm Docker Canonical Gate 设计

- 对应需求文档: `doc/testing/ci/ci-builtin-wasm-docker-canonical-gate.prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

## 1. 设计定位
本设计统一 CI 与测试门禁的流水线分层、门禁策略、产物校验与失败保护。

### 1.1 目标与范围

以单一 Docker canonical 产物及 hash/receipt 对账收敛 builtin Wasm 门禁，按 planner 选中范围执行并保留失败保护。本设计只负责该独立工程门禁及校验/发布写入边界，不替代 `ci-tests required/full`，也不据此宣称产品功能或生产发布已验证。

## 2. 设计结构
- 流水线分层：独立 hash/receipt-evidence gate 按 planner 选中的 module set、runner 与 target 执行；`ci-tests required/full` 是另一套测试层级，外部 macOS summary 仅补跨宿主产物证据。
- 门禁策略层：定义通过条件、阻断条件与 required check 保护。
- 校验执行层：收敛构建、测试、hash/determinism 等自动校验入口。
- 回归治理层：沉淀失败签名、发布影响与后续演进。

## 3. 关键接口 / 入口
- CI workflow / check 入口
- 门禁/required check 配置
- runner/target/产物校验点
- CI 回归与失败签名

## 4. 约束与边界
- 门禁变更必须可审计、可回放。
- 基础门禁与增量专题门禁需边界清晰。planner 的 `scope=skip` 范围只执行规划，collect/verify 在 job 层跳过；partial/all 仅展开选中模块，必要执行失败、取消、缺失或意外跳过不能算通过。实际保护规则按 GitHub 配置与开发流程规范核实，文档不解除保护。
- 不在本专题重构整个平台 CI 基础设施。

## 5. 设计演进计划
- 先冻结门禁与执行分层。
- 再补专题校验与保护策略。
- 最后固化失败签名与回归。

## 6. 决策与风险

沿用配对 PRD 的 DEC-WASMHARD-001～003：单 canonical token、独立 `wasm-determinism-gate` 入口，以及 Linux 默认执行、macOS 外部跨宿主产物证据。

主要风险是文档 scope 与实际 planner、required-check 保护或证据范围发生漂移；对应选中任务必须真实成功，保护仍须按实际 GitHub 配置核实，跨宿主结论仍需真实 Docker-capable macOS summary 与 Linux 对账。本次文档校正不证明这些运行与配置验证已经完成。

## 2. 需求与验证关系

### 2.1 需求承接与分配表

| 上游要求 | 分配内容 | 本设计 | 负责范围 | 限制 |
| --- | --- | --- | --- | --- |
| [canonical gate](ci-builtin-wasm-docker-canonical-gate.prd.md#1-executive-summary) | Docker/hash/receipt 与测试层级分工 | [设计结构](#2-设计结构) | Wasm workflow 与汇总器 | 原生保护由实际配置确认；产品层 N/A：理由=本条是工程 CI 产物门禁合同，不拥有玩家产品行为要求；范围=Docker/hash/receipt 与测试层级分工，仍保留产物真实性、发布验证及产品影响消费者的适用义务 |
| [只读与发布边界](ci-builtin-wasm-docker-canonical-gate.prd.md#4-technical-specifications) | 默认校验和受控写入 | [约束与边界](#4-约束与边界) | 构建与发布脚本 | 本地通过不证明生产发布；产品层 N/A：理由=本条是工程校验与发布写入边界，不拥有玩家产品行为要求；范围=构建与发布脚本的只读/受控写入，仍保留安全、发布验证及产品影响消费者的适用义务 |

## 11. 验证

### 11.1 验证映射表

| 上游要求 | 本设计 | 验收内容 | 验证来源 | 实际记录 | 限制 |
| --- | --- | --- | --- | --- | --- |
| [canonical gate](ci-builtin-wasm-docker-canonical-gate.prd.md#1-executive-summary) | [设计结构](#2-设计结构) | module scope、hash 与 receipt 对账 | [测试手册](../../../testing-manual.md) | 当前 HEAD 的 CI 与 summary | 本地文档检查不能证明 Docker 产物 |
| [只读与发布边界](ci-builtin-wasm-docker-canonical-gate.prd.md#4-technical-specifications) | [约束与边界](#4-约束与边界) | 校验/发布写入分离 | [测试手册](../../../testing-manual.md) | 节点验收与发布记录 | CI 不执行生产激活 |
