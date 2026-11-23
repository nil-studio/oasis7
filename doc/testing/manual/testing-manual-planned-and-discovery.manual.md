# Testing Manual 分册：节点经济 QA 场景、统一 Seed/Resource 验证计划与 Discovery 观测

- 主测试手册：[`testing-manual.md`](../../../testing-manual.md)
- 本分册按 #6507 原样承接 `testing-manual.md` 末尾的 planned QA 场景、统一 seed/resource 验证计划与 Discovery / local bootstrap trust 观测说明，不改变既有验收边界。

## 节点既有经济合同 QA 场景（planned，未执行）

本节从既有节点积分、PowerCredit、创世分配/奖励专业接受条件抽取可逐项执行的 QA 方法，仅供后续获得适用授权的验证任务使用；没有新增经济功能、资格、参数、额度、审批、权限或当前能力结论。当前这些场景均未执行，文档检查不能使其通过。每项只能使用固定同一候选、配置版本、受控fixture及原专业合同指定的角色/阶段；不得在fixture中填入真实地址/密钥、绑定真实账户、启动实际铸造/兑换/奖励发放。真实经济操作始终另需原逐次授权及全部原前置，本手册不授予该授权。

业务oracle以各项链接的专业原文为准；本节不成为经济参数第二真值。正式验证者须保存候选commit、环境、fixture/输入和配置引用、适用验收与层级、预期/实际结果、拒绝原因和未覆盖范围。局部测试定义只能证明其列出的断言，不代表完整矩阵已通过。生产控制或审批证据缺失时如实保留pending/conditional/block，不能用fixture补真值。本任务不运行任何业务场景。

