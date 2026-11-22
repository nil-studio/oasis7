# oasis7 主链 Token 创世分配与早期贡献奖励设计

- 对应需求文档: `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

## 1. 设计定位
定义主链 Token 创世分配、创世绝对发行量、控制边界、低流通门禁与早期贡献奖励的统一设计，让 Token 发行从一开始就是“可审计的战略配置”，而不是“事后补解释的营销行为”。

## 2. 设计结构
- 创世分配层：冻结 `10000 bps` 分配表、bucket 命名、控制主体与锁仓方式。
- 绝对总量层：冻结当前 `main_token_config.initial_supply = 10,000,000,000 OC`，把比例表落成可复核的绝对金额。
- 控制边界层：区分项目战略控制、协议奖励池、单人直接受益和外部流通。
- 奖励执行层：把 early contributor reward 约束为 evidence-based 审核与受控发放。
- 审计门禁层：对比例求和、个人上限、低流通和禁语边界做 QA/治理复核。

## 3. 关键接口 / 入口
- `Action::InitializeMainTokenGenesis`
- `Action::ClaimMainTokenVesting`
- `Action::DistributeMainTokenTreasury`
- `WorldState.main_token_genesis_buckets`
- `WorldState.main_token_treasury_balances`

## 3.1 TIGR-1 创世参数表（草案）
- 实现约束：
  - `InitializeMainTokenGenesis` 当前只会把 bucket 分配写入 recipient account 的 `vested_balance`，不会直接初始化 `main_token_treasury_balances`。
  - 因此本表中的 `protocol:*` recipient 全部是 custody account 名称草案，不等于 runtime treasury bucket 名称。
  - 当前暂按 `genesis_epoch=0`、`1 epoch ~= 1 day` 进行锁仓换算；若最终链上 epoch 节奏不同，需保持相同自然时间重新换算。

| bucket_id | ratio_bps | recipient | controller | start_epoch | cliff_epochs | linear_unlock_epochs | recommended_claim_cadence | 说明 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `team_long_term_vesting` | `2000` | `protocol:team-core-vesting` | team multisig | `0` | `365` | `1095` | quarterly | 核心团队主锁仓盘 |
| `early_contributor_reward_reserve` | `1500` | `protocol:early-contributor-reward` | reward multisig + producer approval | `0` | `0` | `3650` | batch-by-batch only | 早期贡献奖励储备，控制首年最大可 claim 容量 |
| `node_service_genesis_custody` | `2000` | `protocol:node-service-custody` | protocol governance / node committee | `0` | `180` | `1825` | governance batch | 创世节点服务储备，和 post-genesis `node_service_reward_pool` 区分 |
| `staking_genesis_custody` | `1500` | `protocol:staking-custody` | protocol governance | `0` | `180` | `1825` | governance batch | 创世质押储备，和 post-genesis `staking_reward_pool` 区分 |
| `ecosystem_governance_reserve` | `1500` | `protocol:ecosystem-governance` | ecosystem governance multisig | `0` | `90` | `1460` | quarterly | grant / ecosystem 计划储备 |
| `security_reserve_emergency` | `1000` | `protocol:security-council-reserve` | security council multisig | `0` | `0` | `0` | emergency only | 安全事故与协议防御储备 |
| `foundation_ops_reserve` | `500` | `protocol:foundation-ops` | ops multisig | `0` | `90` | `730` | monthly or quarterly | 基础设施与运营成本储备 |

## 3.2 控制边界汇总
- 项目战略控制：`team_long_term_vesting + early_contributor_reward_reserve + security_reserve_emergency + foundation_ops_reserve = 5000 bps`
- 协议长期储备：`node_service_genesis_custody + staking_genesis_custody = 3500 bps`
- 生态/治理储备：`ecosystem_governance_reserve = 1500 bps`
- 创始人个人直持：不单列独立大 bucket；如需个人受益，必须内嵌在 `team_long_term_vesting` 受益人表内并继续受 `500~1000 bps` 目标区间与 `1500 bps` 硬上限约束。

## 3.2A 创世绝对总量冻结（TIGR-9）
- 当前冻结值：`main_token_config.initial_supply = 10,000,000,000 OC`
- 对应首年非团队外部释放边界：
  - 目标：`100,000,000~200,000,000 OC`
  - 硬上限：`500,000,000 OC`
- 对应 bucket 绝对分配额：
| bucket_id | ratio_bps | allocated_amount_at_10b |
| --- | --- | --- |
| `team_long_term_vesting` | `2000` | `2,000,000,000 OC` |
| `early_contributor_reward_reserve` | `1500` | `1,500,000,000 OC` |
| `node_service_genesis_custody` | `2000` | `2,000,000,000 OC` |
| `staking_genesis_custody` | `1500` | `1,500,000,000 OC` |
| `ecosystem_governance_reserve` | `1500` | `1,500,000,000 OC` |
| `security_reserve_emergency` | `1000` | `1,000,000,000 OC` |
| `foundation_ops_reserve` | `500` | `500,000,000 OC` |
- 该口径下 `allocated_amount` 全为整数，runtime rounding remainder 为 `0`；执行清单继续沿用 runtime 真值算法，但当前冻结值不会触发补差额分配。

## 3.3 TIGR-4 制作人执行路径决策
- 当前 limited preview 期间，`early_contributor_reward_reserve` 保持为独立的 reward multisig / producer approval 执行路径，不并入 `ecosystem_pool`。
- 原因-1：`InitializeMainTokenGenesis` 当前只把分配写入 recipient account `vested_balance`，创世时并不会直接生成 treasury bucket 余额。
- 原因-2：limited preview 需要更强的低流通与低承诺纪律，独立储备更容易把“贡献奖励”与“生态 grant”分开管理。
- 原因-3：`TIGR-3` 已把奖励口径固定为 contribution-based review，如果现在并入 `ecosystem_pool`，会放大对外对 grant / reward / airdrop 的语义混淆。
- 重审条件：至少完成 1~2 轮真实贡献奖励台账、审批记录与复盘，再由新专题决定是否迁移到 fully on-chain 的治理分发路径。

## 3.4 TIGR-5 正式执行清单
- 目标：把 `TIGR-1` 的逻辑草案收成 pre-mint freeze sheet，明确每个 bucket 的 `recipient_slot_id / controller_slot_id / signer_policy / runtime_target / freeze_status`。
- runtime 真值：
  - `Action::InitializeMainTokenGenesis` 输入的是 `MainTokenGenesisAllocationPlan`。
  - runtime 会先按 `floor(initial_supply * ratio_bps / 10000)` 计算 `allocated_amount`，再将 remainder 按 `ratio_bps` 降序、`bucket_id` 升序逐个补 1。
  - 生成的 `MainTokenGenesisAllocationBucketState` 会写入 `main_token_genesis_buckets`，并把 recipient 对应账户的 `vested_balance` 初始化为聚合后的已分配金额。
- 当前冻结值下的执行含义：
  - `initial_supply = 10,000,000,000 OC`
  - 由于 7 个 bucket 的 `ratio_bps` 在该口径下都能整除，当前 `remainder = 0`。
- 冻结原则：
  - 逻辑参数、slot registry、签名规则和 rounding 规则现在冻结。
  - 创世绝对发行量 `10,000,000,000 OC` 与 7 个 bucket 的绝对分配额现在冻结。
  - 真实链上 `recipient_account_id` 与 multisig 地址允许后补，但在补齐前 freeze status 只能是 `ready_pending_address_binding`。
  - 最终 mint 前仍必须以 QA checklist 输出 `pass`，不能把 formal sheet 当作最终放行。

## 4. 约束与边界
- 创世分配总和必须为 `10000 bps`。
- 项目战略控制目标为 `5000 bps`，单人直接受益硬上限为 `1500 bps`。
- 创世液态流通硬上限为 `500 bps`。
- 早期奖励只能按贡献证据发放，不能用 `play-to-earn` 叙事替代。
- `TIGR-1` 参数表所有 bucket 的 `genesis_liquid` 默认都为 `0`；若后续要形成 liquid，必须通过 claim 动作显式发生并留下审计记录。

## 5. 设计演进计划
- 先冻结比例和控制边界。
- 再冻结当前创世绝对发行量 `10,000,000,000 OC` 与 7 个 bucket 绝对分配额。
- 再落实具体 bucket/account/vesting 参数表与审计 checklist。
- 再输出 slot-based 正式执行清单，固定 runtime 落点与 rounding 规则。
- limited preview 期间先按独立 reward reserve 多签执行。
- 后续先绑定真实地址并跑 QA 最终审计，再决定是否进入 mint 准备。
- 更后续根据真实奖励轮次与治理成熟度，再决定 early contributor reserve 的长期执行载体。


## 6. 本次规范化范围、上下游与状态

本次仅规范化既有经济合同的身份、需求接收和验证计划；原设计正文、参数、接口、签名策略、权限、计划和实现声明原样保留。产品结果由世界规则根定位，专业 PRD 继续拥有规则/数值；P2P/runtime、治理维护者与 QA 分别负责技术、治理和验收，涉及贡献奖励的资格/审批仍归 README reward governance。这里的设计关系和 planned 方法不启用资产、不授权交易，也不是本轮业务测试或生产证据。

- 当前基线：2026-10-15，固定文档提交 `cde57786e711b03a9c9fa2a5174aba357e8350d5`；本次只核对文档关系，当前实现与完整验收未核实。
- 目标：原独立经济结果可从产品条款追至专业规则、有限设计和真实验证入口。
- 差距：下列全部业务场景尚未在本次候选运行；已有源码、模板、局部夹具或历史结果不外推完整实现/生产安全。
- 裁剪：不改变技术合同、协议/ABI、算法、权限或状态模型，仅追加有限承接和 planned 验证；原文未展开的技术细节仍由专业原条款与后续实现评审承担，不填空N/A或宣称十二段完整设计。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation) | 按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 | [冻结分配可核算到各bucket与recipient](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-allocation) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不变更总量/比例/账户，不执行InitializeMainTokenGenesis |
| Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control) | 项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 | [控制主体与个人受益及释放边界分别可审计](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-control) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 不改控制人/vesting/释放额度，不把协议储备当团队库存 |
| Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze) | slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 | [逻辑冻结不等于真实mint-ready](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-freeze) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易 |
| Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward) | 独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 | [独立reserve只消费最终复核的批准记录](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-reward) | 原专业P2P/runtime与治理owner、QA；涉及奖励资格由README治理权威处理 | 本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次 |

## 7. 既有结果的有限设计承接

<a id="des-economic-genesis-allocation"></a>
### 冻结分配可核算到各bucket与recipient

Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation) / [产品唯一声明](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-genesis-allocation)。

按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不变更总量/比例/账户，不执行InitializeMainTokenGenesis。

<a id="des-economic-genesis-control"></a>
### 控制主体与个人受益及释放边界分别可审计

Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control) / [产品唯一声明](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-genesis-control)。

项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：不改控制人/vesting/释放额度，不把协议储备当团队库存。

<a id="des-economic-genesis-freeze"></a>
### 逻辑冻结不等于真实mint-ready

Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze) / [产品唯一声明](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-genesis-freeze)。

slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易。

<a id="des-economic-genesis-reward"></a>
### 独立reserve只消费最终复核的批准记录

Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward) / [产品唯一声明](../../product/world-rules-core-gameplay/node-economy-contract-reception.prd.md#req-wr-economic-genesis-reward)。

独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 本节接受上述已采用结果；具体字段/公式/参数和执行前提仍按配对PRD第4章，不改变原第2至5章。失败/恢复仍按原专业规则拒绝、保留状态或显式降级，不新增自动修复、重发资产或外部许可。未覆盖边界：本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次。

## 8. 验证设计

### 11.1 验证映射表

所有方法沿用原专业适用场景与层级，固定同一候选、配置、输入和环境后逐项保留实际结果；当前未运行。文档结构检查不能代替业务测试。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation) | [冻结分配可核算到各bucket与recipient](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-allocation) | 按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-genesis-allocation)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 固定参数版本、七桶金额与rounding对账，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不变更总量/比例/账户，不执行InitializeMainTokenGenesis；结构检查不证明业务验收 |
| Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control) | [控制主体与个人受益及释放边界分别可审计](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-control) | 项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-genesis-control)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 真实控制映射、受益拆分、释放计划与阻断依据，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 不改控制人/vesting/释放额度，不把协议储备当团队库存；结构检查不证明业务验收 |
| Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze) | [逻辑冻结不等于真实mint-ready](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-freeze) | slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-genesis-freeze)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | 绑定证据、完整pre-mint清单与该次QA verdict，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易；结构检查不证明业务验收 |
| Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；[专业接受](mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward) | [独立reserve只消费最终复核的批准记录](mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-reward) | 独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 | planned；[该独立结果的QA输入/方法/oracle/失败边界](../../../testing-manual.md#qa-economic-genesis-reward)；固定同一候选、配置与受控fixture，沿原专业适用层级，当前未运行；局部定义不代完整矩阵。 | canonical批准行、最终金额复核、审批及distribution引用，按[主手册证据规范](../../../testing-manual.md#证据规范)记录实际版本/环境/结果 | 本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次；结构检查不证明业务验收 |

## 9. 既有决策、风险与未决

原第4至5章及配对PRD的已采用决定、技术风险和计划完整保留。具体参数变化、经济安全结论、地址/控制绑定、签名治理、真实批准和执行许可不得由身份登记消除；有实质冲突时停止对应部分并由原owner裁决。本次不新增 Task 或冒用历史任务为当前交付；后续实际 PR 应引用本次适用身份与真实证据。
