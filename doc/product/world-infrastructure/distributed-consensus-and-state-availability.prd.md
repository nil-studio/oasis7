# 分布式共识与状态可用性

## 文档身份

- 所属产品模块：权威世界基础设施
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：`2026-10-09`
- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)（共识最终性、证明与 freshness）、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)（版本化执行、manifest/head、pending、receipt 与去重）、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)（消费者/Viewer/入口状态反馈）、[`doc/testing/prd.md`](../../testing/prd.md)（组合验证与证据）。
- 运维 authority：[`node triad 运维与可观测性`](../../p2p/node/node-triad-operations-observability.prd.md#权威边界)（节点 inventory、topology、health/status 与运维证据边界）、[`public-testnet governed bootstrap runbook`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#stable-authority-and-evidence-boundary)（deployment truth、恢复/回滚演练与同窗口事实捕获）。
- 状态同步 evidence envelope：[`state-sync closure evidence packet`](../../testing/templates/state-sync-closure-evidence-packet-template.md#claim-boundary)（topology/node truth、peer heads、gap sync、observer catch-up 与 blob closure；`module_full` 证据，不单独证明 readiness）。

本文定义 oasis7 区块链/分布式系统底层向上层确定性世界执行提供的产品级保证。它不定义共识消息、密码学、网络协议、节点配置、存储格式或运行步骤；这些由 P2P、共识和运维专业权威拥有。运维证据的采集与解释由 `blockchain_ops_engineer` 负责，不能单独把模块证据解释为 release、public testnet 或 mainnet readiness。

<a id="使用者情境与术语"></a>
### 使用者体验、决策与术语

玩家与 Agent 需要判断当前观察是否可信、行动是否可以提交；节点运营者需要判断副本是否能 serving（提供可信状态）或 voting（参与权威投票）。正常读取、提交与等待结果的路径见 §4.1，失败时的可用性和恢复选择见 §4；这些是目标合同，不是当前节点已经支持全部路径的说明。

`canonical history` 是同一世界的唯一权威历史；`commit certificate` 是用于验证最终性提交的证书，节点可达或接口成功不能替代它；`checkpoint` 是恢复所依据的历史检查点，`snapshot` 是状态快照，`replay` 是按权威日志重放历史，`hash-bound` 是材料与哈希承诺绑定。`freshness` 指材料的新鲜度可证明，不能从本地时间或页面刷新推导。`light companion` 是验证所需最终化头及证明的轻量伴随节点；它与 full/state-sync/archive 服务节点均不因提供材料而取得共识权威。`manifest`、`intent`、`receipt` 与 `fail closed` 的产品含义见[根 PRD 的阅读术语](prd.md#阅读术语)，具体字段仍由专业域定义。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 适用边界：仅豁免重复的产品体验配对 design；专业系统设计仍按各功能状态和准确承接独立判定，不因此变为“不适用”。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 只定义 canonical history、可用性、权限隔离和 fail-closed 产品结果；它不声明独立玩家信息架构。
- 历史设计判定记录：历史分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，历史闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
## 1. 基础承诺

基础层为一个 `world_id` 提供唯一的可验证提交序列：只有当时合法激活 authority profile 的正式提交证明与持久条件成立才使输入成为世界历史。它提供节点间复制、状态同步、可验证的数据可用性、故障恢复与证明服务；它不定义游戏规则、Agent 行为、玩家交互或发行体验。

首次开放目标为 `controlled_single_authority`，正确性信任包含运营权威；唯一追加权、跨独立故障域持久确认与同世界恢复必须通过后才开放。当前兼容原型不能据此升级为正式权威。后续确定性 BFT 是目标模型：oasis7 自己拥有协议语义，首个实现采用 Tendermint/CometBFT 风格的 `Propose -> Prevote -> Precommit -> Commit` 轮次。有效 commit certificate 必须证明活动、治理注册的验证者集合中超过三分之二质押权重的签名预提交。slot 只负责提议节奏；round 在超时后推进。三等权验证者是首个可验证的运行基线，不构成 `f=1` Byzantine 容错声明；四等权验证者才是该声明的最小常规拓扑。

## 2. 边界与不变量

- 一个全局 canonical order 和一个权威 `world_id` 历史是默认拓扑；区域只是逻辑作用域，不能独立最终化。未来分片必须先证明跨域收敛。
- chain 保存顺序、commitment、验证者转换和最终性证明；DistFS/CAS 保存 hash-bound 的 snapshot、blob 和历史可用性材料，永不自行取得最终写入权。
- 验证者为治理注册、可轮换的受保护节点；公开 sentry/relay、full/state-sync/archive、RPC/proof gateway 可无许可运行。服务节点被攻破不能产生共识权威。
- 验证者保留权威恢复/投票窗口；full/state-sync 节点提供较长热历史和快照；archive 保存完整审计历史；light companion 保留 finalized header、验证者转换和所需证明。这些角色决定本地常驻工作集，不缩减正式持久世界的完整合法历史及原始依赖保留承诺。任何 pruning 仅在所有受影响历史仍可重建、原 bytes/hash/root 验证及必要独立故障域中的冗余 archive 实际可读被证明后进行；hash、索引或 manifest 本身不能替代原材料。
- bootstrap/recovery 必须依序绑定 固定 world/chain/genesis 身份、各历史高度合法 profile/epoch/manifest 激活链、已确认 checkpoint 证明、hash-bound snapshot、canonical committed-log replay 和 verified state root。任一身份、签名、连续性、hash、replay 或 root 不匹配均 fail closed。

完整历史的具体保留对象、无损优化、冷热读取、回收、迁移与容量验收由[完整世界历史与无损存储](lossless-world-history-storage.prd.md#req-lws-001)承接。`dev_local`、`release_default`、`soak_forensics` 是存储工作集与预算策略，不是提交权威或历史保留权限；用于同一正式持久世界时均不得以热窗口、条数、时间或磁盘上限删除唯一历史。数据随真实新增信息持续增长，容量不足按既有可用性语义限制相应操作，不改写已确认结果或减少必要独立副本。

## 3. 运行与经济边界

- 终端安全模型中，验证者使用 stake、奖励和客观可证明的共识故障 slashing；非权威服务使用可验证 usage/availability receipt 与市场费用或治理 grant。基础层拥有计量、证明、锁定与确定性结算 hook，不拥有奖励率、费用、补贴预算或资格数值。
- 基础层可选择性采用成熟库并借鉴公开链架构，但所有共识、状态、治理、receipt、replay 与恢复语义仍由 oasis7 自己拥有；第三方依赖必须保持可替换。

## 4. 可用性等级、消费者反馈与 fail-closed 边界

共识、复制和存储层的“可达”不等于消费者可以把读到的字节当作当前世界或提交新行动。为避免节点、缓存和入口各自发明健康含义，产品层将可用性收敛为以下四种消费者语义；它们不是新的玩家访问模式，也不冻结实现字段或内部枚举。

- **已验证可服务**：`world_id`、最新可验证 checkpoint/head、hash-bound 状态和适用的最终性/追加条件均成立。消费者可以观察 committed state，并按对应专业合同提交新的 intent；“可提交”仍不等于该 intent 已产生世界效果。
- **已验证只读**：同一 `world_id` 的 checkpoint/snapshot、canonical replay 和 state root 可验证，但当前追加、最终性或服务闸门尚未成立。消费者可以查看最后一个已验证状态、历史 receipt 与仍无世界效果的 pending；新的 intent 必须原子拒绝或保持明确的无效果待决，不得取得资源、资格、排队优先或隐性完成承诺。
- **陈旧/追赶中**：副本能证明自己属于同一世界，但其 head 或可用性材料落后于当前权威，或 freshness 尚未证明。只读内容必须标记陈旧/追赶中及最后可信边界；不能把陈旧状态用于声称当前权限、价格、容量、治理结果或行动成功。依赖新鲜条件的 intent 只能等待重新验证、重新评估或被 fail closed。
- **不可用/隔离**：`world_id`、checkpoint、hash/root、canonical 连续性或证明材料缺失、冲突、回退或指向其他世界。消费者只能看到最后一个已验证 receipt（如有）及“原世界不可用/待恢复”说明；不得把缓存、部分快照、替代 endpoint 或本地开发世界表述为当前世界，也不得接受新的权威 intent。

“已验证可服务”还必须满足当前 governing runtime manifest/version 的 compatibility declaration 已能对应到 committed/finality-verified canonical execution block，且 head continuity 单调可验证；客户端提交的 version 不能自行选择世界规则。manifest、compatibility 或 head continuity 证据缺失、冲突、未知或非单调时，不得保持可服务语义，必须降级为已验证只读或不可用/隔离，并原子拒绝或保留无效果待决；每个受影响新 intent 的 committed receipt 必须为 `0`。版本激活与 governing version 的具体边界见[`确定性世界执行`](deterministic-world-execution.prd.md)第 2.2 节及其 DE-1/DE-4。

每次从一种语义降级或恢复到另一种语义，正式消费者都必须能读到当前状态、影响的操作范围、最后可信边界（若可提供）、主要 blocker 和真实下一步（等待、重新验证、重新进入、重新规划或安全返回）。状态变化不能依靠页面刷新、进程存活、重连成功、历史提交回执或本地缓存静默升级；恢复只读、恢复可服务和恢复受阻的组合闸门仍以本模块根 PRD 的同名语义为准。

以下边界必须保持一致：

- 网络分区、lagging replica、proof gateway 暂时不可达或 archive 追赶时，不能选择“看起来最新”的副本继续写入；只有重新取得适用证明后才能升级状态。
- 状态字节可读但 proof/root 校验失败时，视为不可验证而非“可服务”；不得以 transport green、HTTP 成功或节点存活替代权威证明。
- 两个候选 snapshot/checkpoint 互相冲突时必须隔离并报告冲突，不得在产品层合并、择一或把其中一个当作新世界；同一 `world_id` 的已确认 receipt 仍保持原历史。
- 重连、重复提交和跨入口重试都要重新评估当前等级。缓存的“已发送/处理中”只保留请求关联，不产生第二次世界效果，也不能把无效果 pending 表述为已结算。

本节定义的是跨消费者的产品结果与反馈语义，具体合同按以下入口分工：P2P 负责共识最终性、证明与 freshness；world-runtime 负责版本化执行 manifest/compatibility、head continuity、服务闸门、pending 持久化、去重与 receipt；world-simulator 负责消费者/Viewer/入口的状态与 UI/API 反馈投影；testing 负责组合矩阵、验证命令与证据。产品层不复制这些专业字段、错误码或实现。

Compatibility declaration 只证明客户端能理解当前 manifest，不能选择或锁定 governing version；manifest 缺失、冲突或无法与当前 committed/finality-verified block 对账时，前述可服务语义不成立。

## 4.1 正常路径与消费者选择

正常路径从消费者读取同一 `world_id` 的已验证状态开始：消费者确认当前等级、最后可信边界和适用的版本/最终性条件，再选择等待、重新验证或提交一个 intent。只有在“已验证可服务”成立时才进入专业合同定义的提交路径；提交后仍须等待可验证的 committed receipt，消费者再据此更新自己的世界结论。已验证只读、陈旧/追赶中或不可用/隔离时，路径停留在查看历史、等待恢复、重新规划或安全返回，不把本地排队和界面反馈包装成世界效果。

该选择的代价是明确的：等待或重新规划可能延迟行动，但提交未经证明的状态会造成重复、错误授权或把临时读面误认为权威历史。基础设施产品层只规定这种可观察的选择和风险边界；提交顺序、去重、receipt 字段和具体恢复步骤仍由 P2P、world-runtime、消费者与测试专业权威定义。

提交未知与已知无效果待决分别表达；断连/超时不能证明未提交时，按原请求身份核对，不能判确定失败或重发。世界保留、网络环境、发行阶段与经济价值独立；同世界合法 profile 演进及 H/H+1 不可撤回交接依 [P2P 专业合同](../../p2p/prd.md#p2p-authority-profiles) 验证。恢复可服务还必须满足当前 profile 的持久性与旧写者隔离条件。

## 5. 当前与目标的分离

当前实现是 stake-weighted proposer/attestation threshold prototype，不是已经具备完整 BFT 最终性的公开承诺。目标仍缺持久且可复验的 quorum certificate、prevote/precommit 锁定、round timeout/view-change、验证者转换证明、复制端证书复验与对抗性恢复证据。本文不因目标描述而宣称 mainnet、去中心化规模、SLA 或发行 readiness。

<a id="6-叶子需求与可观察验收"></a>
## 6. 叶子要求与可观察验收

这些叶子把本专题的五类可独立失败义务映射到现有 DC 汇总标准；它们保留 P2P、runtime、blockchain ops 和 QA 的专业 authority，不把证书、拓扑或节点运行步骤复制到产品层。

<a id="req-dcs-001"></a>
### REQ-DCS-001：单一 canonical history 与最终性

- Trace ID: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39
- 适用范围：本条要求及对应 AC-DCS-001，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[p2p 承接条款](../../p2p/design.md#des-p2p-receipt-finality)；P2P 提交证明已有承接；适用 profile 持久性、runtime apply 与消费者完整组合仍须核对。
- 实现状态：部分实现
- 实现与验证依据：正式读面发布校验 World snapshot root、world/chain/genesis、governing manifest 与 projection root；执行桥拒绝非连续 committed height。 [crates/oasis7/src/world_service/formal_read_service.rs L20-L49](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_service.rs#L20-L49)；[crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs L336-L438](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs#L336-L438)。测试定义：[crates/oasis7/src/world_service/formal_read_adapter_tests.rs L11-L74](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_adapter_tests.rs#L11-L74)。[相关历史交付/跟踪 #4683](https://github.com/nil-studio/oasis7/pull/4683)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计方面，P2P 提交证明已有承接；适用 profile 持久性、runtime apply 与消费者完整组合仍须核对；本轮实现与验证细化：合法 profile 的最终提交、跨故障域持久条件、非权威 peer/cache 冲突负例及 runtime/消费者同候选组合仍未完整核清；不把读面证明解释为 BFT。

同一 `world_id` 只能由适用合法 profile 的可验证正式提交证明及持久条件 推进唯一 canonical order；副本、缓存、非权威 peer 或未最终化输入不能成为玩家世界结果。

- 对应验收：[AC-DCS-001](#ac-dcs-001)。
- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。

<a id="ac-dcs-001"></a>
### AC-DCS-001：非权威材料不能代签

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；唯一要求声明：[REQ-DCS-001](#req-dcs-001)。

同一候选的服务节点、full node、light companion 和消费者只能依据验证过的合法 profile 提交证明及 hash-bound 材料得出世界状态；冲突、缺证或非权威写入被拒绝且不进入玩家可见历史。证据必须覆盖适用的 P2P/runtime 组合边界，transport 成功不能代签。

- 对应需求：[REQ-DCS-001](#req-dcs-001)。

<a id="req-dcs-002"></a>
### REQ-DCS-002：状态可用性与恢复链连续

- Trace ID: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4
- 适用范围：本条要求及对应 AC-DCS-002，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[p2p 承接条款](../../p2p/design.md#des-p2p-target-recovery)；恢复链已有目标设计；日志后缀正向续跑、真实故障域与重新可服务组合仍须核对。 补充局部依据：[Node PoS 局部恢复](../../p2p/blockchain/p2p-blockchain-p2pfs-hardening.design.md#hardening-recovery-continuity)（仅 startup reconciliation，非完整恢复链）。
- 实现状态：部分实现
- 实现与验证依据：driver_startup_recovery 与V3 record/CAS恢复实现已存在；回归定义覆盖metadata丢失、缓存删除、CAS缺失、冲突同高record、stale pointer及错误world。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver_startup_recovery.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/driver_authoritative_recovery.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。  本次存储子范围静态依据：[旧代保护源码](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7/src/runtime/world/persistence_support.rs#L641-L666)、[完整候选装载源码](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7/src/runtime/world/authoritative_recovery_generation.rs#L35-L80)、[重放控制状态源码](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7/src/runtime/world/event_processing.rs#L176-L363)及[现有非空后缀回归定义](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7/src/runtime/tests/persistence_recovery_tests.rs#L249-L301)；仅核对下列已知差距，源码/测试定义不等于本轮执行通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。 本次仅增补存储子范围核对：2026-10-17，固定源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；静态审读，未运行恢复、故障或部署测试，不刷新上述其他子范围的证据基线。
- 新增局部设计关系核对：2026-10-15；`4e94163a3dff9ba69f6959b406ca3fcedf8aa097` 与本次文档候选；仅核对上述新增局部设计关系，不新增行为验证，既有实现与运行证据基线不变。
- 新增局部设计依据（Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4）：[Node authoritative-state 拒绝启动](../../p2p/node/node-identity-replication-contract.design.md#des-nir-recovery)，由[专业恢复条款](../../p2p/node/node-identity-replication-contract.prd.md#nir-recovery)承接；仅 corrupt/unreadable PoS state 不默认重置的负向边界，不证明完整恢复链、非空日志后缀或新请求/模块行动/业务结算续跑。
- 本次局部关系静态核对：2026-10-15；`a0b6ab12ad14c6c881e48cecdb4adbdb322dd30e` 与文档候选；不改变上述系统设计/实现状态、既有执行证据或原 AC / test_tier_full。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，仅本地driver/CAS恢复子范围；真实多节点、合法profile激活/authority链、pruning/archive与新请求/模块行动正向续跑对照仍缺完整验收。 本地存储恢复仍有已披露实现缺口：[G2](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g2-design) 的旧代完整引用保护、[G3](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g3-design) 的完整候选坏代回退，以及 [J1 action allocator](../../world-runtime/runtime/runtime-numeric-safety.design.md#numeric-action-allocator-replay) / [J2 module schedule](../../world-runtime/runtime/runtime-integration.md#module-tick-journal-replay) 的非空 tail 接续控制状态。现有 staged/CAS 与 snapshot 恢复基础不证明这四项闭合；恢复可读不等于下一动作、模块唤醒与不中断对照一致，也不等于重新取得写入资格。 后续设计兼容与验收输入见 [Issue #4838](https://github.com/nil-studio/oasis7/issues/4838)；本处不另定存储方案。

恢复只能沿同一 `world_id` 的 固定 genesis 身份、合法 manifest/profile/epoch 激活链、已确认 checkpoint 证明、hash-bound snapshot、canonical replay 和 verified state root 建立连续历史；材料缺失、冲突、回退或指向其他世界时必须保持隔离或只读。

在提交权威、世界版本、权限、资源与输入条件相同且有效时，恢复可服务必须让合法后续工作保持与不中断运行一致的结果。较早合法检查点及其后完整、非空的权威事件历史必须足以延续新请求、已确认的模块行动时点和业务结算，不能只恢复可读历史而丢失继续运行的能力。

- 对应验收：[AC-DCS-002](#ac-dcs-002)。
- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`node triad 运维与可观测性`](../../p2p/node/node-triad-operations-observability.prd.md#inventory-与采样合同)、[`public-testnet governed bootstrap runbook`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#stable-authority-and-evidence-boundary)、[`state-sync closure evidence packet`](../../testing/templates/state-sync-closure-evidence-packet-template.md#topology-and-node-truth)。
- 证据层级与 owner：`test_tier_full`；`blockchain_ops_engineer` 负责同一证据窗口的 topology/inventory、`/healthz` 与 `/v1/chain/status`、peer-head/replication、state-sync closure 及 restore/rollback 事实，`runtime_engineer` 负责 replay/state-root 连续性，QA 负责 verdict。缺少任一适用证据时保持隔离或只读，不得解释为 readiness。

<a id="ac-dcs-002"></a>
### AC-DCS-002：分区、重启与恢复不产生第二世界

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；唯一要求声明：[REQ-DCS-002](#req-dcs-002)。

分区、重启、落后追赶、snapshot/state sync 或 pruning 样例能证明同一 `world_id` 的唯一顺序、可重建性和 state-root 连续；任一恢复材料不匹配时停止服务/投票或降为明确只读，不能把替代 endpoint、缓存或本地世界包装成原世界恢复。

恢复验收还覆盖根 [`prd.md`](prd.md) 的 SC-7：恢复只读、恢复可服务、恢复受阻/隔离和闸门回退。相同 `world_id` 的历史验证链完整只允许恢复只读；只有当前追加/最终性、版本化执行和 head 单调连续性的专业证据全部重新成立，才可恢复可服务。其余状态下新 intent 的 committed receipt 必须为 `0`；进入可服务后，既有 pending 按当前有效条件和 canonical 顺序重新裁决，已确认 receipt 保持不变。消费者能读到当前恢复语义、主要 blocker 和真实下一步。测试层级：`test_tier_full`。

正向续跑样例固定同一世界的有效提交权威、版本、权限、资源和有序输入，对比不中断运行与从较早合法完成边界的检查点加其后非空事件历史恢复的两条分支；在同一目标边界继续提供相同输入，并分别判定：

- **新请求连续性**：接受或拒绝的结果和业务结算与对照一致；至少一条满足全部执行前置的合法新配方正常完成一次，其历史任务与本次任务可区分，不能因恢复而误拒绝、覆盖历史或重复结算。
- **模块行动连续性**：已确认的等待、暂停及下一次行动时点与对照一致，不提前唤醒、不漏掉应有行动；恢复读取原有执行结果与费用，不重新执行历史模块调用或重复计费。恢复后的新调用仍按其当时合法条件正常计费和结算。

两项独立判定，均须通过；仅目标边界状态相等、“至多一次”、完整目标快照保存/加载成功或重执行已提交输入均不能替代从非空事件历史恢复后继续运行的结果。此处是受控有效条件下的确定性验收，不新增完成时限或无条件成功承诺。事件恢复的专业边界见 [runtime 日志恢复合同](../../world-runtime/prd.md#runtime-journal-suffix-recovery)，具体场景由 [QA 正向续跑设计](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-journal-suffix-recovery) 承接；条款采纳不表示实现或运行验证完成。

- 对应需求：[REQ-DCS-002](#req-dcs-002)。

- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`node triad 运维与可观测性`](../../p2p/node/node-triad-operations-observability.prd.md#权威边界)、[`public-testnet governed bootstrap runbook`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#4-hard-rules)、[`state-sync closure evidence packet`](../../testing/templates/state-sync-closure-evidence-packet-template.md#peer-heads-and-gap-sync)。
- 证据层级与 owner：`test_tier_full`；`blockchain_ops_engineer` 负责同一窗口的 topology/inventory、health/status、peer-head/replication、state-sync 与 restore/rollback 事实，`runtime_engineer` 负责 replay/state-root 连续性，QA 负责 verdict。缺少任一适用证据时保持隔离或只读，不得解释为 readiness。
- 运维证据边界：[`public-testnet governed bootstrap runbook`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#4-hard-rules) 的 deployment truth 与恢复约束，以及 [`state-sync closure evidence packet`](../../testing/templates/state-sync-closure-evidence-packet-template.md#peer-heads-and-gap-sync) 的 peer-head、gap sync、observer catch-up 与 blob closure 字段；产品层只消费同一窗口的结果，不复制运行步骤。

<a id="req-dcs-003"></a>
### REQ-DCS-003：共识权限与服务角色隔离

- Trace ID: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3
- 适用范围：本条要求及对应 AC-DCS-003，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[p2p 承接条款](../../p2p/design.md#des-p2p-layer-authority)；角色隔离已有设计；各存储角色、pruning 冗余和运行暴露面的完整范围仍须核对。 补充局部依据：[identity-v2 域隔离](../../p2p/blockchain/public-testnet-identity-v2-signing.design.md#identity-v2-role-isolation)、[签名/复制准入](../../p2p/blockchain/p2p-blockchain-p2pfs-hardening.design.md#hardening-signature-boundary)、[registry-first](../../p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-registry-isolation)（均为局部承接，不证明完整角色暴露面或 BFT）。
- 实现状态：部分实现
- 实现与验证依据：NodePos attestation 从已配置 validator stake map 取权重，未知 validator 拒绝且重复投票不重复计权。 [crates/oasis7_consensus/src/node_pos.rs L99-L169](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_consensus/src/node_pos.rs#L99-L169)；[doc/p2p/design.md L24-L56](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/p2p/design.md#L24-L56)。测试定义：[crates/oasis7_proto/src/distributed_finality_tests.rs L211-L340](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_proto/src/distributed_finality_tests.rs#L211-L340)。未核到可精确归属本条全部义务的实际交付 PR/Task；不虚构。；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 新增局部设计关系核对：2026-10-15；`4e94163a3dff9ba69f6959b406ca3fcedf8aa097` 与本次文档候选；仅核对上述新增局部设计关系，不新增行为验证，既有实现与运行证据基线不变。
- 新增局部设计依据（Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3）：[Node 条件 signer binding](../../p2p/node/node-identity-replication-contract.design.md#des-nir-signer)及其[专业接受](../../p2p/node/node-identity-replication-contract.prd.md#nir-signer-binding)，仅启用配置/签名模式后拒绝冒充受治理 validator；[local signer 权限边界](../../p2p/blockchain/local-file-signing-backend.design.md#des-local-signer-install)，仅安装、OS caller、grant 与通用文件签名不自授链/governance 权限。未配置 map、完整角色/暴露面/注册轮换/BFT 及真实主机组合仍未证明。
- 本次局部关系静态核对：2026-10-15；`a0b6ab12ad14c6c881e48cecdb4adbdb322dd30e` 与文档候选；不改变上述系统设计/实现状态、既有执行证据或原 AC / test_tier_full。
- 剩余范围：设计方面，角色隔离已有设计；各存储角色、pruning 冗余和运行暴露面的完整范围仍须核对；本轮实现与验证细化：治理注册/轮换、被攻破 service role 不扩大权限、sentry/relay/full/archive/RPC 暴露面与无公网 IP 参与的完整目标及同窗口运维证据未核清；配置集合局部约束不代表完整角色隔离。

验证者集合及其轮换保有受治理的共识权威；sentry、relay、full/state-sync/archive、RPC/proof gateway 等非权威服务只能提供传播、存储或证明材料，不能通过暴露面、缓存或服务被攻破取得最终写入权。

目标态参与边界：公网 IP 不是受治理 validator 或 permissionless full/storage/observer service role 参与网络的通用前置条件；治理 admission/activation 仅适用于 validators，full/storage/observer 等服务角色按本地 role policy 和技术可达性参与。该产品目标对应 [P2P reachability PRD §2 的首项验收标准](../../p2p/network/mainnet-private-reachability-architecture.prd.md#reachability-user-experience)，不表示当前软件支持、已有部署能力，也不证明 mainnet 或 release readiness。

- 对应验收：[AC-DCS-003](#ac-dcs-003)。
- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`node triad 运维与可观测性`](../../p2p/node/node-triad-operations-observability.prd.md#权威边界)、[`public-testnet governed bootstrap runbook`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#stable-authority-and-evidence-boundary)。

<a id="ac-dcs-003"></a>
### AC-DCS-003：非权威节点不能扩大权限

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；唯一要求声明：[REQ-DCS-003](#req-dcs-003)。

验证者注册/轮换、网络暴露和服务角色的组合样例证明非权威节点不能推进 canonical history、签发最终性或代替受治理验证者完成高影响状态变化；被攻破的服务节点只能导致隔离/缺证等可观察结果，不能产生权威写入。

- 对应需求：[REQ-DCS-003](#req-dcs-003)。

- 证据层级与 owner：`test_tier_full`；`blockchain_ops_engineer` 负责同一 topology/inventory 与 health/status 窗口中的 validator/service-role 隔离、网络暴露和停止 serving/voting 的运维证据，P2P 保有权限/最终性语义，QA 负责 verdict。该映射只证明 fail-closed 结果，不证明部署或发行 readiness。

<a id="req-dcs-004"></a>
### REQ-DCS-004：commit certificate 的安全推进边界

- Trace ID: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46
- 适用范围：本条要求及对应 AC-DCS-004，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[p2p 承接条款](../../p2p/design.md#des-p2p-target-bft)；BFT 拒绝边界已有目标设计；锁定、round、验证者转换及对抗性恢复仍未形成完整证明。
- 实现状态：未实现
- 实现与验证依据：目标BFT证书推进合同未实现。补核当前声明入口与feature：NodeRuntime::start无条件实例化PosNodeEngine，NodeConsensusMode仅Pos；node features仅libp2p/test-hooks，consensus crate无feature分支；另一公开QuorumConsensus为人数多数helper，SequencerMainloop显式PosConsensus，均无目标round/phase/QC。与P2P authority明确当前prototype缺QC/round/复验、lock/timeout未实现的正面声明一致，足以判目标合同未实现，而非搜索未找到。 当前生产链已核为 slot/epoch 的 stake-weighted proposal/attestation prototype：阈值decision驱动执行、持久化与committed snapshot，消息签名/identity/root检查不等于BFT precommit证书。独立 WorldFinalityProofV1 已有 Ed25519、active-set/stake 去重及治理授权转换验证，CLI验证成功才输出pass；未见它接入canonical commit推进。历史Actions job 112266509032有22个具名finality测试通过，但整体job在final task/PR binding处失败；对应历史提交34def83f6的finality实现/测试与6463975 remote blob相同，只证明该有界验证器切片，不是当前全main或目标BFT验收。 定位：[证据1](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_proto/src/distributed_finality.rs#L551-L925)；[证据2](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/node_engine_core.rs#L216-L275)；[证据3](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_consensus/src/node_pos.rs#L170-L295)；[证据4](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_proto/src/distributed_pos.rs#L13-L62)；[证据5](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_proto/src/distributed_finality.rs#L207-L220)；[证据6](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_proto/src/distributed_finality.rs#L925-L1067)；[证据7](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_proto/src/bin/oasis7_world_head_proof_verify/oasis7_finality_proof.rs#L100-L189)；[证据8](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/node_engine_network.rs#L959-L1056)；[证据9](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/pos_engine_gossip.rs#L339-L386)；[证据10](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/node_engine_core_tick_impl.rs#L266-L469)；[证据11](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/node_engine_core.rs#L571-L594)；[证据12](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/node_engine_core.rs#L659-L814)；[证据13](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/lib.rs#L691-L761)；[证据14](https://github.com/nil-studio/oasis7/actions/runs/37462864200/job/112266509032)；[证据15](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/types.rs#L478-L481)；[证据16](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/src/lib.rs#L292-L331)；[证据17](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_node/Cargo.toml#L10-L13)；[证据18](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_consensus/src/lib.rs#L1-L54)；[证据19](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_consensus/src/quorum.rs#L26-L120)；[证据20](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_consensus/src/quorum.rs#L721-L738)；[证据21](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/crates/oasis7_consensus/src/sequencer_mainloop.rs#L126-L151)；[证据22](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/doc/p2p/prd.md#L121-L125)；[证据23](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/doc/p2p/prd.md#L227-L229)；[证据24](https://github.com/nil-studio/oasis7/blob/6463975178f3d32ef6d8e19e14f0b24f1c7f9471/doc/p2p/design.md#L95-L99)。
- 核对日期与基线：2026-10-17；固定源码 6463975178f3d32ef6d8e19e14f0b24f1c7f9471；只读源码与历史Actions，未重跑/部署/调用模型。
- 剩余范围：待实现目标Propose/Prevote/Precommit独立round/timeout、锁定/解锁、严格>2/3 active治理stake证书，并将验真结果绑定canonical提交/执行/复制/checkpoint/消费者与恢复；补齐equivocation/缺证/错集/round故障均无权威历史效果的同候选验收。prototype和独立finality verifier已实现的能力及历史22具名测试PASS保留，不能替代该目标。

目标 BFT commit 只有在活动、治理注册的验证者集合满足产品要求的最终性证据时才能推进；equivocation、缺证、错误验证者集合或 round 故障不得产生部分或可见权威历史。

- 对应验收：[AC-DCS-004](#ac-dcs-004)。
- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。

<a id="ac-dcs-004"></a>
### AC-DCS-004：错误证明和 round 故障 fail closed

Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；唯一要求声明：[REQ-DCS-004](#req-dcs-004)。

BFT 实现样例证明符合产品最终性条件的 commit certificate 才能推进历史；equivocation、缺证、错误验证者集合和 round 故障全部被拒绝或保持无效果，不以 prototype 状态宣称完整 BFT readiness。具体阈值、签名和 round 状态由 P2P authority 定义。

- 对应需求：[REQ-DCS-004](#req-dcs-004)。

<a id="req-dcs-005"></a>
### REQ-DCS-005：消费者可读的可用性与 fail-closed

- Trace ID: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b
- 适用范围：本条要求及对应 AC-DCS-005，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[p2p 承接条款](../../p2p/design.md#des-p2p-target-service-gate)；服务闸门已有目标设计；结构化附件、正向续跑、同窗口故障与消费者完整范围仍须核对。
- 实现状态：部分实现
- 实现与验证依据：formal HTTP 准入在 ready=false 时于入队前拒绝，degraded 仅保留最后 qualified CommitRef 的精确只读；writer 经正式 proof 校验后才推进并发布 cut。 [准入与只读](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_service.rs#L594-L634)、[准入](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_service.rs#L784-L861)、[提交发布](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_writer.rs#L965-L1081)。 [C06 定义](https://github.com/nil-studio/oasis7/blob/bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_protected_availability_tests.rs#L38-L226)覆盖副本失联、原 GameTaskCreate 恢复及重试同结果；测试 ignored、本轮未运行，不是 full 或合法新配方通过；本轮未核清这些实现的精确交付 PR/Task，[#4838](https://github.com/nil-studio/oasis7/issues/4838) 仅承接剩余恢复验收。
- 核对日期与基线：2026-10-17；需求与实现语义均读取基线 `bab8990cef3b42fe5a0a5538b5dd0c80ca3b19f5`；仅核源码和测试定义，未运行；验收原文及测试层级不变。
- 剩余范围：设计方面，服务闸门已有目标设计；结构化附件、正向续跑、同窗口故障与消费者完整范围仍须核对。 实现与验证：四级转换与证明冲突隔离、全部非 serving 等级及跨入口逐 intent receipt=0、恢复 pending 当前条件/期限/优先级重审、新请求与模块行动不中断对照和合法新配方、三消费者 blocker/next-step 与 DC-5 同候选结构化附件及 test_tier_full 执行仍未核实。ready/503、旧 cut 可读或 C06 定义均不代签完整合同。

只有“已验证可服务”同时满足 `world_id`、checkpoint/head、hash-bound 状态、finality/追加条件、manifest compatibility 和单调 head continuity 时，消费者才可提交新的权威 intent；其他等级必须保持只读、无效果待决或不可用/隔离，并说明 blocker 与下一步。

- 对应验收：[AC-DCS-005](#ac-dcs-005)。
- 专业域权威：[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、[`node triad 运维与可观测性`](../../p2p/node/node-triad-operations-observability.prd.md#inventory-与采样合同)、[`public-testnet governed bootstrap runbook`](../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#stable-authority-and-evidence-boundary)、[`state-sync closure evidence packet`](../../testing/templates/state-sync-closure-evidence-packet-template.md#peer-heads-and-gap-sync)。

<a id="ac-dcs-005"></a>
### AC-DCS-005：状态转换不伪造世界效果

Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；唯一要求声明：[REQ-DCS-005](#req-dcs-005)。

同一候选覆盖“已验证可服务 -> 陈旧/追赶中 -> 已验证只读 -> 已验证可服务”和证明冲突进入“不可用/隔离”的转换；非可服务等级的新 intent 的 committed receipt 为 `0`，正式消费者能读到当前等级、受影响操作、最后可信边界、blocker 和真实下一步。重连、重复提交和跨入口重试不产生第二次世界效果。

重新进入可服务的正向结果须同时满足 [AC-DCS-002](#ac-dcs-002) 的新请求连续性与模块行动连续性：固定有效条件下与不中断运行对照结果一致，且至少一条合法新配方正常完成一次。新 intent 的 receipt 上限为 `1` 仍须遵守，但全部无效果、误拒绝或丢失应有行动不能作为恢复可服务的通过证据。

- 对应需求：[REQ-DCS-005](#req-dcs-005)。

- 证据层级与 owner：`test_tier_full`；`blockchain_ops_engineer` 负责 topology、inventory、`/healthz`、`/v1/chain/status`、peer-head/replication 与 state-sync/恢复窗口的事实捕获，runtime/P2P 负责执行、manifest/head 与 finality 语义，world-simulator 负责消费者投影，QA 负责 verdict。health endpoint、transport green 或一次重连不能单独升级可服务状态。

## 7. 组合验收

- DC-1：任何服务、full node 或 light companion 都只能从当时合法 profile 的已验证正式提交证明和 hash-bound 材料得出世界状态；非权威 peer/缓存/快照不能代签。
- DC-2：分区、重启、落后追赶、恢复和 pruning 样例证明相同 `world_id` 的唯一顺序、可重建性与 state-root 一致；[AC-DCS-002](#ac-dcs-002) 的新请求与模块行动连续性分别通过正向续跑对照；不满足证据时停止服务或投票。
- DC-3：验证者注册/轮换、网络暴露和服务角色不扩大非权威节点的共识权限。
- DC-4：BFT 实现样例证明超过三分之二活动质押预提交形成可验证 commit certificate，且 equivocation、缺证、错误验证者集合和 round 故障均不得推进权威历史。
- DC-5：同一候选至少覆盖“已验证可服务 -> 陈旧/追赶中 -> 已验证只读 -> 已验证可服务”以及证明冲突进入“不可用/隔离”的转换；仅在已验证可服务且当前 governing manifest/compatibility 与单调 head continuity 证据均成立时接受新的权威 intent，其他等级或版本/连续性证据缺失、冲突时新 intent 的 committed receipt 数为 `0`。陈旧内容带有最后可信边界与 blocker，不被表述为当前权限、价格或成功；冲突/替代世界不被当作原世界恢复；重连、重复提交和跨入口重试不产生第二次世界效果。正式消费者能读到当前等级、受影响操作与真实下一步。测试层级：`test_tier_full`。证据追踪必须将根产品 PRD 的 SC-7 恢复闸门与 [`Game World State Sync and Commit Closure` 计划](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md) 作为执行 lane，将 [`state-sync closure evidence packet` 模板](../../testing/templates/state-sync-closure-evidence-packet-template.md) 作为 evidence envelope，并在同一候选的 linked structured attachment/schema 中提供状态转换、manifest/head 负例、每个 intent 的 receipt `0/1` 与消费者 blocker/next-step 字段；缺少该 attachment/schema 或任一字段时，DC-5 不得判定通过。

## 8. Non-Goals

- 不定义区域设施、市场、工业、charter、frontier、普通治理或玩家资源经济；这些是世界规则与玩法系统模块的产品语义。
- 不定义 deterministic world runtime 的规则解释；该上层基础子层由本模块的执行专题和 `doc/world-runtime/` 专业权威承载。
- 不定义共识/存储/网络实现或当前运维、发布与公开状态。

## 未决问题与追踪解释

准确专业设计及 QA 场景回链见下表；[历史文档接收任务 #4180](https://github.com/eng-cc/oasis7/issues/4180)记录当时三层接收与文档检查，不能代替实现、semantic verdict 或 full 执行证明。设计/接收可定位的问题可据此复核，仍缺的当前能力、同候选证据与真实环境保持未证明；专业 owner 与 QA 的判定权不变。

当前追踪表提供产品条款到专业 authority 的导航；重复的模块入口、泛化的“可导航追踪证据”和产品 owner 不能证明准确的专业接受、系统设计与验证场景已经全部闭合。产品 owner 拥有产品结果，P2P 拥有最终性、权限与 freshness，runtime 拥有 manifest/head、服务闸门和 receipt，运维拥有同窗口事实，消费者 owner 拥有投影，QA 拥有组合 verdict。准确承接及当前能力事实仍须分别核对。

| 未决问题 / 影响 | 决策 role 与所需信息 | 解决触发与临时边界 |
| --- | --- | --- |
| DCS-001/004 的最终性、证书和 round，以及 DCS-002/003 的恢复链、存储与角色隔离由哪些真实设计条款承接？ | P2P 技术 owner、runtime、运维与 QA；需要每项义务的接受条款、设计条件及场景定位 | 专业设计条款获批准后回链；此前不把整份 PRD 导航或目标描述当作完整设计覆盖 |
| DCS-005/DC-5 的转换、manifest/head 负例、receipt 0/1 和消费者反馈由哪个结构化附件合同判定？ | runtime/P2P、运维、消费者 owner 与 QA；需要同候选字段、证据窗口和明确 verdict | 验证设计与附件可定位后复核；此前 state-sync 模板、health 或重连成功均不构成通过证据 |
| DCS-001/004 表中的 required 局部检查与根 SC-1/SC-2 等 full 组合证据如何分工？ | P2P/runtime 与 QA；需要独立义务覆盖和适用环境说明 | 验证设计明确两层范围后确认；局部结果不降低根组合条件，DCS-003/005 仍按正文 full 验收 |

问题的历史接收来源为[已关闭的文档迁移协调任务 #3935](https://github.com/eng-cc/oasis7/issues/3935)；该任务已关闭，不恢复其当前义务；任务依赖和执行状态由 Git、PR 与实际 CI 记录维护，不在长期正文复制进度；解决前不新增未来路径或虚构任务。下面的“验证证据”栏尚包含导航描述，除非有同候选的实际执行证据，不能解释为测试已存在或已通过。

## 全量语义追踪

DC-1..5 与各 REQ/AC 分别保留独立义务。恢复与服务 feedback 的消费者回链为 [`Agent 消费`](../../world-simulator/llm/continuous-agent-harness.prd.md#harness-infrastructure-consumer-matrix)、[`Agent join`](../../world-simulator/llm/continuous-agent-harness.design.md#harness-infrastructure-evidence-join)、[`Viewer 读回`](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract)、[`Viewer 捕获`](../../world-simulator/viewer/viewer-manual.manual.md#viewer-infrastructure-capture-receiver)、[`Viewer join`](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join)；DC-5 attachment/schema shape 不代替 semantic、trust、domain 或环境检查。

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-001](#req-dcs-001) / [AC-DCS-001](#ac-dcs-001) | `producer_system_designer` |[`doc/p2p/prd.md`](../../p2p/prd.md#目标)（共识最终性、证明与 freshness）、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)（版本化执行、manifest/head、pending、receipt 与去重）、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)（消费者/Viewer/入口状态反馈）、[`doc/testing/prd.md`](../../testing/prd.md)（组合验证与证据）；[`P2P 设计`](../../p2p/design.md#des-p2p-ordered-commit)、[`P2P 设计`](../../p2p/design.md#des-p2p-receipt-finality)、[`runtime 设计`](../../world-runtime/design.md#runtime-deterministic-design)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-FINALITY`，独立承接 DC-1；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_required` |
| [REQ-DCS-002](#req-dcs-002) / [AC-DCS-002](#ac-dcs-002) | `producer_system_designer / blockchain_ops_engineer / runtime_engineer / viewer_engineer / qa_engineer` |[`doc/p2p/prd.md`](../../p2p/prd.md#目标)（共识最终性、证明与 freshness）、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)（版本化执行、manifest/head、pending、receipt 与去重）、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)（消费者/Viewer/入口状态反馈）、[`doc/testing/prd.md`](../../testing/prd.md)（组合验证与证据）；[`P2P 设计`](../../p2p/design.md#des-p2p-target-recovery)、[`runtime 设计`](../../world-runtime/design.md#runtime-recovery-design)、[`日志恢复与续跑设计`](../../world-runtime/design.md#runtime-journal-suffix-recovery-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-RECOVERY`，独立承接 DC-2；SC-3/6/7；[正向续跑判定](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-journal-suffix-recovery) 分别验证新请求身份与模块行动时点延续、同条件对照结算一致且合法新配方完成一次；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_full` |
| [REQ-DCS-003](#req-dcs-003) / [AC-DCS-003](#ac-dcs-003) | `producer_system_designer` |[`doc/p2p/prd.md`](../../p2p/prd.md#目标)（共识最终性、证明与 freshness）、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)（版本化执行、manifest/head、pending、receipt 与去重）、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)（消费者/Viewer/入口状态反馈）、[`doc/testing/prd.md`](../../testing/prd.md)（组合验证与证据）；[`P2P 设计`](../../p2p/design.md#des-p2p-layer-authority)、[`P2P 设计`](../../p2p/design.md#des-p2p-replication-scope)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-FINALITY`，独立承接 DC-3；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_full` |
| [REQ-DCS-004](#req-dcs-004) / [AC-DCS-004](#ac-dcs-004) | `producer_system_designer` |[`doc/p2p/prd.md`](../../p2p/prd.md#目标)（共识最终性、证明与 freshness）、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)（版本化执行、manifest/head、pending、receipt 与去重）、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)（消费者/Viewer/入口状态反馈）、[`doc/testing/prd.md`](../../testing/prd.md)（组合验证与证据）；[`P2P 设计`](../../p2p/design.md#des-p2p-target-bft)、[`runtime 设计`](../../world-runtime/design.md#runtime-deterministic-design)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-FINALITY`，独立承接 DC-4；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_required` |
| [REQ-DCS-005](#req-dcs-005) / [AC-DCS-005](#ac-dcs-005) | `producer_system_designer` |[`doc/p2p/prd.md`](../../p2p/prd.md#目标)（共识最终性、证明与 freshness）、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)（版本化执行、manifest/head、pending、receipt 与去重）、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)（消费者/Viewer/入口状态反馈）、[`doc/testing/prd.md`](../../testing/prd.md)（组合验证与证据）；[`P2P 设计`](../../p2p/design.md#des-p2p-target-service-gate)、[`runtime 设计`](../../world-runtime/design.md#runtime-recovery-design)、[`日志恢复与续跑设计`](../../world-runtime/design.md#runtime-journal-suffix-recovery-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-semantic-validator)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-RECOVERY / V-CONSUMER`，独立承接 DC-5；[正向续跑判定](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-journal-suffix-recovery) 独立于 receipt 上限证明同条件下可正常继续；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_full` |