<a id="qa-economic-points-eligibility"></a>
### 基础义务与合格额外贡献分离：planned QA

Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；[准确专业接受关系](../../../doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得SC-1..3、贡献输入与计分公式的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原 SC-1..3 与贡献输入分别构造自身模拟、代跑、维护、有效/无效存储和在线挑战/回退样例，逐项核对资格与计分；不得把自身义务或不合格贡献计成额外收益。 当前只定义方法，不执行该步骤。
- 原权威oracle：为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不能推出所有玩家获益、实际采样可信或公开网络安全；不得凭其他项green代签。
- 证据字段与层级：原贡献画像、输入、资格与得分对账；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-points-allocation"></a>
### 固定双池的确定性且不重复分配：planned QA

Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；[准确专业接受关系](../../../doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得AC-9、独立存储奖励池与结算合并规则的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按 AC-8/9 的至少3节点、连续2epoch画像，分别覆盖存储池启用/关闭、严格阈值、最小挑战数、可选质押封顶、零合格贡献及惩罚；核对各池预算不超发、排序与累计单调，不假定余额自动结转。 当前只定义方法，不执行该步骤。
- 原权威oracle：两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不调整预算/权重/阈值，不证明实际数值平衡；不得凭其他项green代签。
- 证据字段与层级：逐epoch/node得分、两池分配及预算差额；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-points-recovery"></a>
### epoch结算与恢复不重复入账：planned QA

Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；[准确专业接受关系](../../../doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得AC-7、Runtime 与多节点闭环约束的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原 runtime闭环约束对同一epoch/node注入重复tick、重启恢复、replay及不可读reward-runtime-state.json，对账awarded/cumulative不重复增加、快照不改ledger与明确warning/metric；结合3节点2epoch场景。 当前只定义方法，不执行该步骤。
- 原权威oracle：同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody；不得凭其他项green代签。
- 证据字段与层级：恢复前后ledger、settlement及降级记录；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-points-asset-gates"></a>
### 积分与独立资产路径保持门禁：planned QA

Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；[准确专业接受关系](../../../doc/p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)取得积分与资产结算边界的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原积分与资产边界分别核对PowerCredit身份/签名/预算/reserve/nonce/replay门禁和Token同epoch node_service_reward预算分配；双路径并用列出重复奖励审查，缺该审查不形成经济安全通过。 当前只定义方法，不执行该步骤。
- 原权威oracle：积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-contribution-points.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-contribution-points.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness；不得凭其他项green代签。
- 证据字段与层级：每条路径预算/授权/拒绝与独立审查记录；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-ledger"></a>
### 贡献形成受限协议资产账本：planned QA

Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；[准确专业接受关系](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得AC-1..6、资产与储备账本和结算记录的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原test_tier_required铸造守恒场景对照awarded_points、points_per_credit、floor结果与mint record及余额，核对未获授权的资产路径不能形成有效铸造。 当前只定义方法，不执行该步骤。
- 原权威oracle：PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不更改兑换率，不新增转账/市场或当前可兑现承诺；不得凭其他项green代签。
- 证据字段与层级：积分/credits换算与mint/balance/record对账；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-redemption"></a>
### 全部条件成立才燃烧资产并增加目标电力：planned QA

Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；[准确专业接受关系](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得兑换动作与事件、关键规则的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原required兑换扣账/电力一致、reserve不足、nonce重放拒绝场景，同时覆盖最小量和epoch上限；按full多节点多epoch压力对账原动作及事件结果。 当前只定义方法，不执行该步骤。
- 原权威oracle：兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不改额度、目标、权限或承诺当前服务可用；不得凭其他项green代签。
- 证据字段与层级：credits/reserve/electricity与拒绝原因；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-signature"></a>
### 签名治理条件不足必须拒绝：planned QA

Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；[准确专业接受关系](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得审计与签名治理演进、MIG-097验收的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原MIG-097的required/full验证v2/redeem签名、治理拒绝与快照策略，分别对缺签名、错误绑定、策略要求无法满足和历史v1兼容边界核验；不改变worker安全默认。 当前只定义方法，不执行该步骤。
- 原权威oracle：v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不接触密钥，不证明HSM/KMS或跨组织custody；不得凭其他项green代签。
- 证据字段与层级：版本化载荷、绑定/策略与拒绝结果；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-power-audit"></a>
### 审计与恢复检测异常而不自动修复：planned QA

Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；[准确专业接受关系](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)取得状态与回放一致性、审计与签名治理演进、MIG-096验收的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原MIG-096 required/full审计、required快照余额一致及full重启replay/libp2p复制抽样，对比余额储备和记录，核对报告列异常且不自动修复/对账；恢复后重验策略。 当前只定义方法，不执行该步骤。
- 原权威oracle：mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/node/node-redeemable-power-asset.prd.md#6-validation--decision-record)为准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不证明mainnet或生产恢复，不恢复历史viewer flags入口；不得凭其他项green代签。
- 证据字段与层级：快照前后状态、invariant report与策略复核；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原专业required/full的适用场景分别记录；本地fixture不能代full实际多节点/复制或公开网络。该项planned，不是执行日志。

<a id="qa-economic-genesis-allocation"></a>
### 冻结分配可核算到各bucket与recipient：planned QA

Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；[准确专业接受关系](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得SC-1/8、AC-1/8/9/12/13、DEC-TOKEN-INIT-009的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原TOKEN-INIT-001/002与QA TGA-01/02/08，逐条核对七桶参数、比例合计、冻结绝对金额、rounding及recipient聚合，不使用手工补差或把custody写成treasury。 当前只定义方法，不执行该步骤。
- 原权威oracle：按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](../../../doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](../../../doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不变更总量/比例/账户，不执行InitializeMainTokenGenesis；不得凭其他项green代签。
- 证据字段与层级：固定参数版本、七桶金额与rounding对账；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。

<a id="qa-economic-genesis-control"></a>
### 控制主体与个人受益及释放边界分别可审计：planned QA

Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；[准确专业接受关系](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得SC-2..4、AC-2..5、DEC-TOKEN-INIT-002/004的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按QA TGA-03..07和原TOKEN-INIT-001/002核对控制主体、个人受益拆分、vesting/claim与创世/首年释放；缺主体、个人拆分或计划数值不得以比例表正确代签。 当前只定义方法，不执行该步骤。
- 原权威oracle：项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](../../../doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](../../../doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。不改控制人/vesting/释放额度，不把协议储备当团队库存；不得凭其他项green代签。
- 证据字段与层级：真实控制映射、受益拆分、释放计划与阻断依据；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。

<a id="qa-economic-genesis-freeze"></a>
### 逻辑冻结不等于真实mint-ready：planned QA

Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；[准确专业接受关系](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得SC-6、AC-11、DEC-TOKEN-INIT-006、正式执行清单的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：planned依formal freeze sheet §5/§7、QA checklist §7的真实地址/controller/个人拆分/rounding/最终verdict逐项核对；任一TBD或conditional/block必须阻断，模板与历史勾选不代签。 当前只定义方法，不执行该步骤。
- 原权威oracle：slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](../../../doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](../../../doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易；不得凭其他项green代签。
- 证据字段与层级：绑定证据、完整pre-mint清单与该次QA verdict；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。

<a id="qa-economic-genesis-reward"></a>
### 独立reserve只消费最终复核的批准记录：planned QA

Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；[准确专业接受关系](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)。

- 输入与适用条件：固定候选、配置/输入版本和适用原角色/阶段；从[原技术约束](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)取得AC-6/10、DEC-TOKEN-INIT-003/005、Execution Path Decision的原字段/参数，不自行调值；正例与缺失/错误条件反例使用受控fixture，生产事实保持原未决。
- 方法：按原TOKEN-INIT-003及canonical reward pack/ledger复核贡献证据、Approval ID、approved状态、distribution前actual-value review和最终Actual Amount；缺任何条件不得发放，执行后的Distribution Ref/date按原ledger闭合。 当前只定义方法，不执行该步骤。
- 原权威oracle：独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 原SC/AC/DEC、公式、严格比较、权限和状态以[专业原文](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#4-technical-specifications)与[原验收](../../../doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#6-validation--decision-record)为准；另以[QA原verdict与审计项](../../../doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md#71-verdict-规则)、[freeze原前置](../../../doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#5-pre-mint-checklist)和[原ledger批准](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#5-approval-summary) / [distribution闭合](../../../doc/readme/governance/readme-limited-preview-contributor-reward-ledger.md#6-distribution-closure)分别核相应条款，不能混用模板和真实批准。
- 失败边界：任一原成功条件未满足或无法证明时，不记录该项通过；保留原拒绝/未核实/降级/conditional/block语义及原因。本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次；不得凭其他项green代签。
- 证据字段与层级：canonical批准行、最终金额复核、审批及distribution引用；另保存上述统一候选/环境/输入/预期实际/拒绝/剩余字段。沿原§6的test_tier_required。该项planned，不是执行日志。


<a id="unified-world-resource-verification"></a>
## 统一 seed/resource 已采用子范围与正式目标验证计划

来源为 [unified 专业设计](../../../doc/world-simulator/scenario/unified-world-seed-fragment-runtime.design.md#unified-verification-plan)。本节仅登记现有合同的计划与阻塞，不改测试层级、原验收或正式设计选择；本次未运行。所有命令先核固定候选的 test discovery，0 tests 不视为通过。

<a id="uws-v-local-spawn"></a>
### UWS-V-LOCAL-SPAWN：本地默认初始化

对固定 candidate 执行 `cargo test -p oasis7_viewer_runtime --lib bootstrap_formal_release_runtime_world_uses_seeded_fragment_bootstrap_agent`，先确认 discovery 命中该具名测试。沿 [support.rs](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/support.rs) 原 fixture 核本地默认 Agent 坐标与 [gameplay_actions.rs](../../../crates/oasis7_runtime_host/src/gameplay_actions.rs) 所取 seed-model 坐标，保存版本、有效配置、模型坐标、命令和原输出。该断言不验证正式 Agent location_id、链 provenance、claim 完整流程或玩家可读性；对应范围继续待验证，本次未运行。

该具名旧 fixture 仅是正例。负例计划尚待实现：在隔离测试构造中使运行时 Agent 坐标不同于同一 seed-model Agent 坐标，或保持 seed 不变而改有效场景配置后仍错误复用旧坐标；按本地接线原 oracle，该比较必须报告不一致而不能通过。负例用于验证比较/接线，不新增生产 fallback 或正式 claim 模型。无 frag/边界降级沿出生专题原验证，不由本 fixture 代签。保存受控差异、两组坐标和预期/实际 verdict；目前没有声称该负例测试已存在。

<a id="uws-v-artifact"></a>
### 11.2 UWS-V-ARTIFACT：采用分离的离线核验计划

由 artifact / QA owner 在待验候选中使用既有已批准测试 genesis 与尚不存在的临时输出路径（脚本拒绝已存在目录，不默认启用覆盖）。运行脚本 `create --genesis <fixture> --out-dir <temp> --world-scenario <existing-scenario> --merged-manifest-out <temp-manifest>`，再运行同脚本 `validate --world-dir <temp>/world --merged-public-manifest <temp-manifest>`；create 输出的 governed world 位于 `<temp>/world`，sidecar 位于同级 `<temp>/generated-scenario-world`，provenance 位于 `<temp>/world-generation-provenance.json`；validate 接收 world 子目录并从其父目录找 sidecar。具体参数沿脚本 usage，不填造 fixture、seed 或配置。保存 genesis 与 manifest hash、scenario/config/seed provenance、两目录 listing、audit 输出与退出码。断言 governed world 仍由 registry import/audit 路径拥有，generated-scenario-world sidecar 与 provenance 保留；不能以后一目录覆盖前者。缺 approved fixture 或脚本实际依赖时先记录 blocked，本计划不授权连接真实网络或激活世界。本次未运行。

负例计划只在复制的离线输出中单项改变：删除必需 snapshot/journal、将 artifact_kind 改为不符、将 seed 变成非正数、清空 config、改 public_manifest_sha256 或 entry_count。按 [现有 validate 源](../../../scripts/p2p-build-governed-bootstrap-world.sh) 对应校验，必须非零拒绝并保存精确报错，未运行不记为已覆盖。另将 sidecar 目录误作 governed world 输入，或把 sidecar snapshot/journal 覆盖到复制的 governed 目录：核 registry audit 是否拒绝、两来源身份是否失配，分离 oracle 是绝不能把 sidecar 证据当 audited governed 身份。该替换负例尚无已核实测试；若工具未拒绝，也必须作为分离验证失败/实现缺口记录，而非把它解释成已统一或自动选择新承载。保存原/篡改目录 hash、provenance、audit/validate 命令与输出；不得改真实工件。

<a id="uws-v-status"></a>
### 11.3 UWS-V-STATUS：当前 status 来源正负例

源码与 [Cargo bin 配置](../../../crates/oasis7_chain_runtime/Cargo.toml)对应：对固定候选使用 `cargo test -p oasis7_chain_runtime --features node-libp2p --lib <filter>`，filter 分别为 `status_prefers_latest_execution_record_cas_snapshot_over_stale_execution_world_json`、`status_rejects_tampered_authoritative_execution_record_cas_snapshot`、`status_rejects_invalid_latest_execution_record_over_ready_legacy_json`、`status_rejects_newer_execution_record_with_stale_verified_snapshot_height`；原测试使用临时离线材料。先核 discovery 每个命中具名测试，再保存原输出及使用的 record/CAS/height 关系，0 tests、编译失败或环境阻塞不报通过。另用现有 `status_uses_runtime_execution_snapshot_when_simulator_mirror_is_empty` 检查其真实 fallback fixture，不把一个 fallback 覆盖全部 recovery。范围只为来源读取和拒绝旧 ready，不启动链或证明共识。本次未运行。

<a id="uws-v-consumers"></a>
### 11.4 UWS-V-CONSUMERS：正式目标的对账与阻塞记录

这是 R1–R6 的待实施验证计划，尚未核实完整可执行 harness，不伪装成已有测试。QA 分别记录 local/public/runtime-live 的候选、声明 profile、世界/链/genesis、effective seed/config/schema/算法版本、starter 选择及其批准来源。缺正式选择时对应场景标 blocked，不能以 Simulator 或本地默认替代。固定同 Agent、snapshot height/hash 与观察权限后取得 Viewer 原始 location/resource/seed/version 和 provider observation；按 R5 逐字段比较 IDs、附近资源与来源。兼容锚点、pending、provisional 分别核状态和正式动作拒绝，不自动等同 committed。

R3/R4 用正常、有明确 empty-world 状态、缺 manifest、未 committed 与资源/出生缺失场景逐步核 §6 等待/错误，不把注册成功当资源绑定。R6 取得真实生成 result/commit、一次消费 delta、刷新及 replay 前后 remaining，证明同一记录引用和余额未被 generation 复原；没有获准的正式 record 时停止该场景并记录具体缺口，不新增 record/schema。执行范围与权限由实际实现/QA 任务另行确认；本次只建立计划与来源，不执行业务操作。


<a id="uws-v-seed-reference"></a>
### UWS-V-SEED-REFERENCE：R1/AC1/AC5 的来源一致性

输入为同候选同 profile 的 Simulator、runtime snapshot 与 testnet replay/chain 材料，附已批准的 world/chain/genesis/manifest 关系、effective seed/config/schema/算法版本与 chunk coord/触发顺序。方法逐一追原引用再对照实际生成摘要；oracle 是同一有效输入产生一致 chunk/resource summary，且各入口引用同一 canonical seed 或其可验证派生证明，不能只比较最终数量。负例在隔离对照材料中分别换 world_id/genesis、seed、config hash 或 schema/version 中一项，或提供缺失的 seed 引用；必须记录引用/条件不一致，不能给 AC1/AC5 或 testnet readiness 的一致性通过。这里不定义新运行时错误码或替正式 seed 位置作决定。保存三份原材料、链上引用、每字段对比和缺失/不一致 verdict；正式承载未选或材料不足即 blocked，不能用本地 seed 常量填空。

<a id="uws-v-resource-budget"></a>
### UWS-V-RESOURCE-BUDGET：R2 的账本来源与不临时重算

输入为固定候选和同一有效 seed/config/order 的 Simulator model，选真实存在且有 fragment 与所属 chunk total/remaining 的地点，再取同 profile snapshot/consumer 可采量。方法核 fragment profile、fragment/chunk 预算以及 consumer 所用来源；对一个原支持且合法的消费场景分别保存动作前、权威结果后与刷新后账本。oracle 来自原 R2 和 chunked 库存合同：total 为原定量，remaining 取当前账本；同一次实际资源消费的 fragment 与所属 chunk 扣减对应，consumer 可采量不等于重新生成的初始 total。不得将 runtime_agent_chunk_v1 的合成预算冒充 Simulator 自然资源。

负例为非零已消耗后错误把初始生成预算作为当前可采量、缺 fragment/chunk budget 却临时重算、或把另一 chunk 的预算引用到当前 fragment。比较必须报告来源/余额不符，不能通过 R2；对正式可消费状态的处置继续由 R6/原动作合同决定，不在此新增 schema 或失败策略。保存原模型/来源引用、前后两级账本、真实动作结果、刷新 consumer 输出、反例差异与 verdict。实际消费/harness 尚未核实，只有 snapshot 具名 fixture 不足以覆盖上述矩阵，本次未运行。

<a id="uws-v-observation-parity"></a>
### UWS-V-OBSERVATION-PARITY：R5/AC4 同快照逐字段核对

输入固定 candidate/profile、同一 Agent、同一 snapshot height/hash 及同等合法观察权限；分别保存 Viewer 实际 model.locations/terrain/resource summary 和 provider observation。oracle 是同一 location/resource IDs、附近资源摘要与 world seed/version，并保留 compat/provisional 来源标记；隐藏 frag 低层细节不允许替换正式资源身份。方法在同一快照静止窗口采两输出，逐字段及引用对账；异步到达但版本不同先归为不可比，不强行判一致。

负例在隔离 harness 中使一方读取旧 snapshot、不同 location/resource ID、不同 seed/version 或将 compat anchor 标为正式资源来源，每次只改一项。比较必须显式报告对应 mismatch/stale/不可比，不能给 AC4 通过；正式动作是否拒绝另由 R6 和原权限/动作合同核验，不在本计划发明新 API 行为。保存两份完整原输出、snapshot/Agent/权限标识、受控差异、字段对照和 verdict。缺真实两消费者接线则 blocked，不能用同一 DTO 自比或两份模拟输出代签。

### Node reward host observation boundary

Node snapshot / filesystem reward observations are owned by the Chain reward host. Run `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --no-default-features --features node-libp2p,test_tier_required node_points` to execute the snapshot proof-hint/storage-count unit tests and the multi-node points closure regression. These tests retain their existing feature tier; the core package no longer owns the Node host adapter or its network feature.


### Governed validator admission authentication (controlled profile v2)

Refs #4975; Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`.

Run canonical commands directly from the candidate worktree:

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_agent_api governance_authorization -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib governance_auth -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --no-default-features governance_authorization -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --no-default-features governance_validator_admission -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib --no-default-features governance_registry_publication_transaction_regressions -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib --no-default-features --features node-libp2p governance_authorization_tests -- --nocapture
```

The host suite uses actual signed profile-v2 activation and qualified parent verification, exercises all four commands, parent/world/policy/signature/action failures, threshold and atomic nonce behavior, and real NodeRuntime-to-driver ingress preflight. Node scheduling tests use an explicit oracle to test stale/duplicate queue filtering and progress; runtime transition tests use synthetic parent tokens and do not prove qualification. No Wasm modules execute in these selected governance tests; do not treat an unfiltered `--no-default-features` crate run as the full Wasmtime acceptance tier. Run normal required CI for the final candidate.

Legacy unsigned history remains an explicitly historical replay prefix. Signed replay requires the original qualified parent; a missing parent and unsigned suffix after signed governance must fail closed. These tests do not establish multi-parent journal-tail restoration, BFT authorization, key rotation, actual component installation ACK, real fault-domain separation or production deployment readiness.


### Qualified governance history across committed parents

Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`; contract: [qualified history authentication](../../../doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-governance-history-authentication).

Run `env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --lib governance_history` and `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib governance_history`. Runtime fixtures use synthetic qualified-parent tokens to test original-policy/nonces, projection tampering, event cursor rollover, time, capacity, deadline and authenticated cold archive success/corruption without mutating either World. Opaque projection acceptance is compared with the original World-based validator; invalid execution-policy fields must also be rejected by full checkpoint restoration. Host fixtures form cryptographically qualified C0→signed submit→signed approve material and consume it through the production exact-history resolver, restore the same target Snapshot and continue with a signed activate action. A fully re-signed candidate with an unjournaled controller-policy replacement must fail before publication; this is distinct from merely corrupting an artifact hash.

Two additional ignored process regressions use the real authenticated Primary/Replica endpoints with the test custody oracle: a missing intermediate local publication is restored across signed parents, and a native-qualified silent policy replacement is rejected by `FormalLiveRuntime::open_with_native_history`. Build `oasis7_durability_endpoint --release`, set `OASIS7_FORMAL_ENDPOINT_TEST_BIN` to that candidate executable, then run `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib governance_history_ -- --ignored --test-threads=1`. Both exact cases are registered in the formal `runtime-protocol` CI suite. Also run the existing 70-request `protected_endpoint_children_world_long_history_prunes_reopens_and_retains_original` process regression with the same configured operation deadline; inspect all intermediate and final reopen results rather than inferring long-history availability from short fixtures. The existing real signer-handoff process test covers the valid readback path; a separately constructed malicious signed read service for a direct negative handoff test is not covered by these fixtures. Local custody-oracle processes do not prove independent physical fault domains.

host fixture 验证 Runtime 执行和实际历史消费者，尚未覆盖这些新候选的 Node committed-action 入口或 Formal executor。既有 governance authorization ingress suite 继续按影响进入普通 required；重型 Formal history/recovery suites 按完整层策略在 periodic-full 与 candidate-full 执行。该变更不影响 Agent 游戏入口或行为，不触发 Agent 自主试玩。任意 raw-tail World 重建、BFT、组件安装 ACK 及真实独立故障域证据仍由各自合同承接。

<a id="discovery-observation-local-trust-boundary"></a>
## Discovery observations and local bootstrap trust

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3. Contract: [discovery evidence boundary](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-evidence-boundary), including its [current implementation limits](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-peer-record-implementation-boundary).

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib
env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib libp2p_replication_network::peer_selection_tests
```

The net suite includes a three-node loopback TCP flow: B publishes a record without a bootstrap tag, A configures B as a local seed, and C requests B's record through A's real cached-record RR endpoint. C verifies B's original signature and does not inherit A's seed trust. Local regression cases cover forged source/operator/ASN claims, repeated or changed payloads, configuration removal, actual DHT upstream selection, observation bounds and clock regression. Recordless and self-claimed observer sessions remain subject to path/unknown-domain budgets, including mixed signed/recordless sets; source-only degradation does not falsely upgrade health or disconnect every otherwise admissible session.

This proves the existing wire's local trust boundary, not full record freshness, sequence/tombstone persistence, authenticated operator/ASN mappings, rendezvous ID provenance, restart anti-rollback, public-network independence, BFT or deployment readiness. Unknown control domains intentionally remain degraded and share conservative quotas. Normal required CI must cover the final candidate and downstream node status/filtering consumers.


<a id="discovery-rendezvous-trigger-provenance"></a>
### Rendezvous ID-only trigger provenance

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3. Apply the [bounded trigger contract](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-peer-record-implementation-boundary). This is a local diagnostic association, separate from the actual signed-record ingress and trusted source labels.

Use canonical `env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib rendezvous -- --test-threads=1`, followed by the full net suite and applicable downstream node checks. Exercise the production Discovered callback with real signed libp2p registrations, then the DHT result or authenticated RR proxy path. Assert that registration addresses are not installed for dialing and only a correctly targeted, verified live SignedPeerRecord can receive trigger metadata. Verify exact payload binding and unchanged actual ingress, source labels and health trust.

Include two servers announcing the same peer under query deduplication, the 1024-target/8-server bounds (including reactivation of an expired pending context at capacity), fixed 60-second monotonic expiry, target mismatch, invalid signature, query/fallback failure and no-fallback cleanup. A DHT miss must preserve the context for its pending or newly available RR fallback; it is distinct from a successful local-cache result. If another request installs the identical signed payload first, a valid remote result of the triggered query may associate with that exact existing observation without refreshing the record lease. Confirm the existing bounded diagnostic output identifies the peer, exact-payload digest, actual ingress and trigger servers. Both direct candidate-cache hits and Kademlia local-store results with no remote upstream must discard the association. A tombstone must consume the trigger without creating an observation or transferring it to a later live record. A later unrelated query must not inherit a failed or expired trigger. Parallel DHT/RR attempts share a one-shot context: any explicit error or rejection clears it globally, and a later valid response must still install normally without trigger attribution. A normal DHT or RR material-missing result may instead preserve the same bounded fallback context; an outbound transport failure must clear it before any normal proxy retry. Attribution is best-effort diagnostics, not an exhaustive successful-attempt history.

These cases prove bounded process-local trigger attribution only. They do not authenticate an independent operator/control domain, persist provenance across restart, establish trusted absolute time, or prove public-network fault independence. Report actual test results on the candidate PR; this method is not an execution receipt.

<a id="discovery-record-v1-time-policy"></a>
### Discovery record v1 time policy

Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`; [time policy and compatibility](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#当前-v1-时间策略).

Run canonical `env -u RUSTC_WRAPPER cargo test -p oasis7_proto -p oasis7_net --lib` and downstream `oasis7_node` libp2p replication tests. The signed schema/domain remain v1: malformed TTL, overflow and negative publication are rejected structurally; network use enforces expiry, fixed 30-second future allowance and a per-network process-local clock. Existing v1 inputs receive no exemption. Tests cover:

- `peer_record_signed_expiry_is_checked_and_bounded` and `peer_record_time::tests`: TTL/future/exact-expiry boundaries, wall rollback recovery, monotonic expiry and retransmission without deadline extension.
- `peer_record_store::tests`: automatic inbound Kademlia storage and every get/replication iterator validate original signature, matching key, size and deadline; unrelated DHT objects/providers retain behavior; metadata stays bounded during churn.
- `peer_record_lifecycle_tests`: expired records stop cached responses and dedupe, queued record paths fail before dial, failed ingress preserves accepted payload, and expiry keeps established sessions subject to original capability restrictions and actual path/unknown-domain quotas.
- `peer_record_dial_gate::tests`, `swarm_behaviour::dial_gate_tests` and the final-resolution lifecycle cases check live/configured exact addresses against cached Kademlia routes and automatic original DialOpts, keep explicit local caller dials separate, and prevent record dials from populating permanent routing entries. Unknown automatic intermediate routes are deferred until a live record or exact configuration is available. `peer_record_protocol_dial::tests` and the origin-aware outer behaviour test preserve single-attempt DCUtR/AutoNAT/relay protocol-owned original targets in both roles, consume the source ID once, and still reject stale augmented routing alternatives. These protocol operations do not install permanent address grants or renew the parent candidate TTL.
- Existing three-node TCP forwarding and role/recordless-budget regressions remain in the same net suite. DHT result selection and final handoff recheck current time. Three-node TCP also verifies cached expiry and idle health degradation within the one-second refresh tick while authenticated connections survive.

These tests do not prove absolute wall-clock trust, restart/whole-machine rollback protection, persistent sequence/tombstones, aggregate discovery-round budgets, real public-network independence or deployment readiness. Explicit static bootstrap addresses have a separate local configuration lifetime; remote record addresses never inherit it. Signed-record TTL expiration does not revoke governance, validator authority or authenticated sessions. Do not replace signed timestamps with receiver time or extend expiry by the future-skew allowance.

### 轻量工具构建边界

协议 CLI `oasis7_chain_transfer_submit_client` 与 `oasis7_rollback_strict_audit_evidence` 由 `oasis7_protocol_tools` 唯一拥有，生产 normal/build 依赖只有 Client API/Proto、签名和 HTTP 等工具依赖，不引入 Runtime/Wasmtime；真实 Runtime 签名互操作保留为 dev 测试。`oasis7_init_demo` 与场景 integration tests 由已有 `oasis7_simulator` 拥有。三个 binary 的名称与命令行合同保持不变，内部 Cargo 调用使用新 package。其他 Ops 工具继续保留原默认 Wasmtime 与 tier 语义。required/full 同时选择新协议工具包，场景回归使用 Simulator owner。


### Discovery work budget

Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`; [shared discovery policy](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-work-budget).

Run the pinned-client unit suite with canonical `env -u RUSTC_WRAPPER cargo test --locked --manifest-path vendor-libp2p-rendezvous-0.18.1/Cargo.toml --target-dir target --lib -- --test-threads=1`; the target is the existing workspace target, shared with normal network verification. The net CI group runs this standalone suite because dependency unit tests are not selected by the workspace net suite.

Run canonical `env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib` and downstream `env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib libp2p_replication_network`. Use controlled monotonic instants for rolling-window boundaries and actual Swarm scheduling entrypoints for fanout and retry tests:

- Exhaust the combined 32 in-flight slots across automatic discovery work; a new peer/source must create no extra pending work or request traffic. Release only terminal operations; rate tokens remain consumed.
- Fill the same window with production DHT and RR scheduling, then attempt rendezvous register and discover through the real Swarm helpers. Both must be refused without protocol requests or pending markers; switching kind or server must not reset the budget. Cover ConnectionEstablished and periodic RefreshPeerDiscovery callers.
- For rendezvous, successful and failed client terminal events release only the matching kind/peer concurrency slot; synchronous registration failure also releases only concurrency. A disconnect or pending-set cleanup must not release an outstanding operation or permit a duplicate kind/peer start. Verify late or unrelated terminal events cannot decrement another operation, and retries after a true terminal still consume the shared rolling window. Keep ordinary successful registration/discovery coverage.
- Verify the pinned-client patch preserves default auto-refresh, while native assembly explicitly disables it without freezing the client address set. Address changes must later re-register through the shared budget; an ACK issued for an older address snapshot cannot mark the new snapshot current. Register and discover must expose their real request IDs in all four terminal outcomes. Exercise both valid-but-wrong response types and a request message returned as a response: emit the corresponding failure exactly once rather than leaking a slot or panicking. Two independent request-response behaviours may both emit ID 1; app RR and rendezvous receipts must remain separate and release only their own slots.
- Complete 128 operations in a rolling 60-second window, switch sources and reconstruct requests, then verify refusal until the oldest start leaves the window. Moving wall time must not refill the budget.
- Feed oversized, over-count and duplicate cached ID lists; reject beyond 16 KiB or 128 items before fanout, and schedule duplicates once. Cap cumulative DHT provider results and server output at 128 IDs.
- Traverse cache misses and outbound failures across proxies; stop after four attempts for the target chain and retain global consumption. Exercise cooldown saturation and expiry reclamation without exceeding 1024 entries per RR category (3072 total across three maps).
- Retain the real three-node TCP signed-record forwarding/expiry flows and downstream status/filtering tests as positive-path coverage. Budget exhaustion must not itself disconnect authenticated sessions or grant source/role trust.

Observe limits, cumulative starts and bounded rejection reasons in existing traffic metrics. This is a per-process automatic discovery work limit, not wire-byte accounting, a total connection/CPU budget, durable anti-replay, source fairness, physical topology proof or deployment evidence. Local publication/provider announcements, inbound rendezvous server service and registration-list validation, application-driven operations and protocol-internal messages retain their own limits. Outgoing rendezvous client register/discover are included in the shared work budget. New network instances start a new budget; no restart-resistant quota is claimed.


<a id="discovery-resource-budget-v1"></a>
### 发现连接占用与累计子流预算

Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`；主责合同为[发现资源预算](../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-resource-budget-v1)。标准 development node／local-provider 的 `runtime.p2p_resource_budget` 必须显式提供九项限额；测试夹具的额度是固定输入，不是产品默认值。

在同一候选和 Unix 开发环境运行以下 canonical 入口，保留默认 `wasmtime` 和 `node-libp2p` features，使正向 provider 进程测试实际被编译和执行：

```sh
env -u RUSTC_WRAPPER cargo test -p oasis7_net --features libp2p --lib --locked
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --test host_api --locked
```

| 真实入口与固定输入 | 独立观察及判定 |
| --- | --- |
| `host_api::shipping_budget_process`：实际二进制的 `dev node --config` 和 `dev local-provider --config`，同一份显式配置、TCP＋QUIC 监听 | 非法额度在运行时文件副作用前拒绝；正向进程经认证 RR 成功返回可独立解码的原 peer record，核对 peer／world／policy，并观察双向实际字节。provider 使用可执行 WASM，重开原 world 后核对模块授予和原 authority。不得把畸形请求、unsupported 响应或只有 HTTP ready 当正向协议成功。 |
| Net `connection_budget` 的真实 TCP／QUIC／relay、多地址竞速和 pending 测试 | 以 socket、真实 Swarm 终态和独立对端核对 total／pending／unknown inbound／outbound／per-peer；认证拒绝和请求关闭不能提前退款，真实终止后才恢复，重复或迟到事件不释放其他 attempt。 |
| 同一认证连接上的 Kademlia／rendezvous；真实业务 RR／gossip 与 relay 嵌套流 | 下层固定样本与接收端结果对照收、发和总量，巨大响应在完整读取前等待或拒绝；协议切换不另起账本，relay 两个受控边界分别计费。 |
| `wire_byte_window`、vectored I/O 与真实 muxer | 部分／空输入、Pending、可信零进展错误、不确定进展、提交前后取消及并发读写分别核对 `A`、`C`、`U=A+C`、`R`；底层回调期间不持有账本锁，预留不会造成额外额度。 |
| `real_quic_window_waits_sixty_seconds_and_does_not_revive_cancelled_payload`，以及原 deadline 回归 | 实际等待至少 60 秒，自然出窗后独立对端只收到仍有效的新内容；取消内容不复活。另以合成单调时刻参考序列核对最多 61 桶和精确到期，这组边界测试不替代真实等待。 |
| 原配置停止并冷重开 node／provider | 原实例必须先完成合法 RR 且有双向计费；新实例 ID 不同且预算为零，原身份、provider authority 与 world 授予保持。只证明新实例新账，不声称跨重启累计配额。 |

自然窗口测试有真实等待，不能为缩短执行时间而改 origin 或把它替换为预先老化的账本。继续执行上节固定 rendezvous client 和旧工作预算回归，保留 128／32／4 门禁、启动次数不退款及精确协议终态释放。严格 JSON、正式 profile 混用、配置逐层传递和状态投影的对应 Client API／Node／Host 测试由受影响 CI 一并核对。

实际记录须包含 source／tested SHA、执行命令和 features、transport／显式限额、成功／拒绝／恢复及独立对端结果。开发环境测试不证明公网来源独立性、正式 authority lane 隔离、恶意主机回滚防护、provider 的付费模型业务资格或攻击下公平性。

<a id="discovery-record-refresh"></a>
### Discovery record before-expiry refresh

Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`; [refresh policy](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-record-refresh).

Run canonical `env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib` and downstream `env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib libp2p_replication_network`. Check deterministic monotonic schedule boundaries and actual Swarm scheduling/record-exchange paths:

- Before original expiry, refresh becomes due within the fixed margin/jitter range. Different receiver randomness can spread starts; repeated identical signed payloads preserve the original lease, due time and retry lower bound. New accepted signed payloads get a new schedule.
- A connected publisher returns a later freshly signed local record via the existing RR endpoint; the receiver verifies and installs that immutable payload. A disconnected candidate uses the explicit DHT refresh path without bypassing pending or shared work limits.
- Each tick attempts at most four eligible candidates; denied/failed work is delayed by at least five monotonic seconds. Shared budget exhaustion, the existing 60-second RR cooldown and identical old DHT replies must not extend the record lease or create hot retries.
- Refresh failure retains the old valid record/path until actual expiry and preserves independently authenticated sessions. At expiry the existing response/dial/health rules still apply.
- `discovery_query_interval_ms <= 0` disables periodic network refresh while the expiry tick continues. Short TTLs may expire before a successful refresh because tick, cooldown and budget constraints remain authoritative.

This is best-effort process-local refresh, not a guarantee of uninterrupted discovery, fairness under attack, trusted absolute time, persistent sequence/tombstones or restart-resistant anti-replay. Existing three-node immutable forwarding/expiry and downstream status tests remain positive-path coverage. Do not reset jitter or receiver lease on a cache replay, and do not clear a live record merely because its refresh failed.


<a id="traffic-durable-attempt"></a>
### 独立 traffic 持久尝试与历史事务：离线验证

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；合同见[traffic 持久尝试](../../../doc/p2p/node/node-triad-operations-observability.design.md#traffic-durable-attempt)。执行 `bash scripts/p2p-real-env-observability-monitor.test.sh`，其中应包含实际入口 `scripts/p2p-real-env-traffic-durable.test.py`；另保留现有流量归约和新鲜度回归。

- 实际 wrapper 与隔离 HTTP／SSH／接口替身覆盖 pre-probe pending、SIGKILL、同钟、旧 A 对新 B pending／失败、预发布／终态失败、损坏状态及恢复；类型误用／旧无类型外层兼容沿共享 helper 回归。
- 用固定来源的真实旧 wrapper 先生成历史及时间戳 run，再用新入口在同一 root 初始化；核对 seq1 pending 不采用旧成功、旧工件逐字节保留、后续 seq2 可继续，以及混入孤立版本化工件仍拒绝。
- 同一和不同输出 root 指向同一历史，核对 admitted 事务不丢样、不重复批次，保留顺序、失败及回退的新行，裁剪与本 run 快照是同一事务；损坏锁或写入失败不显示旧成功。未接入事务的其他 writer 不据此取得并发保证。
- 只读外部历史回放不创建／写入源目录、锁或文件，不追加／裁剪源；原 capture／producer 时点不刷新。实际 outer→traffic→merger→canonical 保留 incomplete／unknown 与历史身份，损坏协议不回退 legacy。结果不证明角色权威、可信绝对时间、全存储物理回滚或真实环境。

<a id="snapshot-durable-attempt"></a>
### 独立 snapshot 持久尝试：离线验证

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；合同见[snapshot 持久尝试](../../../doc/p2p/node/node-triad-operations-observability.design.md#snapshot-durable-attempt)。执行 `bash scripts/p2p-real-env-triad-snapshot.test.sh` 与 `bash scripts/p2p-real-env-observability-monitor.test.sh`；实际入口回归为 `scripts/p2p-real-env-snapshot-durable.test.py`，应接入普通运维合同选测入口。

- 实际 snapshot 入口通过隔离 HTTP／SSH／服务替身验证 pending 先于探测、SIGKILL、同钟独占与 A 慢于 B pending／失败；不能只测 helper。
- 核对错误输入不写入不采样、显式节点清单／样本计数、失败后恢复和 last-success；预发布／终态故障、锁／锚／状态缺失及摘要损坏均不能返回旧成功。三类 collector 类型不可互用，旧无类型外层状态保持兼容。
- 实际外层读取 versioned snapshot canonical 结果，仅用验证通过的同 run raw status／WASM 样本；损坏协议或悬空链接不触发 legacy fallback。另保留受支持旧 child 的隔离兼容回归。该证据不证明 traffic、角色权威、全存储物理回滚或真实环境恢复。

<a id="host-durable-attempt"></a>
### 独立 host 持久尝试：离线验证

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；合同见[host 持久尝试](../../../doc/p2p/node/node-triad-operations-observability.design.md#host-durable-attempt)。执行 `bash scripts/p2p-real-env-host-monitor.test.sh` 与 `bash scripts/p2p-real-env-observability-monitor.test.sh`；实际入口套件为 `scripts/p2p-real-env-host-durable.test.py`。

- 以实际 host wrapper 和隔离 probe／SSH 驱动正常采集、最新失败／恢复、pending 先于采样、SIGKILL、慢 A／新 B pending 或失败、同钟独占目录与 last-success 保留。
- 预留／工件／状态提交失败不得提前采样或恢复旧成功；共享机制继续覆盖锁替换、序号／锚损坏、摘要与 generation 不匹配。分别核对 host 与 observability 类型不能交叉使用，旧无类型外层状态仍按原边界兼容。
- 实际外层消费新 host canonical 读取结果，保留错误状态、节点诊断和非零退出；legacy child 仅在外层该次独占目录内兼容。验证不证明其他 collector、全存储物理回滚、真实 SSH 或角色就绪，也不刷新历史测量的年龄。

<a id="host-failure-report"></a>
### Host 失败续采与报告：离线验证

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；合同见[失败样本](../../../doc/p2p/node/node-triad-operations-observability.design.md#run-failure-report)。执行 `bash scripts/p2p-real-env-host-monitor.test.sh` 与 `bash scripts/p2p-real-env-observability-monitor.test.sh`。

- 实际 host 入口使用隔离 probe／SSH 替身，分别注入首个、后续、全部节点失败和成功后的失败／恢复；核对所有请求轮次仍执行，失败行准确保留 label、时点、序号与有界原因，JSON／Markdown 生成后非零退出。
- 空响应、部分字段、无效数字及 shell 文本不得成为有效样本，也不得执行响应内容；节点间不串用变量或旧成功值。正常原 helper 输出须兼容，包括空值、空格及转义字符；不增加远端运行时依赖。
- latest 失败保持 unavailable，last-success 独立保留；恢复不擦除本 run 的失败计数。真实 host reducer→merged report 和外层条件发布保留有效其他节点、历史与 snapshot blocked，未知 host schema 不转绿。
- 既有 all-success／缺 storage 指标 fixture 保持兼容。stub 只证明本地调用链，不证明真实 SSH、物理故障域、独立 collector 的持久尝试或目录并发安全。

<a id="observability-consumer-routing"></a>
### 外层三节点参数路由：离线验证

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；合同见[消费者路由](../../../doc/p2p/node/node-triad-operations-observability.design.md#run-routing)。执行 `bash scripts/p2p-real-env-observability-monitor.test.sh`，路由套件为 `scripts/p2p-real-env-observability-routing.test.py`。

- 驱动实际 outer wrapper，逐项及组合覆盖 service、status、target、storage、health/env/public fallback、world 和流量专属选项；验证参数只到声明支持的消费者，WASM 仍只消费本 run 的 snapshot 样本。
- 用实际子 collector／reducer 加隔离底层采样替身验证配置路径可完成报告；严格 argv stub 用于完整路由矩阵，不能代签真实 SSH、服务状态或任意拓扑。
- 缺值、空值、非法数字、未知选项、子工具专属选项与 `--node` 在预留尝试和 collector 前非零拒绝；核对既有状态、目录和历史均未修改。help 不产生尝试；路径含空格／元字符保持独立 argv，外层不执行其内容。
- 继续执行现有 freshness 历史回放及持久尝试回归，核对只读历史、原 as-of 和失败／blocked 语义；支持环境中使用 Bash 3 与当前 Bash 核对空数组行为。

<a id="durable-observability-attempt"></a>
### 外层 observability 持久尝试：离线验证

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；合同见[持久尝试与条件发布](../../../doc/p2p/node/node-triad-operations-observability.design.md#durable-observability-attempt)。执行 `bash scripts/p2p-real-env-observability-monitor.test.sh`，其中包含 `scripts/p2p-real-env-observability-attempt.test.py` 的实际外层入口／发布 helper 回归。

- 正向：正常采集／归约、带 blocked 模块的执行完成、失败后的新尝试，核对 JSON／Markdown、严格递增序号与独立 last-success。
- 并发／恢复：固定墙钟并发不覆盖 run，旧 A 晚于 B 的失败或 pending 完成不更新 latest；B 预发布后 SIGKILL 留下 pending，不能回退旧成功。
- 持久化：分别使预发布、终态工件及状态提交失败，核对采集未提前启动或 canonical read 保持 pending／unknown；损坏或缺失初始化锚／序号状态禁止自动重置；锁丢失、替换、inode 复用或持久随机标识损坏时阻止采集。
- 消费：验证读取核对 generation、工件摘要、状态可读性；不匹配时非零诊断，不改变历史文件，也不把缓存物化工件当实时状态。
- 这些测试仅使用隔离本地 stub，不证明真实 SSH、授权撤销、BFT／P/R、物理故障域或单独 collector 的持久尝试。

<a id="traffic-freshness-v2"></a>
### Traffic observation freshness

Trace refs: `O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b`; [schema 2 policy](../../../doc/p2p/node/node-triad-operations-observability.design.md#traffic-freshness-v2).

Use offline collector stubs and the actual reducer→merged JSON/Markdown chain. Run `bash scripts/traffic-monitor-summary.test.sh` and `bash scripts/p2p-real-env-observability-monitor.test.sh`, plus their freshness regression suites. Validate shell syntax and Python modules. Do not contact historic fleet hosts for these tests.

- Success at 00:00/00:10 followed by failures through 00:30 must retain latest failure and last success separately. At as-of 00:30 the last ten minutes are not a full current window.
- Cover all failures, recovery, normal dense full-window samples, two sparse endpoints, exact age/gap boundaries, reset or changed identity, malformed/future/regressing timestamps and per-node capture completion times.
- A new successful capture of an old producer observation keeps producer age stale; missing observation stays unknown. Neither report generation nor HTTP 200 establishes freshness.
- Explicit historical as-of excludes later successes/failures and leaves original history bytes unchanged. All nodes share the selected boundary; replay is distinct from a current-health claim. Summary-only without an explicit cutoff must also preserve history bytes and mtime while keeping current-time freshness checks.
- Merge an actually generated child report, then advance report as-of. Formerly fresh metadata must become stale/uncovered; old or unknown schema warns. JSON and Markdown retain matching attempt, age, source-window and bounded error semantics.
- Route age/gap/as-of options only to supported consumers; invalid values fail before collection. Stub invalid/empty JSON must produce failed history rather than vanish. Outer historical replay requires `--traffic-history-path` with as-of and invokes the real traffic child in summary-only mode without mutating history; host/snapshot remain current evidence.

These checks do not prove persistent pending/terminal publication, SIGKILL safety of the whole collector lifecycle, profile readiness, producer authorization, real SSH/physical fault domains, trusted clocks or deployment health. Existing single-node attachment/report consumers outside the merged triad path retain their documented scope.

### 单节点正式 profile 状态报告

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；范围见[单节点正式 CLI 报告](../../../doc/p2p/node/node-triad-operations-observability.design.md#single-node-profile-report)。

- `bash scripts/oasis7-node-observability-report.test.sh` 与 `python3 scripts/p2p-profile-readiness.test.py`：实际 file report 入口、旧 HTTP 兼容、未知 schema/profile、实例绑定、停止／阻断、缺字段、陈旧／未来观察和恢复；保留 JSON／Markdown 只读角色边界，不输出私有配置和原始 error。现有 observability 合同入口同时执行它。
- `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib schema2_process_hook_reports_verified_public_readiness_and_stops_cleanly`：真实 observer supervisor 与 `process::status` 正／停止状态进入报告；这不是外部 CLI 进程证明。
- `env -u RUSTC_WRAPPER cargo build -p oasis7_chain_runtime --bin oasis7_chain_runtime`，再设置 `OASIS7_FORMAL_NODE_TEST_BIN` 为该候选二进制，执行 `env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib formal_full_state_sync_cli_ -- --ignored --test-threads=1`：shipping CLI 的首次就绪、重启、停止及 epoch 演进后的旧 grant 拒绝状态送入实际报告。普通不带 ignored 的 cargo test 不代签此项；沿现有 formal process CI suite 执行。

文件报告保留 producer 检查时点与年龄，报告生成不更新源观察。ready 只说明受支持本地实例的最近只读观察，source freshness 不等于已证明最新 head；不证明 triad HTTP profile 接线、生产部署、撤销协议正确性、BFT 或 P/R。本次不改变 Agent 游戏接入、行为或玩法，不触发 Agent 自主试玩。

### 独立本地 profile 采集

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；范围见[本地采集合同](../../../doc/p2p/node/node-triad-operations-observability.design.md#local-profile-collection)。

使用绝对可执行文件和已有 data-dir：`bash scripts/p2p-node-profile-monitor.sh --node-binary /absolute/oasis7_chain_runtime --data-dir /absolute/node-data --out-dir /absolute/profile-output`；再用 `python3 scripts/p2p-real-env-observability-attempt.py read --collector node-profile --out-dir /absolute/profile-output --format markdown` 读取当前状态。采集成功不表示角色 ready；读取时重新计算采集年龄与 producer 观察年龄，静态 latest 文件仅是历史发布物。

- `python3 scripts/p2p-node-profile-monitor.test.py`：实际入口的参数和路径隔离、固定 argv、pending、成功／失败／恢复、超时、输出限额、继承 stdout 的子进程、SIGTERM／SIGKILL、并发条件发布与协议损坏；fixture executable 不代替 shipping CLI。
- `python3 scripts/p2p-node-profile-publication.test.py`：真实 publisher 的读取时过期、时钟回退、读取不写盘、失败保留显式历史身份、scope 变化清除当前成功引用、跨 collector 与 artifact 损坏拒绝。两组测试由现有 observability 合同入口执行。
- 上述 `formal_full_state_sync_cli_` ignored shipping CLI 测试同时覆盖直接采集与 canonical read：首次 ready、旧 epoch grant 拒绝、重开和停止；collector 本身只调用 status，不启停节点。

不执行生产采集或远程节点操作；不扩大为 triad profile 接线、BFT 或 P/R 验收。


<a id="full-sync-v2-issuer-management"></a>
### V2 full-sync issuer management ledger

Trace refs: `O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8`; contract: [V2 issuer ledger and witness decision](../../../doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-issuer-ledger).

Run focused protocol and durable-store tests separately, using the existing shared Cargo cache:

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_formal_protocol full_sync_issuer
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs full_sync_issuer
```

Verify signed management Issue/Replace/Revoke against the original externally pinned activation trust, complete world/epoch scope, authorized manager and current dedicated issuer generation. Negative cases include changed initial trust pins, wrong root/role/subject, reused request identity or manager nonce with different content, stale full head, wrong target epoch, absent-target tombstones, counter/byte limits, conflicting same-generation policy and reuse of any historical issuer key. An explicit root-authorized policy transition must advance the same ledger atomically; witness-key replacement is unsupported and must fail.

Use a separately keyed simulated witness for deterministic CAS and crash tests. Exercise actual private filesystem ownership/modes, symlink refusal and competing store locks; persist the original pending request before witness CAS. Inject loss before/after CAS and before the local decision fsync, reopen from both current and old valid local backups, and retry the exact original command. Require the original committed entry/receipt, with no extra mutation or automatically rebased command. Reject corrupted/forked/future local logs, missing or truncated witness history, stale nonce observations and an unavailable witness. A valid earlier local prefix may recover only from the complete history closing to a fresh witness head and the external minimum anchor.

Validate the initial hard limit with a fully signed 1,024-record history containing distinct targets; time construction and complete replay separately, then require the 1,025th prepare/reconstruction to fail without mutation. The 64 MiB encoded-log bound remains independent. This capacity check is not a sustained-throughput or production-witness latency claim.

These tests establish the management protocol and local persistence behavior under a simulated independent witness. They do not prove production nonrollback storage, physical isolation, provider lease freshness, per-chunk enforcement, effective revocation or V1 migration. Until those consumers and their acceptance evidence exist, successful Revoke means a committed management decision and cannot be reported as stopped private reads. No production witness adapter means no production issuance capability.

<a id="full-sync-v3-provider-lease-log"></a>
### V3 full-sync provider lease event log

Trace refs: `O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8`; contract: [unified provider lease log and migration](../../../doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-provider-lease-log-v3). This defines the method; actual source, commands, fault points and results belong in the implementation PR.

Run the protocol and durable-store `full_sync_issuer` test groups above, retaining V2 signature/history fixtures. Use a single separately keyed witness double whose V2 and V3 APIs share one atomic state. Interleave V2 commit against migration in both orders: only the first matching CAS may succeed. After migration reject every V2 observe, current-history and commit call, including a stale observation and a client with no local cache. Replay the complete original V2 prefix, unique migration and V3 suffix; reject missing/forked/truncated prefixes, changed root or manager signatures, mismatched scope/head/policy and repeated migration.

Verify daily V3 open requires the external V3 minimum. Lose the migration CAS reply before that pin, then use only the explicit non-serving resume API with the original request and V2 minimum to recover the exact original receipt/anchor. Before a V3 pin, resume must expose no management or lease issuance capability. Replay a valid V2-mode and V3-mode resume response against a newly signed query nonce; both must fail before changing local history or pending intent. Exercise the opposite race too: a V2 commit makes the pending migration stale; only a fresh complete V2 chain proving non-commit permits persistent head-conflict termination. A new migration must carry a new manager approval rather than silently rebasing the old event. Include a V2 signer/manager policy rotation winning before resume, both while still in V2 mode and after another V3 migration. Authenticate the complete current history while validating the original approval at its historical prefix; the old configuration must still fail ordinary serving restore.

Issue a lease for an allowed provider, exact node/role generation, incarnation and unique lease ID. Assert root policy, provider proof, delegated signer, witness nonce/time profile and conservative time bounds; reject an unknown provider, reused identity with changed body, stale authorization/head, invalid interval, excessive uncertainty and expiry beyond the grant or policy bound. Use actual shared-witness interleaving: revoke first prevents the old lease; lease first remains present after revoke. Overlapping leases and old incarnations must all remain enumerable with exact signed bytes and expiry.

Inject CAS reply loss and local fsync failure, reopen the actual private directory and retry the same original request; require identical lease/receipt bytes with no extra revision. Also fail CAS before commit while retaining the same head, advance the witness clock, then reopen and assert that the second CAS receives the exact original signed observation and candidate without extending expiry. Reconcile both committed and provably uncommitted V2 pending intents across migration, retaining ambiguous or corrupt intents. Reject unavailable witness, malformed receipt, missing current history and local future/forked state without repairing from an unauthenticated digest. Verify old V2 code rejects the schema-3 local ledger before a write.

Count legacy entries, migration and V3 events together against 1,024 entries and 64 MiB; exhaustion must leave pending, witness anchor and ledger unchanged before CAS. Also retain many distinct active targets so the accumulated registry exceeds a single-event byte bound, and verify a valid lease still commits under the full-state bound. This is a lifetime hard stop, including renewals, not a sustained-service claim. Provider read gates, protected clock deployment, nonrollback witness storage, cross-process/VM fencing, effective revocation and physical fault-domain acceptance remain separate. A simulated signed time assertion demonstrates protocol validation only; it cannot establish the asserted clock profile in production.

<a id="full-sync-v3-provider-read-gate"></a>
### V3 full-sync provider read gate

Trace refs: `O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8`; contract: [V3 provider full-sync 逐请求读取门禁](../../../doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-provider-read-gate-v3). Run the real loopback HTTP consumer tests with the shared target, serially:

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_client_api --lib grant_identity_signature_check_is_clock_independent
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib read_gate_tests:: -- --test-threads=1
```

The injected test evidence source returns raw signed query, observation and complete V2/V3 history. Require a valid manifest and two independently authorized object chunks; after a same-epoch V3 revoke, the following chunk must fail. Also require a grant with a bad issuer signature to fail before the evidence-source fetch counter changes, while a valid grant reaches both the pre-material and final-response gates. Cover exact provider signer/trust/World attachment, fresh query nonce, unavailable evidence, missing committed lease, wrong target generation and incarnation, grant/lease not-before, and `latest + elapsed` expiry bounds. Test gate installation with lower, equal-exact, equal-conflicting and higher-prefix histories; conflicts poison the gate and a lower history must not replace the installed state.

Ordinary `start()` preserves `legacy_v1` and supplies no V3 dependencies. `v3_required` may start only through the explicit host dependency-injection path with root-signed policies, external V3 minimum and a real issuer/witness source; provider signer and a fresh process incarnation are derived by the host. No production source or protected-clock adapter is currently supplied, so ordinary V3-required startup must fail before listener binding. Test fakes establish route-consumer behavior only. They do not establish production witness durability, protected-clock trust across process/VM rollback, cross-fault-domain fencing, or effective revocation outside the next manifest/chunk authorization.

The actual `FormalLiveRuntime::open`/`start` missing-adapter guard is an ignored process regression in the existing `runtime-protocol` suite. Build the loopback endpoint fixture and run it explicitly:

```bash
env -u RUSTC_WRAPPER cargo build -p oasis7_chain_runtime --bin oasis7_durability_endpoint --release
OASIS7_FORMAL_ENDPOINT_TEST_BIN="$PWD/target/release/oasis7_durability_endpoint" env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib protected_endpoint_children_v3_required_runtime_start_rejects_missing_gate_before_listener -- --ignored --test-threads=1
```

<a id="discovery-tombstone-v2"></a>
### V2 发现撤销与恢复验证

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；合同见[持久撤销与显式恢复](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-tombstone-v2)。同一候选、正常原件和实际 host→node→net 路径执行：显式 durable 发布 live 1，使用 `--withdraw-peer-record-publication` 撤销为无路径 tombstone 2；分别在持久预留和签名后重启，原序号不得被签成 live。普通启动、周期和地址刷新不得恢复；只有 `--resume-peer-record-publication` 发布更高 live 3。两个 flag 互斥且不落入持久配置，initializer 与 formal/guarded 组合明确拒绝。撤销／恢复要求 durable publisher 与已初始化的持久 receiver；legacy 接收策略对 tombstone 必须明确拒绝，不能接纳后继续从旧 live 缓存返回候选。

接收端接纳 tombstone 后重开同一原件，拒绝旧 live 与 V1，撤掉候选、发现来源和排队拨号；当前有效撤销原件可以经实际 DHT／RR 返回原签名，不进入健康／ID／拨号集合。验证更高 live 恢复、同序冲突隔离不自动解除、无路径约束、过期拒绝转发、V1 固定向量和未知状态拒绝。保留已认证会话的限制性元数据，独立静态地址不随发现撤销删除。

采用 proto／net／host／node 受影响回归并扩大到必要全量；记录实际命令、source／tested tree、配置、原签名序号和状态、退出码以及故障点。至少一条真实 TCP／DHT 撤销传播及显式恢复正例贯穿消费者。Quorum::One 不作为全网撤销确认；未支持新 state 的旧节点不获得新保证，可信时间、跨故障域与恶意磁盘回滚仍需独立证据。

<a id="discovery-receiver-floor-v2"></a>
### V2 接收端持久水位验证

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；合同见[接收水位](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-receiver-floor-v2)。在同一 source/tested tree 使用独立接收策略和私有原目录，记录真实 host→node→net 配置、进程重启和签名 payload。本段定义验收方法；实际受测版本、命令与退出码记录于实现 PR。

同候选 canonical 本地命令（共享 target，串行执行；环境参数与受测 source 在 PR 记录）：

```bash
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --lib receiver_ -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib libp2p_replication -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_client_api --lib peer_record_reception -- --test-threads=1
```

标准 development host 使用独立 `--p2p-peer-record-reception legacy_compatible|v2_durable_compatible|v2_only`。持久模式先在相同稳定身份、world/network 与 runtime root 下执行 `--initialize-peer-record-receiver`，初始化完成即退出；日常启动移除 initializer flag，保持同一 reception mode。默认 legacy 与本机 `--p2p-peer-record-publication` 相互独立，已有 receiver 目录不能通过改回 legacy 忽略。普通启动使用 `v2_only` 接收时还需显式配置 `--p2p-peer-record-publication v2_durable` 并按其独立流程先初始化 publisher；缺少 durable publisher 的严格接收组合应在起网络前拒绝。receiver initializer 仍只创建接收原件，不顺带初始化发布器。策略转换与清空原目录不作为本次支持的迁移方法。

验证 V1 兼容 peer 首次接受 V2 后跨重启拒绝 V1／低序号；严格 V2 拒绝所有 V1；相同序号摘要幂等且不延寿；同序号不同的有效签名持久隔离，旧缓存响应、DHT 转发、健康来源和排队发现拨号不再使用该候选，既有会话与独立静态配置按原合同保留。分别验证尚未释放的 queued dial 在新 gate 下拒绝，以及已在旧有效 gate 下启动的 transport 操作若随后完成，不恢复旧记录、来源或永久候选资格；后者仍受原 transport 超时和独立角色／资源策略约束。至少一条真实 TCP 或 DHT 正向升序安装和一条重启重放负例必须贯穿实际消费者。

故障覆盖显式初始化中断、正常打开原件、缺失／损坏／未来 schema／不明临时文件、绑定不符、重复实例、禁用或换策略降级、文件或父目录同步失败；失败不发布新的内存水位或候选；原子替换后的目录同步失败按提交不确定处理，不回写旧状态，重开验证实际原件并保持防回退。容量上限拒绝新身份但允许原未隔离 peer 正向更新，不用无可信时间的 GC 腾空间。分别记录普通文件持久性与未覆盖的恶意整机回滚、可信时间和物理故障域边界；tombstone 另按[撤销扩展](#discovery-tombstone-v2)验证。

<a id="discovery-record-admission-budget"></a>
### Discovery peer-record admission budget

Run focused admission/store checks and then the affected net suite serially:

```sh
env -u RUSTC_WRAPPER cargo test -p oasis7_net peer_record_admission -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_net peer_record_store -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib -- --test-threads=1
```

Verify the [process-local admission contract](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-record-admission-budget) with deterministic monotonic time and real RR/DHT entry paths. A valid response/request/store attempt charges the same shared rolling window before decoding or signature verification; invalid payloads retain their charge. The candidate installation path must consume the admission permit. Verify per-source isolation, source churn and tracking capacity, global sharing across protocol/provider changes, exact expiry, no refund on failed crypto, and positive admission after the window expires.

Exercise a real 128-ID discovery list followed by its record responses without accidental double charging. Reject a throttled update without replacing prior cache/store/observations or exposing a new dial/forward candidate. Include ordinary DHT `get_peer_record` results, peer-record `RecordStore::put`, special RR requests and RR record responses; use authenticated transport/Kad source metadata, never the claimed record identity. Report unknown-source DHT store as global-only. Record commands, candidate and actual accepted/rejected work; this does not prove cross-restart quotas, total Kad throughput or durable sequence floors.

<a id="discovery-durable-publication-v2"></a>
### Discovery V2 durable local publication

This scope verifies the [V2 local publication contract](../../../doc/p2p/network/mainnet-private-reachability-architecture.design.md#discovery-durable-publication-v2), not receiver anti-replay or protected hardware storage. Use one source/config candidate across proto, net, node and chain-runtime. Keep V1 wire/signature vectors fixed; exercise explicit legacy and durable V2 configurations separately.

Run the matching canonical filters serially (development feedback may use `scripts/cargo-dev.sh`):

```sh
env -u RUSTC_WRAPPER cargo test -p oasis7_proto --lib -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_net peer_record -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_net durable_peer_record_publisher -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_client_api peer_record_publication -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime peer_record_publication -- --test-threads=1
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime old_typed_node_config -- --test-threads=1
```

Verify that reserve-before-sign and signed-before-send persistence survives interruption without signing different payloads at one sequence. First initialize V2 explicitly with `--initialize-peer-record-publication` and the same `v2_durable` mode/identity/runtime root used for the later ordinary start. Verify missing state never triggers initialization during ordinary startup. Cover missing entire directory, missing state and marker together, repeated explicit initialization, reopen with the same identity/runtime root, interrupted reserve and signed-result persistence, concurrent owner refusal, corrupt/private/symlink/binding failures, overflow, expired pending payloads and a later valid publication. Unsupported private-storage platforms must reject V2 explicitly while preserving legacy V1. Cover interruption after intent, lock, state and Ready persistence: ordinary open refuses unfinished initialization, explicit same-binding initialization resumes only unissued state0, and wrong binding, corrupt intent, old Ready with missing state or issued state never reset. Manifest defaults must produce the same initialization and ordinary-start binding. Check every local refresh/address-change path and confirm remote cache forwarding preserves original signed bytes. Exercise repeated real handler/TCP local-record queries: V2 sequence and durable state bytes remain unchanged, missing/expired/current-address-mismatched records return unavailable, and a subsequent local refresh still advances the sequence. Unknown versions/states, zero sequence and sequence/state/payload tampering must reject without V1 fallback; a legacy V1 verifier must reject V2 signatures. Record actual commands, source, exit codes and the boundaries of any simulated crash or loopback network test in the PR.

Ordinary local files do not prove resistance to restoring both key and publisher state from an old machine snapshot. Receiver floors and conflict quarantine use the [receiver contract](#discovery-receiver-floor-v2); tombstone retention and explicit recovery use the [withdrawal contract](#discovery-tombstone-v2). Trusted time and independent fault-domain evidence remain outstanding under #4975.
