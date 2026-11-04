# 工业创作、模拟、试点与准入验收手册

- 生命周期：`active`；本文保存可执行方法，不记录本轮运行通过。
- Owner role：`qa_engineer`；Runtime、WASM、工业与治理 owner 提供真实候选和适用入口。
- 技术合同：[工业创作、确定性模拟与受限准入](../../world-runtime/runtime/industrial-creation-simulation-and-admission.design.md)。
- 唯一主要求：[AC-WR-GR-009](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#ac-wr-gr-009)；Trace refs: O7-REQ-3d713ab7-87c2-41df-9c89-827859c23313。

## 1. 使用范围、候选与结果记录

IC-M02 的源核对、IC-M03/08 的明确算例可以不运行游戏完成；其结论只写“静态预期/算术核验”，不写工业模拟成功或试点完成。其他步骤须在原任务已授权、具备真实目标能力的环境执行。缺 evaluator、pilot/资源/退出 adapter、准入或正式消费者时记 `blocked/not_run` 和准确缺项；不得借普通 `ScheduleRecipe`、伪 artifact、改状态或手工追加 receipt 绕过。

运行前固定 canonical repository、source commit/tested tree、测试入口及参数、世界/branch/checkpoint、authority profile、runtime/manifest/ABI、所有候选/基线 artifact 和 Profile、metering schedule、`IC-BoundedV1` 限额以及执行者范围。引用真实现行 authority、资源 owner、有限 custody 和退出规则；不要用本文的 tick/数量例伪造世界、签名或授权。先保存统一 read anchor 下的输入和前态，测试后用同一查询合同回读并保留原始 receipt；版本或 authority 变化时另立实际场景记录。

每个场景报告：前提/刺激、预期、实际、source/环境、准确入口、通过/失败/blocked、证据位置及未覆盖项。至少对账经济余额、held/consumed/released、工厂/工作/容量/custody、proposal/simulation/pilot/admission、准入有效性、ordinal/去重、原 Root/Child/receipt、奖励/需求/版税和 outbox。不要只断言 HTTP 返回、页面 toast 或某个 status 字符串。

required tier 覆盖同候选单 Runtime 的成功、逐项拒绝、幂等和基本恢复；full tier 增加独立权威/跨节点、并发顺序、真实持久化、正式 Viewer/API/Agent 和适用的真实试玩。静态检查、fixture、业务运行和真实发布分别标注，禁止从局部绿推断全链完成。执行命令采用该实现候选的真实入口，本文逻辑方法名不是现存 CLI。

<a id="ic-m01"></a>
## 2. IC-M01：提案、来源、选择与修订

**前提。** 准备一项有完整 source/dependency 闭包、明确基线替代、受限 scope、正反假设、风险、criteria 与退出的真实候选；另备同候选的缺来源、未知依赖和越权版本。资源前态可读，但不提前占用。

1. 读取机会与提案预览，随后登记完整提案；回读 proposal identity/revision、payload digest、材料版本、审查缺项和所有合法下一步。预览/登记不增加材料或费用占用、recipe/factory、资格、能力、生产或奖励。
2. 每次只移除来源、假设、作用域、风险、退出、判据或一个必需依赖；预期具体缺项/拒绝，无半个能力。使用错误 actor/owner 时拒绝，不能拿 OC、旧发布者或 vote 数补权。
3. 复用相同请求键和内容，读取同一结果；同键改行为/资源/退出，必须冲突。仅文字 annotation 保留原 bytes；改变候选因果内容、scope 或判据产生 linked revision，旧模拟/试点批准不能搬过来。
4. 比较继续、补证、缩小、延期、停止与既有路线，分别核对时间/资源/锁定/风险和保留价值。对有活动试点的 abandon 只发停止/处置，不立即删除记录或返回全部投入；无义务的关闭保留原失败和历史。

**证据与判据。** 保存原/后业务投影、提案内容摘要、修订关联和明确拒绝；合法登记只形成审查事实。证明作者材料能继续编辑另沿 PPE 手册，不用此场景代签实际构建/源使用权。

<a id="ic-m02"></a>
## 3. IC-M02：同输入模拟、来源隔离与可复算例

### 3.1 可立即执行的只读源核对

在固定源码版本读取[铁锭行为源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs)、[共享模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)及[Runtime 消费者](../../../crates/oasis7_runtime/src/world/economy.rs)。独立记录基线的 `4 ore + 1 carbon → 3 ingot + 1 slag`、生产电力 8、duration 1、marker/recipe ID 检查，以及真实 consumer 从 factory ledger/builder 电力读取输入的事实。

将设计样例的候选电力 6 明确标成“未制作的假设”，手算或用普通整数计算器复核以下值。没有实际 candidate bytes、world 或 authority 时到此停止运行部分，不运行 WASM 或提交发布。

| 输入/期望 | 基线 | 假设候选 |
| --- | --- | --- |
| 申请 3 批、矿石 12、碳 3、生产电力 18 | `min(3,12/4,3/1,18/8)=2` 批；用 8/2/16，产 6/2 | `min(3,12/4,3/1,18/6)=3` 批；用 12/3/18，产 9/3 |
| 仅生产电力变 12 | 1 批，用 4/1/8，产 3/1 | 2 批，用 8/2/12，产 6/2 |
| 申请 1 批但矿石 3 | 0 批，材料不足 | 0 批，材料不足 |
| recipe/设施 marker 不匹配或申请 0 批 | 拒绝计划 | 不允许因电耗更低跳过原检查 |

这里整数除法取可支持完整批次的下界，duration 不是批数乘数。记录由原代码得出的参数和候选假设；表格只说明逻辑结果，不是任何真实 world receipt，也不证明节能平衡合理。

### 3.2 真正的模拟验收

**前提。** 实现候选确有正式隔离 evaluator 与可验证输入/结果登记；实际制作的候选、基线和 authoritative snapshot 可取回。准备常态、资源不足、scope/退出反例，并把所有场景/seed/依赖/limits 固定成一个输入闭包。

1. 由有权服务端 evaluator 分别运行 baseline/candidate，每场景两次；这就是权威验证运行，不先额外运行 producer 再漏记 verifier。3 场景共 12 调用，客户端预计算不作为权威结果；部署另设复算层时完整计入额外调用/fuel。对照结果摘要、资源/产出/失败、调用计量和全部 read-set；同一输入结果相同。合法集合重排后规范化一致，有语义的步骤顺序改变必须得到新输入身份，不能排序掉顺序含义。
2. 核对两方案仅差声明的 candidate 变更；修改初始余额/设施、seed、runtime/工件/Profile/schedule 或任一敏感假设，必须形成不同输入/结果身份。不能在 candidate 分支补库存或改 failure criterion 才判成功。
3. 尝试读未声明/未授权 ledger、即时网络/clock、注册/激活模块、创建设施/job、扣电/材料、发送 effect 或赚奖励。预期无法发布这些业务 delta；结束后经济/权限/registry/工作/奖励/outbox 投影不变。允许的证据登记只是元数据，须独立列出，不能把整个 state root 不变当错误断言。
4. 模拟结果由客户端自填 `verified`、错 artifact hash、结果摘要不一致、缺一个两次运行结果、缺 read-set、超 fuel/output/bytes、未知 schema 时，正式采纳均不能通过。对 wall-clock watchdog/进程中断记录 incomplete/未知，不能将其作为确定性失败证据。
5. 相同输入换 transport ID、重连、恢复后再次请求，返回同一 simulation identity；缺证阻断不能通过重复累积成多份正面 evidence。
6. 模拟后只改无关对象，再改 decision-relevant 余额/设施/authority。前者在验证完整 read-set 后可保留原证据来源；后者要求新输入和模拟。若实现无法证明无关，按 stale 处理，不猜测。

**判据。** 同输入的结果、权限和业务隔离达到设计不变量；full 在实际合法执行集合重算同一输入。局部 executor 构造、Shadow hash 或 build hash 相等都不是本场景通过证据。

<a id="ic-m03"></a>
## 4. IC-M03：有限试点、完整成本和退出

**前提。** 真实 verified simulation、现行试点 authority、所有投入 owner、执行/保管/退出 adapter 已存在，Root/Child/资源及 receipt 能联合读取；用于正式验收的 candidate 内容须与模拟一致。trial purpose 明确，商业结算消费者不会接受其产物/receipt。

### 4.1 静态成本闭包

先复核设计 §9.3 的完整调用图：start controller、planner 1、completion controller 1、planner 2、completion controller 2、close controller。这里输入/输出各为正且 ≤1 KiB，均 1 emit/0 effect；两个 planner 无 new_state，四个 controller 各一次 new_state；实际原产品验证为 native，不隐含第七个 WASM 调用。manifest limits 明确 `max_output_bytes=1024,max_effects=0,max_emits=1`，并有正有限 fuel/memory。核读[实际/上界费用函数](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)：Compute 每次 3，实际 planner Electricity 2/controller 3，但现有 upper_bound 对所有调用均预留 Electricity 3。费用 payer 还须按[当前计量入口](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)检查 exact artifact owner 与 `ResourceKind::Data` 的 Compute 扣费；fixture 中同一有权主体承担全部费用，其他情况分 payer 验证。目标 hold adapter 必须让 preflight 与实际 charge 只消费一次受保护余额。任一前提不符就重新预算。

| 切点 | 原料/产物 | Electricity | Compute |
| --- | --- | --- | --- |
| start 总承诺 | 矿石 12、碳 3；隔离容量 9 铁锭/3 炉渣 | 生产 18 + 6次预检上界×3 = 36；start 自身费计入该总量 | 6×3 = 18 |
| 第一轮 2 批已完成 | 消耗矿石 8/碳 2；隔离产物 6/2；原料余 4/1 | 累计 12+3+2+3 = 20；余 16 | 累计 9；余 9 |
| 第一轮后选择 close | 未消费原料 4/1 单次返还；产物按批准 close 处置，不进普通库存 | 再用 3；最终消耗 23，返还 13 | 再用 3；最终消耗 12，返还 6 |
| 完整 3 批并 close | 全部材料消费；隔离产物 9/3 | 实际18+2×2+4×3=34，返还预检余量2 | 累计 18，无剩余 |

close 必须是该 fixture 已支持的有界处置，不是把数据标记 closed。源模板并没有这些 controller；此表是目标 adapter 的成本合同，不证明它们已制作。只提供 Electricity 33 时拒绝开始完整例，不能先锁全部投入再发现缺 close 的 3。任何附加 validation、tick、重试或异常 handler 都须纳入最坏调用闭包，不从平台费用中漏掉。

### 4.2 真实试点刺激和断言

1. 只读 prepare：输出精确 scope、`[100,104)` 或实际等价有限窗口、3 批/2 次排程、工作节点/lot/资源/费用/custody 上限、当前 owner 和退出接收器；前态无任何 hold。显式 start 后一次产生 permit、Root/Child、真实占用及 start 费用；不得同时保留普通可消费余额。
2. 运行 2 批并读回 actual；逐项对比上表对应真实量。选择 close，检查未消费投入只返还一次，已消费/损失不复原，产物 purpose/custody 保留，无 royalty、需求、交付或稳定产线奖励。
3. 重置独立授权 fixture，运行完整 3 批并 close；真实判据同时满足才可得到 `met_criteria=true`。故意令输出不合格或失败，即使有产物且预算耗完，也只得到失败结果；不得用于准入正面证据。
4. 在 cap、cap+1、批次 0/超额、负数、整数溢出、重复资源条目、第二个活动 permit、超工作节点/lot、输出容量不足时分别尝试 start/step；首个非法边界原子拒绝，不截断数量或丢弃副产物。
5. 到期前可完成的合法步骤、恰在 end_tick 的新 sink、工期跨过有效窗口、running→stopping→paused→resume、主动终结和 authority 撤销分别测试。暂停只在原范围仍合法、已开始的不可中断段按原处置完成且未开始本次终结性返还/销毁后成立；失败/撤销/到期停止不能转 paused。end_tick 不包涵；到期后零新试验 sink。resume 不延期、不加预算、不复活已关闭试点。另在第一轮后合法释放部分未消费资源再 resume：Electricity remaining 从16释放8后剩8，下一轮上界生产6+planner3+completion3+close3需15；虽然 consumed20+15仍低于旧cap36，也必须因当前hold不足拒绝，不能暗中从原 owner 余额补扣。执行真实 close 时可消费自己的退出保留，只为剩余必要退出步骤保留预算，不因再次要求已花的同一 close 保留而永远无法关闭。
6. 与普通工业请求及另一试点争同一真实资源/设施，分别调整权威提交顺序。总占用/消费不超过资源量，原公平/分配规则继续适用，无 creator/OC/重试优先；换 transport ID 不复制预约、重置预算或插队。
7. 退出时原 ledger/合法目的地不可用、当前退出权失效或 close adapter 失败：保留预留有限量、责任和具体 blocker，不把库存删除/瞬移，不宣布 closed。只有合法后继处置完成后才关闭；先验缺少任何有界退出路径的请求必须在 start 前拒绝。
8. exact recipe candidate 为 pending_validation/validated_pending_admission 时，经真实 `ResolvePilotCandidate`/trial adapter 的合法正例可以试点，formal recipe 状态仍未 active，普通 ScheduleRecipe 仍不能使用。撤回、安全校验失败、retiring/retired 或禁止范围的负例不能试点；变更后的真实 build/validate 类型分别测自身受限 adapter。避免以先取得 formal admission 作为试点前提的循环。
9. 尝试将 pilot 产物或 `IndustrialReceiptV1` 改 purpose 后发给商业/许可/需求消费者，必须拒绝。准入成功也不自动把此前 pilot 结果转成正式收费/产量。

**证据与判据。** 联合保存 permit、累计 cap/exit reserve、每 step ordinal、真实账本/custody/Root/Child、退出和关闭 receipt；同单位 `initial=consumed+released+remaining`，跨材料按绑定配方关系验证。没有真实支持的 adapter 时这一节运行部分为 blocked。

<a id="ic-m04"></a>
## 5. IC-M04：四阶段准入与消费者防绕过

**前提。** 完整合法提案、同修订 simulation、closed 且 met_criteria 的 pilot、当前治理来源、scope/有限期限及原类型消费者均已存在。建立一份可合法正式使用的正例及每次仅缺一个条件的负例。

1. 按完整顺序申请准入；提交前重新检查当前 authority、证据、criteria、review/revoke/处置及 scope。取得一个 exact `IndustrialAdmissionRefV1`，但尚未排程、生产、交付或收费。
2. 分别只提供源码/构建、Shadow、technical Approved/Applied、ordinary passed、client 签名、自报 pilot 成功、失败 pilot、未关闭 pilot、旧 revision、超 evidence scope 或未知 schema。不得形成有效工业准入；明确缺哪一阶段/当前条件。
3. 在批准后、正式执行前改变 owner、delegation、scope、authority、有效时间或复查事实；新使用拒绝或缩到当前确切可用范围，不能自行续期/替换。对已知准入结果的查询保留原历史，不把 authority 漂移改写成过去未批准。
4. 对实际候选的 native Build/Schedule/Validate、对应 WithModule 路径、tick directive、直接事件/内部 producer、恢复未生效动作分别运行同一条件矩阵。无法经公开 API 构造的路径由候选的真实 contract regression 驱动；不得为了测试直接写 canonical state 绕过 guard。遗漏入口明确列未覆盖。
5. 对 recipe `pending_validation/validated_pending_admission/active/restricted/retiring/retired` 与 creation admission 做组合：只有原六态/当前 scope 允许的动作可执行，历史 `admitted` 不覆盖退休。对已有 WIP/transit/buffer/terminal 各按原处置合同检查，creation/license 不自动续授完成权。
6. 有 current admission 但缺适用许可时按许可合同拒绝；有 license/open 但 admission 过期、能力退休或工厂无权时同样拒绝。paid 正式 recipe 的成功完整 batch 进入原 Root/Child 完成账；pilot、半批、计划、失败、重复生产/交付引用不能成为可赚版税事实。
7. 合法争议、重提、scope 缩小和既有路线保留明确动作；没安全恢复时返回 `no_safe_creation_recovery`，旧 receipt/独立资产保持，无默认能力。

**判据。** 全部 gate 在真实提交处相交；正例只产生声明范围的一份准入结果及另一次合法使用结果。技术 release、许可、配方六态、creation 和工业工作各自身份清楚，不能只比较 UI 禁用按钮。

<a id="ic-m05"></a>
## 6. IC-M05：身份、原子提交、未知结果与恢复

**前提。** 候选支持对真实提交边界故障注入及 snapshot+非空 journal 恢复；保存完整业务/控制状态和原版本。所有新增逻辑记录及原资源/工作图能联合读回。

| 注入点/刺激 | 必须独立对账的结果 |
| --- | --- |
| proposal 分配身份/证据登记前后 | 不存在已发权威 ID 却无对应 committed disposition；重试无重复证据。 |
| pilot start 的 permit/root、资源扣/hold、容量/custody、start 费用、ordinal/去重、receipt/journal 任一 prepare 点 | 确认未提交时全体旧态；成功时联合出现；不能只回滚材料而保留费用或 request consumed。 |
| step 的 sink、输出/custody、累计 cap、工作/完成、domain receipt 与索引任一 prepare 点 | 全体提交或零部分效果；正常完成与停止不能同时消费同 lot。 |
| close 的释放/返还、输出处置、退出费用、终态和 receipt 任一 prepare 点 | 未消费量只返一次，closed 必须无未结义务；失败保留原合法有限量与责任。 |
| admission 的当前授权消费/结果、类型转换适用部分、receipt/去重任一 prepare 点 | 不出现有排程权而缺证据链/结果；不产生半份 scoped 权限。 |
| 提交后响应丢失、通知/投影失败、进程重启 | 查询原身份返回原结果；不能把 unknown 当没发生再创建。 |

同一语义分别测试：相同 key/相同 payload、相同 key/不同 payload、不同 transport key 指向原 simulation/pilot step/admission、跨 world/branch/实例重用 ref、篡改 exact capability version、replay 旧结果以及显式新 pilot attempt。只有已关闭旧 attempt、真实新选择、当前资源/权限再次通过时才能分配新 attempt；重试不能自行增加 ordinal。

同时提交同一 step、close 与完成、到期/撤销与 first sink、两次 admission revision，分别固定两种合法权威顺序。各顺序允许结果不同，但每个槽至多一个结果、总量守恒、scope 和授权边界不变；不得使用线程完成时间或客户端点击时间作裁定。

重启后从原 manifest/schema/artifact 的 snapshot+tail 重建。依次移除旧工件、证据 refs、去重索引、累计量或处置记录，预期拒绝相关写入并保留诊断，不以当前规则/默认0恢复。已提交结果不会重新调用计费试点、发资源或外部 effect；已知未提交 pending 才能在当前 authority 下重验。full 覆盖实际 authority-profile 持久与独立故障域，局部 clone 回滚/单次 fsync 不能代签。

另将根 generation 注入 `StatusUnknown`，在原 pilot 之外尝试无关账户 mutator/save/依赖派发：旧 writer 必须整体拒绝继续，直到精确持久决定确认且内存对齐。局部缺工件/权限隔离与根提交未知分别测试，不能把所有故障一律降为只封当前试点。

**判据。** 对比恢复前后 canonical root、资源/工作/custody、完整阶段链、结果数量、累计量和索引。任何一项不一致都失败，不用“最终页面看起来正常”替代。

<a id="ic-m06"></a>
## 7. IC-M06：读权限、机会成本和跨 surface 同义

**前提。** 同候选有正式 Viewer、pure API、Agent 读入口；按同一 read anchor 和 audience 查询。至少设置作者、有权审查者、使用者、无权旁观者四种实际权限，不用角色名称自行赋权。

1. 逐个读取 proposal needs_evidence、simulation failed/incomplete/verified、pilot prepared/running/paused/stopping/closed、admission pending/rejected/effective/expired/revoked/restricted/review_due/unknown，与独立 technical release state 比较。确认每个词对应设计中的事实和 receipt。
2. 核对 source/version、scope/期限、预测与 actual、held/consumed/loss/可返还/保管、primary blocker、next_action/next_recheck 和可行替代一致。模拟节能估计不得写成已节省，pilot 输出不能显示正式产线/版税收益。
3. 让玩家解释继续、补证、缩小、延期、停止和既有路线分别锁定什么、可能失去什么、还保留什么；记录误解和缺项。设计文案审读可以先执行，真实理解/试玩结论必须使用真实参与和实际环境证据。
4. 断连、旧缓存、缺证、未知提交和无 safe recovery 时，各入口显示真实状态；重连查询原结果，不自动试点、重提或扩大授权。scope 改变必须重新确认，Agent 不能由推荐代为消费资源。
5. 对私有源、ledger、身份断言和未公开风险做读权限负例；拒绝泄漏完整他人闭包，也不能以报告差分暴露隐藏余额。当前无读取权仍可说明被阻断的对象类别及真实下一步，不伪造零值。

**判据。** 保存同 anchor 的响应/截图/实际 Agent 观察、权限拒绝及独立解释。图稿、离线文案或单 API 绿不证明三面真实接通。

<a id="ic-m07"></a>
## 8. IC-M07：旧历史、启用边界及有限回滚

**前提。** 固定含旧 ModuleRelease/Profiles/普通 job 的真实兼容样例及新 creation 数据，准备能够读取新 schema 的回退候选。旧 artifact、Profile、receipt 字节可对账。

1. 在新功能未启用和明确启用边界前后分别恢复旧数据；旧发布状态、身份、已有合法能力和结果保持原版本，不补造 simulation/pilot/admission，也不默认把 legacy Profile 变六态 active。
2. 新 capability、新因果版本或更名重复材料尝试走 legacy consumer，必须按新路径处理或拒绝；原合法旧工作继续/处置仍按原合同判断，不因新字段缺失被一刀切删除或复制。
3. 新 reader 可读、writer/exit/consumer 任一缺支持时保持相应新路径 blocked；不降级到普通 ScheduleRecipe。模拟历史的读取不授正式写入能力。
4. 复核 PPE exact FROM artifact freeze 的既有正负例：注册/安装/升级/激活/重发布/rollback TO 的原检查保留，其他已存实例与既有 Profile/资产不因该 marker 自动停用。另行撤销 industrial admission 时只通过其独立 authority 生效，不能修改 marker 含义。
5. 带活动 pilot、已关闭失败、已有 admission 和旧/new receipt 回退；能完整读取/reduce/保留预算及义务才可写，旧 binary 不识别则拒绝相关写而不是删字段。已消费投入、历史准入及合法收费不能因 binary rollback 追溯清除。

**判据。** 历史 byte/语义、原 marker 行为和当前 guard 分别保存；本场景不授权真实生产 rollback，运行沿原任务环境和发布合同。

<a id="ic-m08"></a>
## 9. IC-M08：有界性与完整约束组合

先用设计 `IC-BoundedV1` 复算最小闭环：3 场景×2 方案×2 重算=12 调用，若每次 fuel 上界 1,000,000，则总 12,000,000，小于 32 调用/40,000,000 fuel 上限。8 场景×8 步×2 方案×2 重算需要 256 调用，虽然各局部数都在上限内，组合必须拒绝；不能删掉负例或只计算 candidate 通过。

在真实候选逐项验证输入 1 MiB、32 evidence refs、8 场景、8 有序步骤、32 总调用、40,000,000 总 fuel、单次较小 ModuleLimits/10,000,000 fuel/64 MiB/256 KiB、1 MiB 报告的精确上限和上限+1。未知必需字段、重复集合键和缩短后已破坏 closure 的输入都拒绝。采集真实峰值与返回 disposition；未取得测量就不声称吞吐/时延达标。

试点测试 64 tick、16 step、16 node/未结 lot、32 resource/capacity 条目及各上限+1；同时制造输出最坏量超过 custody、close 费用不足、工期跨窗口、异常停止需要额外未预算调用和两次共享资源争用。任一组合不可行都在首 sink 前拒绝；不能以局部电力足够推断能完成。begin 前未提供有限退出路径拒绝，begin 后合法目的地变化则保留 bounded obligation 和下一复查，不谎称已关闭。

模拟调度按实际 actor 检查一个活动/一个等待、每世界最多 64 个等待，队满拒绝新请求且不驱逐原任务；相同请求重连保留位置，不重复消耗 world 资源。调度顺序可以影响等待时间，不能改变同输入的结果摘要。试点与普通工作使用原同一资源池/公平规则，关闭/释放后才可被其他请求消费。

**证据与判据。** 输出完整调用图、输入/输出尺寸、fuel/内存、生产与运行费用、退出保留、最坏产出容量和时间边界矩阵；每条依赖都有实际 producer/consumer 和成本。静态算术只证明所声明前提下的可行性，真实性能、持续公平、权限恢复和多节点行为需对应候选的 required/full 证据。
