# 社会系统生产级方案：事实账本 + 声明式关系层（设计文档）

- 对应设计文档: `doc/world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

审计轮次: 5

## 1. Executive Summary
- 不预设单一“正确声誉模型”，为世界提供生产级社会系统底座。
- 将“信任/声誉/关系”从硬编码规则改为可组合的事实与声明机制，保障高自由度。
- 保证跨节点可重放、可审计、可治理，并具备完整争议处理闭环。

## 2. User Experience & Functionality
### In scope
- Simulator 内核新增“社会事实账本”能力：发布、质疑、仲裁、撤销、过期。
- 新增“声明式关系边”能力：任意主体可基于事实声明关系（信任、合作、黑名单、声誉维度等）。
- 新增治理/安全护栏：证据引用校验、权益质押（可选）、角色权限校验、状态机约束。
- 新增回放与持久化闭环：事件驱动回放保持确定性。
- 新增 `test_tier_required` / `test_tier_full` 测试覆盖。

### Out of scope
- 单一固定评分公式（例如“全局唯一信誉分”）。
- 绑定经济系统的硬编码定价联动（后续由模块/策略接入）。
- 性能优化与索引压缩（本轮优先语义完整性）。

## 设计原则
- 最小语义内核：内核只记录可验证事实与状态，不强行解释含义。
- 多制度并存：不同 schema/关系维度可并行存在、互不覆盖。
- 证据优先：事实必须携带可追溯证据引用（事件或动作）。
- 争议可闭环：符合当前生命周期与资格条件的事实可被质疑，并由有权动作给出终局裁决；历史挑战不是重开终态的许可。
- 事件溯源：所有状态变更均以事件入账，可回放还原。

## 3. AI System Requirements (If Applicable)
- N/A: 本专题不新增 AI 专属要求。

## 4. Technical Specifications
### 1) Action 扩展（simulator）
- `PublishSocialFact`
  - 字段：
    - `actor: ResourceOwner`
    - `schema_id: String`
    - `subject: ResourceOwner`
    - `object: Option<ResourceOwner>`
    - `claim: String`
    - `confidence_ppm: i64`（`1..=1_000_000`）
    - `evidence_event_ids: Vec<WorldEventId>`
    - `ttl_ticks: Option<u64>`
    - `stake: Option<SocialStake>`
- `ChallengeSocialFact`
  - 字段：
    - `challenger: ResourceOwner`
    - `fact_id: u64`
    - `reason: String`
    - `stake: Option<SocialStake>`
- `AdjudicateSocialFact`
  - 字段：
    - `adjudicator: ResourceOwner`
    - `fact_id: u64`
    - `decision: SocialAdjudicationDecision`（`Confirm` / `Retract`）
    - `notes: String`
- `RevokeSocialFact`
  - 字段：
    - `actor: ResourceOwner`
    - `fact_id: u64`
    - `reason: String`
- `DeclareSocialEdge`
  - 字段：
    - `declarer: ResourceOwner`
    - `schema_id: String`
    - `relation_kind: String`
    - `from: ResourceOwner`
    - `to: ResourceOwner`
    - `weight_bps: i64`（`-10_000..=10_000`）
    - `backing_fact_ids: Vec<u64>`
    - `ttl_ticks: Option<u64>`

<a id="social-preview-contract"></a>
### 1.1) 玩家侧社会动作后果预览

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)与[产品验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)的五动作只读后果预览。本文拥有玩家可读含义、选择与专业验收；精确读点、接口、消费者验证、读取上限和恢复由[配套设计](social-fact-ledger-declarative-reputation.design.md#social-preview-source)承接；方法见[预览验收手册](../../testing/manual/social-action-consequence-preview-acceptance.manual.md)。

- 当玩家准备发布事实、质疑事实、仲裁事实、撤销事实或声明关系边时，玩家侧必须能读取 `social_fact_impact_quote` / `relationship_consequence_preview`，用于判断该社会动作会影响哪些信任、合作、黑名单、治理、claim 或交易协作表面。
- 最小字段：
  - `actor_id`
  - `action_kind`: `publish_fact` / `challenge_fact` / `adjudicate_fact` / `revoke_fact` / `declare_edge`
  - `schema_id`
  - `subject_id`
  - `object_id`
  - `claim_summary`
  - `confidence_ppm`
  - `stake_at_risk`
  - `ttl_ticks`
  - `affected_relationships`
  - `affected_social_surfaces`
  - `cooperation_opportunity_delta`
  - `blacklist_or_dispute_risk`
  - `governance_or_claim_relevance`
  - `recommended_social_action`
  - `why_this_action_matters`
- Edge case: 若玩家准备发布/质疑事实或声明关系边，但看不到影响对象、可见社交表面、合作机会变化、争议/stake 风险或推荐理由，标记为 `social_fact_impact_quote_missing`。
- Acceptance: 玩家提交社会事实或关系声明前，至少能看懂“这条事实/关系会影响谁、影响哪些协作/交易/治理入口、我冒了什么风险、为什么现在值得发或应该暂缓”。
- Non-goal: 本预览不新增全局唯一声誉分公式，不把社会事实硬绑定到经济定价，不重做仲裁权限、stake 结算或事实状态机，不新增复杂外交 UI、社交网络图或完整 NPC 谈判系统，也不改变 `PublishSocialFact` / `ChallengeSocialFact` / `DeclareSocialEdge` runtime ABI。

#### 1.1.1 目标、来源与适用边界

玩家在提交前应能区分“我打算声明什么”“原社会账本会怎样变化”“谁可能据此作判断”和“哪些后果目前无法确认”，再选择继续、修改声明或范围、核对来源、暂缓。预览沿原五动作和原资格工作；不替玩家发布、接受关系、下注、裁决或撤销，也不产生组织代表权、独立仲裁权、个人 credential 或成熟世界机会。

每份预览须绑定完整候选动作、原世界与来源读点、当前请求主体和适用版本，并让玩家识别作者、声明所指主体、关系两端、原事实/关系及受影响范围。主体类型有意义：同名 Agent 与 Location 不是同一人或同一对象。事件引用必须能定位到同一来源中的实际事件；无法读取、尚未同步、来源不支持与确实不存在分别表达。原 source 没有地点、服务、用途或身份连续性证据时，不能从名称、当前位置或叙事补齐。

原事件存在不证明它支持自由文本内容。首版不替任意 `schema_id` 或 `claim` 建立真伪判定；可核验的账本状态、引用关系及支持范围按真实来源显示，语义支持尚未建立时明确说明。来源生产、当前读面或消费者尚未接通的入口保留能力缺口，不能用测试 seed、合成事件或另一个世界的成功序列填补。

本节要求完整解释所预览的执行边界。单动作预览须同时考虑成功动作与紧随其后的原生命周期维护；采用批次的入口须说明批次边界和其间依赖，不能把独立单步结果冒充整批结果。时间、资格、原程序与资源结算继续由[生命周期合同](#social-lifecycle-contract)和原规则拥有。

#### 1.1.2 声明、状态与声誉含义

| 预览信息 | 玩家应读到的含义 | 不能据此作出的结论 |
| --- | --- | --- |
| claim、理由、notes 与事件引用 | 谁作出何种声明，引用哪些可读原件，哪些关联已核实、哪些尚未支持 | 字段非空或 event ID 存在就代表内容属实；摘录等于完整事实 |
| `confidence_ppm` | 发布者提交的声明强度及原值 | 已校准的真实概率、系统可信度或独立验证等级 |
| `Active / Challenged / Confirmed / Retracted / Revoked / Expired` | 原 v1 账本状态及其 backing 资格 | 全社会共识、对内容的独立裁决或所有制度中的信用状态 |
| declarer、from/to、relation kind、`weight_bps` | “由 A 声明 B→C 的某种关系”，保留原方向、原值和 schema 限定 | A 代表 B 或 C、双方同意合作；负 weight 自动构成处罚或正向机会 |
| 原 TTL 与实际截止 | 原来源中的开始、截止和当前状态；未提供 TTL 时明确“未声明自动到期” | 重新预览重启期限；无 TTL 就是永久有效信用或不可纠正的永久 blacklist |
| 名称含 reputation/trust/blacklist 等词 | 声明自报的类别，须由实际 schema 与消费者验证其用途 | 名称命中关键词即改变评分、治理、claim、交易价格或机会 |

原 `Confirmed` 是该原制度的状态。v1 可表示主体中仅事实发布者有裁决权；玩家必须能看懂“原发布者确认/撤回了其声明”，不能包装成无冲突独立 panel。`ResourceOwner::World` 尚非该 ABI 的可用裁决主体，产品的完整有限争端程序也不由这项 v1 动作代签。

预览保留历史记录及其原状态变化，不因撤销、到期或关系失效删除因果。无 TTL 的旧记录也不因此授予跨情境、永久或无法复核的限制；其下游用途只能由实际消费者自己的合法范围、时效与救济决定。本节不添加新的年龄窗口、自动评分、上诉程序或旧事实重开权。

#### 1.1.3 五动作的条件后果与质押

下表描述原资格、容量和来源齐全且实际提交成功时的直接后果；预览时所有效果均未发生。某个前置尚未核实，必须在相关后果旁说明，不能以可读文案代替执行资格。

| 候选动作 | 必须解释的原账本、关系与资源后果 | 玩家可据此作出的选择 |
| --- | --- | --- |
| `PublishSocialFact` | 作者、subject/object、完整声明、证据引用、原 confidence、TTL 和可选 stake；成功登记一条原事实，按原规则锁定作者指定资源。还须解释原维护切点是否已触发到期，不承诺该事实一定继续 Active | 核对引用与措辞，修改或缩小声明，调整原动作允许的投入，继续或暂缓 |
| `ChallengeSocialFact` | 原事实当前状态、已有 challenge、挑战者及理由；合法首次提交锁定其原 stake，转 Challenged。在单步后的原维护切点，依赖该事实且仍 Active 的关系边失去 backing 后变 Expired；无关关系不因此失效 | 比较可核争点和自身投入；继续挑战、查原件或暂缓，不把开案当胜诉 |
| `AdjudicateSocialFact` | 当前原发布者资格与原 challenge；Confirm 按原规则将挑战者剩余 stake 记入对应系统池并返还发布者剩余 stake，Retract 反之；展示事实与 backing 的条件变化。Confirm 不复活已 Expired 的旧边，新的边仍须另行声明并满足当时全部 backing/TTL 条件 | 比较两种决定的明确状态与逐资源结算后果；继续所选决定或暂缓，不称为独立仲裁 |
| `RevokeSocialFact` | 原发布者资格、原事实状态、仍存的双方 stake 与返还容量；成功撤销并按原规则返还剩余 stake。原维护使仍有效的依赖边失效，已失效边和无关边保持原义；历史与其他制度已合法发生的结果不被删除 | 看清撤回的具体范围及返还前置，继续撤回、核对来源或暂缓 |
| `DeclareSocialEdge` | 作者与 from/to 分开，保留方向、关系种类、原 weight、全部 backing 及 TTL；成功只登记这条声明。原规则不为该动作新增 stake；任一 backing 在原维护切点不再合格或边已到期时，不能继续显示有效 | 核对自己是在作第三方声明还是描述自身关系，修改范围或措辞、继续或暂缓；不假称他方已同意 |

质押按“原主体＋资源种类＋数量＋原去向”分别说明：本次新锁定、仍在风险中的原 stake、所选决定下可能入池、可返还及尚缺的返还条件。Data 与 Electricity 不能相加成一个风险数；同为零的“没有 stake”“尚不能读到 stake”和“没有本次新增 stake”也不能混用。兼容字段 `stake_at_risk` 只有数量时，客户端仍须从获准的同源解释取得单位与主体；不能默认为 Data、货币或调用者的总损失。

**可复算样例。** 这是原规则的示例输入，不新增玩法参数。A 的 Data 为20，B 的 Electricity 为9；来源完整、无其他资源变动、无到期或容量障碍，逐动作执行并完成原维护。A 发布 F 并押10 Data 后持有10 Data；基于 F 的旧边 E 随后生效。B 挑战 F 并押4 Electricity 后持有5 Electricity，F 转 Challenged，E 在该步维护后为 Expired。下列是同一前态的三个替代选择，不能顺序叠加：

| 后续选择 | A 的 Data | B 的 Electricity | 本组系统池增量 | 关系边 E |
| --- | --- | --- | --- | --- |
| A 执行 Confirm | 20 | 5 | ＋4 Electricity | 保持 Expired；F 的新 backing 资格仍受其原期限约束 |
| A 执行 Retract | 10 | 9 | ＋10 Data | 保持 Expired |
| A 执行 Revoke | 20 | 9 | 无 | 保持 Expired |

预览上述任一选择时，仍保持原前态；实际成功后才有表中资源效果。若 F 之后合法 Confirm，重新声明一个 `weight_bps=-5000` 的边只表达该 schema 下的原负值，不能显示“合作机会增加”；即使 `confidence_ppm=700000`，也不能称内容有70%的真实概率。将任一声明改为无 TTL，只能说明未声明自动到期，不产生永久信用或处罚。

#### 1.1.4 当前受众、动作资格与历史主体

查看预览、查看证据和提交动作各自消费其当前权限。能看某个原件不代表能裁决；当前控制某个 Agent 也不自动代表所有受影响主体。原发布者/声明者、原关系两端和当前请求者分别显示其有权可见的身份；使用同一个 Agent ID 或成为后任不能继承前任个人 credential、私有证据或未取得的代表权。当前受众不能查看的事实、stake、关系或证据，不通过错误文本、隐藏项数量或推荐理由间接泄露。

声明者可以按原动作描述第三方关系，但预览必须直说“由 A 声明 B→C”，不能把显示名称省略成“B 与 C 已建立关系”。这里不新增对方接受动作或放宽原授权；当前来源无法证明作者、受众或原关系时，相关部分保持未核实。仅有 Agent/Location 的来源也不能假装包含玩家、组织或岗位的历史身份合同。

引用和自由文本以原声明内容呈现。摘要必须保留否定、范围与争点；截断时可在当前权限内查看完整候选或原件，不能只凭截断后的正面片段给出建议。私人证据不可读时可说明受限及合法下一步，不能借“补齐预览”给后来 owner 开放原文。

#### 1.1.5 真实消费者、各入口与玩家表达

合作、治理、claim、交易或机会的影响必须有具体消费者依据。预览应区分以下情形，允许同一动作在不同表面具有不同结论：

| 可核情况 | 玩家表达与边界 |
| --- | --- |
| 原社会账本的直接条件后果已核实 | “若按当前条件提交成功，将登记/挑战/撤回这条原记录，并发生所列资源或 backing 变化。”预览本身不执行 |
| 当前消费者已接入且其规则与输入可核 | 指明该消费者、用途、范围和当前条件下的解释；消费者自身的提交、重验、容量与 receipt 仍独立，不保证机会、访问、排序或奖励 |
| 所查消费者范围完整核实为未接入 | “本版本已核范围内，该表面未消费这类记录。”只对该范围成立，不预测其他玩家如何选择或未知外部模块 |
| 消费者不支持、未就绪或覆盖不完整 | “该后果尚不能确认。”显示缺口和可用下一步，不能填零变化、固定 positive 或保证无风险 |
| 来源或消费关系当前不可读 | 仅说明当前受限，不披露隐藏内容，不把无权当成不存在或未接入 |

若本次必需的来源、当前权限、质押风险或所声明消费者覆盖缺失，`recommended_social_action` 只能给出查证、修改、刷新或暂缓等当前可执行的下一步，不得继续默认推荐发布、确认或其他社会提交。原世界提交资格仍由原执行入口负责，本预览不新增世界准入闸门。

`cooperation_opportunity_delta` 等原字段保留其兼容用途，但文本必须来自上述证据与范围；无消费者依据时不得宣称正负收益。关联 schema、原事实/关系状态与未来资格是不同层次。原声誉分、治理 vote cap、claim、工业许可、价格或成熟世界机会消费者没有被此次预览重新接线；如果某消费者确实读取相关来源，应按其实际合同解释，不能静默假定已接入，也不能笼统宣称全世界零影响。

同一候选、同一受众和同一读点的 Viewer、API、Agent 应提供同义的主体、前置、后果、风险、来源缺口及选择。某入口未支持某动作，就明确未支持；不能从另一入口成功或一份 seed fixture 推定已具正式能力。Agent 可解释原结果，不能用模型总结填补来源、预测对方同意、将 `Confirmed` 改写成独立真相或代玩家提交。首次社交接触预览保持原范围，不被替换为社会事实发布。

| 必要可读表达示例 | 玩家能够作出的判断 |
| --- | --- |
| “找到了这条事件，但尚不能证明它支持你的声明。” | 核对原件、改写范围或暂缓，不把引用存在当真实性背书 |
| “这是 A 对 B→C 的声明；B 和 C 的接受未由这条记录证明。” | 理解作者、关系对象与同意的差别 |
| “这是原发布者的 Confirm；原独立仲裁程序未由此完成。” | 区分 native 状态和更广的争端程序 |
| “本次选择会将 B 的4 Electricity 记入原池，返还 A 的10 Data；现在尚未执行。” | 比较不同主体、单位与决定；不把预览当到账 |
| “旧关系边已失效；确认该事实不会恢复这条边。” | 决定是否按原权限另行声明，避免误以为历史关系自动重建 |
| “该治理/claim 表面的消费关系尚未核实。” | 等待合法来源或沿原入口查证，不据此承诺权重或资格 |

#### 1.1.6 只读、新鲜度与原提交恢复

预览、刷新和重复预览不改变社会事实/关系、stake、库存/池、世界时间、原动作队列、评分、资格或其他世界效果，不创建世界 receipt、保全或收费。技术鉴权与必要的只读服务记录不被解释为动作已受理。报价不预留 stake 或返还容量，也不阻止原世界继续变化。

输入、当前权限、事实/关系状态、实际截止、资源条件或消费者版本变化后，旧预览只能保留其原读点含义；需要最新判断时重新读取，提交仍按原执行边界重验。超时、来源未知或过期不以旧文案补签“仍可执行”，也不重启原 TTL、挑战资格或其他程序。

只有预览失败时，保持无世界效果并允许恢复原读取或重取当前预览。若原社会动作已经提交而结果未知，则沿该入口原请求/动作身份查询结果与后续维护事件；不能通过重新预览推定“未提交”，不能自动换 ID 重发、重复锁 stake、结算或补造事件。预览服务不建立第二套裁决、世界恢复或历史 journal 重写程序。

#### 1.1.7 有限读取与能力缺口

五动作的候选输入、证据、事实、backing、关联边、消费者与响应必须在配套设计的有限预算内读取和解释。上限与技术计量只在[设计预算](social-fact-ledger-declarative-reputation.design.md#social-preview-budget)维护，不改变原世界允许的事实数量、玩法期限或资源值。一个隐藏的大列表、整份 seed model 或完整 journal 不是免费依赖。

必需闭包缺证、超限或读取中断时，可展示已获准且已核实的部分，但必须标明其范围及不能判断的后果；不能截断后仍称完整预览、把未读项当零影响，或在内部重开读取以绕过一次工作上限。无权与预算不足分别表达，错误消息不泄露缺口背后的私有内容。再次读取不自动提交动作，也不消费旧请求或别人的资源预留。

当前局部 quote、认证请求或测试定义不证明本合同已实现。固定源码基线 `8035c556ade7eb989491d37e74f890701509bd40` 中，Publish/Declare 的固定 positive、schema 关键词表面和 runtime-live 的 seed 投影仍不足以证明上述语义、真实消费者与完整来源；五动作正式入口须分别核实。原 Challenge 扣款前置、Adjudicate lifecycle guard 等差距仍由 [SOCIAL-AC-1–5](#social-lifecycle-contract)承接，文档不能把它们记作已修复。本切片不完成通用声誉更新/申诉、其他组织岗位、R&D 或成熟世界机会，CR-003 整体仍部分覆盖。

#### 1.1.8 预览专业验收

下列七项消费同一原产品要求；验证方法存在不表示已经执行。每项均须固定候选、原世界与源读点、受众、规则/adapter 版本和所查范围，以独立原件判断预期，不以待测 quote 的文案自证。

<a id="social-preview-ac-1"></a>
#### SOCIAL-PREVIEW-AC-1：候选绑定与真实来源

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

完整候选动作、typed 主体、来源读点、版本和覆盖可追溯；原事件/事实/关系来自真实受支持来源。输入或来源换版使旧预览不能代表新动作；seed、异世界原件、无关事件和仅有事件编号不能代签所需来源关系。设计：[来源](social-fact-ledger-declarative-reputation.design.md#social-preview-source)；方法：[SOCIAL-PREVIEW-01](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-01)。

<a id="social-preview-ac-2"></a>
#### SOCIAL-PREVIEW-AC-2：声明含义与情境边界

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

引用存在、内容支持、原生命周期和消费者认可分别表达；confidence 不是真实概率，原发布者 Confirm 不是独立 panel，第三方关系/负 weight 不代表双方同意或正向机会，无 TTL 不代表永久 blacklist。期限与历史不因预览重置。设计：[含义](social-fact-ledger-declarative-reputation.design.md#social-preview-meaning)；方法：[SOCIAL-PREVIEW-02](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-02)。

<a id="social-preview-ac-3"></a>
#### SOCIAL-PREVIEW-AC-3：五动作、维护与逐资源风险

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

五动作的条件直接后果覆盖原动作及其维护切点，原 stake 逐主体、单位、数量和去向解释；有关系与无关系、有效与已失效边分别判断。Challenge 后失效的旧边不因后续 Confirm 复活，混合 Data/Electricity 样例可独立对账；不改原仲裁、结算或 ABI。设计：[动作效果](social-fact-ledger-declarative-reputation.design.md#social-preview-effects)；方法：[SOCIAL-PREVIEW-03](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-03)。

<a id="social-preview-ac-4"></a>
#### SOCIAL-PREVIEW-AC-4：当前受众与权限

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

当前读取、原件读取和动作资格分别核验，原作者、所指主体与当前控制者不混同；后任不继承个人 credential 或前任私有证据。受限来源/消费者不通过错误、计数或建议泄露，不被当作不存在。设计：[授权](social-fact-ledger-declarative-reputation.design.md#social-preview-access)；方法：[SOCIAL-PREVIEW-04](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-04)。

<a id="social-preview-ac-5"></a>
#### SOCIAL-PREVIEW-AC-5：真实消费者、入口与玩家判断

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

影响绑定实际消费者的用途、范围与版本；已核实未接入、未知、受限和直接后果明确分开。各实际入口同义，缺入口不由其他 handler 或模型代签。独立玩家样例须能选择继续、修改、查证或暂缓，并解释该选择的来源与风险；固定 positive、schema 命名、拥有 Agent 或原社会状态均不自动授予资格、治理/claim、合作或机会。设计：[消费者](social-fact-ledger-declarative-reputation.design.md#social-preview-consumers)；方法：[SOCIAL-PREVIEW-05](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-05)。

<a id="social-preview-ac-6"></a>
#### SOCIAL-PREVIEW-AC-6：只读、漂移与原提交恢复

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

重复预览和读取失败保持世界零效果；来源、权限、期限或消费者漂移时旧预览不冒充当前。原动作已提交且结果未知时按原身份恢复，不通过重报价判失败、换 ID 重发或补写事件；原维护事件和既有请求幂等边界分别核实。设计：[恢复](social-fact-ledger-declarative-reputation.design.md#social-preview-recovery)；方法：[SOCIAL-PREVIEW-06](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-06)。

<a id="social-preview-ac-7"></a>
#### SOCIAL-PREVIEW-AC-7：有界读取与缺证表现

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；声明：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

实际依赖、证明、关联边与消费者全部计入设计限额；完整闭包不足时保留明确缺口，不以全量 clone、隐藏 bundle、截断或内部重读伪造有限完整预览。以正常可行输入及各限额边界验证实际读取/响应和无世界效果，方法不代签生产容量。设计：[预算](social-fact-ledger-declarative-reputation.design.md#social-preview-budget)；方法：[SOCIAL-PREVIEW-07](../../testing/manual/social-action-consequence-preview-acceptance.manual.md#social-preview-07)。

### 2) 事件扩展（simulator）
- `SocialFactPublished`
- `SocialFactChallenged`
- `SocialFactAdjudicated`
- `SocialFactRevoked`
- `SocialFactExpired`
- `SocialEdgeDeclared`
- `SocialEdgeExpired`

### 3) 世界状态扩展（simulator `WorldModel`）
- `next_social_fact_id: u64`
- `social_facts: BTreeMap<u64, SocialFactState>`
- `social_edges: BTreeMap<u64, SocialEdgeState>`
- `next_social_edge_id: u64`

### 4) 关键结构
- `SocialStake`
  - `kind: ResourceKind`
  - `amount: i64`
- `SocialFactLifecycleState`
  - `Active` / `Challenged` / `Confirmed` / `Retracted` / `Revoked` / `Expired`
- `SocialChallengeState`
  - `challenger` / `reason` / `stake` / `challenged_at`
- `SocialAdjudicationDecision`
  - `Confirm` / `Retract`
- `SocialFactState`
  - 事实核心字段 + 生命周期字段 + 可选挑战信息 + 时间戳
- `SocialEdgeState`
  - 声明核心字段 + 生命周期字段

## 规则与约束
- `schema_id` / `claim` / `relation_kind` 必须非空。
- `confidence_ppm` 必须在闭区间 `[1, 1_000_000]`。
- `evidence_event_ids` 必须非空，且所有事件 ID 均已存在。
- `backing_fact_ids` 必须全部存在，且仅允许引用 `Active/Confirmed` 事实；不可引用 `Challenged/Retracted/Revoked/Expired` 事实。
- 仲裁权限：v1 当前可表示的 `ResourceOwner` 中仅事实发布者可仲裁；`ResourceOwner::World` 保留为设计目标，但尚未在 simulator owner ABI 中表示。
- 撤销权限：仅事实发布者可撤销。
- 过期规则：带 `ttl_ticks` 的事实/关系在到期 tick 自动标记 `Expired`。
- 质押规则：设置 stake 的动作必须先完成全部无副作用资格检查，再扣减并提交对应事实/挑战记录；拒绝请求不得部分扣资源。
  - 仲裁 `Confirm`：质疑者质押记入系统池，发布者剩余质押按已有结算规则归还。
  - 仲裁 `Retract`：发布者质押记入系统池，质疑者剩余质押按已有结算规则归还。

<a id="social-lifecycle-contract"></a>
### 生命周期与拒绝不变性

本节限定 Simulator 已有社会动作及游戏资源，不增加社会玩法、仲裁权限、真实货币或 runtime-live 的可达性承诺。
设计与验证见 [状态机与资源提交](social-fact-ledger-declarative-reputation.design.md#social-lifecycle-design)。

| 当前事实 | Challenge | Adjudicate（Confirm/Retract） |
| --- | --- | --- |
| 不存在 | 拒绝，资源不变 | 拒绝，资源不变 |
| Active 或 Confirmed，且无历史 challenge | 全部资格/资源检查通过后，扣一次 stake 并转 Challenged | 拒绝；不能以无 challenge 事实开始裁决 |
| Challenged | 拒绝重复挑战，资源不变 | 发布者且其余检查通过时转 Confirmed/Retracted，并仅结算一次 |
| Confirmed 且保留历史 challenge | 拒绝重复挑战 | 拒绝新的重复裁决，不新增成功裁决事件或再次结算 |
| Retracted / Revoked / Expired | 拒绝，资源不变 | 拒绝，即使保留历史 challenge 也不能复活 |

<a id="social-ac-1"></a>
#### SOCIAL-AC-1 局部验收

- SOCIAL-AC-1：quote/apply 共享当前资格和生命周期规则；quote 始终只读且不预留，apply 在执行时重新校验。旧 quote 或历史 challenge 不产生当前裁决资格。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；局部承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)及[产品验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)的只读预览与 lifecycle 支撑边界；专业实现与验证见 [SOCIAL-AC-1 设计](social-fact-ledger-declarative-reputation.design.md#social-obligation-1)。不覆盖 R&D charter、份额/credential 不转移、机会组合或整个产品验收。
<a id="social-ac-2"></a>
#### SOCIAL-AC-2 局部验收

- SOCIAL-AC-2：Challenge 在扣 stake 前完成 owner、reason、stake、事实存在、生命周期、已有 challenge 及资源条件检查；普通拒绝后库存、stake pool、事实和关系边均不变。正常时间/拒绝事件 ID 推进独立断言，不要求整个世界逐字不变。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；局部承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)及[产品验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)的只读预览与 lifecycle 支撑边界；专业实现与验证见 [SOCIAL-AC-2 设计](social-fact-ledger-declarative-reputation.design.md#social-obligation-2)。不覆盖 R&D charter、份额/credential 不转移、机会组合或整个产品验收。
<a id="social-ac-3"></a>
#### SOCIAL-AC-3 局部验收

- SOCIAL-AC-3：Publish→Challenge→Revoke→Confirm 的最后一步拒绝，事实仍 Revoked；以其新建 backing 边也拒绝。Retract→Confirm、Challenged→Expired→裁决和重复裁决分别覆盖；保留 challenge 审计信息，不用清空历史掩盖状态机问题。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；局部承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)及[产品验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)的只读预览与 lifecycle 支撑边界；专业实现与验证见 [SOCIAL-AC-3 设计](social-fact-ledger-declarative-reputation.design.md#social-obligation-3)。不覆盖 R&D charter、份额/credential 不转移、机会组合或整个产品验收。
<a id="social-ac-4"></a>
#### SOCIAL-AC-4 局部验收

- SOCIAL-AC-4：合法首次挑战只扣一次并记录同额 stake，正常裁决、撤销、过期及结算保持；新动作的重复裁决拒绝不改变上层已有请求幂等返回原结果的合同。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；局部承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)及[产品验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)的只读预览与 lifecycle 支撑边界；专业实现与验证见 [SOCIAL-AC-4 设计](social-fact-ledger-declarative-reputation.design.md#social-obligation-4)。不覆盖 R&D charter、份额/credential 不转移、机会组合或整个产品验收。
<a id="social-ac-5"></a>
#### SOCIAL-AC-5 局部验收

- SOCIAL-AC-5：新拒绝序列的动作前 snapshot+journal replay 与 live 资源/社会状态一致。收紧新动作资格不无说明改变历史事件重放；旧记录版本、兼容读取和不可恢复的历史缺证需明确处置，不能伪造事件补余额。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；主责：Simulator social 维护者 / qa_engineer；局部承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)及[产品验收](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)的只读预览与 lifecycle 支撑边界；专业实现与验证见 [SOCIAL-AC-5 设计](social-fact-ledger-declarative-reputation.design.md#social-obligation-5)。不覆盖 R&D charter、份额/credential 不转移、机会组合或整个产品验收。

未开放社会动作的首局候选不因本节新增社会玩法前置；开放了该能力的候选不能以“不在工业主路径”为由把缺陷记为通过。
文档修订只固定目标语义，行为修复与各场景结果仍须由实际候选证明。

## 可重放与一致性
- 所有状态变更仅由事件驱动。
- replay 仅依赖事件顺序，不读取外部时间或随机数。
- 过期事件由 `step` 的确定性逻辑触发，重放时保持一致结果。

## 5. Risks & Roadmap
- M1：文档与任务拆解完成。
- M2：数据模型与动作/事件类型落地。
- M3：内核执行逻辑 + 回放逻辑落地。
- M4：测试与回归（required/full）完成。
- M5：文档与 devlog 收口。

### Technical Risks
- 语义风险：无固定评分会提升上层使用复杂度；通过示例 schema 文档缓解。
- 安全风险：证据字段若校验不足会引入垃圾事实；通过强校验与质押约束缓解。
- 治理风险：仲裁权限配置不当可能集中化；后续可引入多签/委员会策略。
- 体量风险：新增事件与状态增多，需持续关注单文件长度约束。

## 6. Validation & Decision Record
- 追溯: GitHub task issue evidence 与 Git history 保留原实施过程；本文与 design 保持现行约束语义。
