# Gameplay 预工作 R&D charter、贡献归因与份额转让合同

- 生命周期：`active`（本轮提出的可评审目标政策；未治理激活、未实现，数值未经玩法或性能实测）。
- Owner role：`gameplay_designer`；协作：`producer_system_designer`、`runtime_engineer`、`agent_engineer`、治理/P2P owner、`qa_engineer`；Last reviewed：2026-11-04。
- 专业归属：game 的个人合作研究、发现和发明首版规则，承接 [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003) 和 [AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003) 的 R&D 切片；工业收益接口消费 [REQ-WR-GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)。不另建产品模块、PRD-ID 或 Trace ID。
- 专业执行：[R&D charter、provenance 与转让设计](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md)；验证：[R&D 验收手册](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md)。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）。本文拥有目的、玩家选择及本 profile 的玩法参数；接口、权限、状态、费用上界、持久化和恢复由上述 Runtime design 承接。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。本切片不完成一般社交事实、组织/岗位声誉、成熟世界机会或整个 CR-003。

## 1. 玩家目的与四种不同的事实

合作方在投入研究工作前，应当知道自己要做什么、怎样留下贡献、哪些成果有权使用、有什么经济约定、如何退出或质疑结果。玩家可以加入、要求在开工前改条款、明确接受零收益贡献角色、另做独立工作，或拒绝。参与并不保证发现成功、工业准入、付费许可、版税或未来价值。

| 事实 | 本合同的表达 | 不随该事实自动产生的结果 |
| --- | --- | --- |
| 历史贡献 | 当时的主体、Agent/控制任期、授权、工作、来源和结果 receipt | 不按工时、模型评分或声望自动发份额 |
| 成果使用权 | 精确来源及允许使用、修改、发布或许可的范围 | 发布者、artifact owner、材料持有人不因此拥有全部合作成果 |
| 经济分配 | 开工前同版接受的稳定 benefit slot、正整数 bps 和明确收益范围 | 不改写作者、贡献事实、个人身份或一般声誉 |
| 个人 credential | 沿原身份、政治/社会资格及各情境 authority | 取得 Agent、成果或收益份额不继承他人的资格、责任或全球信用 |

本首版支持同一 canonical world/branch 中，自然主体或其当时受权 Agent 的一个有界工作范围。组织、岗位代表、开放加入、跨世界归因、长期受托服务及现实著作权/专利裁判不在本 profile；不把个人签名或新的 charter 当作组织代表权。它们仍保留原产品义务，由对应 authority 另行承接，不能以本首版替代。

