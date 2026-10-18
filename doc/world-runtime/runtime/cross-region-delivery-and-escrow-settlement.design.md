# 跨区物流与 escrow 结算系统设计

- 生命周期：`active`（目标技术合同，不是实现或发布声明）
- Owner role：`runtime_engineer`
- 相关专业职责：`gameplay_designer` 拥有经济与争议政策；`blockchain_ops_engineer` 拥有权威顺序与最终性；`viewer_engineer` 拥有消费投影；`qa_engineer` 拥有验证。
- 审读日期：2026-10-09
- 固定上游基线：canonical repository `eng-cc/oasis7`，source commit `d8be9bcbb3f4f2781f5bf674aaff48e55493e60c`。实现候选与测试版本另由 PR / CI 记录。

## 1. 问题、目标与非目标

跨区合同需要连接报价、库存、在途货物、目的地保管、所有权与 escrow。已有运输完成与 tariff receipt 不能单独证明商业合同的最终交付。本设计明确技术责任、条件提交、里程碑去重和失败恢复，使提交、到达或入库不会提前释放最终收益。

只覆盖 GAME-018 的物流、结算与读面部分；不覆盖工业提案/试点/准入、许可发放或 OC 桥。版税只消费已有有效政策，不定义费率、价格、赔偿公式、争议裁决规则或外部兑换。本文定义逻辑合同，不新增 wire/ABI 字段、服务或运行实现。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | 发现、承诺、物流和结算分层，适用于跨区合同及其恢复。 | [DES-WR-CR-001](#des-wr-cr-001) | gameplay_designer 的合同政策；runtime_engineer 的权威日志；viewer_engineer 的投影 | 不证明工业创建、许可准入或发布。 |
| [AC-SC31-010](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-010) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | 前序状态不冒充最终交付，适用于跨区合同及其恢复。 | [DES-WR-CR-001](#des-wr-cr-001) | gameplay_designer 的合同政策；runtime_engineer 的权威日志；viewer_engineer 的投影 | 不证明工业创建、许可准入或发布。 |
| [AC-SC31-011](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-011) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | 重复、争议与恢复不复制结算，适用于跨区合同及其恢复。 | [DES-WR-CR-002](#des-wr-cr-002) | gameplay_designer 的合同政策；runtime_engineer 的权威日志；viewer_engineer 的投影 | 不证明工业创建、许可准入或发布。 |
| [AC-GAME-018-04](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-04) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | 信息新鲜度与物理接收边界，适用于跨区合同及其恢复。 | [DES-WR-CR-001](#des-wr-cr-001) | gameplay_designer 的合同政策；runtime_engineer 的权威日志；viewer_engineer 的投影 | 不证明工业创建、许可准入或发布。 |
| [AC-GAME-018-05](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-05) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | 里程碑原子结算与争议恢复，适用于跨区合同及其恢复。 | [DES-WR-CR-002](#des-wr-cr-002) | gameplay_designer 的合同政策；runtime_engineer 的权威日志；viewer_engineer 的投影 | 不证明工业创建、许可准入或发布。 |
| [AC-GAME-018-06](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-06) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | Viewer/API/Agent 同一权威投影，适用于跨区合同及其恢复。 | [DES-WR-CR-003](#des-wr-cr-003) | gameplay_designer 的合同政策；runtime_engineer 的权威日志；viewer_engineer 的投影 | 不证明工业创建、许可准入或发布。 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标与差距 | 证据 / owner |
| --- | --- | --- | --- |
| 运输与 tariff | runtime PRD 记录 transfer/transit、路线预留、损耗、tariff escrow 与一次运输 receipt。 | 保留现有兼容通路；运输 receipt 不自动升级为商业交付凭据。 | [执行矩阵](../prd.md#industrial-execution-status-and-authority-matrix) / runtime_engineer |
| 商业交付与 escrow | 完整合同状态、目的地保管和所有权关联未验证。 | 独立物流/保管/所有权状态、里程碑账本及原子提交。 | 本设计目标；尚无完整实现审计 / runtime_engineer |
| 玩家读面与恢复 | 现有报价测试只覆盖对应运输范围。 | 同候选跨 surface、重启、replay 与争议恢复验收。 | [运输报价测试](../../../crates/oasis7_runtime/src/tests/logistics_transfer_quote.rs)；完整合同仍未验证 / qa_engineer |

## 4. 边界与结构

玩法 authority 提供版本化合同条款：货物/数量、双方、目的地、接收条件、部分交付是否允许、里程碑谓词、结算资产/收益分配、期限、证据窗口、取消/争议/处置条件。runtime 拥有当前合同版本、库存与资金占用、运输事实、保管记录、结算账本及 receipt。P2P 提供同一世界权威顺序与最终性；不新增跨 shard 事务。

物流负责货物位置、数量与损耗；目的地存储负责准入、容量和保管责任；商业合同负责所有权与支付。它们可以在同一 runtime 内执行，但不能互相伪造事实。Viewer/API/Agent 读取同一投影；LLM 建议、客户端时间、缓存、链上付款或单方证明均不是交付 authority。外部付款若被允许，必须由另行审定的资产桥合同证明可用余额，本文不直接消费外部 receipt 为世界资金。

## 5. 关键运行流程

<a id="des-wr-cr-001"></a>
### DES-WR-CR-001：承诺、运输与接收分层

1. 发现和报价只读，返回来源、权威版本、可见范围、新鲜度与不确定性；不预留货物、资金、容量或优先级。TTL 和价格有效期由玩法政策给出，不使用客户端时钟裁定。
2. 确认时绑定合同身份与修订、请求身份/内容、货物、路线、目的地、政策和相关状态版本。提交点重新验证权限、可用库存/余额、路径与目的地条件；漂移或缺依赖原子拒绝，不能静默换货或改道。
3. 承诺提交在同一世界事务中记录合同、资金/货物占用及 receipt。预留不会复制库存；若合同声明不预留目的地容量，明确返回该风险，不能提前承诺必然入库。
4. 出发消费已承诺的货物占用，转为在途保管；保持数量、来源与责任 lineage。路线失败或损耗记录权威事实；自动改道仅在原合同已明确授权且重新校验范围、预算与资源时允许，否则等待显式新选择。
5. `arrival` 只记录抵达边界。`storage_acceptance` 独立验证剩余数量、货物身份、目的地权限、容量和保管责任，并原子登记入库及容量占用；拒收保留边界货物和原责任，不能当作最终交付或删除货物。
6. 所有权转移只在对应合同里程碑的事实、政策和结算条件满足时生效。合同允许部分交付时，每部分绑定不重叠的数量区间与接收事实；不允许时不通过拆分请求绕过整批要求。

<a id="des-wr-cr-002"></a>
### DES-WR-CR-002：条件结算、去重与争议恢复

1. 结算键绑定世界、合同、义务 lineage、里程碑与可结算数量范围；请求身份另用于传输去重。同义新请求、改道、重谈或新合同版本不能使已履行义务重新可结算。不同内容复用同一请求身份必须拒绝。
2. 权威提交点检查已结算账本、接收证据、当前合同/裁决版本、资金占用和争议限制。同一键已有 committed receipt 时返回原结果；未提交准备不等于已结算。
3. 一个里程碑的所有权变化、escrow 释放/扣减、适用费用/版税/奖励、需求履行量、货物可用性、义务账本及 receipt 必须原子提交。总释放不超已锁定可用资金，履行量不超合同数量；同一货物数量不能同时保留原库存、在途量和可用入库量。
4. 争议开启按权威顺序冻结受争议的未结算义务与资金，不冻结无关合同。已合法结算部分保持历史；并发争议与结算按同一顺序判断，不能以客户端先点击为依据。证据提交不自动形成裁决，窗口结束也不自动把钱交给一方；缺适用裁决政策时保持 blocked。
5. 有权裁决记录身份、范围、政策版本、证据与处置；分配/退回/赔偿仍走同一条件结算和去重路径。纠错用新的、关联原 receipt 的补偿义务，不改写历史；补偿的资金来源、上限与授权必须可验证，不凭空生成余额。
6. 延误、损耗、拒收或超时不自动退还全部资金/恢复已损失货物。等待、补救、退回、改道、重谈、争议或放弃均重验权限与剩余资源；退回也是新的物理过程，不等于瞬时入原库。取消只释放可核验、未被消费且不再承担义务的占用，保留在途保管与待决资金。
7. 响应丢失先查询原身份/receipt；重启后只重建未提交状态，不重放已提交 effect。合同 revision 更新保留原义务 lineage 和累计量；不得通过版本变化清零已释放资金或需求履行。

<a id="des-wr-cr-003"></a>
### DES-WR-CR-003：统一读面与能力边界

投影分别呈现物流位置/损耗、保管接收、所有权、各里程碑资金与争议状态；它们不是单一 success 布尔值。返回权威快照身份、来源/新鲜度、已锁定/释放/损失/剩余量、primary blocker、适用 next_action 与 next_recheck。未知 authority 返回 unknown/incomplete/blocked，不用默认值补全。

Viewer/API/Agent 在同一快照下给出等义结果；建议不能提交改道、发起争议或完成结算。断连后的 UI 保留未确认状态，通过权威查询恢复；重连不是付款或交付成功证据。隐私受限的信息标注可见范围，投影不能泄露其他主体的私有库存或证据。

## 6. 接口与数据合同

以下为待实现的逻辑关系，不是已存在字段；实际 schema 必须另行审定并更新执行矩阵。

| 接口 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| 报价/确认 | 玩法政策与 runtime → 玩家请求/runtime | 世界、报价/合同版本、相关资源版本 | 报价只读；确认条件提交 | committed receipt 或漂移/资格/资源拒绝 | 旧运输报价不隐式签发商业合同 |
| 物理事实 | 物流/存储 → 合同结算 | 货物 lineage、数量范围、目的地、事实版本 | arrival 与 acceptance 分开；重复不二次入库 | 可验证事实或拒收/未知 | tariff receipt 不代表商业接收 |
| 里程碑/裁决 | runtime/有权裁决 → 资产账本 | 合同义务键、政策/裁决版本、receipt | 权威顺序；条件原子提交 | 单次结算或 blocked/rejected | 未知版本禁止产生新效果 |
| 投影/查询 | 权威日志 → Viewer/API/Agent | 快照、合同与 receipt 身份 | 查询无效果；旧快照显式 stale | 真实状态、blocker 与合法下一步 | 不支持的能力不能显示成功 |

每个请求、证据、路线和里程碑数量须有确定上限。超限、证据不可核验、超时或未知版本明确拒绝/阻断，不静默截断；producer/consumer 的 wire 版本、错误码与预算仍待实现前冻结。

## 7. 状态、事务与持久化

逻辑维度分别为：合同 `proposed/committed/closed`；物流 `reserved/in_transit/arrived/returned/lost`；接收 `not_received/accepted/rejected`；里程碑 `pending/blocked/settled`；争议 `none/open/resolved`。部分货物可分别持有这些状态，不能用全合同终态抹去残余义务。closed 仅在货物、资金、保管与争议均有明确 disposition 后成立；终态保留历史。状态名称不冻结枚举或 ABI。

账本必须能对账：起始货物 = 当前库存/占用 + 在途/边界保管 + 已接收量 + 确认损耗；locked escrow = 尚持有 + 已释放 + 已退回 + 合同允许的费用处置。每项转移只记一次且记录双方，入库可用性和所有权不是默认同时成立。生产、转售与后续转移另有新 lineage，不能再次消费原已履行义务。

在单一 canonical world transaction 内提交跨子系统变更；不通过独立数据库双写加事后补偿伪装原子性。checkpoint 同时包含合同/义务/占用/货物事实/裁决/receipt，日志恢复逐提交核对；缺依赖或 commitment mismatch 停止相关执行。accepted 只是收到请求；applied 需 committed effect；persisted 需恢复可回读；finality 由 P2P 合同证明。

去重与义务账本不得在合法重放、争议或旧请求仍可到达时清理。retention 由[存储治理](runtime-storage-footprint-governance.prd.md)确定；若压缩历史，保留可验证的累计量、终态与防重放边界。保管丢失或不可读不能推导库存可用或自动退款。

## 8. 部署、安全与运行约束

嵌入现有 runtime 与权威日志，不新增管理员旁路结算。签名只证明请求来源；权限、当前有效合同、证据和资源仍需重验。裁决者权限与作用域必须由治理合同明确，运维工具不能任意改余额、清 escrow 或删除损失。

时间和争议窗口采用权威世界时间。落后节点、证据不可达、预算耗尽或恢复失败只阻断相关义务，不冻结无关常态交易；禁止用线程完成顺序或外部 wall clock 决定结算先后。部署/操作仍沿用已有 runbook，本次不提供生产验证。

## 9. 质量与容量

| 场景 | 环境 / 刺激 | 响应与判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 边界正确性 | 固定候选、拒收/容量竞争/部分损耗 | 前序状态无最终收益；货物守恒差为零 | CR-VERIFY-001 | 计划，未验证 |
| 原子与去重 | 提交前/后 crash、重复身份/新身份、并发结算 | 每义务一次 effect；资金/库存/需求对账差为零 | CR-VERIFY-002 | 计划，未验证 |
| 争议与恢复 | 争议竞态、退回/重谈、checkpoint/replay | 已结算历史不变；残余义务与账本一致 | CR-VERIFY-003 | 计划，未验证 |
| 跨 surface / 容量 | 同快照、stale/断连、超限证据 | 同义状态与 blocker；超限明确失败且零部分 effect | CR-VERIFY-004 | 规模/延迟预算待冻结，无 SLA |

各入口见下列验收手册。规模、并发、硬件、窗口、分位数与预算由 runtime_engineer / qa_engineer 在实现前确定；没有测量不能宣称吞吐或延迟达标。

## 10. 兼容、迁移与回滚

本次只补设计。现有 direct transfer、unbound transit、path-bound transit 与 tariff 兼容语义继续以 runtime PRD 为准；不得将历史完成事件回填成缺少商业接收证据的 settlement。新 schema 必须显式版本化，旧快照识别为未启用新合同，不能默认创建免费 escrow 或 destination acceptance。

启用前完成 schema/政策/预算、单事务实现、消费者兼容、恢复与负例验证，再沿用既有发布入口启用。回滚仅到可读取已写入合同与账本的版本；否则停止新请求并保留待决状态修复。禁用入口不撤销 committed 所有权、历史付款或在途货物；纠错只通过合法新义务处理。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

以下是未来实现验收计划，本次文档检查只证明导航与追踪结构。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-SC31-009](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-009) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [DES-WR-CR-001](#des-wr-cr-001) | 发现、承诺、物流和结算分层。 | [CR-VERIFY-001](../../testing/manual/cross-region-settlement-system-acceptance.manual.md#cr-verify-001)；固定实现候选，runtime/full-tier 按手册刺激并对账。 | 未来 PR/CI 的状态差分、receipt 与恢复结果。 | 本次无完整实现、真实网络或发布证据。 |
| [AC-SC31-010](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-010) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [DES-WR-CR-001](#des-wr-cr-001) | 前序状态不冒充最终交付。 | [CR-VERIFY-001](../../testing/manual/cross-region-settlement-system-acceptance.manual.md#cr-verify-001)；固定实现候选，runtime/full-tier 按手册刺激并对账。 | 未来 PR/CI 的状态差分、receipt 与恢复结果。 | 本次无完整实现、真实网络或发布证据。 |
| [AC-SC31-011](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#ac-sc31-011) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [DES-WR-CR-002](#des-wr-cr-002) | 重复、争议与恢复不复制结算。 | [CR-VERIFY-002](../../testing/manual/cross-region-settlement-system-acceptance.manual.md#cr-verify-002)，并执行同手册 CR-VERIFY-003 的争议/恢复场景；固定实现候选，runtime/full-tier 按手册刺激并对账。 | 未来 PR/CI 的状态差分、receipt 与恢复结果。 | 本次无完整实现、真实网络或发布证据。 |
| [AC-GAME-018-04](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-04) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [DES-WR-CR-001](#des-wr-cr-001) | 信息新鲜度与物理接收边界。 | [CR-VERIFY-001](../../testing/manual/cross-region-settlement-system-acceptance.manual.md#cr-verify-001)；固定实现候选，runtime/full-tier 按手册刺激并对账。 | 未来 PR/CI 的状态差分、receipt 与恢复结果。 | 本次无完整实现、真实网络或发布证据。 |
| [AC-GAME-018-05](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-05) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [DES-WR-CR-002](#des-wr-cr-002) | 里程碑原子结算与争议恢复。 | [CR-VERIFY-002](../../testing/manual/cross-region-settlement-system-acceptance.manual.md#cr-verify-002)，并执行同手册 CR-VERIFY-003 的争议/恢复场景；固定实现候选，runtime/full-tier 按手册刺激并对账。 | 未来 PR/CI 的状态差分、receipt 与恢复结果。 | 本次无完整实现、真实网络或发布证据。 |
| [AC-GAME-018-06](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-06) Trace refs: O7-REQ-7737ac31-a377-46ec-b912-9aadc0ac7fd2 | [DES-WR-CR-003](#des-wr-cr-003) | Viewer/API/Agent 同一权威投影。 | [CR-VERIFY-004](../../testing/manual/cross-region-settlement-system-acceptance.manual.md#cr-verify-004)；固定实现候选，runtime/full-tier 按手册刺激并对账。 | 未来 PR/CI 的状态差分、receipt 与恢复结果。 | 本次无完整实现、真实网络或发布证据。 |

## 12. 决策、长期风险与未决问题

| 项目 | 决策 / owner | 解除或复核条件 |
| --- | --- | --- |
| 多维状态而非单一完成位 | 保留 arrival/acceptance/ownership 的独立因果；runtime_engineer。 | schema 与读面实现前复核部分交付表示。 |
| 单世界原子提交 | 避免资金和货物分裂；runtime_engineer / blockchain_ops_engineer。 | 若引入独立 shard/存储事务，另行设计，不直接外推本文。 |
| 义务 lineage 去重 | 防新请求/版本/重谈重复获益；runtime_engineer。 | 确定数量范围、补偿与压缩身份 schema 并验证。 |
| 裁决/取消/部分交付政策未冻结 | 无默认退款或争议胜者；gameplay_designer。 | 发布版本化政策、权限与窗口，缺失前相关动作 blocked。 |
| 预算、retention、wire 与容量未冻结 | 无当前实现或 SLA 承诺；runtime_engineer / qa_engineer。 | 实现前确定上限、兼容矩阵与故障注入并复核 §§6–10。 |

未来实际修改权限、持久化兼容或关键安全边界时，按[开发流程规范](../../engineering/workflow/source-of-truth.md)安排独立评审；本次设计不改变运行权限、协议或数据。
