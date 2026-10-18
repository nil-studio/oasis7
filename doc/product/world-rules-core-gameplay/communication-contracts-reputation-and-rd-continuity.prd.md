# 沟通、合同、声誉与 R&D 连续性

## 文档身份

- 所属产品模块：世界规则与玩法系统
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-26
- 专业域权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)

本文是长期产品分册，定义沟通、合同、争端、情境声誉与研究开发归因的产品边界。它不定义聊天/Agent UI、合同字段、抵押数值、争端随机算法、证据格式、仲裁规则、声誉分数、身份技术、专利/法律结论、版税比例或当前实现/readiness。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 的合同、服务失败、声誉和 R&D 连续性是产品结果合同；独立 design 不增加另一层玩家信息或控件语义。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
本节的产品体验设计豁免不等于专业系统设计不适用；各要求的技术覆盖与实现证据分别按下列状态摘要核对。

## 1. 产品目标

玩家可以自由协商、委托 Agent 并在同一世界中形成可信协作，但非绑定交流不能被误当作已成交或强制义务。只有经有效授权、明确接受和权威校验的 Agent-rendered 合同才产生绑定世界后果。持续服务允许有成本的违约、证据和救济，而争端、声誉与 R&D 归因保持局部、可审计、可更新且不把身份永久污名化或私有化。

本目标不声称当前社交事实、合同、仲裁、声誉或 R&D 机制已经实现；现有社交事实与产业专业合同仍是更窄的专业真值，任何当前可用性须由相应 authority、QA 和根 `README.md` 支持。

<a id="2-范围与玩家边界"></a>
## 2. 范围与玩家体验边界

### 2.1 沟通与绑定合同

- 直接的人类聊天、口头协商、意向、草稿、点赞或未接受提议均为非绑定沟通；它们不能自动转移资产、承诺服务、改变权限或产生违约责任。
- 只有当 Agent 在有效 owner/组织授权范围内渲染明确条款，相关主体明确接受，并经权威世界校验后，合同才绑定双方。合同 receipt 必须让当事人读懂授权来源、接受、范围、主要义务、状态和后续救济；Agent 不能以自主性把未授权沟通改写成绑定义务。
- 可确定且可即时结算的交换使用 hard atomic 结算：要么按规则完整发生，要么不产生部分世界效果。跨时间的持续服务、维护、运输或协作可以是可违约义务，并需要与风险相称的 collateral、证据、补救、重新履行、解除或仲裁路径；不得把持续服务伪装为原子保证。
- Agent、设施或组织的控制权转让、退休、解散或资不抵债，不会静默取消既有持续义务、制造部分结算，或把责任自动转给新 owner。在新的高风险承诺被冻结后，既有合同只能依其条款履行、终止或结清；只有相关当事人明确同意并经权威校验、留下可审计 receipt 的继受，才能由新主体承担后续义务。可识别托管资产、债权、已发生结果和责任链须在随后转让、重组、拍卖、退休或剩余分配前保持可追溯；该顺序不改变未争议的既有世界历史。

#### 2.1.1 持续服务承诺的预览、检查点与恢复

