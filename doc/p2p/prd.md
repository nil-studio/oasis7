# p2p PRD

## P2P 专业承接与设计分配

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；审读 source baseline：f9d5a552d9af04c1b1398262808198a58e560230（2026-09-26）。本文的 current 指冻结源所记专业合同；本次未独立验证实现、部署或测试通过。target 是规范目标，historical 是 MIG/CCG/TASK 与 dated evidence provenance。GitHub Issue/Project 维护实际任务与候选证据。新增稳定条款是原义务的细化入口；原章节/常量/命令/失败边界仍有效。

能力边界按子范围读取：native formal controlled 提交、受保护 signer 接口、原输入恢复及有界只读 full-state sync 已有代码接线，范围见[持久节点手册](blockchain/persistent-world-node-lifecycle.runbook.md#正式合同层与准入边界)和[本机生命周期手册](blockchain/local-node-lifecycle.runbook.md)；真实独立故障域与完整部署验收仍须另证。下述正式 BFT、动态集合安装和长期 checkpoint 接续是待实现目标，不能用旧 prototype、文档采纳或上述有界能力代替。

<a id="p2p-authority-profiles"></a>
### 版本化提交权威与同世界连续性（目标）

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[REQ-DCS-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)：仅合法 profile 的证明与持久条件产生权威历史。
- Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[REQ-DCS-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004)：BFT profile 的证书与验证者集合边界。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

正式世界按创世或已提交升级规则合法激活的 authority profile 验证提交证明，不能把兼容单签、threshold-1 或 stake-threshold prototype 原地改名为正式权威。

| Profile | 提交决定与信任边界 | 激活与验收 |
| --- | --- | --- |
| `controlled_single_authority` | 一个受控权威决定提交；证明历史来源、顺序与连续性，正确性信任包含运营权威，不证明独立验证者共识。 | 首批持久世界开放前须闭合唯一追加权、跨故障域持久确认、幂等、证明消费与同世界恢复。 |
| `bft` | 合法活动验证者按完整协议形成证书；证明适用 Byzantine 故障模型内的共识安全与提交。 | 必须通过 [BFT 证书闸门](#p2p-target-bft) 和同世界交接验收后激活。 |

两种 profile 复用同一动作、确定性执行、状态、receipt 与恢复协议。提交证明至少绑定固定 `world_id`、`chain_id`、`genesis_hash`、高度/顺序、parent、authority epoch、profile 版本、runtime manifest，以及输入、执行结果和状态承诺。固定身份锚点不限制合法软件、manifest 或 authority 演进；变化必须经既有授权规则在同一历史接续，不得重发资产、删除旧结果或以新创世伪装恢复。建立长期世界前先核对既有保留承诺；有既有世界时沿用其合法身份和历史。隔离 local/dev 使用另一身份，不能并入。

<a id="p2p-single-authority-durable-commit"></a>
### 单权威正式提交与恢复闸门（目标）

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[REQ-DCS-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)：正式提交前的唯一追加权、证明与持久条件。
- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)：同世界恢复链及已确认结果连续性。
- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[REQ-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)：共同提交与失败不产生部分世界效果。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

- **唯一追加权**：最终日志/存储原子校验合法 writer epoch、预期 parent/head 与追加顺序；signer 仅对合法 epoch 且达到提交条件的记录签发证明；完整 prepare 必须先持久确认，正式决定/证明生成后还须跨故障域持久确认才成为可对外认可的 committed 证明。本机进程锁与 `SingleWriterReplicationGuard` 的单调校验不足以隔离旧写者。恢复/人工切换先隔离旧 signer 与旧存储追加权，再从合法 head 推进授权 epoch；隔离无法证明时保持停写。
- **持久性先于 committed 回应**：初期故障模型为单主机或单存储故障域失效后已确认提交不丢失。回应前本机及独立故障域持久保存同一完整提交记录、正式提交决定/证明、head/epoch、重放输入与结果、幂等身份和引用版本/工件。独立副本未确认时暂停 committed 确认，异步快照不能替代此条件；多个独立故障域同时毁损不在该保证内。
- **准备与提交决定分开**：所有副本按同一持久提交判定规则恢复；预备记录不产生权威效果，已生效 epoch 不回退。超时/断连且不能证明未提交时保留“提交状态未知、待核对”，按原请求身份查询或完成原提交；只有证明尚无效果且已终止才判失败，不能作为新动作重试。
- **实际开放路径共同提交**：输入扣除、产出、事件、执行结果、持久里程碑及去重事实共同提交；回复丢失、恢复与 replay 不产生第二效果。外部 effect 按 runtime 提交后派发与幂等合同执行，replay 不重新调用 LLM 或重复派发。
- **同世界恢复**：固定身份 → 各历史高度合法 profile/epoch 与 manifest 激活链 → 已确认 checkpoint → hash-bound snapshot → 连续 committed log replay → state-root 核验。核对资产、Agent 归属、有效授权、未完成目标、pending、nonce/幂等和版本；回执列表裁剪不能删除持久里程碑依据。先提供验证只读；当前权威/head/版本/持久性全成立才恢复写入。

首次开放证据必须在副本写入中、正式证明/提交决定写入中、回应 committed 前分别注入故障，并实证单故障域恢复 root/receipt 对账与旧 writer 拒绝。普通单测不能替代实际故障域演练；本段是 target，不宣称现有 prototype 已满足。

P/R 是两个持久化端点，不是两名 BFT 验证者；不同 role、key、地址或目录只证明逻辑区分。部署须给出与声明故障模型一致的实际主机、存储、回滚及密钥托管边界，并在 P、R 任一域真实失效后恢复所有已确认 CommitRef、root 与原请求结果。单域容灾保证是已确认结果不丢失，不承诺缺一端时继续写入：幸存材料先提供验证只读，双端持久条件、旧写者隔离、单调 floor 和合法接续重新成立后才可写。签名 provider 或 issuer witness 无法证明不可导出、不可回滚时，保持 `FENCING_UNPROVEN` / recovery blocked，不以本机 mock 代签。场景与证据主责见[故障域验收](design.md#des-p2p-controlled-fault-domain-acceptance-v1)。

后继设计把这些义务具体落为 [原 candidate 恢复 v1](design.md#des-p2p-original-candidate-recovery-v1)、
[正式激活与消费 v1](design.md#des-p2p-formal-controlled-authority-v1) 和
[人工 epoch 接管 v1](design.md#des-p2p-controlled-epoch-takeover-v1)。完整 candidate 落盘前
的恢复必须先核对原持久决定，再以执行前双端保存的原输入/版本/nonce 重建，不能重新
admission 或替换身份；只有尚无固定决定的 pending 才按恢复后的当前条件重审。
人工接管先隔离旧进程签写权限，由创世预授权且限定原请求的恢复器排空旧 epoch，再封旧
epoch 并激活唯一后继；恢复 issuer 不获任意业务或 BFT 回退权。
正式证明需贯通普通 Describe、Submit、Lookup、ReadView 和 Changes，不能仅移除工程 scope。
这些是规范协议及[验收计划](design.md#val-p2p-controlled-authority-v1)；已有本机代码接线的
实际范围按上述手册核对，不能整体记为未实现，也不能由此判为部署通过。现有工程入口
拒绝边界、未实施子范围与实际部署 hold 保持有效。

<a id="p2p-authority-handoff"></a>
### 单权威到 BFT 的 H/H+1 交接（目标）

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[REQ-DCS-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)：同世界合法 profile 的唯一历史与不可双写。
- Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[REQ-DCS-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004)：交接后的 BFT 证明条件；不由交接计划代替证书。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

先接入验证固定身份、历史 profile、重执行和 state root 的只读节点；它们尚无写权/投票权。

BFT 完成独立验证者重执行、持久证书、round/锁定/超时、重启、分区和集合转换后，通过计划停写窗口交接：

1. 排空在途提交并保留 pending 原请求身份；新集合接受 H−1 的 checkpoint/状态根并持久保存切换材料。
2. 旧合法 authority 在 H 提交唯一交接记录，绑定交接前 checkpoint/root、新 validator set、profile 版本、epoch 和 H+1 生效边界；H 的正式证明另绑定执行交接记录后的结果根，新集合验证接受记录、证明与结果根。
3. 存储、入口、signer 与全部 proof verifier 在 H+1 及以后只接受新 profile，隔离旧写入与签名通道；新集合从 H+1 产生首个有效 BFT 提交后恢复写入。

H 正式提交前可以取消计划；H 一旦按旧 profile 正式提交，接续权不可撤回，旧 profile 的合法提交范围截止 H。即使 H+1 尚无 BFT 提交，重启、取消请求、旧写者隔离或新集合启动失败都只能保持停写，不能恢复旧单权威。记录、结果根、集合和边界须满足既定持久条件。

旧历史仍按当时 profile 验证，不追认为独立验证者共识。pending 恢复按当前有效权限、资源、manifest 重新裁决。

同候选验收覆盖 H 已提交而 H+1 未提交时宕机/取消，重启仍拒绝旧 profile 在 H+1 追加；并证明无双写、断链、身份重置、资产重发或失效 pending 自动生效。当前运行状态仍由 GitHub task 与候选证据决定。

<a id="p2p-ordered-execution"></a>
### 有序动作与执行绑定

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[REQ-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)：同一有序输入、父状态与版本的执行绑定及错误原子拒绝。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

admission 身份/认证、非零 identity、payload/queue 上限；同一确定有序序列贯穿 action_root、proposal/commit、execution context；下一 committed height 才可执行；decode/root/journal/proof mismatch 阻断并显式失败或 requeue。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="p2p-receipt-finality-boundary"></a>
### 执行 receipt 与最终性

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；[REQ-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-001)：活动验证者重执行与结果绑定。
- Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39；[REQ-DCS-001](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-001)：receipt 不替代适用正式提交证明。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

receipt 不是 authority commit proof；`bft` 下活动 validator 在投票前按同一 parent/manifest/动作重执行并核对 action/execution/event/state roots。工件/fault/root 错误拒绝；旧 threshold-1 TickCertificate 仅兼容诊断，不能推进 committed/replication/checkpoint/light-final。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="p2p-replication-recovery"></a>
### 复制和权威恢复

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)：同世界恢复中 head、checkpoint、journal 与 root 连续，失败保持隔离；有界存储取回仅供应材料，不提升权威。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

同 world 单调 head；writer epoch/sequence 单调且切换从1开始；鉴权/hash/写入失败不污染 guard；权威恢复 root/journal/checkpoint/proof 不匹配停止。严格 DHT fetch 与 root storage-challenge 的有界 availability fallback 是不同调用合同。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="p2p-target-bft"></a>
### 目标 BFT 证书闸门

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46；[REQ-DCS-004](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004)：活动验证者、阈值、round 与证书失败边界。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

首个正式协议采用[系统设计指定的 Tendermint 算法与状态机](design.md#des-p2p-bft-round-lock-v1)，由 oasis7 拥有应用语义和实现；不要求外部进程或特定语言依赖。每个高度的治理活动集合固定，`W` 为其有效总 voting power，证书必须有 `floor(2W/3)+1` 的去重有效 precommit 权重；恰好 `2W/3` 不通过。安全条件为 Byzantine 权重严格小于 `W/3`，活性另依赖最终同步、可用的足额正确验证者及确定性执行材料。三等权验证者须三签，不能据三节点部署宣称可容忍一节点作恶或离线后继续提交；单机多个 key 也不增加独立故障域。

proposal、prevote、precommit、nil vote 与证书绑定同一世界、chain/genesis、协议版本、高度、round、phase、活动集合和精确 block ID；block ID 承诺 parent、manifest 及有序输入、执行与状态 roots。进入新 round 不清除锁；仅规范中的已验证 prevote 证据允许改投，超时、节点重启、同高替换 proposal 或本地 slot 不授予该权限。签名决定须在对外释放前持久化，并与锁、round、有效值及恢复水位一致；坏盘、丢失或回滚签名状态必须停签。[签名恢复设计](design.md#des-p2p-bft-signing-recovery-v1)定义原字节重发、冲突拒签和复制实例的隔离。

治理批准、已提交排程与本机 runtime 已采用分别可观察。首版同一世界至多一个尚未激活的集合转换：在 `h` 最终提交的更新固定于 `h+2` 生效，`h`、`h+1` 仍按旧集合，`h+2` 才按新集合计票；本机未完成安全安装只能停签，不延后全网边界或回退旧集合。注册、轮换、撤销和 stake 变更均走[集合转换](design.md#des-p2p-bft-validator-transition-v1)，本地配置不授予投票权。此规则仅用于 BFT 内部更新，与[controlled→BFT 的 H/H+1 profile 交接](#p2p-authority-handoff)分开。

验收须覆盖足额签名真实性、阈值边界、重复/矛盾投票、锁与跨 round 证据、分区恢复、重启签名水位、成员转换及全部提交证明消费者。equivocation 证据不能增加权重，处罚不能在各节点本地改写当前集合。当前 stake-threshold prototype 和独立 finality verifier 不满足完整状态机、持久投票决定、动态集合安装及恢复组合；本条仍为正式 BFT 激活前的阻断条件。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="p2p-target-recovery-trust-chain"></a>
### 目标同世界恢复链

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4；[REQ-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)：逐环验证同世界恢复材料，不以可读材料代替恢复。
- Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[REQ-DCS-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)：pruning 前的可重建及冗余 archive 条件。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

固定 world/chain/genesis 身份→各高度合法 profile/epoch/manifest 激活链→verified committed checkpoint/header+authority transition→hash-bound snapshot→canonical committed replay→verified state root→serve/vote；任何 identity/signature/continuity/hash/replay/root mismatch 阻断；prune 前证明可重建及冗余 archive。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="p2p-full-sync-read-only"></a>
### 完整状态同步与只读资格

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4、O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3、O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；分别承接 [DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 的恢复连续性、[DCS-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) 的角色边界及 [DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) 的可用性语义。

完整状态同步固定外部认证的 C0、minimum 和同一目标 cut，验证合法提交链、manifest、snapshot/journal 与全部 retained material 闭包，再原子发布 verified read-only head。连接成功、head 较高、读授权或下载完整均不授予 writer/validator 权限；本机已保留的验证历史可读也不表示当前网络可服务。私有全状态的读取与撤权只服从[安全治理授权合同](blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-read-authorization)，不以同步角色自授读权。

<a id="p2p-full-sync-capacity-checkpoint"></a>
### 长期同步容量与 checkpoint 接续（目标）

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4、O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3、O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；分别承接 [REQ-DCS-002](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) 的长期恢复连续性、[REQ-DCS-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) 的可重建与 archive 条件，以及 [REQ-DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005) 的容量受阻与可用性语义。

正式完整同步须声明版本化支持规模：历史页数、证明与 manifest 字节、对象/retained-tree 数量、单轮总字节、重开耗时预算及保留策略，并在达到限制前暴露用量、剩余额度、checkpoint age 和 lag。合法历史增长触及限制时保留最后 verified read-only cut，显式报告 stale/catching-up、容量 blocker 或 `HistoryUnavailable`，不能升级 current/serve/vote，也不能删除唯一审计历史或降低 minimum。

后继 checkpoint 只能由原 C0 的合法认证链和完整材料闭包推导；需受保护的持久最低水位或独立认证的外部更新证据支持冷恢复。单机可回滚 pointer、普通 provider 签名和更高 height 均不能成为新信任根。安装与 GC 须保证中断不降水位，只有新 checkpoint 可独立重开、canonical replay/root 验证及冗余 archive 可重建后，才回收可重建的热历史。协议及验收见[容量与接续设计](design.md#des-p2p-full-sync-checkpoint-capacity-v1)。

当前完整同步已有有界历史页链与只读角色；observer/watch 的 inline 历史限制、full-state page chain 的容量限制及调用方是否自动续页须分别披露。分页 API 本身不证明长期接续、任意规模历史或重开 SLO 已完成。

<a id="p2p-target-role-isolation"></a>
### 目标角色与权威隔离

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；[REQ-DCS-003](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)：验证者与公开服务权限隔离；服务材料不授予共识权威。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

world-state registry 的有效 membership/stake/signer 经[确定高度的集合转换](design.md#des-p2p-bft-validator-transition-v1)成为 validator 投票真值；治理视图有效不等同 runtime 已安装。受保护 validator 与公开 sentry/relay/full/state-sync/archive/RPC/proof 服务隔离；服务只供应可验证材料。公网 IP 不是通用参与前提；validator 治理准入与服务本地 role/reachability policy 分开。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。

<a id="p2p-service-gate"></a>
### 目标恢复与重新服务

本专业条款承接以下产品义务的适用部分，保留原文技术责任与未证明范围：

- Trace refs: O7-REQ-ee563883-fe1d-4192-b58b-53251d32920b；[REQ-DCS-005](../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-005)：只读、可服务、陈旧及隔离转换的追加闸门。
- Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；[REQ-DWE-002](../product/world-infrastructure/deterministic-world-execution.prd.md#req-dwe-002)：已知无效果待决在恢复后按当前条件重审；消费者字段仍由外部 authority 拥有。

上述关系不表示产品全部义务已覆盖、实现或验收；本条工程前置、目标协议及外部依赖仍按原文区分。

历史链验证只允许只读；同世界材料、当前追加/finality、版本化 manifest compatibility、单调 head 全成立才可服务。陈旧/追赶/只读/隔离新 intent 无效果，committed receipt 为0；恢复按当前条件重审 pending，不继承期限/优先级，不改已确认 receipt。消费者反馈/lineage/manifest schema 由外部 authority 拥有，未闭合。

配对设计必须用该条款的独立条件建立承接/验证关系；本条不把文档合并解释为运行能力或组合通过。


> 专业域 authority：本文件拥有网络、共识、DistFS、复制、同步、恢复与节点可观测性契约；它向 [`doc/product/world-infrastructure/prd.md`](../product/world-infrastructure/prd.md) 提供分布式状态层证据，并为 [`doc/product/world-rules-core-gameplay/prd.md`](../product/world-rules-core-gameplay/prd.md) 的权限与治理边界提供专业合同，但不是并列产品入口。

审计轮次: 15

## 目标
- 建立 p2p 模块设计主文档，统一需求边界、技术方案与验收标准。
- 确保 p2p 模块后续改动可追溯到 PRD-ID、任务和测试。

## 范围
- 覆盖 p2p 模块当前能力设计、接口边界、测试口径与演进路线。
- 覆盖 PRD-ID 到 GitHub Issue / GitHub Project task truth 的映射。
- 不覆盖实现代码逐行说明与历史过程记录。

## 接口 / 数据
- PRD 主入口: `doc/p2p/prd.md`
- 项目管理入口: GitHub Issue / GitHub Project（不在仓库镜像可变任务状态）
- 文件级索引: `doc/p2p/prd.index.md`
- 追踪主键: `PRD-P2P-xxx`
- 测试与发布参考: `testing-manual.md`

## 当前专题路由

### 共识动作到执行与恢复合同

- 共识动作是有界、确定排序的 payload 序列；admission 校验 submitter/authentication、非零 identity、payload limits 与 queue capacity。
- proposer 从该精确有序序列计算 `action_root`；proposal/commit replication 与 `NodeExecutionCommitContext` 携带相同序列和 root。执行只接受下一 committed height，重新计算并验证 root，拒绝不可解码 payload，再通过 committed runtime context 应用动作。
- node block、action root、execution block/state roots、journal 与 snapshot references 作为同一权威记录持久化。被拒动作必须显式 requeue 或失败；execution/root/journal/proof mismatch 阻断恢复，不能继续推进状态。
- 本合同只拥有共识载荷完整性、提交顺序与恢复一致性，不代表 topology health、deployment、release readiness、市场规则或玩家体验结论。

#### ExecutionReceipt、validator 重执行与 distributed finality 分层

- `ExecutionReceipt` 是 world-runtime 对确定性执行产物的 commitment/receipt，不是 consensus certificate，也不是 distributed finality。receipt 的字段、编码和 replay 语义由 [`doc/world-runtime/prd.md`](../world-runtime/prd.md) 拥有；本 PRD 只约束它如何进入 proposal、vote、commit 与 recovery 链路，不复制 runtime schema。
- proposer 可以提交候选 receipt，但 active validator 在投票前必须从同一 committed parent state、同一 runtime/manifest 与同一确定排序的 action sequence 重执行，并独立重算和匹配 `action_root`、execution/event commitment 与 next `state_root`。缺少 artifact、执行 fault 或任一 root/commitment 不匹配时必须拒绝投票与提交，并阻断恢复；重启或本地修补不能替代该验证。
- p2p/consensus 才负责 proposal ordering、validator-set/round 状态、签名投票、quorum/commit certificate 与 finality。只有验证当时合法激活 profile 的正式提交证明并满足其持久条件后，才能推进 committed height、replication、checkpoint 或向 light client 宣告 committed；`bft` 额外要求绑定 world/height/round/roots/validator-set 的 quorum certificate。单个 receipt 或兼容本地签名均不能替代证明，也不能把单权威提交称为 distributed finality。
- `tick_consensus_records`、`TickBlock`、`TickCertificate` 等旧记录仅作为 snapshot/replay/diagnostic compatibility surface。尤其是本地 threshold-1 或 process-local record/certificate 不得解释为 distributed finality，不得单独驱动投票、committed height、replication、checkpoint 或 light-client final 状态；迁移期间按兼容读取与 receipt conformance 逐步切换。
- `WorldTick`、consensus height 与 wall-clock slot 仍是可兼容但不同的时间身份；tick 记录的存在不改变上述 execution-versus-finality 边界，也不改变本 PRD 后文既有的 BFT target/current-prototype 区分。

### 分布式运行时、PoS 与复制恢复合同

- 分布式状态由内容寻址 blob、snapshot/journal、world head 与 execution record 共同锚定；head follower 和恢复路径只能接受可验证的同 world 单调进展。state root、journal、checkpoint 或 proof 不一致时必须停止恢复，不能以重启、覆盖或跳过失败项推进状态。
- PoS 当前合同是 stake-weighted validator set、slot/epoch、确定 proposer、attestation 与 double/surround-vote 拒绝及其持久恢复边界。它不等于完整以太坊 beacon chain、BLS 聚合、经济惩罚或 mainnet finality。
- node 默认不替所有 validator 自动 attestation。提交前执行、ordered `action_root`、`NodeExecutionCommitContext/Result`、execution height/hash/state-root snapshot 字段和 committed replay 组成同一绑定；observer/viewer 不是默认执行控制面。
- replication 的现行请求协议是 `/aw/node/replication/fetch-commit/1.0.0` 与 `/aw/node/replication/fetch-blob/1.0.0`。writer epoch/sequence 必须单调，切换 writer 或 epoch 时从 sequence 1 开始；鉴权、hash 或写入失败不得污染 guard。
- storage challenge 的本地 self-probe 对错误 blob 或验证失败保持 fail-closed；provider、DHT 或 fetch route 的部分可重试不可用允许进入有界 fallback/degraded 路径，不能被概括为“任何网络失败都拒绝提交”。
- 历史 `/aw/rr/1.0.0/*` 草案、固定 bitswap/graphsync 选型、lease 覆盖每次 zone commit 与未接线 PoS 风险均不是当前合同。Phase-C 专用 DistFS request/proof envelope、challenge topic 和 specialized challenge driver 仍是未来能力缺口；这不否定现有 provider/DHT/fetch-route 网络探测 gate。
- 历史 reward leader/failover closure 曾描述 `reward_runtime_leader_*` 与 `oasis7_viewer_live` 控制面，但当前代码没有该 leader election/failover 合同；现行 reward worker 只按本地 signer 与本地 consensus readiness 提交。旧完成态不得被解释为多节点自动 leader、failover SOP、分区去重或 release readiness，未来若需要该能力必须另立 runtime/consensus 任务与回归。

- DistFS 公开反馈账本、announce/fetch 复制与 NodeRuntime 有界接线的当前专业 authority 是 [`distfs-feedback-ledger-and-replication`](distfs/distfs-feedback-ledger-and-replication.prd.md)。它保留签名、BlobState lane、replication 与非共识边界；不得据此宣称 finality、state-sync/restore 或 release/readiness。
- 早期 net path-index/Observer 的 `execution_storage.rs`、`head_follow.rs`、`head_validation.rs`、`observer.rs`、`observer_metrics.rs`、`observer_replay.rs` 与 `bootstrap.rs` 均已从 tracked source tree 删除，历史实现仅可从 Git history 追溯；保留的历史合同不是当前 checkpoint、replay、observer recovery 或自动 fallback API。未来重新实现或暴露能力必须建立 runtime 任务、验证路径并重新审查，不能从历史完成态推导 active capability。
- `oasis7_net` 的 `runtime_bridge` feature 当前是空的兼容 feature；历史 compile-closure 文档不代表 runtime bridge 逻辑仍由该 crate 暴露。当前执行/证明 bridge 以现行 runtime、proof 与 S9A authority 为准。
- 三个 2026-03 feedback 源三件套已删除；完成态与审计 provenance 仅在 Git 与 `.pm` task evidence 中保留，不是当前首读入口。
- 移动轻客户端 intent、批次根、challenge、reorg recovery 与 session-key 生命周期的当前专业 authority 是 [`p2p-mobile-light-client-authoritative-state`](network/p2p-mobile-light-client-authoritative-state.prd.md)。它只定义 evidence-gated 技术合同；不得据此宣称公开移动端可用、网络 finality、SLA、release/readiness 或 UI 已完成。2026-03-06 源三件套已退役，完成记录从 Git 与 GitHub task evidence 追溯。

### 世界基础设施终态合同（目标，非当前能力声明）

- **唯一世界与权威边界。** 目标系统只有一个 `world_id`、一条全局 canonical committed history 和一个确定性 world runtime；区域只可作为逻辑/运营分区，不能成为独立权威链。game、agent、viewer 和 light companion 与基础设施同时运行：前者提交 signed intent、消费已提交状态或证明，不能以本地模拟、RPC 可达性或暂存 intent 改写权威历史。finality 不可用时，权威推进 fail-closed；pending intent 可以保留并重试，但在重新排序、执行和最终确认前没有世界效果。local/dev 必须使用不同 `world_id`，不得把本地历史并回全局世界。
- **验证者治理与拓扑。** 终态 validator/finality signer 由 world-state 中的治理 registry 按 `genesis -> apply -> approved_candidate -> probation_ready -> active -> rotate/revoke` 管理；registry 的生效 epoch、stake、身份与 signer binding 是投票权唯一真值。允许公开申请，但 activation 不是 permissionless。当前三节点等权 validator triad 是可观测的运营基线，不是去中心化或 BFT readiness 结论；在传统 `n >= 3f + 1`、Byzantine stake 小于三分之一假设下，等权 triad 不能宣称容忍一个 Byzantine validator，四个等权 active validator 才是 conventional `f=1` 边界。
- **首个确定性 BFT 目标。** oasis7 自主实现首个 Tendermint/CometBFT-style 的高度/round 状态机：slot 只负责 proposer/pacing，独立 round 在 timeout 后推进；流程为 `Propose -> Prevote -> Precommit -> verified >2/3 active stake commit certificate`。每张 certificate 必须绑定 `world_id`、height、round、phase、block/action/execution roots、validator-set snapshot/hash 以及去重的签名投票证明；只有验证证书后才能更新 committed height、执行、复制、checkpoint 或向 light client 宣告 final。锁定/解锁、anti-double-sign、timeout/new-round、validator-set transition、partition/heal 与重启恢复均是该目标的一部分。HotStuff chained QC、Gasper/GRANDPA 的 head/finality 分层和 Solana PoH/Tower pipeline 不属于首个 skeleton。
- **当前原型与目标的缺口。** 当前实现仍是 stake-weighted proposer/attestation threshold prototype：它没有持久或可验证的 quorum certificate、round/view-change/timeout、fork/partition 恢复证明，也不会在 replication replay 时重新验证 QC。`auto_attest_all_validators` 是显式开启且默认关闭的原型辅助开关，但当前 CLI 没有 test/dev 环境硬限制；因此它在任何环境都不能作为独立验证者 quorum 或 finality 证据，生产路径必须在 BFT readiness 前限制或移除该开关。上述 BFT、治理 registry 的终态 admission、slashing/reward settlement 和 public-chain readiness 全部是 target；本 PRD 不得将其写成 current implementation 或发布结论。
- **网络与服务节点。** validators 保持 protected/private；public exposure 由 sentry/relay、full/state-sync/archive、RPC/proof gateway 等 permissionless service roles 承担。服务节点可自由运行，proof、hash 和 receipt 而非 operator identity 决定其输出是否可信；其失陷不得获得 consensus authority。libp2p 仍是网络底座，目标只借鉴主流 BFT 架构，不引入第二套 transport 或替换现有网络真值。
- **链、DistFS 与恢复。** consensus 链承载全局顺序、validator transition、certificate、checkpoint/header 和 execution/state root；DistFS 承载 blob、snapshot、journal/proof 等 hash-addressed bulk data，不能替代 quorum finality。full-node bootstrap/recovery 的目标信任链固定为：fixed world/chain/genesis identity -> verified historical profile/epoch/manifest activation chain -> verified committed checkpoint/header and authority transition -> hash-bound snapshot -> canonical committed-log replay -> verified state root -> serve/vote。任何 world identity、certificate/signature、continuity、hash、replay 或 root mismatch 都必须阻断，不得以任意 peer/latest backup、重启或覆盖恢复权威状态。
- **分层存储与激励边界。** validators 保留投票和恢复所需的 authority-critical window；full/state-sync nodes 保留较长热历史并服务 verified snapshots/blobs；archives 保留完整 canonical audit history；light companions 只保留 finalized headers、validator transitions 与按需 proofs。prune 前必须证明 checkpoint/replay 可重建、hash/root 可验证且冗余 archive 可用。终态协议为治理准入 validators 提供 slashable stake 与有界 issuance/fee rewards，并只对客观可证明的共识故障结算；permissionless storage/relay/RPC services 以可验证 usage/availability receipts 获得可选 market fee 或 governed grant，不取得投票权，也不承受 blanket availability slashing。计量、签名证明、stake lock、fault evidence 与 deterministic settlement hook 属基础设施机制；费率、奖励池、补贴/授予预算和资格参数属于经济治理，非本 PRD 的已实施经济政策。

## 里程碑
- M1 (2026-03-03): 完成模块设计 PRD 主体重写与任务改造。
- M2: 补齐模块设计验收清单与关键指标。
- M3: 建立 PRD-ID -> Task -> Test 的长期追踪闭环。

## 风险
- 模块边界演进快，文档同步可能滞后。
- 指标口径不稳定会降低验收一致性。
## 1. Executive Summary
- Problem Statement: 网络、共识、DistFS 与节点激励相关设计迭代频繁，缺少统一 PRD 导致跨子系统改动难以同时满足可用性、安全性与可审计性。
- Proposed Solution: 以 p2p PRD 统一定义分布式系统的目标拓扑、共识约束、存储策略、奖励机制与发布门禁。
- Success Criteria:
  - SC-1: P2P 关键改动 100% 映射到 PRD-P2P-ID。
  - SC-2: 多节点在线长跑套件按计划执行并形成可追溯结果。
  - SC-3: 共识与存储链路关键失败模式具备回归测试覆盖。
  - SC-4: 发行前完成网络/共识/DistFS 三线联合验收。
  - SC-5: 移动端轻客户端路径可在不运行本地权威模拟器前提下稳定接入。
  - SC-6: PoS slot/epoch 在多节点间由统一时间公式驱动，允许漏槽但不出现时间语义倒退。
  - SC-7: PoS 支持槽内 logical tick 相位门控与动态节拍调度，实现可配置 `tick/slot` 语义。
  - SC-8: `oasis7_chain_runtime/oasis7_game_launcher/oasis7_web_launcher/oasis7_client_launcher/scripts` 控制面参数与状态口径与 PoS 时间锚定语义一致，不再将 `node_tick_ms` 误解为出块时间。
  - SC-9: 清理残留时序语义偏差（`tick_count` 观测命名、`oasis7_viewer_live` 旧控制面假设、`world-rule` 时间模型描述），保证规范/实现/运维口径一致。
  - SC-10: runtime/game/web/client launcher 默认 PoS 时间参数与文档一致，默认启动即满足“slot 时钟锚定 + 轮询语义解耦”口径。
  - SC-11: runtime/game/web/client launcher 与 longrun 脚本默认参数统一读取 `config/chain-pos-defaults.env`，满足当前默认出块与槽内 tick 基线。
  - SC-12: `oasis7_viewer_live` 对外 CLI 收敛为纯观察服务，不再接受 `--release-config` 与 `--node-*` 控制面参数；误传时必须显式拒绝并提示改用 `oasis7_chain_runtime`。
  - SC-13: `oasis7_viewer_live` 移除 legacy 参数兼容层，不再接受 `--runtime-world` 等历史别名；代码库中不再保留未接入生产入口的旧 CLI 解析路径。
  - SC-14: 历史 PRD/project 文档中的 `oasis7_viewer_live` 旧文件路径完成替换，不再指向已删除的 `src/bin/oasis7_viewer_live/` 子目录文件。
  - SC-15: 主链 Token 创世分配与早期贡献奖励口径具备可审计分桶、低流通边界、单人直持上限与贡献制发放约束，能够直接映射到现有 runtime 创世/金库机制。
  - SC-16: hosted access 网页远程接入具备明确的 `public player plane / private control plane / signer plane` 分层、`guest/player/strong-auth` 授权梯度、公开 join admission control 与 `gui-agent` surface split 策略，且浏览器不再被视为可持有 host 节点长期私钥的受信环境。
  - SC-17: p2p 模块具备一份 public-chain-grade 的“非全公网依赖”覆盖网络目标态，明确 `public/hybrid/private/relay_only/validator_hidden` 多部署模式、`validator core/sentry/relay` 角色分离，以及 `peer record + discovery + reachability + traffic lanes` 的统一框架边界。
  - SC-17A: p2p 模块的 iroh-inspired reachability follow-up 必须保持 oasis7/libp2p 底座不变，只借鉴 identity-first address boundary、path behavior evidence taxonomy 与 bounded status observability；不得引入 iroh 依赖、替换 libp2p、或新增第二套 net health truth。
  - SC-18: 当前链上代币的正式产品命名、runtime `main_token.symbol` / ticker 与公钥派生账户前缀已统一迁移到“绿洲币 / Oasis Coin” / `OC` / `oc:pk:`；对外 API、viewer/client、脚本与测试不得再把 `AWT` / `awt:pk:` 当作现行真值。
  - SC-19: 当前本机 + 2 ECS real-env triad 必须具备一条可审计的“三节点等权 validator”落地路径，明确 validator set、signer binding、static bootstrap、same-window snapshot evidence 与 residual legacy service naming 的边界；当 execution world 已存在 `governance_finality_signer_registry` 时，节点启动/恢复必须优先从该 world-state registry 恢复 validator membership 与 signer binding，而不是继续把 role-separated `observer + sequencer + storage` 或 operator-local env 当成唯一真值拓扑。
  - SC-20: `oasis7` 可以通过独立部署的 bridge-service，把已确认的 `OC` 充值映射为 LetAI Run OpenAPI 的用户额度与动态项目 `token_key`，同时冻结“只支持 one-way service-credit bridge，不是公开兑换所、不是 AMM、也不支持自动提现回 OC”的对外口径。
  - SC-21: `hosted_public_join` 必须具备一条对普通玩家友好的正式身份路径：默认推荐通行密钥、邮箱为可选兼容方式；账号接入遵循[多凭证合同](blockchain/hosted-public-join-key-based-onboarding.prd.md)，托管 player signer 与后续自托管升级；浏览器不再被要求作为长期玩家私钥保管面。
  - SC-26: 面向普通用户的 game / web / client launcher 默认进入 `hosted_public_join`，并不再暴露 `trusted_local_only` 本地可信预览作为可选启动模式；历史本地配置迁移到 hosted join 后继续走 hosted account / player session 链路，账号认证按多凭证合同推荐通行密钥、保留可选邮箱。
  - SC-22: validator / finality signer 必须具备一条正式的治理准入流程，至少覆盖 `genesis registry cold start -> apply -> approved_candidate -> probation_ready -> active -> rotate/revoke`，并明确 world-state registry 才是正式激活真值；正式激活真值必须同时覆盖 validator membership、governed stake 与 finality signer binding，public tiers 的冷启动必须来自 genesis/world-state registry，且 genesis runtime import 只允许空 execution world 首启，已有 snapshot/journal world 必须走专用 registry migration/import；`--node-validator*` / `NODE_VALIDATORS_CSV` 与 operator-local env 只能作为 local/dev legacy fallback 或一次性迁移适配输入。
  - SC-23: p2p 模块必须具备一套正式的公共主链式网络分层机制，明确目标 tier 收束为 `local_devnet -> public_testnet -> mainnet`，同时把 `public_testnet_rehearsal` 限定为 legacy/rehearsal evidence，固定 manifest 真值与 claims/promotion 规则，避免把 shared release-train 与正式 testnet/mainnet 继续混写。
  - SC-24: `public_testnet` 必须具备一条 repo-owned readiness review 路径，能把“只有 skeleton manifest”“rehearsal / partial evidence”与“正式全 pass candidate evidence”明确区分，避免把 placeholder endpoint 或模板证据误报为可部署结论。
  - SC-25: `public_testnet` 必须具备一份 repo-owned live-candidate companion checklist，把当前 11 条 required-lane readiness gate、最小 evidence、canonical 命令与允许/禁止 claims 固定成单一执行入口，避免 producer/liveops/QA 对“还差哪些”各说各话。
  - SC-26A: `hosted world` 仅作为历史专题名 / legacy label 保留，现行概念名为 hosted player entry / hosted access / `hosted_public_join`，表示玩家进入统一持久大世界的接入面，不表示多个玩家世界。
  - SC-27: p2p 模块必须把“链上大世界状态底座”作为 P2P transport、DistFS/blob closure、replication/gap sync/state sync、consensus/finality、execution record/receipt 与 observer/ops 的联合验收对象；`P2P` 只表示底座中的传输层，不得用单一 transport green 代替分布式世界状态闭环。

## 2. User Experience & Functionality
- User Personas:
  - 协议工程师：需要明确网络与共识边界。
  - 节点运营者：需要稳定部署和可观测运行信号。
  - 安全评审者：需要签名、治理、资产流转的可审计证据。
  - 移动端玩家：需要低算力设备可持续在线并获得正确最终性反馈。
  - 制作人与金库治理维护者：需要在创世前冻结主链 Token 分配结构，避免过早流通、单人过度控盘或“玩就发币”的错误口径。
  - hosted access host / operator：需要把可公开分享的 join URL 与私有控制面拆开，避免分享试玩地址时连控制权一起暴露。
  - hosted access 远程玩家：需要通过网页先建立 session 再游玩，而不是直接继承 host 节点 signer。
  - 私网 / 家宽 / 企业内网节点运营者：需要在没有公网 IP 的前提下，仍能以正式角色加入网络、同步状态或通过 sentry/relay 参与主链。
  - validator 候选运营者：需要明确提交什么材料、何时从候选转为 active validator，以及哪些 signer/控制权限并不对外开放。
- User Scenarios & Frequency:
  - 协议演进评审：每次共识或网络协议改动前执行。
  - 多节点长跑：按周执行并记录稳定性与恢复结果。
  - 发行前联合验收：每个候选版本执行一次三线联测。
  - 安全审计复核：关键资产链路改动后立即触发。
  - 轻客户端接入验收：每次移动端协议调整后执行输入/最终性/重连验证。
  - 创世发行前评审：每次准备冻结 Token 创世配置或早期奖励口径时执行一次。
  - hosted access 架构复核：每次准备让“玩家部署服务给其他玩家通过网页进入统一持久大世界”进入公开测试前执行一次。
- User Stories:
  - PRD-P2P-001: As a 协议工程师, I want explicit protocol boundaries, so that multi-crate changes remain coherent.
  - PRD-P2P-002: As a 节点运营者, I want reliable longrun validation, so that production confidence increases.
  - PRD-P2P-003: As a 安全评审者, I want auditable cryptographic and governance flows, so that risk is controlled.
  - PRD-P2P-004: As a 移动端玩家, I want intent-only light client access, so that low-end devices can still participate fairly.
  - PRD-P2P-005: As a 协议工程师, I want slot/epoch to be wall-clock driven, so that block time semantics remain stable across restart and lag.
  - PRD-P2P-006: As a 协议工程师, I want slot-internal tick-phase pacing, so that proposal cadence can follow configured `ticks_per_slot`.
  - PRD-P2P-007: As a 节点运营者, I want runtime/launcher/scripts to expose anchored slot-clock parameters explicitly, so that block-time tuning is deterministic and auditable.
  - PRD-P2P-008: As a 协议工程师, I want cross-doc and status field naming to disambiguate worker polling vs consensus ticks, so that observability and operations avoid semantic drift.
  - PRD-P2P-009: As a 节点运营者, I want sane default PoS timing values and uniform validation wording, so that default startup already follows anchored block-time semantics without hidden overrides.
  - PRD-P2P-010: As a 发布维护者, I want `oasis7_viewer_live` to reject legacy release/node control-plane flags, so that chain control is unambiguously hosted by `oasis7_chain_runtime`.
  - PRD-P2P-011: As a 发布维护者, I want legacy compatibility aliases removed from `oasis7_viewer_live`, so that CLI semantics are single-source and there is no dead parser path.
  - PRD-P2P-012: As a 维护者, I want historical docs to reference current source layout, so that reviewers do not chase deleted paths during audit or regression.
  - PRD-P2P-013: As a producer_system_designer, I want one frozen genesis allocation and early-contribution reward policy, so that oasis7 can issue token with low circulation, auditable control boundaries and no accidental play-to-earn framing.
  - PRD-P2P-014: As a producer_system_designer, I want one explicit cryptographic security baseline verdict, so that oasis7 does not overclaim “mainstream public-chain-grade security” before transaction authorization, signer custody and genesis control are actually ready.
  - PRD-P2P-015: As a `runtime_engineer`, I want public main-token transfer submit to require signed account authorization, so that the first exposed asset surface no longer trusts unsigned `from_account_id` fields.
  - PRD-P2P-016: As a producer_system_designer, I want the remaining post-STRAUTH signer custody, governance signer and genesis ceremony blockers turned into formal readiness gates, so that oasis7 can truthfully stay in preview while still having one executable path toward a later mainnet-grade re-evaluation.
  - PRD-P2P-017: As a producer_system_designer, I want one explicit production signer custody / keystore baseline, so that preview bootstrap signers are clearly separated from production signer ownership and release policy.
  - PRD-P2P-018: As a producer_system_designer, I want one explicit governance signer externalization baseline, so that deterministic local seed and local config signer truth are clearly separated from long-term production governance truth.
  - PRD-P2P-019: As a producer_system_designer, I want one explicit genesis freeze/ceremony/QA gate, so that logic-frozen but still-unbound genesis parameters cannot be mistaken for mint readiness.
  - PRD-P2P-020: As a producer_system_designer, I want one final public claims policy re-evaluation after MAINNET readiness planning, so that outward language stays aligned with execution reality rather than spec completeness.
  - PRD-P2P-021: As a producer_system_designer, I want one explicit benchmark against mainstream public-chain testing systems, so that oasis7 testing maturity is judged by layered evidence rather than isolated green checks.
  - PRD-P2P-022: As a producer_system_designer, I want one explicit legacy network-rehearsal / release-train rehearsal model, so that oasis7 can preserve `L5` auditability without treating it as a current target environment or player world model.
  - PRD-P2P-023: As a producer_system_designer, I want one explicit hosted player access and session-auth model, so that one player部署服务给另一个玩家通过网页进入统一持久大世界时，不会再把 host control-plane、shared `gui-agent` control surface 与 node signer 暴露给浏览器。
  - PRD-P2P-024: As a producer_system_designer, I want one public-chain-grade private-reachability P2P architecture, so that oasis7 不再把“所有正式节点都要有公网 IP”当成默认前提，并能在 mixed-topology 现实下继续对标公共主链。
  - PRD-P2P-025: As a producer_system_designer, I want one canonical triad observability stack, so that 当前 real-env triad（物理上为本机 + 2 ECS，runtime 上已收口为 three_equal_validator，历史 service label 仅作兼容别名）的真实运行状态可以在同一轮监控里同时回答资源、链状态、流量、WASM 健康，并进一步定位到具体 runtime 子模块和优化热点。
  - PRD-P2P-026: As a producer_system_designer, I want the live triad to support a three-equal-validator topology, so that the local node is no longer a permanent observer exception and triad semantics can match “three peer-equal validators” when operations explicitly choose that mode.
  - PRD-P2P-027: As a producer_system_designer, I want one canonical one-way `OC -> LetAI Run OpenAPI quota/token_key` bridge model, so that oasis7 可以把当前主链 Token 用作受控的 AI 服务额度充值资产，同时不误滑成公开兑换所、浏览器热钱包或双向提现承诺。
  - PRD-P2P-028: As a producer_system_designer, I want one formal public-chain-style network-tier mechanism, so that oasis7 can stop treating `public_testnet_rehearsal`、`public_testnet` and `mainnet` as informal aliases and instead promote networks through explicit manifest + gate truth.
  - PRD-P2P-028A: As a `producer_system_designer`, I want one idealized transaction target model, so that主链交易未来从“转账裸参数”升级为统一协议对象时，有一份不受当前实现约束的长期目标态可对齐。
  - PRD-P2P-029: As a producer_system_designer, I want one explicit hosted-public-join managed identity / custody model, so that普通玩家可以通过推荐通行密钥或可选邮箱登录游戏，而不是被迫管理裸公私钥，同时 hosted player signer、step-up auth 与自托管升级路径都保持在可审计边界内。
  - PRD-P2P-031: As a qa_engineer, I want one explicit chain-world-state substrate closure model, so that P2P transport、DistFS blob closure、replication/gap sync、consensus/finality、execution records/receipts and observer ops can be tested independently without overclaiming full game or public-testnet readiness.
- Critical User Flows:
  1. Flow-P2P-001: `网络拓扑变更 -> 共识联调 -> DistFS 同步 -> 节点状态一致性验证`
  2. Flow-P2P-002: `执行 S9/S10 长跑 -> 采集故障与恢复数据 -> 输出收敛报告`
  3. Flow-P2P-003: `资产/签名链路变更 -> 审计检查 -> 安全门禁 -> 发布判定`
  4. Flow-P2P-004: `手机端提交签名 intent -> 权威模拟执行 -> 链上承诺/挑战 -> 客户端 final 确认`
  5. Flow-P2P-005: `节点读取 wall-clock -> 计算 slot/epoch -> 允许漏槽推进 -> 拒绝未来槽/过旧槽提案`
  6. Flow-P2P-006: `节点按 wall-clock 计算 logical tick/phase -> 相位命中才提案 -> runtime 动态等待下一 tick 边界`
  7. Flow-P2P-007: `运维配置 slot_duration/ticks_per_slot/proposal_phase -> runtime/game/web/client launcher 统一生效 -> status/soak 输出可观测并用于门禁`
  8. Flow-P2P-008: 状态接口/手册/PRD 同步更新 -> `tick_count` 明确为 worker poll 指标 -> 采样脚本以共识 slot/tick/height 为主
  9. Flow-P2P-009: `默认启动 runtime/game/web/client launcher -> 使用统一默认 slot_duration/ticks_per_slot -> 文档/帮助/校验文案一致呈现 poll vs slot 语义`
  10. Flow-P2P-010: `用户误传 oasis7_viewer_live --release-config/--node-* -> CLI 显式拒绝并给出替代入口 -> 文档与示例迁移到 oasis7_chain_runtime`
  11. Flow-P2P-011: `用户误传 oasis7_viewer_live 任意 legacy 参数（含 --runtime-world） -> CLI 明确拒绝并输出迁移入口 -> 测试与手册口径一致`
  12. Flow-P2P-012: `执行历史文档巡检 -> 替换已删除源码路径到当前入口路径 -> 文档门禁 + grep 零残留校验`
  13. Flow-P2P-013: `制作人冻结创世分配表 -> runtime 映射创世桶与 vesting 参数 -> liveops/QA 审核早期贡献奖励边界 -> 创世配置与低流通门禁共同放行`
  14. Flow-P2P-014: `盘点签名/地址/交易授权/keystore/治理 signer/创世控制真值 -> 形成 red/yellow/green 矩阵 -> producer 输出总 verdict 与 P0 blocker`
  15. Flow-P2P-015: `客户端为 transfer submit 构造 canonical payload 并签名 -> runtime 验签并比对 oc:pk 账户绑定 -> 通过后才进入余额/nonce 预检与 consensus submit`
  16. Flow-P2P-016: `operator 运行 triad 完整监控 -> snapshot/host/traffic/wasm 产物全部落盘 -> merged summary 输出 overall status + node-level alerts -> evidence doc 引用 canonical 输出路径`
  17. Flow-P2P-017: `STRAUTH-3 收口后复盘剩余安全缺口 -> 冻结 MAINNET-1~4 readiness gate -> signer custody/governance signer/genesis ceremony 逐项过门禁 -> producer 再决定是否重评阶段`
  18. Flow-P2P-018: `盘点 node/viewer/governance signer 来源 -> 冻结 preview-only bootstrap 与 production target backend 边界 -> rotation/revocation/audit policy 入门禁`
  19. Flow-P2P-019: `盘点 finality/controller signer 真值 -> 冻结 externalized source-of-truth 与 operator ownership -> 候选 validator 提交 node identity/finality signer/public manifest -> candidate/probation 审核 -> activation 生效后再写入 world-state registry -> failover/rotation/revocation 进入治理门禁`
  20. Flow-P2P-020: `读取 genesis freeze sheet -> 绑定 slot/bucket 真值 -> 执行 ceremony checklist -> QA 审核 evidence bundle -> 决定是否允许 mint-ready 口径`
  21. Flow-P2P-021: `读取 MAINNET-1~3 当前状态 -> 判断哪些仅为 spec gate、哪些已 execution complete -> 冻结 claim allowlist/denylist 与未来升级条件`
  22. Flow-P2P-022: `读取 testing-manual 与安全/readiness 专题 -> 映射 oasis7 当前测试层 -> 对照主流公链 testing benchmark -> 冻结 gap matrix 与下一步验证优先级`
  23. Flow-P2P-023: `host 启动 hosted access -> public join 先过 admission control -> 远程访客建 guest/player session -> runtime 按 capability 绑定实体与动作 -> `gui-agent` 仅走 player-safe split surface -> 资产/治理类动作再升级 strong auth`
  24. Flow-P2P-024: `用户绑定 bridge 身份 -> bridge-service 分配唯一 deposit route -> 用户通过受信转账面支付 OC -> bridge watcher 等待确认并写入 bridge_ledger -> LetAI OpenAPI 执行 user upsert / project+token_key / topup / query verification -> operator 对账与异常收口`
  25. Flow-P2P-025: `访客通过推荐通行密钥或可选邮箱从 guest 升级到 hosted account -> identity broker 恢复账户与 signer_ref -> session broker 签发 device/player session -> 高风险动作再经 step-up + custody sign -> 如需退出托管则走 external wallet bind / transfer-out`
  26. Flow-P2P-026: `同一 world_id/chain_id/genesis/manifest + action/event 输入 -> 单节点执行生成 execution record/receipt/state hash -> 多节点 consensus/replication 收敛 -> DistFS/blob closure 可验证 -> observer 自动追高 -> API/viewer 读取同一 world state -> S10/public-testnet gate 再决定是否升级 claim`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 按钮/动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 网络与共识协同 | 节点ID、轮次、提交高度、延迟 | 启动联测并比对共识结果 | `joining -> syncing -> committed` | 高度/轮次单调递增 | 仅授权节点参与共识 |
| DistFS 复制 | 文件ID、副本状态、同步延迟 | 触发复制并校验完整性 | `queued -> replicating -> verified` | 优先关键数据副本 | 节点需满足存储策略 |
| 长跑与恢复 | 失败类型、恢复动作、恢复时长 | 注入故障并执行恢复流程 | `stable -> degraded -> recovered` | 按故障等级排序处理 | 运维/评审可操作恢复流程 |
| 轻客户端权威状态 | `intent(tick/seq/sig)`、`state_root`、`finality_state` | 手机端只上报 intent，接收 delta/proof 并展示最终性 | `pending -> confirmed -> final` | 按 tick 排序，重复 seq 幂等去重 | 权威状态仅由模拟节点提交，客户端无写权限 |
| PoS 固定时间槽 | `genesis_unix_ms`、`slot_duration_ms`、`epoch_length_slots`、`last_observed_slot`、`missed_slot_count` | 每次 tick 按真实时间换算 slot；仅在 `next_slot <= current_slot` 时允许提案 | `pending -> committed/rejected`（槽位单调） | `current_slot=floor((now-genesis)/slot_duration)`；`epoch=slot/epoch_length_slots` | 仅验证者可提案/投票；未来槽消息拒绝 |
| PoS 槽内 tick 节拍 | `ticks_per_slot`、`tick_phase`、`proposal_tick_phase`、`last_observed_tick`、`missed_tick_count` | 仅在命中提案相位时触发提案；worker 按下一 logical tick 边界动态调度 | `idle -> proposing`（相位门控） | `logical_tick=floor((now-genesis)*ticks_per_slot/slot_duration)`；`phase=tick%ticks_per_slot` | 节拍公式全节点一致；本地调度可回退固定间隔 |
| PoS 控制面参数对齐 | `node_tick_ms`（轮询）+ `slot_duration_ms`、`ticks_per_slot`、`proposal_tick_phase`、`adaptive_tick_scheduler_enabled`、`slot_clock_genesis_unix_ms`、`max_past_slot_lag` | runtime/game/web/client launcher/scripts 显式暴露并校验参数，状态接口回显观测字段 | `configured -> running -> audited` | `node_tick_ms` 不参与出块时间计算，仅作为 worker 轮询/回退间隔 | 运维可配置；非法值必须启动前拒绝 |
| 时序语义残留收敛 | `worker_poll_count`、`consensus.last_observed_tick`、`consensus.committed_height`、`slot_duration_ms` | 更新状态字段命名/文档叙述并修复过时控制面假设 | `legacy -> aligned` | 轮询指标与共识指标分离，不混用同一“tick”语义 | 运维/QA 只读；配置前必须通过校验 |
| Viewer 控制面边界收敛 | `oasis7_viewer_live` CLI（`--bind`/`--web-bind`/`--llm`/`--no-llm`） | 仅保留观察服务参数；误传 `--release-config`、`--runtime-world`、`--node-*` 与其他 legacy 控制面参数直接拒绝 | `legacy_mixed -> observer_only_strict` | CLI 白名单固定；错误信息必须包含迁移目标 `oasis7_chain_runtime` | 运行链控制面仅限受信运维入口 |
| Token 创世分配与低流通 | `display_name_cn`、`display_name_en`、`symbol`、`account_prefix`、`initial_supply`、`allocation_bps`、`bucket_id`、`recipient`、`cliff_epochs`、`linear_unlock_epochs`、`founder_direct_cap_bps`、`circulation_cap_bps` | 冻结正式命名、创世分配表、创世绝对总量、设定 vesting 与金库控制边界、审计早期贡献奖励准入，并统一 runtime/account 派生真值 | `draft -> named -> symbol_migrated -> supply_frozen -> auditable` | 正式产品名固定为“绿洲币 / Oasis Coin”；当前 `symbol=OC`、`account_prefix=oc:pk:`；当前 `initial_supply=10,000,000,000 OC`；分配总和必须 `10000 bps`；项目战略控制目标 `5000 bps`；协议奖励池 `3500 bps`；单人直持硬上限 `1500 bps` | 制作人定义口径；runtime 按创世配置落地；liveops/QA 仅可在已定义边界内执行与验收 |
| 密码学安全基线评估 | `primitive_status`、`transaction_auth_status`、`account_model_status`、`key_custody_status`、`governance_signer_status`、`genesis_control_status`、`overall_verdict` | 盘点代码/文档真值并输出 system-level verdict；若 blocker 未清零则拒绝高级安全口径 | `unknown -> inventoried -> verdict_frozen` | 只要资产动作缺统一签名交易模型，整体必须保持 `not_mainnet_grade` | `producer_system_designer` 拍板，`runtime_engineer`/`qa_engineer` 联审 |
| 主链 Token 签名交易鉴权 | `from_account_id/to_account_id/amount/nonce/public_key/signature` | runtime 先验签并校验 `oc:pk:` 账户绑定，再进入既有余额/nonce 预检与 consensus submit | `unsigned_surface -> transfer_signed_surface` | transfer submit 必须带固定版本签名；`from_account_id` 必须等于 `oc:pk:<public_key_hex>`；其他资产动作仍待后续专题 | `runtime_engineer` 牵头实现，`viewer_engineer`/`qa_engineer` 跟进客户端与回归 |
| 主流公链测试体系对标 | `layer_id/current_coverage/evidence_paths/gap_status/next_action` | 将 oasis7 suites/evidence 对位到主流公链测试分层，并冻结缺口矩阵与执行优先级 | `draft -> mapped -> prioritized` | 若缺共享网络、真实 drill 证据或 fuzz/property gate，则不得宣称“主流公链级测试成熟度” | `producer_system_designer` 拍板，`qa_engineer` 联审 |
| Validator / finality signer 治理准入 | `candidate_id/node_id/stake/finality_signer_public_key/operator_owner/public_manifest/activation_epoch/admission_status` | 创世时由 genesis validator registry 初始化 world-state registry；后续受理申请、审核 reachability/registry/failover 准入条件，并在 activation 生效后把候选节点的 membership、governed stake 与 signer binding 写入正式 validator truth | `genesis_registry -> applied -> approved_candidate -> probation_ready -> active -> rotated/revoked` | 只有 world-state registry 生效后才算正式 validator；public_testnet/mainnet 缺 registry 且未提供 genesis registry 时必须拒绝启动；genesis threshold 必须来自显式 genesis document/manifest policy，N>3 不允许静默降级为 `2-of-N`；finality 与 module-release threshold_bps 必须按签名 validator stake / active validator total stake 执行；`--node-validator*`、`NODE_VALIDATORS_CSV` 与本地 env 改动不算长期 admission 完成态；stake 必须来自 world-state registry / governance admission，旧 registry 缺失 stake 时只允许按兼容默认值回退 | `producer_system_designer` 拍板，`runtime_engineer`/`qa_engineer` 联审 |
| Hosted player access 与 session auth | `deployment_mode/session_id/session_level/capability_set/control_plane_scope/strong_auth_state/admission_policy/player_safe_agent_surface` | 将网页远程玩家面、host 控制面与 signer plane 分层；签发 guest/player session，并对敏感动作要求 strong auth | `trusted_local_only_preview -> hosted_public_join_blocked_until_strong_auth -> hosted_public_join_strong_auth_preview -> hosted_access_ready` | 只要浏览器仍依赖 `node.private_key` bootstrap、可命中 host 控制面路由或未冻结 admission / `gui-agent` split，就不得判为 hosted access ready；`*_preview` 只表示受限预览态，不表示一个单独玩家世界 | `producer_system_designer` 拍板，`runtime_engineer`/`viewer_engineer`/`qa_engineer`/`liveops_community` 联审 |
| Hosted public join 托管身份与托管密钥 | `hosted_account_id/player_id/device_session_id/signer_ref/custody_mode/step_up_state/transfer_out_state` | 为公开 join 玩家提供推荐通行密钥 / 可选邮箱的账号接入、托管 signer、step-up auth 与自托管升级路径 | `guest_only -> account_verified -> managed_custody_active -> self_custody_bound/transferred_out` | 只要 hosted player 仍要求用户保存裸私钥、浏览器仍长期持有托管 signer 或 `main_token_transfer` 没有正式 custody lane，就不得宣称“任意新用户默认可安全登录并长期使用” | `producer_system_designer` 拍板，`runtime_engineer`/`viewer_engineer`/`qa_engineer`/`liveops_community` 联审 |
| 链上大世界状态底座自闭环 | `world_id/chain_id/genesis_hash/network_tier_manifest/action_or_event_input/execution_record/receipt_id/state_root/blob_manifest/consensus_hash/observer_cursor/claim_level` | 分阶段验证本地合同、proxy/triad 多节点、state-sync/observer、real-env 运维与 S10 世界状态集成；每次只声明当前 evidence 支撑的 claim level | `contract_inventory -> single_node_executed -> deterministic_exact_passed -> proxy_soak_passed -> state_sync_closed -> real_env_observed -> world_integration_passed` | `module_required` 只证明底座本地合同可接入；`module_full` 证明 proxy/triad 可推进与恢复；`integration_required` 需要 action 到 API/viewer 投影无漂移；`release_full` 必须经过 real-env/public-testnet readiness，同一窗口 evidence 不可缺席 | `qa_engineer` 定义 gate，`runtime_engineer`/`blockchain_ops_engineer` 联审，`producer_system_designer` 负责 claim boundary |
- 三线联合验收清单（TASK-P2P-002）:
| 线别 | 必跑命令（基线） | 联合验收门禁 | 阻断条件（任一命中即 fail） | 证据产物 |
| --- | --- | --- | --- | --- |
| 网络线（net） | `env -u RUSTC_WRAPPER cargo test -p oasis7_net --lib`；`env -u RUSTC_WRAPPER cargo test -p oasis7_net --features libp2p --lib` | `./scripts/release-gate.sh --dry-run` + S9 发布档位命令（见 `testing-manual.md`） | `oasis7_net` 单测失败；S9 `metric_gate.status != pass`；`consensus_hash_consistent != true` | `release-gate-summary.md`、S9 `summary.json/timeline.csv` |
| 共识线（consensus） | `env -u RUSTC_WRAPPER cargo test -p oasis7_consensus --lib`；`env -u RUSTC_WRAPPER cargo test -p oasis7_node --lib` | S9 + S10 发布档位命令（见 `testing-manual.md`） | 共识/节点单测失败；S9 或 S10 `overall_status/run.status != ok`；`consensus_hash_mismatch_count > 0` | S9/S10 `summary.json`、`failures.md`（若失败） |
| 存储线（DistFS） | `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib` | S9 发布档位命令（含 `--max-distfs-failure-ratio 0.1`） | DistFS 单测失败；`distfs_failure_ratio` 超阈值；反馈/复制不一致无法闭环 | S9 `summary.json`、`feedback_events.log`、`chaos_events.log` |
- Acceptance Criteria:
  - AC-1: p2p PRD 覆盖网络、共识、存储、激励四条主线。
  - AC-2: p2p project 文档任务项明确映射 PRD-P2P-ID。
  - AC-3: 与 `doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.prd.md` 等稳定设计权威口径一致。
  - AC-4: S9/S10 相关测试套件在 testing 手册中有对应条目。
  - AC-5: 轻客户端专题以稳定三件套 `p2p-mobile-light-client-authoritative-state.*` 维护，并映射到独立维护责任；历史 `TASK-P2P-MLC-*` 只作追溯，不单独构成现时完成或发布结论。
  - AC-6: PoS fixed slot/epoch wall-clock、漏槽与时间窗口合同已由 `doc/world-runtime/runtime/chain-pos-control-plane.prd.md` 承载；历史任务链 `TASK-P2P-008` 只作追溯。
  - AC-7: PoS subslot tick phase、adaptive pacing 与 strict-lag recovery contract 已由同一 runtime authority 承载；历史任务链 `TASK-P2P-009` 只作追溯。
  - AC-8: 三线联合验收清单明确给出“基线命令 + 发布门禁阈值 + 阻断条件 + 证据产物”，可直接用于发行前检查。
  - AC-9: S9/S10 长跑结果模板与缺陷闭环模板完成定义，失败运行必须能映射到 `incident_id -> 修复任务 -> 回归证据`。
  - AC-10: 发行门禁分布式质量指标（S9/S10）具备“阈值 + 数据源 + 阻断策略 + 责任归属”映射，并与 `release-gate` 脚本参数一致。
  - AC-11: runtime/game/web/client launcher/scripts 的 PoS control-plane 参数与 status 接口口径由 `doc/world-runtime/runtime/chain-pos-control-plane.prd.md` 承载；历史任务链 `TASK-P2P-010` 只作追溯。
  - AC-12: 残留语义项完成收敛：`world-rule` 时间模型、launcher `chain_node_tick_ms` 校验文案、`/v1/chain/status` 轮询字段命名、viewer/manual/site 与 `oasis7_viewer_live` 实际 CLI 能力保持一致。
  - AC-13: `oasis7_chain_runtime/oasis7_game_launcher/oasis7_web_launcher/oasis7_client_launcher` 默认 `slot_duration_ms` 与文档基线一致；`oasis7_web_launcher` 校验文案明确 `chain_node_tick_ms` 为 poll interval 语义。
  - AC-14: `oasis7_chain_runtime/oasis7_game_launcher/oasis7_web_launcher/oasis7_client_launcher/oasis7_viewer_live/p2p-longrun/s10` 默认 `slot_duration_ms/ticks_per_slot/proposal_tick_phase` 与 `config/chain-pos-defaults.env` 基线一致，相关默认值断言与手册同步更新。
  - AC-15: `oasis7_viewer_live` 解析层移除 `--release-config` 与 `--node-*` 参数能力；定向测试覆盖“误传 legacy 参数 -> 启动失败 + 替代提示”路径。
  - AC-16: `oasis7_viewer_live` 进一步移除 `--runtime-world` 兼容别名与旧 split CLI 路径，定向测试覆盖 `--release-config/--runtime-world/--node-*` 拒绝行为。
  - AC-17: 历史文档中 `oasis7_viewer_live` 子目录旧路径完成迁移（对齐 `oasis7_viewer_live.rs` 与 `oasis7_chain_runtime/*` 现行布局），文档门禁通过。
  - AC-18: `doc/p2p/**` 仍可读历史专题的首行标题必须统一使用 `oasis7 Runtime` 或 `oasis7` 品牌；旧 `oasis7*` 标题仅允许保留在正文历史上下文、证据原文与兼容说明中。
  - AC-19: `mainchain-token-genesis-and-contributor-reward` 专题文档落盘并映射任务链 `TASK-P2P-031`，明确 `10000 bps` 创世分配表、项目战略控制 `5000 bps`、协议奖励池 `3500 bps`、单人直持目标/上限、低流通边界与“贡献制奖励而非 P2E”口径。
  - AC-20: 稳定权威 `p2p-mainnet-security-governance-readiness` 吸收 `TASK-P2P-032` 的密码学基线，保持整体 verdict 为 `not_mainnet_grade`，固定交易授权、custody、治理 signer 与创世控制 blocker。
  - AC-21: `p2p-mainnet-security-governance-readiness` 承载当前 signed transfer contract：`POST /v1/chain/transfer/submit` 必须校验 `public_key/signature`、绑定 `oc:pk:<public_key_hex>` 并完成 required 回归；历史任务链 `TASK-P2P-033` 只作追溯。
  - AC-22: 稳定权威 `p2p-mainnet-security-governance-readiness` 吸收 `TASK-P2P-034`，明确当前阶段只可称为 `limited playable technical preview` + `crypto-hardened preview`，并冻结 `MAINNET-1~4` readiness gate。
  - AC-23: 同一稳定权威吸收 `TASK-P2P-035`，明确 `config.toml` 明文 key、HTML 私钥注入与 env 私钥 bootstrap 只属于 preview-only signer path，不得作为 production custody 完成态。
  - AC-24: 同一稳定权威吸收 `TASK-P2P-036`，明确 governance registry 优先、deterministic local seed / `NodeConfig` local fallback 只属于 preview/local truth，不得作为 production governance truth，并冻结 validator / finality signer 的治理准入目标流程。
  - AC-25: 同一稳定权威吸收 `TASK-P2P-037`，明确 `logic_frozen_address_binding_pending`、`TBD_BEFORE_MINT`、`pending_binding` 与 `ready_pending_address_binding` 都属于 mint-ready blocker。
  - AC-26: `TASK-P2P-038` 是 2026-03-23 的历史公开口径审查，记录当时 `MAINNET-1~3` 仅完成 spec gate、整体 verdict 为 `not_mainnet_grade`；它不再充当当前公开状态权威。当前状态以根 `README.md` 为准，长期公开口径规则由 `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md` 承接，网络与测试文档提供专业证据。
  - AC-27: `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md` 保留 2026-03-24 专题历史并映射任务链 `TASK-P2P-039`，明确主流公链测试分层模型、单实现栈等价要求、历史 gap matrix、`fuzz/property` 与 `network rehearsal/release train` 缺口及 claims denylist；当前网络状态与成熟度不由该 benchmark 维护。
  - AC-28: `formal-network-tiers-testnet-mechanism` 与 `doc/testing/evidence/legacy-shared-devnet-provenance-2026-07-26.md` 承接历史任务链 `TASK-P2P-040` 的有效语义，明确旧 rehearsal 轨道、`release_candidate_bundle`、promotion/freeze/rollback 与 provenance 边界；当前不作为目标环境、公开 readiness 或玩家世界模型。
  - AC-29: hosted player access 的长期合同由本 PRD 与 `doc/p2p/blockchain/hosted-player-access-operator-runbook.md` 承接，明确 `public player plane / private control plane / identity plane / custody or signer plane`、`guest/player/strong-auth` 会话梯度、player-safe / operator-only action split、public join admission control，以及“无需 invite-only 也不能把长期 signer 暴露给浏览器”的边界；历史 `TASK-P2P-041` 从模块 project、GitHub task evidence 与 Git history 追溯。
  - AC-30: `mainnet-private-reachability-architecture` 专题文档落盘并映射任务链 `TASK-P2P-043`，明确 `public/hybrid/private/relay_only/validator_hidden` 部署模式、`validator core/sentry/relay/full-storage/observer-light` 角色边界、`peer record + discovery + reachability + traffic lanes` 框架，以及 mixed-topology 下的 anti-eclipse / relay budget / claims gate。
  - AC-30A: `mainnet-private-reachability-architecture` 必须补齐 iroh-inspired follow-up 的 truth chain：`PeerReachabilityContract` 负责 runtime reachability 归一化，path behavior matrix 只负责 evidence/claim taxonomy，triad/status observability 只消费 bounded status projection。
  - AC-31: `TASK-P2P-045` 必须把当前链上代币的正式产品名冻结为“绿洲币 / Oasis Coin”，作为后续 runtime 符号与账户派生迁移的前置口径。
  - AC-32: `TASK-P2P-046` 必须把当前链上代币的 runtime `main_token.symbol`、公钥派生账户前缀与签名鉴权前缀统一迁移到 `OC` / `oc:pk:`，并同步 API、viewer/client、liveops、脚本、测试与模块入口文档，不再把 `AWT` / `awt:pk:` 当作现行真值。
  - AC-33: `TASK-P2P-047` 必须把当前链上代币的创世 `initial_supply` 冻结为 `10,000,000,000 OC`，并把 7 个 bucket 的绝对分配额、首年外部释放绝对边界与 formal freeze sheet 的 supply gate 同步回写到 token 专题与模块执行台账。
  - AC-34: `triad-observability-stack` 必须把 real-env triad 的 host/process、chain status、traffic window、wasm window 收敛到统一 repo-owned 监控入口，并在 `testing-manual.md` 冻结 canonical 命令与产物路径。
  - AC-35: `triad-three-equal-validator-topology` 必须把当前 real-env triad 从“本机 observer + 两台云端 validator”提升为“三节点等权 validator”可审计基线，至少覆盖：`3` 个 validator 的 stake/signer binding、local 节点不再以 observer-only 角色运行、repo-owned snapshot/manual 不再把 `partial_with_observer_blocker` 当成唯一有效 claim、same-window evidence 对 legacy service label 与真实 runtime role 的区分，以及 `oasis7_chain_runtime` 在 execution world 已落盘 `governance_finality_signer_registry` 时会优先用该 world-state registry 恢复 validator membership / signer binding；`--node-validator*` 只保留为 bootstrap 或显式运维覆盖。
  - AC-36: `mainchain-token-newapi-quota-bridge` 专题文档落盘并映射任务链，明确 `one-way OC -> LetAI Run OpenAPI quota`、bridge-service 独立部署、唯一入账映射、`bridge_ledger` 幂等对账、动态 project/`token_key`、query verification 与 manual review 风控，以及“不支持自动提现/不承诺公开兑换所”边界。
  - AC-37: `formal-network-tiers-testnet-mechanism` 专题文档与 repo-owned skeleton 必须落盘并映射任务链 `formal-network-tiers-testnet-mechanism (PRD-P2P-028)`，明确 `local_devnet/public_testnet/mainnet` 三层 operator/runtime network-tier 模型、`public_testnet_rehearsal` 的 legacy/rehearsal evidence 边界、`network_tier_manifest` 字段集合、`public_testnet` 的 public RPC/explorer/faucet/reset 语义，以及 `mainnet` 的 `no faucet + frozen reset + MAINNET-1~4` gate；这些 tier 不作为玩家世界模型。
  - AC-38: `hosted-public-join-managed-identity-custody` 专题文档必须落盘并映射任务链 `hosted-managed-identity-doc-freeze (PRD-P2P-029)`，明确 hosted account、多凭证登录（推荐通行密钥、邮箱可选）、`signer_ref`、device session、step-up auth、托管退出与“默认不让玩家管理裸私钥”的正式产品边界。
  - AC-38A: 稳定权威 `mainchain-token-ideal-transaction` 承接 `ideal-transaction-model-doc-freeze (PRD-P2P-028A)` 与 `PRD-P2P-ITX-001/002/003`：明确 signed metadata 字段、完整 JSON、签名域、回执与 Phase 1 最小升级集；fee quote 仍是 metadata-only，真实 fee/auth execution 属于独立 Phase 2+，不得误称当前实现真值。
  - AC-39: `public_testnet` 必须具备 repo-owned readiness review 入口，至少能基于 manifest + lane evidence 输出 `specified_skeleton_only|partial|block|ready_for_live_candidate`；这些是判定枚举，placeholder endpoint / 缺失 candidate bundle / 缺 lane evidence 必须保持阻断。
  - AC-40: `formal-network-tiers-testnet-mechanism.runbook.md` 必须作为 companion runbook 落盘并映射任务链 `formal-public-testnet-live-candidate-checklist (PRD-P2P-028)`，至少冻结当前 11 条 required lanes 的 owner/evidence/check 命令/claim boundary，并以同候选、同窗口 readiness review 的实际 verdict 约束 public claims；历史验收见 [Task #2059](https://github.com/eng-cc/oasis7/issues/2059#issuecomment-4890065749)，不以历史 PASS 或 Task 状态代签当前 readiness。
  - AC-41: `p2p-network-runtime-hardening` 必须让 `libp2p` gossip `publish()` 在路由或连接不可用时向调用方同步返回失败，且失败 publish 不得继续记入 `published` 事件或成功流量统计；同时 replication request 的 retry/fallback/cooldown 判定必须收敛到共享 availability classifier 与稳定 reason prefix，不再在 `oasis7_net`/`oasis7_node` 多处重复猜测自由文本错误串。
  - AC-42: `subscribe-ack-udp-gossip-hardening` 必须让 `libp2p` `subscribe()` 只有在底层 gossipsub 真正订阅成功后才向调用方返回 `NetworkSubscription`；若后端拒绝订阅，调用方必须同步收到失败而不是拿到 dead subscription。与此同时，UDP gossip 路径必须显式拒绝超出 datagram 上限的 replication 消息、把接收缓冲提升到覆盖完整 UDP payload、并在 fan-out 遇到单点 `send_to` 失败时继续尝试其余 peer 后返回汇总错误，而不是靠 4KB 截断或“第一个错误即提前退出”的旧语义。
  - AC-43: `testing-manual.md` 必须明确“链上大世界状态底座”自闭环分层，并把 `module_required/module_full/integration_required/release_full` 的命令、最小指标、阻断签名、禁止 claim 与 real-env/public-testnet 边界写清，避免把底座自测误报为链上大世界或公开网络整体 ready。
- Non-Goals:
  - 不在本 PRD 细化 viewer UI 交互。
  - 不替代 runtime 内核的模块执行细节设计。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 长跑脚本、链路探针、反馈注入、共识日志分析工具。
- Evaluation Strategy: 以在线稳定时长、分叉恢复成功率、反馈链路可用性、错误收敛时间评估。

## 4. Technical Specifications
- Architecture Overview: p2p 模块负责 `oasis7_net`/`oasis7_consensus`/`oasis7_distfs` 与 node 侧分布式运行协同，强调一致性与故障恢复。
- Integration Points:
  - `doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.prd.md`
  - `doc/p2p/design.md`
  - `doc/p2p/network/p2p-mobile-light-client-authoritative-state.prd.md`
  - `doc/world-runtime/runtime/chain-pos-control-plane.prd.md`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
  - `README.md`
  - `doc/product/player-entry-distribution/release-communications-and-public-claims.prd.md`
  - `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md`
  - `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md`（legacy topic label; current generated artifacts use network rehearsal / public-testnet rehearsal terminology）
  - `doc/p2p/blockchain/hosted-player-access-operator-runbook.md`
  - `doc/p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md`
  - `doc/p2p/network/mainnet-private-reachability-architecture.prd.md`
  - `doc/p2p/token/mainchain-token-newapi-quota-bridge.prd.md`
  - `doc/p2p/token/mainchain-token-ideal-transaction.prd.md`
  - `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md`
  - `doc/product/world-rules-core-gameplay/prd.md`
  - `doc/world-simulator/viewer/viewer-manual.manual.md`
  - `doc/world-simulator/launcher/game-client-launcher-runtime-session-continuity.prd.md`
  - `doc/world-simulator/launcher/README.md`（当前 Launcher 发布/分发配置入口；托管 chain runtime 边界见 `game-client-launcher-runtime-session-continuity.prd.md`，旧 Viewer release-config 样例只从固定提交 `25fbcd7cf590c5e1b1248119d407cc7c34e8e634` 追溯）
  - `doc/testing/longrun/p2p-longrun-soak-and-chaos.prd.md`
  - `doc/p2p/token/mainchain-token-allocation-mechanism.prd.md`
  - `testing-manual.md`
- Edge Cases & Error Handling:
  - 节点掉线：共识链路需在节点恢复后自动重同步并验证状态。
  - 网络分区：检测分区后阻断不安全提交并等待合并恢复。
  - 轻客户端弱网：启用低频增量+关键帧同步并保持最终性状态不倒退。
  - 空副本：DistFS 副本不足时触发补副本任务并记录告警。
  - 超时：共识轮次超时后执行回退/重试策略。
  - 并发冲突：同高度多提交候选按共识规则拒绝冲突分支。
  - 数据损坏：校验失败副本立即隔离并重建。
  - 时钟回拨/漂移：wall-clock 出现回拨时禁止 slot 倒退；超阈值漂移进入拒绝或告警路径。
  - 大跨度漏槽：节点恢复后按当前 wall-clock 对齐 slot，并累加漏槽计数，不补历史空块。
  - 控制面兼容：保留 `node_tick_ms` 时必须明确其“轮询/回退间隔”语义，避免误用为 slot/block 时间。
  - 旧参数误用：`oasis7_viewer_live` 若收到 `--release-config` 或任意 `--node-*` 参数，必须立即失败并输出“请改用 oasis7_chain_runtime”。
  - 兼容别名误用：`oasis7_viewer_live` 若收到 `--runtime-world`，必须立即失败并输出“请直接使用纯 viewer 参数”。
  - 创世控盘越界：若单人直持或项目直接液态份额超过 PRD 上限，则创世配置不得冻结。
  - 奖励语义漂移：若 early-player reward 被描述为“登录就发”或“时长挖矿”，则必须退回为 contribution-based 口径重审。
  - 安全等级误判：若局部 `ed25519`/allowlist 能力被误写成“整体已达主流公链安全”，则必须退回并以系统级交易授权/托管/治理真值重审。
  - 阶段误升级：若 `STRAUTH-3` 已完成但生产级 keystore、治理 signer 外部化或创世 ceremony 仍未通过，就把安全阶段升级为 `mainnet-grade`，必须直接阻断并回到 readiness gate 检查。
  - hosted access 误暴露：若 public join URL 仍可命中 runtime/control 启停、链控制或 GUI operator action 等管理接口，则必须直接判定为架构越界而非部署细节问题。
  - 浏览器 signer 泄露：若 HTML bootstrap、JS 全局对象或任意 public API 仍返回长期 signer 私钥、seed 或等价真值，则 hosted access 路径必须直接阻断。
  - 私网节点离网：若家宽 / NAT / CGNAT 节点因缺少公网入站而被默认判定为不可参与，则必须回到覆盖网络架构重审，而不是继续追加静态 peer 补丁。
  - relay 单点依赖：若 private/validator_hidden 节点只剩单一 relay-domain 路径，必须直接降级 verdict，不得继续声称已具备 public-chain-grade mixed-topology。
  - 拓扑安全退化：若 active peer set 集中于单一 operator、ASN 或 `/24`，则必须触发 anti-eclipse 阻断，而不是只要“能连上”就放行。
  - publish 假成功：若 `gossipsub.publish()` 因无路由、无连接或 topic 不可用失败，调用方必须拿到显式错误，且该消息不得继续记入“已发布”事件或成功出站流量统计。
  - fallback 误分类：若 replication fallback 仍依赖多层自由文本 contains 匹配，就可能把可等待的 availability gap、不可等待的 route unavailable 与真正 unsupported handler 混成同一类；判定必须收敛到共享 classifier 与稳定 prefix。
  - 权限混层：若 guest/player session 在没有强鉴权的情况下能执行资产转账、治理或高风险 prompt/control，则必须回退到 hosted access 权限设计审查。
  - admission 失控：若 public join 在没有 `max_guest/max_player/rate_limit/world_full_policy` 的情况下无界签发 session，则必须回退到 hosted access admission 设计审查。
  - bridge 错配：若 bridge-service 无法把一笔 `OC` 入账唯一映射到一个 LetAI `platform_user_id`、`platform_project_id`、`external_order_id` 或 `token_key`，则必须停在 `manual_review`，不得把共享收款账户上的模糊入账自动折算成 quota。
  - credit 半成功：若链上入账已确认、`bridge_ledger` 已写入，但 LetAI OpenAPI 的 user/project/topup/query 任一步失败，则必须保留幂等重试键并阻止重复入账，不得靠人工重复发放覆盖。
- Non-Functional Requirements:
  - NFR-P2P-1: 多节点长跑稳定性指标持续达标并可追溯。
  - NFR-P2P-2: 共识提交与复制链路关键失败模式覆盖率 100%。
  - NFR-P2P-3: 节点异常恢复流程具备标准化操作与证据产物。
  - NFR-P2P-4: 资产与签名链路审计记录完整率 100%。
  - NFR-P2P-5: 协议演进不得破坏既有网络兼容性基线。
  - NFR-P2P-6: 手机轻客户端路径必须可验证最终性，且不要求端侧权威模拟。
  - NFR-P2P-7: slot 计算在重启前后保持单调一致；槽位倒退容忍度为 0（仅允许漏槽）。
  - NFR-P2P-8: 在启用 `ticks_per_slot` 时，logical tick/phase 计算跨节点一致，提案节拍可观测且可回归验证。
  - NFR-P2P-9: S9/S10 若出现失败，必须在同一审计轮次内沉淀 `incident_id/root_cause/fix_commit/regression_command` 四元组证据。
  - NFR-P2P-10: 分布式发布门禁不得接受 `insufficient_data` 作为通过结果；S9/S10 指标门禁结果必须显式为 `pass`。
  - NFR-P2P-11: 控制面参数命名与状态字段在 runtime/game/web/client launcher/scripts 上保持一致，避免语义分叉导致错误调参。
  - NFR-P2P-12: 指标命名必须区分“worker poll”与“consensus tick”；任何对外接口不得将二者混称为同一 tick 语义。
  - NFR-P2P-13: `oasis7_viewer_live` CLI 帮助与错误文案中不得再出现 release/node 控制面入口，避免与 `oasis7_chain_runtime` 控制平面重复。
  - NFR-P2P-14: `oasis7_viewer_live` 仅保留一个生效的 CLI 解析实现；仓内不得存在与生产入口分叉的 legacy 参数解析代码路径。
  - NFR-P2P-15: 模块文档中的源码路径引用必须可解析到当前仓库存在文件，避免审计与回归排障时出现失效链接。
  - NFR-P2P-16: Token 创世分配表必须满足 `sum(allocation_bps)=10000`、项目战略控制目标 `5000 bps`、单人直持硬上限 `1500 bps`、创世液态流通硬上限 `500 bps`，且首 12 个月非团队外部释放目标 `100~200 bps`、硬上限 `500 bps`。
  - NFR-P2P-17: 在资产动作签名交易模型、生产级 keystore、治理 signer 外部化与创世 slot 真实绑定完成前，`oasis7` 不得宣称“对标主流公链安全”或 `mainnet-grade`。
  - NFR-P2P-18: 公开 `transfer submit` 面不得存在无签名旁路；请求级鉴权必须在余额/nonce 预检之前完成，且 `oasis7_web_launcher` 代理结构与 runtime 保持同一字段集合。
  - NFR-P2P-19: 在 `MAINNET-1~4` readiness gate 全部通过前，公开口径最多只能使用 `limited playable technical preview` 与 `crypto-hardened preview`，不得升级到 `production mint ready`。
  - NFR-P2P-20: 生产 signer custody 未外部化前，任何本地 `config.toml`、HTML bootstrap 或长期 env 私钥路径都不得进入 production release allowlist。
  - NFR-P2P-21: 生产 governance truth 未外部化前，任何 deterministic local seed、单机 `NodeConfig` signer policy、手工 env 改动或 `--node-validator*` 参数注入都不得进入 production governance allowlist 或被视为正式 validator admission 完成态。
  - NFR-P2P-22: 在 genesis slot/bucket 真值、ceremony evidence bundle 与 QA `pass` 完成前，任何 `mint_ready` 或 `production mint ready` 口径都不得进入 public claims allowlist。
  - NFR-P2P-23: 在 `MAINNET-1~3` 仍停留于 spec gate 而 execution blockers 未清零时，任何高于 `crypto-hardened preview` 的 public claims 都必须被 denylist 拒绝。
  - NFR-P2P-24: legacy/rehearsal `public_testnet_rehearsal/staging/canary` 证据只能按历史 rehearsal 追溯；不得把它作为当前目标环境、玩家世界模型、formal `public_testnet` readiness、`mainnet` readiness 或“对标主流公链测试成熟度已完成”的依据。
  - NFR-P2P-25: hosted access public player plane 在任何 HTML/JS/bootstrap/API 响应中都不得暴露长期 signer 私钥、seed 或等价真值。
  - NFR-P2P-26: hosted access public join origin 默认不得暴露 runtime/control start/stop、chain start/stop 或 operator-only GUI action 入口；能力不足时前后端都必须拒绝。
  - NFR-P2P-27: hosted access public join 必须具备有界 admission control，至少冻结 `max_guest_sessions/max_player_sessions/issue_rate_limit/world_full_policy`，且超限时返回结构化拒绝。
  - NFR-P2P-28: `doc/p2p/**` 活跃文档、token 专题、模块入口与 runtime/account 相关实现提到当前链上代币时，必须统一使用“绿洲币 / Oasis Coin” / `OC` / `oc:pk:` 作为现行真值；`AWT` / `awt:pk:` 仅允许保留在明确标注的历史语境或兼容说明中。
  - NFR-P2P-29: `OC -> LetAI Run OpenAPI` bridge 必须坚持 `one-way service-credit only`、独立 bridge-service、唯一入账映射、`bridge_ledger` 幂等对账、动态 project/`token_key` 持久化与 operator-review fallback；在公开钱包体系、生产级 custody、双向兑回与价格发现机制缺失前，任何“公开兑换所”“自动提现”“浏览器直连热钱包充值”口径都不得进入 allowlist。
  - NFR-P2P-30: `hosted_public_join` 的正式产品路径按[多凭证合同](blockchain/hosted-public-join-key-based-onboarding.prd.md)推荐通行密钥、保留可选邮箱，并保持服务端托管 player signer 与后续自托管升级；在浏览器仍长期持有托管私钥、账户恢复仍依赖手抄私钥或高风险动作仍没有正式 step-up + custody sign lane 之前，不得声称“任意新用户都已有安全可用的 hosted account + wallet”。
- Security & Privacy: 需保证节点身份、签名、账本与反馈数据链路的完整性；所有关键动作必须具备可审计记录。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (2026-03-03): 固化网络/共识/存储统一设计基线。
  - v1.1: 补齐在线长跑失败模式和恢复手册。
  - v2.0: 建立分布式质量趋势看板（稳定性、时延、恢复、失败率）。
- Technical Risks:
  - 风险-1: 多子系统并行演进带来接口漂移。
  - 风险-2: 长跑测试覆盖不足导致线上异常暴露滞后。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-001 | TASK-P2P-001/002/005 + p2p-network-runtime-hardening | `test_tier_required` | 网络/共识/存储联合验收清单检查 + `libp2p publish/fallback` 定向回归 | 协议边界、跨 crate 兼容与网络错误语义一致性 |
| PRD-P2P-002 | TASK-P2P-002/003/005 | `test_tier_required` + `test_tier_full` | S9/S10 长跑与恢复演练 | 多节点稳定性与故障恢复 |
| PRD-P2P-003 | TASK-P2P-003/004/005 + p2p-network-runtime-hardening | `test_tier_required` + `test_tier_full` | 签名与治理链路审计检查 + replication availability 分类/回退边界回归 | 资产安全、发布风险控制与节点侧 fallback claim 一致性 |
| PRD-P2P-004 | TASK-P2P-006/007 | `test_tier_required` + `test_tier_full` | 轻客户端 intent/finality/challenge/reconnect 闭环验证 | 移动端接入、公平性与可用性 |
| PRD-P2P-005 | TASK-P2P-008 | `test_tier_required` + `test_tier_full` | 固定时间槽单调性/漏槽/重启恢复/未来槽拒绝回归 | 共识时间语义、提案与投票窗口 |
| PRD-P2P-006 | TASK-P2P-009 | `test_tier_required` + `test_tier_full` | 槽内 tick 相位门控、动态调度等待与跨节点节拍回归 | 共识提案节奏、runtime 调度与可观测 |
| PRD-P2P-007 | TASK-P2P-010 | `test_tier_required` + `test_tier_full` | runtime/game/web/client launcher/scripts 参数映射、状态观测与兼容回归 | 控制面调参、运维门禁与观测一致性 |
| PRD-P2P-008 | TASK-P2P-011 | `test_tier_required` | 状态字段语义对齐、launcher 校验文案回归、文档一致性检查 | 运维观测、发行手册与参数治理 |
| PRD-P2P-009 | TASK-P2P-012 | `test_tier_required` | 默认参数一致性、launcher/web 校验文案与手册口径回归 | 默认启动行为、控制面配置与运维认知一致性 |
| PRD-P2P-009 | TASK-P2P-013 | `test_tier_required` | 默认值切换到 `config/chain-pos-defaults.env` 基线并回归 CLI/脚本/文档口径 | 时间锚定基线一致性与默认运行节拍 |
| PRD-P2P-010 | TASK-P2P-014 | `test_tier_required` | `oasis7_viewer_live` legacy 参数拒绝、帮助文案收敛与文档/示例迁移回归 | Viewer/chain 控制面边界一致性 |
| PRD-P2P-011 | TASK-P2P-015 | `test_tier_required` | `oasis7_viewer_live` 删除 `--runtime-world` 兼容别名、移除旧 split CLI 路径并回归手册/测试口径 | CLI 单一事实源与维护成本收敛 |
| PRD-P2P-012 | TASK-P2P-016/018 | `test_tier_required` | 历史文档旧路径替换、历史专题标题零残留校验 + 文档门禁（过程日志除外） | 文档可追溯性与维护效率 |
| PRD-P2P-013 | TASK-P2P-031 | `test_tier_required` | 创世分配专题 PRD/project/design 建档、模块入口映射、文档门禁与差异检查 | Token 创世口径、低流通边界与早期贡献奖励策略 |
| PRD-P2P-013 | TASK-P2P-045 | `test_tier_required` | 正式命名冻结、symbol/ticker 边界说明、模块入口与 token 专题口径同步、文档门禁与差异检查 | 链上代币 public naming 与后续迁移前置边界 |
| PRD-P2P-013/015 | TASK-P2P-046 | `test_tier_required` | `OC` / `oc:pk:` 迁移、签名鉴权前缀同步、API/viewer/client/liveops/tests/docs 回写、文档门禁与差异检查 | 链上代币 runtime/account 当前真值统一 |
| PRD-P2P-013 | TASK-P2P-047 | `test_tier_required` | `10,000,000,000 OC` 创世总量冻结、bucket 绝对分配额/首年释放边界回写、formal freeze sheet supply gate 更新、文档门禁与差异检查 | 链上代币创世绝对总量与低流通绝对边界 |
| PRD-P2P-014 | TASK-P2P-032 | `test_tier_required` | 密码学安全基线专题 PRD/project/design 建档、代码真值盘点、模块入口映射、文档门禁与差异检查 | 安全口径、mainnet-ready blocker 与优先级治理 |
| PRD-P2P-015 | TASK-P2P-033 | `test_tier_required` | 签名交易鉴权专题 PRD/project/design 建档、transfer submit 鉴权实现、control-plane schema 同步与定向回归 | 主链 Token 首个公开资产面签名化 |
| PRD-P2P-016 | TASK-P2P-034 | `test_tier_required` | mainnet-grade readiness 硬化专题 PRD/project/design 建档、剩余 P1/P2 gate 冻结、模块入口映射与文档门禁 | signer custody、治理 signer、创世 ceremony 与 public claims gate |
| PRD-P2P-017 | TASK-P2P-035 | `test_tier_required` | 生产级 signer custody / keystore 专题 PRD/project/design 建档、signer inventory、环境门禁与 readiness project 回写 | signer source boundary、rotation/revocation/audit 与 release policy |
| PRD-P2P-018 | TASK-P2P-036 | `test_tier_required` | 治理 signer 外部化专题 PRD/project/design 建档、governance signer inventory、source-of-truth 门禁、validator/finality signer admission target workflow 与 readiness project 回写 | governance truth、validator admission、failover/rotation/revocation 与 operator ownership |
| PRD-P2P-019 | TASK-P2P-037 | `test_tier_required` + `test_tier_full` | 创世 freeze/ceremony/QA gate 专题 PRD/project/design 建档、freeze sheet blocker 冻结、QA evidence bundle 与 claim gate 回写 | mint readiness、创世执行与对外口径 |
| PRD-P2P-020 | TASK-P2P-038 | `test_tier_required` | public claims policy 复评专题 PRD/project/design 建档、allowlist/denylist 冻结、future upgrade condition 与 readiness 完结回写 | 对外口径、阶段复评与后续升级条件 |
| PRD-P2P-021 | TASK-P2P-039 | `test_tier_required` | 主流公链测试体系 benchmark 专题 PRD/project/design 建档、testing-manual 映射、gap matrix 与执行优先级冻结 | 测试成熟度口径、QA 证据体系与后续 hardening 排序 |
| PRD-P2P-022 | TASK-P2P-040 | `test_tier_required` | legacy network-rehearsal / release-train minimum 专题 PRD/project/design/runbook 建档、three-track rehearsal model、candidate bundle、claims gate 与 `testing-manual` 入口冻结 | 历史 network-rehearsal 执行模型、release train 口径与后续 rehearsal 排序，不作为当前目标环境或玩家世界模型 |
| PRD-P2P-023 | TASK-P2P-041 | `test_tier_required` | hosted player access / session-auth 专题 PRD/project/design 建档、plane split、session ladder、`gui-agent` split、admission control、sensitive-action capability 与 claims boundary 冻结 | hosted access 边界、浏览器 signer 暴露风险与后续实现排序 |
| PRD-P2P-024 | TASK-P2P-043 | `test_tier_required` | 非全公网覆盖网络专题 PRD/project/design 建档、deployment mode / role model / peer record / reachability / traffic lanes 与 claims gate 冻结；2026-06-15 补齐 peer reachability contract、path behavior matrix taxonomy 与 no-iroh/no-libp2p-replacement 边界 | mixed-topology 网络边界、私网节点参与能力与后续框架拆解排序 |
| PRD-P2P-025 | triad-observability-stack | `test_tier_required` | triad host/process monitor、merged observability summary、testing manual 入口、fixture 回归与 real-env smoke；2026-06-15 补齐 bounded reachability/path summary projection 边界 | 当前 real-env triad（本机 + 2 ECS，runtime 已为 three_equal_validator）的 canonical 运维监控入口，并显式区分 legacy service label 与真实 runtime role |
| PRD-P2P-026 | triad-three-equal-validator-topology | `test_tier_required` | live triad validator-set/signer/bootstrap 改造、same-window snapshot evidence、testing manual claim 口径更新与 legacy service label 边界说明 | 三节点等权 validator 拓扑、live 运维真值与 mixed-topology 历史边界 |
| PRD-P2P-027 | mainchain-token-newapi-quota-bridge-proposal | `test_tier_required` | `OC -> LetAI Run OpenAPI quota` 专题 PRD/design/project 建档、one-way bridge boundary、独立 bridge-service、唯一入账映射、`bridge_ledger` 状态机、动态 project/`token_key`、query verification 与 operator risk gate 冻结 | 链上资产到 AI 服务内部额度的受控桥接口径与后续实现排序 |
| PRD-P2P-028 | formal-network-tiers-testnet-mechanism | `test_tier_required` | 正式网络分层 / testnet 机制专题 PRD/design/project 建档、`network_tier_manifest` 脚本+smoke+example manifests、`testing-manual` 入口、current verdict 冻结与 `public_testnet` live-candidate checklist companion runbook | 公共主链式 `local_devnet/public_testnet/mainnet` 目标分层口径、`public_testnet_rehearsal` legacy/rehearsal evidence 边界、manifest 真值、live-candidate checklist 与后续 runtime/liveops 接线排序；不表示 public launch、mainnet、production OC settlement、public validator admission 或 no-reset release；当前 live-candidate 口径以 11-lane readiness script / claims boundary evidence 为准 |
| PRD-P2P-028A | ideal-transaction-model-doc-freeze | `test_tier_required` | 理想化交易升级专题 `prd/design/project` 建档、字段分组、完整 JSON 草案、完整签名域建议、理想回执与最小理想升级集冻结 | 主链交易长期目标模型与未来协议升级对齐基线 |
| PRD-P2P-029 | hosted-managed-identity-doc-freeze | `test_tier_required` | hosted-public-join 托管身份 / 托管密钥专题 PRD/design/project 建档、hosted account/device session/`signer_ref`/step-up auth/self-custody upgrade 边界冻结、模块入口映射与文档门禁 | 普通玩家 hosted onboarding、player custody 产品边界与后续实现排序 |
| PRD-P2P-031 | chain-world-state-substrate-test-closure | `test_tier_required` + `test_tier_full` | `testing-manual.md` S9A 分层文档、S4/S9/S9B/S10/readiness gate traceability、claim boundary 与 forbidden-claim 检查 | 链上大世界状态底座测试口径、模块自闭环与 public_testnet / game-world integration claim boundary |
- S9/S10 长跑结果模板（TASK-P2P-003）:
| 字段 | 说明 | 来源 |
| --- | --- | --- |
| `suite` | `S9` 或 `S10` | `testing-manual.md` 套件定义 |
| `run_id` | 本次运行唯一标识（目录名/时间戳） | `.tmp/p2p_longrun/*` 或 `.tmp/s10_game_longrun/*` |
| `profile` | `soak_smoke/soak_endurance/soak_release` 或 S10 对应档位 | 执行命令 |
| `gate_status` | `pass/fail` | `summary.json` |
| `key_metrics` | `lag_p95/distfs_failure_ratio/consensus_hash_mismatch_count` 等 | `summary.json` |
| `evidence_paths` | `summary.json/timeline.csv/failures.md` | 产物目录 |
- S9/S10 缺陷闭环模板（TASK-P2P-003）:
| 字段 | 填写要求 | 闭环判定 |
| --- | --- | --- |
| `incident_id` | `S9-YYYYMMDD-xxx` 或 `S10-YYYYMMDD-xxx` | 与失败运行一一对应 |
| `symptom` | 失败现象 + 首个告警指标 | 可定位到日志行或指标项 |
| `root_cause` | 技术根因（网络/共识/存储/配置） | 具备可复现实验步骤 |
| `fix_task` | 对应任务 ID（如 `TASK-P2P-00x-*`） | 任务文档可追踪 |
| `fix_commit` | 修复提交 SHA | commit 可检索 |
| `regression_command` | 至少 1 条定向回归 + 1 条长跑复验命令 | 命令可执行且结果通过 |
| `closure_note` | 风险评估与是否阻断发布 | 发布门禁结论一致 |
- 发行门禁分布式质量指标映射（TASK-P2P-004）:
| 指标 | 数据源 | 发布阈值（2026-03-07） | 阻断策略 | 执行责任 |
| --- | --- | --- | --- | --- |
| `S9.topologies[].metric_gate.status` | S9 `summary.json` | 必须为 `pass` | 任何拓扑 `fail/insufficient_data` 直接阻断 | 发行值班工程师 |
| `S9.topologies[].metrics.consensus_hash_consistent` | S9 `summary.json` | 必须为 `true` | 任意 `false` 直接阻断并拉起共识排障 | 共识 owner |
| `S9.topologies[].metrics.consensus_hash_mismatch_count` | S9 `summary.json` | 必须为 `0` | 非 0 直接阻断并要求补 `consensus_hash_mismatch.tsv` 分析 | 共识 owner |
| `S9.topologies[].metrics.lag_p95` | S9 `summary.json` | `<= 50`（由 `--max-lag-p95 50` 注入） | 超阈值阻断并进入网络退化复盘 | 网络 owner |
| `S9.topologies[].metrics.distfs_failure_ratio` | S9 `summary.json` | `<= 0.1`（由 `--max-distfs-failure-ratio 0.1` 注入） | 超阈值阻断并进入 DistFS 复制链路修复 | DistFS owner |
| `S10.run.metric_gate.status` | S10 `summary.json` | 必须为 `pass` | `fail/insufficient_data` 直接阻断 | 发行值班工程师 |
| `S10.run.status` | S10 `summary.json` | 必须为 `ok` | 非 `ok` 直接阻断并要求 `failures.md` | 发行值班工程师 |
| `S10.run.metrics.lag_p95` | S10 `summary.json` | `<= 50`（由 `--max-lag-p95 50` 注入） | 超阈值阻断并回退到性能/网络专项 | 网络 owner |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-P2P-001 | 网络/共识/DistFS 统一验收 | 子系统独立验收 | 可降低跨链路隐性回归风险。 |
| DEC-P2P-002 | 长跑结果进入发布门禁 | 仅开发阶段抽样运行 | 发布质量依赖真实长稳证据。 |
| DEC-P2P-003 | 关键动作全链路审计 | 仅关键节点日志 | 审计深度不足会放大安全风险。 |
| DEC-P2P-004 | 移动端采用轻客户端+链下权威模拟 | 手机端参与权威模拟 | 移动端资源受限，权威性和实时性需分层保障。 |
| DEC-P2P-005 | PoS slot 按 wall-clock 统一公式驱动 | 继续本地 tick 自增 slot | 可消除重启/负载抖动造成的时间语义漂移。 |
| DEC-P2P-006 | PoS 增加槽内 tick 相位门控与动态调度 | 仅保留固定 `tick_interval` 与 slot 门控 | 需要稳定落地 `10 tick/slot` 节奏并降低固定 sleep 漂移。 |
| DEC-P2P-007 | 三线联合验收采用“子系统单测基线 + S9/S10 长跑门禁”双层收口 | 仅保留单测或仅保留长跑 | 单一层级无法覆盖“确定性回归 + 长时退化”双维风险。 |
| DEC-P2P-008 | 分布式质量指标按 `release-gate.sh` 参数固化为“硬阻断” | 发布前人工主观评估放行 | 降低人工判断漂移，确保门禁可复现。 |
| DEC-P2P-009 | 将 `node_tick_ms` 定义为 worker 轮询/回退间隔，并显式暴露 slot-clock 参数 | 继续用 `node_tick_ms` 承担出块时间语义 | 减少运维误配与观测误读，保证时间锚定语义可操作。 |
| DEC-P2P-010 | 在状态与文档中显式区分 `worker_poll_count` 与共识 tick/height 指标 | 继续沿用 `tick_count` 作为泛化进度字段 | 避免“轮询次数=出块推进”的误读，降低误判与误调参风险。 |
| DEC-P2P-011 | 统一 runtime/game/web/client launcher 默认 `slot_duration_ms` 为文档基线值，并收敛校验文案为 poll interval 语义 | 继续维持 `slot_duration_ms=1` 且允许文案混用 tick/block 语义 | 减少“默认启动即偏离锚定口径”的隐性配置风险，降低运维误读。 |
| DEC-P2P-012 | 默认 PoS 时间参数采用 `config/chain-pos-defaults.env` 的统一基线 | 保持 `200/1/0` 等压测导向默认组合，或在多个入口重复维护数字默认值 | 默认体验与协议基线对齐，并避免 runtime/launcher/script 配置真值漂移。 |
| DEC-P2P-013 | `oasis7_viewer_live` 移除 `--release-config` 与 `--node-*` 控制面参数，仅保留观察服务 CLI | 继续在 viewer 保留 release/node 控制面兼容入口 | 避免控制面双入口造成运维误配，统一由 `oasis7_chain_runtime` 承担链参数与节点生命周期。 |
| DEC-P2P-014 | `oasis7_viewer_live` 删除 `--runtime-world` 兼容别名与 legacy split CLI 代码，保留单一生产入口 `oasis7_viewer_live.rs` | 继续保留兼容别名和未接入入口的旧解析代码 | 避免“文档/测试改了但真实入口不生效”的双轨风险，降低后续维护和误判成本。 |
| DEC-P2P-015 | 统一将历史文档中的 `oasis7_viewer_live` 旧文件路径替换为当前源码布局路径（`oasis7_viewer_live.rs` / `oasis7_chain_runtime/*`） | 保留旧路径并依赖读者自行映射 | 降低审计误导与排障成本，确保文档可直接定位现行实现。 |
| DEC-P2P-016 | 先冻结“项目战略控制 50% + 协议奖励池 35% + 低流通 + 贡献制奖励”口径，再决定具体创世账户与执行节奏 | 先广泛发币或直接采用开放式 play-to-earn | 当前阶段仍是 `limited playable technical preview`，需要先守住低流通、可审计与反滥用边界。 |
| DEC-P2P-017 | hosted player access 采用 `public player plane / private control plane / signer plane` + `guest/player/strong-auth` 梯度 | 继续把 join/control/signer 混在单一 web bootstrap 里，或用 invite-only 代替安全边界 | 旧 hosted world 专题的核心问题是信任面混层；现行口径将其视为统一持久大世界的 hosted access 接入面，不是单独玩家世界。 |
| DEC-P2P-018 | 先冻结当前链上代币的正式产品名为“绿洲币 / Oasis Coin”，再单开专题迁移 runtime/account 真值 | 在未评审 API/UI/兼容性影响前直接顺手改 runtime symbol / account prefix | 产品名、symbol、链上字段和客户端展示面属于不同治理层；先冻结 public naming，才能把后续 runtime 改动收成独立可审计任务。 |
| DEC-P2P-019 | 在独立迁移专题中，把当前链上代币的 runtime symbol、公钥派生账户前缀与签名鉴权前缀统一切到 `OC` / `oc:pk:` | 继续让 `AWT` / `awt:pk:` 作为现行真值，或只改产品名不改 runtime/account | 当前 public naming 已冻结，继续双轨会让 API、viewer/client、liveops 与审计口径长期分叉；需要一次把 runtime/account 当前真值收口。 |
| DEC-P2P-020 | `libp2p` 可用性错误分类统一沉淀在 `oasis7_net`，由 node 侧消费共享 classifier 与稳定 reason prefix；`publish()` 改为同步回传 gossipsub 失败 | 继续 fire-and-forget publish，或让 `oasis7_node`/probe gate 各自重复字符串匹配 | 这条路径直接决定 replication fallback、waitable gap 与 unsupported handler 的 claim；若 publish 仍可静默成功、fallback 仍靠重复猜字符串，网络故障会在跨 crate 边界被误报或漏报。 |
