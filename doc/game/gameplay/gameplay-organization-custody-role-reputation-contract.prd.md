# Gameplay 组织托管返还与接替岗位情境咨询政策

- 生命周期：`active`（本轮可评审的目标政策；不是新咨询入口已实现或经玩家验证的声明）。
- Owner role：`gameplay_designer`；协作：`producer_system_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`；Last reviewed：2026-11-04。
- 上游：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)、[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)。本文不新建产品模块、PRD-ID 或 Trace ID。
- 执行与验证：[组织托管及岗位咨询设计](../../world-runtime/runtime/organization-custody-role-reputation.design.md)、[验收手册](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md)。原授权、返还与救济仍沿[组织连续性政策](gameplay-organization-continuity-policy.prd.md)及[专业设计](../../world-runtime/runtime/organization-continuity-and-disposition.design.md)。
- 设计适用性：`simple-topic-exemption`（`PRD-only-sufficient`）。本文唯一拥有本 profile 的来源选择、用途、主体/情境、时效、受众和玩家决定；来源证明、查询、投影校正、容量和恢复归配套 Runtime design。
- 固定审读输入：`803e81524e29a988b40f8f642776a2c3c03ab085`。基线已有组织登记、有限接替及托管返还的窄执行路径；本次未运行代码、正式入口或玩家验证。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。只承接 CR-003 的组织托管与有限岗位情境咨询；该要求整体仍为部分覆盖。

<a id="organization-custody-role-policy-v1"></a>
## 1. 目的、范围与玩家决定

`organization-custody-role-policy-v1` 帮助有权查看的玩家回答两个具体问题：这家已登记组织在这份托管事项中实际返还了什么；这位接替者在这次已生效的有限任期内实际完成了哪些返还。玩家可以核对自己的原件、确认到账记账、查看当前范围、等待未明结果、请求客观校正，或进入原组织救济。咨询不给接替候选排序，也不替玩家选择、任命或接受下一项合作。

首版组织视图仅针对已登记 `OrganizationContinuity` 的精确案件和托管事项；岗位视图仅针对真实 `CommitHandoff` 产生的有界接替任期。组织存在、候选申请、成员关系、当前 `appointed` 标记和岗位名称各有原含义，不能单独证明任期已生效或实际返还。普通组织签约、开放岗位、任命资格、一般维护业绩和组织偿付能力不在本 profile。