- 维护、运输和协作等跨时间服务不是“接受即完成”的 hard atomic 交换。玩家或 Agent 在接受前必须能比较一份只读的 `service_commitment_preview` 目标语义：对方及授权来源、服务目的、范围与期限、首个检查点价值、前置成本与 collateral、持续义务、失败暴露、退出/修复选项、无合同替代路径、推荐动作，以及报价所依据的权威状态与有效边界。预览与 `offered` 都属于未绑定阶段，不预留资产或资格，不锁定、不排队、不增加债务/collateral/奖励/优先级，也不改变声誉、权限或继受关系；报价所依据的授权、条件或权威状态在提交前变化、不确定或过期时，只能重新报价或原子拒绝，不能把陈旧预览表述为保证。本目标语义不声称当前 preview 或玩家 surface 已实现或 ready。
- 产品结果按概念顺序表达为 `non_binding_draft -> offered -> accepted_pending_validation -> active -> checkpoint_due -> fulfilled | partial | defaulted | terminated | disputed`；这些是产品层结果标签，不冻结 runtime enum、schema、金额、UI 或法律结论。`non_binding_draft` 是合同前沟通类别，只有授权完整的 offer 才能进入 `accepted_pending_validation`；接受待校验仍不产生绑定或世界效果。只有有效授权、明确接受和权威校验 receipt 同时成立，合同才进入 `active` 并创建约定义务。
- `checkpoint_due` 只是约定检查点已到的可读条件，不自动产生处罚、违约、债务或资产转移。每个检查点和状态转换都必须留下可审计 receipt，分别说明已交付价值、未履行范围、已发生义务、collateral 暴露和可执行下一步。`partial` 只适用于可违约持续服务，且必须分别记录已完成与未完成范围；它不能伪装为 hard atomic 成功。排队、在途、客户端显示或未确认重试不能伪装为 `fulfilled`；`defaulted` 必须来自合同预先声明的条件、证据和权威判断，而不是法律结论；`fulfilled` 或 `terminated` 也不抹去已发生义务与历史 receipt，且不触发静默续期。
- 检查点部分履行或失败时，只冻结对应范围内新的高风险承诺，保留已确认结果、已发生成本和未决责任。玩家必须能在原合同范围内明确选择 `repair/reperform`、`terminate/settle`、`dispute/preserve` 或 `replace_with_explicit_successor`；无安全路径时则明确停止或重新规划。修复/重新履行只覆盖未满足范围，不能重放已完成效果；同一合同 lineage 的 retry、reconnect 或重复提交必须幂等，不能产生第二次交付、扣款、优先级或义务。续期不得自动发生，必须基于当前权威状态重新报价、明确接受并生成新的 receipt；它只创建本次新披露的范围、成本与义务，不继承旧优先级或静默转移责任。不得把失败变成隐藏欠费、把 collateral 变成自由奖励，或通过反复取消/重签洗掉合同 lineage、失败记录与排队边界。
- `dispute/preserve` 继承 2.2 的最小保全边界：只保护直接争议的资产、collateral、权限或不可逆效果，记录范围、原因和 receipt，并让无争议、到期且可安全履行的义务继续执行；它不能升级为全面冻结、单方控制或隐藏风险转移。显式继受是当事人另行同意、经权威校验并留下 receipt 的延续事件，而不是自动状态迁移；控制权变化期间冻结新的高风险承诺，同时保留旧合同历史、托管资产、债权和责任链，不继承个人 credential 或情境声誉。

### 2.2 receipt-first 的有限争端程序

- 争端先以合同、行动、世界状态、沟通接受和其他可审计 receipt 为事实基础，而非事后声望、财富、组织权势或不可核验叙事。
- 合格争端由随机且无利益冲突的本地 panel 审查；panel 的地域/事项权限、排除冲突、证据范围和结果必须有界。它不是任意全局法院，也不能重写未争议的世界历史。
- 提交争端本身不暂停全部合同或制造免费的风险转移：未争议、到期且仍可安全履行的义务继续按原合同处理。只有争议直接涉及的资产、collateral、权限或不可逆世界效果，才可在有理由、可审计且尽量可逆的最小保护范围内被保全；保全不得扩大为一方对对方资产、行动或声誉的单方控制，也不得新增义务、处罚、收益或新的高风险承诺。
- 争端期间的 receipt 必须区分未争议的已确认结果、待审事实、被保全的范围和原因，以及当事人仍可执行的履行、补证、和解、等待或停止路径。最终处理只能按授权结果结清、恢复、补救或解除对应范围；不得借由保全静默没收资产、抹去历史、追溯伪造已履行事实，或把一次争端变成无限期冻结。
- 当事人拥有有限的程序性申诉：针对授权、证据遗漏、利益冲突、程序越界或可验证错误进行复核。申诉不等于无限重审、拖延履约、用外部声量取代 receipt，或自动推翻权威世界已确认的无争议部分。

