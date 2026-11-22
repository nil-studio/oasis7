# 世界规则与核心玩法 — 既有节点经济合同结果接收

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-11-22
- 专业域权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)

本文按 #6507 原样承接根 PRD [`prd.md`](prd.md) 原第 8 节（既有节点经济合同的结果接收），保留全部既有 REQ/SC/AC 锚点、Trace ID 与验收边界。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 设计适用性理由：本专题 PRD 按 #6507 从根 PRD [`prd.md`](prd.md) 拆出承载既有根成功标准/节点经济合同的逐项接收与状态摘要，不新增独立交互稿；对应专业系统设计与验证义务仍由各配对专业 `*.design.md` 承接。

## 8. 既有节点经济合同的结果接收

本节只为已经由下列 P2P 专业 SC/AC/DEC 采用的节点贡献、受限资产兑电和创世控制结果登记产品位置与唯一身份，不新增经济功能或面向所有玩家的收益承诺。原适用角色、阶段、前提、成功与拒绝条件、局部 PRD-ID、历史记录和演进计划全部保留。经济参数、公式、预算、额度、recipient/controller、vesting、签名规则、approved ledger、actual-value review、QA 和执行 authority 仍分别由原 P2P/runtime、README reward governance 与 QA 拥有；本根与 game 不取得新的调参、批准、发放或执行权。根只引用参数真值，不复制数值表。

