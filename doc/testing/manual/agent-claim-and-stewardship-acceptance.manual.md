# Agent 认领承诺与失权连续性验收手册

- Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`。
- 生命周期：`active` 验收计划；Last reviewed：2026-11-05。以下方法尚未执行，不表示实现、真实入口、平衡或玩家验收通过。
- 产品范围：[AOS001](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-001)、[AOS002](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-002) 及各自原 AC。团队扩张由 [MFG-V08](agent-industrial-manufacturing-system-acceptance.manual.md#mfg-v08) 独立接收。
- 专业主源：[认领经济合同](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md)，经济数值和准入取其[原玩法 PRD](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md)。本手册不另定取得路径、费用、处罚、补贴或申诉权限。

Trace refs: O7-REQ-612f4d3d-e831-48f9-9dfc-f0844690e6cf、O7-REQ-8b94a27c-bbae-4349-94c1-f1f40f46e287。

## 1. 同候选前置与独立判断

固定 source/tested tree、配置及 policy revision/hash、world/chain/genesis/branch、epoch 规则、请求主体/target Agent、控制来源及当前读权、真实能力目录、候选集合和工业目标。普通经济 Claim、首次注册/绑定、制造交付和 `agent_disposition_v1` 各自采用真实支持 profile；不能把可见未认领 Agent 或兼容 snapshot 中的候选当作任意正式取得许可。

分别记录三个层次：`test_tier_required` 的真实领域 action→状态/事件/投影及独立拒绝；`test_tier_full` 的实际提交/持久恢复和支持入口组合；按[Agent 自测手册](agent-runtime-self-playtest.manual.md)组织适用的独立 Agent 路线与图形观察。受控 fixture 只证明其实际层次；缺正式来源、producer、身份、持久化或入口时记录具体 blocked/inconclusive，不能将 fixture 或组件通过提升为全链结果。

每个案例保存原输入和预期，再执行受测 action。使用原 ledger、policy、来源证明及世界事实独立计算金额、时间和权限；不直接调用待测 quote/helper 生成 expected。记录前后 liquid/restricted、原资金池、bond provenance、burn/treasury/upkeep sink、取得容量/预留、identity/source generation、请求/结果/事件及未决义务。经济费用与实际 Runtime/模型/存储调用费用分账，不把预览本身无经济效果解释为全链运行免费。

既有源码基线可用于选择真实切点：[claim runway 测试](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/claim_runway.rs)、[claim choice 测试](../../../crates/oasis7_viewer_runtime/src/viewer/runtime_live/tests/claim_choice.rs)、[epoch 处理器](../../../crates/oasis7_runtime/src/world/agent_claims.rs)及[原子 claim 准备器](../../../crates/oasis7_world_state/src/world/agent_claim_economic_publication.rs)。这些源码及历史测试名称不是本手册已执行的证据；当前单候选理由与多候选比较缺项须分别验证。

## 2. 方法与证据

<a id="claim-01-choice"></a>
### CLAIM-01：同一候选的完整承诺与替代选择

对应[候选承诺](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#claim-choice-commitment)。选择同一权威 cut 中真实可读且适用当前取得 profile 的候选，先记录来源，再读取正式 producer 的既有 quote/choice 字段。

1. **费用与 runway 的独立数值。** 固定原 slot-1 默认 `activation=100、bond=200、first upkeep=25`，关闭本例不适用的自动补足条件，分别准备合格余额 325、374、375。预期 upfront 均为 325；确认后余额分别为 0、49、50，可付完整后续 epoch 分别为 0、1、2。已有阈值 2 下，前两项有低 runway 警告；余额足够时警告仍为建议，不成为新 canonical 拒绝条件。下一维护为当前 epoch+1；预计 grace 说明须与本例完整 epoch 算法和真实政策一致。不能把 375 的 50 余额显示为三轮维护，也不能省去 upfront 中的首期 25。
2. **单候选的可核用途。** 在原支持的工业阶段及正式能力目录下，先从位置、当前任务、已实际启用能力和目标所需条件独立写出该候选的用途、帮助与风险，再核 producer 的五类既有理由。唯一候选不等于适合：缺目标、未知能力、缺电力/接口等适用条件时，不得只凭 body/module 名称写成已能完成目标或自动推荐。原机体与能力准入继续由其主责合同决定。
3. **多个候选的完整比较。** 固定 A/B 两个真实不同候选，独立记录各自位置、用途、目标帮助、主要风险和适用取得条件。原五类理由中逐候选分段的 ID 必须与同 cut 的 `candidates` 一致，不得用 A 的一句共享理由覆盖 B。保留 `compare_candidates_first` 与可读差异，不新增客户端 ranking/score，不静默换到“最佳”对象。选择 B 后仍显示 A 的理由、只显示裸模块数组、漏掉 B 的主要风险，均失败。
4. **缺项、集合与替代路径。** 分别移除真实目标输入、能力目录项或一个理由来源；再用受限读权、分页/预算导致非完整候选集合。显示准确缺项、当前比较范围及补证/等待下一步，不能将缺项记为零风险，不能把当前页只剩一个候选宣称全世界唯一。比较其他候选、等待可解释条件、补足合法资金和维持原规模均只提供原可用选择；`candidate_rationale_missing` 的安全回退不能作为完整正向 producer 已交付的证据。
5. **承诺包整体读取。** 在同一次确认前读取用途差异、upfront/资金来源、后续 upkeep/runway、主动释放冷却、欠费 grace、闲置 warning/reclaim 和真实恢复/替代入口。Viewer 中英文、pure API 与获准 Agent 对同窗口保持同义；没有当前读权的入口保留不可用。静态稿和文案也核算同一费用恒等式：若示例 activation=4、bond=8、first upkeep=2，则 upfront=14；不能因标注“示例”允许写成 12。数值不同的独立示例须明确自己的完整输入。

证据：权威输入/候选范围、独立算式、原 quote/choice、各入口的完整字段与显示、状态前后差。查看、切换候选、比较和刷新均不创建 claim/bond/预留或任何补贴；尚无完整消费者时记录所缺 producer/入口，不能仅以字符串存在判定通过。

<a id="claim-02-confirmation"></a>
### CLAIM-02：确认重验、一次效果与原子拒绝

对应[确认重验](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#claim-confirmation-revalidation)。正例须使用当前真正允许普通 Claim 的来源和 profile；已绑定主 Agent 的额外取得、制造 actor 的再取得等，应按现有工业取得 guard 选择预期拒绝，不能借兼容候选列表绕过。

| 场景 | 输入/执行切点 | 独立预期 |
| --- | --- | --- |
| 完整合法确认 | 同主体/world/target/当前政策与有效取得来源；全 liquid 375、原 slot-1 默认费用；一次真实 Claim | 一件有效 claim、bond 200、liquid 50、activation burn 50/treasury 50、首期 upkeep 25；五项合计 375。请求结果、容量和当前控制来源与同一权威结果一致。不得先更新 UI owner 再补扣款。 |
| 预览后单变量失效 | 分别改变当前身份/签名、目标已被取得、取得来源、容量/预留、合格余额、政策/声誉用途来源、治理限制或目标能力/用途依据 | 当前适用准入重新核验；影响已确认对象/承诺的漂移要求刷新/重新明确确认或原子拒绝。不得沿旧 quote 授权、静默调换 target、扩大资源支出或遗漏 blocker。若仅 warning 改变且原合同允许继续，不能添加未采纳的硬阻断。 |
| 两主体争同目标 | A/B 都读到旧 quote，交错至真实发布；另测相同主体两个请求和不同取得入口抢同容量 | 最多一个成功结果；失败方无部分扣款、claim、bond、容量豁免、自动补足或 refund。制造预留与普通 Claim 共享当前容量，不以入口拆账。 |
| 首次受限补足 | 原资格满足、无 active grant/现成 restricted、原 dedicated pool 恰可覆盖 upfront 缺口；配对池少一单位、已有 grant、非首次/额外 slot | 仅合格原正例同次补足并认领；失败不先搬走池款、不造 pending 审批或新资格。已有 grant 与非首次分支不叠加资助，不能以新请求/退出/换候选重新领取。 |
| restricted/liquid 来源 | 合法 mixed funding；额外 slot 尝试 restricted；来源到期/撤销发生于读取后 | 按当前准入与原 restricted-first 规则，记录准确来源与各项实际花费。restricted 不变成 liquid/材料/设施能力；过期事实不能继续被旧 quote 使用。反例不部分消费。 |
| 发布失败与 ACK 丢失 | 在候选准备、持久提交前/后、结果返回前故障；保留同一逻辑原请求与已确认摘要 | 未提交不产生新效果；已提交只取原结果。未知不猜失败、不新建请求再次认领；完整恢复另按 CLAIM-04。不能仅观察 UI 超时就断言 world 未改变。 |

每个拒绝分别比较全写集、原池和容量；不能只断言返回错误。注册/恢复绑定、首聊支持、正式经济 Claim 与制造交付另作身份对照：单纯已能操作 Agent 不生成经济维护义务，有历史未决责任也不因当前 claim 集合为空而消失。

<a id="claim-03-loss"></a>
### CLAIM-03：不同失权原因、真实处罚与旧义务保全

对应[失权连续性](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#claim-loss-continuity)。各分支保留原原因、作用域、已确认结果、当前控制来源、未决交付/债务和真实下一步；先在相同候选的各自原合法路径建立前置。

1. **主动释放。** 读取真实已接受的 `release_requested_at_epoch`，默认 ready 为该值+3，不从 claim 创建时间起算。ready 前不退 bond/腾容量；重复 Release 请求不能重开时钟。ready 时先按原 epoch 路径结清到当前的 upkeep，再按 provenance 退剩余 bond。正常 Released 不加 20% 强制处罚；无钱支付 upkeep 时进入其原欠费分支，不能借新 disposition 的 bond 抵欠费政策伪装旧 native 已可主动释放。
2. **欠费边界。** 取合法 claim 已付至 epoch0、upkeep25、bond200 全 liquid、之后无可付余额，逐 epoch 处理且不跳跃。epoch1 首次缺费时，原 `grace_epochs=2` 给出 `grace_deadline=1+(2−1)=2`；epoch2 仍处宽限，epoch3 才 forced reclaim。此时 arrears75，先扣75，剩余125按原20%扣25，退款100；bond 的 75+25+100=200。分别在 deadline、deadline+1 和 deadline 前补足真实欠费，检查因果和原子结果。若测试跳过 epoch，使用原实际首次处理切点与记录的 deadline 重算，不能把逐 epoch 算例套作跳跃时钟的预期。
3. **闲置边界。** 在足额支付维护、无已到期释放的前置下，固定真实最后有效控制 epoch；默认7发 idle warning、10 forced reclaim。warning 不是终态；合法控制推进可改变后续闲置判定，普通查询、重连、改显示焦点不算控制推进。无欠费的 bond200 按20%罚40、退160；maintenance 与其正常 sink 另记，不能把闲置回收写成主动结束。
4. **资金原源。** 对 restricted/liquid mixed bond 分别选择固定原 source treasury、有有效原 grant、grant Expired/Revoked。按原 resolver 核对实际 restricted refund 返回原 restricted 或准确原 treasury bucket，liquid 仅返其原合法 liquid 路径；总和与被扣部分对账。原 activation/已付 upkeep 不退；历史 restricted 不洗成可转让余额，也不重开首个支持。
5. **范围有限的争议限制。** 用[保护效果目录](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-action-catalog)中真实已采纳的合法 case；验证 governance identity freeze 只影响其治理资格，不能因此接管 claim、没收独立资产或变更 owner。实际影响 Agent 行动的已授权 policy 限制，须有其准确 target/scope/有效窗/来源；解除只去掉本件，其他仍有效限制继续。举报、旧历史截图和原生内部 helper 本身不产生新处罚权。
6. **旧履行与失效权限。** 在合法控制期间建立真实已有服务/交付责任及受限 grant，再使 claim 终态或根控制来源失效。当前已确认世界结果、旧合同与债权/责任继续可追溯；旧 owner/source/grant、缓存签名和待执行意图不能继续提交新效果。来源失效、权力收窄与真正 claim 终态分别观察，不能一律删除世界对象、重发支持或重置未决义务。对仅保留旧义务的有限保全动作，仍需原合同的当前技术授权与业务范围。

证据同时包含原 claim/限制/合同原件、当前 ledger/sink、source/identity generation、历史 receipt 和当前读面。当前 `agent_claims` 列表删除终态项不等于存在完整历史消费者；有权用户必须能经真实原结果及义务来源读到结论。经济转让/退休/报废的两阶段及遗产语义另用 [AD01–AD08](agent-economic-disposition-acceptance.manual.md)，不能把其新 family 当作上述全部旧路径的现有执行器。

<a id="claim-04-recovery"></a>
### CLAIM-04：当前重新准入、原请求恢复与真实下一步

对应[当前恢复](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#claim-current-recovery)。为同一历史结果建立以下成对场景，记录每项路径是否真实可用及其 authority；选择“不可用”时说明具体 blocker、已保留内容和原可行动下一步，不生成假按钮或自动动作。

| 场景 | 必须观察的正常、拒绝与恢复结果 |
| --- | --- |
| 原权利仍存续、技术身份失效 | 原合法主体按既有身份恢复/轮换与当前来源证明显式恢复；未取得新权利，不改变 old claim、付款或历史。旧证明、伪造来源及被撤销 grant 均拒绝；恢复读面不等于恢复执行权限。 |
| 已真正结束后再取得 | 分别用满足和不满足当前取得 profile、资格、资源、容量/预留、治理/反滥用条件的请求。合法新取得形成新的当前控制来源/必要 generation，旧 receipt 只证明历史；不得承继旧 grant、重开 starter 支持、免 upkeep 或消除旧责任。若当前路线规定只能制造取得，普通 Claim 重认领仍拒绝。 |
| 有限复核/申诉与安全停止 | 对真实已有限制按[原复核与到期合同](../../world-runtime/runtime/protective-governance-authority-and-recovery.design.md#protective-review-expiry)送达当前有权接收者；待决不是撤销/驳回，已接受也不清除另案。没有该种合法救济入口时明确不可用，可保留证据/查询或安全停止，不能新造通用资产申诉权。 |
| ACK 丢失、重复与提交未知 | 故障恢复沿同 world/branch、原请求与摘要、原提交身份查询。Committed 只恢复原结果，NotCommitted 按其真实 parent 保留既有已提交阶段；G1 StatusUnknown 保留原 World 隔离直到权威恢复，不能在旧 World 继续执行、换 ID 再扣费或重发 pending。更换入口/Provider/当前显示对象也不能形成第二结果。 |
| 当前权限与历史私有内容 | 同一结果分别由原当前合法 reader、已失权旧 owner、只读公共者和执行 delegate 查询。按准确当前读权裁剪；旧可见性不授权现在读取新 owner 私有上下文，历史审计不等于可操作。拒绝不泄漏别人的债务/草稿/Prompt 正文，也不能把无权读取解释为“无未决义务”。 |
| 刷新、重连、重放和换对象 | 变更 world/branch、target、owner/source 或版本后撤下旧作用域的可执行显示并禁止旧 scope dispatch；保留原请求身份、摘要、nonce、未决结果及其当前获准的精确查询，不删除未知原件。新快照不以缺字段/空列表推导无债、可重领或自动恢复。实际恢复后的 receipt、余额、容量、义务及有权读面与原因果链一致。 |

full 恢复固定前态 snapshot 加非空事件 suffix，至少分别跨 Claim、ReleaseRequested 与真实终态切点，和同输入的无中断参考逐项比较 ledger、source/identity generations、请求结果、容量及历史义务；不能只读末态 snapshot。这里禁止退出、换候选或复用旧请求自动重发支持，不禁止原 issuer 在原独立 reissue 条件满足、当前权限及来源有效时明确另行合法发放；该发放不能由本手册或重认领动作自动触发。

原注册/返回的 ENTRY、首聊资格的 GATE、工业制造 MFG、形成合同 CF、保护治理 GOV 和经济处置 AD 使用其已有方法与同候选输入交叉核对，只调用本例实际依赖的场景。完整动作图的实际调用/预算、提交恢复、Viewer/API/Agent 当前消费者及真人理解各自记录结果；本手册不把文档关系、组件渲染或一条兼容路径的通过相加为全部入口完成。

## 3. 交付记录与判定

记录每项 method ID、实际 profile/层次/候选、前置原件、执行序列、独立预期、实际结果、反例和恢复后的状态，以及未覆盖入口。仅当正常、单变量拒绝及受影响恢复均取得同候选证据才签该项；新增缺陷与修复保留原失败，复测绑定新候选。金额、状态、来源、玩家措辞或设计稿任一不符均留下具体差异，不能靠降低产品设计/实现标签掩盖。

实际实现与运行结果写入对应 PR/Task。静态设计、链接和算术检查只证明本方法与合同的准确性；真实提交、生产恢复、平衡、玩家理解与发行仍依原要求独立验收。
