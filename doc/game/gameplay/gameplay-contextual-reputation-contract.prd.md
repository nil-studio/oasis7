# Gameplay 合同履约的情境声誉政策

- 生命周期：`active`（本轮可评审的目标政策，尚不是已启用能力或经玩家验证的参数）。
- Owner role：`gameplay_designer`；协作：`producer_system_designer`、`runtime_engineer`、`agent_engineer`、`qa_engineer`；Last reviewed：2026-11-04。
- 专业归属：game 专业域的合作前事实咨询政策，消费上层 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) 与 [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)。本专题不新建产品模块、PRD-ID 或 Trace ID。
- 执行与验证：[情境声誉证据及复核设计](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md)、[验收手册](../../testing/manual/contextual-reputation-acceptance.manual.md)。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）；本文唯一拥有本 profile 的来源选择、用途、时效、玩家选择和复核政策；逻辑身份、投影、数据权限、事务与恢复由上述 Runtime design 承接，不另建同名系统设计。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。只承接合同履约咨询切片的主体/情境、到期更新、复核及转让边界；完整社交事实预览、组织/岗位声誉、R&D charter/份额和机会消费仍按上层原范围逐项闭合。

<a id="contextual-reputation-policy-v1"></a>
## 1. 首版目的与适用范围

`contextual-reputation-policy-v1` 帮助玩家在形成新合同前回答具体问题：这个签署者或这个 Agent 的本次控制任期，在相同地点和同类服务中，有哪些可验证的交付、未定责任、最终责任和补救事实？玩家可以查看来源、比较其他合作方、缩小新承诺、要求更合适的合同保障、等待或放弃。读取履历不替玩家接受条款，不保证对方将来履行，也不把缺少记录解释为不可信。

首版只服务于合作前只读咨询，支持两个已定义个人合同 profile 的合法来源。它不授予合同成立、资源、资格、排名、奖励、工业准入、治理权或 claim 权；更广的机会与资格消费者必须由其原 authority 独立设计、授权和验收，不能接入此查询便默认获得一套评分制度。

| 来源 | 可以表达的事实 | 必须拒绝的推断 |
| --- | --- | --- |
| `owner_atomic_resource_exchange_v1` | 同一合同原子完成的完整双腿及相应签署、授权和结算来源；两腿共同构成一次履约事实 | 单腿、提议、接受、action ack、旧 native `EconomicContractSettled` 不能补成该 profile 的完整交换 |
| `owner_checkpoint_power_service_v1` | 精确已交付单位、终止处置、有效 final 责任及实际到账补救；保留各自 receipt 和原单位关系 | 检查点已到、尚未履行、初审待复核、中性尾部或未到账承诺不能被说成最终失约、免责或已赔付 |
| 其他来源 | 显示 `unsupported_source` 或已明确的原来源状态，允许按原权限查阅 | 任意 social claim、消息、模型描述、war/native 分数、任意 event ID 或组织成员身份不成为本 profile 的履历证据 |

两个合同 profile 本身仍有待交付的 Runtime/托管/正式入口能力。本政策不声称当前 main 已有完整可用来源，不为缺失的历史字段补造授权、控制任期、两腿结算或责任裁决。

<a id="contextual-reputation-policy-subjects"></a>
## 2. 主体、情境与相关方

玩家签署责任和 Agent 行动履历分别归属。玩家视图指向当时实际签署并承担义务的个人主体；Agent 视图绑定该 Agent、当时控制来源及控制任期。新 owner 可以按合法权限了解旧世界历史，但不会因接手 Agent 而获得前任个人信誉或承担其责任。原个人也不能通过换 Agent、重签或资产转让清除自己的已确认事实。

首版不支持组织或角色/岗位声誉；查询必须明确返回不支持，不能把成员的个人记录、Agent 或 `Location` 冒充组织/岗位。后续扩展须另定代表权限、责任及转换边界，不得静默解释旧记录。

每次咨询须明确同一 canonical world/branch、地点、服务 profile 和服务目的；结果继续披露原合同、发生时间、对手方及受影响群体。不同地点、服务、世界或任期的事实不得混成当前查询的结论。玩家可另行查看有权限的历史背景，但不能用扩大查询条件获得跨情境万能信誉。

相关方关系只表达有来源的已知关联、在适用证明范围内的独立性或未知。不同 Agent、不同地址、没有查到关联均不证明独立。分单、重复 receipt、retry、split、修复和显式继受沿原 agreement lineage/不可重复单位关联；这些不能被计为多份独立成功经验。真实重复交易可以分别展示，但必须保留同一对手及关联性，首版不汇总成功率或将交易次数变成评分。

<a id="contextual-reputation-policy-lifetime"></a>
## 3. 时效、补救与历史

首版统一采用 `H = 600` 个 canonical world ticks 作为本咨询 profile 的当前相关窗口：`[origin_tick, origin_tick + H)`，加法必须检查溢出。`origin_tick` 是该结果首次权威生效的 tick，不能由客户端、索引器或事实发布者重选。正向交付和负向 final 责任使用相同窗口；它不是现实分钟或服务 SLA，也不改变原合同、债权、申诉或世界历史的期限。

到期只将事实移出“当前相关”集合。按原数据权限仍可读取历史、有效责任和 receipt；到期不清债、不证明当前可靠，也不形成永久黑名单。晚建立索引、转载、重新登录、转让、重签、更正或补偿均不刷新原事实的窗口。

