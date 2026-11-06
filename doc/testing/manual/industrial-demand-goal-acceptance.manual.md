# 工业需求目标与匹配结算验收手册

- 生命周期：active；Owner role：qa_engineer；Last reviewed：2026-11-06。
- 目标合同：[需求目标执行设计](../../world-runtime/runtime/industrial-demand-goal-execution.design.md)；产品：[SC-31](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#sc31-leaf-requirements)。
- 本文定义独立验收方法，不是本轮运行记录，也不假定存在同名测试命令、endpoint 或已实现 profile。原实现历史、实玩与发行结论保持原证据范围。

## 1. 验证范围与前置

按[测试层级](../../../testing-manual.md)执行：required 覆盖每个独立叶子正常路径、拒绝、单次效果与入口语义；full 保留多目标、跨窗口取消/到期、多阶段/多批次/实际损耗、容量争用、revision 链、持久化/补偿和长期三入口一致性。不得只测失败关闭，也不得用一个正例代签所有 profile。

实施候选必须固定 commit/tree、world/branch、runtime/ABI/artifact 与 profile revisions、原生或模块路径、当前 authority/owner、fixture 来源、scope、测试层级和真实入口版本。新 profile 未激活时记录 unsupported；不把 source fixture 手工塞入状态当作正式 producer 已验收。

普通输入 profile 的合法 fixture 由正式入口接受真实 recipient 的工业用途/input obligation，再接受 G。目标量不可来自任意 UI 文本、假订单或测试者直接改 F/C。fixture 使用原 recipe catalog、原工厂/Site 控制、材料、电力、产品验证、物流和实际有限容量；成本/维护/损耗无权威时相关路径不可声明可用。已有 native_regional_supply_use_v1 用其原 2 iron、窗口、独立主体和供应用途前置，不能因同一执行 adapter 扩大该 profile。

每案记录：

1. 同一 committed source cut、输入 authority/revisions、完整性/当前读权与原 request/operation 身份。
2. 原 ledger/工厂/route/goal/receipt/index 的 before/after、原 journal 范围与 world root；不得只比较 UI 标签。
3. T、累计 committed、C、P/D/X、F/R、实际 surplus/loss、五类 bucket 和仍未结清义务。
4. 真正发生的 production/物流/需求/容量/费用 effect 与对应单次键；未发生或不适用明确记录。
5. 实际入口输出、primary blocker、允许动作、cost refs、next_recheck；重试/恢复所查询的原结果。

<a id="dg-m01"></a>
## 2. DG-M01：同一快照与低于、等于、高于 quantum

对应 REQ-SC31-001 / AC-SC31-001；Trace refs: O7-REQ-bfd4b2f4-6b21-4c4a-961d-387b220e762f。设计：[目标与数量](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-authority)、[正常例](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-preview)。

准备独立 G2、G3、G4 切片。每片 recipient 的真实 2 batches gear 用途需4 iron，已合法分配的既有 input 分别2/1/0，因此新需求2/3/4。供应者原 smelter 的完整 one/two-batch 输入、主副产物去向、E、实际验证和容量合法。只读比较1 batch与2 batches，核原配方3iron/1slag、8E每batch的业务量；所有实际其他费用分别展示，不将这组配方量误称总成本。

断言 preview 前后 canonical world、目标数/承诺、材料/电力、hold/queue、模块计费和 tick 不变；没有新 root/revision。目标/原 quote/容量/receipts 必须同 cut。以旧目标、新库存拼接，以及缺一个必要 authority、unit 不匹配、数值溢出、索引落后、缺页的对照均不能产出 available/matched/surplus 结论。保留可读已知事实和具体 unknown/blocker，不能补0。

证据为原 source refs/cut、实际 recipe/quote、数量投影与完整 before/after 根。只证明合同与读取无效果；不证明最终交付、数值平衡或实玩。

<a id="dg-m02"></a>
## 3. DG-M02：支持集、显式接受与并发承诺

对应 REQ-SC31-002 / AC-SC31-002；Trace refs: O7-REQ-3c8dcb96-cb2b-4ec8-824a-5cb50b75f060。设计：[支持矩阵](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-profile)、[提交](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-commit)。

G4 分别确认 full2 batches 与 reduced1 batch；核输入/能源/主副产物、q=4或3、immutable baseline、原 operation 与目标 allocation 共同接受。不能把 full 因漂移静默缩为 reduced，不能产生半个 batch；produce_full 也不能把全部6产物都算目标承诺。按原能力分别检查补料/调运、hold 和 stop/defer 的真动作及费用，不存在的动作不出现在可选集中。

同一 G 的两个并发确认竞争 A；只有当前剩余量足够的原顺序成功，后者重算/拒绝，C 不超过 R。同一物理资源上的两个不同 G 仍受原材料/电力/route 争用约束，目标 C 不产生免费物理 reservation。相同 request key 重发返回同一结果；同 key 改 payload 拒绝；不同已授权 intent 不因 payload 相同而被错误合并。

finite G/U 对照必须仍只允许原2iron供应，不出现拆分、追加生产或 supplemental 按钮；另测缺 authority/未注册 capability。每项记录原请求与接受/拒绝、q/C、ledger、能力发现和消费者 actions。依靠直接构造 allocation 的 unit test 不能代签正式接受路径。

<a id="dg-m03"></a>
## 4. DG-M03：生产、交付和 matching 单次

对应 REQ-SC31-003 / AC-SC31-003；Trace refs: O7-REQ-ebb96559-33a9-4029-af82-a709c71225e0。设计：[matching](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-matching)。

G2 生产3后 P增加但F=0/R=2，副产物另列。显式提交原合法 transfer2；原 received/目的地接收与 matching 同事务后，D/F=2、R=0、C=0、生产者 iron surplus1 仍在原合法账本。运输实际费用、损耗、目的 credit、route release 只由原 writer 发生一次。

独立负例包括：错误 recipient/Site/material/unit/world、原目标已取消/到期、合法但不关联本 allocation 的 receipt、production receipt、到货提示而无原 settlement、旧已匹配 receipt 换 goal/key/revision。它们不得减少本目标或发奖；真实物理结果仍按原合同保留，不能用目标失败删除收货。

若候选含正式声明的 terminal profile，另外证明 arrival只更新D，合法terminal结果才增加F；同量先delivery后terminal不能双计。未声明 terminal 的普通 profile 必须 not_applicable/unsupported，不能以该拒绝证明 terminal 正例已通过。实际损耗/partial 按原 receipt输入，分别核 matched、unmatched、loss、C释放与R。

证据包括原 production/transfer/settlement refs、单次与跨目标归属索引、所有数量和原资源账。full 添加多个 goals、分支终态/仍pending、取消与到达不同 canonical顺序；每种合法顺序独立守恒，不要求不同业务顺序得到同一目标结果。

<a id="dg-m04"></a>
## 5. DG-M04：满足后停止、盈余和五类工作

对应 REQ-SC31-004 / AC-SC31-004、AC-SC31-005；Trace refs: O7-REQ-32d009e3-a57c-4994-8876-4cbc7517b2a3。设计：[旧工作处置](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-disposition)。

在目标 matched、cancelled、expired、unknown 和 reduced 时，分别准备 accepted-unstarted、active WIP、in-transit、buffer-held，以及已有正式 terminal profile 的 terminal-pending。required 逐格核实际支持/拒绝与保留义务，不把没有该 bucket 的首版静态样例当作完整五类证明。

matched 后从原生、模块、direct/trusted、tick/后台、Agent 等实际可写路径提交旧计划，均不得产生新的生产 sink；已消费WIP按原合同完成/blocked，不能为“停止”退款或消失。transit保留原运输；buffer与surplus保留原账本、有限容量和实际持续成本。terminal-pending继续保留recipient、destination、admission、capacity、provenance，既有义务不因新profile不支持而丢弃。

分别检查G2产3后剩1、G4产6后剩2及各自slag；没有销售、销毁、伪交易、奖励或重复credit。容量在quote后变化须按原writer拒绝/保持，不能把hold解释成无限免费仓储。reduced目标保留历史F，新T低于F时R=0且不退款。

检查目标恢复/扩大只给fresh比较；旧stop/release不自动撤销。full制造到达/生产完成与cancel/expiry/reduce的竞态，达到处置预算后确认admission fence立即生效、完整closure保留且可续接，每个旧operation只结清一次。证据是全writer准入清单、closure/处置refs、前后原账和当前下一步。

<a id="dg-m05"></a>
## 6. DG-M05：真实 shortage 后的补产或停止

对应 REQ-SC31-005 / AC-SC31-006；Trace refs: O7-REQ-62129528-2238-4733-8b8e-d226dc40943d。设计：[补产](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-supplement)。

G4先显式reduced1batch并真实交付3，核F=3/R=1/C=0、原baseline/actual/loss不变。从该结果分别走两条独立分支：补产一次合法1batch并仅分配/交付1，其余2持有；或stop/defer保留R=1且不创建child。确认补产所需原料/电力/验证/容量和物流全部重新检查，不将前一次的条件续借。

同parent/shortage cut两个不同key并发确认，最多一个child；重复原key仍同结果。前一allocation存在未解释pending时不能假定损失再补一份。改变recipe/factory/path/purpose的“补产”必须走原causal cutover/new root且W=0，不能给它quantity-only标签绕过。

full记录至少三次显式parent-linked结果/选择与snapshot恢复，保证每轮一次，不自动递归；原目标F、损耗、baseline和已结清receipt不因新revision迁移或重复。验证多轮并不强迫游戏自动制造新的shortage。

<a id="dg-m06"></a>
## 7. DG-M06：每项条件漂移和 fresh 裁决

对应 REQ-SC31-006 / AC-SC31-007；Trace refs: O7-REQ-544d08be-9d56-4106-bd80-89c46786bfdf。设计：[提交](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-commit)、[事务](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-recovery)。

每次只改变一个权威条件：target/current revision、F/C、recipient/control、输入库存、E、factory/recipe candidate、quantum/yield、输出/副产物有限容量、route/目的接收、窗口/原profile的terminal容量。用原quote提交，断言fresh requote或原子拒绝，新sink/hold/义务/费用/奖励为零；已存在原effects不被清零。普通首版不以“pending”创建新队列或保留幻影C。

再组合目标被另一供方满足与本次first sink竞态、目标变化与due arrival竞态。按canonical order记录合法结果及下一动作；不使用客户端时间决定先后。unknown恢复仅重报价，不自动继续。terminal条件不适用首版时明确列未运行适用profile，不把N/A写成全叶子无要求。

<a id="dg-m07"></a>
## 8. DG-M07：重复、崩溃、乱序、恢复与补偿

对应 REQ-SC31-007 / AC-SC31-008；Trace refs: O7-REQ-1e60706a-d6b6-43e8-b3f9-86af391700fd。设计：[事务与恢复](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-recovery)。

分别在prepare后commit前、物理/goal共同commit后但响应前、outbox发布前/后、snapshot与非空journal恢复处注入故障。确认前者无半个物理effect或allocation，后者只查询原结果，不因timeout新建intent。快照安装缺matching/source index、缺parent、F/C不符或未知required schema时拒绝，不能按当前余额修复。

在原根 generation 发布点分别取得已证 NotCommitted、Committed 与 StatusUnknown：仅第一种可继续旧 World；第二种安装原完整候选/receipt且维护失败不回退；第三种必须隔离全部旧 World writer/tick，保留原 operation/candidate 查询线索，不只阻塞当前 goal。恢复判定前尝试无关写入与 tick 也必须拒绝；核清原提交结果后按原服务闸门恢复，不能以超时、另一个可读快照或更换 key 解除隔离。

重放/乱序覆盖原submit、production/arrival、matching、goal变更、supplement、hold release和已支持补偿。核production、原delivery、F、surplus处置、原费用/容量释放各至多一次；普通profile无reward。完整replay不调用模块/物流/外部adapter，不重报价、延窗或刷新顺位。跨入口重连仍用原identity。

full加入不同goal复用同receipt的攻击式负例、缩量后历史F>T、旧schema读取边界、合法linked compensation与不支持的需求更正。没有原补偿政策时应保留争议/unknown，不编造减F或退款。证据为原intent/result、generation、索引/closure、所有原账以及live与replay同root；局部recipe/transit测试不代签该组合。

<a id="dg-m08"></a>
## 9. DG-M08：正式三入口、权限和共享预算

对应 REQ-SC31-008 / AC-SC31-009；Trace refs: O7-REQ-34f477af-5903-48fd-8889-ceb8b1028ef3。设计：[正式消费者](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-consumers)、[预算](../../world-runtime/runtime/industrial-demand-goal-execution.design.md#demand-budget)。

在同一candidate、goal和cut由正式World Service读取，分别经Viewer、pure API和正式游戏Skill/Agent执行受支持比较与确认。对照T/C/P/D/X/F/R、quantity disposition与goal state、五bucket、surplus、blocker、支持集、成本、next_recheck；accepted/produced/arrival/terminal/matched不可互相改名。推荐、读查询、按钮刷新与Agent重试都不创造新选择。

分别以recipient、实际供应方、委托撤销后主体、无权旁观者读取/提交；当前authority不足不得用旧缓存泄露预算/目标/其他竞争者信息。历史归因仍显示有权读取的原控制事实，不能被新owner覆写。

按共同宿主每项limit及limit+1、超大单条、嵌套读取、并发槽/每主体槽、取消边界逐项测量；计原始+解码source bytes、对象/refs/fetch、原规则求值、overlay和response，不只计最终摘要。超预算返回明确incomplete/budget_exceeded，原goal/物理义务不丢失；完整数量与处置不能以截断页冒充完整closure。shared pool不得因SC31与mature profile各调用一次而翻倍。

证据保留真实请求/响应/截图、API reference/capability、Agent skill和模型轨迹、原世界结果与实测资源。任一正式入口未交付就逐项记录未实现/未验证，静态SVG、共享DTO或文档检查不能填补。玩家理解、可继续理由和实际操作体验按原产品实玩要求独立验证。

## 10. 结果记录与结论边界

每条方法独立记录通过/失败/未执行、实际candidate/environment、原REQ/AC、原来源与异常。required通过不签full；首版普通输入交付不签全部terminal/商业/其他profile；有限G/U正常例不签普通full/reduced。新设计采纳只证明合同可交付，checker只证明结构和引用。

提交证据时列清尚缺的writer、正式入口、授权、配置或测试环境，保留原目标设计义务；不能因实现尚缺把设计标N/A，也不能因只剩实现/实玩未跑就重复宣布专业设计没有承接。
