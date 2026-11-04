# Agent 中介合同形成与即时原子交换系统设计

- 状态：active；本文冻结有界目标合同，不表示实现、激活或验收完成。
- Owner：`runtime_engineer`；交易规则与费用政策归 gameplay/governance authority，签署与展示消费方为 Agent、World Service、Viewer 和纯 API，验证方法归 testing。
- 固定审读基线：`nil-studio/oasis7@5e62da69ddb511f9f10a524accdf0071ecd6b91c`。§3 的实现事实只对应此提交；目标接口名称均为逻辑语义，不是已存在的 HTTP、`Action` 或 Rust 类型。
- 上游：[绑定合同要求](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001)、[预览与待校验接受验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001)。验收方法：[合同形成手册](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-01-nonbinding)。

## 1. 问题、目标与非目标

<a id="contract-formation-scope"></a>
### 合同形成的有界目标

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

聊天、Agent 建议、单方提议或一个动作 ACK 都不足以证明双方形成了合同。本文使一个明确范围内的结果可以判定：两名不同的个人当前 owner，分别控制两个不同 Agent，对同一份可执行条款明确接受；权威在一个提交点完成两腿资源交换、全部适用费用和 receipt，或确认没有该次业务效果。断连后只能恢复原件，不能因重复接受、重发或恢复再交换一次。

目标 profile 的逻辑名为 `owner_atomic_resource_exchange_v1`：同一 canonical world/branch、同位置的两个 Agent，各自向另一方转移一条正数 `Electricity` 或 `Data` 余额腿。两腿可为同种资源；每方必须用提交前已有余额覆盖自己的全部扣减，不能用此次收到的资源融资。它没有运输、保管、未来交付或待执行的服务义务；形成与履行在同一次 canonical staged commit 中完成，成功直接为 `fulfilled`。receipt 同时证明何时生效和如何履行，不制造一个等待后续 executor 的持久 `active` 状态。

本设计对 CR001 为部分承接：覆盖上述即时交换的明确条款、直接当前 owner 接受、权威形成及原件恢复。组织、多方、长期受托、资产继受、借贷、运输、持续服务、抵押、检查点、default、remedy、争议裁决、奖励和新税制不在此 profile 内。未支持的义务必须停在非绑定草稿或返回 `unsupported_profile`，不能先扣款再等后继实现。

**零业务效果不变量：** quote、`offered`、单方或双方 `accepted_pending_validation`、未提交的撤回/失效与拒绝，不预留或扣减本金、交易费、执行费或任何其他参与方资源，不消费游戏额度、资格、顺位、锁、债务、声誉、奖励、权限或继受机会。只允许有界协议元数据和无效果决定；CPU/通信防滥用限额不兑换游戏权益，不接入旧 `max_open_contracts_per_agent`。若当前通用管线不能无上述效果地登记提议/接受，该写路径不可启用；不能改向 module owner 收费或利用缺少 owner 的免费分支绕过。

## 2. 上游约束与相关角色

