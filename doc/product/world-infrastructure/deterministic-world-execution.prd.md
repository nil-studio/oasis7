# 确定性世界执行

## 文档身份

- 所属产品模块：权威世界基础设施
- 上位产品 PRD：[prd.md](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：`2026-10-12`
- 专业域权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、Agent 与入口/Viewer 的 [`doc/world-simulator/prd.md`](../../world-simulator/prd.md)；入口动作的具体跨表面契约见 [`客户端启动器跨表面受控动作契约`](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)

本文定义分布式共识底层之上的确定性世界执行层。它是基础设施内部的上层：接收已签名意图、在版本化规则边界内确定性重执行、提交已最终化结果，并向游戏、Agent 与玩家入口提供稳定协议；它不定义那些消费者的规则、行为或界面。

<a id="使用者情境正常路径与选择"></a>
### 使用者体验、正常路径与决策

玩家提交行动后需要知道它是否真正生效，Agent 需要基于真实结果继续规划，验证者需要确认同一输入确实得到相同结果。正常路径是：消费者确认目标 `world_id`、已验证可服务状态和协议兼容性，签名并提交 intent；活动验证者按同一版本、有序输入和父状态重执行，在可验证最终性成立后形成 committed receipt，消费者再据此更新世界结论。客户端 compatibility declaration 只说明客户端是否能理解协议，执行版本仍按 §2.2 的 canonical block 与激活边界确定。

未得到已提交结果时，使用者可以查看请求、等待或重新规划；专业域支持安全撤回/替代时才可选择该动作。等待可能延迟行动，激活后的重新校验可能使旧计划不再适用；原请求和替代请求仍须分别呈现真实状态，不能用重提换取免费取消、旧报价、优先级或第二次效果。已经消费的投入、可用的退出处置和实际成本由相关专业合同决定，本专题不承诺完成时间或返还。失效、拒绝、过期和恢复后的选择见 §2.1/§2.2。

### 阅读术语

`intent` 是请求产生效果的意图；`pending` 是尚无世界效果的待决；`committed receipt` 是关联已提交结果的回执，提交端接收回执不等于它。`canonical block` 是权威历史中的执行区块，candidate/proposed block 只是候选；`manifest` 是版本化执行规则及工件的声明，`governing version` 是该区块实际适用的执行版本。`attestation` 是验证者对重执行结果的确认，`replay` 是按原历史和版本重放；`intent lineage` 是原请求及其明确撤回/替代请求的可追溯关联，只有专业域标记的互斥成员才受唯一胜者规则约束。`fail closed`、最终性、世界身份与状态根的含义见[根 PRD 的阅读术语](prd.md#阅读术语)。这些说明不定义 schema、排序算法或新的客户端能力。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 适用边界：仅豁免重复的产品体验配对 design；专业系统设计仍按各功能状态和准确承接独立判定，不因此变为“不适用”。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 只定义确定性提交、待决恢复、幂等和版本边界；执行流程与状态呈现由 runtime/P2P 专业 authority 负责。
- 历史设计判定记录：历史分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，历史闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
提交边界沿用 [P2P 合法 authority profile](../../p2p/prd.md#p2p-authority-profiles)：受控单权威不等于 BFT；超时/断连且不能证明未提交时必须表达提交未知并按原身份核对，下文无效果 pending 只用于已知尚无效果的请求。实际开放路径的投入、产出、事件、结果、里程碑和去重事实共同持久提交，replay 不重发外部行为。固定身份与合法 manifest/profile 激活链保持连续，不把版本升级当新世界。

## 1. 执行与共识边界

- 两种合法 authority profile 复用同一确定性执行；`bft` 下每个活动验证者在 attestation 前完整重执行同一输入。权威世界结果只在适用 profile 的 verified commit proof 与持久提交条件成立后生效；未最终化的 signed intent 不改变世界。
- 共识和执行是独立协议而非必须独立进程：它们通过版本化语义合同通信，in-process 与未来 IPC/network transport 必须通过同一 conformance suite。首个 triad 可单进程共运行。
- 执行版本、manifest 与激活必须可审计、治理激活且 replay-safe。软件 rolling upgrade、治理激活 runtime manifest、独立客户端升级、基础协议 fork 是不同 release lane；若混合版本能对同一输入计算不同权威结果，则必须走协调协议升级/fork。

## 2. 消费者和部署边界

- gameplay、Agent 和 Viewer/玩家入口是基础设施的并发消费者，不是基础设施内的产品循环。游戏提交 intent 并投影 committed state；它不直接写入或以未最终化状态推进权威世界。
- 普通玩家运行游戏加 light companion；operator 运行 full infrastructure node；development/local test 运行游戏加 embedded 或 full local node。所有 profile 使用同一版本化 game/infrastructure 协议，只改变共识、存储、执行和验证职责。
- 最终性不可用时权威 progression fail closed；pending signed intent 在最终化前无世界效果。local development world 必须使用不同 `world_id`，且永不合入全局历史。

### 2.1 Finality 中断时的待决 intent 连续性

- 已签名并送出的 intent 在最终性不可用、陈旧或无法验证时，只能保留为**尚无世界效果的待决请求**。本地排队、提交被接收或界面仍显示请求，都不授予资源、控制权、资格、声誉、阶段完成或依赖它的后续结果，也不得被表达为执行成功或对完成时间的承诺。
- 恢复后，待决请求必须按当时仍有效的权限和前置条件重新进入 canonical 顺序；它可能被执行、拒绝、过期或需要替换。只有可验证的 committed receipt 才能更新玩家、Agent 或入口的世界结论。消费者必须区分仍待决与已无效、被拒绝或须重新规划，并提供查看状态、等待结果，或在专业域允许时明确撤回、替换或重提的路径。
- 替代或撤回请求本身也是待决请求，在各自 committed receipt 出现前，不得单方面取消、覆盖、隐藏或宣称优先于原请求；原请求与其撤回/替代请求之间的关联和真实状态必须可读。若专业域不支持安全的撤回或替代，消费者只能等待、查看状态或重新规划，不能把普通重提伪装成取消。
- 对被专业域明确标记为同一 intent lineage 中互斥的成员，首个产生有效世界效果的 committed receipt 是唯一胜者，并原子地终止其余成员为无效果、不可执行且可追溯的状态。拒绝或过期只终止自身；未标记为互斥的独立 intent 仍可并发。产品层不定义 lineage 标记、排序、原子化、pending 持久化、去重、重试、过期或 receipt schema。

### 2.2 治理激活与在途 intent 的版本边界

- 运行时 manifest 的激活边界是 canonical 世界事实；玩家、Agent、客户端提交时间、本地队列或节点软件版本都不能自行绑定世界语义。请求中携带的 manifest/runtime version 只能作为**非权威 compatibility declaration**，用于检查提交端是否能理解当前协议，不能选择或锁定世界规则。一个 intent 只有在首次进入**已 committed 且 finality-verified 的 canonical execution block** 时才确定执行版本：该 block 位于激活边界之前则使用旧 manifest，位于激活边界或之后则使用新 manifest；candidate/proposed block 不能产生版本选择或世界效果。等待中的请求不会因为本地已经提交或旧节点仍在运行而自动锁定旧规则。
- 激活边界前提交、但尚未取得 committed receipt 的请求，若在新 manifest 下首次进入已 committed/finalized 的 canonical block，必须按新 manifest 的当前权限、资源和前置条件重新裁决；不兼容时只能原子拒绝/过期并给出可读原因，或由玩家/Agent 通过明确的、新版本兼容声明的新请求重新规划。不得静默翻译 payload、沿用旧报价/资格、部分应用旧规则后再套新规则，或让重连把它伪装成已结算。
- 玩家可以查看待决请求、等待 canonical 结果、改道或在专业域允许时撤回/替换；不能把客户端版本、旧缓存、提交回执或本地倒计时当成执行版本、成本、权限、优先级或完成保证。跨版本重提若意图替代原待决请求，必须明确关联为 linked replacement，进入原请求的同一 intent lineage 和互斥成员集合，服从 2.1 的首个有效 committed receipt 原子胜者规则，旧/新 intent 至多产生一次世界效果；未明确关联的新 intent 才能在专业域确认不互斥时独立执行，不能借旧请求或历史 receipt 获得第二次效果。
- 已 committed 的历史结果永远按其所在 block 的 manifest 重放；恢复、审计和 replay 不得用当前 manifest 重新解释旧 receipt，也不得因激活而追溯改写成本、权限、资源或责任。若激活证明、版本工件或兼容性检查缺失/冲突，执行与恢复必须 fail closed，不产生部分世界效果，保留已确认历史并把未确认请求留在真实的待决/拒绝/重新规划路径。

本条只规定版本选择、玩家可见边界与单次效果语义；manifest 字段、兼容矩阵、激活高度、pending 持久化、重试/过期与 receipt schema 仍由 runtime、P2P、Agent 和入口/Viewer 专业权威定义。

## 3. 当前与目标的分离

当前 runtime/consensus 集成不得因本产品目标自动被表述为 BFT-ready、分区恢复完成或可公开发行。完整目标需要 runtime version activation、certificate-gated execution/replication、replay compatibility、恢复后的 root verification，以及与 P2P 相同候选版本的对抗性证据。

<a id="4-叶子需求与可观察验收"></a>
## 4. 叶子要求与可观察验收

这些叶子把本专题的四类可独立失败义务映射到现有 DE 汇总标准；它们只表达产品可观察结果和证据边界，不复制 runtime/P2P 的字段、状态机或 schema。

<a id="req-dwe-001"></a>
### REQ-DWE-001：同输入确定性提交

- Trace ID: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607
- 适用范围：本条要求及对应 AC-DWE-001，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 承接条款](../../world-runtime/design.md#runtime-deterministic-design)；同输入执行与原子拒绝已有设计；全部验证者组合、实例隔离和消费者完整范围仍须逐项核对。
- 实现状态：部分实现
- 实现与验证依据：execution driver 以 committed context 执行并校验/保存 execution state root；determinism 回归比较不同node provenance的两driver根和执行hash。 [源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs)、[源码/回归定义](https://github.com/eng-cc/oasis7/blob/ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad/crates/oasis7/src/bin/oasis7_chain_runtime/execution_bridge/tests/determinism.rs)；[核验任务](https://github.com/eng-cc/oasis7/issues/4695)。本轮为固定源码审读，回归定义不等于实际通过。  本次存储子范围静态依据：[普通保存调用方](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7/src/runtime/world/cognition_economy_world.rs#L98-L119)及[提交后维护路径](https://github.com/nil-studio/oasis7/blob/ed8d201059f84b8bebd04421e1fd0f56008bec11/crates/oasis7/src/runtime/world/persistence_support.rs#L641-L666)；仅核对下列已知差距，源码/测试定义不等于本轮执行通过。
- 核对日期与基线：2026-10-15；需求与实现源码 `ef28aa8461e45fe277ca2ec28b485a1c9a0ac0ad`；GitHub 固定源码审读；入口与已核对范围见上，未执行运行、浏览器、部署或真人验收；本条原验收条件与层级不变。 本次仅增补存储子范围核对：2026-10-17，固定源码 `ed8d201059f84b8bebd04421e1fd0f56008bec11`；静态审读，未运行恢复、故障或部署测试，不刷新上述其他子范围的证据基线。
- 剩余范围：设计仍按上述准确承接及其未覆盖边界；实现与验证方面，该回归使用空action局部输入，不证明全活动验证者、完整实例隔离、越权/缺证原子拒绝及native/WASM/transport组合；本轮未执行回归。 本地普通保存仍有已披露的 [G1](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design) 实现缺口：持久提交结果、调用方内存安装与维护失败尚未完整分离。已有同代持久化和故障回归基础，不能代签提交后维护失败仍保住已承诺世界、未知结果隔离及同请求重试的完整合同；本轮没有新增线上损失证据。 后续设计兼容与验收输入见 [Issue #4838](https://github.com/nil-studio/oasis7/issues/4838)；本处不另定存储方案。

同一 execution version、已排序输入和 parent committed world state 必须产生同一权威结果；缺少适用证明、输入冲突、越权或版本不匹配时，执行不得产生部分世界效果。同一模块能力的不同实例必须保持结果隔离：一次已确认操作只影响其被授权的目标实例；目标不明确、失效或越权时，不能改写其他实例或产生部分业务结果。

- 对应验收：[AC-DWE-001](#ac-dwe-001)。
- 专业域权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。

<a id="ac-dwe-001"></a>
### AC-DWE-001：验证者重执行与拒绝边界

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607；唯一要求声明：[REQ-DWE-001](#req-dwe-001)。

在同一候选中，活动验证者和适用 replay 对相同版本、输入和世界状态得到一致结果；缺证、冲突、越权或版本不匹配的样例被原子拒绝且没有部分 receipt、资源或状态变化。多实例样例须证明对一个实例的操作不会串写另一个实例；无法唯一确认被授权目标的请求无部分业务结果，消费者能够区分拒绝与成功。该验收由 runtime 的执行/回放合同与 P2P/QA 的组合证据共同判定，单节点成功不能代签；实例命令的具体承接见 [runtime 实例命令设计](../../world-runtime/design.md#module-instance-command-design)，设计采纳不表示当前实例隔离已实现。

- 对应需求：[REQ-DWE-001](#req-dwe-001)。

<a id="req-dwe-002"></a>
### REQ-DWE-002：finality 缺失时保持无效果待决

- Trace ID: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405
- 适用范围：本条要求及对应 AC-DWE-002，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 承接条款](../../world-runtime/design.md#runtime-pending-design)；待决恢复已有目标设计；通用 pending 持久字段、消费者反馈与真实恢复范围仍须核对。
- 实现状态：部分实现
- 实现与验证依据：formal-live 的 signed intent 提交只入 Node queue 并答 Received/Volatile；ready=false 在入队前拒绝。formal client lookup 对原请求结果页校验后才输出 Committed，合法分页尚未找到结果输出 Unknown。直接覆盖提交应答不冒充世界结果、该 API 结论由证明资格化的子范围；不是 finality 缺失时完整 durable pending 合同。 [formal_live_service.rs L784–862](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_service.rs#L784-L862)；[client_formal_operations.rs L135–189](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7/src/world_service/client_formal_operations.rs#L135-L189)；[formal_writer.rs L965–1081](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_writer.rs#L965-L1081)；[world_service_execution.rs L12–77](https://github.com/nil-studio/oasis7/blob/0ec10379419b0aaedb9d16157b81c92f4e6e3495/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/world_service_execution.rs#L12-L77)。本轮仅固定源码审读，测试定义及CI汇总不代签业务通过。
- 核对日期与基线：2026-10-17；需求与源码固定 `0ec10379419b0aaedb9d16157b81c92f4e6e3495`，GitHub connector静态审读；未运行本条测试、故障注入、模型、浏览器或部署。旧历史记录保持其原候选/环境，不刷新为本基线实测。
- 剩余范围：设计仍按本卡原系统设计状态与依据保留；实现与验证：无效/陈旧 finality 下所有通用 signed intent 的可读持久 pending、恢复时当前权限/资源/期限重裁决、过期/重新规划语义及玩家和 Agent 实际消费仍未闭合。已有 frozen-original resume 不等于 fresh pending 重裁决；postcommit publication 错误不能推导零 receipt。#4838 J1/J2 allocator/schedule 接续仍待安排，不能据恢复基础已交付写已修复。

签名 intent 在 finality 不可用、陈旧或无法验证时必须保持可读的无效果待决；恢复后按当时有效权限和前置条件重新裁决，只有 committed receipt 才能更新玩家、Agent 或入口的世界结论。

- 对应验收：[AC-DWE-002](#ac-dwe-002)。
- 专业域权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。

<a id="ac-dwe-002"></a>
### AC-DWE-002：待决请求恢复重审

Trace refs: O7-REQ-4fcbad4f-f576-4bcf-b4fc-b027967b0405；唯一要求声明：[REQ-DWE-002](#req-dwe-002)。

finality 中断样例能区分仍待决、被拒绝/过期、须重新规划和已生效；本地排队、重连、提交回执或界面成功不产生资源、资格、控制权或阶段完成，恢复后的结论只由对应 committed receipt 改变。证据必须覆盖恢复重审和消费者可读状态，不把 transport green 当作世界结果。

- 对应需求：[REQ-DWE-002](#req-dwe-002)。

<a id="req-dwe-003"></a>
### REQ-DWE-003：同一 intent lineage 至多一次效果

- Trace ID: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6
- 适用范围：本条要求及对应 AC-DWE-003，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 承接条款](../../world-runtime/design.md#runtime-lineage-design)；互斥胜者已有目标设计；专业互斥标记、持久原子处置与跨消费者竞态仍须核对。
- 实现状态：部分实现
- 实现与验证依据：Agent chat 已实现明确 replaces_intent_id 关联、旧请求 superseded 记录与精确 replay 检查。 [crates/oasis7/src/runtime/world/agent_intent.rs L382-L580](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/agent_intent.rs#L382-L580)。测试定义：[crates/oasis7/src/runtime/tests/agent_intent_v2/lifecycle.rs L110-L605](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/agent_intent_v2/lifecycle.rs#L110-L605)。[相关历史交付/跟踪 #3612](https://github.com/nil-studio/oasis7/pull/3612)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 剩余范围：设计方面，互斥胜者已有目标设计；专业互斥标记、持久原子处置与跨消费者竞态仍须核对；本轮实现与验证细化：这是单一 chat intent 槽位的替代，不能证明通用互斥 lineage 的首个有效 receipt 原子胜出；撤回竞态、拒绝/过期仅终止自身及独立 intent 并发仍未核清。

被专业域标记为互斥成员的原请求、撤回或 replacement 必须共享可读 lineage，并由首个有效 committed receipt 原子决定唯一胜者；拒绝或过期只终止自身，独立且未标记互斥的 intent 不被无故取消。

- 对应验收：[AC-DWE-003](#ac-dwe-003)。
- 专业域权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。

<a id="ac-dwe-003"></a>
### AC-DWE-003：替代、重试与 receipt 重放

Trace refs: O7-REQ-78948fd8-29dc-4e9b-8a38-321ed37b7ad6；唯一要求声明：[REQ-DWE-003](#req-dwe-003)。

竞态中的 linked replacement/withdrawal、重复提交和 receipt replay 样例至多产生一个有效世界效果；未获胜成员被明确终止，独立 intent 仍可按专业合同并发。验收关注单次效果和可追溯关联，不冻结 lineage、去重或 receipt 的实现字段。

- 对应需求：[REQ-DWE-003](#req-dwe-003)。

<a id="req-dwe-004"></a>
### REQ-DWE-004：canonical block 决定 governing version

- Trace ID: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc
- 适用范围：本条要求及对应 AC-DWE-004，保留原文入口、前置条件及测试层级。
- 系统设计状态：部分覆盖
- 系统设计依据：[world-runtime 承接条款](../../world-runtime/design.md#runtime-version-design)；选版与历史回放已有目标设计；激活证明、工件可用性和跨版本替代的完整组合仍须核对。 补充局部依据：[builtin 工件身份](../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-artifact-version-binding)、[identity 完整性](../../p2p/consensus/builtin-wasm-identity-consensus.design.md#builtin-identity-contract)；仅可验证工件子范围，不能替代 canonical block activation 或历史 replay。
- 实现状态：部分实现
- 实现与验证依据：FormalReadPublication 将当前 runtime manifest digest 与 canonical record 的 governing_manifest_digest 对照，不一致拒绝发布读面；版本/manifest 绑定已有真实局部实现。 [crates/oasis7/src/world_service/formal_read_service.rs L20-L49](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_service.rs#L20-L49)；[crates/oasis7/src/runtime/world/release_manifest.rs L148-L259](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/release_manifest.rs#L148-L259)。测试定义：[crates/oasis7/src/world_service/formal_read_adapter_tests.rs L11-L74](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/world_service/formal_read_adapter_tests.rs#L11-L74)。[相关历史交付/跟踪 #4683](https://github.com/nil-studio/oasis7/pull/4683)；本轮仅静态审读，测试定义不等于执行通过，未取得本条全部适用验收的实际运行记录。
- 核对日期与基线：2026-10-17；需求与实现源码 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；GitHub 固定源码审读，未运行业务、部署或真人验收；入口、原验收条件与测试层级保持本条原文。
- 新增局部设计关系核对：2026-10-15；`4e94163a3dff9ba69f6959b406ca3fcedf8aa097` 与本次文档候选；仅核对上述新增局部设计关系，不新增行为验证，既有实现与运行证据基线不变。
- 剩余范围：设计方面，选版与历史回放已有目标设计；激活证明、工件可用性和跨版本替代的完整组合仍须核对；本轮实现与验证细化：完整 canonical activation before/at/after、历史原版本工件 replay、缺/冲突 activation proof、混合版本与跨版本替代仍未核清；读面绑定不等于执行选版全合同。

intent 的 governing version 必须由其首次进入已 committed、finality-verified 的 canonical execution block 与 manifest activation boundary 决定；客户端 compatibility declaration、提交时间、本地版本或 candidate block 不能选择世界规则。已确认历史按原 block 的 manifest replay，激活证据缺失或冲突时 fail closed。

- 对应验收：[AC-DWE-004](#ac-dwe-004)。
- 专业域权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)。

<a id="ac-dwe-004"></a>
### AC-DWE-004：版本激活窗口与历史回放

Trace refs: O7-REQ-229af15b-05ad-4a1b-85f4-f7baf15f10bc；唯一要求声明：[REQ-DWE-004](#req-dwe-004)。

激活窗口样例证明待决 intent 在激活后按新 manifest 重新校验，不兼容时原子拒绝/过期或由主体明确重提；不能静默翻译 payload、沿用旧报价/资格或产生部分副作用。旧 receipt 仍按原 block manifest replay，缺失或冲突的激活证据阻断执行和恢复，消费者能区分待决、拒绝/过期、重规划与已结算。

- 对应需求：[REQ-DWE-004](#req-dwe-004)。

## 5. 组合验收

- DE-1：两种 profile 复用同一确定性执行；相同 execution version、已排序输入和 world state 在 BFT 全部活动验证者上产生相同结果；缺证、冲突、越权或版本不匹配的输入不产生部分副作用。
- DE-2：游戏/Agent/入口通过稳定协议仅见 committed state，能验证或获得适用证明，并在 finality 缺失时将 pending 表达为无世界效果的待决请求，而非结果。恢复样例必须证明待决请求按当时条件重审、只有 committed receipt 更新结论，并能区分待决、无效/拒绝、须重新规划及已生效；对明确互斥的同 lineage 成员，竞态、替代/撤回、重复重试和 receipt 重放至多产生一个有效世界效果，拒绝/过期不取消独立 intent。
- DE-3：执行升级、snapshot/replay、node recovery 与版本混合的样例证明同一 `world_id` 历史和 state root 连续；未证明则 fail closed。
- DE-4：版本激活窗口样例证明 intent 的 governing version 由首次进入**已 committed/finalized 的 canonical block** 与激活边界确定，而不是由提交端的非权威 compatibility declaration、提交时间或本地版本决定；candidate/proposed block 不产生世界效果。激活前待决请求在激活后按新 manifest 重新校验，不能兼容时原子拒绝/过期或经明确的新版本请求重提，不能静默翻译、沿用旧报价或产生部分副作用。旧请求与明确 linked replacement 的新请求发生竞态时，必须进入 2.1 的同一 intent lineage/互斥成员规则，由首个有效 committed receipt 原子胜出，另一方无效果，合计至多一次世界效果；未关联且专业域确认独立的新 intent 才可并发。历史 receipt 按原 block 的 manifest replay，激活证据缺失/冲突时 fail closed。玩家可区分待决、拒绝/过期、重规划与已结算结果。测试层级：`test_tier_full`。

## 6. Non-Goals

- 不定义配方、设施、市场、区域/组织治理、Agent 决策、玩家动作、UX 或数值平衡。
- 不定义 BFT 消息、签名格式、存储实现、节点部署或具体运行手册。

## 未决问题与追踪解释

准确专业设计及 QA 场景回链见下表；[历史文档接收任务 #4180](https://github.com/eng-cc/oasis7/issues/4180)记录当时三层接收与文档检查，不能代替实现、semantic verdict 或 full 执行证明。设计/接收可定位的问题可据此复核，仍缺的当前能力、同候选证据与真实环境保持未证明；专业 owner 与 QA 的判定权不变。

本专题四组叶子并不代替根 SC-1～SC-10 的组合验收。追踪表中重复的工业状态锚点与模块导航不能单独证明通用确定性、待决、互斥 lineage、版本激活的系统设计承接；产品 owner 不代替 runtime/P2P 的技术 owner、Agent/Viewer 的消费者 owner 或 QA 的验证判断。正文目标、专业设计、当前实现与执行证据分别成立，不能由文档采纳推导能力或发行完成。

| 未决问题 / 影响 | 决策 role 与所需信息 | 解决触发与临时边界 |
| --- | --- | --- |
| DWE-001 的同输入/父状态/版本重执行和原子拒绝、DWE-002/003 的待决持久化、恢复重审及互斥胜者由哪些真实设计条款承担？ | runtime、P2P、消费者 owner 与 QA；需要技术接受条款、状态/原子边界及竞态、独立 intent、消费者场景 | 专业设计与验证场景获批准后回链；此前工业状态锚点或单节点成功不构成通用承接及组合证据 |
| DWE-004 的激活证明、工件可用性、首次已提交区块选版、跨版本 linked replacement 与历史 replay 如何共同验证？ | runtime、适用 WASM/模块 owner、P2P、消费者 owner 与 QA；需要激活窗口、历史版本、失败关闭和消费者重规划判定 | 条款及同候选验证设计可定位后复核；缺证/冲突时仍按正文拒绝或无效果待决，不锁定旧版本、不静默翻译 |
| DWE-001 的 required 局部检查与 SC-4 的 full 组合证据如何区分？ | runtime/P2P 与 QA；需要全部活动验证者及 replay 的适用范围和证据身份 | 验证设计明确范围后确认；局部检查不能代签组合条件，DWE-004/DE-4 仍按 full 验收 |

问题的历史接收来源为[已关闭的文档迁移协调任务 #3935](https://github.com/eng-cc/oasis7/issues/3935)；该任务已关闭，不恢复其当前义务；任务、依赖和解决进度由 Git、PR 与实际 CI 记录维护，本表不维护第二台账。下面的“验证证据”导航描述不表示测试已接入或已执行，准确专业设计及验证回链尚须复核。

## 全量语义追踪

[`Agent 消费`](../../world-simulator/llm/continuous-agent-harness.prd.md#harness-infrastructure-consumer-matrix)、[`Agent join`](../../world-simulator/llm/continuous-agent-harness.design.md#harness-infrastructure-evidence-join)、[`Viewer 读回`](../../world-simulator/viewer/viewer-control-plane-split-live-playback.prd.md#viewer-infrastructure-readback-contract)、[`Viewer 捕获`](../../world-simulator/viewer/viewer-manual.manual.md#viewer-infrastructure-capture-receiver)、[`Viewer join`](../../world-simulator/viewer/viewer-control-plane-split-live-playback.design.md#viewer-infrastructure-evidence-join)接收跨读面结果；[equal-version MIG-01](../../testing/prd.md#qa-infrastructure-equal-version-negative)仅是已合并 H3 设计下 shape-valid `1→1` 的 planned/unexecuted semantic negative，预期拒绝、保持旧状态及零部分效果。它不代替四组 DWE、DE 或根 SC 的完整回链。

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-DWE-001](#req-dwe-001) / [AC-DWE-001](#ac-dwe-001) | `producer_system_designer` |[`doc/world-runtime/prd.md`](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、Agent 与入口/Viewer 的 [`doc/world-simulator/prd.md`](../../world-simulator/prd.md)；入口动作的具体跨表面契约见 [`客户端启动器跨表面受控动作契约`](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)；[`runtime 设计`](../../world-runtime/design.md#runtime-deterministic-design)、[实例命令设计与目标验证](../../world-runtime/design.md#module-instance-command-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-EXECUTION`，独立承接 DE-1；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_required` |
| [REQ-DWE-002](#req-dwe-002) / [AC-DWE-002](#ac-dwe-002) | `producer_system_designer` |[`doc/world-runtime/prd.md`](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、Agent 与入口/Viewer 的 [`doc/world-simulator/prd.md`](../../world-simulator/prd.md)；入口动作的具体跨表面契约见 [`客户端启动器跨表面受控动作契约`](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)；[`runtime 设计`](../../world-runtime/design.md#runtime-pending-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-EXECUTION / V-CONSUMER`，独立承接 DE-2（无效果待决）；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_required` |
| [REQ-DWE-003](#req-dwe-003) / [AC-DWE-003](#ac-dwe-003) | `producer_system_designer` |[`doc/world-runtime/prd.md`](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、Agent 与入口/Viewer 的 [`doc/world-simulator/prd.md`](../../world-simulator/prd.md)；入口动作的具体跨表面契约见 [`客户端启动器跨表面受控动作契约`](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)；[`runtime 设计`](../../world-runtime/design.md#runtime-lineage-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-EXECUTION / V-CONSUMER`，独立承接 DE-2（互斥 lineage）；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_required` |
| [REQ-DWE-004](#req-dwe-004) / [AC-DWE-004](#ac-dwe-004) | `producer_system_designer` |[`doc/world-runtime/prd.md`](../../world-runtime/prd.md#industrial-execution-status-and-authority-matrix)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)、Agent 与入口/Viewer 的 [`doc/world-simulator/prd.md`](../../world-simulator/prd.md)；入口动作的具体跨表面契约见 [`客户端启动器跨表面受控动作契约`](../../world-simulator/launcher/game-client-launcher-cross-surface-action-parity.prd.md)；[`runtime 设计`](../../world-runtime/design.md#runtime-version-design)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-qa-negative-matrix)、[`GWSC 目标场景`](../../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-runtime-scenarios)| [`QA 场景`](../../testing/prd.md#qa-infrastructure-scene-families)：`V-EXECUTION / V-CONSUMER`，独立承接 DE-4；DE-3 历史连续性；[`QA 方法`](../../testing/design.md#qa-infrastructure-evidence-method) 与 [`环境与 verdict`](../../testing/prd.md#qa-infrastructure-tier-boundaries) 固定同候选 oracle/artifact/环境及未证明边界。原正文验收条件全部保留，链接不是执行证明 | `test_tier_full` |