消费/组合说明：[参与与权益分离](../player-entry-distribution/participation-and-recognition-boundaries.prd.md#req-entry-part-001)与[同候选发行门禁](../player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-candidate-001)继续拥有各自结果，本节不重复声明其身份。基础设施只提供[同输入确定性与原子拒绝](../world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)子范围，不能代替以下经济结果；epoch/node幂等不等同互斥intent lineage。PowerCredit路径不改starter支持、liquid starter OC或任何原有资源反补贴限制。下列条款不声称当前兑付、mint、奖励发放或生产可用；未取得完整证据前保持未核实。

<a id="req-wr-economic-points-eligibility"></a>
### REQ-WR-ECONOMIC-POINTS-ELIGIBILITY：基础义务与合格额外贡献分离

Trace ID: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16

适用范围与成功标准：为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[基础义务与合格额外贡献分离](#req-wr-economic-points-eligibility)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-ELIGIBILITY](#ac-wr-economic-points-eligibility)。
- 系统设计状态：未核实
- 系统设计依据：[基础义务与合格额外贡献分离有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-eligibility)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：真实build_settlement仅计算代跑/维护，self_sim用于基础义务惩罚；完整采样可信性与所有角色画像运行未核实。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不能推出所有玩家获益、实际采样可信或公开网络安全。

<a id="ac-wr-economic-points-eligibility"></a>
### AC-WR-ECONOMIC-POINTS-ELIGIBILITY：基础义务与合格额外贡献分离的原验收

Trace refs: O7-REQ-118e8c96-669b-4b11-bbec-a32831803e16；覆盖要求：[REQ-WR-ECONOMIC-POINTS-ELIGIBILITY](#req-wr-economic-points-eligibility)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-eligibility)适用的固定候选中逐项核验：为自身 Agent 模拟是基础义务，不直接获奖励；代跑离线节点、世界维护、有效存储和合格在线仅按原专业规则进入积分计算。 原验收成功/拒绝条件及层级不变，实际证据至少保留原贡献画像、输入、资格与得分对账；没有对应完整证据时保持未核实。不能推出所有玩家获益、实际采样可信或公开网络安全。本验收不授权执行经济操作。

<a id="req-wr-economic-points-allocation"></a>
### REQ-WR-ECONOMIC-POINTS-ALLOCATION：固定双池的确定性且不重复分配

Trace ID: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2

适用范围与成功标准：两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[固定双池的确定性且不重复分配](#req-wr-economic-points-allocation)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-ALLOCATION](#ac-wr-economic-points-allocation)。
- 系统设计状态：未核实
- 系统设计依据：[固定双池的确定性且不重复分配有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-allocation)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：settle_epoch分别调用主/存储池分配并checked累计；3节点2epoch、严格阈值与独立存储池全部配置运行未核实。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不调整预算/权重/阈值，不证明实际数值平衡。

<a id="ac-wr-economic-points-allocation"></a>
### AC-WR-ECONOMIC-POINTS-ALLOCATION：固定双池的确定性且不重复分配的原验收

Trace refs: O7-REQ-1bd30efb-22b7-403e-94f6-fe0a2efde4a2；覆盖要求：[REQ-WR-ECONOMIC-POINTS-ALLOCATION](#req-wr-economic-points-allocation)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-allocation)适用的固定候选中逐项核验：两个固定 epoch 池分别按原合格正得分分配，独立存储池启用时主池排除存储权重；允许未分配余额，保持守恒、排序、惩罚效果和累计单调。 原验收成功/拒绝条件及层级不变，实际证据至少保留逐epoch/node得分、两池分配及预算差额；没有对应完整证据时保持未核实。不调整预算/权重/阈值，不证明实际数值平衡。本验收不授权执行经济操作。

<a id="req-wr-economic-points-recovery"></a>
### REQ-WR-ECONOMIC-POINTS-RECOVERY：epoch结算与恢复不重复入账

Trace ID: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407

适用范围与成功标准：同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[epoch结算与恢复不重复入账](#req-wr-economic-points-recovery)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-RECOVERY](#ac-wr-economic-points-recovery)。
- 系统设计状态：未核实
- 系统设计依据：[epoch结算与恢复不重复入账有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-recovery)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：worker实际载入/保存collector snapshot，collector恢复ledger/epoch/cursor，mint准备层拒绝相同epoch/node重复记录，直接覆盖恢复与去重子义务；多节点崩溃窗口、降级后重复提交与replay全矩阵仍未核清。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)；[reward_runtime_worker.rs:216–231](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L216-L231)；[reward_runtime_worker.rs:546–573](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L546-L573)；[node_points_runtime.rs:275–323](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L275-L323)；[node_points_settlement_publication.rs:137–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L137-L160)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody。

<a id="ac-wr-economic-points-recovery"></a>
### AC-WR-ECONOMIC-POINTS-RECOVERY：epoch结算与恢复不重复入账的原验收

Trace refs: O7-REQ-54617172-fa9b-4efe-a058-b327ce348407；覆盖要求：[REQ-WR-ECONOMIC-POINTS-RECOVERY](#req-wr-economic-points-recovery)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-recovery)适用的固定候选中逐项核验：同一 epoch/node 结算幂等；采样或快照写入不直接增加奖励，恢复、重复tick与replay只能复现结果；不可读collector状态须可观测降级，不是资产对账成功。 原验收成功/拒绝条件及层级不变，实际证据至少保留恢复前后ledger、settlement及降级记录；没有对应完整证据时保持未核实。runtime closure草案未完整吸收；不证明已结算资产安全恢复或生产custody。本验收不授权执行经济操作。

<a id="req-wr-economic-points-asset-gates"></a>
### REQ-WR-ECONOMIC-POINTS-ASSET-GATES：积分与独立资产路径保持门禁

Trace ID: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5

适用范围与成功标准：积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 仅接收[既有专业义务](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[积分与独立资产路径保持门禁](#req-wr-economic-points-asset-gates)；验收沿[原专业接受关系](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)。
- 对应验收：[AC-WR-ECONOMIC-POINTS-ASSET-GATES](#ac-wr-economic-points-asset-gates)。
- 系统设计状态：未核实
- 系统设计依据：[积分与独立资产路径保持门禁有限设计](../../p2p/node/node-contribution-points.design.md#des-economic-points-asset-gates)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：生产settlement准备分别对PowerCredit使用reward_asset_config及签名mint记录，对主Token bridge使用独立epoch issuance的node_service预算、分配总额与重复epoch守卫，直接支持资产路径不同账本/门禁子范围；两路径完整治理授权、非自动兑换策略及多激励重复奖励的独立审批仍未核清。 源码/测试定位：[node_points.rs:175–300](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L175-L300)；[node_points_runtime.rs:325–381](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points_runtime.rs#L325-L381)；[reward_runtime_worker.rs:325–349](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L325-L349)；[node_points.rs:557–580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/node_points.rs#L557-L580)；[node_points_settlement_publication.rs:27–60](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L60)；[node_points_settlement_publication.rs:137–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L137-L160)；[node_points_settlement_publication.rs:227–279](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L227-L279)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness。

<a id="ac-wr-economic-points-asset-gates"></a>
### AC-WR-ECONOMIC-POINTS-ASSET-GATES：积分与独立资产路径保持门禁的原验收

Trace refs: O7-REQ-45b12bd9-9d07-4bbf-86bc-2a85af07d5f5；覆盖要求：[REQ-WR-ECONOMIC-POINTS-ASSET-GATES](#req-wr-economic-points-asset-gates)。

在[原专业条件与场景](../../p2p/node/node-contribution-points.prd.md#economic-contract-points-asset-gates)适用的固定候选中逐项核验：积分进入PowerCredit与主链Token bridge是两条独立受治理路径，不是自动兑换或同一池；多激励并用仍需独立审查重复奖励。 原验收成功/拒绝条件及层级不变，实际证据至少保留每条路径预算/授权/拒绝与独立审查记录；没有对应完整证据时保持未核实。不启用资产，不调整兑换比例或发行分桶，不证明兑付readiness。本验收不授权执行经济操作。

<a id="req-wr-economic-power-ledger"></a>
### REQ-WR-ECONOMIC-POWER-LEDGER：贡献形成受限协议资产账本

Trace ID: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0

适用范围与成功标准：PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[贡献形成受限协议资产账本](#req-wr-economic-power-ledger)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)。
- 对应验收：[AC-WR-ECONOMIC-POWER-LEDGER](#ac-wr-economic-power-ledger)。
- 系统设计状态：未核实
- 系统设计依据：[贡献形成受限协议资产账本有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-ledger)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：实际mint准备按source_awarded_points/points_per_credit限制credits，为各node生成power_credit_balance/total_minted及可追溯mint records后安装，直接覆盖积分换算到资产账本子范围；不可直接转账的全部入口排除、快照/replay完整账本核对尚未核清，不扩大市场或兑付承诺。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[node_points_settlement_publication.rs:121–225](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L121-L225)；[node_points_settlement_publication.rs:371–394](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L371-L394)；[reward_runtime_settlement.rs:9–21](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_settlement.rs#L9-L21)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不更改兑换率，不新增转账/市场或当前可兑现承诺。

<a id="ac-wr-economic-power-ledger"></a>
### AC-WR-ECONOMIC-POWER-LEDGER：贡献形成受限协议资产账本的原验收

Trace refs: O7-REQ-c0f136a1-3baf-497e-a0ec-622e745024d0；覆盖要求：[REQ-WR-ECONOMIC-POWER-LEDGER](#req-wr-economic-power-ledger)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-ledger)适用的固定候选中逐项核验：PowerCredit保持不可直接转账的受限协议资产；按原积分换算和mint记录形成可追溯余额，不扩展通用交易市场。 原验收成功/拒绝条件及层级不变，实际证据至少保留积分/credits换算与mint/balance/record对账；没有对应完整证据时保持未核实。不更改兑换率，不新增转账/市场或当前可兑现承诺。本验收不授权执行经济操作。

<a id="req-wr-economic-power-redemption"></a>
### REQ-WR-ECONOMIC-POWER-REDEMPTION：全部条件成立才燃烧资产并增加目标电力

Trace ID: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb

适用范围与成功标准：兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[全部条件成立才燃烧资产并增加目标电力](#req-wr-economic-power-redemption)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)。
- 对应验收：[AC-WR-ECONOMIC-POWER-REDEMPTION](#ac-wr-economic-power-redemption)。
- 系统设计状态：未核实
- 系统设计依据：[全部条件成立才燃烧资产并增加目标电力有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-redemption)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：实际worker签名并提交RedeemPowerSigned；PowerRedeemed准备层校验nonce、credit、reserve、epoch cap、目标存在与溢出后才生成扣credit/减reserve/加目标电力投影，直接覆盖兑换守卫和余额变化子义务；完整授权/治理组合、跨节点恢复与重放矩阵未核清。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[reward_runtime_settlement.rs:23–109](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_settlement.rs#L23-L109)；[power_redemption_publication.rs:78–206](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/power_redemption_publication.rs#L78-L206)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不改额度、目标、权限或承诺当前服务可用。

<a id="ac-wr-economic-power-redemption"></a>
### AC-WR-ECONOMIC-POWER-REDEMPTION：全部条件成立才燃烧资产并增加目标电力的原验收

Trace refs: O7-REQ-a061488d-5b14-489a-af77-ed3f6f75f1bb；覆盖要求：[REQ-WR-ECONOMIC-POWER-REDEMPTION](#req-wr-economic-power-redemption)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-redemption)适用的固定候选中逐项核验：兑换沿原余额、nonce、最小量、epoch储备与上限条件；成功须扣资产、扣储备与增加指定Agent电力一致，失败不得伪装兑付成功。 原验收成功/拒绝条件及层级不变，实际证据至少保留credits/reserve/electricity与拒绝原因；没有对应完整证据时保持未核实。不改额度、目标、权限或承诺当前服务可用。本验收不授权执行经济操作。

<a id="req-wr-economic-power-signature"></a>
### REQ-WR-ECONOMIC-POWER-SIGNATURE：签名治理条件不足必须拒绝

Trace ID: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a

适用范围与成功标准：v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[签名治理条件不足必须拒绝](#req-wr-economic-power-signature)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)。
- 对应验收：[AC-WR-ECONOMIC-POWER-SIGNATURE](#ac-wr-economic-power-signature)。
- 系统设计状态：未核实
- 系统设计依据：[签名治理条件不足必须拒绝有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-signature)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：mint生产准备层实际调用基于state的签名校验并拒绝invalid；worker绑定身份后签署并提交RedeemPowerSigned，直接覆盖部分签名守卫，不只是签名工具存在；完整governance/emergency/authority策略和恢复矩阵仍未核清。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[node_points_settlement_publication.rs:143–160](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L143-L160)；[reward_runtime_settlement.rs:23–104](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_settlement.rs#L23-L104)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不接触密钥，不证明HSM/KMS或跨组织custody。

<a id="ac-wr-economic-power-signature"></a>
### AC-WR-ECONOMIC-POWER-SIGNATURE：签名治理条件不足必须拒绝的原验收

Trace refs: O7-REQ-85c5baeb-e05a-4924-9024-e05f1903bd2a；覆盖要求：[REQ-WR-ECONOMIC-POWER-SIGNATURE](#req-wr-economic-power-signature)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-signature)适用的固定候选中逐项核验：v1仅历史可重算摘要；v2与兑换签名依既有治理策略和绑定执行，缺少密钥、绑定或所需策略条件时fail closed。 原验收成功/拒绝条件及层级不变，实际证据至少保留版本化载荷、绑定/策略与拒绝结果；没有对应完整证据时保持未核实。不接触密钥，不证明HSM/KMS或跨组织custody。本验收不授权执行经济操作。

<a id="req-wr-economic-power-audit"></a>
### REQ-WR-ECONOMIC-POWER-AUDIT：审计与恢复检测异常而不自动修复

Trace ID: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5

适用范围与成功标准：mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 仅接收[既有专业义务](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[审计与恢复检测异常而不自动修复](#req-wr-economic-power-audit)；验收沿[原专业接受关系](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)。
- 对应验收：[AC-WR-ECONOMIC-POWER-AUDIT](#ac-wr-economic-power-audit)。
- 系统设计状态：未核实
- 系统设计依据：[审计与恢复检测异常而不自动修复有限设计](../../p2p/node/node-redeemable-power-asset.design.md#des-economic-power-audit)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 固定候选静态复核：公开只读invariant report检查node/global的mint-burn-balance关系及mint签名，仅累积violations而不改state；真实reward worker调用report.is_ok更新metrics，直接覆盖异常检测不自动修复子义务；旧快照签名策略、跨节点/replay恢复一致性与人工报告复核仍未核清。 源码/测试定位：[node_points_settlement_publication.rs:27–95](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/node_points_settlement_publication.rs#L27-L95)；[reward_asset.rs:129–253](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/reward_asset.rs#L129-L253)；[reward_asset_core.rs:246–315](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L246-L315)；[resources.rs:352–436](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/resources.rs#L352-L436)；[reward_runtime_worker.rs:447–461](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/reward_runtime_worker.rs#L447-L461)；[reward_asset_core.rs:316–393](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/reward_asset_core.rs#L316-L393)。测试定义不等于执行通过；本次未运行业务测试，同候选 CI 的业务测试组 skipped，不能代签本卡验收。
- 核对日期与基线：2026-10-17；需求、设计与静态源码固定 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；实际业务运行未执行。
- 剩余范围：完整专业技术承接和适用验收仍待核实；不证明mainnet或生产恢复，不恢复历史viewer flags入口。

<a id="ac-wr-economic-power-audit"></a>
### AC-WR-ECONOMIC-POWER-AUDIT：审计与恢复检测异常而不自动修复的原验收

Trace refs: O7-REQ-9804d9d7-2c75-475f-b321-5c4ed36276b5；覆盖要求：[REQ-WR-ECONOMIC-POWER-AUDIT](#req-wr-economic-power-audit)。

在[原专业条件与场景](../../p2p/node/node-redeemable-power-asset.prd.md#economic-contract-power-audit)适用的固定候选中逐项核验：mint/burn/balance/record与快照/replay保持一致；invariant report用于检测而非自动修复，旧快照可读取不免除签名策略与报告复核。 原验收成功/拒绝条件及层级不变，实际证据至少保留快照前后状态、invariant report与策略复核；没有对应完整证据时保持未核实。不证明mainnet或生产恢复，不恢复历史viewer flags入口。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-allocation"></a>
### REQ-WR-ECONOMIC-GENESIS-ALLOCATION：冻结分配可核算到各bucket与recipient

Trace ID: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4

适用范围与成功标准：按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[冻结分配可核算到各bucket与recipient](#req-wr-economic-genesis-allocation)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-ALLOCATION](#ac-wr-economic-genesis-allocation)。
- 系统设计状态：部分覆盖
- 系统设计依据：[冻结分配可核算到各bucket与recipient有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-allocation)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.1/3.2A/3.4明确七桶、绝对额、custody/treasury区别及runtime rounding；真实地址/最终QA仍为外部前提。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：生产genesis把输入plan按floor+ratio降序/bucket升序余数分配，并聚合recipient vested；circulating/liquid初始为0。 InitializeMainTokenGenesis→build allocations→PreparedMainTokenMonetaryEvent→genesis buckets/recipient balances。 真实recipient/controller绑定仍pending；实现校验任意传入plan总bps=10000，不证明冻结七桶内容被强制或正式mint获准。 已非仅fixture：真实rounding、总额守恒、重复初始化拒绝与custody非treasury路径直接可达。 证据：[main_token.rs:144–289](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/main_token.rs#L144-L289)；[main_token_monetary_publication.rs:105–218](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/main_token_monetary_publication.rs#L105-L218)；[mainchain-token-genesis-freeze-sheet.md:17–60](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L17-L60)；[mainchain-token-genesis-and-contributor-reward.design.md:22–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L22-L107)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：冻结七桶输入的正式选定/约束、真实账户绑定、同候选QA与实际执行仍缺；不执行交易、不以任意合法plan等同正式分配。

<a id="ac-wr-economic-genesis-allocation"></a>
### AC-WR-ECONOMIC-GENESIS-ALLOCATION：冻结分配可核算到各bucket与recipient的原验收

Trace refs: O7-REQ-585609e7-ee93-4315-b9c7-fe58857164b4；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-ALLOCATION](#req-wr-economic-genesis-allocation)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-allocation)适用的固定候选中逐项核验：按既有冻结总量和七桶参数核算分配及recipient vested_balance；采用原runtime rounding，创世custody account不等于treasury bucket。 原验收成功/拒绝条件及层级不变，实际证据至少保留固定参数版本、七桶金额与rounding对账；没有对应完整证据时保持未核实。不变更总量/比例/账户，不执行InitializeMainTokenGenesis。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-control"></a>
### REQ-WR-ECONOMIC-GENESIS-CONTROL：控制主体与个人受益及释放边界分别可审计

Trace ID: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9

适用范围与成功标准：项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[控制主体与个人受益及释放边界分别可审计](#req-wr-economic-genesis-control)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-CONTROL](#ac-wr-economic-genesis-control)。
- 系统设计状态：部分覆盖
- 系统设计依据：[控制主体与个人受益及释放边界分别可审计有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-control)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.2/3.2A分开战略控制、协议储备、个人受益及首年释放；有限合同明确，真实控制映射与审计执行未闭合。
- 实现状态：部分实现
- 实现与验证依据：2026-10-17 有界源码/记录续核：genesis liquid=0；claim实际校验recipient、epoch cliff/linear unlock与releasable，再准备vesting事件。 ClaimMainTokenVesting→main_token_bucket_unlocked_amount→PreparedMainTokenMonetaryEvent。 当前bucket recipient与nonce约束独立于真实个人受益/控制主体审计。 直接支持原vesting/claim/genesis_liquid子范围，不是仅测试fixture。 证据：[main_token.rs:314–330](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/main_token.rs#L314-L330)；[main_token.rs:325–380](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/main_token.rs#L325-L380)；[main_token_monetary_publication.rs:188–218](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/main_token_monetary_publication.rs#L188-L218)；[mainchain-token-genesis-freeze-sheet.md:49–68](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L49-L68)；[mainchain-token-genesis-and-contributor-reward.design.md:22–107](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L22-L107)。本次仅静态检查，未运行业务测试或实际操作，不宣称完整验收。
- 核对日期与基线：2026-10-17；引用逐条固定于 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9` 或当前 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，旧源码经 Git blob 与固定树核对；新引用来自当前固定提交；未把旧引用重标为当前运行证据。
- 剩余范围：真实控制主体、个人受益拆分、非团队首年释放总门槛及正式controller资格未核实；协议储备不作团队库存。

<a id="ac-wr-economic-genesis-control"></a>
### AC-WR-ECONOMIC-GENESIS-CONTROL：控制主体与个人受益及释放边界分别可审计的原验收

Trace refs: O7-REQ-2ee6964f-7c70-453c-958d-616b07c5d9d9；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-CONTROL](#req-wr-economic-genesis-control)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-control)适用的固定候选中逐项核验：项目战略控制、协议储备、个人直接受益与外部流通分别核算；原vesting、claim、genesis_liquid与首年释放门槛继续独立约束。 原验收成功/拒绝条件及层级不变，实际证据至少保留真实控制映射、受益拆分、释放计划与阻断依据；没有对应完整证据时保持未核实。不改控制人/vesting/释放额度，不把协议储备当团队库存。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-freeze"></a>
### REQ-WR-ECONOMIC-GENESIS-FREEZE：逻辑冻结不等于真实mint-ready

Trace ID: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15

适用范围与成功标准：slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[逻辑冻结不等于真实mint-ready](#req-wr-economic-genesis-freeze)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-FREEZE](#ac-wr-economic-genesis-freeze)。
- 系统设计状态：部分覆盖
- 系统设计依据：[逻辑冻结不等于真实mint-ready有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-freeze)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.4和formal sheet gates把逻辑冻结、实际绑定和QA-before-mint分开；不据存在清单宣布执行门禁已验证。
- 实现状态：未核实
- 实现与验证依据：本轮把三层分开：正式 freeze sheet 已实际产出 slot/controller/signer/runtime_target/rounding/status（原专题第11项验收的文档产物已存在），仍明确为 address_binding_pending；不能把该已完成产物抹成纯模板。工具层已实现 public manifest → controller registry 持久化，strict audit 检查 threshold/manifest 一致性，chain runtime 再消费 registry 配置 signer policy；其 ready_for_ops_drill 不是 mint-ready 或创世 QA pass。该工具链不证明本次全部 recipient/controller 真实绑定、个人受益表和最终 QA 已闭合。[正式冻结与待绑定清单](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/p2p/token/mainchain-token-genesis-freeze-sheet.md#L49-L118)；[manifest 导入消费者](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/crates/oasis7_ops_tools/src/bin/oasis7_governance_registry_import.rs#L119-L163)；[strict audit 的真实结论](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/crates/oasis7_ops_tools/src/bin/oasis7_governance_registry_audit.rs#L292-L347)；[chain runtime 消费 registry](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/governance_registry.rs#L235-L263)；[独立 QA 门槛](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/testing/governance/token-genesis-allocation-audit-checklist.design.md#L12-L26)。未核到同候选完整 pre-mint 证据，保持整体未核实；这不是代码未实现结论，也不把通用 signer 机制认作本卡业务门槛已验收。
- 核对日期与基线：2026-10-18；固定 main `97ceafff06d0a76b79eef8e34fc4c49f470ceb05`；引用源码/记录逐文件与完整固定树 Git blob SHA 核对，行锚来自该提交。仅静态源码、合同及记录检查；未运行测试、ceremony、QA 审计或资金/链上操作。
- 剩余范围：由原治理/runtime/QA owner 对应唯一候选补核真实 recipient/controller 绑定、个人受益拆分、完整 pre-mint 清单及最终 QA verdict/原始证据；逻辑冻结、public signer policy 与 ready_for_ops_drill 均不授予 mint 许可。当前资料不足以判定外部 ceremony 已发生或未发生，不新增政策/参数，不重启已停止的经济域汇总。

<a id="ac-wr-economic-genesis-freeze"></a>
### AC-WR-ECONOMIC-GENESIS-FREEZE：逻辑冻结不等于真实mint-ready的原验收

Trace refs: O7-REQ-6ca62d73-1934-4a80-bb18-1c21e35e2a15；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-FREEZE](#req-wr-economic-genesis-freeze)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-freeze)适用的固定候选中逐项核验：slot逻辑冻结与public signer规则不代替真实recipient/controller绑定、个人受益表和最终QA pass；pending或conditional状态不授予mint许可。 原验收成功/拒绝条件及层级不变，实际证据至少保留绑定证据、完整pre-mint清单与该次QA verdict；没有对应完整证据时保持未核实。本轮不绑定地址/账户、不运行ceremony、不改变signer set或执行交易。本验收不授权执行经济操作。

<a id="req-wr-economic-genesis-reward"></a>
### REQ-WR-ECONOMIC-GENESIS-REWARD：独立reserve只消费最终复核的批准记录

Trace ID: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa

适用范围与成功标准：独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 仅接收[既有专业义务](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)，原角色与原验收不变；具体原条款由该专业接受关系列明。

- 功能点：[独立reserve只消费最终复核的批准记录](#req-wr-economic-genesis-reward)；验收沿[原专业接受关系](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)。
- 对应验收：[AC-WR-ECONOMIC-GENESIS-REWARD](#ac-wr-economic-genesis-reward)。
- 系统设计状态：部分覆盖
- 系统设计依据：[独立reserve只消费最终复核的批准记录有限设计](../../p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#des-economic-genesis-reward)提供对应定位，未核清其全部技术义务；存在链接不等于覆盖。 本轮补核：专业设计§3.3明确独立reserve与producer approval，正式round记录体现最终复核义务；reserve执行和closure仍未证明。
- 实现状态：未核实
- 实现与验证依据：现行合同的 limited preview 路径是独立 reward reserve 多签按 canonical README ledger 最终 actual-value-reviewed approved record 执行，不要求本轮额外建设 fully on-chain proposal-bound 分发器。固定记录确有 LTRL-PR-60/59 经复核批准的 100/50 OC，但 Execution Owner、Distribution Ref/Date 与 closure 仍空/pending；只证明批准事实，未证明独立 reserve 实际消费。PR intake 工具只生成 draft/deferred 行，审批、金额和 distribution 字段留空，是候选采集工具而非奖励执行器；通用 treasury/node reward 不能替代本卡。[独立多签适用合同](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/p2p/token/mainchain-token-genesis-and-contributor-reward.design.md#L60-L67)；[批准行与最终金额](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/readme/governance/readme-limited-preview-contributor-reward-ledger-round-2026-04-13.md#L15-L46)；[待执行闭包](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/doc/readme/governance/readme-limited-preview-contributor-reward-distribution-closure-round-2026-04-13.md#L1-L30)；[intake 状态输出](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/scripts/readme_reward_pr_intake_lib.py#L198-L213)；[intake 保留审批字段为空](https://github.com/nil-studio/oasis7/blob/97ceafff06d0a76b79eef8e34fc4c49f470ceb05/scripts/readme_reward_pr_intake_lib.py#L277-L299)。保持整体未核实，不因外部发放记录缺失推断代码未实现；不将批准或采集工具升格为已发放。
- 核对日期与基线：2026-10-18；固定 main `97ceafff06d0a76b79eef8e34fc4c49f470ceb05`；引用源码/记录逐文件与完整固定树 Git blob SHA 核对，行锚来自该提交。仅静态源码、合同及记录检查；未运行测试、ceremony、QA 审计或资金/链上操作。
- 剩余范围：由已获授权的执行 owner 提供该轮独立 reserve/多签消费 approved record、最终金额、不可变 distribution ref/date 及源 ledger 闭包证据；#56 不在本轮批准集合。资格、评分与 actual-value review 继续归 canonical pack/ledger；不改变金额或全局 band 映射、不并 ecosystem_pool、不执行发放或恢复旧汇总。

<a id="ac-wr-economic-genesis-reward"></a>
### AC-WR-ECONOMIC-GENESIS-REWARD：独立reserve只消费最终复核的批准记录的原验收

Trace refs: O7-REQ-e84d3e5c-af7c-4293-8d02-3b466af6dbfa；覆盖要求：[REQ-WR-ECONOMIC-GENESIS-REWARD](#req-wr-economic-genesis-reward)。

在[原专业条件与场景](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#economic-contract-genesis-reward)适用的固定候选中逐项核验：独立reward reserve只按canonical README ledger中approved且完成distribution前actual-value review的最终金额执行；资格/评分仍归pack/ledger，不并ecosystem_pool，不按登录或游玩自动发放。 原验收成功/拒绝条件及层级不变，实际证据至少保留canonical批准行、最终金额复核、审批及distribution引用；没有对应完整证据时保持未核实。本轮无发放授权，不修改资格/金额/审批；模板不是已批准批次。本验收不授权执行经济操作。