### 2.3 情境声誉与转让边界

- 声誉分别属于玩家、Agent、组织和角色/岗位，并限定在相关地点、服务、合同、时间和受影响群体的情境中。它不能被压缩为全球万能分数，也不能从一次成功、付费、名望或组织规模自动推出所有场景的信任与权力。
- 声誉记录具有新近性、到期、可补充更新与申诉路径；严重或重复事实可以在适用情境内持续可见，但不存在不可纠正的永久 blacklist。任何限制都应说明来源、范围、期限、更新/恢复条件和可用复核。
- 在玩家发布或质疑社会事实、或声明关系前，必须提供只读的后果预览，列明可能受影响的主体，以及相关合作、治理、claim 表面和争端/质押风险，帮助玩家决定继续或暂缓。预览只呈现情境内的可能影响，不代表事实已确认或质押已处理；它本身不改变事实、关系、声誉、资格、权限或其他世界状态，也不保证合作、访问、排名、治理权重或奖励。相关记录继续遵守本节已有的情境范围、到期、更新和申诉边界，不扩展为全局分数、永久 blacklist 或 credential 自动转移。
- Agent、设施、合同或组织控制权转让时，相关世界历史、风险和已确认 receipt 仍需披露；但玩家的个人、政治或社会 credential 不随资产/控制权自动转移、出售或继承。新的 owner 只能在自己的授权、关系和情境记录下重新建立资格。
- 当情境声誉被成熟世界用于未来机会或候选资格时，本分册只主责主体/情境归属、来源、期限、更新与申诉的可追溯记录；成长、独立基线、容量竞争、待决/hold/receipt 和单次机会效果由[`成熟世界成长与区域参与`](mature-world-progression.prd.md#req-wr-mw-004)主责。它不把声誉记录扩成区域分配、pioneer priority、治理权或 Agent delegation authority。

### 2.4 预声明 R&D charter、归因与份额

- 合作研究、发现或发明若要求归因、版税或收益份额，必须在实质工作开始前由可读的 R&D charter 声明目标、贡献角色、归因方法、可适用收益/版税范围、争议路径和变更边界。事后占有、单方模型输出或组织权势不能抹去已记录贡献。
- 参与的 Agent 保留永久 provenance：其身份、授权下的贡献、所用训练/模块来源和关键 receipt 可被追溯。Agent provenance 不是把 Agent 变成法律人格、永久锁定人员或自动赋予所有经济权利。
- 研究产生的 share、许可或其他可转让份额只在明确的转让/继受合同下移动；Agent/玩家/组织的身份、个人政治 credential、一般声誉或历史因果不因 R&D 份额转移而自动转移。

### 2.5 可验证的合同、救济与归因要求

<a id="req-wr-cr-001"></a>
### REQ-WR-CR-001：绑定合同必须经过授权、接受与权威校验

Trace ID: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付原子经济合同的主体接受子义务：Simulator动作映射经生产分发进入Runtime，Accept检查主体、Open状态、指定counterparty及到期条件，并生成EconomicContractAccepted进入publication。明确条款渲染、offered/accepted_pending_validation零效果与玩家授权/权威receipt完整绑定仍未核清。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_policy_contract.rs#L793-L1004)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/economic_contract_publication.rs#L30-L111)；[测试定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/gameplay_protocol_regressions.rs#L1-L140)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing.rs#L448-L457)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/llm_sidecar_runtime_support.rs#L250-L268)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：未核清明确条款渲染、双方授权接受、绑定义务创建及同一 receipt 的端到端流程；社交消息或 action ack 不证明合同已成立。

- 要求：人类沟通、草稿、未接受提议和 Agent 建议保持非绑定；只有授权范围内渲染的明确条款、主体明确接受并获得权威校验 receipt 后，才可以创建绑定义务和世界效果。
- 专业权威：[`doc/game/prd.md`](../../game/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)
- 验收：[AC-WR-CR-001](#ac-wr-cr-001)

<a id="ac-wr-cr-001"></a>
### AC-WR-CR-001：预览与待校验接受不伪装成交付

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；声明：[REQ-WR-CR-001](#req-wr-cr-001)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-CR-001
- 给定：一份包含授权来源、范围和义务的 Agent-rendered offer，以及一份普通聊天或草稿。
- 当：玩家查看、接受或重连到这两类沟通结果。
- 则：普通沟通和 `offered`/`accepted_pending_validation` 只显示非绑定状态且不预留、扣款或改变权限；只有授权、明确接受和权威 receipt 同时成立时才显示 active 义务。

<a id="req-wr-cr-002"></a>
### REQ-WR-CR-002：持续服务失败必须提供有界恢复而不重复效果

Trace ID: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d

- 系统设计状态：未核实
- 实现状态：未实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：持续服务本身尚未实现：新建与结算生产入口均显式拒绝Service；结算及过期事件准备层也拒绝Service（Opened/Accepted准备分支没有该拒绝），过期生命周期仅处理AtomicExchange。已交付的原子交换、通用恢复与幂等基础不属于本条持续服务检查点恢复交付。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_policy_contract.rs#L793-L809)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/event_processing/action_to_event_policy_contract.rs#L1035-L1051)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/economic_contract_publication.rs#L504-L512)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/world/gameplay_loop.rs#L292-L315)；[测试定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/runtime/tests/gameplay_protocol_regressions.rs#L104-L140)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：持续服务的collateral/evidence/remedy、检查点已履行/未履行责任、修复/终止/dispute/preserve、续期重报价与重试去重尚未交付；不能以AtomicExchange或通用恢复代替。

- 要求：跨时间服务按检查点保留已完成结果、未履行范围、责任和可执行下一步；玩家可选择修复/重新履行、终止/结清、最小争议保全或显式继受，且 retry、reconnect 和续期不能重复交付、扣款或静默转移责任。
- 专业权威：[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- 验收：[AC-WR-CR-002](#ac-wr-cr-002)

<a id="ac-wr-cr-002"></a>
### AC-WR-CR-002：检查点失败只影响对应范围

Trace refs: O7-REQ-805fc92a-bbcb-4e77-b2d8-e40105522a0d；声明：[REQ-WR-CR-002](#req-wr-cr-002)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-CR-002
- 给定：一个已 active 的持续维护、运输或协作合同在检查点部分完成或 default。
- 当：玩家重连并选择修复、终止或 dispute/preserve。
- 则：receipt 区分已完成和未完成范围，只冻结直接相关的新高风险承诺；修复不重放已完成效果，争议不扩大为全面冻结，续期必须重新报价并明确接受。

<a id="req-wr-cr-003"></a>
### REQ-WR-CR-003：社交事实预览与声誉、R&D 归因必须保持情境边界

Trace ID: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905

- 系统设计状态：未核实
- 实现状态：部分实现
- 系统设计依据：本条产品与专业权威已按原适用范围复核；尚未建立覆盖下列剩余义务的准确专业设计条款、反向承接和验证计划组合。相邻设计或产品体验 design 不作为完整技术覆盖依据。
- 实现与验证依据：已交付社会事实发布/挑战/裁决/撤销的局部生命周期及提交前只读影响预览，Viewer实际调用kernel quote；这是本条的一部分，不覆盖R&D charter、credential/份额转让或世界机会消费组合。 [源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/simulator/kernel/social.rs#L17-L86)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/simulator/kernel/social.rs#L397-L640)；[源码](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/viewer/runtime_live/social_quote.rs#L334-L499)；[测试定义](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7/src/simulator/tests/social.rs#L55-L103)。本轮只读静态源码与断言，未运行Rust或正式玩家入口；同SHA CI的业务job为skipped，不能记为这些测试通过。实现交付PR到本条的完整映射仍未核清。
- 核对日期与基线：2026-10-17；需求、专业设计与静态源码基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；仅GitHub只读源码及CI日志审查，未执行本条业务测试、正式入口、跨节点或真人验收。
- 剩余范围：社会事实专业设计覆盖 lifecycle/只读预览的一部分，但与本条的准确反链及 R&D charter、份额/credential 不转移、机会消费组合未闭合。

- 要求：玩家发布或质疑社会事实、或声明关系前，必须能查看只读预览，了解可能受影响的主体、合作/治理/claim 表面和争端/质押风险，并据此选择继续或暂缓；预览不判定事实、不处理质押、不改变资格或权限、不保证合作、访问、排名、治理权重或奖励。声誉按主体与情境记录，并支持到期、更新和有界申诉；R&D 在工作前以 charter 声明贡献与份额，Agent provenance 持续可追溯，份额转移不得自动转移个人 credential 或一般声誉。当该记录被世界内机会消费时，仍由[`成熟世界成长与区域参与`](mature-world-progression.prd.md#req-wr-mw-004)约束机会的作用域、容量、待决、结算和恢复边界；入口组合边界见[`免费进入、世界内成长与有界认可`](../player-entry-distribution/free-entry-world-progression-and-recognition.prd.md#req-entry-free-002)；本要求不授予区域分配或优先级，也不声称当前实现或 readiness。
- 专业权威：[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md)、[`social-fact-ledger-declarative-reputation.prd.md`](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md)、[`social-fact-ledger-declarative-reputation.design.md`](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md)、[`doc/game/prd.md`](../../game/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)
- 局部专业设计导航：Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[生命周期五项局部承接](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-lifecycle-design)与[逐项验证](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#111-验证映射表)仅支撑 lifecycle/只读预览；以下状态及剩余范围不变，不代表整项要求已覆盖。
- 验收：[AC-WR-CR-003](#ac-wr-cr-003)

<a id="ac-wr-cr-003"></a>
### AC-WR-CR-003：社交事实预览、转让、更新与申诉保持情境边界

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](#req-wr-cr-003)；适用范围沿用本 AC 原覆盖条件。

- 覆盖要求：REQ-WR-CR-003
- 给定：玩家准备发布或质疑社会事实、声明关系；主体在特定地点、服务和时间范围内有情境声誉记录，并参与已声明 R&D charter 的工作。
- 当：玩家在提交社交事实或关系动作前查看后果，或发生控制权/份额转让、声誉到期更新或主体提出复核。
- 则：预览在提交前只读展示可能受影响的主体、合作/治理/claim 表面及争端/质押风险，足以让玩家决定继续或暂缓；它不确认事实、不处理质押、不授予资格或权力，也不承诺合作、访问、排名、治理权重或奖励。历史 receipt、贡献和 provenance 仍可追溯；新主体不会继承个人 credential 或全球信用，声誉可按情境到期、更新和有界申诉，份额只有经显式合同转移；不产生全局声誉分或不可纠正的永久 blacklist。

## 3. 权威与冲突处理

| 产品层拥有 | 专业与执行权威 |
| --- | --- |
| 沟通与绑定合同的结果分层、atomic/持续服务区别、有限争端程序、情境声誉、社交事实决策预览、转让披露边界与 R&D 归因/份额的组合产品语义 | `doc/game/prd.md` 拥有玩家行为、经济和玩法平衡；[`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md) 拥有社交动作的玩法呈现边界；[`social-fact-ledger-declarative-reputation.prd.md`](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md) 与配对 [`design`](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md) 拥有社交事实/关系声明的技术 schema、权限、证据/质押校验、生命周期、事件与回放；`doc/world-runtime/prd.md` 拥有合同/资产/receipt 的权威执行、确定性和恢复；`doc/p2p/prd.md` 拥有身份、签名、治理和安全技术边界；`doc/testing/prd.md` 拥有证据与当前 verdict |

本分册不扩大当前社交事实 adjudication 或工业专业合同，也不定义合同/仲裁/声誉/R&D 的实现、数值或法律效果。与 Agent 授权、组织 charter、区域治理或安全保护冲突时，以更窄的授权、权利与安全边界为准，并由相关专业 owner 显式裁决。

## 4. 路线图

1. 可信沟通：明确人类非绑定交流与 Agent 授权/接受后绑定合同之间的界线。
2. 有限违约与救济：区分原子结算和持续服务，让争端以 receipt 和本地无冲突程序处理。
3. 可恢复信任：使声誉按主体与情境更新，而不制造永久全球污点或可买卖人格。
4. 持续归因：以预声明 R&D charter、永久 Agent provenance 与显式份额转让保护合作历史。

## 5. Done：成功标准与验收

- CR-1：沟通与合同样例证明直接人类交流保持非绑定，只有有效授权、明确接受和权威校验后的 Agent-rendered 合同产生可审计义务。
- CR-2：结算样例区分 hard atomic 交换与可违约持续服务；后者能说明 collateral、证据、补救、重新履行、解除或仲裁，而不产生隐藏部分结算。控制权转让、退休、解散或资不抵债样例会冻结新的风险承诺，并在履行、终止/结清或经明确同意的可审计继受前，不静默取消合同、自动转移责任或处置其相关托管资产。
- CR-2A：持续服务样例让代表性玩家在接受前比较首个检查点价值、成本、持续义务、失败暴露、退出/修复和无合同替代路径；preview、offer 与接受待校验均不产生预留、债务、collateral、奖励、优先级或其他世界效果，只有经授权接受与权威校验后才进入 active。正常检查点以一个可审计 receipt 进入 fulfilled；部分履行、违约、终止或争议保留已确认结果并提供修复/重新履行、终止/结清、最小争议保全或显式继受路径。报价/授权漂移会原子拒绝或重新报价；retry、reconnect 与重复提交不会重复交付或扣款，续期必须重新报价、明确接受并生成新 receipt，控制权变化不会自动增加优先级/义务或转移责任；本样例不作为当前实现/readiness 声明。
- CR-3：争端样例以 receipt 为首要事实，使用随机无冲突本地 panel 和有限程序性申诉；争端期间未争议且可安全履行的义务不会自动停摆，直接争议范围只能获得有理由、可审计、尽量可逆的最小保全，并清楚区分已确认、待审、保全和可执行下一步；声量、财富或组织地位不能替代事实、扩大重审或把保全变成单方控制、没收或无限冻结。
- CR-4：发布/质疑社会事实及声明关系的样例在提交前预览可能受影响主体、合作/治理/claim 表面与争端/质押风险，并让玩家决定继续或暂缓；预览不授予权利、资格、权重或奖励，也不构成当前实现或 readiness 声明。声誉样例区分玩家、Agent、组织和角色/岗位的情境记录，支持到期、更新和有界申诉而无永久 blacklist；转让披露世界历史但不转移个人/政治 credential。
- CR-5：R&D 样例以工作前 charter 追溯归因与适用 royalty，永久保留 Agent provenance，并只通过显式合同转让 share；现有社交事实或产业真值不被误报为已实现本专题。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| CR-1 / CR-2 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 非绑定沟通、授权/接受、atomic/持续服务、collateral/evidence/remedy，以及控制权变化时冻结新风险、履行/终止/结清或明确同意继受、托管资产/责任/receipt 连续性的组合证据 | test_tier_full |
| CR-2A | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / qa_engineer | PRD-GAME-012 / PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-indirect-control-agency-contract.prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | preview/offer/接受待校验无世界效果、active 后检查点与 receipt、partial/default/terminate/dispute 恢复选择、报价与授权漂移、幂等 retry/reconnect，以及显式继受不重复效果或自动转移责任的组合证据 | test_tier_full |
| CR-3 | producer_system_designer / gameplay_designer / runtime_engineer / blockchain_ops_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | receipt-first、随机无冲突本地 panel、程序性申诉、未争议履行、最小可逆保全、保全范围/原因/下一步和结清/恢复/补救/解除的范围限制证据 | test_tier_full |
| CR-4 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-WORLD_SIMULATOR-008 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/game/gameplay/gameplay-top-level-design.prd.md`; `doc/world-runtime/prd.md`; `doc/world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md`; `doc/world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md`; `doc/testing/prd.md` | 提交前预览受影响主体、合作/治理/claim 表面与争端/质押风险；预览不授予资格/权重/奖励，亦不构成当前实现或 readiness 声明；分主体/情境声誉、到期/更新/有界申诉、无永久 blacklist 和转让 credential 负例 | test_tier_required |
| CR-5 | producer_system_designer / gameplay_designer / agent_engineer / runtime_engineer / blockchain_ops_engineer / qa_engineer | PRD-GAME-014 / PRD-WORLD_RUNTIME-001 / PRD-P2P-003 / PRD-TESTING-003 | `doc/game/prd.md`; `doc/world-runtime/prd.md`; `doc/p2p/prd.md`; `doc/testing/prd.md` | R&D charter、贡献归因/royalty、永久 Agent provenance、显式 share 转让与现状 claim 分离证据 | test_tier_full |

## 6. Non-Goals

- 不实现聊天、Agent 渲染、合同、抵押、结算、仲裁、声誉、R&D、版权/专利、份额转让或玩家 surface。
- 不定义金额、比例、期限、证据 schema、抽样算法、仲裁人员、声誉分值、法律管辖或强制法律结论。
- 不把本分册中的持续服务结果标签冻结为 runtime enum、状态 schema 或 UI 字段，也不据此声称 preview、检查点、恢复或玩家 surface 已实现。
- 不把人类聊天、草稿、Agent 建议、社交事实、产业活动或组织身份自动升级为绑定合同、全球信用或 R&D 权利。
- 不把本专题写成当前 provider、社交、合同、声誉、R&D、preview 或发行 readiness。

## 全量语义追踪

| REQ / AC | 专业 owner | 专业权威 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- |
| [REQ-WR-CR-001](#req-wr-cr-001) / [AC-WR-CR-001](#ac-wr-cr-001) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-CR-002](#req-wr-cr-002) / [AC-WR-CR-002](#ac-wr-cr-002) | `producer_system_designer` | [`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |
| [REQ-WR-CR-003](#req-wr-cr-003) / [AC-WR-CR-003](#ac-wr-cr-003) | `producer_system_designer` | [`gameplay-top-level-design.prd.md`](../../game/gameplay/gameplay-top-level-design.prd.md)、[`social-fact-ledger-declarative-reputation.prd.md`](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md)、[`social-fact-ledger-declarative-reputation.design.md`](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md)、[`doc/game/prd.md`](../../game/prd.md#3-player-facing-authority-boundary)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/p2p/prd.md`](../../p2p/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md) | 提交前 preview 的受影响主体/合作/治理/claim/争端/质押风险与无 entitlement/readiness 承诺；本专题对应要求、验收与专业 authority 的可导航追踪证据 | `test_tier_required` |

## 未决问题与证据边界

绑定合同、持续服务恢复、情境声誉和 R&D 归因各自的专业合同及验证范围须逐项核对；本文不裁定经济参数、仲裁权限或归因算法，不把预览/提案/历史记录视为已生效结果。

核对责任由本文 Owner role 联合所列专业 owner 承担；解除触发是相应功能进入实现或对外 claim 评审前，按本条 REQ/AC 核定专业承接、固定候选与实际证据。未核清前保留上列缺口，不用结构检查推导完成。
