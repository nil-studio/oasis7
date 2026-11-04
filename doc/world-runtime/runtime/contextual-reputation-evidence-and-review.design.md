# 合同情境声誉的事实投影与客观复核系统设计

- 生命周期：`active`；本文定义目标合同，不表示功能已实现、启用或通过玩家验收。
- Owner role：`runtime_engineer`；协作：`gameplay_designer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`。
- 固定静态审读基线：`nil-studio/oasis7@ad2bf1bb9e29d315ea70b64603b57c206da1be0f`；未运行 Rust、游戏、模型或跨节点测试。
- 产品上游：[CR003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。本 profile 的适用来源、用途、相关性寿命和复核窗口只由 [Gameplay 情境声誉政策](../../game/gameplay/gameplay-contextual-reputation-contract.prd.md#contextual-reputation-policy-v1)维护。
- 验证入口：[情境声誉验收手册](../../testing/manual/contextual-reputation-acceptance.manual.md#rep01-source)。下文类型、命令与拒绝名是逻辑设计，不是现有 Rust、HTTP 或 WASM ABI。

## 1. 问题、目标与非目标

玩家考虑与另一名玩家合作时，需要知道对方在相关地点和同类合同中实际做过什么、结果发生于谁控制 Agent 的期间、哪些责任已确认、哪些只是未解决事实。通用社交声明、一个旧声誉数字或一次成功交换都不能回答这些问题。

目标 profile 为 `contextual_contract_consultation_v1`。它只把已支持个人合同的权威结果整理为合作前的只读事实参考，允许客观更正派生展示。玩家可以查看来源、比较已履行与未定责范围、了解补救、暂缓或继续按原合同流程谈判；查询和复核均不订立新合同、不作责任裁决，也不产生资格、排名、奖励、黑名单或新的资源冻结。

来源限于 `owner_atomic_resource_exchange_v1` 和 `owner_checkpoint_power_service_v1` 的完整有效 receipt 及原来源有权更正。本文不扩大这两个目标 profile 的已完成设计，不把它们标为当前可用；不覆盖组织/岗位声誉、任意社交 schema、战争结果、R&D charter、份额转让、一般争端或完整机会分配。CR003 仍为部分承接。

## 2. 上游约束与相关角色

产品规定主体与情境边界、更新/申诉、非转让和无全局评分；Gameplay 政策决定本咨询 profile 的业务范围与时间规则；原合同决定什么是履约、责任、补救和有效申诉。Runtime 只验证这些事实、派生当前投影、原子提交客观纠正并保留来源；Viewer、API 和 Agent 消费同一投影。P2P 和原执行 authority 继续拥有 canonical 顺序与最终性。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | 合作参考只接受支持的权威结果，事实、声明和未知可区分 | [来源适配](contextual-reputation-evidence-and-review.design.md#reputation-source) | 两个个人合同 profile、原执行 proof 和 Gameplay 来源白名单 | 任意社会事实、native 旧分数及其他合同 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | 玩家责任与 Agent 行动历史分别归属；控制变化不转移个人记录 | [主体与任期](contextual-reputation-evidence-and-review.design.md#reputation-subject) | 原签署者、身份连续性、历史控制来源 | 组织/岗位、人格出售、继受一般 credential |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | 地点、服务、合同、时间和受众约束不能由查询或文案放宽 | [情境与去重](contextual-reputation-evidence-and-review.design.md#reputation-context) | 原条款与合法受众；经验证的关联关系断言 | 跨世界信用、跨服务信任或独立交易次数奖励 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | 当前相关性按原事实到期；更新不删除责任或刷新原期限 | [寿命与更新](contextual-reputation-evidence-and-review.design.md#reputation-validity) | Gameplay 固定政策、canonical tick、源更正链 | 原债权/凭证期限、历史保留和法定意义 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | 提交前只读展示真实影响与不确定性，不承诺资格或合作结果 | [预览与读面](contextual-reputation-evidence-and-review.design.md#reputation-preview) | 同源查询、受众校验和真实 consumer 登记 | 不补齐所有社交动作 ABI/布局或实际 UI |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | 客观投影错误可纠正；责任争点沿原有限申诉，不能重开事实终态 | [有界复核](contextual-reputation-evidence-and-review.design.md#reputation-correction) | 原来源责任/申诉 authority、原政策窗口 | 新法院、发布者自行裁定他人信誉或惩罚 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | 更正、原请求结果和投影版本同根；未知、重试和恢复不复制修正 | [事务与恢复](contextual-reputation-evidence-and-review.design.md#reputation-recovery) | 统一提交、generation 与原件查询 | 不代签持久化、跨节点或线上验收 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | 咨询结果不转换为机会、治理、claim、工业资格或全球分数 | [消费隔离](contextual-reputation-evidence-and-review.design.md#reputation-consumers) | 各资格/机会原 authority | CR003 的完整世界机会组合与 R&D |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | 缺来源/适配能力时保持未支持，旧历史不能被补造为新信誉 | [兼容与启用](contextual-reputation-evidence-and-review.design.md#reputation-compatibility) | 版本化 institution/query adapter、历史 read 能力 | 不迁移旧 score、不改变旧 social lifecycle |

## 3. 当前状态、目标状态与差距

以下是固定基线的静态事实，源码和测试定义不等于运行证据。

| 对象 | 当前事实及可复核来源 | 本设计目标与差距 |
| --- | --- | --- |
| Simulator 社会事实 | [social.rs](../../../crates/oasis7_simulator/src/social.rs) 的事实包含自由 `schema_id/claim`、证据事件 ID、confidence、可选 TTL/stake；[ResourceOwner](../../../crates/oasis7_simulator/src/types.rs) 只有 Agent/Location | 事件存在只证明被引用事件存在，不能证明 claim 的含义；没有本专题的玩家责任、控制任期和合同情境映射 |
| 社会动作预览 | [kernel/social.rs](../../../crates/oasis7_simulator/src/kernel/social.rs) 的 publish/edge quote 使用固定 `positive`，surface 由 schema 文本包含词推导；[Viewer 转接](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/social_quote.rs) 读取并转发 kernel quote | 不是资格变化或真实 consumer 计算；不能把固定文案导入本专题的结果 |
| 原社会生命周期 | [局部设计](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.design.md#social-lifecycle-design) 已明确五项修复；本基线 `apply_challenge_social_fact` 仍先锁 stake 再查事实，`apply_adjudicate_social_fact` 未检查当前必须 Challenged | 属于已有目标的实现缺口；本设计不修改社会动作状态机，不靠它建立已确认履约事实 |
| Runtime 旧分数 | [economic_contract_publication.rs](../../../crates/oasis7_world_state/src/world/economic_contract_publication.rs) 与 [alliance_war_publication.rs](../../../crates/oasis7_world_state/src/world/alliance_war_publication.rs) 写 `reputation_scores` | 旧分数不是本 profile 的情境事实，也没有可逆还原的逐项来源；不导入、不换算、不清零旧历史 |
| 旧分数的真实消费者 | [governance_identity_snapshot_for_agent](../../../crates/oasis7_world_state/src/state.rs)、[Agent claim quote](../../../crates/oasis7_runtime/src/world/agent_claims.rs)、[工业制造资格](../../../crates/oasis7_world_state/src/world_rules/industrial.rs) 读取旧 score | 这些入口必须与新咨询投影隔离；本文不宣称已经迁移或修复全部旧资格制度 |
| 允许的目标来源 | [即时交换](agent-mediated-contract-formation.design.md#contract-formation-commit) 与 [供能结算](checkpoint-power-service-fulfillment.design.md#service-checkpoint-settlement) 已定义目标 receipt；native 合同仍是不同的旧协议 | 需要两个真实 source adapter、完整 proof/受众/历史身份和目标合同实现；不能从两次 transfer 或旧 Settle 拼出目标 receipt |
| 本专题投影/复核 | 未找到已实现的本 profile、完整 source adapter 或专用验收 | 本文定义目标与方法；不宣称任何当前入口已能产生本专题有效记录 |

## 4. 边界与结构

| 组件 | 产生、持有与读取 | 禁止越界 |
| --- | --- | --- |
| 原合同 authority | 原签署、执行、单位账本、责任决定、补救和有权更正；保留原 receipt | 咨询层不能更改原价款、责任、finality 或程序窗口 |
| 来源适配器 | 在指定 canonical cut 验证原 profile/schema、完整因果、当前有效性和受众；输出 typed evidence | 不接受模型总结、自由 claim 或只存在的事件引用代替结果 |
| 情境投影 | 按政策派生主体、情境、相关性及更新链；索引和摘要可重建 | 不持有第二份余额、资格、声誉总分或独立事实真值 |
| 客观纠正路径 | 有界请求、证据比对、版本检查、linked correction 和原请求结果 | 没有自由裁量的“声誉管理员”；无权判定合同责任 |
| World Service / Viewer / Agent | 受众允许的同 root 读面、当前/历史标识和可做下一步 | 不用缓存缺失证明清白、不把推荐变成自动拒绝合作或新授权 |

本制度的政策和派生记录按 [Kernel/Institution 边界](../prd.md#native-surface-分类与冻结规则)承载；不新增制度专属 native Action 或 `WorldState` 顶层分数字段。来源适配必须读取 Kernel/原合同已验证的事实，不能允许 module 自报 receipt 有效。目标可用受保护的版本化 instance/component 与统一提交；这不证明当前通用存储、零业务费用登记或 source adapter 已存在。

## 5. 关键运行流程

1. **说明咨询目的。** 请求指定 subject kind/identity、world/branch、原服务 profile、地点和查询时间范围。缺少合法身份或本 profile 不支持的组织/岗位，明确拒绝；不以当前持有 Agent 的账号补填历史主体。
2. **固定可解释的读点。** World Service 取得 exact canonical root/tick、policy version、来源适配版本和投影 coverage；按原受众规则读取来源。partial/unknown coverage 继续显式显示，不输出虚构的“全部成功”或“零违约”。
3. **验证并派生。** 对受支持来源计算原事实身份、有效结果、主体/任期、情境、去重和相关性；补救和原问题并列。不存在总分、胜率或跨主体相加。
4. **玩家决定。** 展示实际事实、来源限制和可行下一步：查看原件、缩小范围、等待来源恢复、请求客观更正，或另行按原合同流程提出合作。咨询结果不自动形成 offer/accept/activate。
5. **请求纠正。** 玩家提交具体原记录和可核验证据；确定性比对可证明投影有错时，按当前版本一次修正派生展示并追加链接。需要责任判断的请求指向原合同程序；证据缺失不判任何一方错误。
6. **恢复。** 响应丢失先查原请求；已提交修正不能因为尝试窗口到期再次执行。来源不可用、投影不完整或旧版本不兼容时保留历史并说明阻塞。

## 6. 接口与数据合同

<a id="reputation-source"></a>
### 6.1 只从完整权威来源建立记录

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

| 来源类别 | 适配器必须证明 | 可显示的事实及不能推导的含义 |
| --- | --- | --- |
| 即时双边交换 | 原 `owner_atomic_resource_exchange_v1` 的同版条款/双方接受、合法提交证明、两腿与全部费用、原请求及唯一履行 receipt | 这一次约定已履行；不能推导长期保供、其他服务能力或一般诚信 |
| 供能已交付 | 原 `owner_checkpoint_power_service_v1`、provider 段、检查点/不重叠单位、对应交付/价款/抵押释放的同次 receipt | 这些具体单元已交付；不能把部分成功变成全合同完成 |
| 供能终止或中性尾部 | 原合同允许的具体单位处置及合法实际提交；原已确认责任与未付量仍可查 | 剩余服务已按所列条件结束；中性结束不证明无过错，返还 pending 不等于已到账 |
| 供能责任最终化 | 原责任承认/和解，或原有效 panel 决定依固定程序成为 final；完整案件版本、范围与当前有效性 | 只陈述该范围、原责任主体和原结论；due、一个失败 ACK、缺席或无裁决都不构成此事实 |
| 补救或赔付已发生 | 原关联责任、合法实际履行/到账 receipt、范围及未执行量 | 已完成的补救量；不改写原失约，也不把承诺补救当实际完成 |
| 原来源有权更正 | 原 authority 的明确纠错种类、权限、前后结果和 linked receipt；适配器理解该版本 | 派生展示反映更正并保留历史；未知“管理员改值”不成为可消费更正 |

`SourceEvidenceV1` 必须包含：canonical world/branch、source profile/schema 与 instance、原 agreement lineage/revision、原 outcome/单位 identity、原 terms/接受与 authority refs、原事实 tick、当前核对 root/tick、source receipt/commit proof、当时签署者与 Agent/控制来源、情境和受众依据、结果及有权后继更正链。只有已授权读取者得到相应字段；摘要省略私密内容不能省略决定结果是否成立所需的验证。

每个来源必须由登记的 typed adapter 判定，不能通过 `schema_id` 含 reputation、description 或 notes 文本匹配。`native EconomicContractSettled`、`SocialFactPublished/Adjudicated`、war score、两条独立 transfer、外部截图和 LLM 解释均不在此白名单。原 `confirmed` 社交状态只表明原社交程序的结果，不等于本专题确认了履约。

缺 profile/schema 支持为 `unsupported_source`；缺原件、身份、完整更正链、最终性或当前覆盖证明为 `source_unavailable`。二者均不能回退到零分、负面事实、无过错或已履行。当前两个允许来源仍是目标设计，真正 adapter 未交付前，本专题不能凭合成 payload 报“真实来源已可用”。

<a id="reputation-subject"></a>
### 6.2 主体、Agent 与控制任期分别绑定

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

| 视图 | 记录主体与可归属内容 | 控制/成员变化 |
| --- | --- | --- |
| `personal_principal` | 原合同中经验证的个人签署主体，以及其当时承担的确切责任/履行结果 | 不把原个人责任写给后来的 Agent owner，也不把个人历史转给组织 |
| `agent_history` | 同一 Agent 稳定身份、当时控制来源/任期、实际参与动作及原责任主体引用 | Agent 历史继续可追溯；必须标出控制任期，不能展示为现 owner 的个人表现 |
| `organization` / `role` | v1 不支持 | 返回 `unsupported_subject_kind`；不能以 Location、成员 Agent 或角色名称代替 |

任期使用原 authority 的身份来源、generation/revision 与可核验起止，逻辑上表示为 `control_epoch_ref`；不假定当前代码已有同名字段。不能仅凭 Agent ID、当前 owner 或客户端日期反推原责任。原合同已有显式供方继受时按原 provider 段归属，新供方只承接其已同意的未来范围；旧失约、价款来源和补救事实仍指向原责任主体。

一份 source outcome 可以投影到个人与 Agent 两种受限视图，但保留同一个 `origin_key`。不得把两个投影加成两份经验，也不得把已转移 Agent 的全部历史批量计入新 owner。无历史任期证明时，该记录标记归属不完整，不改成匿名负面记录或自动借用当前主体。

<a id="reputation-context"></a>
### 6.3 情境匹配、来源去重与关联方

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

`ContextKeyV1` 包含 world/branch、源服务 profile、原签署/履行所适用的 canonical 地点、原合同 lineage、事实时间区间、责任/行动角色，以及受众可读范围。查询的合作目的固定为 `consult_before_contract`。查询地点必须与源范围按原地点 authority 精确匹配；当前位置、组织势力范围或“附近”文本不得扩大地点，跨区域匹配尚无 authority 时不支持。

玩家可以在同地点、同服务类型下比较多个原合同，但每行必须保留原合同与时间。查询过滤条件只能收窄，不能把 Electricity 服务履约翻译为物料运输、R&D 能力或一般政治信誉；同一个世界名称也不能替代 branch/identity。

来源键先于投影键：`origin_key = (world/branch, source instance, source profile version, agreement lineage, outcome kind, canonical business-unit identity)`。实际编码使用原 canonical codec 与稳定身份，不 trim/casefold ID。服务单位为原 checkpoint 下规范化、互不重叠的半开整数区间；相邻交付可并为一行解释，但仍保留原 receipt 集和各自时间，不能给新合并行重置寿命。

同一个 receipt 的转载、重试、不同请求、缓存重建、补证和主体投影不增加事实；同一个单位的拆分交付/修复使用原 lineage 累计。继受切 provider 段，不能复制原范围；新续期确为新合同时保留前序关联，不清洗旧结果。消费者显示确定数量时只显示来源明确的合同/单位，v1 不输出成功率、总体评分或“独立合作次数”。

对手关联性只能来自获准身份 authority 的最小化断言，分别为 `related`、`independent_in_declared_scope`、`unknown`，同时保留适用范围与版本。不同 Agent、不同地址、交易次数、互相好评和短期循环转移都不能证明独立；缺证明显示 unknown，不因怀疑删除真实履约或惩罚主体。原资源/责任记录继续保留，但这种记录不生成可刷取奖励、排序或资格。

<a id="reputation-validity"></a>
### 6.4 当前相关性、补救与历史更新

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

相关性寿命和尝试窗口数值只取 [Gameplay 政策](../../game/gameplay/gameplay-contextual-reputation-contract.prd.md#contextual-reputation-policy-v1)。对原事实 `origin_tick` 和其首次适用的政策版本，派生 `relevance_until = checked_add(origin_tick, policy.relevance_ticks)`；区间为左闭右开。晚索引、迁移、重连、纠正、转载或补救不修改该原事实 tick 或延长到期。

首次收录晚于到期的事实可以进入历史视图，但不得获得一个从收录时刻开始的新有效期。更正源 tick 的前提是原 authority 的确切事实证明，不能用本次更正提交时间替代。溢出、未知政策或缺 canonical tick 时为 `relevance_unavailable`；不得饱和到最大值制造无限期记录。

记录至少分开三个维度：

| 维度 | 可读状态 | 判定 |
| --- | --- | --- |
| 来源有效性 | 已验证、来源失效/被更正、来源不可核对 | 原 receipt 与当前有效后继链；缺来源不是撤销 |
| 当前相关性 | 当前相关、已过期、当前性未知 | 所查 canonical tick、原事实 tick、冻结政策与完整覆盖 |
| 原业务结果 | 已履行、原约终止、责任已 final、补救已发生及剩余量 | 原合同 authority；不由前两维度推导 |

已 final 的责任后来支付赔付，新增的是原结果下关联的实际补救附件；附件保留实际付款 tick 和 receipt，但不建立独立声誉 origin 或当前窗口。“原责任已补救”的当前性始终沿原责任窗口，晚于该窗口的补救仅更新授权历史展示。部分付款可以累计原单位的真实已付量，不能通过拆付刷新寿命或把每次付款计作独立成功交易。到期不是清偿、免责、原事实删除、Agent 记忆清空或旧 receipt 回收。

源事实被合法纠正时，追加 `supersedes/corrects` 因果；历史读保留当时的旧投影和其已被纠正的标识。源仍有效但读点落后时显示 `stale_at_root`，不把旧错误投影继续称作当前有效。来源暂不可取时保留“截至上次已验证 cut 的事实”及来源未知标识，既不静默隐藏历史，也不把尚未复核的缓存称为当前有效。原争议程序中的初审、待复核、final 或 invalid 始终按原合同判定，不能因本层查询、投诉或到期改变。

<a id="reputation-preview"></a>
### 6.5 只读预览、权限与接口结果

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

| 接口 | producer → consumer | 身份、版本与顺序 | 结果与失败 |
| --- | --- | --- | --- |
| `read_contextual_history` | 已授权玩家/Agent → World Service/source adapter | subject、context、分页 cursor、exact root、policy/adapter version | 带来源和 coverage 的当前/历史记录；unsupported、unknown 与 verified-empty 分开 |
| `preview_projection_correction` | 受影响主体 → 同源只读验证器 | 原 origin、具体错误字段、expected projection version、证据 refs | 可证的展示差异、受影响主体/行及原责任程序入口；不登记请求、不改记录 |
| `request_projection_correction` | 受影响主体 → 统一 admission | 原 request key、完整 payload digest、source root、expected version、证据 refs | 登记有界尝试；只有确定差异的合法提交才返回 corrected；登记不是事实改变 |
| `query_correction_original` | 原请求有权读取者 → 原件查询 | 原 request/attempt identity、所查 root | 原决定与 correction receipt，或明确 unknown；过期不擦除已提交结果 |

预览必须说明哪些记录会改变、变更依据、源责任是否仍待决、补证/原申诉是否可用、观察 root、期限与当前 blocker。v1 的明确效果只有咨询展示和已提交的客观纠正；合作、治理、claim、工业资格和机会分配均为未连接的消费面。没有真实消费者依据时显示 `not_connected` 或 unknown，不输出“合作机会提升”“会取得资格”或从正负权重推导效果。

任意社交 publish/challenge/edge 请求仍沿 [原社交预览合同](../../world-simulator/kernel/social-fact-ledger-declarative-reputation.prd.md#11-玩家侧社会动作后果预览)处理。本专题只给出其不得被误当本咨询来源或资格变化的接缝，不声称重做了所有社交动作的预览。原字段 `cooperation_opportunity_delta=positive` 和 schema 文本只能作为旧表面的历史实现事实，不是本设计可信预览输入。

只读请求不得预留资源、扣费、改变声誉、建立申诉/合同、添加关系边或发出世界动作。可读摘要至少显示：谁/哪个 Agent/什么任期、什么地点和服务、具体结果与数量、何时发生、来源/当前核对状态、有效期、关联补救、原件和实际下一步。字段被受众权限遮蔽时必须显示覆盖限制，不以遮蔽当不存在；历史参与者可依原授权读原事，不因此得到现 owner 的余额、私有目标或新写权。

<a id="reputation-correction"></a>
### 6.6 有界客观纠正与原事实申诉分流

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

允许的客观错误仅为主体/控制任期错配、遗漏已授权可读的确定来源、重复单位、错误来源有效性、错误相关性时间与投影未反映原有权更正。申请人必须是该记录受影响的个人主体、其当前有权代理，或具备原历史读取和纠正申请资格的原当事人；当前控制同一 Agent 不自动取得前任的请求权限。

| 输入判定 | 本层结果 | 不能发生的效果 |
| --- | --- | --- |
| 完整 canonical 证据可唯一确定投影错误 | 由确定规则生成精确 diff，CAS 成功后 `corrected`；追加原件链接 | 不改原账本、责任、余额或过去事件 |
| 完整证据与现投影一致 | `no_change`，可引用原已提交纠正 | 不生成新 revision 或延长相关性 |
| 证据可读但不证明所述投影错误 | `not_established`，说明未成立的字段 | 不反过来证明申请人恶意或给予负面记录 |
| source/root/更正链不完整或不可取 | 尝试状态 `awaiting_source`、外部 blocker `evidence_unavailable`；窗口内允许有界补证/重查，到期为 `review_incomplete` | 不自动隐藏原有效结果、不冻结主体、不裁定原事实真假 |
| 原义务、责任或裁决程序有争议 | `source_procedure_required` 并指向原合同 case/原窗口 | 不建立第二案件，不复活原终态或自行任命裁决者 |
| 不支持主体/来源、无权、输入超限或错误 world | 明确无效果拒绝 | 不登记资源 hold、不给申诉优先级或计分 |

一次尝试绑定 `(world/branch, profile instance, origin_key, subject, error_kind, relevant_source_revision, first_request_identity)`，记录核对的 exact canonical root、首次完整受理 tick 与按政策导出的不可重置截止。`relevant_source_revision` 只由本 origin 的来源及其有效更正/归属依赖计算，不能直接使用不断推进的 World root 作为新证据。相同 origin/subject/error/相关来源修订的未结束尝试合并到同一原请求；补证、换 ID、刷新页面或 source 暂不可用不重新计时。补证只追加原范围内证据，不能换合同、扩大主体或重复消耗更多份额。

客观错误即使历史已过期也可更正派生展示。一次尝试到期不授予永久正确性，也不取消此权利：后来取得新增可核验证据，或该 origin 及其归属/有效更正依赖确实产生新修订，可提交新的有界尝试；仅 World root 推进、重排相同 refs 或把同一 receipt 重新编码不算新证据，相同材料返回原结果。记录是否当前相关仍依据原事实 tick，而非重新尝试时间。

提交点重验受众/申请权限、policy、source 有效链和 expected projection version。若 source 或派生版本变化，返回 `review_stale`，给出重读方向；不把旧预览 diff 直接覆盖新事实。若源争议已在其原程序中得到有权更正，本层只反映该决定；否则不得以“更正评分”名义争夺责任、删除 final 或绕过原一次程序性申诉。

本层窗口只限制一次纠正尝试的执行资格，不承诺墙钟处理时长。明确未提交时到期才为 `review_incomplete`；已提交或结果未知必须查原请求，即使查询发生在截止后也不能撤回、更改为无效果或再执行。

## 7. 状态、事务与持久化

<a id="reputation-recovery"></a>
### 7.1 一份来源、一个投影版本与一个提交点

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

业务事实仍归原合同；派生索引包含 origin、source receipt/更正链、subject/context、原事实 tick/冻结 policy、派生版本与 coverage。可重建索引不能成为删原件的理由。纠正尝试状态与事实有效性分开：`registered/awaiting_source/corrected/no_change/not_established/source_procedure_required/review_stale/review_incomplete` 只描述该次工作，不给原事实追加罪责或可信评分。

传输去重键为 `(world/branch, profile instance, requester identity, request_id)`，payload digest 覆盖原 origin、error kind、subject、source root、expected version、完整证据 refs 与实际操作。相同键相同内容返回原决定，相同键不同内容 `request_conflict`。业务去重使用原尝试身份和 projection CAS；新请求 ID 不能再应用相同 diff。

纠正 prepare 在同一 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)中校验来源依赖、受众、窗口、证据大小和当前投影，暂存 correction link、派生版本/索引、原请求结果与 receipt。任一业务检查、容量、编码或持久化失败且确定未提交时全部保持原状；不得先改缓存再异步补原件。

只有达到实际 [authority commit](../design.md#runtime-authority-commit-recovery-target)证明条件才返回 `corrected`。该 receipt 引用原事实与前后投影 commitment、来源 cut、政策、错误种类、具体 diff 和原请求；不嵌套自包含的 containing-block hash。对照原事实和全部资源/权限/分数的 diff 必须为零。

本 profile 的只读、尝试登记和纠正不扣业务资源或游戏额度。若当前管线只能通过收费 module 调用登记，或没有合法的有界元数据路径，则不可启用写入口，不能暗扣 artifact owner、调用者或缺 owner 的免费分支。工程防滥用限额限制输入/工作，不出售声誉、资格或更正权。

### 7.2 恢复、重放与依赖保留

恢复闭包包含原 source refs/必要原件、历史身份和任期、适用政策与 adapter 版本、全部有效更正链接、projection commitment、coverage cut、尝试首次受理与截止、请求 digest/结果和纠正 receipt。原来源的 retention 与 [存储治理](runtime-storage-footprint-governance.prd.md)共同决定依赖保留；业务相关性到期或一次复核结束不删除去重、责任、源更正或恢复依赖。

索引重建从受验证的 source 和同版本规则确定性派生，不重新调用模型、不读取现在的 owner、不把历史补救当新交易、不以现在的 tick 重启寿命。旧 root 可用于明确的历史读取；作为当前查询时必须核对到当前 source head 的覆盖。分页和增量构建途中显示 partial/unknown，不能把尚未索引的失败记录当不存在。

客户端超时先按原请求与业务键查询。`not_found_at_root` 只说明这个 cut 没看到，不证明后续/在途没有提交。根 generation 进入 `StatusUnknown` 时沿 [G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design) fence 整个旧 writer，停止后续 mutator/save/依赖派发，核对精确候选与持久决定后恢复；不能用本案到期或“只改展示”继续写不确定 World。

已提交纠正恢复后只返回原结果；其索引未发布可重建，不重复创建 correction。关闭 profile 仍保留授权历史读取和原件查询；新写入禁用不能改成历史记录被撤销。

## 8. 部署、安全与运行约束

<a id="reputation-consumers"></a>
### 8.1 消费隔离与最小披露

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

允许消费者只有合作前咨询读面与客观投影纠正。无任何 adapter 将本 profile 输出写入 `reputation_scores`、治理 vote cap、Agent claim tier、制造资格、价格/税费、pioneer priority、区域许可或稀缺机会。即使所有记录当前有效，消费者也只能据此解释历史；后续是否合作仍走当前授权和原合同形成。

[成熟世界认可/机会](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-recognition-opportunity)有独立的资格、用途、容量和一次结算合同。未来若消费本来源，必须另行定义具体可消费字段、授权、适用范围、到期重验及拒绝/恢复；本版没有这个边，不能把产品允许未来组合写成已经支持。

原合同受众继续限制披露。相同主体之间、历史当事人与新 owner、个人与 Agent 视图的权限分别核验；不因“信誉公开”扩大到私有余额、控制关系原件、私人模型提示或完整对手材料。关联关系只给最小适用断言，不暴露非必要个人资料。无权读取的行显示不可核实覆盖限制，不能补写正面/负面结论。

拒绝跨世界/分支/instance 重放、未知 adapter/schema、自签 proof、未声明 source 更正和不规范单位区间。错误信息给出可行动原因与合法入口，不泄露第三方源内容。无权申请不产生负面信誉；有权申请也不自动压制原有效事实。

## 9. 质量与容量

本 profile 的工程初值为：单请求 canonical bytes 最多 32 KiB，最多 16 个内容寻址的 receipt/proof refs，一段说明最多 2048 UTF-8 bytes；每次纠正只针对一个 origin 派生记录。补证与重复请求共同占用原尝试的 16 个引用份额，替换相同引用不重置上限。读取每页最多 50 条，cursor 绑定 exact root、subject/context、adapter/policy 与排序键，禁止跨 root 续页后仍宣称同一快照。

来源正文按原权限和有界 exact read 取得，不把无限 ancestry 全部塞进命令。一次复核工作批最多读取 64 个已登记依赖对象或 1 MiB 解码内容，任一先到即暂停为 `awaiting_source`/`review_budget_exhausted`，保存只读 continuation；每次尝试最多 32 批、2048 个不同依赖对象及 32 MiB 总解码内容，任一累计上限先到则终止为 `review_incomplete` 并注明预算原因。重取对象仍消耗读取/字节预算，重复内容身份不成为新证据；发现依赖环即拒绝该闭包。后续批、补证与换请求不重置累计预算或截止。continuation 只引用 source cut 与已校验依赖，不缓存可跨版本复用的授权。在完整验证前不出“已确认”的部分结果；同材料不能换尝试绕过累计耗尽。

每个 canonical instance 同时最多 256 个未结束尝试，每名个人主体最多 8 个；受托 Agent 共享其请求主体的限额。精确原请求查询、已有尝试补证和最终结果恢复不占新的尝试槽。上限满时只拒绝新登记并返回 `review_capacity_unavailable`，不处罚、隐藏原记录、取消已受理尝试或挪用其份额。该拒绝不构成已受理请求，不开始尝试时钟；容量释放后可正常提交。关闭/终结只释放活动槽，原件与去重依赖继续保留。不得从 metadata 限额导出可交易资格或世界资源占用。

这些是本目标的工程工作量约束，不是当前生产指标。政策寿命/尝试窗口仍只由 Gameplay 维护。实现必须证明必要来源闭包在有界工作量内可完成，或把该来源明确置为未支持；不能通过截断 proof、漏掉不利事实或默认可信来适配限额。

| 场景 | 环境、输入与刺激 | 预期响应及判据 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 真实来源与隔离 | 独立候选 world，两个个人合同 profile 与旧 native/social 对照 | 允许来源逐字段核对；伪来源和跨主体/情境拒绝 | [REP01](../../testing/manual/contextual-reputation-acceptance.manual.md#rep01-source) | 目标，未运行 |
| 期限与恢复 | 固定政策、边界 tick、晚索引、修复、失联与 restart | 原寿命不刷新；unknown 不变无效果；同一纠正只生效一次 | [REP05](../../testing/manual/contextual-reputation-acceptance.manual.md#rep05-correction) | 目标，未运行 |
| 容量与分页 | 恰好/超过 refs、bytes、page 和依赖批上限 | 有界拒绝/续查，覆盖态准确，原事实不被截断或隐藏 | [REP07](../../testing/manual/contextual-reputation-acceptance.manual.md#rep07-recovery) | 目标，未运行 |
| 玩家可理解性 | 目标入口、原记录与补救/更正并存 | 能区分事实、责任、时效和未支持消费，不把参考当保证 | [REP08](../../testing/manual/contextual-reputation-acceptance.manual.md#rep08-consumers) | 需真人证据 |

本专题不新增延迟/吞吐 SLO，不从一个 fixture 推断大世界容量。索引按 subject/context/origin 有界查询，不为每次咨询扫描全世界完整历史。写入/索引容量不足时保持明确不完整或不可写，已有合法历史和纠错依据不能被新请求挤掉。

## 10. 兼容、迁移与回滚

<a id="reputation-compatibility"></a>
### 10.1 目标 profile 与旧系统分开启用

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

启用至少需要：合法 institution 分类/激活、两类明确的源版本适配、可核验历史身份/受众、完整 current cut、有限免费元数据路径、同根纠正与持久化/原件恢复、单一政策版本，以及真实消费者隔离。缺一项不得用旧 quote、旧 score 或合成 receipt 顶替。

旧 `reputation_scores`、social facts/edges 与原 journal 按各自历史语义保留；本 profile 不回算、清零、更新或宣称修复它们。旧 score 无逐项可核验来源时明确 `unsupported_legacy_source`；不把累计数字拆成虚构成功/失败。两个目标合同 receipt 尚未产生时结果为来源能力未启用，不能显示“此主体履历为空且可信”。

source schema/adapter/policy 改版固定版本与生效边界。已记录事实继续保留其原寿命和解释；新版本不能给旧记录重启当前窗口、扩大受众或把旧事实转换为资格。对原投影算法的确定错误可以按同一纠错规则追加修正；无法解释的旧 schema 保持可读原件/不可派生，不猜测迁移。

回滚必须保留读到已接受的新 source/纠正和原件查询的能力。旧消费者无法验证新 profile 时关闭该消费者的新写/当前结论，保留明确历史读取；不能删除新 correction、重放旧业务动作或恢复成一份未标记陈旧的缓存。此次设计不选择 SC32 pilot，不改 IWS 的在途存储实施范围。

## 11. 验证设计与可追溯性

以下均是未来实现的验收方法。运行需固定 source/tested tree、政策、两个源 profile/adapter 版本、root、受众及环境。静态设计审读、文档检查和测试源码不能代签已运行、真实玩家或完整 CR003 验收。

### 11.1 验证映射表

| 上游要求与验收 | 本设计条款 | 独立 obligation | 验证方法 / candidate / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | [来源适配](contextual-reputation-evidence-and-review.design.md#reputation-source) | 来源真实、完整且可读，不从声明造信誉 | [REP01](../../testing/manual/contextual-reputation-acceptance.manual.md#rep01-source)；固定候选，required 来源合同，full 真实执行来源；未执行 | 源 receipt/proof、字段对照与拒绝零差分 | 未支持合同、所有 social schema |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | [主体与任期](contextual-reputation-evidence-and-review.design.md#reputation-subject) | 原责任、Agent 历史、换 owner 和继受分开 | [REP02](../../testing/manual/contextual-reputation-acceptance.manual.md#rep02-subject)；固定身份与控制历史，required；未执行 | 个人/Agent 投影、权限及 provider 段 | 组织/岗位正式支持 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | [情境与去重](contextual-reputation-evidence-and-review.design.md#reputation-context) | 不跨情境，不把拆分/循环/转载变独立经验 | [REP03](../../testing/manual/contextual-reputation-acceptance.manual.md#rep03-context)；固定 source lineage，required；未执行 | 原单位覆盖、唯一 origin、关联断言及拒绝 | 全网防女巫、玩法平衡 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | [寿命与更新](contextual-reputation-evidence-and-review.design.md#reputation-validity) | 到期、晚索引与补救不洗掉历史或延长期限 | [REP04](../../testing/manual/contextual-reputation-acceptance.manual.md#rep04-validity)；固定时钟/政策，required；未执行 | 边界 tick、原/补救来源与更新链 | 现实天数或历史回收策略 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | [预览与读面](contextual-reputation-evidence-and-review.design.md#reputation-preview) | 预览只读且不虚构 surface 效果 | [REP06](../../testing/manual/contextual-reputation-acceptance.manual.md#rep06-preview)；同 root API/Agent/Viewer，required；未执行 | 前后业务 diff、coverage、当前/历史表述 | 全社交预览或实际 UI 可用 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | [有界复核](contextual-reputation-evidence-and-review.design.md#reputation-correction) | 客观纠错可重现，责任申诉不越权/续窗 | [REP05](../../testing/manual/contextual-reputation-acceptance.manual.md#rep05-correction)；同来源和请求矩阵，required；未执行 | 原请求、唯一 diff/receipt、原程序路由 | 一般裁决与 R&D 归因 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) | [事务与恢复](contextual-reputation-evidence-and-review.design.md#reputation-recovery) | 同根、去重、未知及原依赖可恢复 | [REP07](../../testing/manual/contextual-reputation-acceptance.manual.md#rep07-recovery)；固定候选，required + full 故障/持久化；未执行 | 前后 commitment、故障点、原件和恢复 diff | 未运行的故障域与跨节点能力 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | [消费隔离](contextual-reputation-evidence-and-review.design.md#reputation-consumers) | 不影响旧 score/治理/claim/工业与机会 | [REP08](../../testing/manual/contextual-reputation-acceptance.manual.md#rep08-consumers)；required consumer 对照，full 正式入口；未执行 | 所有登记 consumer 的零变化及真人理解记录 | 全部旧制度修复、完整机会组合 |
| Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) | [兼容与启用](contextual-reputation-evidence-and-review.design.md#reputation-compatibility) | 未支持来源不回填，旧历史与新纠正可区分 | [REP08](../../testing/manual/contextual-reputation-acceptance.manual.md#rep08-consumers)；版本/禁用/回滚对照，required + full；未执行 | profile/adapter 清单、历史读取与无伪迁移 | SC32 pilot、部署或发行 |

## 12. 决策、长期风险与未决问题

| 决定或依赖 | 理由与限制 | 主责与复核触发 |
| --- | --- | --- |
| 采用不评分的两种个人合同咨询切片 | 能以逐项来源解释合作历史，避免无证据地把现有 score 包装为产品声誉 | Gameplay / Runtime；新增来源或正式资格 consumer 前重新设计 |
| 业务寿命/窗口由唯一 Gameplay 政策维护 | 当前相关性、原责任期限、历史保留互不替代；初始 tick 选择不是墙钟保证 | Gameplay / QA；代表性世界节奏与玩家理解证据出现后复核，不追溯刷新旧事实 |
| 先做客观纠正，责任仍归原合同 | 防止新管理员或原事实发布者取得裁决权 | 原合同 owner / Runtime；原来源申诉变化时核对 adapter 和投影 |
| 需要真实 source adapter、历史身份与受众证明 | 原目标 receipt 尚未完整交付；没有这些输入不能形成有效信誉 | 两个合同实现 owner / QA；实现后以实际提交和原件完成 REP01–08 |
| 旧分数与其消费者保持显式差异 | 不破坏历史回放，不假称全局信誉矛盾已经由本专题解决 | 原治理/claim/工业 owner；另行迁移时保留其适用产品边界及独立验收 |
| 组织/岗位、任意社交声明、完整机会/R&D 仍未覆盖 | 本切片不降低 CR003 的剩余承诺，也不代表 GD-13 全部设计完成 | 产品与对应专业 owner；后续独立完整切片承接 |
