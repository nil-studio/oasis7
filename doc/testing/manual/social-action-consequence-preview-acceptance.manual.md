# 社会动作后果预览验收手册

- 生命周期：`active`；Owner role：`qa_engineer`；协作：Simulator social 维护者、`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`；Last reviewed：2026-11-04。
- 上游：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)、[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；本次仅承接[社会动作预览专业合同](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-1)的七项局部验收。
- 技术依据：[来源与后果设计](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-source)。本手册提供可执行的方法与独立判据，不表示新预览、正式消费者或玩家验证已经通过。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。原 [SOCIAL-AC-1–5](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-lifecycle-contract)继续拥有资格、拒绝不变性、终态、一次结算与历史 replay；本手册不另立生命周期、仲裁、质押或机会规则。

## 1. 执行前置与真实入口

每次运行固定 source HEAD、tested tree/build、world/branch、来源类型与版本、原事件/事实/关系身份、精确读点、完整候选输入及其摘要、当前读者和动作主体的授权。保存调用方式、输入输出、原件定位及独立 oracle，逐项记 `pass/fail/blocked/not_run`。缺正式入口记 `blocked`；有入口而本次没有执行记 `not_run`；静态审读、受控 fixture、真实源生成、正式消费者及玩家理解分别报告。

固定审读基线为 `8035c556ade7eb989491d37e74f890701509bd40`。以下是已定位源码和方法边界，不是当前运行通过声明：

| 层次与实际入口 | 可复用来源 | 仍须取得的证据 |
| --- | --- | --- |
| Simulator kernel 五种预览和动作 | [`kernel/social.rs`](../../../crates/oasis7_simulator/src/kernel/social.rs) 的 `quote_publish_social_fact`、`quote_challenge_social_fact`、`quote_adjudicate_social_fact`、`quote_revoke_social_fact`、`quote_declare_social_edge`；[`types.rs`](../../../crates/oasis7_simulator/src/types.rs) 的五个 `Action`；[`step.rs`](../../../crates/oasis7_simulator/src/kernel/step.rs) 的公开提交与 step | 隔离 kernel 可检验语义与状态差分；直接调用 kernel 不证明玩家鉴权、正式世界持久结果或跨入口可达 |
| Viewer 协议预览 | [`ViewerRequest`](../../../crates/oasis7_proto/src/viewer.rs)、[`social_quote.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/social_quote.rs) 的 Publish / Adjudicate / Revoke / Declare 四条已签名请求；[`viewer_auth`](../../../crates/oasis7_client_api/src/viewer_auth.rs) 的对应签名辅助函数 | 当前无 `QuoteChallengeSocialFact` 路由；Revoke 另需协商能力。handler、协议和签名存在不证明浏览器控件、真实 source adapter 或对应写动作已贯通 |
| Runtime→Simulator 投影 | [`mapping.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/mapping.rs) 从 `seed_model.cloned()` 开始；Publish handler 将 Runtime journal 枚举为 Simulator ID | seed 内 social facts/edges 不能冒充当前 Runtime 社会源；序号重映射必须能回到同 world/branch、同 cut 的原事件身份和内容，不能用整数相同证明同源 |
| Agent / pure API | [`oasis7_agent_api::Action`](../../../crates/oasis7_agent_api/src/world_action/action.rs) 与现有正式调用方 | 本基线未找到这五种社会动作对应的 Agent API 枚举或完整同义预览链路。新增接线须给真实 endpoint/handler/授权/source 证据；不能把 Simulator Action、Viewer DTO 或 `WorldServiceIntent` 名称当成可调用的社会 API |
| 既有局部回归 | [`social.rs`](../../../crates/oasis7_simulator/src/tests/social.rs)、[`social_revoke_quote.rs`](../../../crates/oasis7_simulator/src/tests/social_revoke_quote.rs)、[`social_persist.rs`](../../../crates/oasis7_simulator/src/tests/social_persist.rs)、Viewer [`social_quote.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/social_quote.rs) 与 [`social_quote_capability.rs`](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/social_quote_capability.rs) | 旧测试含固定 `positive` 预期、取首个 journal ID 作为任意 claim 证据，以及直接插入 seed social 状态；它们只定位现有分支，不能作为新增后果真值的独立 oracle |

当前 `apply_challenge_social_fact` 先扣 stake 后核事实资格、Adjudicate 缺当前 lifecycle guard 的旧差距仍按原 SOCIAL-AC 判断。新预览不得把这些旧缺陷包装成合法行为；相关执行前置未满足时报告阻塞/失败，不改写旧验收以取得通过。

在仓库根可运行以下只读定位；输出只证明定义和分支存在，不代签业务：

```bash
rg -n 'pub fn quote_|cooperation_opportunity_delta: "positive"|fn social_affected_surfaces|fn social_adjudicator_is_authorized' crates/oasis7_simulator/src/kernel/social.rs
rg -n 'QuoteDeclareSocialEdge|QuotePublishSocialFact|QuoteAdjudicateSocialFact|QuoteRevokeSocialFact|QuoteChallengeSocialFact' crates/oasis7_proto/src/viewer.rs
rg -n 'seed_model.cloned|social_facts|social_edges' crates/oasis7_viewer_runtime/src/viewer/runtime_live/mapping.rs
rg -n 'simulator_event_ids|enumerate|mapped.id|quote_publish_social_fact' crates/oasis7_viewer_runtime/src/viewer/runtime_live/social_quote.rs
```

## 2. 共同样例与独立对照

在隔离世界 W 用公开 `RegisterLocation`、`RegisterAgent` 建立 A、B、C；从实际 `AgentRegistered` 内容选取 B 的事件 E，记录原 ID、主体、地点及读点，不能固定取 journal 第一项。F 的 claim 只陈述 E 能直接支持的“B 在该地点完成登记”，不由登记推断交付、合作质量或治理资格。A 为发布者，B 为 subject，C 为 object 与后续 challenger；原件与输入均保留 typed `ResourceOwner`。

质押样例为 A 的 `Electricity:30`、C 的 `Data:20`。这只是测试数据，不设新经济数值。初始库存须有可核来源；若复用 [`seed_owner_resource`](../../../crates/oasis7_simulator/src/tests/mod.rs) 修改 snapshot，仅标为 kernel 资源 fixture，再沿公开社会动作生成事实/挑战/边，不把该初始补库称为正式发放。正式世界样例另须真实库存来源和当前签名授权；不可用时保持阻塞。

| 顺序 | 真实候选与源动作 | 不依赖预览文案的预期 |
| --- | --- | --- |
| 1 | A 预览并显式提交 `PublishSocialFact`，引用 E，stake 为 Electricity 30，主序列不设 TTL | 预览无效果；实际成功后产生 F/Active，A 的该资源减少 30、F 记录同额 stake，其他主体/资源不变；内容是 A 的声明 |
| 2 | C 预览并显式 `ChallengeSocialFact(F)`，说明需复核登记与主体的对应，stake 为 Data 20 | 预览不挑战也不锁资源；成功后 F/Challenged，C 的 Data 减少 20，记录同额 challenge stake |
| 3 | A 预览 `Confirm` 与 `Retract` 两分支，只显式执行 `AdjudicateSocialFact(F, Confirm)` | 两预览均无结算；Confirm 的实际结果为 C/Data 20 入同资源池，A/Electricity 30 返还，F/Confirmed；A 是发布者裁决者，不是独立 panel |
| 4 | A 预览并显式 `DeclareSocialEdge`，from=A、to=B、backing=[F]、weight=-2500 | 真实新边 R 记录原负权重与作者；未采用该边的合作/治理/claim 表面不得被标为确定正收益，也不能仅因负数推定所有表面受损 |
| 5 | A 预览并显式 `RevokeSocialFact(F)` | 预览区分 R 的当前 backing 影响与已结清 stake；实际成功后 F/Revoked，维护事件使 R/Expired，已结清 stake 不再返还；历史原件保留 |

同一 kernel 的每次 quote 前后比较 `kernel_snapshot()`、`kernel_journal_snapshot()`、pending actions、事实/关系、库存和分资源 stake pool；不得产生 ID、时间、动作队列或游戏状态变化。正式服务的 nonce、限流和诊断可按其合同变化，独立比较世界业务量，不把正常鉴权元数据误判成世界效果。

执行效果由原动作事件、当前 ledger 与前后分账共同核对，不能将被测 quote 的字符串抄为预期。五动作、维护和后续 source 更新分别记时；`step()` 会进行 lifecycle maintenance，只看返回的主事件可能漏掉随后记录的边过期。恢复时以原 snapshot/journal 对照相同社会状态，旧 replay 兼容仍由 SOCIAL-AC-5 单独证明。

<a id="social-preview-01"></a>
## SOCIAL-PREVIEW-01：候选输入、真实来源与完整覆盖

承接 [SOCIAL-PREVIEW-AC-1](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-1)；依据 [social-preview-source](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-source)。

1. 对共同序列每一步分别保留完整候选、source 类型/版本、world/branch、精确读点、原事件/事实/边身份和当前状态。核对 actor、subject/object、schema、claim/reason/notes、decision、confidence、stake、TTL、weight、backing/evidence 引用；任何预览输出必须能回到该候选及其源，摘要不能替代完整输入绑定。V2 重算 `candidate_digest` 并拒绝不符；响应/缓存按 reader、`preview_request_id`、`draft_revision` 和摘要整体匹配。同 opaque ID 的另一候选不能命中旧结果，但不要求跨重启保存全部读 ID。重复 JSON key、未知动作和数字溢出按输入合同拒绝，不能静默选值继续解释。
2. 复制一个真实 ID 到另一 world/branch；再把 Runtime 原 ID 与枚举后的 Simulator ID 对调。原事件不存在、身份/内容错配或无法还原映射时，只能拒绝/报告来源不足，不能因 `id < next_event_id`、列表长度相等或整数碰巧相等认定合法原件。
3. 在测试 adapter 中分别使用旧 seed F/Active 和当前真实 F/Revoked、缺少 F、混合两个 cut 的 F 与 R、仅有 ACK 的请求。必须识别 stale/缺源/不一致；把当前余额或 seed 状态重新序列化不构成来源修复。该注入验证 adapter 分支，另需正式 source 生成与读取的证据。
4. 依次给出：同范围完整覆盖且无匹配记录；无读权；源未启用；缺历史原件；原 source 仍有未读页；来源版本不支持。完整空集只用于第一个条件，其余须保留实际限制；不全显示“零影响”。恢复缺件后重读应只恢复原信息，不补写事实、关系或结算。

保存每字段到原件的定位、输入摘要与 source cut、覆盖声明及负例原因。页数、引用 ID 和响应成功不能独立证明证据完整；同源重复引用不增加证据强度或独立影响数量。

<a id="social-preview-02"></a>
## SOCIAL-PREVIEW-02：声明、引用、状态与真实含义

承接 [SOCIAL-PREVIEW-AC-2](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-2)；依据 [social-preview-meaning](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-meaning)。

| 独立构造 | 判据 |
| --- | --- |
| E 是 B 的登记事件，claim 改为“B 已完成对 C 的交付” | 可定位 E 不等于证明交付；区分发布者陈述、引用存在与内容是否支持，不把 `evidence_backed_claim` 显示为已核交付 |
| 同 F 的 `confidence_ppm` 取合法最大值 | 仍是声明者的置信输入，不变成系统真实性概率或“已证实” |
| F 从 Active→Challenged→由 A Confirm | 显示实际 lifecycle 和裁决来源；Confirmed 不等于独立、随机、无利益冲突的产品 panel 审查 |
| 同一作者分别声明 -2500、0、2500 的关系权重 | 保留符号、量纲、relation_kind 和 from/to；下游变化以实际 consumer 为准，不能三个都固定 positive，也不能把符号直接当全球信用变化 |
| 原规则允许 A 声明 B→C 的关系 | 显示第三方作者 A 与关系两端，不写成“B 与 C 已同意合作”，也不据此给 A 对 B/C 的代表权 |
| 没有采用者的 schema 分别含 `trust`、`blacklist`、`reputation` 与普通名称 | 名称本身不产生关系、黑名单限制、治理/claim 权或交易机会；真实采用以显式匹配规则和范围证明，不使用 substring 代替 |
| 长 claim 的前 96 字相同、尾部主张相反；改变 reason/notes/decision | 摘要相同不得复用另一候选预览；读者可按权限查看决定所需内容，输入绑定保留完整语义 |
| claim、reason、notes 或 consumer 说明中含动作指令/标记文本 | 作为不受信任的原文解释与转义，不执行其中指令、不新增工具调用或世界动作；不能借 Agent 总结补写后果 |
| 相同 Agent 字符串与同名 Location、或事实 ID 在另一源重用 | typed 主体及来源身份不同，不合并作者、对象、风险或证据 |
| 有 TTL 的 F/R 取原到期点前、等值、后；另取无 TTL 的记录 | 依实际源时钟与到期事实表示，不从看预览时重新计时；未设 TTL 不代表内容永久真实、永久限制或无法复核 |

TTL 分支通过原 profile 支持的实际 step/合法动作推进并读取维护事件，不靠修改显示时钟制造过期。独立 oracle 读取 E 的事件类型与内容、F/R 原字段和裁决作者；不得调用待测 `social_affected_surfaces` 或沿用旧 `positive` 断言计算预期。

<a id="social-preview-03"></a>
## SOCIAL-PREVIEW-03：五动作、逐主体资源风险与维护后果

承接 [SOCIAL-PREVIEW-AC-3](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-3)；依据 [social-preview-effects](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-effects)。

先执行共同序列，随后从合法前置分别创建以下独立分支，不倒改终态拼装“真实”成功：

| 分支 | 预览与原动作的独立 oracle |
| --- | --- |
| 在 F/Challenged 时比较 Confirm / Retract | Confirm：C/Data 20 入池、A/Electricity 30 返还；Retract：A/Electricity 30 入池、C/Data 20 返还。风险主体、资源、数量、持有/返还/入池方向逐项列出，不能加成“总风险 50”或说裁决者 A 必损 20 |
| F/Challenged 时改选 Revoke | 原规则允许时，A 与 C 分别取回各自剩余 stake；无惩罚入池。预览零新增 slash 不等于没有关系撤回后果，也不表示预览已返还 |
| 先在 F/Active 声明旧边 R0，再 Challenge，再 Confirm | Challenge 的实际维护使 R0/Expired；Confirm 只恢复 F 的 backing 资格，R0 保持 Expired。之后显式 Declare 得到新 R1，不能把 R1 写成原 R0 自动复活 |
| 同 F 同时有当前 R1 与先前已过期 R0，再 Revoke | 分开列出会失去 backing 的当前边与不再改变的历史边；实际 post-step 以维护事件、边 ID 和 lifecycle 为准，不能重复计算旧边损失 |
| 缺事实、非法 owner、终态、重复挑战/裁决、资源不足或返还溢出 | 按原 SOCIAL-AC 拒绝/原子性要求检查；旧实现若违反原合同则记失败，不能通过预览隐藏风险或宣称已被本次设计修复 |

资源对照按 `(owner kind/id, ResourceKind)` 分账，另核 F/挑战记录中持有的 stake 与 pool；余额相等不能证明来源或没有重复结算。无 stake、部分 stake 已结清和两方同 owner/同 kind 的既有边界分别复用原结算规则，不从 `stake_at_risk: i64` 推造币值或统一金额。

五种预览各自都要做只读差分；动作资格和确定维护后果来自原 source，合作/治理/claim 等下游效果另按 SOCIAL-PREVIEW-05。不能因实际 Publish/Confirm 成功，就将任何外部表面变化推定为成功。

<a id="social-preview-04"></a>
## SOCIAL-PREVIEW-04：当前读权、动作资格与历史作者

承接 [SOCIAL-PREVIEW-AC-4](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-4)；依据 [social-preview-access](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-access)。

在真实已鉴权入口分别使用 A 的当前合法 session、过期/撤销 session、无绑定读者、仅有可见历史的读者、B/C 及无关 U。输入签名后逐项改动 fact/schema/subject/object/claim/decision/weight/stake/backing/TTL，核对签名绑定和拒绝；kernel 的 owner 存在检查不能代替玩家身份和当前权限。

1. “能查看 F”“是 F 的 subject”“准备声明 from=A 的边”“是当前控制者”“是历史发布者”分别断言。只有原动作规则授予的 actor 能执行相应动作；预览、推荐或看见 from/to 不授予控制、代签或新的任意主体限制权。
2. 通过原 owner authority 合法更换同一 Agent ID 的控制来源，或撤销旧会话后重读；历史 F 的作者保持其源可证明的 typed owner 与身份范围，不用当前显示姓名重写。源未保留历史自然主体时明确无法判定，不能把 Agent ID 相同解释为新 owner 继承前任信誉/credential、责任或私有证据。动作资格仍由原当前规则决定，不在手册创造新的不可转让动作权限。
3. 对公开摘要、受限正文、缓存命中、原件打开和原 source 支持的后续页分别重验当前读权。隐藏对象与不存在对象对无权 U 不泄露差异；受限数据不能先完整下发再由客户端遮挡，也不能把不可见条数作为零影响。
4. 在 quote 后、返回前以及原 source 翻页前分别撤权或切换主体，核对缓存及 source cursor 的读者、来源、候选和授权版本绑定；此处不提供预览续页。旧 quote 保留其历史属性但不再证明当前可执行；新授权不能补签原动作或自动提交。

独立 oracle 是当前身份/授权记录及原动作权限条款，不采用预览中的 actor 字符串自证。仅构造签名 fixture 可证明参数绑定，合法换主、撤权和正式消费者读面需分别有真实源证据。

<a id="social-preview-05"></a>
## SOCIAL-PREVIEW-05：实际消费者、各入口同义与玩家判别

承接 [SOCIAL-PREVIEW-AC-5](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-5)；依据 [social-preview-consumers](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-consumers)。

对每个声称会受影响的合作、黑名单、治理、claim 或交易协作表面，取得该 consumer 的实际采用声明/版本、适用 schema 与情境、source cut、输入条件、只读判断和完整覆盖。不从 surface 名称或 natural-language 推荐推导效果；没有已采用 consumer 时，正例只能证明内核事实/边的变化。

| 构造与消费者资格 | 必须区分的输出 |
| --- | --- |
| 已采用 consumer、覆盖完整且规则判断确有指定变化 | 仅在其明确 scope/用途/时间内陈述已证条件后果，保留来源和提交时重验；不保证其他主体接受、未来机会或最终成交 |
| 已采用 consumer、完整评估确认本候选无变化 | 明确该 consumer、该范围和该 cut 下无变化，不外推全球零影响 |
| 已完整核明本候选未被任何适用 consumer 采用 | 说明未采用及覆盖范围；可以保留声明/内核后果，不能伪装真实合作增长或治理 relevance |
| 未声明采用关系、不可读、原 source 未完页、缺依赖、预算停止或版本不兼容 | 保持未知/受限与下一步；“尚未核清”不能降格为未采用或无变化 |

逐一移除必需来源、当前权限、stake 风险或已声明 consumer 的覆盖，再读取 `recommended_social_action`：只能保留当前可执行的查证、修改、刷新或暂缓，不能仍默认 `publish_fact`、`confirm_fact` 或其他社会提交。独立核对建议与缺口，不因输出有 `incomplete` 标签就容许正文继续承诺收益；这不新增原世界提交闸门。

受控 consumer stub 可以验证四类状态传递，但不证明真实合作/治理/claim 规则已安装。实际正例须从原 consumer 合同独立算出预期，并沿其正式读面核对；本切片不新建全球评分、定价、资格、奖励、排名、panel 或机会分配协议。

同一 world/source cut、候选与读者范围分别走 kernel 预览、实际 Viewer 协议、实际 Agent/pure API；记录真实路由与响应。依技术合同归一已声明的 action 别名后，对账主体、输入绑定、来源/覆盖、声明与状态、分账风险、维护后果、consumer 状态和可用下一步，不能只比字符串或共享 DTO。缺 Challenge Viewer 路由、缺 Agent API/source adapter 时对应项为 blocked；未来新增路由必须重新取证，不用本地直调补签。

协议分支分别提供完整 V2、旧 `SocialFactImpactQuote`、缺必要来源/分账/completeness 字段及未声明的状态。新客户端不能把旧字段补零后认作 V2，也不能继续显示旧 positive 收益；未协商该 route/profile 能力时给未支持/待升级，不伪装网络重试。只读 route 不显示已提交；回滚停用 V2 后保留草稿与原动作回查，不恢复错误收益兜底。上述适配 fixture 与正式能力协商分别取证。

玩家理解先用清楚标注为静态示例的候选卡进行独立读述，再在接通候选的真实界面/Agent 文本重复；两者结果分别记录，不把读稿当实玩。由未参与文案编写的人看到输入与预览，回答“谁声明了什么、哪项真的有证据、谁的什么资源可能去哪、哪些后果未知、此时继续/暂缓/补证/重读各意味着什么”。以下是期望含义，可由产品选择等义文案：

- “这条引用是 B 的登记记录；它没有证明交付完成。现在只是在预览，尚未发布。”
- “你准备声明 -2500 的关系权重。当前没有核实合作系统采用它，因此无法承诺增加或减少合作机会。”
- “若发布者 A 按现有规则确认，C 的 20 Data 将入池，A 的 30 Electricity 将返还；这不是独立 panel 的判决。”
- “部分来源不可读/尚未读完，不能据此认定没有影响。可以暂缓，或在权限和来源允许后重新查看。”
- “旧预览之后权限或事实状态已变；先核原请求与当前状态，不能把旧推荐当作提交许可。”

若读者误认已发布、已扣/返 stake、已验证 claim、已取得合作/治理/claim 权或可凭旧 quote 重试，记录对应文案/字段和原因，修订后重验。允许玩家合理选择暂缓，不用“选择执行”作为理解正确的标准。

<a id="social-preview-06"></a>
## SOCIAL-PREVIEW-06：只读、来源漂移与原请求恢复

承接 [SOCIAL-PREVIEW-AC-6](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-6)；依据 [social-preview-recovery](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-recovery)。

在候选接入的受控环境，分别于读源后、权限复核前后、返回前、真正动作提交后响应丢失、服务重启恢复时注入故障。每次固定原候选、原 query/动作请求身份与预览 source cut；quote 与后续动作的身份分开，不能把 quote ID 当世界 receipt。

| 条件 | 恢复 oracle |
| --- | --- |
| 只读 quote 超时、取消或响应丢失 | 世界无效果；在当前授权下显式恢复/重读该查询，返回同 cut 结果或明确失效并形成新只读判断，不在内部无限重试，不能自动提交任一社会动作 |
| 编辑草稿产生新 revision/query，旧异步响应迟到 | 旧响应按原请求/摘要标为过时，不能覆盖新草稿或新 source 判断；当前权限未知时撤去相关敏感详情，重新获得读权后才重取 |
| quote 后 F 被质疑/撤销/过期、backing 失效、库存/权限/consumer 版本变化 | 旧 quote 标为陈旧，需要当前判断时重读；执行端仅按当前原规则重验，不满足时拒绝/明确失败，仍合法时保留原可执行性与实际维护/分账结果。来源漂移本身不新增 World gate，也不隐式改 decision/stake/对象 |
| 真正动作已提交但结果未知 | 先沿原 authority 查询同一原请求；保留已确认的 receipt/事件、库存/stake 与事实/边，未知期间不换 ID 重发、不再次扣款或重新声明 |
| 原 authority 证明同请求已经成功 | 返回原结果；只读重新预览不会生成第二个事实/边或结算。新业务动作与查询原结果分别取证，不把 kernel 再次 `submit_action` 当幂等恢复 |
| 旧 cut 找不到请求、缓存丢失或历史 source 不可取 | 只说明目前查不到/缺证，不能据此证明从未生效；按原 authority 的 durable generation 与恢复规定核清，不伪造事件/补余额/倒退事实 |
| 恢复期间原读者失权或 Agent 换主 | 只披露当前可读的原状态；不能为恢复泄露前任私有证据，也不撤销原合法结果 |
| World generation 进入未知 | 保持原 [G1](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design) 闸门；历史受限读不叫当前确认，预览不能解除 writer fence |

漂移正例从 A 已预览 Revoke 的 Active F 开始：C 随后合法 Challenge，原 Revoke 对 Challenged F 仍可能合法。旧预览不能保证当前风险，重新读取应解释双方剩余 stake；若 A 显式按原入口提交且全部当前条件满足，执行应成功返还，而非仅因旧 quote 的 source cut 不同被新闸门拒绝。该例与撤权、终态或返还容量不足的拒绝负例分开。

复核每种故障前后业务对照，并保存原请求→原 authority 结果的定位。原世界动作结果查询走原恢复通道，不排在新 quote 之后或等待预览容量。没有实现可查询的正式原请求结果时，本项正式动作恢复为 blocked；snapshot replay 只能证明其局部状态还原，不能证明跨入口请求已唯一生效。

<a id="social-preview-07"></a>
## SOCIAL-PREVIEW-07：完整输入与有界读取

承接 [SOCIAL-PREVIEW-AC-7](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#social-preview-ac-7)；依据唯一 [social-preview-budget](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-preview-budget) 预算和停止合同；具体数值不在手册复制。

实现候选须暴露可核计数或受控限额 fixture，对解压后请求字节、JSON 深度、UTF-8 文本、evidence/backing 引用、单对象/总解码、依赖/关系/consumer 数、索引/证明节点、读次数/单次返回/最终响应与错误体、计量工作内存、并发工作及截止时间逐项取 `limit-1 / limit / limit+1`，并测组合边界。多字节文本和 JSON 转义按真实计量，截止边界可用受控时钟验证。保存实际消耗、停止点、原因和只读业务差分，不把超限输入截断成另一合法候选；预览工程上限不修改原动作合法输入上限。

先按设计的代表性 Revoke 来源束逐项展开事件、事实、关系、分账、采用/覆盖、授权与 cut/proof，独立计算对象、字节、读次数和缓冲峰值。前置完整且预算内的合法样例必须能取得完整预览，不能用“一律 incomplete”满足所有负例；该计算是工程可行性核对，运行时内存与截止时间仍需实际观测。

1. 用接近文本/引用上限的候选叠加单个大对象、完整 backing 依赖和多个 consumer；完整解析必需数据后才可给确定结论。输入小不等于闭包小，索引、身份/授权、来源映射与隐藏内部 proof 也进入同次预算。
2. 一条影响的必需闭包超过上限、多个影响合计越界或 source 未能完整返回时，整份关键预览为 incomplete。允许展示的已核内容必须带覆盖限制，不能跳过未知项后给确定建议或完整合计；未完成范围不能统计为零。
3. 重复同一引用、循环依赖、内部重试、重开 source cursor、先取少量再静默补全以及多入口并发不得绕过同次工作上限。本 profile 不分批拼齐一份完整预览，也不把一个未核完整的候选/单项效果拆页后伪作已验证；原 source 有页不意味着预览新增续页承诺。
4. 对当前 reader 与进程工作容量分别测满额，再请求新预览与原世界结果回查：预览有界拒绝且不留后台队列，原结果仍走原恢复通道。注入取消/超时后底层 I/O 尚未终止及慢客户端尚未收完响应：先停止新读和派生工作，仍持有的槽、来源/响应缓冲不能提前复用；实际终止后释放，新的合法读取可进入。不丢失原 authority 的未知提交身份，也不以世界资产 hold、费用或机会占位补偿服务内存。
5. 原件恢复可用、当前权限重新成立且预算内时重新查询，恢复正确的 source/覆盖/后果；不续 TTL、不刷新原结果时间、不自动执行社会动作。若需要全量 clone/扫描 journal 才能回答，须计入实际工作并报告该候选超限，不能将其藏在预处理后宣称有界。

边界测试是新设计的实现验收目标。当前静态源码、旧局部测试或文档链接通过均不证明预算、正式来源、跨入口和玩家理解成立；最终只报告本次实际执行层及其未覆盖项。