<a id="rd-charter-policy-v1"></a>
## 2. 首版政策：`personal-rd-charter-v1`

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；上游：[CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。以下参数只在本文维护，Runtime 和手册按本版消费。

| 规则 | 固定政策与边界 |
| --- | --- |
| 必需方 | 至多 16 个去重的自然权利/责任主体，包含全部贡献方、原收益人、需要本次新同意的来源权利人和本次承担费用的主体。身份别名和多个 Agent 不扩容；第 17 方使新激活拒绝，不能开工后删去该方。已有完整许可且本次未增加义务的第三方沿原许可处理，不伪造其接受了本 charter。 |
| 收益向量 | 至多 8 个稳定槽；初始受益人互异且属于必需方，权重为正整数 bps、合计 10000。零收益贡献方不占正权重槽，须明确接受零收益而保留归因、证据、退出和程序权利。向量是事前约定，不以实际贡献评分自动改权。 |
| 工作范围 | 一份 charter 固定一个工作 scope、目标、角色名册、来源规则和至多 32 个可识别工作单元，最多交接一个经工业 authority 确认的 capability lineage。它不预占公共想法、资源或独立成果。 |
| 接受有效期 | 同一条款 offer 的全部接受仅在 `[Q,Q+60)` 内可用于激活；`Q` 为原 quote 的 canonical tick。缺同版接受、当前授权或时序证明均不能激活。重新报价取得新的完整接受，不续用旧签名。 |
| 工作窗口 | 激活 tick 为 `T`，新工作及其获准完成范围为 `[T,T+600)`。不自动续期。首次实质工作必须晚于有效激活提交的世界顺序；同 tick 仍比较 canonical 顺序。 |
| 开工后的变更 | 原 scope、工作角色名册和经济向量冻结。撤回自己后续工作授权、追加真实结果或纠正记录不重写原约定；替换参与者、扩 scope 或改变 bps 不属于本版。新的独立工作须先形成自己的合法约定，不能迁走旧事实、伪造独立 lineage 或重置原收益池。 |
| 归因通知 | 工作关闭时原子发布一份原结果/贡献目录和 canonical 通知 `N`；登记异议窗口为 `[N,N+120)`，程序基点 `C=N+120`。不以在线、已读或第一次读取重新起算。 |
| 争议程序 | 原名册最多 64 个去重 controller，激活时至少有 6 名可验证无冲突候选；随机初审 3 人、独立复核另 3 人，每阶段至少 2 人同意同一输入和类型化结论。每方每阶段最多 16 个不同权威证据引用；只有一次程序申诉。具体截止见 §6。 |
| 额外经济规则 | 本版不收研究入场费、申诉押金或额外 panel 奖金，不铸造奖励、征收 levy 或预支未来版税。实际执行费用和完整程序/退出预算仍须在承诺前由真实 payer 明确同意、足额保护；研究生产投入另走原消费者。 |
| 转让 | 仅整槽的全部未付权益及后续权益一并显式转让；不拆 bps，不把已付历史追回。v1 拒绝受让人在同一原分配中已经持有另一槽，不合槽。额外售价或附条件交换不在本次无额外对价的转让 profile，不能先收价款再人工改归属。 |

所有窗口均采用 canonical world ticks、左闭右开区间及 checked 运算，不保证现实分钟或客户端送达时限。16 方包含实际需要本次同意的费用主体，不能假定调用者、贡献者和 metering payer 是同一个人。容量不足只阻断新承诺，不抹掉既有记录或程序机会。

只读检索、比较旧成果、非绑定协商和登记工业提案不算本 scope 的实质工作；它们不能产生新的可归因研究结果。为本 scope 新生成模拟分析、实验、代码、训练/微调或其他研究成果，是实质工作，须先激活并沿受支持的工作来源执行。事后上传文件不证明它在 charter 后才被创作；无法证明时只能声明为先存来源，不能倒签贡献或补造旧收益。

<a id="rd-contribution-and-provenance"></a>
## 3. 贡献、来源与 Agent provenance

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；上游：[CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

归因方法是事前列明的角色、工作单元、可接受来源和可观察结果判据。贡献记录区分 `claimed`、有 canonical 证据支持的事实、证据不完整、被有权更正及争议中的范围；自由文字、模型断言、artifact owner 或一般治理票不能升级为已验证贡献。验证成功证明记录与来源一致，不证明科研结论正确或未来有商业价值。

每项 Agent 贡献保留稳定 Agent 身份、当时控制主体/任期及授权、使用的模块/版本和来源、实际训练或微调输入的可验证记录、provider/model 的明确版本或已知范围、工作结果与关键 receipt。提供方没有披露训练来源时准确记录 `not_disclosed/unknown`；模型可调用、输出可读取或存在一个 hash 均不证明训练资料全部获许可。用于特定成果的使用权仍须满足其真实来源合同，不以“未知”伪造通过，也不据此对所有贡献者作负面信用判断。

先存代码、数据、模型、素材和第三方成果分别保留原作者/权利来源、精确版本及适用使用条件；原许可已允许的使用与需要新同意的事项分开。缺少本次必要权限时停止相应工作或交接。R&D panel 不能替未参加的权利人授权；原来源的权限、撤销或第三方争议回到其原有权程序，不能靠原名册上限删除真实权利方。

Agent 转让、重配置、退休、离线或报废后，旧控制期贡献、训练/模块来源和关键世界结果仍可追溯；新 owner 不自动取得旧收益或个人 credential。永久 provenance 表示历史因果不被生命周期操作覆盖，不表示无限公开私人材料或永久在线 SLA。公开目录、当事人/获选 panel 的受权证据和秘密原件分层读取；原件不可取得时如实显示不可核验，不能用摘要补成完整证明。

## 4. 工作、停止与合法交接

| 阶段 | 玩家可以做什么 | 可读结果与边界 |
| --- | --- | --- |
| 草稿 / offered / 接受待校验 | 比较角色、零收益安排、来源、费用、窗口和替代路线；拒绝或重报 | 不占资源、slot、预算、工业容量或世界资格；不表示已绑定 |
| 激活后、尚未开工 | 在原 scope 内发起受支持工作，或各自撤回后续授权 | charter 只建立原条款与受保护程序预算；每次真实工作还需当前资源、设施、Agent 和执行权限 |
| 工作中 | 查看真实贡献/失败/未定结果，停止自己后续工作，沿原工作消费者安全结束 | 停止不消除已发生贡献，不扣掉固定份额，也不让别人替该主体继续；已有服务义务仍沿原合同 |
| 工作关闭 / 通知 | 检查完整来源、未完成单元、贡献记录、固定向量及真实 blocker；登记异议 | 关闭不是成功发现；未启动单元记为未执行，已启动结果未知则先恢复原请求，不能签“无效果” |
| 归因可用 / 交接受阻 | 对有充分依据的成果继续工业提案/模拟/准入、选择零费路径，或停止 | 完整付费交接只证明特定来源与原分配；缺工业准入、许可或收费 adapter 仍不能收费 |

到 `T+600` 停止发起新研究工作。已接受工作必须沿预先具备的有限退出、迟到结果 fence 和原请求恢复合同处置；没有这样的来源 adapter 不能开始。闭合预算不能依靠研究成功或未来收益筹集。可以在所有必需方同意且已启动工作均有确定处置时提前关闭；一方不再在线不能永久延长窗口或制造继续研究的授权。

交接保持历史贡献、成果使用范围、原冻结分配及当前持有人四项独立。登记提案只需当时有效的 charter 和已有证据，不强求尚未产生的最终交接。正式付费许可才消费完整、当前有效的交接；收入计量、硬开放、通知、累计 `E`、尾差、付款及收益案由 [GAME-018 许可政策](gameplay-industrial-creation-and-cross-region-market-contract.prd.md#industrial-license-policy-v1)唯一决定。

Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47；上游：[GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)。同 lineage 已有原分配时，新 charter 只能保留新增工作的真实归因，不能在本版另开付费向量或从 `E=0` 重算。需要改变 bps 的安排必须有另版政策及完整旧权益迁移，不能伪装成普通修改或份额转让。

<a id="rd-share-transfer"></a>
## 5. 整槽转让与旧权益连续性

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；上游：[CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。工业池接口同时消费 Trace refs: O7-REQ-0ec3d72f-51d1-4d64-9824-ebd9365f2d47 的 [GR-010](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-010)。

原合法 holder 和受让人须接受同一份只读预览中的 charter/slot、原 bps、累计收入与已付/未付账、完整转让链、原程序期限及尚存风险、当前收款权限和实际执行费用。只在原分配及成果来源已足以合法交接后转让；身份争议、未知付款或不能保持原程序机会时先恢复或处理相应范围。没有合法工业收入时可以转让有限的未来收益安排，但不能显示已经有收入或保证将来可收费。

生效是一个权威提交：R&D 的当前权利版本和已存在许可池的同槽 holder/版本同时改变。池尚未建立时保存当前权利 head；后来建池读取该 head，不复活初始 holder。多个许可和用途根仍读同一原池，不另开池或另建槽。付款与转让按同一权利版本串行；原付款结果未知先查询原件，不能让新 holder 再领一次。

转让保留原稳定 slot、bps、累计 `E`、已收费批次、未付/已付与尾差。原合法已付仍归旧人，全部未付及后续应得归新 holder；未能收款仍是该 holder 的个人应付，不退付款人、不交给新的 Agent owner。更换同一主体的收款 Agent 只改收款授权，不能冒充新收益人或重置账。

受让人不会变成历史贡献者；原工作名册、个人 credential、一般声誉及已发生责任不动。维护、服务、赔偿等义务也不随收益权移动；需要继受时由其原合同另行取得相关方明确同意。当前 v1 不支持分拆槽、转给同一原分配中另一槽的 holder，或附带原子价款的交易。

每个仍有程序权的原主体都要保留。转让预检逐原 notice/case 计算“原合法当事方、仍有对应权益的历次 holder 和新 holder”的并集，不以 current holder 替换历史当事人；新增程序槽及费用在生效前合法补足。许可的每案 `m≤10`、固定随机/窗口和完整预算仍适用，无法满足则拒绝新转让。历史保留没有人数截断；程序关闭只释放在途容量，不删除历史。原 R&D 归因案只处理冻结工作名册，受让人明示接受已结或剩余的原程序，不重新开启 R&D 窗口。

静态算例：原向量为 `5000/3000/2000`。`E=7` 时基础应得 `3/2/1`，第一槽此前合法已付 2、尚有 1 未付；整槽转让后 `E=11`，第一槽累计基础仍为 5，新 holder 可得 3。全部合法闭合后该原槽再得 1 尾差，最终旧人已付 2、新人共 4、另外两槽 3/2，总额 11。该算例只复核既有许可算法与转让的连续性，不证明实际到账或经济平衡。

<a id="rd-review-and-exit"></a>
## 6. R&D 专属归因程序、有限申诉与硬尾部

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；上游：[CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)，并消费同专题 §2.2 的 receipt-first、有界本地程序。其技术承接见 [rd-review](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-review)。

激活前须有原世界治理明确授予的 `rd-charter.review` 本地事项权限，固定 world/branch、region、政策、名册来源、可处理的来源/贡献/原槽身份范围和资金处分边界。charter 自身、普通 passed、服务合同 panel、管理员或 `industrial-license.review` 都不产生这项管辖。名册成员须有其原制度下的合法资格和同意；本政策不强迫无关人免费承担职责。

排除所有本案当事方、其可核验关联控制者、相关来源/工业批准者和直接经济关系人；“未查到关联”不能证明无冲突。案件承诺先固定，再取已获授权且事前不可挑选的可验证随机源的首个合格结果，形成唯一排列。初审和复核使用不同的合格人，失格者沿原排列顺延；重开案、换 nonce、补位或转让不能重抽。候选不足时保持明确程序缺口，按原截止和硬尾部处理，不能改成管理员裁定。

同一 charter 的这一归因通知只有一案；各方合格争点按原范围并入，不能由抢先者用尽后到者的窗口或预算。决定性 receipt 的错配、重复或授权错误先按完整原件重验，不把可确定错误交给任意多数。相同证据/争点的重复请求恢复原结果；新补证也不重置 `N/C`。

| 阶段 | 固定窗口或截止 |
| --- | --- |
| 登记 | 各必需方在 `[N,C)` 内登记；canonical 登记才算入窗，不以客户端发送、排队或 ACK 代替 |
| 普通证据 | 各方受理后至 `C+60` 前；每方至多 16 个引用。到期冻结完整已登记集合 |
| 初审 | 在证据冻结后至 `C+120` 前，发布可验证的类型化结论；该发布 tick 为 `P` |
| 一次程序申诉/复核证据 | `[P,min(P+60,C+180))`；针对权限、遗漏证据、冲突、程序越界或可验证错误；每方至多一次主张及 16 个引用 |
| 复核 | 全部合格主张统一在 `C+180` 冻结；在 `[C+180,C+240)` 形成复核。没有合格申诉的初审可在其原申诉窗口结束后 final |
| 硬尾部 | `C+300` 核对完整 canonical 输入/结论/和解/结果，完成本案有限结清；重连、离线、晚读及继受均不延期 |

允许结论仅为确认原基线、依原权威 receipt 追加贡献/来源事实的纠正、按原同版接受纠正未付槽的合法主体映射、标明某成果当前缺少必要使用权，以及解除错误程序保全。不能改 bps、授予缺失来源许可、追扣合法已付、增罚、造补偿、转移身份或把未履行角色自动变成失信。只纠正历史署名且不影响合法来源或原分配的争点，不阻断本来合格的独立成果/许可；首次付费交接所必需的某项权利尚不能验证时，准确阻断该交接，不冻结已有合法产业、无关资产或其他 charter。

有效初审没有合格申诉，或到复核截止仍未被有效替代时按原规则 final；申诉本身不推翻初审。有权 final 和合法同版和解保留执行。硬尾部能证明没有有效改变原约定的结论时，解除本案程序性保全，只沿原同版条款及实际已验证来源处理、返还未用费用；这不是“贡献争议已获实质裁判”。必需来源仍缺失则保留具体来源 blocker，不伪造成功交接。完整结果不可取得时进入原证据/提交恢复，不能把未知当“无裁决”或重复开窗；恢复困难不授权一方继续施加无限 hold。

R&D 案不裁定后续许可的完成数量、费用、付款、开放或其原通知下的收益争议。许可案只消费已核验原 charter、客观更正或显式转让证据，不能回头重审科研贡献、重分 bps 或重开 R&D 窗口。转错队列时返回精确原管辖、原对象和仍适用的截止；转介不代替登记、不保留一个新期限。两种程序不得为同一争点各创一轮裁决或两份保全。

## 7. 玩家读面、成本与可玩性问题

Viewer、API 和 Agent 在同一权威快照/受众下，分别显示条款是否有效、工作是否开始、贡献是否已验证、来源是否允许当前用途、原经济向量、当前 holder、真实已付/未付、程序期限、费用及下一步。明确区分“被认可贡献”“具备许可前置”“预计收益”“已赚取”“可分配”“实际到账”，不能合成一个成功标签。

确认前至少比较继续合作、零收益参与、缩小开工前 scope、采用有权先存成果、独立重新研究、零费公开或停止的成本和限制。零费也不授予他人材料、私有数据或工业准入。可恢复错误展示补证/原请求查询；已冻结向量与当前不支持的拆槽等限制直接说明，不能用无效按钮暗示可以操作。

完整程序的保守资源预留可能显著高于最终研究收益；实际执行费及完整闭合算例由 [Runtime 容量与费用](../../world-runtime/runtime/rd-charter-provenance-and-share-transfer.design.md#rd-capacity)唯一给出。资金不是入场奖励或已经消费的费用，未使用量按原来源返还。首版的 600 tick 工作期、16 方/32 单元、程序等待及拒绝合槽是本次明确选择，尚未验证合作意愿、研究节奏、经济承受能力或世界吞吐；不得宣传成已经济可用。

## 8. 专业验收与剩余边界

| 专业接受条款 | 必须证明的结果 | 精确验证方法 |
| --- | --- | --- |
| <a id="ac-rd-01"></a>AC-RD-01 | 同版全体接受先于实质工作；草稿无效果；不倒签、扩名册或借新 charter 洗旧 lineage | [RD01](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd01-activation) |
| <a id="ac-rd-02"></a>AC-RD-02 | 历史贡献、使用权、收益和个人 credential 分开；来源与 Agent 任期可追溯，未知和敏感原件不被伪装公开证明 | [RD02](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd02-provenance) |
| <a id="ac-rd-03"></a>AC-RD-03 | 非绑定提案不等待未来成果，正式付费消费完整交接；现有许可的 slot/bps/E/开放规则不被重置 | [RD03](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd03-handoff) |
| <a id="ac-rd-04"></a>AC-RD-04 | 整槽转让与池同根，先转后建池/付款竞争/历史当事增长均保持唯一 holder、旧账及原程序 | [RD04](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd04-transfer) |
| <a id="ac-rd-05"></a>AC-RD-05 | receipt-first、随机无冲突本地 panel、有限程序申诉、最小保全和硬尾部；R&D/许可不双重裁决 | [RD05](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd05-review) |
| <a id="ac-rd-06"></a>AC-RD-06 | 完整费用、当前执行权限、旧数据及未知请求恢复有界且不丢历史、不重复世界效果 | [RD06](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd06-recovery) |
| <a id="ac-rd-07"></a>AC-RD-07 | 玩家能辨认状态、成本、原截止和可行选择；三入口同义，研究/转让不变成信用或资格 | [RD07](../../testing/manual/rd-charter-attribution-and-share-transfer-acceptance.manual.md#rd07-reading) |

以上条款沿用本专题两项上游 Trace，未新增独立产品要求。当前只交付政策、专业设计和方法；真实来源 adapter、治理授权/随机、资源保护、原子双消费者、正式表面与同候选验证仍须实现。缺前置记为 `blocked/not_run`，静态算例、文档检查或示意界面不代签实玩、吞吐、权利核实或发行 readiness。
