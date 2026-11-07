# 情境声誉、完整用途采用与有限复核验收手册

- Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`。
- 上游：[CR003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；规则唯一来源：[Gameplay 情境声誉政策](../../game/gameplay/gameplay-contextual-reputation-contract.prd.md#contextual-reputation-policy-v1)。
- 专业合同：[事实投影与客观复核](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source)。本手册提供方法与独立判据，不表示已执行或当前入口已支持。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。REP01–REP08 保留两个个人合同 v1 的合作前咨询、客观纠正和消费隔离方法；其中“不支持组织/岗位/资格”只检验该旧版本。后继完整目标另按 REP09–REP13 联合原形成、服务、组织、R&D、五动作预览及机会方法检验，不能以一个个人来源代签全项。本文所有步骤均为待执行方法，不是已运行证据。

## 1. 执行前置与证据层级

每次运行固定 source HEAD、实际 tested tree、canonical world/branch、源合同及 adapter 版本、Gameplay 政策、identity/控制历史、原 receipt 和所查 root/tick、环境、目标入口及执行者。记录命令、输入、实际输出、判据、失败和未覆盖范围，放在实际实现 PR 与原始测试产物中；不向长期文档追加临时通过台账。

三种证据分别报告：

1. **静态设计走查。** 用政策与合法源样例手算主体、单位、窗口和结果；证明规则可判定，不证明实现存在。
2. **合同/恢复测试。** 固定测试候选的真实入口、状态、receipt 与依赖，验证正负路径、拒绝不变性及故障点。可以用受控 source adapter fixture 测 schema/映射，但必须明确它不证明真实合同或 finality 已接通。
3. **真实来源与玩家表面。** 使用同一候选实际生成两个目标合同的 receipt，经 World Service 到 API/Agent/Viewer，并完成持久化恢复和需要的真人任务。未交付适配器、正式入口或裁决前置时为 `blocked/not_run`，不能以旧 native 合同或手工 JSON 替代后判通过。

本手册不要求修改生产世界、调用付费模型、发起真实惩罚或邀请玩家；执行这些动作沿相应测试任务已有授权。原合同形成、托管、供能与责任程序分别按 [即时交换手册](agent-mediated-contract-formation-acceptance.manual.md#cf-01-nonbinding)和[供能手册](checkpoint-power-service-acceptance.manual.md#ps01-formation)生成合法前置，不绕过签署、来源或当前授权。

## 2. 共同样例与独立预期

使用一个隔离 world `W1` 和其明确 branch、地点 `L1/L2`。个人 A/B/C/D 各有独立可验证身份；Agent `a1/b1/c1/d1` 由对应人合法控制。至少保留 A 控制 `a1` 的任期 E1、后续 D 控制同一 Agent 的任期 E2，以及一项已知关联和一项关联性未知的对照。identity/controller 断言由测试环境认可的 authority 提供，不从名字不同推断独立。

| 样例来源 | 通过真实源操作取得的记录 | 预期可表达范围 |
| --- | --- | --- |
| X1 | A/B 在 L1 的 `owner_atomic_resource_exchange_v1`，同版接受后两腿同次成功；源 tick 100 | 一个交换事实；个人 A/B 与各自 Agent 视图保留相同来源，不能加成四份经验 |
| S1/K1 | A 为供方、B 为买方的合法供能合同；K1 原单位 `[0,4)` 于 tick 101 交付 | 只证明 4 个已交付单元 |
| S1/K2 | 原单位 `[0,4)`，tick 141 先完成 `[0,2)`；剩余 `[2,4)` 到期 | 2 已交付、2 未履行；到期本身不是 final 责任 |
| S1/F | 独立分支按原供能有限程序确认 `[2,4)` 的具体责任，合法 final 于 tick 210 生效 | 原责任主体、原单位及 final；原交付不撤回 |
| S1/M | 对 F 的实际补救于 tick 220 提交，带到账 receipt；另有仅承诺付款对照 | 只给 F 追加实际补救附件；不产生独立成功或新当前窗口 |
| N1 | 原供能程序合法执行的无有效责任结论的中性尾部 | 服务按原条件结束；不能显示“履约成功”“供方无过错” |
| P1 | 合法 `SocialFactPublished`，引用一个真实但与履约无关的注册事件，claim 声称“全部履约” | 原社会声明可存在，但不是本咨询 profile 的可信来源 |
| O1 | 旧 native EconomicContract/war 结果和既有 `reputation_scores` | 仅是旧制度对照；不能转换为新履历 |

源操作的具体时点必须满足原合同窗口。S1 的示意可沿原供能样例的 `Q=8`、K2 due150/R166 构造：合法初审190、窗口内程序性申诉196、原证据/争点冻结后复核210；先证明这些前置和实际 final 成立，再使用其 tick。若测试候选无法产生此具体序列，应记录实际合法 source ticks 并重新计算下表，不硬写不存在的 receipt 或直接修改源状态。

依据当前 Gameplay 政策的样例计算，不在此另定义规则：

| 原事实 | 当前相关的最后一个示例 tick | 首个到期 tick | 更新后的要求 |
| --- | --- | --- | --- |
| X1 at100 | 699 | 700 | tick710 才建立索引也不能变成“新记录” |
| K1 at101 | 700 | 701 | 分页或重建保持该原窗口 |
| K2 已交付部分 at141 | 740 | 741 | 补交必须沿原单位，不能重发前两个单元 |
| F at210 | 809 | 810 | M at220 不把截止改成820；补救仍归 F |
| 纠正尝试首次受理 at650 | 709 可提交 | 710 窗口关闭 | 补证、换请求、重连不延长；已提交结果仍可在710以后查回 |

建立前后快照对照向量：原合同/单位、资源/hold/费用/treasury、控制/授权、旧 score、治理 snapshot/权重、claim tier/成本/额度、工业资格/占用、generic opportunity/priority、social facts/edges、原请求/纠正版本及 receipt。每个只读或无效果拒绝都核对这些量；不要只断言返回文字。

<a id="rep01-source"></a>
## REP01：真实来源、完整证明与声明隔离

依据：[来源适配](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source)。

**正向。** 从真实源入口取得 X1、K1、K2、F/M/N1。逐项核对 source profile/schema/instance、world/branch、原 agreement lineage、两方接受、历史 authority、单位/结果、原 tick、receipt/commit proof、受众和当前更正链。查询输出的每一项都能回到原件，结果与原账本完全一致；中性尾部、final 和补救保持不同含义。

**不得升级的输入。** 分别提交 P1、O1、一条转账、不同请求的两条转账、只剩一个签名的 X1、proposal/accept ACK、LLM 生成的“履约成功”、任意存在的 event ID。另将 `schema_id` 改为含 reputation/blacklist/trust 的字符串。它们均不得得到本 profile 的已验证履约记录；拒绝不改变源账本、资源、旧 score 或关系边。

**证明断裂。** 分别移除或替换 commit proof、原 terms、原 subject/任期、源更正链中的一个必需节点；使用别的 world/branch、尚未生效版本或未知 adapter。来源不可取时为 `source_unavailable`，明确不支持时为 `unsupported_source`；不能退化为 verified-empty、零失约或用户违约。只删除 cache 与删除受控 source adapter 可读能力分别测试，不能把二者误认为源事实已撤销。

**关键可观察失败。** 任意一条自由声明变成履约事实，或只有测试内构造 payload 却宣称真实源链通过，即本项失败。已有 social 生命周期测试和旧 native 结算通过均不足以代签。

**保留证据。** 固定来源清单、真实生成步骤、receipt/proof 原件或有权限的内容引用、每字段映射、拒绝原因及前后零差分。敏感身份材料不进入公开测试日志。

<a id="rep02-subject"></a>
## REP02：个人、Agent 任期、继受与受众

依据：[主体与任期](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-subject)。

1. 在 E1 结束前查询 A 的个人记录和 `a1/E1` 的 Agent 历史。记录指向同一 source origin；责任归 A，行动历史归 a1/E1。不得对两视图相加计算经验。
2. 依法完成 `a1` 的控制变更到 D/E2，再查询 D 的个人履历及 `a1/E2` 当前任期。X1/F 不属于 D/E2 的个人/任期记录；在合法历史权限下仍能另读 E1，并看见原责任人和任期。D 不能仅凭现 owner 身份更正 A 的个人记录、提取其资金或继承 credential。
3. A 换用新的自有 Agent 后，其原个人记录仍保留原源 identity；不能因换 Agent 或另开合同清洗 F。去掉历史 authority/任期证明，记录归属必须显示不完整，而不是绑定当前 owner。
4. 使用另一份未决范围已明确、符合原 profile 全部条件的供能合同，完成原允许的显式供方继受。新供方只归属继受后其真实交付；旧履行/责任仍归旧方。尚有可能恢复交付的未决保全时，原源应拒绝继受，本咨询不能伪造成功接替记录。
5. 对原 v1 请求 organization/role kind，或把 Location/成员 Agent 当组织；预期 `unsupported_subject_kind`。后继真实组织/岗位正向必须另外通过 REP09，不得沿此旧拒绝冒充整项完成。不能通过修改显示名称进入个人记录查询。
6. 以原当事人、现 owner、无关玩家和授权 Agent 分别查询。每一行按原受众返回；无权不可补成“没有不良记录”。原当事人合法读历史不应同时获知现 owner 私有余额、目标和新授权。

**失败判据。** 控制权转让带来个人信誉/责任转移，或权限遮蔽被显示为完整空记录，即失败。历史可读与当前任期适用必须分别断言。

<a id="rep03-context"></a>
## REP03：情境匹配、去重与关联交易

依据：[情境与去重](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-context)。

**情境矩阵。** 在相同已授权主体下，逐项改变 world、branch、L1→L2、源服务 profile、查询目的、原合同和时间区间。允许的同地点/同服务多合同比较保持每条原合同与时间；不匹配只能排除并说明范围，或明确不支持。不能把历史背景、当前地点或组织区域作为跨情境可信结论。

**去重矩阵。** 对 K1 使用整段 `[0,4)` 和合法的 `[0,2)`、`[2,4)` 交付两种独立合法源序列；分别重试同 request、换 request、复制 receipt、重建索引、并查个人/Agent 视图。比较总已交付量、原 origin/单位覆盖和来源关系；两种序列都只覆盖4单元，任何重复处理不再加量。不同真实 source tick 的分段可分别显示到期，不能把较新一半的到期替换整段原到期。

**恢复与新合同。** 原范围修复不能重复前两个单元；显式继受不复制已交付范围；新续期要有新接受/receipt，并保留旧合同关联。原结果尚未知时客户端换 ID 新建同一“成功经验”应失败或仍等待原件，不能由缓存解释为新交易。

**关联性。** 分别使用 authority 可证明关联、在声明范围内独立和未知三种对手。三者的真实履约仍可读，关系标签准确；不同账号/Agent、循环交易、互相声明好评不制造独立证明，不奖励、不处罚。输出不得存在总分、成功率或独立交易数排名字段。

**故意破坏断言。** 在受控测试实现中把 dedupe key 改为 request ID，或把 source timestamp 改成索引时间；同一组断言必须失败。该负控制只验证验收能发现错误，不将修改带入生产或正式候选。

<a id="rep04-validity"></a>
## REP04：时效界点、补救、历史与更新

依据：[寿命与更新](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-validity)。

1. 对共同样例表分别在到期前一 tick、恰好到期与后一 tick 查询。以 canonical tick 判定，不改变客户端时钟来推进状态；相同 raw source 和 policy 在两个 reader 上结果一致。
2. 在 X1 已到期后才首次构建索引，预期仅历史，不产生从索引时刻起的新窗口。清缓存、重启、迁移、换 owner、转载或原请求重试分别复测。
3. S1/K2 在 due/R 到达、只有未履行时不创建 final 负面记录；初审待复核和合法 final 分别显示。N1 只能表达原处置结果，不把中性尾部写成无过错。
4. 读取 F 后分两次实际付清原补救金额。实际到账量按原责任累计，付款承诺/ACK 保持未付；F 的原 tick 和到期不变，不能出现两次“独立成功”。再将合法付款置于原窗口到期之后，只更新授权历史的补救情况，不能重新把 F 移回当前集合。
5. 通过原来源的合法有权纠正取得 linked source receipt，确认新当前投影反映其内容，历史视图仍保留旧结果及已纠正标识。普通管理员改缓存、publisher 自报撤回或未知更正 schema 不构成源纠正。
6. 使用 source tick 缺失、未知 policy 和加法溢出的合成边界输入测试 adapter 失败语义。必须明确 `relevance_unavailable`，不能饱和成无限有效；合成输入只证明防御分支，不证明真实源可产生极端 tick。

**独立断言。** 相关性到期前后，原责任/应付量、实际已付量、原 receipt、历史身份和权限无隐式变化；只是可读的当前性不同。任何自动清债、删原件、刷新有效期或永久黑名单均失败。

<a id="rep05-correction"></a>
## REP05：客观投影更正、有限尝试与责任申诉

依据：[有界复核](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-correction)。

**可纠正错误。** 保持原合同和 receipt 正确，仅在受控派生索引层分别引入主体错配、遗漏一个已授权 receipt、重复单位、未反映源更正和期限错误。先证明来源本身未改，再以受影响主体提交具体错误、expected version 和可核验证据。正确结果是一份精确的派生 diff、一次 revision 和 linked correction receipt；源账本、资源、旧 score、权利与原时间均保持原样。

不能以直接篡改正式来源制造“成功纠错”场景。实现必须提供受控投影故障注入点，或让实际错误的旧投影版本经正常 reader 产生样例；没有可控方法时记录该场景未执行。

| 场景 | 独立预期 |
| --- | --- |
| 完整证据与当前投影一致 | no_change，引用原结果，不发新 revision |
| 来源可读但只证明另一个合同/字段 | not_established，不反过来给申请人负面记录 |
| 必要来源/更正链暂时不可读 | evidence_unavailable；尝试可为 awaiting_source，但原有效事实不因此隐藏或被冻结 |
| 申请改变原责任、赔付金额或 panel 结论 | source_procedure_required，精确指向原合同 case/原窗口；本层不判责 |
| 非当事人、现 owner 冒充前任、跨世界申请 | 无效果拒绝，不改投影或授予读取权限 |
| 旧原事实已过期但投影有客观错误 | 仍可纠正历史展示，不重启当前窗口 |

**不可重置窗口。** 在样例 tick650 首次完整受理；tick680/709 补证、换 ID、重连，截止仍为710。恰好710时尚未提交的尝试返回 review_incomplete，无 correction；相同证据再请求返回原结果，即使 World root 已因其他业务推进也不续窗。该 origin 的相关来源修订或新增可核验证据到来可形成新尝试，仍不能改变 X1/F 的原寿命；重排 refs、换编码或加入无关 receipt 不算新证据。客户端断网、来源暂不可读不把 canonical 窗口改为墙钟秒数。

**陈旧与并发。** 预览后原 source 更正或另一个纠正先提交，旧 expected version 请求必须 review_stale 或返回原已知结果；不能覆盖新值。两个同差异请求按 canonical 顺序只有一个改投影；相同 request 不同 payload 为 request_conflict。若两个不同错误均成立，后到者重读当前版本后才提交剩余真实差异，不能复制前次修正。

**责任程序分流。** 在原供能允许的初审/一次程序性申诉/已 final/已 invalid 条件分别提出本层请求。原程序窗口和决定有效性不变；本层不得用新的60tick尝试替代已结束的申诉窗，不重新抽 panel、不冻结全部服务。对原有权更正只消费其合法结果。

**未知结果。** 将正确纠正提交于709但丢失响应，710之后按原请求查询。已提交时返回原 correction；不能报告“窗口已过，所以没有效果”。持久结果未知分支沿 REP07 验证，不能换请求继续猜测提交。

<a id="rep06-preview"></a>
## REP06：真实影响预览与跨入口同义读面

依据：[预览与读面](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-preview)。

对 X1、F/M、N1、来源缺失和过期记录，分别取得 API、Agent 和 Viewer 的相同 audience、root、subject/context 查询以及纠正预览。逐字段比较主体、任期、范围、数量、原时间、来源/coverage、当前性、责任/补救、blocker 和下一步。文字可不同，但事实和允许动作不能不同。

预览至少显示本次可证展示差异、受影响行、原责任/申诉入口及其真实限制。对治理、claim、工业准入、机会或排名，v1 显示 not_connected/unknown；不允许复用旧 social quote 的 positive/contested 作为效应证明。换入同数字但不同来源的 confidence/weight，不能改变此结论。

分别测试完整空集合、partial覆盖、source_unavailable、受众遮蔽、旧 root 与当前 root。完整空只证明该有限情境/当前有权查询范围为空，不等于“信誉良好”；partial/unknown 不能显示零失败或成功率。分页中切 root/过滤条件必须重开读点，不能拼接出伪完整集合。

连续读取、预览、刷新、暂缓与关闭页面后，核对共同对照向量。除允许的非业务诊断计数外，不得有资源/费用、占用、关系、资格、合同、声誉、纠正请求或世界动作变化。预览后直接发执行必须独立重验，不得因看过预览而省略签署或 source proof。

**关键可观察失败。** “可能合作”被写成已获权限，缺记录被写成可信，或阅读动作触发任何 offer/accept/hold，均失败。

<a id="rep07-recovery"></a>
## REP07：事务、原件恢复、容量与版本化索引

依据：[事务与恢复](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-recovery)。

在可控独立环境逐点注入故障：来源读取后、身份重验后、投影 diff prepare 后、receipt 编码/持久决定前、持久决定已成立而响应前、索引发布前。记录实际 authority profile 和故障点；按该 profile 的真实持久提交条件判定，不能把内存 return 当持久成功。

| 已证状态 | 恢复判据 |
| --- | --- |
| 明确未提交 | 原投影、原账本和所有业务向量不变；只有同根稳定无效果决定可读 |
| 已提交 | correction、projection version、原请求结果与 receipt 同根恢复；重复查询/重发无第二次修正 |
| 客户端未收到响应 | 按原 request/业务键查询；not_found_at_root 不能证明后续没有提交 |
| 根 generation StatusUnknown | 原 writer 全局 fence；不继续 mutator/save/派发，以精确候选和持久决定恢复；不能仅关闭本声誉入口后继续写 World |
| 仅索引/缓存缺失 | 从同 source/adapter/policy 重建，保持原 source tick、任期和更正链；构建中如实 partial/unknown |

恢复前后比较原 source、policy/adapter、origin/单位、subject/任期、期限、纠正链、原请求及前后 commitment。重放不得重新使用当前 owner、重新运行模型、重判源责任或给已过期记录刷新有效期。profile 禁用后历史/原件读取仍可区分旧记录和当前可用能力。

容量按 design §9 的唯一上限取值，至少覆盖：

- 单个请求恰好达到及超过 byte/ref/说明上限；补证和换请求不能重置原尝试份额。
- 一次请求混入第二个 origin、无关合同或未知引用类型，明确拒绝而不部分更正。
- source 闭包恰好在一个工作批完成、需多个批以及单对象超出可支持范围；未完成不得输出确认，continuation 必须绑定原 cut，批次不延长尝试窗口。
- 累计工作批、不同依赖数和总解码内容达到上限；重取同对象、环形依赖、换请求或重开同材料不能绕过预算。耗尽仅表示本次未完成查验，不否定原事实。
- 个人和 instance 活动槽恰好满、同时登记竞争和终结释放；受托 Agent 不另占个人份额，原件查询、已受理尝试补证/结案仍可用，容量拒绝不开始新的复核窗口或改变履历。
- 多于一页记录，合法按 root 固定续页；篡改 cursor 的 root、subject、context 或版本后拒绝/重新查询，不混成完整视图。
- 索引容量不足、旧恢复依赖丢失、未知 adapter/schema；不丢历史或默认可信，保留准确 coverage/blocker。
- 旧 source + 新 policy/adapter、旧 reader + 新 correction、写入口禁用后的历史读。没有合法迁移映射时保持不可派生，不能重算旧 score 成新履历。

**负控制。** 故意让投影先发布而原请求结果未持久化，或恢复时用当前 owner/当前 tick 重建，原同根、归属和窗口断言必须失败。测试中捕获一次错误不能声称所有故障域已验证；跨进程/跨节点只按实际执行证据报告。

<a id="rep08-consumers"></a>
## REP08：旧消费者隔离、启用闸门与玩家理解

依据：[消费隔离](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-consumers)与[兼容启用](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-compatibility)。

**真实代码 consumer 对照。** 在固定候选列出读取新投影的全部消费者，并沿实际调用链检查旧 `reputation_scores` 写入、governance snapshot/权重、claim quote/tier、工业制造资格、generic opportunity/priority、经济价格/费用和关系边。以同一 source 分别提供当前、过期、已纠正、partial和unknown咨询结果；这些非咨询状态不得因新投影变化。只验证一个 UI 不足以声称隔离完成。

保留旧 score 的合法固定对照值及其原消费者结果；新咨询的读取、纠正、到期、关闭均不能改变该值或其历史 journal。反向测试把旧 score 数字、war结果或native单腿结算当适配输入，必须 unsupported_legacy_source/unsupported_source。此项证明两条路径没有被混接，不证明旧制度已符合全部产品目标。

依次缺失合法 activation、允许source adapter、完整 proof/任期、current coverage、免费元数据路径、可靠提交/原件查询以及未知policy，验收新写/当前确认均不可启用。不能因read demo有样式就开放“提交更正”，不能用收费module或缺owner分支实现所谓免费复核。

**真人任务方案。** 在有可操作目标表面后，首轮招募至少4名代表性参与者，其中2名首次使用此类间接控制游戏、2名熟悉合同协作；至少覆盖键盘操作和一种需要放大/辅助阅读的使用方式。样本用于识别误解和阻塞，不估计总体信任或留存比例；未覆盖的语言、设备和无障碍需求单独列出。使用虚构身份与隔离源样例，不展示他人真实敏感履历。

不先解释正确结论，给出以下任务：

| 任务 | 应观察的独立判断 | 不允许的主持提示 |
| --- | --- | --- |
| 比较X1与F/M后决定是否提出新的合作 | 能指出源地点/服务、已确认结果与补救；继续、缩小承诺、换人或暂缓都可合理 | 不暗示有一个“正确合作对象”或用分数替其决定 |
| 看到新owner/E2时解释旧记录归谁 | 能区分原个人责任、Agent历史与当前任期 | 不直接告诉参与者“owner变了所以不算” |
| 查看当前空、过期和source不可读三个样例 | 不把三者都说成零违约/值得信任；能找到范围和限制 | 不先翻译状态码替代界面信息 |
| 给一条重复单位记录申请更正，再给责任争议选择路径 | 能区分客观展示更正与原合同申诉，不期待新一轮任意裁决 | 不替玩家点击入口或给出case答案 |
| 纠正响应丢失且已过尝试窗口，选择下一步 | 优先查看原请求，不假定没生效后重复创建 | 不提示后端真实结果，只给表面可见材料 |
| 查阅一次合作预览后解释获得了什么权利 | 知道尚未签约、没有资格/治理/claim保证 | 不用口头免责声明替代产品缺失表达 |

关键理解判据为：参与者能从界面找到并正确解释主体/任期、事实/责任/补救、当前/历史/unknown和咨询/世界效果四种区别，并为给定失败选择现有合法下一步。首次无提示未做到时记录阻塞；给予一次不含答案的“请查看来源/限制入口”提示后再记录是否恢复。仍混淆其中任一种关键边界，或正常路径无法完成，判该样例需修订后复测，不以其他参与者成功平均掉。

记录实际动作/打开的来源、用时、错误重试与放弃、参与者原话、主持提示和评审推断，后两者分开。规则、术语、身份展示、期限/unknown反馈、source adapter或入口权限改变后重测受影响任务。真人理解记录不代替 source正确、同根持久化或 consumer零变化。

## 3. 结果输出与未覆盖范围

每个 REP 项分别给出 pass/fail/blocked/not_run、固定候选与源版本、运行层级、原始证据位置和残余风险。合成映射测试、真实来源测试、恢复测试和真人任务不得互相顶替；未运行的 provider、设备、跨节点环境或不可获得来源保持未验证。

本手册完成只说明这个有限咨询 profile 的证据达到了所报告范围。完整 CR003、通用声誉、组织/岗位、R&D、机会消费、其他多人冲突与整个 GD-13 的设计/实施/验收状态，仍分别由原任务和权威条款判断。

<a id="rep09-all-subjects"></a>
## REP09：全部来源、组织与岗位的正向适用

依据：[完整 source catalog](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source-families)、[四主体](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-general-subjects)。同一 tested tree 固定当前 source/adapter/consumer manifest，至少真实生成下列各族，不能只交一个 hand-written SourceEvidenceV2。

| 必需场景 | 独立来源 oracle 与成功判据 | 必须失败或保持未知的对照 |
| --- | --- | --- |
| 普通组织 O 与自然人 P 的即时合同 | 原组织登记、完整签署附件/mandate、当前代表、P及另需同意资金 owner 的同版接受；两腿/费用/commit 分别核原 ledger | 只用组织名字、成员列表、passed 或 OC custody grant 代表 O；组织代表换届后沿旧 mandate 再形成 |
| 全部已采纳持续服务及续作/继受 | 逐项采用 ServiceOutcomeSourceV1 真实 domain receipt，核履行、未满足、旧/新责任段、原 final 及实际补救/付款 | due、离线、失败 ACK、初审未 final、承诺赔偿均不得升级为违约/已付；新主体不继承旧个人责任 |
| 一般工作角色与组织代表任期 | 原 accepted service/RD 角色或 ordinary mandate 的明确 role/term、实际动作及历史 controller；个人/Agent/组织/岗位分别查同一 origin | 只改职位字符串、当前名单反推旧角色、代表个人钱误归组织、四视图加成四笔事实 |
| 原托管返还两路径 | 沿 [ORGREP01](organization-custody-role-reputation-acceptance.manual.md#orgrep01-source)真实 appointment_direct/delegated_term 前置和 ledger；只有合法 delegated 结果进入原岗位 | 直接路径强贴岗位、过期 delegated 降成 direct fallback；测试内注入阶段不能代签正式源 |
| 组织/多方 R&D | 沿 [RD08](rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd08-organizations)、[RD09](rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd09-membership-epochs)真实激活、工作、来源/贡献、当前权利 | 发表/模型输出变全部权利，换代表重绑作者，收到 slot 变原作者，旧 epoch 原件仅剩不可解析摘要 |
| 社会声明、认可/区域/治理事实 | 原声明/TTL/backing 或 program/合法决定/原 receipt 各自可回读，保留原状态含义 | publisher Confirm、confidence、任意 event 或通过提案不代签履约、使用权、认可、岗位或准入 |

组织 O 的代表 A→B、P 的 Agent p1→q1/换 owner，分别在原事实前后发生。独立从原 commit 的主体、mandate 和 controller 原件建立预期映射，再对比查询输出；不能用被测投影自己的 subject 字段作为 oracle。O 的历史属于 O，A 的个人承担只来自原显式接受，B 不取得 A 的个人履历；p1 的原行动仍可读但不计入新 owner 的取得用途。

以 public、原个人、现组织代表、已离任代表、现 Agent owner、无关人和获准 panel 分别读取，核每个受众能读的真实字段及覆盖态。历史读取需要当前合法读权，但不能要求原当事人重新任职；没有当前资料权限的后任不得取前任私人目标/资金/模型原件。保留完整来源链的受限引用及当前查询结果，公开报告不泄露秘密。

<a id="rep10-acquisition"></a>
## REP10：真实正向取得来源、独立计算与原扩槽

依据：[唯一采用政策](../../game/gameplay/gameplay-contextual-reputation-contract.prd.md#contextual-reputation-consumer-policy)、[用途计算](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-purpose-admission)。必须使用真实 immediate_exchange_v2 与原 claim/制造消费者；fixture 只可核 schema，不能证明可达扩槽。

1. 固定 P 与 Q1…Q5 的真实稳定主体、各自 Agent/控制任期和非共享 source-owner 集合。五份 immutable terms/render 明示同一 source_projection_use 的 schema、purpose、context、audience、采用 policy/version/hash 和双向非零 predicate；所有必需方签署同版正文。用原账本记录各 resource kind 的前后本金、税费/执行费和实际 treasury，不能仅断言 success。
2. 在合法时点依次真实完成五份交换，例如 origin ticks 100、101、102、103、104；同 tick 顺序另有变体。用独立表格从原完成 receipt、原条款/owner、关联证明和当前有效更正列出唯一 origin/对手。当前 Q=110 的数列应为 0→5→10→15→20→25，对应 cap 1、1、2、2、2、3。给 source 原件不变的客户端请求任意声称9、24、25，均不能代替该派生表。
3. 在取得所需资金、主绑定/工厂/材料与原其他条件真正满足的同候选，分别实际 quote/确认普通 claim、制造预留及交付；核第2、第3容量正向可达与超 cap 拒绝。同步尝试另一路 writer 争抢末个槽，按 canonical 顺序只有原共享集合允许的结果；不得两路各占最后一槽。报价不消费容量，制造交付将原预留替换为实际 Agent。
4. Q=699仍有五条；Q=700排除 tick100、值20/cap2；Q=703只剩tick104、值5/cap1；Q=704全部到期。观察的 cutoff 由当前 canonical tick决定，不能用客户端日期。让旧25分quote跨到700再确认，第3新取得不能沿旧quote通过；已持有 Agent/原维护债务不被回收或免除，已在途制造走其原有界等待/退出。
5. 重试原 request、改 request ID、同 origin 多视图、同对手新 Agent/别名/组织代表、多份实际同对手交易、共享资金 owner 的多个组织、某关联证明未知分别运行。不得按别名或拆单增加单位；对手之间也需独立，按原规范顺序选取且可重算。真实履约历史仍保存，unknown 不变负面记录。
6. 原 authority 合法更正/撤销或原 linked 冲销使某 origin 不满足 predicate 时，重新计算；单纯自愿反向交易保留原事实但不给同窗第二次对手单位。单方投诉或未决责任不自产扣分；来源当前有效性确为未知则当前用途 unknown。退款只有原 authority 的真实关联冲销才能改变该来源，不能客户端写 refund=true。
7. 用旧 owner_atomic v1、native 一腿旧 delta、war/setter、两独立 transfer、双腿中一腿0、单方/双接受 ACK、未签 preview 附言、签后改 audience/policy/predicate、正确正文配错误摘要逐项对照。均不能得到新单位。普通 v1 旧无奖励/资格含义保持，不能靠政策升级追溯采用。
8. 分别测试完整已证空集、source family未启用、当前覆盖缺页、私有源缺合法断言、当前主体/更正链未知。只有完整空集可称0；unknown不能发高档，但独立原slot1通路在自身全部条件满足时仍可用。不得冻结无关运输/原债务处置或将未知显示成“未合作过”。

证据包括原 v2 完整签署、实际两腿/费用/receipt、身份/独立性的准确范围、独立去重/窗口计算、全部 writer 的真正前后集合和结果。没有两个及五个真实有效对手的正向消费者证据，本项不得标通过；“所有来源都拒绝很安全”不证明原扩槽能力。

<a id="rep11-consumers"></a>
## REP11：四类旧分数消费者与所有已采纳机会

依据：[全消费者表](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-all-consumers)。先由被测部署的原 manifest、源码入口和调用链登记实际消费者清单，区分业务采用与序列化/display；再从各自原合同计算独立预期，不能只断言新字段存在。

| 消费者 | 正向/历史判据 | 必需反例 |
| --- | --- | --- |
| claim 与制造 | REP10全部quote/confirm/direct/自动writer/预留/交付共用当前basis和去重集合；原10/25、1/2/3与价格保留 | 只改UI、另一路用旧score、quote后覆盖漂移、预留/交付各占一次 |
| 治理 | 在真实 active/warmup/stake/min/max 条件下独立算新权重，经济/war/咨询加成为明确未采用的0；在该边界前已有提案的原snapshot/原票仍不变 | 将未知stake当0、将取得25单位加到票权、回写旧提案、缓存当前权重覆盖冻结snapshot |
| native 与 WASM war | 历史按各原版本完整公式和事件回放；目标采用仍遵守War Defer/原冲突authority | 咨询/新acquisition单位进入战力；以“代码仍读旧score”声称已迁移；本案设计放开新的战争写口 |
| 地方认可/物流机会 | [原机会验收](recognition-opportunity-acceptance.manual.md)的真实program/审核、普通与认可共享容量、一次claim/Use/完成/恢复仍通过；同源索引不替审核 | 无审核凭咨询领取、续窗口/复制容量、组织视图代领个人recognition、unknown后新nonce重领 |
| pioneer/FI、区域/设施/工业/许可/价格 | 原权利、scope/有效期/current gate及各自普通通路独立成立；新咨询无隐含折扣/授权 | generic recognition随收益份额转让、全球score补工业许可、声誉满额免材料/OC/成本 |

同一真实五动作 publish/challenge/adjudicate/declare/update 的只读预览按 [社会预览手册](social-action-consequence-preview-acceptance.manual.md)另验：每个实际consumer要能给采用且变/采用但不变/已证未采用/未知的准确结果，不从正负权重或schema文本推算。source、采用版本或新增已部署consumer未纳入清单时为覆盖未知，不能据上表固定六行宣称覆盖无限未来插件。

<a id="rep12-review-recovery"></a>
## REP12：一般主体有界纠正、原程序与恢复

依据：[完整复核](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-full-review)、[版本与恢复](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-versioned-adoption)。

用同一 org/role/service/RD 来源各构造：可唯一确定的错主体/重复/漏更正、完整证据证实无误、缺必需原件、实质责任争议。前两者分别是一次精确projection correction或no_change；缺源为未知；责任只能定位原有权事项/原案/原截止。不得为同一争点在声誉、服务、RD和许可各开一案。托管事实继续原一次缓存校正，不能多出60tick窗口或业务Action。

原尝试受理650时最后可执行tick709、710关闭；把补证、换request、换代表、无关root前进和重复同材料跨该边界，各自必须保留原窗口/结果。真正新source revision/新的可核证据可按原规则新尝试，但旧origin寿命和旧实质案件不刷新。已离任代表失去新写权，组织原请求及其有权历史读取不消失；原个人不被当前组织代表取代。

四层分别注入缺cache、source读未知、原业务请求ACK丢失、World持久generation StatusUnknown。只读cache恢复不改世界；业务unknown按原scope先查；World unknown fence整个旧World的写/tick/save/依赖派发。对实际持久决定的三分支独立核对：Committed安装精确原候选及receipt；NotCommitted才弃候选保留旧代；仍Unknown保持fence。另测相关截止过去、代表撤权或版本升级均不能使未知自动变无效果，不给原已提交世界动作新nonce重做。

记录所有原程序的真实授权、随机/受理/截止/final/实际补救及范围，不把提出申诉视为推翻初审或自动赔付。历史原件须实际按授权取回；仅剩digest、空返回或脱离原audience的公开原件都不合格。

<a id="rep13-composition"></a>
## REP13：有界完整闭包、三入口及整卡组合

依据：[完整预算](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-full-budget)。用真实 source adapter 展开 REP10 五对手及 REP09 每族常规闭包，记录对象数、完整canonical输入bytes、解码累计bytes、每批次数、真实查询/效果cut和coverage。分别造32KiB/16refs/50行/64对象/1MiB/32批/2048对象/32MiB的边界及首个超限输入，任一上界先到就停止；超限返回unknown/over_budget，不按已读正值给高档。

把一项合法更正、共享owner证明或撤销放在最后一页/最后允许对象中；只读前五个好记录的实现必须被判失败。合法五对手正例闭包必须可编码、可验证、可真正通过新取得；若格式使正例根本无法完成，记录本采用不能启用，不能用全拒或截断proof冒充覆盖。原件可受权逐项读取，紧凑索引/root必须覆盖全部已登记来源与更正。

填满普通新读/新纠正容量，再执行原已受理请求查询、补证、程序关闭和业务恢复，验证保留队列/预算不被新流量挤掉。query超限不吞原程序期限、不新建业务押金、不改变实际资源；若管线需要收费module才能完成原免费纠错，记为实现前置未满足。

最后以同一 tested tree/manifest/cut 对齐 API、Agent、Viewer：事实/声明、角色任期、actual/final/paid、当前/历史、用途已采用或未采用、verified-empty/unknown和可行下一步逐字段同义。正式按钮只通向原实际动作，quote不替执行；真人任务必须能解释为什么同一已履行记录可用于合作参考却不能变政治/战争加成，为什么新owner/新holder不继承个人credential，以及到期只影响新取得。mock/API通过不能代签Viewer或真人。

整卡判定同时列出原五动作预览、CR001形成、CR002全服务、原托管组织事项、RD01–RD10、原认可/机会和本REP01–REP13的实际范围。各方法可以并行执行，但缺一个已采纳来源族/消费者/恢复闭包须如实标partial或blocked；不得再以单一新profile或一份静态表代签完整CR003。静态设计评审结论与真实业务、性能/平衡、多人/真人结果各自记录。
