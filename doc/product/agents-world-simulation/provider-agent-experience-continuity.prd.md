# Agent/provider 体验连续性

## 文档身份

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[prd.md](prd.md)
- 配对产品 design：[provider-agent-experience-continuity.design.md](provider-agent-experience-continuity.design.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[Local Provider 与内置 Agent 体验等价（parity）验收方案](../../world-simulator/llm/provider-agent-experience-parity.prd.md)
- 配套专业系统设计：[Provider Switch Intent Continuity](../../world-simulator/llm/provider-switch-intent-continuity.design.md#des-psw-001)（DES-PSW-001..004：provider/lane 切换、候选与在途意图连续性及重连不重放）
- 统一客户端系统设计：[Agent Game Client 边界](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)
- Last reviewed：2026-10-15

本文是长期产品分册，定义 Agent/provider 切换时的体验连续性，以及官方内置与外部执行方遵循同一游戏能力合同的产品承诺。它不声明任何 provider 当前受支持、可用、默认或已就绪；具体 provider 组合的场景范围、评估、技术合同和结论仅由专业域权威文档维护。

## 设计适用性与生命周期闭合

- 设计判定：`paired-design`。
- 设计适用性理由：执行方选择、委托、独立运行、Stop、未知结果恢复涉及非平凡体验状态；[配对产品设计](provider-agent-experience-continuity.design.md)承接这些信息与选择，不复制协议实现。
- 历史设计判定 Issue：#3680；原 `PRD-only-sufficient` 仅对应当时的窄 provider 连续性主题，不再作为扩展后主题的设计豁免。
- 历史来源保留：[C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)、[accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)；不恢复历史任务义务。

## 1. 产品目标

玩家已经认领一个 Agent，希望先用游戏提供的执行方建立工厂，之后交给自己熟悉的外部 Runtime 继续经营。玩家选择的是谁来理解目标和提出行动，不是在选择另一套游戏规则；切换不能复制 Agent、丢失已完成成果或把未知请求再执行一次。

当玩家在专业域已明确场景范围的 provider 组合间切换时，切换不能带来无法解释的、实质性的体验退化。连续性以玩家可感知的目标结果、等待体验、多轮记忆与意图延续、问题可诊断性及恢复路径为准，而不要求内部实现相同。

体验连续性不改变世界规则或执行权威。provider 只影响 Agent 如何形成意图；世界结果仍须经过同一权威裁决，不能因切换而把未发生、未接受或被拒绝的结果呈现为成功。

`player_parity` 与 `headless_agent` 是 Agent 的执行 lane，不是新的玩家访问模式。无论 Agent 经由哪一 lane 提出意图，玩家所理解、纠正或继续行动的只能是同一条权威世界结果链；lane 差异不得制造另一份世界事实、绕开裁决，或把 Agent 执行能力误述为玩家获得新的进入权限。两条 lane 的具体合同、适用范围与 `experimental` / 非默认边界由[专业域 parity 权威](../../world-simulator/llm/provider-agent-experience-parity.prd.md)维护，产品层不将其提升为当前支持、默认或发布就绪结论。

<a id="first-party-agent-client"></a>
### 1.1 官方内置与外部执行方的关系

**官方内置 Harness 是第一方游戏客户端，不是世界规则或世界写入权威。** Harness 指组织模型调用、规划、工具和私有记忆的执行软件；外部 Runtime 可以保留自己的实现。两者通过同一游戏能力合同提出意图，世界 Runtime 独立决定是否允许、如何执行以及实际后果。

“同一合同”承诺相同主体、授权、任务版本、世界切点及观察范围下的一致游戏语义，不承诺两个模型选择相同行动、具有相同费用/速度或私有记忆。官方身份不产生额外权限；不同角色的合法差异来自明确授权，而不是内置捷径。既有 `player_parity` 与 `headless_agent` 的观察差异仍保留，不能以统一接口扩大玩家可见信息。

首个迁移样例可以限定在已有 prepared-world 工业链，但全量替换前须核对全部保留的内置能力。未覆盖能力明确保持受限或旧组合，不静默降级；公开支持范围仍由 [REQ-AGENT-PARITY-001](#req-agent-parity-001)约束。

本条不取代[外部 Runtime 首期 Skill/Game API 主入口](external-agent-runtime-play.prd.md#external-runtime-canonical-interface)，也不把直接模型服务或旧 Provider callback 重新命名成真实外部 Runtime。第一方客户端是额外执行方实现，不新增玩家访问模式，不默认成为玩家入口，也不承诺免费推理或云端托管。

<a id="2-玩家边界"></a>
## 2. 范围与玩家边界

- 玩家可以期待在已声明场景范围内继续理解当前目标、Agent 意图、世界结果与下一步，而不需要因 provider 切换重新猜测世界事实。
- 当组合不在适用范围内，或体验处于受限、退化或阻塞状态时，产品必须如实表达该状态，保留权威世界真相，并给出适用于当前状态的下一步；不得把回退、静默替代或局部技术通路包装成等价体验。
- 连续性承诺不等于所有 provider、所有场景或所有体验维度天然等价，也不以一次成功、单一入口或历史证据代签当前产品结论。

### 2.1 切换窗口与在途意图

- 切换只影响切换后由 Agent 形成的新意图；它不是撤销、替换、重放或重新授权切换前候选、已送出或已接受意图的通用入口。玩家可以切换 provider，但不能因此要求旧候选自动交给新 provider 再次执行，或把新 provider 的建议当作旧请求已获接受。
- 切换发生时，玩家必须仍能区分：尚未被权威系统接受的候选、由适用权威状态确认仍无世界效果的待决请求、已送出但提交结果未知的请求、已有 committed receipt 的结果，以及已拒绝、失效或需要重新规划的请求。切换、重连或体验降级本身不把其中任何状态变成成功、取消、失败或新的世界效果。
- 缺少回执、切换或断连不证明请求尚未生效。提交结果未知时保留原请求身份，恢复当前读取资格后先核对原结果；查询受阻不能被解释成确定失败、已取消或可重新执行。沿用[入口组合验收](../player-entry-distribution/prd.md#player-session-unknown-outcome-acceptance)，不新增查询协议或恢复旧授权。
- 已接受或待决的意图只按同一权威世界链和其专业合同继续、拒绝、过期或结算；后续切换不能追溯改写其来源、成本、权限、排序或世界后果。若专业域支持安全的撤回或替换，玩家只能使用该明确能力；否则产品表面只可提供查看、等待或重新规划，不能把普通重试伪装成取消。
- 当切换前后的建议指向相同目标时，产品表面仍须避免暗示两者已自动去重、合并或同时获授权。任何额外提交都必须重新经过当时有效的授权与权威裁决；重复、竞争或重放的实际处理由 Agent、runtime 和测试专业合同定义，产品层不冻结其实现或 receipt schema。

## 2.2 叶级产品要求与验收

<a id="req-agent-parity-001"></a>
### REQ-AGENT-PARITY-001：体验连续性必须限定在已声明场景

Trace ID: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6

- 功能点：[REQ-AGENT-PARITY-001](#req-agent-parity-001)；适用范围沿本条原条件与 [AC-AGENT-PARITY-001](#ac-agent-parity-001)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[声明场景与证据身份](../../world-simulator/llm/provider-agent-experience-parity.design.md#parity-scope-evidence)及[受限结论与扩面边界](../../world-simulator/llm/provider-agent-experience-parity.design.md#parity-rollout-boundary)承接组合身份、证据不足不扩面和明确下一步。剩余：各入口玩家可读受限/退化呈现及真实组合全范围验证，不能由局部parity设计代签。
- 实现状态：未核实
- 实现与验证依据：[实际bench证据身份](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_provider_tools/src/bin/oasis7_provider_parity_bench.rs#L50-L53)固定simulator_world_kernel/not_certified；[报告输入校验](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/scripts/provider-parity-p0.sh#L269-L288)拒绝smoke冒充认证，[聚合与输出](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/scripts/provider-parity-p0.sh#L884-L978)区分benchmark/parity/release并输出blocked。这是已实现专业报告防过度claim能力，未核到报告成为产品支持声明发布源。[正式模型配置](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_model_client/config.rs#L10-L22)明确非public support attestation，[玩家表面](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx#L146-L180)消费部署preset/技术状态；尚未建立声明provider组合、场景、权限、证据窗口→玩家受限表达的消费者。本轮未运行模型或验收。
- 核对日期与基线：2026-10-17；需求与静态实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用源码逐文件Git blob与当前完整tree核验；#4865未合并，不使用其候选类型。本轮只读源码/本地产物，未执行模型、Runtime、产品验收或部署；测试源码和历史QA报告不改称本候选PASS。
- 剩余范围：待定位专业authority当前支持声明的正式发布源/版本，以及Launcher/Viewer/pure API读取声明组合与证据范围的连接；核清超范围/证据不足如何显示limited/degraded/blocked和真实下一步。报告自身blocked与实验模型UI真实存在，但不替代切换连续性的产品承诺边界；不把此报告称为不存在。

- 要求：provider 切换的连续性只能在专业 authority 已声明的 provider 组合、玩家场景、权限和证据范围内作出产品承诺；技术接通、一次成功或历史样本不能扩大当前支持、默认或发布结论。
- 验收：AC-AGENT-PARITY-001

<a id="ac-agent-parity-001"></a>
### AC-AGENT-PARITY-001：未覆盖组合保持窄承诺

Trace refs: O7-REQ-21f1c7b1-8be0-438e-8f92-d4d1b34be0e6；承接[REQ-AGENT-PARITY-001](#req-agent-parity-001)。

- 覆盖要求：REQ-AGENT-PARITY-001
- 场景与结果：当组合超出适用范围或证据不足时，玩家看到受限、退化或 blocked 语义及真实下一步；产品不会把回退、静默替代或局部技术通路呈现为等价体验或当前可用 provider。
- 证据边界：场景范围、评估和具体 parity 结论由 provider 专业 authority 提供；产品层不定义 provider 矩阵、评分或阈值。

<a id="req-agent-parity-002"></a>
### REQ-AGENT-PARITY-002：provider 切换不得重放在途意图

Trace ID: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5

- 功能点：[REQ-AGENT-PARITY-002](#req-agent-parity-002)；适用范围沿本条原条件与 [AC-AGENT-PARITY-002](#ac-agent-parity-002)，不扩大入口或阶段。
- 系统设计状态：部分覆盖
- 系统设计依据：[专业承接条款](../../world-simulator/llm/provider-switch-intent-continuity.design.md#des-psw-003)：原请求身份、持久 handoff、迟到结果隔离与不重放；玩家可读状态与各真实 provider/入口的全范围承接须另行核对。
- 实现状态：部分实现
- 实现与验证依据：[先扫描原操作且pending/unknown阻塞](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_agent_harness/src/native_task.rs#L231-L277)与[共享SDK恢复#4768](https://github.com/nil-studio/oasis7/pull/4768#issuecomment-6202399190)确认重连/不确定投递原件恢复切片；#4768 reviewed source bb2149f230d2e5a68b61376cc5ac44e708ef1d39、合入2f6ba83a246cf871d8cb06c378d406aaa5291b73的实际handler/Node及CI只沿记录范围。[PSW验证计划](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/doc/world-simulator/llm/provider-switch-intent-continuity.design.md#L171-L196)仍将PSW-01..09区分为计划；原件恢复不代签跨provider cutover、durable pending handoff或真实provider配对。
- 核对日期与基线：2026-10-17；需求、设计与静态实现固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；历史执行候选、入口和环境分别按上列记录限定。本轮只读核查，未运行测试、模型、浏览器或部署。
- 剩余范围：已交付重连/不确定投递时原操作身份保持与unknown阻塞的有界native/SDK切片；provider/lane切换绑定、持久pending handoff、迟到结果隔离、玩家状态投影及PSW-01..09真实配对证据仍未核清。

- 要求：provider 切换、重连或体验降级必须保留候选、待决、已结算和已拒绝请求的真实区别，不得自动重放旧请求、伪造取消或产生第二次世界效果。
- 验收：AC-AGENT-PARITY-002

<a id="ac-agent-parity-002"></a>
### AC-AGENT-PARITY-002：切换只影响后续新意图

Trace refs: O7-REQ-70188c23-ddbf-49ca-8a70-3fa05c152ba5；承接[REQ-AGENT-PARITY-002](#req-agent-parity-002)。

- 覆盖要求：REQ-AGENT-PARITY-002
- 场景与结果：切换前候选不自动交给新 provider 重试；权威确认尚未生效的待决请求按原权威合同继续、拒绝、过期、取消或明确重新规划；结果未知时先按原请求核对，查询受阻保持未知，不以切换重放；只有 committed receipt 表达世界后果，受支持的撤回/替换必须走专业路径。
- 证据边界：意图 lineage、去重、receipt 和撤回/替换状态机由 Agent/runtime/QA authority 验证；产品层不冻结 schema。

<a id="req-agent-parity-003"></a>
### REQ-AGENT-PARITY-003：内外执行方共用游戏能力与结果合同

Trace ID: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01

- 要求：在已声明能力范围内，官方内置和外部执行方使用相同主体、授权、任务版本、世界切点与观察范围提出同一意图时，必须遵循同一能力发现、准入、资源后果、权威结果和恢复语义；内置执行方不得凭实现位置读取额外世界状态或绕过游戏规则。
- 理由：玩家选择执行策略，而不是选择权限漏洞或另一份世界事实。外部 Runtime 无须使用官方 Harness，世界能力也不以运行旧 Provider Bridge 为前置。
- 验收：[AC-AGENT-PARITY-003](#ac-agent-parity-003)；与既有 [REQ-AGENT-PARITY-002](#req-agent-parity-002)组合核对未知结果和切换。
- 功能点：REQ-AGENT-PARITY-003；范围为 §1.1 的已声明能力与可比观察范围，不承诺模型行为等价。
- 系统设计状态：已覆盖
- 系统设计依据：[职责与依赖](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)、[共享客户端合同](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-002)、[执行与恢复](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-003)、[迁移覆盖](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-006)；均为目标设计，不是实现证据。
- 实现状态：部分实现
- 实现与验证依据：[共享 Harness](../../../crates/oasis7_agent_harness/src/lib.rs)、[普通签名客户端](../../../crates/oasis7_game_client/src/device_client.rs)及[实际 Skill 范围](../../../skills/oasis7-play/SKILL.md)提供基础；内置仍有 [simulator 适配](../../../crates/oasis7/src/simulator/agent_harness_adapter.rs)。后续实现和实际验证由 [#4726](https://github.com/eng-cc/oasis7/issues/4726)承接，不以创建任务证明交付。
- 核对日期与基线：2026-10-15；源码/原合同 `d6f6ff3d10f0f2990bf17a111d55c7b36ebf0a5d`；仅远程源码和文档审读，本次文档变更未执行行为验收。
- 剩余范围：设计条款已承接；实现仍需共享 SDK 独立构建、内置路径迁移、能力映射及普通客户端同候选验证。

<a id="ac-agent-parity-003"></a>
### AC-AGENT-PARITY-003：没有内部捷径的同合同执行

Trace refs: O7-REQ-ad3ea59c-596c-4c50-8da8-fe47608c6f01；承接[REQ-AGENT-PARITY-003](#req-agent-parity-003)。

- 覆盖要求：REQ-AGENT-PARITY-003。
- 场景与结果：在相同初始世界和有效委托下，固定合法与非法意图分别经官方客户端与独立外部入口执行，授权可见性、接受/拒绝及世界后果一致；关闭已迁移场景的旧 Provider 路径后仍可完成目标与原结果恢复。玩家看到真实成果，而非“模型已完成”的替代说明。
- 反例：内置绕过库存/权限、从私有状态补观察、能力缺口静默回退，以及排产受理被误报为首产物，都判为不满足。
- 证据边界：协议一致性不证明策略、速度、成本、图形体验或两个真实外部 Runtime 等价；完整首局与支持声明仍分别取证。

<a id="req-agent-parity-004"></a>
### REQ-AGENT-PARITY-004：执行方选择与运行边界必须清楚

Trace ID: O7-REQ-d452396d-291d-41a5-a912-6d615b58c29e

- 要求：当提供官方内置或外部执行方时，玩家必须能确认实际执行方、运行位置、模型费用/预算来源、所控 Agent 和有效委托；缺失连接、执行许可或预算时明确阻塞，不得静默改用另一模型、Runtime 或游戏身份。
- 运行边界：选择官方客户端不等于购买托管服务。关闭观战界面不应终止已独立运行且仍获授权的执行方；执行方停止后不得声称仍在推理。已经被世界接受的生产/转运仍按世界规则推进，Stop 不追溯取消其后果。
- 理由：源码拆分不能把安装和排障负担转嫁给玩家，也不能隐藏开销、停止或授权的真实含义。选择、停止与换执行方的在途规则继续由 [REQ-AGENT-PARITY-002](#req-agent-parity-002)拥有。
- 验收：[AC-AGENT-PARITY-004](#ac-agent-parity-004)。
- 功能点：REQ-AGENT-PARITY-004；仅适用实际声明的执行方/平台/访问组合，不新增默认值或运行位置承诺。
- 系统设计状态：已覆盖
- 系统设计依据：[独立控制与世界推进](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-003)、[执行方生命周期和费用隔离](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-005)、[受控迁移](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-006)。
- 实现状态：部分实现
- 实现与验证依据：[可信Launcher执行器选择、预算和状态](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher/hosted_model_client/execution.rs#L48-L126)及[执行位置、计费/预算、停止与效果呈现](https://github.com/nil-studio/oasis7/blob/3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9/crates/oasis7_viewer/software_safe_src/model_connection_panel.jsx#L147-L180)确认第一方native受限入口；[已合入#4744](https://github.com/nil-studio/oasis7/pull/4744#issuecomment-6209860321)报告对应Launcher/UI、确定性真实节点与预算释放验证，tested source d5c111b560d1a8a11dd6169121ab615c3d229028。组件fixture不证明真实登录/供应商/完整游戏；本轮仅静态重读。
- 核对日期与基线：2026-10-17；需求、设计与静态实现固定基线 `3c0a31a6e9d1e7b8633e3bcfcb11b89ebfcddbe9`；历史执行候选、入口和环境分别按上列记录限定。本轮只读核查，未运行测试、模型、浏览器或部署。
- 剩余范围：已交付受限第一方native Launcher选择、实际执行状态、预算/Stop表达；其他声明执行方/平台组合、全部保留能力迁移、真实供应商与完整玩家surface验收仍未核清。关闭页面可能停止续期的明确边界不等于持续托管承诺。

<a id="ac-agent-parity-004"></a>
### AC-AGENT-PARITY-004：清楚选择、停止与重新继续

Trace refs: O7-REQ-d452396d-291d-41a5-a912-6d615b58c29e；承接[REQ-AGENT-PARITY-004](#req-agent-parity-004)。

- 覆盖要求：REQ-AGENT-PARITY-004；未知请求与切换另组合 AC-AGENT-PARITY-002。
- 场景与结果：玩家为同一合法 Agent 选择实际支持的执行方，确认成本与权限后开始；离开观战界面再回来能区分执行方活动和世界已受理作业。断连、预算耗尽、执行器退出或 Stop 后，显示真实阻塞及安全下一步，不伪装运行或换用另一执行方。
- 反例：源码拆分后要求玩家自行发现多个必要进程、只按 Viewer 连接显示 Agent 在线、Stop 被表达为已撤销所有产物/在途请求，均不满足。
- 证据边界：每个声明平台与玩家入口单独验证；本地独立 worker 不证明云端托管，进程健康不证明世界任务完成。

## 2.3 叶级 owner、authority、evidence 与 test tier 追踪

本表把 provider 连续性的每个新叶级关系导航到专业 owner、权威文档和未来验证证据；它不声明任何 provider 当前受支持、默认或已就绪。

| REQ / AC 关系 | 专业 owner 与真实边界 | 专业域 PRD-ID | 专业权威（可导航） | 验证证据（应提供，不预示当前结果） | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-PARITY-001](#req-agent-parity-001) / [AC-AGENT-PARITY-001](#ac-agent-parity-001) | `producer_system_designer`：场景范围、窄承诺和 readiness 边界；`agent_engineer`：provider/Agent 行为与失败反馈；`viewer_engineer`：受限、退化和下一步的真实表达；`qa_engineer`：适用组合的 parity 证据；专业角色各自不把技术接通或局部样本提升为产品准入。 | `PRD-WORLD_SIMULATOR-038` / `PRD-WORLD_SIMULATOR-016` / `PRD-TESTING-003` | [`provider parity authority`](../../world-simulator/llm/provider-agent-experience-parity.prd.md#1-executive-summary); [`world-simulator PRD`](../../world-simulator/prd.md); [`testing PRD`](../../testing/prd.md) | 未来 full-tier 的已声明 provider 组合/场景样例，以及超出范围或证据不足的负例；对比目标、等待、诊断和恢复语义，并确认 blocked/limited 表达不被包装成等价体验或当前支持。 | `test_tier_full` |
| [REQ-AGENT-PARITY-002](#req-agent-parity-002) / [AC-AGENT-PARITY-002](#ac-agent-parity-002) | `producer_system_designer`：切换窗口的产品边界；`agent_engineer`：意图 lineage、provider failure 和重试语义；`runtime_engineer`：接受/结算、去重和单一世界效果；`viewer_engineer`：候选、待决、已结算和拒绝状态可读性；`qa_engineer`：切换/重连/退化组合对账。 | `PRD-WORLD_SIMULATOR-038` / `PRD-WORLD_RUNTIME-001/031/033` / `PRD-TESTING-003` | [`provider parity authority`](../../world-simulator/llm/provider-agent-experience-parity.prd.md#1-executive-summary); [`provider switch system design DES-PSW-001`](../../world-simulator/llm/provider-switch-intent-continuity.design.md#des-psw-001); [`DES-PSW-002`](../../world-simulator/llm/provider-switch-intent-continuity.design.md#des-psw-002); [`DES-PSW-003`](../../world-simulator/llm/provider-switch-intent-continuity.design.md#des-psw-003); [`DES-PSW-004`](../../world-simulator/llm/provider-switch-intent-continuity.design.md#des-psw-004); [`world-simulator PRD`](../../world-simulator/prd.md); [`world-runtime PRD`](../../world-runtime/prd.md); [`testing PRD`](../../testing/prd.md) | 未来 full-tier 切换窗口覆盖候选不重放、待决请求真实结算/拒绝/过期、重连与退化不产生第二次提交，以及撤回/替换仅走专业支持路径；只有 committed receipt 表达世界后果。 | `test_tier_full` |
| [REQ-AGENT-PARITY-003](#req-agent-parity-003) / [AC-AGENT-PARITY-003](#ac-agent-parity-003) | `agent_engineer`：客户端/执行方；`runtime_engineer`：唯一准入/效果；`qa_engineer`：同合同与反例 | `PRD-WORLD_SIMULATOR-016` / `PRD-WORLD_RUNTIME-001` / `PRD-TESTING-003` | [职责与依赖](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-001)、[共享客户端](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-002) | AGC-01..05、07..09 的实际同候选证据；真实模型另按 AGC-11 | `test_tier_full` |
| [REQ-AGENT-PARITY-004](#req-agent-parity-004) / [AC-AGENT-PARITY-004](#ac-agent-parity-004) | `agent_engineer`：独立执行；`viewer_engineer`：状态/选择；`qa_engineer`：平台与玩家 surface | `PRD-WORLD_SIMULATOR-016` / `PRD-TESTING-003` | [运行约束](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-005)、[迁移范围](../../world-simulator/llm/agent-game-client-boundary.design.md#des-agc-006) | AGC-06、09..11：实际启动、断连、停止与预算故障；不代签托管 | `test_tier_full` |

## 3. 产品验收

- PC-1：在专业域明确的场景范围内，provider 切换后的玩家可感知目标结果、等待体验、多轮记忆与意图延续、可诊断性和恢复路径不存在无法解释的实质性退化。
- PC-2：受限、退化、阻塞或不适用的情形不会伪造成功或改写权威世界状态；玩家能够理解当前限制及适用的下一步。
- PC-3：产品验收只引用专业域对具体组合和场景的证据，不把历史评估、局部样本或技术接通提升为当前支持、默认或发布就绪结论。
- PC-4：代表性切换窗口样例证明：切换前的候选不会自动重放；已送出但未结算的请求保持真实待决、拒绝、失效或重新规划语义；只有 committed receipt 可以把结果表达为世界后果。切换、重连和退化均不能制造第二次提交、隐藏原请求或伪造取消/成功。
- PC-5：若专业域提供撤回或替换，样例证明玩家只能通过该受支持路径影响在途请求，且其结果仍经权威裁决；若不提供，玩家只能查看、等待或重新规划。两个 provider 对相同目标形成的建议不会被产品表面承诺为自动合并、去重或共同授权。

- PC-6：AC-AGENT-PARITY-003 与 AC-AGENT-PARITY-004 分别证明统一游戏合同和可理解的执行方生命周期；不以其中一项通过代签另一项，也不扩大 PC-1..5 的既有支持范围。

### 3.1 验收权威与证据边界

| 产品承诺 | 专业 owner | 专业域权威 | 证据边界 | 测试层级 |
| --- | --- | --- | --- | --- |
| 场景内的体验连续性 | agent_engineer / viewer_engineer / qa_engineer | `doc/world-simulator/llm/provider-agent-experience-parity.prd.md` | 专业域维护适用场景、评估方法与具体结论；产品层只消费其可追溯结论 | test_tier_required |
| 受限或退化时的真实表达与下一步 | agent_engineer / viewer_engineer / qa_engineer | `doc/world-simulator/prd.md`; `doc/testing/prd.md` | 以权威世界真相、诊断和恢复证据验证；不以技术连通或截图代签 | test_tier_required |
| 切换窗口中的候选、在途请求与已结算结果边界 | agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 覆盖切换、重连和退化下的候选不重放、在途状态可读、明确撤回/替换或安全等待，以及 receipt 支持的单一世界结果；不以模拟 provider 输出或单张界面证明代签 | test_tier_full |

## 4. 非目标与权威边界

- 不定义 provider profile、支持矩阵、评分、阈值、等待等级、基准协议、命令、实现接线或 rollout verdict。
- 不决定 provider 的当前支持状态、默认选择、可用性、发布准入或扩面顺序。
- 不定义意图 lineage、去重/排序、撤回/替换状态机、pending 持久化、重试、过期、receipt schema 或任何 provider 内部追踪字段。
- 不复制或替代专业域 source 的技术和验证合同；需要具体组合判断时，回到上述专业域权威文档并由对应 owner 裁决。

- 不强制外部 Runtime 使用官方 Harness、官方 SDK 或某一传输包装；公共协议可被独立客户端实现。
- 不承诺两种执行方作出相同策略、共享私有记忆、拥有相同成本/延迟，也不因官方实现获得额外能力或公开支持。
## 5. 取舍、风险与未决问题

- 设计假设与反证：在现有启动流程中交付第一方客户端应能降低维护复杂度，而不增加玩家操作步骤；若受支持平台只能靠手动补进程才能开始/恢复，需由 `producer_system_designer` 与 launcher owner 修正方案，不能降低本条体验承诺。
