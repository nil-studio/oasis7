# 合同情境声誉与客观复核验收手册

- Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`。
- 上游：[CR003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；规则唯一来源：[Gameplay 情境声誉政策](../../game/gameplay/gameplay-contextual-reputation-contract.prd.md#contextual-reputation-policy-v1)。
- 专业合同：[事实投影与客观复核](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md#reputation-source)。本手册提供方法与独立判据，不表示已执行或当前入口已支持。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。证明范围仅为两个个人合同 profile 的合作前咨询、客观纠正和消费隔离；不代签组织/岗位、全部社会事实预览、一般争端、R&D 或完整机会组合。

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
5. 请求 organization/role kind，或把 Location/成员 Agent 当组织；预期 `unsupported_subject_kind`。不能通过修改显示名称进入个人记录查询。
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
