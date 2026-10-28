# oasis7 Runtime：可兑现节点资产与电力兑换闭环设计

- 对应需求文档: `doc/p2p/node/node-redeemable-power-asset.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 1. 设计定位
定义可兑现节点资产与电力兑换主闭环，统一节点资产记录、兑换条件、结算入口与兑现边界。

## 2. 设计结构
- 资产建模层：定义节点可兑现资产、余额与来源。
- 电力兑换层：把节点贡献/资产映射到电力或可消费权益。
- 结算执行层：处理兑换申请、确认与状态落账。
- 审计治理层：以 invariant report 检测守恒/签名异常但不自动修复；恢复后重验策略与报告。
- 签名演进层：兼容历史 `mintsig:v1` 摘要，以 `mintsig:v2`/`redeemsig:v1` Ed25519 和治理策略实现 fail-closed 门禁。

## 3. 关键接口 / 入口
- 节点资产台账
- 电力兑换申请/确认
- 结算落账入口
- 资产审计记录
- `RewardAssetInvariantReport`
- `RewardSignatureGovernancePolicy`

## 4. 约束与边界
- 资产余额与兑换结果必须可追溯。
- 兑换链路不得绕过治理与签名约束。
- v1 摘要不得宣传为密码学签名；v2/兑换签名策略缺少密钥或绑定时必须拒绝。
- 运行证据来自 chain-runtime CLI/runtime-root/report artifacts，历史 viewer flags 不构成当前运维入口。
- 不在本专题扩展完整外部市场流转。

## 5. 设计演进计划
- 先冻结资产与兑换规则。
- 再打通运行时结算。
- 最后持续验证审计、签名治理和旧快照兼容边界。


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
| Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-ledger) | PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 | [贡献形成受限协议资产账本](node-redeemable-power-asset.design.md#des-economic-power-ledger) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不更改兑换率，不新增转账/市场或当前可兑现承诺 |
| Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-redemption) | 兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 | [全部条件成立才燃烧资产并增加目标电力](node-redeemable-power-asset.design.md#des-economic-power-redemption) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不改额度、目标、权限或承诺当前服务可用 |
| Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-signature) | v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 | [签名治理条件不足必须拒绝](node-redeemable-power-asset.design.md#des-economic-power-signature) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不接触密钥，不证明HSM/KMS或跨组织custody |
| Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-audit) | mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 | [审计与恢复检测异常而不自动修复](node-redeemable-power-asset.design.md#des-economic-power-audit) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不证明mainnet或生产恢复，不恢复历史viewer flags入口 |

## 7. 既有结果的有限设计承接

<a id="des-economic-power-ledger"></a>
### 贡献形成受限协议资产账本

Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-ledger) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-power-ledger)。

PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不更改兑换率，不新增转账/市场或当前可兑现承诺。

<a id="des-economic-power-redemption"></a>
### 全部条件成立才燃烧资产并增加目标电力

Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-redemption) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-power-redemption)。

兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不改额度、目标、权限或承诺当前服务可用。

<a id="des-economic-power-signature"></a>
### 签名治理条件不足必须拒绝

Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-signature) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-power-signature)。

v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不接触密钥，不证明HSM/KMS或跨组织custody。

<a id="des-economic-power-audit"></a>
### 审计与恢复检测异常而不自动修复

Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-audit) / [产品唯一声明](../../product/world-rules-core-gameplay/prd.md#req-wr-economic-power-audit)。

mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。

本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不证明mainnet或生产恢复，不恢复历史viewer flags入口。

## 8. 验证设计

### 11.1 验证映射表

所有方法沿用原专业适用场景与层级，固定同一候选、配置、输入和环境后逐项保留实际结果；当前未运行。文档结构检查不能代替业务测试。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-ledger) | [贡献形成受限协议资产账本](node-redeemable-power-asset.design.md#des-economic-power-ledger) | PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-power-ledger)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 积分/credits换算与mint/balance/record对账，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不更改兑换率，不新增转账/市场或当前可兑现承诺；结构检查不证明业务验收 |
| Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-redemption) | [全部条件成立才燃烧资产并增加目标电力](node-redeemable-power-asset.design.md#des-economic-power-redemption) | 兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-power-redemption)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | credits/reserve/electricity与拒绝原因，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不改额度、目标、权限或承诺当前服务可用；结构检查不证明业务验收 |
| Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-signature) | [签名治理条件不足必须拒绝](node-redeemable-power-asset.design.md#des-economic-power-signature) | v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-power-signature)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 版本化载荷、绑定/策略与拒绝结果，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不接触密钥，不证明HSM/KMS或跨组织custody；结构检查不证明业务验收 |
| Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；[专业接受](node-redeemable-power-asset.prd.md#economic-contract-power-audit) | [审计与恢复检测异常而不自动修复](node-redeemable-power-asset.design.md#des-economic-power-audit) | mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-power-audit)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 快照前后状态、invariant report与策略复核，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不证明mainnet或生产恢复，不恢复历史viewer flags入口；结构检查不证明业务验收 |

## 9. 既有决策、风险与未决

原第4至5章及配对PRD的已采用决定、技术风险和计划完整保留。具体参数变化、经济安全结论、地址/控制绑定、签名治理、真实批准和执行许可不得由身份登记消除；有实质冲突时停止对应部分并由原owner裁决。本次不新增 Task 或冒用历史任务为当前交付；后续实际 PR 应引用本次适用身份与真实证据。