这是有来源的事实咨询，不产生评分、排名、一般信用、治理权、工业准入、资源/claim 资格或新的处置权。个人履约记录继续采用[个人情境声誉政策](gameplay-contextual-reputation-contract.prd.md#contextual-reputation-policy-v1)；贡献与经济份额继续采用[R&D 政策](gameplay-rd-charter-attribution-and-share-transfer-contract.prd.md#rd-charter-policy-v1)。三者不合成为一个主体总分，不相互代签来源或转移个人 credential。

## 2. 来源与可以成立的事实

每条可用记录必须能回到原世界、登记、案件、托管来源、当时身份/权限和实际结果；同一当前读面还须说明其权威快照与覆盖范围。当前 case 可能覆盖旧任命字段，不能拿 current head 倒推历史。

| 必需来源 | 在本 profile 中的作用 | 不能据此声称的结果 |
| --- | --- | --- |
| 原组织登记、charter 及 `controller binding` | 确认组织稳定身份、所用登记版本及当时有权主体 | 组织登记不证明活跃、可信、偿债或一般代表权 |
| 原 `DepositOrganizationEstate` 与 held-in-trust 资产 | 定位本托管事项、原材料/数量、contributor 接收账本及案件范围 | 入账、清单存在或当前余额不等于已经返还 |
| 原候选接受与已提交 `CommitHandoff` | 确认接替者当时控制来源、精确生效任期、范围及期限 | 申请、推荐、未生效接受或现在同名候选不等于该历史岗位 |
| 实际 `SettleEstateAsset(operation=ReturnCustody)` | 在原合法路径和完整结算证明下，确认这份 held-in-trust 资产已向原记录 contributor 账本返还 | ack、请求登记、超时、普通 transfer 或别的处置操作不等于返还 |

首版沿原执行范围读取：实际返还的是完整托管项，数量等于该项原量，唯一分配目标是其原记录 contributor。其他债务清偿、一般业务转让、剩余分配或假定的部分托管返还不纳入。原 `disposed/effect_receipt`、来源去重与材料账的同次结果必须一致；单一布尔值不证明结果。文案只说明实际记入了哪个受权可见账本、什么材料和多少最小单位；不能仅凭 Agent ID 推定某个原自然人已经取得全部权益或组织所有责任已结清。

组织视图接收原返还操作的两条合法路径，并逐笔区分：

- **原任命 authority 直接返还（`appointment_direct`）**：须证明当时的登记/controller、规定的处置阶段、原治理和全部资产 gate 成立。它是组织托管事项的结果，不归入任何接替岗位。
- **有限接替任期返还（`delegated_term`）**：须证明实际 actor 是原已生效任期的接替者，资产在该原 scope，执行时原期限、资格、阶段及无重叠 stay/quarantine 等 gate 均成立。组织与该岗位视图可引用同一结果，不算两次返还。

当原 handoff scope 包含该资产时，必须成立 delegated 路径；actor 同时列在 appointment authority 中也不能绕过。两路都需要原治理授权，不以本咨询重新授予。路径或原执行切点无法核验时是来源不完整，不能凭名字、当前候选、现 owner 或成功文字猜出归属。

两路是本版的目标来源白名单，各自仍须证明正式生产入口及完整前置。基线的直接返还分支尚不能沿现有处置推进链到达 `CustodyReturn`；测试注入处置阶段只能核验分支，不能证明正式来源已经可用。实际入口能力与其缺口由配套 Runtime 设计和验收手册逐路记录。

范围外的个人合同、社会声明、LLM 总结、war/native 分数和 R&D 原件各保留原含义，不能通过 `schema_id`、组织标签或转载进入本 profile。旧来源缺少必要历史证明时如实不可用，不回填任期或实际返还。

## 3. 组织、原任期与情境分别绑定

组织记录以稳定组织身份关联，并明确 world/branch、原 registration receipt、case 及其版本关系；组织改名、重新登记或换负责人不删除旧记录，也不把不同登记/案件的事实混为一份当前经营履历。

岗位记录绑定同一组织/登记/案件、**原生效 `CommitHandoff` 原件**、原 scope 与候选接受时的 controller root/player/source/generation。后次交接、续任、控制来源改变或同 Agent ID 换主，不能继承前一任期的实际返还。是否仍是同一合法主体、密钥轮换是否保持身份，按原身份 authority 判断，不从名称或密钥外观推断。历史任期可被授权查阅，但不会给后任添加个人信誉、责任或操作资格。

查询须明确托管事项与目的，并按原 manifest/托管来源和授权事项比对范围。材料、数量和接收账本只在原可读粒度下比较；不同材料不裸加数量，不把拆项、重复 receipt 或多视图当作更多独立成功。没有可核验地点/区域来源时不附会“本地经验”，不能用 Agent 当前位置或组织势力描述替代原情境。

组织视图可显示同一事项内不同任期及合法直接路径的结果，并分别标明来源。岗位视图只包含本原任期的 delegated 结果；“在组织内”不等于“由这位接替者完成”。记录次数、金额或材料量不转为成功率、评分或接替顺位。

## 4. 当前相关性、历史与更新

本 profile **不另设按年龄淘汰的 H 窗口**。玩家正在核对一个具体托管事项与原接替任期，记录的相关性由该上下文是否仍适用决定；任意照搬个人合同的 600 ticks 或组织保护时长，会把已知返还和现行事项割裂。它也不新设声誉时钟、保护期限或申诉倒计时。

| 视图 | 当前情境的成立条件 | 转入历史或未知的边界 |
| --- | --- | --- |
| 组织托管事项 | 同一原案件/manifest 托管事项与当前权威关系可核验，查询范围仍属于该事项 | 原案件明确关闭，或有权决定明确替换/移出该上下文时转历史并保留关联；不能把源读取失败当关闭 |
| 接替岗位 | 同一原生效任期仍存在，未被原有权决定撤销/替换，未按源期限到期或因控制身份变化失效，查询对象仍在其适用上下文 | 原任期整体结束/失效时转历史；部分范围被合法撤回或移出时，只改变该部分的当前相关性，未受影响范围继续按原任期判断；后任不接收旧记录 |
| 当前操作条件 | 独立读取原源的当前权限、阶段、scope、stay/quarantine、时钟和其他 gate | 本任期存在也可能暂不能执行；条件不足显示限制/未知，咨询结果不代签下一次返还 |

**实际返还成功、`asset.disposed=true` 或数量已返完不是 scope 退出。** 它们正是该托管事项应展示的结果；同 case 的组织事实仍可见。原岗位结束只影响该岗位的当前视图，不能让组织视图的原返还消失，也不能归给新任期。部分 reclaim 只撤回 X 而保留同任期的 Y 时，X 转为该任期的历史结果，Y 仍按原任期和原期限判断；不续期、不新建任期。原范围发生合法变更时仍保留变更前后的来源和历史入口。

原组织连续性时钟 `C` 只用于核验其原授权当时/当前的有效性；本咨询不自行推进、暂停、回拨或复制其保护/救济时钟。来源时钟、当前任期关系或覆盖不可核验时，显示无法确定当前适用性；已证明的历史结果可以按权限另读，不得以证据缺失推定已到期、仍有权或没有返还。

迟建索引、重连、转载、客观校正和查询时点不改变原返还时点、原任期、原 case 或原权利期限。历史不因“已不当前”删除；未结权利、责任和有效救济仍依原来源处理。记录只陈述这次实际结果，不形成不可纠正的永久 blacklist。

## 5. 受众与完整性

每次咨询、翻页、原件打开及校正均按当前受权主体和所请求范围校验。旧 `for_reader` 的 Agent ID/名单过滤、完整内部 clone 或某个缓存快照，不是新 profile 已具备完整身份/权限或有界查询的证据。

| 读者 | 可以提供的内容 | 保留的限制 |
| --- | --- | --- |
| 公开读者 | 原源允许公开的组织/案件必要摘要、来源覆盖和该入口是否支持 | 不新增公开托管金额、contributor 身份、控制关联、候选预算或申诉证据；不可读不显示为零返还 |
| 具体托管权利人 | 原权利来源允许其核对的本事项、原接收账本、实际返还与救济定位 | 持有或曾持有相同 Agent ID 不单独证明当前读权；不获得其他人的材料、款项和证据 |
| 原任期主体 | 在其当前身份/读权可证明时，查看其原任期获准披露的事实和来源 | 读历史不复活执行授权，也不提供新 owner 的私有资料 |
| 原制度的有权角色 | 仅按现行原 charter/案件权限读取对应范围 | 成员关系、历史头衔和在候选名单中不新增一般组织代表权或所有案件读权 |

公开/受限信息分层，不把“情境声誉”作为扩大披露的理由。新 owner 不继承前任私有证据；原权利主体经合法当前身份途径读取自己的旧事实，也不能依此控制当前 Agent。错误/拒绝只提供可行动原因，不泄露该读者无权知道的数量、字段或存在性差异。

“已完整核验且无匹配结果”“权限限制”“来源未启用/不支持”“历史原件不足”“当前覆盖未知”“分页尚未完成”必须区分。只有指定受众、scope 和同一快照下的完整可验证覆盖才能给出前一种结果；没有记录不表示该组织不可信、该接替者失败，或未来会成功。

## 6. 有界客观校正与原救济

有权读取该范围的记录主体、原任期当事人或具体托管权利人，可以指出遗漏原 receipt、重复结果、组织/任期/路径错配、源版本陈旧或当前/历史标记错误。校正只在完整原件、当前受众及确定规则能够唯一确定差异时，原子替换相应派生咨询投影并保留可核对的校正关联；它不改 canonical World、原返还账、原任命、原责任或源 receipt，也不新建世界 Action、receipt、资源费用或案件。

本版采用**一次完整输入、有界处理的客观校正**，不接收等待补证的新程序，因此不新增 60 tick 或其他校正窗口。缺证、无当前读权、来源变化或无法唯一确定时给出具体结果和下一步，不先修改缓存再异步补依据。之后取得新的完整证据可以重新核对；相同来源/错误/证据的重试恢复原结果，无差异不重复写投影。

校正请求尚未确认结果时先查询原请求，不换 nonce/请求假装前次未生效。容量不足返回未受理；已受理的结果恢复和安全关闭沿专业合同保留，不以收费、隐藏历史或世界资产 hold 代偿服务资源不足。精确限制、受众绑定、投影版本、原请求保存与 G1 读取失效由 Runtime 设计拥有；一次缓存校正不冒充世界提交或新的实体处分。

对原任命是否合法、托管物权、材料是否应返、是否存在欺诈/责任或补偿的异议，回到[原组织救济](gameplay-organization-continuity-policy.prd.md#oc-policy-remedies)。咨询校正不认定这些争点，不自动创建 stay、撤销授权或重新给原程序计时；转介展示原对象、可读期限和真实入口，转介本身不是已登记救济。

## 7. 具体读面与可行选择

每条记录展示组织/原登记、案件与托管事项、实际结果和原时点、直接或 delegated 路径、适用原任期、当前/历史关系、来源快照/覆盖及实际下一步。当前能不能执行下一动作另向原权威入口核验；咨询、预览、翻页和校正不会开始返还或提交任命。

| 可读状态示例 | 应向玩家表达的内容与选择 |
| --- | --- |
| “已核验：本托管项已返还至原记录账本。” | 展示有权可见的材料/数量、原 receipt 和实际路径；可核对原账，不推论其他债务已清 |
| “这次结果来自原任命 authority 直接执行。” | 组织事项可见，岗位不计入；不能为当前接替者添加业绩 |
| “这是上一任期的返还记录。” | 旧事实仍可查，组织同 case 的结果保留；新岗位只展示其自己的获证结果 |
| “本任期记录可查；当前执行条件尚未核齐。” | 已发生事实与新动作权限分开；查看原 scope/限制或等待来源恢复 |
| “当前范围已完整核验，没有匹配的已验证返还。” | 明确本次受众、scope 和快照，不给正负信用结论；可查看有权历史或另查具体事项 |
| “来源或权限覆盖不足，不能判断是否存在匹配返还。” | 不显示零记录/零风险；可补齐合法读权、查原件或恢复原查询 |
| “咨询投影已校正；原组织事实和权限未改动。” | 展示本次可读差异及来源；实质权利争点仍进入原救济 |
| “校正结果暂无法确认。” | 保留原请求关联、查询结果；不重复校正、不宣称失败或已生效 |

静态走查：同一 case 的资产 X 有 10 个原材料最小单位，原任期 H1 的 A 经合法 delegated 路径全部返还。X 标为 disposed 后，这 10 个单位仍属于该组织事项与 H1 的真实结果；没有第二次返还。H1 到期后 H2 的 B 生效，组织同 case 仍显示原返还，H1 变历史，H2 不继承 A 的结果。另一个不在 handoff scope 的资产 Y 由原任命 authority 在合法阶段直接返还，组织可显示，H1/H2 均不取得这条岗位记录。材料或量不同则逐项保留，不能算成一份全球“履约分”。

以上只是政策样例，前置须由真实源生成。正式 API、Agent 与 Viewer 在同一受众/快照下应给出同义结果、限制和下一步；不能用任意一端的旧候选、旧 score 或固定 positive 文案代替新查询。

## 8. 专业验收、取舍与证据边界

下列接受条款均消费原 [CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。专业技术承接由配套 Runtime 设计逐项反链，方法存在不表示已执行。

<a id="ac-orgrep-01"></a>
### AC-ORGREP-01：真实来源与返还路径

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：真实登记/托管/任期/ReturnCustody 原件闭合；组织两路径、岗位仅 delegated，不把 ack/申请/布尔当结果。验证方法：[ORGREP01](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep01-source)。

<a id="ac-orgrep-02"></a>
### AC-ORGREP-02：组织与原任期归属

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：组织、原任期、当时控制来源与个人资格分开；换主/续任不继承前任事实和私有证据。验证方法：[ORGREP02](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep02-subject)。

<a id="ac-orgrep-03"></a>
### AC-ORGREP-03：情境、时效与部分撤回

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：精确 case/manifest/托管事项与任期分别决定当前相关性；成功 disposed 不消失，部分范围撤回不结束未受影响任期，原期限与历史不重置。验证方法：[ORGREP03](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep03-context)。

<a id="ac-orgrep-04"></a>
### AC-ORGREP-04：当前受众与完整性

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：当前受众和权利来源逐项核验；受限/未知/不支持/未分页完不伪装完整空集，无权内容不泄露。验证方法：[ORGREP04](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep04-access)。

<a id="ac-orgrep-05"></a>
### AC-ORGREP-05：派生校正与原救济

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：完整来源只校正派生投影，实质权利回原程序；无新增世界事实、窗口、费用、stay 或处罚。验证方法：[ORGREP05](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep05-correction)。

<a id="ac-orgrep-06"></a>
### AC-ORGREP-06：原请求恢复与有界资源

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：当前权限/来源漂移、预算/容量和未知原请求有界恢复；缓存校正不重复、不绕过原 writer/读取限制。验证方法：[ORGREP06](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep06-recovery)。

<a id="ac-orgrep-07"></a>
### AC-ORGREP-07：三入口与消费者边界

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905；承接：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)。

必须证明：三入口和玩家选择同义，核得返还与当下操作资格/未来信用分开，不影响原世界资格消费者。验证方法：[ORGREP07](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep07-reading)。

## 9. 取舍与后续边界

选择两种组织返还路径，是为了完整表达原合同允许的托管事项；岗位保持原任期，避免把组织结果变成个人业绩。选择上下文/事件决定的当前相关性，是因为本用途核对具体事项，没有依据把年龄变成新的信用权重。选择一次完整校正，是因为没有必要建立另一套持续争议程序。这些是本轮明确目标，不是历史 owner 批准或实测结论；未新增玩法数值。

现有窄执行分支不能代签新 profile 的历史原件闭包、当前读权、完整覆盖、有界查询/校正或正式三入口。实现与同候选业务/恢复/玩家验证未取得时记录 `blocked/not_run`；静态审读、CI 和样例不证明可玩性、吞吐或发行。若扩展到一般组织合同、开放岗位、更多操作/来源、跨事项评分或其他资格消费，gameplay 与相应专业 owner 必须先承接其新的身份、代表权限、时效、证据和权利边界；本版不预授权这些扩展，也不把 CR-003 全项改为完成。