交付、未履行但尚未定责、有效 final 责任和已到账补救必须分别显示。补救追加与原责任及单位的关联，不能删除原失约、把未付款标成已补偿或把补救再计为独立成功交易。补救可显示自身发生时间；“原责任已补救”的当前相关性仍沿原责任窗口。后来独立形成的 final 责任使用其真正首次生效 tick，同时保持原合同因果关系，不能将同一履约经验重复计数。

| 静态算例 | 应当看到的结果 |
| --- | --- |
| 一个交付事实于 tick 1000 生效，tick 1599 与 1600 查询 | 前者在当前窗口内，后者仅属于可授权读取的历史；窗口不是闭区间 |
| 原责任 tick 1100 生效，tick 1550 实际补偿 | 原责任到 tick 1700 退出当前窗口，补偿不把截止改为 2150；责任和付款分别可查 |
| Agent 的任期 7 于 tick 1300 结束，任期 8 由另一 owner 开始 | 原个人责任仍归旧签署者；新 owner/任期的履历不继承前任结果，旧 Agent 历史按原权限另列 |

上述数值是为本次目标规则选定的可实施初值，没有玩家理解、节奏或经济平衡实测支持。调整时必须建立新政策版本并说明新记录适用边界；不得追溯刷新旧记录、借延长窗口制造永久负面标签，或悄悄扩大用途。

<a id="contextual-reputation-policy-review"></a>
## 4. 客观投影更正与原责任申诉

记录主体或依法可读取且受该记录影响的当事方，可以提出主体错配、遗漏权威 receipt、重复单位、来源失效或时效计算错误。声誉层只依据完整可验证来源确定投影差异，不设“声誉管理员”或由发布者裁决事实。确定错误时追加关联更正及版本；原权威合同记录与历史仍保留。即使当前相关窗口已过，客观投影错误仍可被纠正。

一次复核尝试从首份完整请求所绑定的 canonical tick `t0` 起具有 `[t0, t0 + 60)` 的提交窗口。补证、重连和换请求 ID 不重置起点；相同来源、同一错误及同一证据复用原结果。输入和分页的具体上界由 Runtime 专业设计唯一维护，不能用堆叠无关合同扩大一次复核。

可确定的差异按专业事务合同提交一次。来源不足返回 `not_established`，无法读取所需证据返回 `evidence_unavailable`；到期仍不能确定可提交差异则返回 `review_incomplete`。这些状态均不裁定原事实真伪，不隐藏仍有效的事实，不建立 hold、处罚或额外权力。后来取得新的可验证来源或来源更正，可以开启基于新证据的尝试，但不能凭相同证据无限续窗。

已经提交而结果未知的更正必须查询、恢复原请求；60 tick 截止不能被用来宣告原操作无效果、删除去重记录或重复写更正。恢复所需时间不是本窗口保证的完成时限。

对履约责任、程序错误、利益冲突或赔付结论的异议，回到原服务合同有权程序及其原有限申诉窗口。这里不新增第二次责任裁决、额外申诉轮次、延期权或无限保全，不把复核申请本身视为推翻原有效结论。

<a id="contextual-reputation-policy-reading"></a>
## 5. 玩家可读内容与决定

合作前查询应同时显示主体/任期、查询情境、政策版本、覆盖范围和新鲜度，逐事实的来源、结果、责任/补救关系、到期与历史状态，以及真实可用的来源查阅和复核入口。完整且为空、仅取得部分记录、来源不可验证必须区分；缺证据不能写成“零失约”或“值得信任”。

推荐动作仅解释玩家可以继续比较、缩小承诺、选择其他合作方、等待新证据、请求客观更正或进入原合同救济。不能将这些咨询动作变成签约、拒绝对方使用全世界服务、自动处罚或机会保证。Viewer、API 与 Agent 在同一权威快照和 audience 下须给出相同事实、时效、blocker 和下一步；语言表达可以不同，权限与结果不能不同。

原社交事实发布/质疑/关系声明的后果预览继续归原专业 authority。本 profile 只提供实际接通的咨询来源；对未连接的治理、claim 或机会消费者必须明确 `not_connected/unknown`。旧 quote 中固定的 `positive/contested` 文案不能作为真实资格影响或声誉事实。

<a id="contextual-reputation-policy-done"></a>
## 6. 专业验收与证据边界

同一候选必须能逐项证明：合法来源与自由声明分开；玩家/Agent/任期不混同；世界、地点、服务和目的准确匹配；分单/重复/补救不制造独立经验；时效边界与历史、责任、付款各自保持；客观更正、责任申诉及未知提交有准确下一步；读取和预览无业务效果；旧分数不能被资格、治理、claim 或工业入口当作本 profile 的替代结果。

[Runtime 设计](../../world-runtime/runtime/contextual-reputation-evidence-and-review.design.md)承担逐条反向分配，[验收手册](../../testing/manual/contextual-reputation-acceptance.manual.md)承担输入、步骤、独立预期和证据位置。缺实现或正式入口记为 `blocked/not_run`；静态算例、文档检查或测试定义不能替代业务、跨端、恢复和玩家理解证据。

本切片不完成整个 CR-003。组织/岗位与其他社会来源、完整一般关系/争端、预声明 R&D charter/份额转让和成熟世界机会消费继续属于原父项；它们不能以本咨询 profile、无评分选择或后续任务替代应有设计。
