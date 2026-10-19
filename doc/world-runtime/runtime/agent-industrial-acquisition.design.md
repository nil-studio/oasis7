# Agent 工业取得合同

## 文档身份

- 生命周期：`active`
- Owner role：`runtime_engineer`
- Last reviewed：2026-10-19
- 上游：[Agent 所有权 REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928

## 问题、目标与范围

额外 Agent 的工业投入与交付必须产生同一权威身份和控制结果；现有材料生产与注册/claim 拼接不足以证明这一点。本设计只承接常态工业取得，保留单主 Agent 路线，不启动经济转让或部署。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004) Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 常态额外 Agent 的投入、产能、交付及 quota 非旁路 | [DES-WR-AOS-004](#des-wr-aos-004) | runtime / game | 经济转让与部署 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004) Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | 正常、拒绝、等待和重试恢复 | [DES-WR-AOS-004](#des-wr-aos-004) | QA / Agent / API | 组件不代签正式 Skill 实玩 |


## 边界、结构与 v1 政策执行

初始政策由 owner 授权制定。每个订单制造一个 Agent，投入 alloy_plate 12、control_chip 2、motor_mk1 4、sensor_pack 2，使用订单所有者拥有且完好、空闲的 assembler，持续 120 个权威世界 tick；每个工厂同时一个订单。材料从所有者 Agent ledger 一次扣除，不能使用他人或世界库存。订单占用与普通 recipe 互斥，交付前工厂不可回收。无库存或产能时确认原子拒绝，补足后用原请求身份重试；确认后材料已投入，不退款造资源。

订单保存所有者、工厂、目标身份、投入、政策版本、确认及最早交付时间。目标 ID 由订单身份派生并永久保留，不允许注册/认领动作抢占。工厂失效或 claim 资金/slot 不足时保持等待并公开原因；修复或补足后重试，不能提前创建身份或控制权。

交付在一个事件提交中创建 AgentCell、扣除既有 claim upfront、锁定 bond、建立 claim 和订单交付 receipt；沿用既有 reputation quota、upkeep、release 和 delegation，不由 quota 发放 Agent。单主 Agent 路线不要求下单。重复同请求同内容返回原 receipt，内容冲突拒绝；跨请求重复交付由订单状态与目标身份阻断。状态、材料、claim、身份和 receipt 共同进入 canonical root、snapshot 与 replay。

## 验证与兼容

旧快照缺字段代表无订单，不推导历史工业来源。既有 AgentClaimed 事件缺 acquisition_contract_version 时按 v0 重放；新动作使用 v1，非首个认领须有已交付工业来源。历史已持有 claim 不撤销，不能把新注册身份加 quota 当作工业供给。新增事件仅由兼容消费者处理；旧二进制不能读取新事件后继续执行。回归覆盖正常交付、资源/产能不足、交付前拒绝、工厂失效等待、资金不足恢复、同/跨请求重放和 snapshot 恢复。正式 Agent API/Skill 及玩家表面实玩分别验证，不由组件测试代签。


<a id="des-wr-aos-004"></a>
## DES-WR-AOS-004：订单交付原子性

订单、投入、产能、身份与 claim 共享同一业务提交边界；未交付阶段不创建目标身份。前文政策和后文接口共同约束此条款。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游要求与验收 | 本设计条款 | 独立 obligation | 验证方法 / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004) Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-WR-AOS-004](#des-wr-aos-004) | 投入、产能、等待与同订单单次交付 | [工业回归](../../../crates/oasis7_runtime/src/world_rules/tests/industrial_tests.rs)；`cargo test -p oasis7_runtime --lib world_rules`；typed action / tick / sparse publication / snapshot | 订单、材料、claim 与 root 同候选断言 | 正式玩家接入、跨节点与部署 |
| [AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004) Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928 | [DES-WR-AOS-004](#des-wr-aos-004) | 正常、拒绝、等待恢复与 quota 非旁路 | [工业回归](../../../crates/oasis7_runtime/src/world_rules/tests/industrial_tests.rs)；按正式 Skill 自主试玩手册分别记录组件和真实运行范围 | 原动作/结果、历史交付身份与恢复对账 | 组件断言不代签模型实玩 |

## 接口、失败与恢复

`Action::WorldRule` 包含 actor、稳定 request 和 typed `WorldRuleCommandV1`；`WorldRuleCommitted` 绑定 action ID 与治理 epoch。`WorldRulesStateV1` 与原生 claim/material ledger 同属 canonical state。`ConfirmAgentOrder` 只有原子确认 receipt；`DeliverAgentOrder` 或到期 tick 复核工厂与资金再提交交付。客户端 `world-rule` 沿用签名任务 context、fence、预算与原请求恢复；native delegation 的 `WorldRule` action kind 必须匹配工厂/订单 object，不能借其他生产授权执行。读面公开已确认订单，查询不触发新订单。

不支持的版本、错误 owner、资源/产能不足、未到交付时间、工厂损坏、claim cap 或资金不足均拒绝本次效果，等待订单保留。请求 ID 在 actor 内固定内容；响应丢失查询原 receipt。标识最长 128 UTF-8 bytes；每工厂最多一个未交付订单。历史保留不按交付完成自动 GC；回滚不得逆转已交付的身份与经济效果。


## 决策与风险

工期和材料是初始版本政策，后续调整必须保留旧订单快照与版本。订单交付沿用 claim 的资格和非零成本，不另造免费的控制权。材料投入后不自动退款，资金或工厂阻断不改变订单身份。未覆盖正式玩家实玩、跨节点及发布验证时，不能宣称完整验收。