产品拥有何时绑定与玩家可见承诺；runtime 拥有校验、事务、当前权威、恢复和投影事实；gameplay/governance 拥有收费的适用范围、合法 payer、费率与接收账；WASM/模块只能在已激活的 instance 内产生受限候选，不能自行定义合法 owner、费用或最终性。Agent 只整理 typed 条款、解释与建议；签名仍须来自本 profile 的两名直接当前 owner。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | 草稿、提议、单方或双方接受只登记有界协议事实，不消费本金、费用、游戏额度、资格或顺位 | [非绑定范围](agent-mediated-contract-formation.design.md#contract-formation-scope) | 产品合同规则；零业务费用的统一协议登记与原件查询 | 不证明当前 module 状态登记已免费；其他合同 profile |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | 两方可读同版完整 typed 条款、确定正文、全部费用与时效；CAS 记录变化不改变条款摘要 | [条款与确定渲染](agent-mediated-contract-formation.design.md#contract-formation-terms) | canonical codec/模板、调用者可读范围 | 任意自然语言义务的自动识别；其他合同 profile |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | 两名直接当前 owner 对同一 terms digest 明确接受；提交重验来源、generation、范围、位置及 Data 方向授权 | [当前授权](agent-mediated-contract-formation.design.md#contract-formation-authorization) | 当前 owner/identity resolver；[授权变化边界](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-011) | 组织签署、长期 delegation、新 owner 自动继受 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | 两腿本金及全部费用有明示来源/payer/上限，按提交前余额及逐资源守恒校验 | [原子交换 profile](agent-mediated-contract-formation.design.md#contract-formation-atomic-profile) | 资源、定向 Data grant、适用交易费政策及 Kernel 计量 authority | 服务交付、抵押、奖励、公共 levy 或新税制 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | 只在两腿、全部费用、业务终态与权威 receipt 同次提交后报告形成与履行；并发提交/撤回不部分生效 | [权威提交点](agent-mediated-contract-formation.design.md#contract-formation-commit) | [统一事务](../design.md#62-unified-executiontransaction-architecture-p0)、[合法权威提交](../design.md#runtime-authority-commit-recovery-target) | 服务 active executor、部分履行、外部交付；不代签整个 CR001 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | 重连按原请求/业务键查询；未知不等于失败，重发不再次扣减，持久未知先 fence 原 writer | [原件恢复](agent-mediated-contract-formation.design.md#contract-formation-recovery) | [generation 提交判定](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)、[恢复服务闸门](../design.md#runtime-recovery-design) | 持续服务 checkpoint/remedy 恢复；不提升 CR002 状态 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | Viewer、Agent 与 API 同源区分提议、接受、未知和履行；受限摘要不能代替完整签署正文 | [消费投影](agent-mediated-contract-formation.design.md#contract-formation-read) | World Service 受众授权；[Viewer CR001 target](../../world-simulator/viewer/viewer-governance-targets-uiux.design.md#t-gov-04-cr-001) | 实际 UI 可写、浏览器/跨节点可用性与端到端完成 |
| Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376；[AC-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-003) | 仅此交换的理由/证据、全部成本、替代方向、纠正最早生效点与实际结果沿同一因果链可读 | [合同因果读面](agent-mediated-contract-formation.design.md#contract-formation-read) | [通用因果合同](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007)、Agent/Service 投影 | 全部社交、治理、冲突和记忆驱动路径；真实玩家理解度 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | 不把有后果的旧 Open 当零效果 offered；目标 profile 仅在合法支持范围内形成，未知服务不绑定 | [兼容与启用](agent-mediated-contract-formation.design.md#contract-formation-compatibility) | [Native 分类与冻结](../prd.md#native-surface-分类与冻结规则)、[Institution Migration Test](../design.md#61-institution-migration-test)、governance activation | 不选择 SC-32 pilot、不扩 core enum/WorldState，不改旧历史 |

相邻专业设计按职责复用：[间接控制](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-010)拥有通用授权范围和因果归因；[跨区交付](cross-region-delivery-and-escrow-settlement.design.md#des-wr-cr-002)拥有交付/托管的分段去重与结算；[组织连续性](organization-continuity-and-disposition.design.md#des-wr-oc-001)拥有继受和处置顺序。它们不将本 profile 扩大为持续服务，也不由本文重写其政策。

## 3. 当前状态、目标状态与差距

以下为固定基线的静态源码事实，未运行 Rust、业务 fixture、游戏、UI、模型、崩溃或跨节点验证。

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| native 合同动作与状态 | 公开 Open/Accept/Settle；状态 Open/Accepted/Settled/Expired；没有本专题的 Cancel 动作 | 独立版本的非绑定形成协议与一次性双腿结算 | 不能给旧 enum 加语义或把旧 Open 改名 offered | [Action](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[状态](../../../crates/oasis7_world_state/src/gameplay_state.rs) |
| 旧 AtomicExchange | Settle 是 creator 向 counterparty 的一腿付款，带税、成功声誉奖励及限制；Open 到期已可扣 creator 声誉 | 两条明示资源腿同次提交；未提交不改变声誉，成功不新授奖励 | 旧协议不是双腿交换，也不满足 offered 零后果 | [校验](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_policy_contract.rs)、[到期](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs)、[发布](../../../crates/oasis7_world_state/src/world/economic_contract_publication.rs) |
| Service | Open/Settle 明确拒绝 `service contracts unavailable: collateral/evidence/remedy not implemented`；原子到期筛选排除 Service | 此 profile 仍明确拒绝 | 没有新增服务义务、证据或救济 | [校验](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_policy_contract.rs)、[发布](../../../crates/oasis7_world_state/src/world/economic_contract_publication.rs) |
| 权限和资源 | 已有 current owner/identity resolver、定向 Data permission；普通 EmitResourceTransfer 检查 GeoPos 厘米距离为 0；旧 Settle 无同项显式检查 | 在一次提交上绑定两方 owner/identity、双方腿、当前位置和各方向 grant | 存在基础设施不证明完整合同接受已接入 | [owner](../../../crates/oasis7_runtime/src/world/industrial_control.rs)、[资源转移](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[Data permission](../../../crates/oasis7_world_state/src/state.rs) |
| preview 与协议登记 | `preview_governed_module_command` 不调用 sandbox、不改 World；governed command 无 module state/emit/effect 输出时丢弃 staged clone。成功持久写 module state 或 emit 仍可计费 | 只读 preview 与无业务费用的持久提议/接受分别成立；最终提交才一起收费 | `Pure`、没有资源 transfer 或现有 preview 都不能证明提议登记免费；无费用登记路径尚未交付 | [命令入口](../../../crates/oasis7_runtime/src/world/cognition_command.rs)、[module 执行](../../../crates/oasis7_runtime/src/world/module_runtime.rs) |
| 执行费用 | payer 来自 `module_artifact_owners[wasm_hash]`；compute 扣 Data、电费扣 Electricity，并转入世界资源 treasury | 合法 payer 必须在两名签署当事 Agent 内，所有资源费用进入已接受总额 | 旧 native 税公式不授予新模块税权；缺 owner 的跳过计费不构成免费授权 | [计费入口](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)、[计算/入账](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)、[目标计量合同](../wasm/wasm-interface.md#sr2-metering-contract) |
| 消费面 | Simulator→Runtime 映射强制 AtomicExchange；B23/B24 target 仍 `missing_full_write` | 同源的提议、接受、提交和原件查询投影 | 旧 action ack、静态面板及 mapping 不证明新 profile 已可写 | [mapping](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/llm_sidecar_runtime_support.rs)、[B23](../../world-simulator/viewer/viewer-governance-targets-uiux.design.md#t-gov-04-cr-001)、[B24](../../world-simulator/viewer/viewer-governance-targets-uiux.design.md#t-gov-04-cr-002) |

## 4. 边界与结构

| 组件/authority | 拥有与产生 | 允许读取/写入 | 不得越过的边界 |
| --- | --- | --- | --- |
| Agent/客户端 | typed 草稿、非绑定解释、owner 的签署请求 | 读取受众允许的 quote/原件；转交签名 | 不自造 owner proof、当前余额、费用或 committed 结果 |
| World Service/admission | 调用者认证、协议操作限额、请求关联、可读性检查 | 复用统一入口验证签名、大小和来源；提交完整候选 | 不把 HTTP success/入队 ACK 当合同，不先扣费用 |
| 目标 institution instance | 提议修订、两方接受证明、合同履行键与结果引用 | 以 `(world, module_id, instance_id)` 隔离的 module-owned 状态进入同一 staged transaction | 不新增 native 合同路径、全局 module_id fallback 或第二份业务账本 |
| Runtime/Kernel | 当前身份/位置/权限、资源守恒、排序、事务与 receipt | 在同一 parent 上重验，聚合全部 delta 后一次发布 | 模块不能直接写 canonical state/journal；任何多余效果拒绝 |
| 合法 fee authority | 有效政策与 schedule、payer/source、规则版本、费用接收账 | 提供可校验的完整 quote 和确定性计算输入 | 报价者、LLM 或旧税公式不能自行指定收费权 |
| authority profile/持久化 | 提交决定、证明、完整 generation、请求结果和恢复闸门 | 按既有根事务、持久条件和恢复合同处理 | 局部内存 apply、fsync 或外部缓存不单独证明 committed |
| 投影/Viewer/API | 经授权的同源可读结果 | 展示 authority/root、条款、接受状态、原 receipt 和可用动作 | 不补写状态、不补签、不能用受限摘要代替完整签署正文 |

非绑定协议操作需要统一入口中的有界准备/验证与元数据提交能力，和最终 billable execution 分清。已有无 sandbox preview 可承担只读检查；持久登记仍需证明不调用会发布费用的成功 module 命令，不消费能力游戏预算，也不向 module owner 转嫁成本。实现只能在既有 instance、staged transition 与 generation 边界内补齐这项能力；不能靠另一个未受治理的执行器或客户端数据库获得权威。未满足时可保留本地草稿和只读能力说明，形成写入口禁用。

## 5. 关键运行流程

1. **整理与报价。** Agent 将意图整理为本 profile 的两腿 typed 字段。Runtime 从已验证 canonical view 返回完整条款候选、费用来源、有效期与 blocker；不执行 sandbox、不预留、不登记接受。服务/运输等不可结构化为本 profile 的义务停在草稿。
2. **提出或修订。** 发起方当前 owner 认证后，用原请求键创建提议或以 `expected_record_version` 修订同一提议。双方 Agent 和个人主体固定；修改可执行内容、费用、时效或绑定版本产生新 `terms_revision/terms_digest`，旧接受全部失效，旧修订保留 superseded 关系。此步仅登记非绑定事实。
3. **分别明确接受。** 双方先读取同一完整签署正文，再分别提交自己的接受证明。第一方和第二方接受都只产生 `acceptance_recorded_nonbinding`，不排队执行、不冻结余额。任一方缺少决定性字段、签名或当前权限时不能登记为已接受。
4. **显式提交。** 两份证明齐备后，任一当前当事 owner 提交同一提议、修订、digest 和两份证明引用。Runtime 按 §7 的单一序列化边界重验并暂存完整两腿、费用、终态与 receipt；成功一次形成并 `fulfilled`。本 profile 不设置背景自动提交或待决服务 executor。
5. **拒绝、修订、撤回。** 确认未提交的失败返回原请求的稳定无效果决定；余额不足等可按已确认结果显式重报同一合同，过期/版本/条款失效则重新报价与接受。任一当前当事 owner 可在成功提交前撤回非绑定提议；撤回与提交按 canonical 顺序竞争，先成功者决定结果。成功后的“撤回”只返回原履行 receipt，不退款、不生成补偿。
6. **响应丢失或重连。** 先查原请求与原合同。找到履行 receipt 即展示原结果；找到无效果决定则展示其原因及允许的下一步；找不到或无法证明时保留 unknown，可用原键和完全相同 payload 恢复，不能生成新合同身份来猜测重做。

## 6. 接口与数据合同

<a id="contract-formation-terms"></a>
### 6.1 不可变条款与确定渲染

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

以下是目标逻辑字段，不新增现有 native wire。编码沿用 [WASM 目标执行合同](../wasm/wasm-interface.md#sr2-execution-contract) 的明确 schema、canonical CBOR 与域隔离 Hash32：`H(domain, body) = SHA256(UTF8(domain) || 0x00 || CanonicalCBOR(body))`。本专题域分别为 `oasis7/contract-terms/v1`、`oasis7/contract-acceptance/v1`、`oasis7/contract-request/v1`；签名、传输重试次数、本地时间和所在区块 wrapper 不进入其自身 ID preimage。整数禁止 float，semantic arrays 保序，未知义务字段拒绝；不得以任意 JSON 字符串化结果替代 canonical bytes。

摘要按无环顺序生成：不可变 `TermsBodyV1` → `terms_digest` → 确定渲染的签署正文 → `render_digest` → 各方接受证明 → 请求摘要。`TermsBodyV1` 包含本表的可执行条件和模板版本/hash，但排除可变 `record_version`、自身 `terms_digest`、派生 `render_digest`、接受证明、签名及提交 wrapper。`record_version` 只属于提议记录和命令的 CAS 前置；登记第一方或第二方接受会增加记录版本，但不能改变同一修订的条款摘要。

签署正文由 `TermsBodyV1`、已算出的 `terms_digest` 与固定模板生成；其 UTF-8 bytes 用 `oasis7/contract-render/v1` 域单独计算 `render_digest`，不反写入条款摘要的输入。接受证明的 hash body 引用这两个摘要并排除自身派生 ID/签名；请求 hash body 引用完整命令和证明 ID，并排除自身摘要及传输 wrapper。未知 codec/模板版本拒绝；实现提供 canonical bytes、各阶段摘要和两次接受后条款摘要不变的测试向量，不能以递归求 hash 或每次 CAS 重签替代这些边界。

| 字段组 | 必须携带的事实 | 判定与修订规则 |
| --- | --- | --- |
| scope/instance | world、branch、chain/genesis 绑定；稳定 module/instance；profile/schema、artifact/manifest、activation 绑定 | 每项来自合法激活与当前 world；未知或不匹配不可提交；instance 不能仅凭 module_id 回退 |
| 合同与修订 | `agreement_id`、正数 `terms_revision`、前修订 digest、`record_version` | 业务键为 `(world/branch, module_id, instance_id, agreement_id)`，不含易变 artifact/epoch；同键最多一份履行 receipt；revision 不重置业务去重 |
| 双方 | 固定 A/B slot；各自 Agent、个人 owner identity、owner source/generation、Agent identity generation | 两 Agent 与两个人主体分别不同；位置/密钥显示不能替代 current resolver；主体转让后不沿用旧接受 |
| 两腿 | 固定顺序 A→B、B→A；各自 `resource_kind`、`net_amount`、余额来源 | kind 仅 Electricity/Data，`1..i64::MAX` 整数；每方恰一条；net 是对方实际收到的数量 |
| 位置与授权依赖 | canonical GeoPos 与位置来源；每条 Data 腿的 sender→recipient grant 来源 | 执行时按普通资源转移规则检查厘米距离为 0；位置变动使原位置绑定 stale；grant 按提交 parent 验证，不能靠报价缓存 |
| 费用 | 完整 fee quote、rule/schedule 版本与 hash、每项 payer/source/kind/amount/treasury、每方每资源 `gross_debit_cap` | 包含交易与适用执行费用；签署的是全部扣减上限和确定规则，不是只签本金；详见 §6.3 |
| 有效期 | canonical `not_before_tick`、`expires_at_tick`，报价/授权更短的失效边界 | 只有 `not_before <= now < min(all_expiry)` 可执行；等于到期拒绝，不用客户端时钟延长 |
| 签署呈现 | `render_template_version/hash`、完整 typed terms 的 digest、确定渲染内容 digest | 每位签署人看到相同权利义务、费用来源与上限；模板或内容改变须新修订并重新接受 |

签署正文只由上述 typed 条款和版本化模板确定生成，必须写出即时两腿、各方每资源净收/总扣上限、费款去向、先决条件、无未来义务、失效/撤回边界及提交后只能查原 receipt。自由编辑的正文不能作为权威合同输入；与模板不符即 `render_mismatch`。聊天、目的、Agent 理由和谈判历史单独标为非绑定注释，不进入可执行义务。Runtime 不承诺识别任意自然语言中的全部暗含承诺；可判定的拒绝包括未知 typed service 字段、非本 profile 的 obligation kind 或篡改签署正文。不能渲染为本 profile 的请求只留草稿，不能忽略未知字段后继续收费。

只读裁剪可以隐藏对方无关余额、私有 prompt 或内部 reasoning，但不能删去签署所需的任一腿、条件、来源、费用或风险。若调用者无权读取完整签署条款，返回 `terms_not_readable`，不返回可接受的摘要；同 hash、合法签名或“已看过摘要”不能补足这一前置条件。

<a id="contract-formation-authorization"></a>
### 6.2 接受证明与当前授权

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

每份接受证明绑定 `agreement_id/terms_revision/terms_digest`、A/B slot、完整呈现 digest、world/instance、该 owner 来源与 generation、Agent identity generation、仅此即时交换的动作/来源/对象范围、全部资源扣减上限及接受有效期。使用该当前 owner 来源支持的既有签名验证方式；不把 Agent 模型输出、module capability、另一个参与者的签名、历史 owner 或长期 delegate 当作该方接受。未知签名版本、source、proof 或缺完整呈现均拒绝。

接受登记与最终提交各读一次当前 owner/identity；最终提交必须在执行 parent 上再验证两方来源、密钥、generation、Agent 存在与可用、接受有效期及撤销/替换事实。两方签名必须覆盖同一 digest，允许分别到达；不允许把 A 接受 r 与 B 接受 r+1 拼接。两个签名均有效也不代表当前余额/位置/grant/费用仍有效。

改价、改变腿、改付款来源、费率/规则/schedule/manifest 或扩大时效使旧接受不可使用；重新报价产生新修订并让两方重新接受。owner 转移或主体变化不在同一合同中自动换人；新主体只能另建显式提议并引用原件，原事实保留。凭据 generation 变化后的同一主体也必须按当前绑定重新报价/接受，不恢复旧凭据的效力。

<a id="contract-formation-atomic-profile"></a>
### 6.3 两腿、费用与守恒

每条 Data 腿都检查实际 sender→recipient 的当前有效 grant。若双方都转 Data，必须分别有 A→B 与 B→A，只有一向授权不够。Data 在本 profile 中是可扣减的资源余额；权限 grant 是准入条件，不是另一件已交付资产，也不因本交易被新增、撤销或转授。交易/执行费用进入 canonical world treasury 的 Data 使用合法系统费用 authority，不伪造对 treasury 的 Agent grant。

完整 fee quote 只接受两类来源：已合法适用于此 profile 的交易费政策，以及 [Kernel/WASM 执行计量](../wasm/wasm-interface.md#sr2-metering-contract)。不引入公共 levy、新税率、任意 sink、返奖或 credit。旧 native `electricity_tax_bps/data_tax_bps/power_trade_fee_bps` 的存在不证明新 profile 可征收它们；只有明确 scope/version 的合法政策才可提供交易费，缺适用政策不得默认零费。

- 交易费本 profile 采用可判定的每腿同资产规则：`fee = floor(net_amount * effective_bps / 10000)`，`effective_bps` 为合法版本化政策给定的 `0..10000` 整数，payer 为该腿 sender，接收账为该世界对应资源 treasury。零费也必须是显式有效政策值。使用宽整数 checked 中间值，不能把旧 saturating 计算复制为新溢出语义。
- 执行费引用完整 schedule descriptor/hash、canonical 输入/输出大小、effect/emit/new-state 等实际计数与确定性费用公式，不由 Agent 自报。当前公式的 compute 以 Data 计，electricity 以 Electricity 计；最终使用绑定的合法 schedule。payer 从该 artifact 的当前合法计费来源解析，必须是 A/B 中的一名 Agent，并由其 owner 接受两种资源的全部上限；第三方 payer、无 owner 或无法说明 source 的“免费”都为 `fee_profile_unsupported`。不能为了让交易成功改由请求者或对手承担。
- quote 固定交易费及执行费的规则、来源、币种、接收账、精确计算输入/结果与每方每资源上限。提交在 staged view 按同版规则核对实际输出及费用；数量或费用不一致即 `quote_stale`，不能以“仍小于上限”为由收费。上限另防编码错误、遗漏和来源越界。缺任何必收费项、费用政策不支持本 profile 或 actual cost 无法在同一次事务中确定，均不提交。

对每名参与者 `p` 与资源 `r`：`gross(p,r) = outgoing_net(p,r) + trade_fee(p,r) + execution_fee(p,r)`。必须有 `pre_balance(p,r) >= gross(p,r)` 且 `gross(p,r) <= signed_cap(p,r)`；不能把另一腿或本次返还算作预先可用余额。对所有受影响余额统一聚合：`post_balance = pre_balance - gross + incoming_net`，treasury 增量等于同资源全部费用。每个中间量、最终余额和 treasury 都须在合法整数范围，禁止负数、隐式饱和、遗漏 fee 或顺序导致一腿先见效。

每资源守恒 oracle 为 `参与者总扣减 = 对手总净入账 + treasury 总入账`。本 profile 没有 sink、发行、奖励、声誉变更或其他权限效果。模块候选的实际 effect/state 写集必须恰好符合两腿、明示费用与协议事实；额外资源、grant、第三方扣减、组织活动或声誉效果使整批拒绝。

**可计算样例，仅作测试输入。** A 初始 Electricity=20、Data=20；B 初始 Electricity=10、Data=20。A→B Electricity=6，B→A Data=12，存在 B→A grant 且同位置。测试 fixture 显式提供适用于此 profile 的合法费用政策：Electricity bps=2000、Data bps=1000，各腿交易费为 1；合法 artifact payer=A，meter schedule 采用当前公式，输入/输出各 1024 bytes、2 effects、1 emit、new_state=true，产生 Data=7、Electricity=5 的执行费。两方上限覆盖 A 的 Electricity=12/Data=7、B 的 Data=13。成功后 A=(8,25)、B=(16,7)，treasury 分别增加 Electricity=6/Data=8。该数值不发布生产税率、schedule、实际模块输出或激活结论；实际候选不能提供相同合法输入时此例不可作为其成功证明。玩家是否接受此交换仍由其目标和取舍决定，合规不等于有利。

### 6.4 操作合同

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| quote/read terms | owner/Agent → Service/runtime | world/instance/profile；输入 digest、观察 root/tick、规则版本 | 只读；每次都返回其实际观察边界 | 完整 terms/fee quote 或 blocker；无登记、收费或接受 | 逻辑目标；可复用现有只读 preview，不等于已支持新协议 |
| propose/revise | 发起方当前 owner → 统一 admission/instance | request key、payload digest、agreement、expected record version | 创建需不存在；修订 CAS，固定双方，旧接受清空并保留修订关系 | `offered_nonbinding` 或稳定无效果拒绝 | 不映射旧 Open，不占旧游戏合同额度 |
| accept | 该 slot 当前 owner → 统一 admission/instance | 同版 terms 与 rendered digest、接受证明、request key | CAS；每 slot/revision 仅一个有效证明；精确重复返回原结果 | `acceptance_recorded_nonbinding`；两份齐备仍未履行 | 不映射旧 Accept 作为完整权威接受 |
| submit | A/B 当前 owner → Runtime root transaction | agreement/revision/digest、两份证明、expected record version、request key | canonical order；同业务键最多一个履行赢家 | fulfilled receipt；已履行引用原 receipt；稳定拒绝或 unknown | 不用两个旧 Settle 拼双腿 |
| withdraw | A/B 当前 owner → 同一有序入口 | 原 agreement、expected record version、request key | 与 submit/修订串行；只关闭未履行提议 | `closed_uncommitted`；若已履行则原 receipt | 不创建 native Cancel 或退款路径 |
| query original | 已授权当事人 → Service/runtime read view | 原 request key 或 agreement，所查 root/权威证明 | 只读查询；分页有界；不能用缓存缺失判未提交 | 原决定/receipt、当前提议状态或明确 unknown | 恢复旧/禁用版本的读能力必须保留 |

结构、签名、profile 与大小先检查；其后按原请求结果、业务终态、record version、当前权限、时效/位置/grant、费用、余额/溢出、事务证明的顺序判定。稳定错误至少区分 `unsupported_profile`、`terms_not_readable`、`render_mismatch`、`terms_stale`、`request_conflict`、`record_conflict`、`authority_stale/denied`、`expired`、`not_colocated`、`data_grant_missing`、`fee_profile_unsupported`、`quote_stale`、`insufficient_resource`、`numeric_overflow`、`profile_unavailable`。每项只披露调用者可见的必要字段；未知提交不是普通 error，也不归入这些无效果终态。

<a id="contract-formation-read"></a>
### 6.5 Agent、Viewer 与 API 的同源结果

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376。

签署前提供完整可读条款与 fee breakdown；只读观察者可得到依法裁剪的摘要，但无 accept/submit 权限。签署双方结果至少包括 world/instance、agreement/revision/digest、观察 root/tick、两方接受是否匹配及失效原因、effective proposal state、原请求查询键、是否有履行 receipt、每条实际腿与费用、前后 commitment 和可用下一步。余额只展示有权读取部分；核对费用和自身净结果所需事实不能省略。

`request_received` 只表示收到；`acceptance_recorded_nonbinding` 只表示签署事实已登记；`fulfilled` 必须引用合法 authority profile 的提交证明与完整 receipt；`outcome_unknown` 表示观察或提交状态尚不能确定。SDK/Viewer 不能从动作 ACK、local overlay、Agent 口述、空查询结果或两个局部 transfer 事件升级为 fulfilled。产品概念上的 active 边界在本 profile 由同一履行 receipt 的生效事实证明，不展示“服务进行中”。

两入口消费同一 canonical outcome，不要求相同 UI，但字段含义和权限裁剪一致。签署正文缺失时显示不可接受及具体 blocker；结果未知时提供原件查询，不能默认继续接受或扣费。B23/B24 的 `missing_full_write` 只有真实接口、消费者和相应验收另行交付后才能改变；本文不改 target 板上的可写结论。

本 profile 的因果读面复用 [DES-WR-IA-007](indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007)，以现有 causal-decision receipt 引用本合同原件，不另造通用因果状态机。将 Agent 的建议理由、证据来源、预期资源收益、全部费用和不成交/修订/暂缓的替代方向，与世界验证事实和实际净结果分开。纠正最早生效点是成功提交前的修订或撤回到达 canonical 顺序：修订清空旧接受，撤回先赢则无效果关闭；提交先赢则只展示原实际结果，不能伪报纠正已撤销交换。失败/unknown 引用原请求并解释当前可做的下一步；私有 reasoning 不因因果解释公开。该承接限此交换的理由、取舍及纠正边界，不推导全部 IA003 路径已覆盖。

## 7. 状态、事务与持久化

### 7.1 提议状态与竞争

| 状态 | 权威含义 | 可发生的转移 | 业务效果 |
| --- | --- | --- | --- |
| 本地 draft | 未登记的沟通材料 | 明确提出为 offered，或丢弃 | 无 |
| `offered` | 当前修订存在，尚无该版有效接受 | 接受、修订、撤回、到期 | 无；不保存资源 reservation |
| `accepted_pending_validation` | 当前版有一份或两份接受；齐备也没有自动执行权威结果 | 继续接受、显式 submit、修订清空接受、撤回/到期 | 无 |
| `closed_uncommitted` | 撤回或到期关闭，且该业务键无履行赢家 | 只读；新的交易须显式新提议并保留旧件关系 | 无罚款、声誉、退款或补偿 |
| `fulfilled` | 单次两腿与费用已提交，形成及履行 receipt 已建立 | 只读和原件恢复；新交易另行接受 | 恰好一次签署范围内效果 |

状态来自已验证记录与 canonical 时间。到期即使尚未物化关闭记录，也必须在 read/accept/submit 使用同一 `now >= expires_at` 判断为不可提交；物化关闭只写元数据，不调用旧 `EconomicContractExpired`。版本/授权/位置/费用失效作为当前可执行性 blocker，不自动复活、续期或改条款。新修订是显式操作并清空旧接受；关闭和履行终态不可修订。

所有提议写操作用 `expected_record_version` 比较并更新。两个接受并发时，冲突方查询最新记录；若 terms 未变，可在确认原请求结果后用新 transport request 提交同一接受证明，不能跨修订复用。submit 对同一业务键的赢家与 revision CAS 在同一根事务判定；不同合同竞争同一余额亦由 canonical parent/order 串行判定，不建立本合同的独立锁或账本。

<a id="contract-formation-commit"></a>
### 7.2 一个提交点与一个业务 receipt

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

在合法有序输入上准备候选，候选写集包括：第二次校验的双方证明引用、两腿/全部费用的聚合余额 delta、treasury delta、履行赢家与终态、原 request 的精确结果、module-owned 状态、canonical event/sequence、execution commitment 和业务 receipt 引用。prepare 期间只能修改暂存值；module call、计费、序列号与事件也不能提前发布。最终 effect 集必须与签署 profile 完全一致。

复用 [统一 ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)、[authority commit seam](../design.md#runtime-authority-commit-recovery-target) 与 [generation 保存](../design.md#runtime-generation-persistence-design)。已合法 profile 的提交决定、parent/head/epoch、完整输入结果及去重事实达到规定持久条件后，才发布 `committed`。局部 clone 安装、journal append 或单机 fsync 不单独满足该边界；本机和独立故障域等条件由原权威合同决定，本文不降级。

receipt 复用正式 execution receipt/decision 的 identity 和 finality wrapper，增加本 profile 的绑定 payload：业务键、revision/terms/render digest、双方 owner/identity 与接受证明引用、实际校验 root/tick、位置/grant/费用版本来源、两腿净额、各方各资源总扣减、trade/meter breakdown、treasury 增量、前后 state commitment、履行终态及原 request 引用。业务 receipt 的 hash body 不嵌入尚未形成的 containing-block wrapper，避免递归承诺；签名原件和大证明按内容引用保留。

只有确认候选未提交时才能丢弃暂存业务写集，按原请求键提交稳定 `rejected_no_effect`/fault disposition；该决定本身若不能可靠提交，则返回 unknown，不伪造拒绝。失败不收费，包括已在暂存视图计算的 module 费用。资源不足等确定拒绝可保留原非绑定提议；在确认原结果后显式重报使用同一业务键、仍有效的条款与接受，换 transport request 但不得换业务键规避去重。条款/规则失效则新修订重新接受。后提交维护、镜像或 GC 失败不能把已履行改成 rejected。

<a id="contract-formation-recovery"></a>
### 7.3 原请求、业务去重与恢复

Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f。

传输键为 `(world/branch, module_id, instance_id, requester_owner_identity, request_id)`，payload digest 绑定操作、业务键、修订、expected record version、完整命令与证明引用；键不随重连、artifact 升级或 credential generation 重建。相同键相同 payload 返回原决定；相同键不同 payload 为 `request_conflict`，不能覆盖。读取旧结果须重新验证访问权，但不能把当前授权变化解释成重执行已提交效果。

业务键独立于传输键并跨 revision 保留。另一个 request 对已 fulfilled 的同版合同只引用原业务 receipt，不能再执行或新发奖励；不同 terms 不能夺取已履行键。请求拒绝只说明该次提交未产生业务效果，不证明另一个请求没有履行同一合同，消费者还须读取业务终态。新合同 ID 表示新交易，必须重新取得两方签署，不能作为自动 retry 策略。

| 观察/故障 | 必须执行的恢复 | 禁止推断 |
| --- | --- | --- |
| 客户端断连、超时、某节点读面滞后 | 查原 request 与业务键，标记观察 root/新鲜度；可重发同键完全相同输入 | 不能凭 timeout/newest-cache-empty 判未提交；不因此冻结其他合同 |
| 权威查询返回无此记录于 root R | 保留 `not_found_at_root` 边界；相同原键可恢复投递 | R 的缺失不能排除在途请求以后提交，不允许据此换 ID 自动再做 |
| 确定未提交并有持久拒绝 | 返回原原因；按重新报价/同业务键重报/停止的明确分类处理 | 不重复收费，不将拒绝描述为退款 |
| 找到 committed 履行事实 | 返回同一 receipt 和结果；补齐缓存/投影 | 不重演两腿、不按当前费率重结算 |
| 根 generation 的 `StatusUnknown` | 按 [G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design) fence 整个旧 writer：停止其后续 mutator/save 和依赖派发；核对精确候选与持久决定，安装匹配内存后才解除 | 不能只冻结此合同却让不确定旧 World 继续写“无关”业务；也不能凭可读旧代推断未提交 |
| restart/snapshot/replay | 验证完整 generation、历史 activation/manifest/schedule、请求结果与业务赢家；只归约 canonical 事实，按原规则证明恢复一致 | 不执行 Agent/LLM/sandbox/外部行为，不用今天的 owner/费用重写历史 |

数据 owner 是统一 runtime 的 instance state、请求结果和 generation 保存，不是 Viewer 缓存。恢复闭包包含当前及必要历史 terms、接受证明、版本/权限/fee 来源、请求 digest/决定、业务赢家/receipt 与其引用工件。可按现有 storage 治理迁移大正文，但最低去重摘要和恢复依赖不得因普通时间 TTL、module disable、owner 变化或客户端清历史而删除；只有原存储合同能证明旧请求/签名已不可合法重放且不破坏恢复闭包，才允许相应回收。缺索引、丢 proof 或混代状态须保持不可写/明确 unavailable，不能把“查不到”恢复成新提议。

## 8. 部署、安全与运行约束

只在已分类、合法治理激活、artifact/instance/fee schedule 完整且达到 runtime 服务闸门的 world 中开放本 profile。dev/local fixture 使用独立 world identity，不能并入全局历史或证明 hosted/BFT 行为。实际 authority profile、writer epoch、持久故障域和恢复证书遵守已有部署合同；客户端无权自行启用。

权限最小化为两名直接当前 owner：发起方可提出/修订；各方只能登记自己的接受；当前双方可提交已双签原件或撤回未履行提议。签名验证、完整条款读取和当前 source/generation 三者都必须成立。历史当事人可在验证历史身份连续性和受众权限后读其原决定，不因此获得新 owner 的余额、权限或新写权。日志记录必要业务身份、root、reason 与证明引用，禁止复制私钥、私有 prompt 或对手无关数据。

外部 I/O、消息投递、运输、Oracle 回调和人工自由裁决不属于成功写集；本 profile 无需要另行完成的外部效果。不可用 profile 只读展示和查询历史，写入口返回 blocker。服务不可用与未激活是能力状态，不能伪装成玩家违约。对不可信编码、伪造 scope、跨 instance 重放、过期证明和多余 effects，先拒绝再考虑业务余额，不执行有副作用的“验证”。

## 9. 质量与容量

v1 采用确定结构上界：恰好两方、两腿、每 slot 至多一份当前接受；`TermsBodyV1` 的 canonical CBOR bytes 不超过 16 KiB，含证明引用的单个完整命令 canonical bytes 不超过 32 KiB，非绑定注释不超过 2048 UTF-8 bytes；分页最多 50 项。固定身份使用现有 Id 字节规则，不 trim/casefold。协议限额、最大提议存续 tick 和每方未关闭元数据条数由激活 manifest 给出正数有界值并参与 quote，缺失即不可启用；它们只限 CPU/存储，不成为可交易资格或游戏额度。更新限额不得删除仍需恢复的原结果。

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 双腿及费用 | 固定独立 world，2 owner/2 Agent/2 legs，合法规则/计量输入 | 精确整数守恒，零部分发布，receipt 一份 | §6.3 逐资源等式；无需吞吐推断 | [CF04](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-04-atomic) | 目标，未运行 |
| 并发与重试 | 同合同至少两 submit；不同合同竞争同余额；撤回/修订同时到达 | 单业务赢家，所有 request 原结果稳定，不超支 | §7 canonical order/CAS/业务键 | [CF05](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-05-races) | 目标，未运行 |
| 权限与呈现 | 当前/旧 owner、两个 Data 方向、受限读者、正文篡改 | 无越权签署、无权限效果、决定性字段不缺失 | §6.1/6.2/6.5 | [CF03](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-03-authority)、[CF07](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-07-read) | 目标，未运行 |
| 边界与容量 | terms/command/注释/page 分别取阈值和阈值+1；过期等号；金额/treasury 上界 | 有界解析和确定拒绝，旧原件仍可查 | 本节结构上界、合法 manifest 限额和 checked 数值 | [CF02](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-02-terms)、[CF04](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-04-atomic) | 目标，未运行 |
| 崩溃与确定恢复 | prepare 前后、持久决定、回复前、snapshot/replay；同一候选和合法 profile | 只恢复原效果/原拒绝；unknown 期间不错误续写 | §7.3 与 G1/G2/G3；真实故障域另取证 | [CF06](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-06-recovery) | 目标，未运行 |

本文不新增延迟/吞吐 SLO，没有硬件、并发生产分布和样本窗口时不声称性能达标。实现应按主键和有界 revision/expiry 索引访问，不能为一次签署扫描全世界合同或历史；容量失败不删除 receipt 或降级去重。

<a id="contract-formation-compatibility"></a>
## 10. 兼容、迁移与回滚

native Alliance/EconomicContract 继续为 `Compatibility keep`。现有 Open/Accept/Settle/Expired 的 enum、serde、事件字段、raw replay、单腿付款、税/声誉及 expiry 语义保持原样；旧 Open 不是本设计 offered。旧 Service 即使存在于旧状态类型也不能经新 profile 激活；旧 action 只能按其已实现含义显示，不能成为新两腿合同的适配假象。

新目标只能作为独立版本、明确 instance 的 module-owned 合同接入。本文不选择 SC-32 的唯一 pilot，不新增 core Action enum、专属 WorldState 顶层字段或另一个最终性/余额账本。启用前必须满足下列真实依赖，缺一即 `profile_unavailable`：

1. 产品/governance 已按既有分类决定允许的 institution 路径与 activation；artifact/schema/instance/权限及 fee authority 完整。普通文档采纳不提供该授权。
2. 同一统一入口已实现免费且有界的非绑定协议登记，以及最终两腿/所有费用/receipt/request 结果的单事务提交；普通成功 module 状态更新仍收费时不能开放 offered/accept 写入。
3. 实际 typed 命令、canonical codec/签名向量、owner/grant/位置/费用读取、正好两腿写集校验、事务/持久化/unknown 恢复接缝完整，消费者可查询原件；不能以某个尚未定义的 handler 作为成功分支。
4. 本设计及手册的行为、旧协议兼容、instance 隔离、故障/replay 和真实消费者验证取得实际候选证据；局部 fixture 与源码存在不代替剩余环境。

新旧协议显式分流读取，不双写同一业务，不把旧 Open/Accepted 迁成已零效果接受或补造 owner 签名。升级保留稳定业务键及原请求去重，不以新 artifact/activation 重置它们；旧 pending 的绑定变化后只能重新报价/接受，历史履行按旧版本读取与回放。禁用停止新提议/接受/提交，但保留原件读、恢复与防重放事实；未履行记录按原有效期无效果关闭。

回滚到已知旧 binary/schema 仅在它能完整验证并保留新 generation、receipt 和去重事实时可继续服务；否则保留新数据并进入已验证只读/隔离，由原恢复流程处理。已提交两腿不能因代码回退被撤销，不能恢复旧快照后让同签名再次成交。不存在自动“反向交换”或借回滚实施赔偿。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

下面是稳定验证计划，不是已通过的结果。实际 candidate、manifest、费用/owner 输入、authority profile、故障边界及运行证据归执行 PR。每一业务场景均先记录本 profile 的真实启用前置；前置不成立时报告 blocked/未执行，不用旧 native 行为代演。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | [scope](agent-mediated-contract-formation.design.md#contract-formation-scope) | 草稿/提议/单或双接受无本金、费用、游戏额度、顺位、声誉或权限效果 | [CF01](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-01-nonbinding)；固定候选，比较各阶段业务前后差分；区分只读 preview 与持久登记 | 原请求、结果、差分及独立核对记录 | 文档与旧空输出测试不证明新登记路径 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | [terms](agent-mediated-contract-formation.design.md#contract-formation-terms) | 完整同版条款与正文；无环摘要、CAS 版本隔离、修订失效和结构上界 | [CF02](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-02-terms)；固定 canonical bytes/模板与正反例；按结构/manifest 限额取边界和超限输入 | 两方呈现、摘要向量、签署输入与拒绝决定 | 任意自然语言理解；其他合同类型 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | [authorization](agent-mediated-contract-formation.design.md#contract-formation-authorization) | 双方当前直接 owner、scope/generation、每方向 Data grant 与 canonical 位置重验 | [CF03](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-03-authority)；固定两方身份，注入转让/换代/撤权/移动及第三方 payer，在实际提交边界校验 | authority 来源、前后 root、拒绝及无效果差分 | 组织/长期 delegation/继受 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | [atomic profile](agent-mediated-contract-formation.design.md#contract-formation-atomic-profile) | 两腿、每方每资源全部费用、上限、treasury 与逐资源守恒 | [CF04](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-04-atomic)；合法费用/计量输入的独立算例，分项足额但合计不足、跨资产费、第三方 payer 及 overflow | 聚合账目、费用来源、资源/treasury 差分 | 不证明生产费用适用权和激活 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | [commit](agent-mediated-contract-formation.design.md#contract-formation-commit) | 形成、两腿/费用、终态和 receipt 同次；提交/撤回/修订及共享余额按 canonical 顺序产生唯一结果 | [CF04 与 CF05](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-04-atomic)；同一手册的原子提交故障点及 CF05 两种权威竞态顺序；记录真实 parent 和唯一赢家 | 原 receipt、各请求结果、提交证明与余额/版本差分 | fixture 不代签跨故障域持久性；未执行节点组合 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | [recovery](agent-mediated-contract-formation.design.md#contract-formation-recovery) | 断连只查原件，业务去重跨 request/revision，StatusUnknown fence 原 writer，恢复不重算/重扣 | [CF06](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-06-recovery)；分别注入客户端读未知与 G1 提交未知；实际 snapshot/journal/历史版本和去重恢复 | 原键原 payload、candidate/决定、writer fence、恢复 root/receipt | CR002 checkpoint 恢复；未执行真实故障域 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[AC-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-001) | [read](agent-mediated-contract-formation.design.md#contract-formation-read) | 完整签署信息与受限历史观察分离，各入口不把 ACK/缓存/unknown 当履行 | [CF07](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-07-read)；同候选/root 的双方和旁观者，实际 Agent/API/Viewer 各按已接入层取证 | 受众投影、receipt 引用、blocked 下一步 | 未接入 UI、浏览器和跨节点体验 |
| Trace refs: O7-REQ-b38abaf7-824d-47b7-a954-bcdb57897376；[AC-WR-IA-003](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#ac-wr-ia-003) | [read](agent-mediated-contract-formation.design.md#contract-formation-read) | 此交换的理由/证据、成本和替代、纠正与实际净结果可沿原因果链核对 | [CF07](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-07-read)；固定交换与 CF05 撤回/提交顺序，核对原 causal receipt 和合同结果链接 | 原意图/理由/证据、修订或撤回、实际净结果与 receipt | 通用因果协议、其他社交/治理/冲突、真实玩家理解度 |
| Trace refs: O7-REQ-16ef3247-1587-4cd8-84ab-03a9bb18a17f；[REQ-WR-CR-001](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-001) | [compatibility](agent-mediated-contract-formation.design.md#contract-formation-compatibility) | 旧单腿/expiry 原义保留；未知服务拒绝；不绕开 activation/计费/实例边界 | [CF08](../../testing/manual/agent-mediated-contract-formation-acceptance.manual.md#cf-08-compatibility)；固定旧数据与新目标反例，旧回归另按原测试入口执行 | legacy 序列/快照差异、unsupported 结果、启用缺口 | 不证明 SC-32 pilot 已选择或整个 institution migration 完成 |

已有 [经济合同回归定义](../../../crates/oasis7_runtime/src/tests/gameplay_protocol_regressions.rs) 和 [raw publication 回归定义](../../../crates/oasis7_runtime/src/tests/economic_contract_publication_transaction_regressions.rs) 可定位旧协议的拒绝、溢出、重放边界；它们不是本目标两腿/双签/恢复的业务测试，本设计没有运行它们。文档检查只能证明所选链接、结构和追踪关系。

## 12. 决策、长期风险与未决问题

### ADR-CF-01：先闭合即时形成与履行

决策 owner 为 runtime/gameplay；适用条件为本 profile 的两个人、两 Agent、Electricity/Data、同位置及完整费用 authority。选择独立的 module-owned 形成合同，并在一次原子提交中立即履行；不扩旧 native Open，也不先开放一个通用 active 状态后留执行器待定。这样能完整核对签署、双腿、费用、拒绝和原件恢复，代价是组织、运输与持续服务仍需各自完整设计。只有新 profile 的产品规则、专业执行合同和验证已齐备时才能扩大；扩展须新版本明确承接，不能借相同字段名自动获得能力。

| 风险/未完成依赖 | 负责 authority | 当前处置与解除触发 |
| --- | --- | --- |
| 免费非绑定持久登记与统一事务尚未完整接通 | runtime/WASM | 本设计明确目标边界，实际写入口禁用；须证明成功元数据登记不发布任何方费用、游戏预算或其他业务效果 |
| 新 profile 的生产交易费适用权、module payer/计量 quote 不完整 | gameplay/governance 与 runtime | 不默认套旧税、不改付费主体；合法政策、schedule、source、两方上限和同事务费用证明齐全后才可提交 |
| 分类、唯一 pilot、activation 与 codec/consumer 实现未交付 | 产品/governance、runtime、Service/Viewer | 保持 Compatibility keep 和 target 状态；按原 activation 与本手册取真实证据，不由本设计选择 pilot |
| 无效果拒绝被误当已提交失败，或 unknown 被误当可重做 | runtime/storage、客户端 | 按 §7 原请求/业务键、G1 writer fence 和完整恢复闭包处理；验证提交前、提交未知、提交后维护三种边界 |
| CR001 的其他合同、多方与长期执行仍缺专业组合 | 产品与相应专业域 | 产品状态仅为部分覆盖；按具体 profile 冻结授权、执行、失败及恢复，不用此原子样例覆盖全部合同 |
| CR002 的 collateral/evidence/remedy、检查点、争议、终止与继受 | gameplay/runtime/testing | 保持原设计/实现状态；[CR002](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-002) 的服务种类、证据和经济后果另行决定，本文只保证 unsupported 不生效 |

身份键、CAS、稳定错误、结构上界和恢复索引是本专业设计的实现选择，不升级为玩家审批事项。需要产品决定的是后续服务的玩法与责任后果，或已有治理 authority 的正式激活；这些缺口不影响本次有界目标设计的独立审读，也不意味着该目标已经可以运行。
