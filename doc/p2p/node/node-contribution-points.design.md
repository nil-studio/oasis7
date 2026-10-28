# oasis7 Runtime：节点贡献积分激励设计

- 对应需求文档: `doc/p2p/node/node-contribution-points.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 1. 设计定位
定义节点贡献积分激励主模型，统一贡献事件、积分累计、结算入口与奖励约束。

## 2. 设计结构
- 贡献采集层：采集在线、复制、执行与协作等贡献事件。
- 积分计算层：按规则累计积分并处理去重、衰减与封顶。
- 双池计分层：分别计算主池与存储池合格得分；启用独立存储池时从主池权重中排除存储项，两个固定预算分别确定性分配后再合并为本 epoch 积分。
- Runtime 结算层：`oasis7_chain_runtime` 在采样前从原子 `reward-runtime-state.json` 恢复 collector 实现状态（ledger、heuristics、epoch 起点、cursor、当前 epoch accumulator），并在采样后持久化；以 epoch/node 幂等身份抵抗重复 tick、恢复与重放。快照写入不直接修改奖励台账；不可读状态文件只产生显式告警/指标和新 collector，不构成资产对账恢复。
- 奖励映射层：积分可分别进入受身份/签名/预算/nonce/replay 约束的 PowerCredit 路径，或进入由 `node_service_reward` 预算供给的主链 Token bridge；两条路径不是同一池，也不是自动兑换。
- 观测审计层：输出节点贡献明细、汇总结果与回归证据；3 节点、2 epoch 夹具覆盖计算、存储、在线、可靠性和惩罚画像。

## 3. 关键接口 / 入口
- 贡献事件模型
- 积分累计入口
- 奖励映射规则
- 贡献审计视图

## 4. 约束与边界
- 积分口径需与奖励结算口径保持一致。
- 重复贡献不得被重复计分。
- 固定 epoch 池必须守恒，累计积分必须单调；多节点回归必须断言排序和惩罚效果。
- 采样顺序固定为“收集样本 -> 计算主池/存储池得分 -> 分别确定性分配 -> 合并积分 -> 进入独立治理的资产结算路径”；snapshot 不是结算授权，恢复或 replay 不得重新 mint。
- uptime 有挑战记录时优先使用挑战通过率，否则使用 epoch 时长回退；storage 必须同时满足最小挑战数、严格通过率门槛与可选质押封顶。
- 本地夹具只证明确定性结算闭环，不外推为真实公网证明或生产 readiness。
- challenge/collector 是启发式输入，不构成 PoRep/PoSt、VRF、多观察点、抵押罚没或经济安全证明；多激励路径必须单独审查重复奖励。
- 不在本专题直接定义跨链资产发行细节。

## 5. 设计演进计划
- 先冻结贡献事件与积分规则。
- 再接运行时幂等结算闭环和至少 3 节点、2 epoch 的多节点回归。
- 最后联动奖励与治理专题收口。


## 6. 本次规范化范围、上下游与状态

本次仅规范化既有经济合同的身份、需求接收和验证计划；原设计正文、参数、接口、签名策略、权限、计划和实现声明原样保留。

产品结果由世界规则根定位，专业 PRD 继续拥有规则/数值；P2P/runtime、治理维护者与 QA 分别负责技术、治理和验收，涉及贡献奖励的资格/审批仍归 README reward governance。这里的设计关系和 planned 方法不启用资产、不授权交易，也不是本轮业务测试或生产证据。

- 当前基线：2026-10-15，固定文档提交 `cde57786e711b03a9c9fa2a5174aba357e8350d5`；本次只核对文档关系，当前实现与完整验收未核实。
- 目标：原独立经济结果可从产品条款追至专业规则、有限设计和真实验证入口。
- 差距：下列全部业务场景尚未在本次候选运行；已有源码、模板、局部夹具或历史结果不外推完整实现/生产安全。
- 裁剪：不改变技术合同、协议/ABI、算法、权限或状态模型，仅追加有限承接和 planned 验证；原文未展开的技术细节仍由专业原条款与后续实现评审承担，不填空N/A或宣称十二段完整设计。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；[专业接受](node-contribution-points.prd.md#economic-contract-points-eligibility) | 为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 | [基础义务与合格额外贡献分离](node-contribution-points.design.md#des-economic-points-eligibility) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不能推出所有玩家获益、实际采样可信或公开网络安全 |
| Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；[专业接受](node-contribution-points.prd.md#economic-contract-points-allocation) | 两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 | [固定双池的确定性且不重复分配](node-contribution-points.design.md#des-economic-points-allocation) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不调整预算/权重/阈值，不证明实际数值平衡 |
| Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；[专业接受](node-contribution-points.prd.md#economic-contract-points-recovery) | 同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 | [epoch结算与恢复不重复入账](node-contribution-points.design.md#des-economic-points-recovery) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody |
| Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；[专业接受](node-contribution-points.prd.md#economic-contract-points-asset-gates) | 积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 | [积分与独立资产路径保持门禁](node-contribution-points.design.md#des-economic-points-asset-gates) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness |

## 7. 既有结果的有限设计承接

<a id="des-economic-points-eligibility"></a>
### 基础义务与合格额外贡献分离

Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；[专业接受](node-contribution-points.prd.md#economic-contract-points-eligibility) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-points-eligibility)。

为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不能推出所有玩家获益、实际采样可信或公开网络安全。

<a id="des-economic-points-allocation"></a>
### 固定双池的确定性且不重复分配

Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；[专业接受](node-contribution-points.prd.md#economic-contract-points-allocation) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-points-allocation)。

两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不调整预算/权重/阈值，不证明实际数值平衡。

<a id="des-economic-points-recovery"></a>
### epoch结算与恢复不重复入账

Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；[专业接受](node-contribution-points.prd.md#economic-contract-points-recovery) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-points-recovery)。

同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody。

<a id="des-economic-points-asset-gates"></a>
### 积分与独立资产路径保持门禁

Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；[专业接受](node-contribution-points.prd.md#economic-contract-points-asset-gates) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-points-asset-gates)。

积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness。

## 8. 验证设计

### 11.1 验证映射表

所有方法沿用原专业适用场景与层级，固定同一候选、配置、输入和环境后逐项保留实际结果；当前未运行。文档结构检查不能代替业务测试。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；[专业接受](node-contribution-points.prd.md#economic-contract-points-eligibility) | [基础义务与合格额外贡献分离](node-contribution-points.design.md#des-economic-points-eligibility) | 为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-points-eligibility)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 原贡献画像、输入、资格与得分对账，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不能推出所有玩家获益、实际采样可信或公开网络安全；结构检查不证明业务验收 |
| Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；[专业接受](node-contribution-points.prd.md#economic-contract-points-allocation) | [固定双池的确定性且不重复分配](node-contribution-points.design.md#des-economic-points-allocation) | 两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-points-allocation)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 逐epoch/node得分、两池分配及预算差额，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不调整预算/权重/阈值，不证明实际数值平衡；结构检查不证明业务验收 |
| Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；[专业接受](node-contribution-points.prd.md#economic-contract-points-recovery) | [epoch结算与恢复不重复入账](node-contribution-points.design.md#des-economic-points-recovery) | 同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-points-recovery)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 恢复前后ledger、settlement及降级记录，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody；结构检查不证明业务验收 |
| Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；[专业接受](node-contribution-points.prd.md#economic-contract-points-asset-gates) | [积分与独立资产路径保持门禁](node-contribution-points.design.md#des-economic-points-asset-gates) | 积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-points-asset-gates)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 每条路径预算/授权/拒绝与独立审查记录，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness；结构检查不证明业务验收 |

## 9. 既有决策、风险与未决

原第4至5章及配对PRD的已采用决定、技术风险和计划完整保留。具体参数变化、经济安全结论、地址/控制绑定、签名治理、真实批准和执行许可不得由身份登记消除；有实质冲突时停止对应部分并由原owner裁决。本次不新增 Task 或冒用历史任务为当前交付；后续实际 PR 应引用本次适用身份与真实证据。
