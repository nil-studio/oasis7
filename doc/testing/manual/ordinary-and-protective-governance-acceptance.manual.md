# 普通事项、治理主体与高影响保护验收

- 固定设计审读输入：`nil-studio/oasis7`，main `2731f781536018a857e5daf2cfa7ede504e9cd69`，2026-11-05。执行时另固定实际 candidate/build、world/branch、authority profile、模块/政策版本、真实来源与环境；本文没有运行结果。
- 原产品：[GCB-003](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-003)、[GCB-004](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-004)、[GCB-005](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-005)、[GCB-006](../../product/world-rules-core-gameplay/governed-common-decisions-and-constitutional-boundaries.prd.md#req-wr-gcb-006)。Trace refs: O7-REQ-4f6597ff-8e34-4d12-bb60-060df0e09595；O7-REQ-b584b2ab-bc38-43a1-a8fd-1b89dc3d20cd；O7-REQ-4b64b7c5-9819-4932-a1ad-3cdb09fa82e5；O7-REQ-c3d848a8-0668-408a-99ca-ef08a7c64be8。
- 玩家取舍与验收：[GAME-019](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md)的六项 AC、[共同 causal-decision receipt](../../game/gameplay/gameplay-indirect-control-agency-contract.prd.md#21-reusable-causal-decision-receipt)。既有 [GOV-VERIFY-001～004](../../world-runtime/runtime/governance-classification-and-constitutional-execution.design.md#gov-verify-001)继续拥有分类、宪制合取与其原场景，本手册不重定白名单、保护、票权、费用或申诉政策。

## 1. 输入、独立预期与证据层级

1. 先取得原 authority 发布并合法激活的有限政策、schema/manifest、主体/绑定/控制来源、事项及修订、完整签署/批准、原账本和当前读取权限。自由文本、控制台注入、`passed` 标签、某个 node ID 或模块自报成功均不替代这些来源。组件层可明确构造前态来核确定规则；正式层必须实际经过相应 producer、认证入口和提交消费者，缺接线如实 `blocked/not_run`，不能把所有输入拒绝当作正常能力已成立。
2. 每组在同一 C0 独立建立正常分支及单变量反例。先从原签署正文、完整来源集合和原账本手工列出主体归属、来源去重、额度/票权、精确允许效果、费用和提交顺序，再与被测输出比较；不能用同一 projection 的字段或 `success=true` 自证。实际使用的数值是被测已声明政策的样例，不据本手册新定经济或政治参数。
3. 分开记录声明/证据受理、资格或快照、审查/批准、原业务执行、实际费用、持久决定及最终可读结果。每个原资源按 owner/ledger/kind/unit 分账，锁定/hold 是原 gross 的受限部分；未来收入、另一候选预算、未释放占用不补当前缺口。拒绝的零业务效果不排除原认证 nonce、必要有界审计或原协议费用；先声明并独立核它们，不能制造一条所有失败都免费的规则。
4. 正常与故障均保留原 operation/request/payload、事项/案例/证据身份、policy/source cut、当前授权、前后 root/tick、原 receipt 与真实差分。引用和摘要必须可由当前获准 reader 回取必要原件；真实缺页、权限遮蔽、旧水位或未知提交不当作空集/零影响。读取并不自动获得新执行权。
5. `test_tier_required` 按各原要求运行正常、拒绝和有界来源检查；`test_tier_full` 再在同候选实际 authority 下取得并发、重连、快照加非空后缀、跨节点及正式 Viewer/pure API/Agent 证据。纯文档关系、静态算术、组件 fixture、内部 helper、正式 producer 和玩家理解分别报告，不彼此代签。

<a id="gov-01-approved-execution"></a>
## GOV-M01：准确批准事项与单次执行

依据：[原提交合同](../../world-runtime/runtime/governance-classification-and-constitutional-execution.design.md#governance-commit)。对应 GCB-003；复用原封闭效果集合、`ExecuteGovernedMatter` 和 `ExecutionSlotV1`，这些是目标逻辑合同，不因方法引用就成为已有 wire/API。

- 各选一项原白名单合法的 policy runtime、公共财库、existing-charter 日常事项，分别形成真实类别、明确受益/对象范围、完整 payload digest、revision、批准及当前执行 authority。预览、通过、排队和 `ready` 前后都不能提前改变规则、余额、hold、角色或权限。依赖旧自由文本提案或内部设置的样例只证明其原层次。
- 财库正常样例选择确有合法公共项目和资金来源的原单位 u：财库可用100、接收方0，批准正文恰好向原接收方支付10。独立业务预期为90/10、一份原执行结果；实际模块费、执行费或已声明 hold 另按原 payer/用途逐笔计算，不默认为零。政策/charter 样例则先列完整允许字段和原前后值，逐项核恰好该 delta，没有附带扩权或隐藏拨付。不存在该真实项目/单位/消费者时不能用手填余额替正式正例。
- 固定 `world/module_instance/matter_or_case/approved_revision/execution`，同时提交两个不同 request ID；再换客户端、Agent 或 operation 标签重试。最多一份业务 delta；其余依原协议返回原结果、拒绝或保持待决。仅各自的传输 ID 不同不能占第二个执行 slot。同 key 换接收者、金额、范围、效果或批准摘要必须冲突；借另一个无关 Passed 提案不得执行本笔。
- 在财库已按本案支付10后，保留同一批准事项，仅换 `distribution_id` 再支付10。独立预期仍为90/10，不能因新分发 ID 未出现过变成80/20。真正新项目/实质新修订须重新取得完整批准和当前资金条件，不能将旧批准改名。charter 若一次批准只产生原合法子工作义务，父事项只创建一份原工作身份；父 receipt 不证明物理工作已完成，重试父事项不再派一份，后继按原 child 身份、当前领域条件及其实际结果恢复。
- 将批准正文10改成11、追加一个受保护效果、换 module instance 或旧 revision，仅改变该条件再提交。不能静默取允许子集、先付10再拒绝1，也不能把错位批准当作可消费额度。实质新事项只能在原程序明确新修订并重新批准；新修订仍不能回放已经完成的旧业务。
- 当前原生政策路径仍由 [任一 Passed 检查](../../../crates/oasis7_runtime/src/world/event_processing.rs)和[参数校验](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_gameplay_policy.rs)构成；[MainToken 财库](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core_main_token.rs)另有 proposal/distribution 检查。其局部参数/分发去重不证明上述精确事项 slot、全范围和正式消费者已接通；验证必须指出实际采用的目标 adapter。

<a id="gov-02-current-revalidation"></a>
## GOV-M02：执行前漂移、原子拒绝与已确认历史

仍用 M01 的原已批事项和单独 C0 分支：撤销/到期/收缩/替换执行授权，变更 charter 适用范围，改变受益对象或可用资金，加入保护条款，或使事项转为必须走独立宪制轨道。逐个缺项均在新的业务发布前拒绝、过期或待决；拒绝不产生部分拨付、规则更新、角色、容量或新的债务。无关对象更新不应误报为本事项权限漂移。

用原 canonical 顺序分别令“撤权先于执行”和“合法执行先于撤权”：前者零新业务效果；后者保留原已确认结果，撤权仅约束后续新动作。客户端更早点击、旧缓存 `ready`、原排队位置、Agent 自动重试均不改变该顺序。原资源变成只够9而正文仍要求10时整项失败，不能部分付9或借未来财库收入。

把普通 policy 中的 `blocked_agents/forbidden_location_ids` 等实际限制与日常参数放在同一效果集，分别检验合法原高影响 authority 与缺该 authority 的情况；不因外层叫 policy/charter/紧急升级而跳过原保护和实质分类。此对照核目标全部 writer 的接收，不从内部 API 的行为推断远端可绕过。

已提交后再撤权、过期或收缩，使用原身份和当前合法读权查原结果；不能重新收取业务费用或制造第二次生效。合法纠正/补偿必须由原独立 authority 形成 linked 新结果及真实资金来源，不能修改旧 receipt、重复原执行或凭本手册产生补偿权。原申诉只按其实际 scope 阻断，不冻结无关事项。

<a id="gov-03-subject-aggregation"></a>
## GOV-M03：原账户、绑定与实际控制聚合

依据：[主体与来源](../../world-runtime/runtime/ordinary-governance-subject-and-oc-rights.design.md#og-subject-source)。对应 GCB-004。固定同一事项的原 source cut C0、当时完整获准绑定/controller 断言，以及当前可执行身份；自行填写的 account、Agent、wallet 或 controller 标签不产生这些 authority。

1. 正常例使用一个已验证游戏账户、其经批准的玩家/托管签名关系和两个真实不同外部钱包。逐一核原绑定的 owner、签名、范围、修订、有效窗、source/commit 和实际余额单位；同一钱包经两个别名、重复 proof 或不同顺序输入仍只计一次。钱包分别确有80和40原始单位时，来源总量是120；两份对同一80余额的引用不是160。密钥持有证明不自行证明另一账户独立或实际控制人唯一。
2. 将正常集合分别换成未批准绑定、错 world/asset/source cut、失效签名、被撤销或不完整的实际控制断言、不可读原件、重复来源、遗漏关联边；当前必要证明未成立时不准新增资格，未知不能按独立 controller 或零关联默认。完整可验证的空关系与缺证据必须可区分；不能用名字不同、在线节点不同或曾经有余额作为独立性证据。
3. 两个已批准主体 A/B 属于同一实际 controller G 时，在该 matter 的完整集合先合并再计算上限；把同一来源拆成多个钱包/账户/代表不扩大 G 的总额。另有真实独立 controller H 的来源不得无证据强并入 G；对 G 的 challenge 只影响其准确范围，不能冻结全部公开主体。
4. 公开读者核最小 subject/controller 断言、适用范围、source cut、理由和可复查引用；获准本人、现代表、复核者按各自当前权限读取需要的原件。无关人和已撤权代表不得读取私下账户/钱包关联、密钥或未授权控制图；公开断言缺乏足够可核依据应如实 unknown，不通过公开个人信息来补齐。

对尚无正式生产者的完整绑定/控制证明，组件构造可检查确定算法，正式入口保持 blocked；不把当前 Agent 票权或客户端提交的自报 assertion 当新账户资格已经实现。源码中的旧 Agent sqrt/reputation 票权按原 legacy 版本解释，不混入新线性事项计算。

<a id="gov-04-lock-delegation-cap"></a>
## GOV-M04：线性事项权利、两侧上限与委托占位

依据：[锁定、委托与 cap](../../world-runtime/runtime/ordinary-governance-subject-and-oc-rights.design.md#og-lock-delegation-cap)。从独立原来源计算：同 controller 的有效原始锁定量先求和 L，按已声明 unit 得 `P=min(floor(L/unit),C)`，再按各主体原量的比例和最大余数法分配；余数相同以 canonical subject ID 决定。输入顺序、钱包别名和客户端不影响结果，不能先逐账户取整再叠加 cap。

| 独立有限前态与操作 | 预先计算的结果 | 反例与保留边界 |
| --- | --- | --- |
| unit1、G 的 cap100；同 G 的 A锁80、B锁40 | L120、P100；配额底数为66/33，剩余1给较大余数的 A，得到67/33，总量100 | 按账户分别封顶得到80+40、按顺序抢占100、把一个真实钱包重复计入，均不得通过 |
| 把 A 的同一合法原80分成该 subject 的两条不同真实40来源；重排全部输入 | A仍67、B仍33；来源守恒且 G总量仍100 | 同80的别名不能伪成两条40新来源；拆成更多已批准subject也不能突破G总cap |
| 独立 controller H 的 cap100、R自有配额60；B将完整33委托R | G仍只来源100；H代表侧占用为60+33=93，原origin B只能有一份33 | R同时收A67会达到160，整个新委托拒绝，不能裁为剩余7；同H的R2不提供另一份cap100 |
| R把自己的60另委托出去，再请求接受A67 | R所属H代表侧原own60仍计入，加已有B33后仍无容纳67空间 | 转委托不能给H腾出虚假容量，代表不能将B的受托份额再次转出形成链或环 |
| B撤回尚未投出的委托 | 原B当前代表权撤销；对应H的33占用可按原合同解除 | 用新request并发撤回/转委托最多一个当前代表，不产生第二份origin额度 |
| B的33已经通过R形成一份有效ballot，然后撤委托 | 旧ballot及H33占位保留；同origin的委托与旧ballot占位按union去重，不重复算66 | 撤回本身不删旧票、不解占后再投第二份；仅B按原合法改票条件用同origin ballot key显式替换，或原review/close，才改变对应占位 |

另分别拒绝超原资格范围的代表、错误 matter/window、来源不完整、重复 origin、第二现任代表、多跳/循环或当前被收缩的 authority。一个提交同时更换委托和合法替换 ballot 时，完整候选来源、origin key、双方 controller 占位、当前权限及 receipt 同根核算；任一条件失败保留原可解释状态，不能先释放占位再失败。

与保护模块联测时分别计算 snapshot 的q/P、当前可用a/A和候选后仍有效的旧票V。各subject的a取q与当前合法限制的较小值；同组A取原P、当前group cap与各a之和的最小值。代表侧base取`max(A,V)`，每个外来origin占用取其当前委托可用量与仍有效旧代表ballot量的较大值，再求总和；不能把旧snapshot原P永久当降权后的当前可用量，也不能丢掉旧票占位。

取无其他占位的独立单origin组：原snapshot/ballot为7，当前准确restriction cap3生效，A=3、V=7，base仍7。旧7保留为历史，新Cast或原本合法的Replace须requested≤3；同origin合法显式替换7→3后，候选A=3/V=3/base=3，才释放4且追加原结果。requested7整拒，不自动裁为3；其他仍有效旧票使完整候选用量超cap时不接受新的增加/重分配。cap0仍不自动禁止原合法撤回、退出、查询或申诉，但不能借减权声明更换选项/代表绕过当前新动作条件。另有有效cap2时取严为2；cap3到期不撤掉cap2。不同事项或无关controller不受此限制外溢。

<a id="gov-05-subject-review"></a>
## GOV-M05：质疑、独立审查、纠错与申诉

依据：[完整主体审查](../../world-runtime/runtime/ordinary-governance-subject-and-oc-rights.design.md#og-subject-review)、[有限程序与费用](../../world-runtime/runtime/ordinary-governance-subject-and-oc-rights.design.md#og-bounds)。对应 GCB-004。先固定真实原 review authority、当前完整当事方、primary 与另一独立 appeal 集合、各自原阈值、签定的答辩/决定/申诉/终审时界，以及全程序实际 payer 和保留预算；没有这些前态不能用单一管理员 approve 模拟正常完整流程。

| 阶段 / 刺激 | 独立预期 | 必须另测的失败或并发 |
| --- | --- | --- |
| 仅提交 ChallengeSubject | 记录同一case的材料与理由；不合并账户、不扣款、不改变票或冻结权利 | 同证据/target换request不新开程序；来源不足得到具体待补，不默认有罪 |
| 合法 AcceptReview | 在同根记全部通知/答辩槽、原证据目录、两阶段及关闭预算、最多4个受影响active M目录和本案scope的review_hold | 无权受理、角色与当事方/控制人利益冲突、遗漏一方或一个受影响M、第五个active M、预算不足时不产生部分hold或部分受理 |
| 各方答辩及 primary 决定 | 原合法受众读取材料并各交一次完整回应；primary签精确证据/拟处置/全目录，锁定correction候选，尚不发布资格/计票纠正 | 仅source hash、少签名、旧policy、伪造无冲突或超期材料不成为合法决定；一方反复修订不得花掉其他方首份回应/appeal/关闭保留 |
| 合法primary后，无人在原窗口内提出合法appeal | 仅在该原窗口真正结束且原决定/当前提交条件仍有效时确认；缺primary不能因沉默自动merge | 丢通知、未确认受理、source漂移、pending原请求须依原程序处理；不由客户端墙钟提前确认 |
| 已受理合法appeal | 与原primary分离的有权集合维持、撤销或替换该候选；保留原材料/决定，只有准确最终结果可发布 | 原决定者或其别名自审、权限漂移、错scope、未齐签名均不产生最终纠正；换案号不续窗口 |
| 到原终审硬截止仍无合法结论 | 记review_unresolved并保留理由，去掉本案程序hold及未生效候选；无provisional计票或合并被当作最终结果 | 不永久冻结、不默认败诉或加罚；保留后来合法来源/他案决定及原独立保护限制，不能盲写审查前旧profile |

**纠错的可算正例。** 从同一原C0列出A80/B40两个真实来源；最初错误地当成两个独立controller，各cap100，已投有效权重80/40。合法完整控制证明确认两者在C0时已实际同G后，仍按原C0重算为67/33；cut后才发生的合法控制变化只进入当前准入与后续事项，不能当作C0错误追溯改原票。对尚未finalized的M1，两人原来都投同一选项时该项120→100；对M2原选项不同则80/40→67/33。每个新贡献取`min(原ballot requested,修正q)`，从未投票者仍无票，不添加新选项。独立按原quorum/pass policy重新计算待决结果，不在手册另定通过门槛。

用最多4个实际受影响active M与R形成一份有限纠正，先列全部alias/资格、完整controller/原wallet去重、各当前ballot贡献、代表占位、tombstone、case终态、真实费用/hold和receipt，再验证最多5次invoke在一个root提交。任一M来源/权限/预算/expected revision失败，全部保留旧可读状态，不允许先改M1后等M2；别名合并不合并登录账户、转移OC、接管signer、改变P2P绑定或顺带处罚。

另用已finalized但仍有完整原件的同案分支核：原winning option、执行receipt和世界效果逐字保留，只追加audit-corrected tally及未来资格原因；若事项已经归档，则按确切matter原件及R中的linked决定计算有界只读审计注记，不全历史扫描或改写，不加入R+至多4个active M的自动写集。尚未实际执行的已通过事项仍走M02当前重验。有效appeal撤销纠正时，只恢复有原C0证据的权利/投影并保持同origin ballot唯一性；已经发生的业务只接受原独立redress，不能因此再执行或退款。分别对merge、governance-binding scope revoke、controller correction与confirm_distinct核其准确允许写集，不以merge正例代替其余处置。

对每个受理、阶段决定、纠正和关闭切点应用M09的确定失败/已提交丢ACK/G1三态与非空suffix恢复。原材料、各方回应、两阶段预算、当前hold和准确决定应同时可恢复；不能从当前合并结果倒推当时程序完整，或重启后给同案第二份纠正/补偿。

<a id="gov-06-snapshot-transfer-exit"></a>
## GOV-M06：同事项 source cut、转让与安全退出

依据：[快照、转让和退出](../../world-runtime/runtime/ordinary-governance-subject-and-oc-rights.design.md#og-snapshot-transfer-exit)。对应 GCB-005，并核 GCB-004 来源公平。固定不可变 C0及当时完整获准绑定/controller、每个原钱包的可分配份额、allocation/tombstone和窗口；外部 OC 按原规则仍可转让，本事项锁定不是新造的资产托管或禁止转账。有限时间fixture沿专业设计的合法policy样例：C0=t10、申请[10,20)、投票[20,30)、程序最迟40；不是生产默认。

**注册窗口正反例。** C0时A原钱包100、B原钱包0，unit1/cap100。A先按原资格分配100，再经原链合法转100给B；原链当前余额为A0/B100，本事项的B原来源仍0。B在注册窗口关闭前以新余额、后绑定、别名或新代表尝试再分配100均不得成功；不能以“最终snapshot尚未生成”为由重复算同一100。A的既有allocation事实不因外部转让被删除，但任何后续治理写仍须其当前有效权限。A退出后原allocation的tombstone保留，同事项不能通过重新锁定、换request/绑定或重建索引重开原份额。

相邻正常例使用C0就已有的A100/B40，两条真实独立钱包及合法controller，各自只能分配各自原份额，再按实际controller cap核算；不能把防重复规则误写成同事项只有一个钱包能参加。新事项C1可以在其新的合法source cut和当前批准绑定下读取B的新余额，但必须是实际新事项及其原窗口，不能为旧matter改名续票。

**窗口关闭与投票期。** 在同一原 canonical 顺序分别令合法分配/撤回/绑定或控制修订出现在窗口关闭之前、之后及并发边界。关闭事务须重验完整来源/当前 authority并一次固定原事项snapshot，不能以分批关闭接受同一钱包两份；迟到入账或后绑定不补C0份额。投票期再转账、撤委托或退出都不倒改旧snapshot/ballot；新cast、原合法改票及退出依各原窗口和当前资格执行，不能把退出当自动删票或退还已付费用。

在上述fixture且其他条件齐全时，t19的新合法lock仍可受理，t20不再接受新的申请；完整snapshot在原边界成立后，t20可按当前权限cast，t30不得新增投票。source/原申请结果尚unknown时保持blocked/过期的准确分支，不能假造空snapshot让时间正例通过。每个端点分别从独立C0执行，不把前一分支已用的allocation重复消费。

在注册/投票两窗口各重连、换合法客户端、重试原操作及恢复非空后缀，核allocation、tombstone、原snapshot、origin ballot和代表占位完整保留；unknown先查原身份，不能以当前余额反造原可用份额。合规退出仍保留未结复核、原计票和已执行事实，不能为了安全退出抹除债务/责任，也不能以普通治理冻结无关游戏与服务恢复。

<a id="gov-07-quota-isolation"></a>
## GOV-M07：外部 OC、事项权利和单向 quota 三态

依据：[GAME-019 外部转让](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#41-外部转让与游戏内权利分离)、[服务桥边界](../../game/gameplay/gameplay-ordinary-governance-subject-and-oc-rights-contract.prd.md#42-单向非治理额度桥)、[原桥恢复](../../p2p/token/mainchain-token-newapi-quota-bridge.design.md#bridge-recovery-state)和[Phase 1/Phase 2](../../p2p/token/mainchain-token-ideal-transaction.design.md#itx-phase2-gate)。分别保存 O=原链转账、G=原事项锁定/快照/退出、Q=原服务订单/额度的准确身份及结果；三者不合成一个 success。

1. 由原链入口实际完成一笔合法 OC 转账，核签名 payload、asset、账户、amount、原 nonce、实际余额 delta 和原结果。tracker included/confirmed、当前余额或 last_nonce 单独不证明本笔 World 效果。治理主体、G 的快照/计票/委托和权利没有凭转账自动增加；真正事项权利必须另有其原接受事实。Phase 1 `max_fee` 是 quote metadata，真实其他业务费仍独立核。
2. 用原支持的 operator/project/route、正确 token 与金额、未过期的稳定外部 order 取得服务额度。独立预期为原 order 的一次 topup 与其实际 quota，治理账户、controller、锁定、权重、快照和投票零自动变化；不新增世界库存、资格或 OC 赎回。`credited`、`reconciled` 和真实服务消费分别取证，不把 token_key 当世界权限。
3. 各只改变一项：错误 token/金额、过期、错项目、重复 order、topup 后查询失败、消费失败、原 authority 不可用。原桥决定 manual review/unknown/恢复；已经成功的 topup 或外部转账不因另一侧失败被删改，unknown 查询原 order，不删除重建或再充值。公开证据只留必要引用，不放 token_key 或私密 provider 凭据。
4. 用额度、服务消费回执或治理 receipt 分别尝试新增普通治理资格、改变 controller cap、补票、外部赎回及购买世界结果。没有原独立 authority 时均不得产生该效果。桥关闭/额度失效不能倒改已确认治理记录，治理暂停也不凭空退款或抹去原服务义务。Viewer/API/Agent 对同一可读 O/G/Q 明示各自条件、当前结果和下一步；缺其中一侧保持其真实未知，不以其他侧成功填空。

<a id="gov-08-protective-authority"></a>
## GOV-M08：有界保护、原证据与独立复核

依据：[正式保护 authority](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-authority)、[动作目录](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-action-catalog)、[证据边界](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-evidence-bounds)及[复核/到期](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-review-expiry)。对应 GCB-006。下面 tick、余额和 cap 是设计明确的有限样例；实际运行必须使用具有相同合法条件的原版本，不把样例变成新的全局罚则。

**共同授权。** 固定合法激活政策 P、当前有对应 capability 的 initiator I、按原policy主体规则去重的 guardian G1/G2 及与原决策角色分离的 reviewer R。该样例按原政策阈值2，目录仍核完整16签名上限、其中guardian≤12及各真实来源。所有必要签名必须签同一规范化 world/branch、policy/instance/revision、target、incident/原证据、完整effect set、数量/cap、scope、有效窗及原请求；不能只提交名字或证明“这个key曾登记”。单独验证原事实 producer、证据当前性/完整性和该动作权限；来自节点的 slashing intent、错误日志、普通 Passed、validator admission 或既有 receipt 都不是本次保护许可。

对真实 `CapabilityAuthorityRecord/FinalityProof`、精确 `CapabilityGrantV2` 和本次业务批准分别取证：issuer的原proposal/manifest/finality binding及当前key成立，仍不足以批准保护；grant还须匹配实际module ID/version、namespace、command对象名、subject/owner-generation、audience、scope和期限，业务批准必须匹配本次完整效果。分别用“技术信任正确但无业务批准”“有业务批准但只有policy.publish/role.bind或另一角色grant”“错subject/target instance”“撤销或旧key/旧epoch”反例，均不能开始对应case效果。role.bind不替另一subject签grant，子grant不扩大父scope。成功分支实际先由原proof-bearing安装/登记链取得issuer和独立grant，再经过本专题目标schema/adapter；缺该producer时只报告实际接线层blocked，不借现成validator API完成正例。

| 原动作与正常前态 | 独立业务预期 | 必须拒绝或准确分层的反例 |
| --- | --- | --- |
| 原紧急 brake，t100接受，持续10tick，原max720仍适用 | 有效窗为[100,110)，只限制原technical proposal apply gate；同请求/incident重试不续到111；t110谓词失效，不等待页面刷新或清理 | 超原最大期限、改scope/target、已过生效窗、普通proposal假称紧急；不得据此暂停整个World、普通玩家、审计/申诉/原结果恢复或改finality |
| brake的受权提前释放 | 只能释放原case拥有的限制并追加原结果；其他当前限制仍有效 | 换case释放他人限制；重放release多退资金/授额；客户端Stop或断连冒充正式释放 |
| queued proposal veto | 只有原Approved且已queued、payload与本次授权一致的对象被一次阻止；保留原批准与veto因果 | 未批准、未queued、错误revision、已Applied；不能回滚已经生效的规则、款项或历史 |
| 身份临时freeze及原penalty | 原locked100、合法slash10时原位置余额90，产生本次准确限制与结果；到140仅撤本临时限制，不能仅因到期自动退slash | 未证明原stake、金额超可罚范围/来源、错误主体、只有普通治理或guardian名字；zero-slash freeze亦须合法authority，不能借它免费取得更广冻结 |
| 原资格域临时降权 | 原合法cap7，本次cap3、[100,110)内当前上限取3；旧snapshot7及原票不重算，新cast/原合法替换按M04当前完整候选组核。另有独立有效cap2时当前仍2，本次到期后仍2 | 提高原cap、给本来未知主体默认7、改成扣OC、延长旧窗、撤掉其他限制或使旧snapshot回滚 |
| 原policy中的已获授权个人/地点限制 | 实质分类允许且原独立authority完备时，仅改变本案准确对象/scope/期限对应的原字段，并保留同根case/receipt | 正常日常字段夹带未授权blocked_agents/forbidden_location_ids整项拒绝；不因外层是policy就给新的封路或冻结权限 |
| 原penalty申诉与独立裁决 | 原t100产生32tick申诉窗，132仍可受理、133拒绝新申请。独立accepted只按原处分返还10一次并解除本案允许恢复的部分；rejected不退款；原历史与理由保持 | 原initiator/guardian冒充独立reviewer、仅提交申诉就自动退罚/撤限制、换request/代表重置窗口、裁决未知按胜诉处理 |

正常分支至少由实际正式 producer 完成一种适用保护及其合法释放/到期或申诉结论，并逐类检查本目录实际支持的动作；不能只以brake替代freeze、veto、降权或处罚。节点intent仍pending/enforced=false、内部helper成功、签名校验通过及权威效果四层分别记录；内部helper没有完整角色校验不表示存在可供远端调用的绕过。

多案并存另设独立前态：原可罚locked100，先本案合法slash10，再另一独立合法案slash7，当前83。本案独立accepted仅返本案10一次，结果93；不能恢复成旧profile的100，也不能清掉另一案仍有效的freeze、cap或待结义务。另一案变化与本案裁决在同根当前读集中重验；重复裁决/到期/重放不能再加10，真实无关收入或支出也不被回退。

从同一共同前态依次只改变：I无此capability；一名guardian失权、重复主体或具有原policy明确禁止的角色/控制源重合；一份签名绑定旧payload；证据存在但不能证明所列target/数量/scope；原动作predicate要求的当前风险已消失；原申诉/复核程序缺失；有效窗已过。每项都不得发布部分slash、隐性资格变化或新限制；不把风险结束本身当作已确认历史违规免责。需要更窄合法请求时必须由原authority明确接受该完整范围，不由执行器悄悄裁剪一个已签effect set。

把同一真实证据通过新request、不同字符串别名、不同客户端、reconnect、换module实例或普通policy包装再次提交，独立核原incident、语义slot及效果集合：不产生第二份保护/处罚、资金扣减或新期限。真正不同事件仍须其各自原证据和授权；不能为了去重把所有将来的合法风险都误判同案。复合“合法日常参数+未获授权冻结/封路”整项不生效，不能先执行普通部分取得事实优势。

原调用费用使用已声明完整ABI上界I≤256KiB、O≤128KiB、raw instance state≤32KiB、最多1effect/1emit；I达到256KiB时单次上界为387Data/4Electricity。正常独立编码样例I=8192B时实际预检为8+128+2+1=139Data/4Electricity；O=4096B且实际1effect/1emit/new_state，实际为15Data/4Electricity。真实payer具有400Data/10E时结果385Data/6E，不能直接扣139或387。仅此调用的D138/E10反例预检拒绝；D139/E10满足该预检，按同一真实输出实际余124D/6E。I/O由完整实际编码取得，模板常量不代真实长度；签名、证据、外层封套和state都计入。若真实调用图另有hook/validator/复核/结束节点，逐次真实ordinal与原payer计费，不能拿一个调用上界覆盖整案或从未来退款借资。缺原预算时在相应原准备点失败，不能先罚款再倒扣执行费。

原trusted capability command还须独立核subject/grant绑定的[CapabilityBudgetAccount](../../../crates/oasis7_world_state/src/capability_authorization.rs)，它不是Data、Electricity、OC或bridge quota。令同一实际调用原payload F=2048B、完整ABI I=8192B、O=4096B：按[原capability计量](../../../crates/oasis7_runtime/src/world/capability_authorization_state.rs)先reserve `2+128+2+1=133`单位，actual为`2+4+2+1=9`。原remaining200/reserved0/spent0成功后应为191/0/9；不能把133和9双扣，也不能用Data的15替这9。remaining132即使有D400/E10仍在相应调用准备前拒绝；cap账充足而D138不足同样不产生业务效果。原结果重读、已提交重连或canonical apply不重新消费这两类预算，真正未提交操作仍须当前grant/nonce/原额和真实资源同时可用。

<a id="gov-09-recovery"></a>
## GOV-M09：原件恢复、版本与持久决定

依据：[治理原持久化](../../world-runtime/runtime/governance-classification-and-constitutional-execution.design.md#governance-persistence)、[完整 generation](../../world-runtime/prd.md#runtime-generation-persistence-contract)和[原请求端口](../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port)。每种受测业务先列全部原写集，分别含其真实资源/权利、snapshot/计票、委托、restriction、费用/hold、语义 slot/source-use、receipt、案件和审计/结果索引；不同业务只计自己的真实写集，不造共同治理总账。

| 故障切点 | 独立预期与正确恢复 | 必需反例 |
| --- | --- | --- |
| 业务准备或正式提交前已证失败 | 原旧状态及其他已经确认结果保留，未提交候选无部分业务效果；是否可重提按当前原权限/窗口 | 把部分票权、扣费、restriction或slot先写下再补receipt；借恢复扩scope |
| 已提交、客户端丢ACK或读投影落后 | 按原identity/payload定位同一结果；源业务不再执行，修复派生索引只重建原事实 | 用新nonce、request、matter别名、controller或新artifact重做；快照有余额就推断原请求成功 |
| 持久决定NotCommitted | 查明精确原generation/base后才能丢弃未提交候选，旧代重新可写；仍保留其他原确认事实 | 由网络失败猜NotCommitted，未查原决定就清索引重发 |
| 持久决定Committed | 安装/恢复同代完整staged候选、原费用/slot/历史及receipt；之后通知/维护失败不撤销 | 只装资金不装票/限制/索引，或者回滚重做原效果 |
| 持久决定StatusUnknown | 保留完整候选及base generation，隔离整个不确定旧World的mutator、tick、save和依赖派发，沿原authority核决定后恢复 | 只冻结一个提案或账户而让旧World覆盖可能已提交事实；普通查询unknown冒称全World未知 |

选择提交前快照及包含至少一笔本组真实业务的非空 canonical suffix，恢复同一政策/身份/时间/来源修订与完整原件；再重连、换合法 reader、重试原操作及重放。历史 apply 按原版本重建应有状态，不生成第二份实时业务、第二次费用或外部通知。原当前限制到期依权威时间谓词失效，不靠客户端重新打开或后台清理；历史restriction、原投票、申诉和已付事实继续可读，解除当前限制不授予其他本来缺失的资格。

跨节点核原正式 finality/authority profile，不把本地 generation commit 当链最终性。旧 policy/manifest、未知 schema、异代来源、缺失去重/案件索引均不得自动采用当前版本重算旧票或处罚；按原合同停止相应新写并保留原结果/合法drain。旧阅读者不识别新状态须明确不支持，不能把新保护/未结义务读成空。

<a id="gov-10-bounds-surfaces"></a>
## GOV-M10：完整预算、当前读权和真实入口

按被测激活版本列出全部实际 writer/reader：原生入口、治理模块、tick/directive、自动续作、纠错/复核、到期处理、恢复与原结果查询。对每项列实际 producer、认证方式、原对象、精确 guard、效果/费用和目标方法；generic DTO、snapshot 字段或 CLI 可发送不证明该消费者接通。未登记却实际读取资格/发出限制的路径使覆盖 unknown，不能只测 Viewer 或一条 helper。

有限正常例必须同时满足本设计已声明的主体/绑定/控制关系/委托边/参与方/效果/证据/复核节点、来源对象与proof、编码/解码字节、调用数、时限和并发预算。逐项取limit/limit+1，再把同一正常前态的完整闭包一起核；嵌套列表逐成员计数，不能先全库扫描再截断，也不能把只读摘要当完整原件。超限不得删掉一个控制关系、确认方、复核机会或原未结义务来通过。

本版本的有限接收点分别实量：ordinary R的64subject/256binding/64controller/4active M、同时2case及epoch最多4case；M的64subject/256wallet allocation，R/M raw state128KiB；每root完整source≤64件、每件≤16KiB、总≤1MiB，最多5invoke共用该source预算。保护profile则为请求64KiB、16总签名（guardian≤12）、16证据项、64完整来源对象及其raw/解码各256KiB、raw state32KiB、完整I/O256/128KiB。不同profile不能借对方余量，原更严限额仍取严；source不足以证明完整集合时拒绝该组合，不以容量上限重新定义产品政治人数。

ordinary完整I/O各512KiB是该新manifest内部预算；raw Vec按原编码取最坏2N+5，独立核输入409615B和输出344069B的完整组成，并与真实encoded长度比较。单根5次I/O双边界5MiB、共享source1MiB、staged/索引2MiB共8MiB仅为相关host预算，VM内存/fuel、上层进程与原cache另外受其合法上限约束。

保护请求按[设计的两层编码](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-consumers-budgets)分别实量原payload、编码后的`ModuleCommandEnvelope`及最终`ModuleCallInput`；`payload`和`action`两层普通Vec均计入，另记录state、ctx/封套和完整output。原请求64KiB、raw state32KiB与完整input256KiB须同时满足，不能以原始项合规推定完整输入合规。使用当前schema允许、签名有效的完整请求和可解析state，准备容量内正例及边界两侧的组合；原始项合规而完整input超限时须在模块调用前拒绝，零本次业务效果。若schema无法构造某一边界，给出更紧的可达上界及依据，不用未知字段或放宽原字段限制填充。合成编码形状只验证序列化膨胀，不代签合法业务请求的准入或运行结果。

保护host设计分额仍为原件64+解码128+source256+source解码256+input256+output128+staged128+response64+scratch256=1536KiB，小于其2MiB槽；4个槽预留8MiB不代表整个进程内存。核编码后envelope、输入副本和codec临时结构的同时存活峰值及分额归属，确认合计受对应分额和共享scratch限制，超限组合被拒绝；不能只用wire字节数推定实际分配满足。必须同时核实际WASM和cache限额，分额算术不代签现有实现的峰值或全部资源验收。

普通程序的独立计数为每M512次及关闭16，case的primary128、appeal120、关闭8共256；4个M和epoch最多4case为`4×528+4×256=3136`次，再加registry维护256与关闭16，完整epoch为3408次。按其单invoke保守1025D/3E及原payload≤31KiB对应capability reserve544单位，完整预留上界为3493200D/10224E/1853952capability单位；单根5invoke为5125D/15E/2720capability单位。三种量分账到各原真实payer/subject/grant，不可互兑；这些是有限保留，不是实际扣费。少一个维护/关闭/appeal保留、以新case/epoch搬移未结束事项、偷用另一payer或任意补给时拒绝新受理；已承诺阶段和合法结束必须使用原保留，不能让前序修订耗尽。实际较小完整I/O及原payload分别按原公式核其调用预检与actual，而非每次扣全profile上界；若实际图多一个hook/调用，重新计入，不能拿声明的3408遮掉额外工作。

对实际模块分别计算完整 canonical 输入/输出 envelope、state、effects/emits、真实payer及各source/purpose的预检上界与实际metering。不能只量内层业务字节、把同module两个ordinal合一、借另一个主体资金、让missing owner成为免费执行者，或把费用上界整笔当实际扣款。无实际模块调用的原读路径不新增业务费，原认证/限额/审计成本照原reader；query-only不重投业务、推进原业务nonce、释放hold或刷新资格窗口。

并发满槽、慢I/O、取消、超时及响应发送失败均保留计量与原件直到可安全清理，不重开隐形池或无限wait。原结果查询、申诉/恢复和合法安全退出不能被一般新申请耗尽；是否使用保留槽及费用按原有限策略验证，不能以“系统忙”为由丢掉已受理案件或超过原期限仍默认胜出。

同一精确候选、原事项/主体、source cut和允许披露范围下，Viewer、pure API、Agent分别说明：本人主体与代表边界；来源/资格和blocker；外部OC、事项权利与quota；原高影响动作及当前有效范围；已发生/未发生、费用与历史；原请求及下一步/复查点。`accepted/passed/ready/pending`与实际effective分开；当前失权不泄漏私下控制图、原件、密钥或钱包关联细节，也不抹除本人依法仍可读的历史。公开、本人、现代表、已离任代表及无关人作独立对照；公开最小断言必须能支持其声明范围内审计，不能以隐私为由省掉所有可复核因果。

由独立读者在去掉推荐标签后说明锁定/委托/退出或申诉的成本、哪些权利仍待决、最早可影响哪次后续决策，以及一种合法替代路线，再实际选择原入口并读回原结果。缺正式surface/producer时准确报告缺项，不用截图、静态卡片或模型解释代签玩法闭环。普通治理不成为首局、独立经营或原恢复的强制门槛。

## 2. 结果接收

每个方法保存已固定候选、实际层次、完整输入身份、独立预期与差分、真实入口/原receipt、失败切点、资源与字节计量、当前读权限和未执行部分。缺环境/producer/索引记具体blocked或not_run；观察到不变量违反记failed；方法及静态检查存在不产生当前实现、正式可用、平衡、真人或发行结论。整组接收须覆盖原GCB003–006与GAME019六项适用义务，不能用局部通过抵消其余未测范围。
