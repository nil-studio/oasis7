# 受治理扩展期限、收缩、替代与退出验收手册

- 生命周期：`active` 验证方法；Owner role：`qa_engineer`，共同审读为 gameplay、治理、Runtime、工业/配方、WASM 与读面 owner。
- 本轮只做源码与合同审读、定义静态算例；未运行本功能、世界、模型、Rust、WASM、浏览器、真实发布或平衡测试。固定源审读基线为 `9f332f2973c31f5948269b970c7aa5d4e7a6fe82`，实际执行另记候选 source commit/tested tree。
- 原 recipe 首版玩法政策：[GAME-018 首版受治理配方生命周期](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#governed-recipe-lifecycle-policy-v1)；技术合同：[受治理扩展生命周期](../../world-runtime/runtime/governed-extension-lifecycle.design.md)。本手册 §§1～9 消费其 `governed_recipe_factory_v1` 参数，不另设期限、额度或退出动作；§10 另按原普通类型政策验证完整组合。
- 产品要求：[GR-002](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-002)、[GR-003](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-003)。Trace refs: O7-REQ-ce8c31b1-5294-4acd-a65f-2012bcbef911, O7-REQ-ac12295b-63a5-4440-9924-deaedafd7965。

## 1. 执行范围、前态与证据

首版只覆盖同一 exact Pure recipe/artifact、至多两个显式工厂、每厂至多一个未闭合 job、每 job 至多两批。正常例使用三份各一批的工作，任一时刻至多两个未闭合。物流、转换、迁移、付费许可及未声明模块回路均不在此 adapter；出现这些依赖时报告不适用/缺 adapter，不能删依赖后继续原请求。零版税须有原许可 authority 明确合法零费或 `not_required` 的依据，模块 Data/Electricity 费用仍独立存在。

首个 supported binding 仅为 PPE 新 ID 铁锭候选的两种输入、铁锭/炉渣输出及明确产品验证闭包；其他 Pure recipe 不能只满足工厂/批次上限就适用，须先有对应行为来源、原准入、完整消费者与退出 adapter。本手册的铁锭算例不代签一般配方。

| 前态来源 | 可以证明什么 | 尚须取得什么 |
| --- | --- | --- |
| 原 `BuildFactory`/合法资源或材料转移、已完成的原生产，以及真实原账本/设施 receipt | 设施、当前 builder/owner、输入/输出 ledger、实际可用资源和历史产物；准备阶段的运输/建设已独立闭合 | 当前治理、工业/配方、设施、产品、外部性、费用与目的范围仍合法；不能改 world 文件造余额或瞬移原料 |
| [PPE 交付与发布](../../world-runtime/module/player-published-entities.manual.md#ppe-manual-release)中的真实源码、工件、build proof、`ModuleReleaseSubmit`/Shadow/Approve/Apply 及最终性 | 当前交付/技术发布事实；旧 owner 或旧 role 批准不能代替当前资格 | 原提案→同输入模拟→有限 pilot→正式工业准入及本设计的生命周期 consumer；局部发布测试 fixture 不代签真实 signed producer |
| 本设计的授权、有限处分、可信来源绑定、原子替代、保护预算、恢复与读面 adapter | 实现后才可提供通用 grant/current-effect/exit 的正式消费证据 | 固定基线上仍缺的目标部分记 `blocked/not_run`；方法名不是现有 CLI，不能用手工 DomainEvent 或旧普通动作冒充完整能力 |

每次执行固定实际仓库、source/tree、世界与 read anchor、Runtime/ABI/manifest、工件及 Profile digest、原始 grant/处分内容、当前 authority revisions、原 job/intent/request identity、metering schedule、输入与输出容量、所有 payer。保存前后 grants、资源/hold/材料、job/槽位、receipt/journal/root、实际调用 trace 和读面；独立逐量对账，不能只看 HTTP 成功、状态标签或总 root 不同。

`test_tier_required` 覆盖实现候选的单 Runtime 正常、逐项拒绝、并发顺序、事务故障及 canonical replay；`test_tier_full` 再覆盖实际持久化/重启、多节点和正式 Viewer/API/Agent 入口。静态算例、组件 fixture、业务执行、真实发布分别记证据层。缺前提时保留精确阻塞和下一步，不把“有方法”写成通过。

| GAME-018 AC | 主方法 | 独立判据 |
| --- | --- | --- |
| [07](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-07) | GEL-M01 | 范围与处分完整、稳定身份不变，漏项不产生部分改变 |
| [08](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-08) | GEL-M02 | 每次发布/激活都用当前资格，旧批准不能续命 |
| [09](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-09) | GEL-M04 | 所有实际效果入口遵守同一范围/期限，内置独立能力仍按原规则 |
| [10](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-10) | GEL-M05 | 最小退出有界且有权，历史资产/义务不被撤销或退款 |
| [11](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-11) | GEL-M03、GEL-M07 | F1 只有一份可执行授权，F2 留在旧授权，失败无半次切换 |
| [12](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-12) | GEL-M03、GEL-M06 | 完整有限闭包、按当前 payer 保护的预算足以闭合，无截断成功 |
| [13](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-13) | GEL-M07 | 同意图单次结果，恢复不重跑旧调用、不放大权利/预算 |
| [14](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ac-game-018-14) | GEL-M08 | 待决、技术可用、当前可执行、退出阻塞和历史完成分别可读 |

<a id="gel-m01"></a>
## 2. GEL-M01：完整授权、原存量处分与范围身份

**输入与消费者。** 准备实际发行者/管辖、操作者、稳定能力身份、exact recipe/artifact/Profile、两个准确工厂、半开有效区间及当前原权限；通过正式生命周期确认 consumer 提交。缺稳定归属、重叠判定、闭包索引或当前 authority 时，停止运行部分并列缺项。

1. 分别提交只阻止新发布/激活、限制既有后续效果、受控退出三种明确选择；组合时必须有顺序与共同有效边界。逐次删除既有实例/工作处分、范围或依赖，独立期望是整个改变不成立：旧有效范围、新 admission 资格、既有能力及资源均不发生半份改变。到期仍按原已生效期限，缺材料不能延长它。
2. 读取该范围全部未闭合工作、原 ledger/槽位/hold、依赖和义务。空集合必须有完整索引与一致 revision 的证明；截取第一页/前两项、漏掉一个隐藏 producer、缺索引或并发新增一项均不得确认闭包。
3. 用同一内容换 transport ID、grant ID、module/version 标签、显示名、recipe 别名或请求者，检查其实际范围关系不因换名消失。将独立内置 canonical recipe 作为控制组：本扩展的身份不得反向改写内置能力的 authority；未知来源不猜测为该扩展或免费 legacy。
4. 逐项撤掉当前 builder/资产使用权、工业准入、recipe lifecycle、产品验证、外部性或许可依据。批准/旧 receipt 仍可读，但相应新效果和最小退出都须重验；旧权利完整时再显式重提，不给旧请求自动换授权。

**证据与恢复。** 保存原完整声明、独立范围/闭包枚举、当前权限来源、拒绝原因及原/后相关投影。新的有效处分需新确认；不能补写一行 status 后把原失败变为完成。

<a id="gel-m02"></a>
## 3. GEL-M02：待决旧批准与发布/激活漂移

**输入。** 一个真实待决发布请求，固定批准的 payload、scope、exact artifact、owner、role binding 和所有版本；分别在确认前发生到期、撤销、F1 收缩、F1 被替代、artifact owner 转移、审批角色失效、批准后 payload 改动。

从实际 Submit、Shadow、Apply/最终性消费及其他能激活该能力的正式入口逐一提交；独立期望是仅当前仍完整合法的精确范围成功。F1 失效而 F2 留权时，旧请求不能静默删 F1 后在 F2 部分成功，也不能改绑定到新 grant；明确拒绝/过期/取消或要求新内容重新确认。`ActionRejected` 与发布请求进入终态 `Rejected` 分别核对，不强造同一状态。

增加独立反例：F1 已进入限制/退出，但 G0 尚在原普通时间窗口内，从 PPE Apply、直接 install、重新 activate 请求该受影响 scope 的新发布/安装/激活或扩权。均须拒绝，不能只挡排程/完工而留下新 admission；F2 的未受限当前授权及技术 active 也不能覆盖 F1。另核“只停止新 admission”本身不擅自停止既有实例，两种处分不能混成一个全局开关。

重用相同请求与原 receipt、换客户端/节点/缓存、重启后再 Apply，均不能恢复已失效的资格。保留批准与失败历史；补齐当前 authority 后走原合同明确允许的新请求。还要从实际 [exact tuple admission freeze](../../../crates/oasis7_runtime/src/world/module_admission_freeze.rs) 对照：其范围是 `module_id/version/wasm_hash` 的新 admission，不能把这项现有能力报告成一般性既有实例停止、当前 grant 替代或安全退出。

现有 PPE owner/role/payload/freeze 回归源码可作为组件方法来源，未运行时只记“源码已有”；fixture 中的工件、批准或 finality 不作为本场景真实 signed/consumer 证据。

<a id="gel-m03"></a>
## 4. GEL-M03：正常先闭合、再原子替代的可复算例

### 4.1 真正来源与前提

复用 [PPE §8.3 新 ID 交接例](../../world-runtime/module/player-published-entities.design.md#ppe-creator-example)：实际制作的 module/recipe 分别采用该例的独立新 ID，并绑定实际新源码闭包、WASM hash、manifest 与唯一 RecipeProfile。不能只发布 Profile 就声称取得行为，也不能直接把内置 `recipe.smelter.iron_ingot` 改成新 grant 的私有能力。本例始终使用同一份已合法准入的 exact artifact/recipe，仅替换工厂 scope 的 grant；不做模块升级、全局卸载或第二工件预激活。

独立读取[铁锭源](../../../crates/oasis7_builtin_wasm_modules/m4_recipe_smelter_iron_ingot/src/lib.rs)和[共享模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_recipe_module_template.rs)：每批 `4 iron_ore + 1 carbon_fuel → 3 iron_ingot + 1 slag`，生产电力 8，计划时长 1。源中的 factory ID `smelter` marker 只决定模板匹配，当前设施和使用权还须原验证。

正常世界的实际 `iron_ingot` ProductProfile 必须与[默认经济 Profile](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)一致且 `maintenance_sink=[]`：源码默认函数确实使用空 sink，但安装只补缺项，不覆盖已有 Profile。因此不能从仓库默认推断任意世界也为空。实际非空时本数值例不适用；按[真实消费合并](../../../crates/oasis7_world_state/src/state/apply_domain_event_industry_helpers.rs)每批额外 `3×maintenance_sink` 另立完整场景，不修改原 Profile 造零成本。

F1/F2 均已合法建成，工厂 ID/标签和阶段匹配，实际 input/output ledger 分别属于原设施，容量足够。物料在 checkpoint 前已真实送达，准备成本与 receipt 独立记录；试验中无物流、替换或额外维护输入。由于排产使用各自本地 input ledger，[Runtime 消费者](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)的 world-fallback scarcity delay 不触发，实际 duration 仍为 1；以 `RecipeStarted` 中的真实 duration/ready_at 再核，不凭模板忽略 Runtime。

pin 实际铁锭 product validator 的 module/version/hash 和合法技术准入；其来源可核对[铁锭 validator](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs)与[product 模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs)。对 slag，必须由原产品 authority 明确 `not_required`，并确认当前 resolver 没有适用 `.product.*.slag` 候选；“查不到模块”单独不构成免验证许可。实际多 validator/hook/tick/module 回路时不得沿用本例六次调用。

### 4.2 初态与顺序

这里的 tick、工厂代号、资源数只定义验收输入，不是已存在的世界或伪造签名。所有目标 authority/保护预算/来源绑定消费者到位后才执行。

| 对象 | 冻结前态 |
| --- | --- |
| 授权 | G0 为 `[10,30)`，管相同能力在 F1/F2 的明确 scope；G1 申请 `[20,40)` 且仅 F1，仍 pending，不产生执行权 |
| 操作者 B | 当前合法 builder/操作者，生产 Electricity 至少 24；F1 input 有矿石 8、燃料 2，F2 input 有矿石 4、燃料 1；无旧未闭合 job，两 output ledger 初值分别记录 |
| recipe artifact owner P | 当前真实 owner/payer，保护三次 planner 上界，共 Data 18、Electricity 9；与 B 不同以显出付款来源 |
| validator artifact owner V | 当前真实 owner/payer，保护三次 validator 上界，共 Data 18、Electricity 9；与 B/P 分账；每次 start 同时保护该 job 的闭合份额 |
| 费用与原资格 | 两 exact manifest、所有原权限、明确零版税及 GEL-M06 预算成立；已保护资源不会被普通转移、维护或其他调用提前花掉 |

| canonical 切点 | 原动作/目标变化 | 独立预期 |
| --- | --- | --- |
| t18，先 start | B 经真实 `ScheduleRecipeWithModule` 为 F1/G0 确认 J1，一批 | F1 消费 4/1、B 消费生产 8E；J1 与原 grant/源/ledger 固定，ready_at=19；planner 一次；尚无铁锭/炉渣产出 |
| t18，后 restriction | 正式目标 consumer 关闭 F1 新排程，明确 J1 的一次 finish，处分窗口 `[18,22)`；F2 不变 | G1 仍 pending，F1 在新旧授权下的新排程均不预执行；J1 的原预算/责任保留，无新增批次、退款、移动库存或新工件 |
| t19 | J1 的当前 authority 与 bounded finish 仍有效，原 validator 一次通过并正式完成 | F1 output 增 3/1，J1 job/槽位/闭合预算与义务终结；资产留原 ledger，完成 receipt 保留，不把成品误记为仍在途/待处置工作 |
| t20 | 完整复验 F1 闭包已终结，以单次 CAS 提交 G1/F1 生效与 G0/F1 退出；G0/F2 保留 | 任一 authoritative 状态对 F1 至多一个可执行 grant；G1 仅 pending 时不能 start；旧 F1 未确认请求不迁移 |
| t20，提交后 | J2 在 F2/G0、J3 在 F1/G1 各确认一批 | 两 planner，分别用原 input ledger 各 4/1、B 各 8E，ready_at=21；同一 artifact 继续供 F2，不能全局删除/冻结旧模块替代对象级切换 |
| t21 | J2/J3 各自当前原权限和 validator 通过，分别完成 | F2 output 增 3/1，F1 再增 3/1；三 job 各完成一次，无剩余该例生产义务；历史 J1 身份仍是 G0 |

**独立总账。** F1 累计耗 8/2、产 6/2；F2 耗 4/1、产 3/1。总消耗矿石 12、燃料 3、生产电力 24，产铁锭 9、炉渣 3；成品及副产物只进入各自原 output ledger。输入/输出余额均按初值加减，不能从 grant 切换得到库存。正常 recipe planner 三次、铁锭 validator 三次；每模块每 tick 调用分别是 `1,2`，不超过 Runtime 工程 profile 的每模块每 tick 上限 2。

**空闭包/WIP 对照。** 完全无待决工作/hold/义务且索引完整时，可以直接单次 CAS；J1 仍未完成、验证未知、槽位/hold/终结义务未闭合时，首版拒绝 cutover，G1 保持 pending。只在 G0/有限处分仍有当前权且未耗尽时继续 J1 的原 finish；不得让 G0 继续 drain 的同时让 G1 在 F1 开始生产。F2 的合法新生产不因此失效。

<a id="gel-m04"></a>
## 5. GEL-M04：全入口、期限与同 tick 顺序

**方法。** 对同一 grant/source/工厂，分别保留、去掉或篡改可信来源，再从以下实际生产者及权威提交消费者触发等价效果。组件级直接事件测试可以验证 reducer 的防线，必须标注层级，不能作为正式 action 或 signed producer 已覆盖的证据。

| 必须覆盖的路径 | 独立预期 |
| --- | --- |
| `ScheduleRecipeWithModule` → 普通 `ScheduleRecipe` → `RecipeStarted` | 正式来源绑定一路保留；[当前 economy 转换](../../../crates/oasis7_runtime/src/world/economy.rs)只返回普通 plan，其 `LegacyUnspecified` caller 本身不证明目标 grant 已接通；不能只保护第一层 handler |
| 原生/直接 ScheduleRecipe、内部或受权 effect、恢复后待处理 action、缓存计划、自动 producer | 使用该扩展依赖时必须同一 current-effect gate；没有可信匹配不能装成 legacy 绕过；独立内置 canonical 能力按原规则继续 |
| due completion、product validation、`RecipeCompleted`、prepared/root publication 与实际 reducer | 在产生新验证/结算效果前重验；[当前自动完成路径](../../../crates/oasis7_runtime/src/world/economy.rs)和[原生命周期 reducer](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)均须被目标 adapter 覆盖，不能只拦玩家点击 |
| 原 artifact 仍技术 active、旧 module cache/旧 approved request、另一个节点 | 技术可用不代替当前工厂 grant；F1 拒绝而 F2 原 grant 有权时仍可执行；一般 scope 收缩不得擅自变成全局卸载 |

| 边界 | 期望及恢复 |
| --- | --- |
| `now < start`、`now = start`、`now = end-1`、`now = end` | 半开区间逐项复核；恰好 end 即无原普通生产权，清理事件尚未执行也不能多给一 tick；其他条件仍逐项合取 |
| 申请 t29 start、ready_at=30=普通授权 end，但缺完整到期处分或可靠闭合前提 | 初次 grant/工作确认即拒绝，无生产材料/电力 sink；不能把该输入当成一份“合法但从开始无出口”的既有 job。模块是否已在当前权下调用按 GEL-M06 独立分账 |
| J1 在 t29 带原有效预批准 finish/闭合预算合法 start，ready_at=30；t30效果前，finish 必需的当前工业或产品验证权限被合法撤销 | expiry 清理先/后于 completion 两种排序均无旧普通生产权且无当前合法 exit、零新增产出；保留已消费4/1及生产8E、原job/义务。不得因完成已到时而擅自续权或恢复输入 |
| 同一到期输入，但在原有效确认中已预绑定此 job 的一次 finish，窗口 `[30,34)`，其余当前原权限和闭合预算均有效 | t30 两种清理排序都只可消费该有限 exit：validator/完成各一次，原ledger新增3/1，保留原sink；不取得普通排产/新发布权。t34或权限/预算失效则退出阻塞、零新增产出，不能沿用上个成功判据 |
| 未预定的管理收缩/撤销与 completion 同 tick | 用 canonical 有效 receipt 顺序对照：completion 先提交则保留合法历史；改变先提交则后续 completion 按明确处分重验。客户端点击先后不决定结果，也不倒删先完成资产 |
| Apply/预计算到正式 sink 之间 authority 漂移 | 原产出/hold 不部分发布；已实际发生的计量按原事务与原 trace 对账，不承诺退还已消耗计算，也不为重试重新授予次数 |

每条路径保存触发入口、实际 effect boundary、当前 revision、拒绝/允许的原理由和资源差分。某路径尚未接入则该 AC 不通过，不能用 façade 已拒绝代替。

<a id="gel-m05"></a>
## 6. GEL-M05：原存量、最小退出与义务

复用[原 recipe 六状态及六类工作处分](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md)和[工业 admission 合取](../../world-runtime/runtime/industrial-creation-simulation-and-admission.design.md)，不新增退款、补偿、迁移、转换或“退出即销毁”语义。

| 原存量 | 输入/刺激与独立预期 |
| --- | --- |
| 仅客户端排队、旧 approved 而未确认 | 不算已消费工作；当前确认须重验，失效不迁移到 G1、不继承旧队位或批准。若目标实现另有 accepted/no-sink hold，必须有原真实记录与受支持的释放动作，不能从普通 ScheduleRecipe 想象出这种状态 |
| 已接受但尚无不可逆 sink | 只处理原真实未消费 hold/容量，按当前有权动作单次释放/保持；不从未实际付款或未持有的来源退款；不新建免费 root |
| 已消费、未完成 J1 | 保存原材料/电力 sink、job identity、原 ledger 和未尽义务；只可做声明的有界原 finish。任何新增批次、改 plan、改 destination、产出提前入账或重新运行 planner 均拒绝 |
| 已完成、已结算或已形成资产 | 旧 receipt、已耗费用、产品与所有权不被 grant 改变擦除；此后资产操作仍须当前原资产权限，不由旧完成记录赋予无限后续权利 |
| 在途、buffer 待处分、terminal-pending、转换/有偿许可义务 | 完整闭包必须发现；首版缺 adapter 时拒绝把它们纳入本 profile 的切换/退出，不能把库存标记 completed、清空队列或瞬移到新工厂 |

每个 exit 必须在原有效确认中预绑定准确工作、最小动作、当前原权限、payer、资源/容量、剩余调用次数、数量和 `[effective_tick,effective_tick+4)` 截止；原 job 有更早合法截止时取更早者，不能到期后现场发明宽限。对 `[18,22)` 逐项验证 18、21、22；22 已截止。原 grant 的普通期限与有限 exit 的当前有效性分开检查，exit 不恢复普通排产权。

逐项耗尽 call/资源预算、撤掉 owner/工业/产品资格、使输出容量不可用、令验证失败或未知、让有界处置超时。期望 `exit_blocked` 且原已消费事实与不可避免义务保留，没有新产出、退款、传送、自动继任或无限轮询。修复当前资格仍不能自动延长原截止/次数；需要原合同允许的新明确处理，无法合法处理就保持可读阻塞。没有可靠有界闭合前提时不得先做不可逆新 sink。

原 `PauseFactoryProduction` 要求 Idle 且无 pending recipe job，不能拿它证明现成 WIP 暂停/处置能力。当前支持的 finish、拒绝和只读等待与未来 adapter 能力分别记录。

<a id="gel-m06"></a>
## 7. GEL-M06：完整闭包、真实付款人与有限预算

冻结实际 manifest：完整 canonical CBOR `ModuleCallInput` 至多 4096 bytes，`max_output_bytes=1024`、`max_effects=0`、`max_emits=1`、Pure/no new state、`max_gas=1,000,000`、`max_mem_bytes=16 MiB`、每 module 每 world tick `max_call_rate=2`。这些是目标 profile 上限，不是本轮测得的 fuel/内存/时延；实际工件做不到则拒绝该 profile，不改 limits 后沿用原批准。

独立读[费用函数](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)、[当前 payer 选择](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs)与[实际 WASM 输出计量](../../../crates/oasis7_wasm_executor/src/lib.rs)。令 `u(x)=ceil(x/1024)`，0 bytes 为 0；实际每次用 trace 的 `I_c/O_c`。模板写的 `output_bytes=512` 会被真实 executor 的 CBOR buffer 长度覆盖，不能当实测值。

| 账项 | 独立计算与判据 |
| --- | --- |
| 单次 preflight 上界 | `u(4096)+u(1024)+2×0+1=6 Data`；`1+0+1+1=3 E`。真实 input 更小时可另记实际预检值，但本完整承诺按已批准最坏上界保护 |
| 单次成功调用实际 | `u(I_c)+u(O_c)+1 Data`，`1+0+1+0=2 E`；实际 bytes 必须在批准上限内。未知/失败调用按真实原计量事件记录，不能编出成功 trace |
| 正常六次调用 | planner 三次、iron validator 三次：模块上界 `36 Data+18 E`，加生产 `24 E`，完整承诺为 `36 Data+42 E`。实际模块 `12 E`，连生产 `36 E`，实际 Data 为六条真实公式之和；不虚构 gas 值 |
| 各真实 payer | P：上界18D/9E、实际三 planner 为6E及各 trace Data；V：同样分别核 validator；B：24生产E。若实际主体重合则合并其全部义务，不重复使用同一余额；payer 来自当前 artifact owner 绑定，不从 requester 或 UI 猜测 |
| 原 fee 目的账 | `ModuleRuntimeCharged` 按原规则从 payer 扣实际费用，并增加 world `state.resources` 的相应 Data/Electricity；它不是凭空烧掉的第二份资源。正常前态目的账无饱和溢出时，world 增实际六次 Data 总和及12E；B的24生产E仍按原生产sink。一般边界另按原 saturating 语义核对，不把资源总量相等代替分账 |
| 每 job 的闭合份额 | start 时已保护一次 validator 上界6D/3E；最多两活 job，故并发闭合份额至多12D/6E。这是上述调用预算的子集，不是再加六/十二份费用。完成只释放真实未消费余量，已消费材料/生产E/实际模块费不退 |
| 调用闭包 | 每 job 至多一次 planner 与一次 validator attempt；native 治理/闭合 adapter 无 controller WASM。验证未知不再调用 module；重复读状态/到期清理不会刷新次数或制造免费重试 |

覆盖边界输入：1/2/3 个工厂、每厂0/1/2个未闭合 job、每 job 0/1/2/3批、输入/输出 bytes 上限及 +1、rate=2及第三次、gas/内存或 effect/emit/new-state 不符、checked 时间溢出、缺一个当前 payer、每 payer 资金上界恰足与少1。超出任一边界均拒绝整体准入/改变；不执行前两项后忽略第三项，不从整个世界扫描后截断为成功。

按 [Runtime 完整工程预算](../../world-runtime/runtime/governed-extension-lifecycle.design.md#extension-budget)再核：单 scope 的1/2个 pending successor，一次改变的4/5个 accepted 未执行 intents，单原生控制/完成事务逻辑读项与写项各128/129，request 16KiB及+1，完整引用/proof payload 与新增 journal/receipt 各128KiB及+1。嵌套来源、索引/范围证明、attempt、hold和负面证据全计入；在 decode/分配前限制集合，不能先读完整世界再取前128项。当前 economy request 的 `material_stacks_by_ledger()` 是全账本来源，目标必须改用被许可且完整的 scoped read-set，实际完整 CBOR 在上限内才可 dispatch，不以“两厂”推断现有读取已经有界。输出容量同时计原余额、其他待完成工作与本次批数：两批至少6铁锭/2炉渣，一批只占3/1，容量少1则不开始新sink。

把“已获权且可能已收费、尚未生产 sink 的 planner attempt 及其 hold”计入同一每厂1、全 profile 至多2的未闭合占用。并发四份 accepted intent 时，尚未获执行权的排队记录不占资源/槽位；不能先让四次 planner 各锁一份 closure 再仅数两个已生成 job。失败或未知 attempt 在原合法终结前保留占用，恢复不换键开第二份；明确终结只释放其真实未消费 hold。

再让已保护资金与普通转移、原维护、新模块调用及其他生产同时竞争。合法提交不能花掉旧 job 的闭合份额；上界不足须在首个不可逆 sink 前拒绝。当前计量代码在缺有效 artifact owner 时跳过费用，这一分支不是本验收允许的免费来源；正式目标必须提供明确可核的 payer/资金事实。重复失败、恢复和预算耗尽均不得把已花部分补回。

费用拒绝按边界分别核：前置 current guard 已拒绝而未 dispatch module 时为零调用、零模块费；已在当前授权下进入合法调用，随后计划/验证拒绝时，已实际发生且获同意的原计量至多一次，不能附赠退款或产生新生产效果。未知结果保留原 attempt 查询，不重新调用以获得另一份结果。逐次保存实际费用事件及目的账，不能把“业务拒绝”一概当零费；这些是目标完整消费判据，不声明固定基线已具备所有保护边界。

<a id="gel-m07"></a>
## 8. GEL-M07：原子竞争、重复与旧 replay 恢复

**输入与消费者。** 固定原 intent、payload digest、G0/G1 和整个 F1 读写集合；在实际权威事务、持久化发布与恢复入口做故障/并发试验。源码中存在一个 receipt struct 不构成已支持此边界。

| 故障/竞争 | 独立期望 |
| --- | --- |
| 同一 F1 的两个 successor、旧 F1 新请求与 cutover、旧 revision 更改 | 至多一份 CAS 成功；失败者不获部分 grant/hold/receipt。旧请求不偷换成 G1；F2 的原合法范围不被抹掉 |
| 非重叠 F2 已有旧 revision 工作跨越 F1 替代 | 独立控制组：t19 F1/J1已闭合；t20先按G0/F2合法start K2，ready_at=21并保留旧grant revision stamp，再提交F1 cutover；t21完成K2。共享G0记录升版而F2 selector/binding/实际权利未变时，原K2以原投入和原费用完成一次，immutable stamp字节不改，不挂G1、不重排程；真正撤掉F2当前scope/资产/工业权限或到其期限则另行拒绝。此控制组独立取账，不加进正常三job总账 |
| 准备后、提交前失败；分别在新 grant、旧 scope、预算/job delta、journal/root durable 发布边界注入失败 | 外部只能读原完整状态或新完整状态；不能一边新 active、一边旧仍可执行，也不能只改 grant 不改其预算/闭包事实 |
| 客户端超时、响应丢失、同键同内容重发、同键改内容 | 先按原 intent 查询 durable 结果；同内容只返回同一已决结果，改内容冲突。不得新建 root、重复收费/生产/完成，或把未知直接当失败发第二份请求 |
| 在 cutover 前/后恢复旧 snapshot+journal；并行恢复节点追上新提交 | replay 只应用已确认历史 delta，不重跑 planner/validator、重计量或重发外部效果。最终读取新相同 authoritative 状态；旧 snapshot 不能恢复 F1 的旧 grant |
| 旧 pending job 缺可信 extension binding、旧序列化缺新字段 | 保留原 job/sink/责任供读与恢复核对；不得根据相同 recipe/module 名自动挂 G1、当普通 legacy 新执行或删除。后续新效果必须 current resolver 可证明完整来源，否则阻塞 |
| validator attempt 已落地、decision 未确认；或 decision/完成已落地而响应丢失 | 未知 attempt 保留并 fail closed，不重跑 module；已确认 decision/完成只复用其原结果，不再扣费或产出。任何有权修复仍受原剩余窗口/次数，不扩大预算 |

同一 payload 重试还要检查处分的绝对 deadline、累计调用、已消耗费用与 obligation identity 均不重置。已提交的历史 completion 可在 replay 中复现；它之后的新效果重新检查当前权利，不能用当前撤销倒删历史，也不能用历史完成延长当前资格。

<a id="gel-m08"></a>
## 9. GEL-M08：读面、失败解释与证据层

在同一 read anchor 从实际 Viewer/API/Agent 读取正常例每个切点及所有拒绝。至少核原 grant/发行依据、明确 scope/有效区间、待决 successor、技术发布与当前执行资格、受限/退出中的原工作、剩余 deadline/预算、`exit_blocked` 原因、已完成历史资产和可执行下一步。未公开细节保持原可见性；不能靠复制私有正文扩大权限。

对照普通操作者、原发行者、有权审阅者及无权读者：各自能看到的事实一致且遵守原权限。pending replacement 不能显示新能力已可用，exit pending/blocked 不能显示安全完成，静态例或组件 fixture 不能显示真实 signed/consumer/跨节点通过；有历史完成也不能把失效 scope 显示仍可执行。

结束报告逐项列输入/预期/实际、准确 source/tree/环境、调用与付款原件、权威 receipt、恢复结果、required/full 覆盖和剩余 blocked。目标消费者尚未实现的项目保留 `blocked/not_run`；本手册没有运行通过记录，也不以 docs 检查、同内容 hash 或临时 fixture 代替正式证据。

<a id="gel-pf-scope"></a>
## 10. 普通 product / factory 与一般 recipe 的组合验收

本节消费 [GAME-018 普通类型政策](../../game/gameplay/gameplay-industrial-creation-and-cross-region-market-contract.prd.md#ordinary-product-factory-lifecycle-policy)与[共同执行合同](../../world-runtime/runtime/governed-extension-lifecycle.design.md#extension-ordinary-authority)，直接验证原 GR-002/003/010 及 AC-GAME-018-01/02/06。GEL-PF01～06 是场景方法，不是新产品要求或另一套 Profile；§§1～9 的 exact recipe 原参数和方法不变。这里的一般 recipe 包括只使用已存在产品/工厂的候选，原六态/六类工作、替代/转换和付费许可分别沿原专业合同合取。

本次源审读固定 main 4f6f20ece923c5705e26ef879c2b83e0e637b3f9；以下全部是 S0 源核对、设计和独立静态预期，**没有业务执行通过记录**。L1 fixture 可验证某一 guard/算术，但不能提供正式工业准入、真实治理签名、PPE 发布或实际存储的证据。实际实施须记 source commit/tested tree、世界/branch/authority cut、工件完整 manifest/hash/version、Profile/MaterialProfile/spec、原四阶段证据与授权、许可、全部 payer/receiver 和初始真实资源来源。缺 formal producer/final consumer 时记 blocked/not_run 与具体缺项，不写虚构 CLI、hash、签名或 receipt。

| 独立来源 | 本方法可用的源事实 | 正式候选仍须提供 |
| --- | --- | --- |
| [M4 ABI](../../../crates/oasis7_wasm_abi/src/economy.rs)与[Profile reader](../../../crates/oasis7_runtime/src/world/resources.rs) | Recipe/Product/Factory 正向类型；产品维护/可交易/stage 和工厂 tier/slots/tags 等字段 | 原不可变内容/绑定与当前权限，不能从 upsert 辅助函数取得外部发布权 |
| [smelter 源](../../../crates/oasis7_builtin_wasm_modules/m4_factory_smelter_mk1/src/lib.rs)及[工厂模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_factory_module_template.rs) | 12 structural_frame、4 heat_coil、6 refractory_brick；最低可用电力6；计划工期1 | 新候选若用新 factory ID，实际源码、build proof、受审 spec/Profile 与工件必须匹配；内置模板本身不证明新增扩展已发布 |
| [iron 产品源](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs)及[产品模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs) | iron_ingot 的 positive amount、stack_limit=500、可交易和 quality 输出 | 实际 exact validator/输入来源/用途/当前 grant；沿源派生新 ID 必须重新真实发布 |
| [建设准入](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_factory.rs)、[建设 reducer](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs) | actual construction-power profile 与 site/owner/投入；维护、建成与回收有不同原身份 | 完整当前 extension gate、建设退出与原物理权，不能把最低电力6当建设电费 |
| [产品恢复](../../../crates/oasis7_runtime/src/world/economy_product_validation.rs)、[due 与解析](../../../crates/oasis7_runtime/src/world/economy.rs) | 原 attempt/decision/delivery 的真实路径，with/no-modules、名称 resolver 和普通完成均存在 | 精确行为来源、全入口最终 gate、完整调用/费用与原义务索引 |

test_tier_required 对同一实现候选逐一做正常、拒绝、同 tick 顺序、原始/自动入口、重复与事务故障；test_tier_full 再核真实 G1 保存/重启/多节点、最大合法工作图、依赖组合和三面读者。保存每阶段当前 read-set 与索引 cut、实际调用/计量、原 work/node/source-lot、前后账、receipt/generation/journal；独立按原 quantity 与 effect role 对账，不能只看 active 标签或最终 root 发生变化。业务拒绝若发生在合法实际计算之后，只应拒绝非法业务，已获准真实计算仍须单次计费。

<a id="gel-pf-publication"></a>
### 10.1 GEL-PF01：三类型发布、当前授权与一般 recipe

**输入。** 固定同世界三份真实候选：一个普通产品定义/validator，一个工厂建设行为/准确计划对象，一个使用已有产品和已有工厂的一般 recipe。各自原四阶段、PPE owner/required roles/payload、技术 activation、许可、finite grant 与退出方案齐全；一般 recipe 的 RecipeVersion、六态与完整工作图另取原 authority，不用铁锭 v1 的两厂/四 tick 规则替代。

1. 先逐类型完成当前合法发布与一次允许的使用，记录不可变内容、唯一 scope selector 和三阶段结果。再分别在批准后、Reserve 后、Evaluate 后撤销/到期/收缩/替代授权或改变精确行为来源。最终 publication 或 CommitEffect 必须按当下条件拒绝；旧批准、技术构建和 Evaluate 成功都不能产生剩余非法效果。比较正式 Submit/Apply、direct install/upgrade、ModuleChangeSet、activate、rollback TO 与恢复后 pending 的所有实际提交点。
2. 同一实际 slot 仅换 grant ID、module 名/版本、Profile alias、caller、authority domain 或 transport key，不能得到第二个执行 owner。故意漏一项身份/处分/receiver、伪装 builtin、用未知类型/effect、错 world/branch 或 stale schema，整项拒绝；不静默裁小 scope 后部分发布。独立真实 builtin 作为控制组，其原合法执行不因本扩展撤销被停掉。

   独立非包含反例：原已审核候选请求 F1+F2、建设与持续经营两种 effect、[10,20)；当前证据仅允许 F1、建设和 [10,12)。即使三者存在非空交集，原请求必须整项拒绝，零部分授权/发布/资源预留，原 payload/digest 不变。分别只缺 F2、只缺经营 effect、只短期限，结果相同。系统可以展示较窄建议；只有请求方另建 exact 请求并完成必要审核后才能接受，旧 key 不能被静默改写、旧批准不能复用为新同意。
3. 逐项改变原 Profile 的已提交内容、新工件因果行为、PPE owner/审批 role 或 exact freeze。历史定义/receipt 仍可读，旧批准无效；新增定义走新 ID/证据，直接 upsert/裸事件不能成为对外发布捷径。只 stop-new-admission 的决定不删除已建物/原产品，也不自动宣告现存 scope 安全停止。
4. 对一般 recipe 单独测试：保持 recipe 为 active、产物与工厂均为既有合法定义，只令扩展 grant 过期。其新排程/首次 sink、后继当前效果必须相应拒绝或仅执行预授权的原有限处置；不能因没有新 Product/Factory 绕过。反向撤掉 recipe 六态准入而保持 grant 有效也拒绝。对已有专业 adapter 的替代/转换/物流/付费路线列全原图后正常采用，不以本手册旧铁锭 v1 不支持它们为由删除依赖。
5. 对 [begin,end) 的 begin−1、begin、end−1、end 以及同 tick canonical 先后分别核准入；对已提交历史则仅重读原结果。相邻非重叠 target 同时合法运行，F1 的受限决定不改变 F2 原来源和已开始 work 的事实。

**独立证据。** 每类型至少一份正常完整链、每实际入口的当前拒绝差值、内容/授权/资产三种身份和精确旧新范围。前置拒绝无 dispatch 时零调用费；已合法 Evaluate 后拒绝业务时保留其原实际费用，零非法产物/设施/新 grant。只能通过某个外层 API 的结果不能代签全类型闭合。

<a id="gel-pf-factory"></a>
### 10.2 GEL-PF02：建厂、持续能力、维护与其他制造

**可复算正常例。** 源行为采用上表 smelter 材料与工期。L1 独立 fixture 明确冻结一个合法 construction-power profile 的实际电力消耗 C=10，并明确 builder B 与模块/controller payer 分开；C=10 是此 fixture 的已给前提，源中的 min_power=6 不证明它。正式运行改取真实当前 power profile 的 C 并独立重算，不伪称世界默认值。B 的原材料为24/8/12，Electricity为30；其余必要当前权、容量、独立执行费/closure holds 均足额。

取 construction grant [10,12)，在 t11 开始原工作，ready_at=12；原接受时明确允许仅完成这次建设的 L=4，且没有更早原硬截止，故 T=12、D=16。L=4 只是此案例参数，不是普通类型默认政策。t11 的原 sink 后 B 材料应为12/4/6、电力20；模块费由其真实其他 payer 单独对账。t12 只有原有限 completion 可建成一次准确 factory；新建设不允许。建立原 factory ID、spec、site/owner、建设来源和 built receipt，不能再次扣建设料/电、复制设施或赠送新 tier/slots。t13 若原授予只约束建设，已建资产的独立普通能力仍按其原权限可用。

| 变体/真实入口 | 独立预期 |
| --- | --- |
| t12 才开始；或 t11 Evaluate 后、t12 才 CommitEffect | 未获当下建设权则零建设 sink/job/factory；合法已发生模块调用只收原一次 actual，不从未来产品抵费 |
| with/no-modules due、直接 BuildFactory、raw FactoryBuildStarted/FactoryBuilt、内部完成/恢复 pending | 相同当前 binding/spec/原 work/处分 gate；仅 ready_at 到期或裸 payload 不足以建成 |
| 建设授权不变，site/tenure/builder/power 来源或材料 owner 漂移 | 原 current 合取拒绝下一不可逆效果；不能换地点、偷用他人电力/输入或把 module min_power 当 actual sink |
| 已 sink 后丧失必要物理权，或 t16 才尝试有限 built | 保留原消耗、在建物量/义务和 blocked；不全额退12/4/6或10E。只有原已接受且实际实现的建设拆解/保全 adapter 可按真实损耗、成本与 receiver 结算，未建成对象不能调用 FactoryRecycled 伪退出 |
| 另一组 grant 明确限制已接受的持续经营 facet，到 t12 终止 | 该 facet 下新 recipe/制造工作拒绝，原已接受工作只按有限处分；既有 factory 和普通维护/折旧继续原语义，不把建设-only 变成事后经营租约 |
| 普通维护/回收与扩展限制并发 | 维护扣真实 parts、只保全不扩产；回收须原 owner、准确 recovered/capacity、全工作/第三方闭包及 receipt，retired ID 不重用 |
| pending_recipe_jobs 为空，但原 Agent 制造订单处于 Producing | 沿真实 world_rules.agent_orders 与工业 blocker 枚举；仍占 workload/原义务，不得以“空 recipe 列表”直接 retire、删除订单或放出承诺给第二份 scope |
| 新 Profile 宣称更高 tier/slots，旧 factory 已建成 | 旧实体规格/身份不变；需要的新物理能力独立实际建设/转换，升级 receipt 不自动发新设备 |

**独立判据。** 对建设 start、built、维护、制造负载和真实 recycle 分别记物量、资源、owner/位置、工作和 receipt 差值；静态决策/临时选择不当作物理返回。最坏建设退出无法在首 sink 前给出真实有限路径的候选应在更早接受时拒绝，不留给后续手工修世界。

<a id="gel-pf-product"></a>
### 10.3 GEL-PF03：产品验证、正式产出与读面

**独立正常。** 对实际 pin 的 iron validator，500 应满足其源上界、501 不满足；这证明该源的判定边界，不证明任意产品都应500。以准确受权源片段的已有库存11、此次验证量3做 standalone validation，返回 ProductValidated 后物料仍为11、owner/位置不变，只有原实际计算费用和验证事实变化。另用明确只读候选输入验证时也不得生成实体库存或资产权。

对真实原生产 job 的产出量3，取一个独立 L1 已准入 ProductProfile fixture，其 maintenance_sink 明确为每单位2 hardware_part；原起始10 parts，应在 first sink 随原配方成本额外一次消费6，剩4。这个2不是当前 iron 默认值；正式候选使用其真实 Profile 数值独立相乘。验证成功只形成原判定，原 RecipeCompleted/源数量/ledger 与 terminal obligation 才证明真实产出，未到目的地不能显示已交付。原 stage 未满足时，即使 module 会接受物料也不允许非法 first sink。

1. 从 standalone WithModule→普通 ValidateProduct、module recipe、直接/原生/essential/内部 ScheduleRecipe、with/no-modules due、ProductValidated/RecipeCompleted 最终 reducer、恢复 pending 逐一运行。每个需要验证的输出精确固定 validator module/version/hash/manifest、activation、output ordinal 和数量；缺 validator 或名称后缀匹配到另一个 active 模块不能跳过或替换。
2. 固定一次旧 validator 对450的已确认 accepted 结果，新的不同版本只接受≤400。历史结果恢复仍是原450的事实，不调用新版本改判；若旧 work 尚未获得结果、仍依赖旧 validator，则未合法结清/迁移前不能全局移除其唯一依赖。新的450请求按新的确切当前规则拒绝，不借旧结果取得新货。
3. 先持久化 attempt、再令结果 unknown：换 module、换请求、重连或重启不能再 dispatch 求成功。已有 decision/已交付验证事件只恢复原结果；未投递部分按原 delivery identity 补齐一次，所有实际 subscriber 调用仍在原工作图/费用预算内，不能重复业务效果。
4. product/MaterialProfile ID、品质、数量、源 lot、单位、tradable/stage 或用途任一不匹配时明确拒绝；不能验证3再拿 receipt 接收30，或给已有旧货换新 product ID 生成第二份来源。验证/新产出 grant 到期不删除原11库存；其普通运输/交付/回收沿原当前资产规则，只有需要新受限效果时才核相应 facet。
5. 在相同 read anchor 从真实 Viewer/API/Agent 查看定义、技术 activation、当前 grant/许可、已验证、已生产、在途/终端待结和独立库存。读面须区分这些结果；quote/bootstrap fallback 无 grant时不能显示已获执行权。owner、有权审阅者、无权主体分别核公开字段/私有原件权限，返回前权限漂移不泄露旧缓存。

**独立判据。** 按物料/原 lot 和工作 effect role 对账：validation 既不增加也不减少物量；生产额外维护只扣一次；已合法产出的资产不被 creator 退出删除；current 拒绝不伪造已到货或已安全关闭。three-surface parity 与玩家理解在 full 层，源审读不替代运行。

<a id="gel-pf-license"></a>
### 10.4 GEL-PF04：许可消费、开放与成本并集

普通 product/factory 正常候选必须有原 authority 明确的零版税、开放或有据不适用结论，并有当前范围/主体/用途/期限。分别删除结论、使许可源 unavailable、使 scope/维护/原接受失效；unknown 不视为免费。令单次 term 到期但 lineage 未 hard-open，新使用拒绝；达到原 hard-open 的确切 A 后只开放原许可权，原入场、scope、安全、PPE freeze 和别人资产/私有源仍不开放。换 ID、维护版或受益人不重置最早 A。

依赖一个原已正式采用的付费 recipe 时，选原许可支持的完整 root 单次完成 adapter，并固定其原 batch 与全部资金/程序槽。例取4个完整原 batch、已接受单价 p；新增净版税应为4p，若原真实 p=1E 则为4E。同一 root 多个产品 validator、副产物、建厂、运输和回读不产生第二 fee unit 或额外版税；不以分批/N不同的未支持路径复用一案预算。所有 source lots 和 complete-batch 归因取原 runtime，不由 caller 报数量。

1. 比较原许可完整操作/案件/closure与本生命周期的 call set，用原 root/node/phase/instance/call-role 唯一键求并集。原许可一次迁移可能需要两次制度 invoke，加 lifecycle 一次时该阶段真实上界为3，不能按一个许可调用报价。重复覆盖同一 call 只保护一次，但其他 case/pool/notice/收益付款调用及原槽位全部保留，不能用普通64节点删掉原许可更大的程序。
2. 同 payer 合并所有资源和用途再验余额，不同 artifact owner/payer 独立足额；先读 fee preflight 对该准确 hold 可消费额度。缺 owner 的现有免费分支、持有未实现的费用字段、未来收益/回收和他人未签余额都不能补资金。普通转移、维护、新调用并发时不能消费原受保护 closure。
3. 在正式确认、first sink、实际完成、许可续期/开放的原绑定切换前后各检查一次。原 root 换 term 只处理尚未完成/计费范围，旧未赚取占用退出与新必要占用同根交接；不得双重 hold、先放款给无关消费或重新计费已完成批次。新许可不能重开已限制/终结的扩展工作。
4. 旧收益应计/程序/个人应付尚未结清而扩展已停新：继续原许可闭合和原资金身份，不追回已付、不删除原应付，也不从待付权推出新的生产权。相关 IL 原方法必须实际证明，单独 product validation 通过不足以证明收入已赚取/到账。

**独立判据。** 除物料/生产成本，逐 payer 核原净版税、技术费、held、已耗、应付/已付与单次释放；调用0次不收调用费，合法实际调用后业务拒绝不退款。版税与模块费用的单位/目的账分开，不能相加成一个不明“成本”后声称守恒。

<a id="gel-pf-exit-upgrade"></a>
### 10.5 GEL-PF05：全量处分、有限时界与原子升级

选同一 current cut 的完整 scope/work/dependency 索引：含仅排队、Reserve无 sink、建设 WIP、原 recipe/制造 WIP、产品 attempt/待接收、在途、buffer、terminal、已完成独立资产、未付真实费用。每类至少一个可达源前态；无法通过真实 producer 取得者报告 blocked，不手改状态制造业务证据。清单之外另放一个非重叠合法 scope 作为控制组。

对每一变更分别执行 stop-new-admission、限制后续效果、受控退出及合法组合。删除任一必要处分/真实 receiver/资金/权限、隐藏一项未完制造或 subscriber 责任、索引缺页/不同时点、准备后插入新 work，整项变更拒绝；原期限本身仍按时间生效，错误不能续期。

| 存量与动作 | 独立结果 |
| --- | --- |
| 未确认请求/无 sink hold | 失效原请求不自动挂新 grant；只释放确实未用且无原责任的占用，unknown费用不清零 |
| 原已 sink 建设/制造/recipe | 只用原有限图的有权 finish/保全/真实移交；原消耗、损失、builder/source/quantity和第三方责任连续 |
| 产品 attempt/decision/输出 | unknown不重试；decision不重判；原接收/终端效果只发生一次，不凭验证 mint |
| transit/buffer/terminal | 原位置/数量/损耗/receiver/容量与费用继续；有实际 return/reroute/handoff receipt才结清，不因选择退出瞬移 |
| 已结资产/合法历史 | 原身份/存量/receipt保留，scope closed不要求销毁；后续独立操作仍核原权利 |
| 到 D、源失效或预算耗尽 | 停下一非法效果，明确blocked及残余责任；不把 unsupported、错误返回或管理标记当完成 |

独立时间例固定 T=20、L=5、另一原硬截止23，因此 D=min(25,23)=23；20与22可执行原当前合法最小动作，23不可。重复撤销、改 actor/ID、重启、等新grant都不能把T改为新时点、释放已花费用或将D延至25。后来的独立合法处置关联原责任且给自己的有限范围/资金，不回填旧窗口、续开旧普通权或免费保管。

授权替代先关闭旧重叠新工作，结清原执行依赖；candidate pending 无执行权。两份 successor、旧 completion、新建设/产品接收与 cutover 按 canonical/CAS 排序：只能一个完整结果，receipt共同收缩旧 selector、确认新 selector、明确旧 pending请求与义务，失败不留半份新 active；非重叠原work保留旧 stamp、按其实际当前权继续。

行为升级另核 artifact→binding→scope→work/instance 全反向索引。共用 validator 有两个消费者时，一个消费者同意不能完成全局 activate；另一旧 unknown attempt/依赖不得漏掉。没有原正式 migration adapter 时先结清；有时按原 exact from/to/schema、当前 owner/payer/费用和全部对象独立证明迁移，不把本方法当任意 state transform。Profile 新内容保持 append-only，既有工厂不获免费扩容、旧产品不获新 identity。旧工件退休须保留原执行与历史可恢复依赖，不能先删 bytes 再宣布安全结束。

<a id="gel-pf-recovery-budget"></a>
### 10.6 GEL-PF06：完整预算、故障与原结果恢复

独立读取[原费用公式](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)，再用真实候选 executor 的完整 I/O 与计量复算；不用 factory/product 模板的 output_bytes=512。制度上界input/output各256KiB、effect=1/emit=1，Pure上界64/16KiB、effect=0/emit≤1/no-state；更严格 manifest 继续生效。

| 独立复算 | 期望 |
| --- | --- |
| 制度一次 preflight | 256+256+2+1=515 Data；1+1+1+1=4E |
| Pure 一次 preflight | 64+16+1=81 Data；1+0+1+1=3E；真实正常无state单emit通常2E，须按 actual |
| 最宽普通节点 | Reserve与CommitEffect各3次制度、Evaluate一次Pure，共3171 Data/27E |
| 64节点原root | 202944 Data/1728E；包含32 normal与32原closure的最坏图，不是只给成功分支 |
| 控制总保留 | (8普通+8独立关闭)×8次制度=65920 Data/512E，不挪作新经营 |
| 4根与控制 | 4×(202944,1728)+(65920,512)=(877696 Data,7424E)，原实物/净版税及其他原程序按准确调用并集合并 |
| 零费许可无实际写调用的节点 | 两次lifecycle加一次Pure=1111 Data/11E；实际省下的调用不收费，不能省真正必须的许可消费者 |
| state容量 | 32×256 +50×256 +4×64×256 +4×1024 +16×128 +16KiB=106.5KiB；实际完整编码仍≤128KiB，逐条及总开销均核 |
| 并发准备内存 | 4×32MiB+96MiB=224MiB来源buffer；5×16MiB sandbox=80MiB；本路径304MiB，其他常驻内存另记 |

逐项做上限及+1：32 scope、4未闭合roots、每root32 normal/32 closure、8资源/材料种类与8精确依赖、PPE50 Profiles、128KiB state、256/64KiB完整input、256/16KiB完整output、effects/emits、普通/控制32/96MiB真实来源、非code4MiB、512源对象、2048访问、256写项、64KiB请求、512KiB新增receipt/journal、最多8 artifacts且各受PPE10MB上限、2/5s准备预算、controller/Pure gas2,000,000/1,000,000、每实例每tick16调用及真实artifact rate。源的原始/解码/code/schema/证明与负面索引证据全计，不因cache命中或先全扫后截断而省略。失败完整拒绝当前新承诺，不让先到的几项偷跑。

普通阶段第三次许可相关调用、必要 subscriber/delivery、on-register/migration hook、真实组件访问费如存在，都要出现在接受前的 exact call graph；无法纳入相应阶段/后继/控制上界则拒绝候选。普通控制8槽用尽、独立关闭中的普通关闭7槽也已用时，最后保留的一次全实例停新 fence仍能覆盖所有已存在atoms；不能通过换instance/alias恢复同slot旧权或向关闭再借普通额度。该fence不假装完成原work，其已接受closure预算仍保留给真实处置。

分别令每个真实payer余额恰足与少1，制造普通转移/维护/新工作竞争；少1在相关新承诺前拒绝，原资金不被其他扣款消费。若两个payer恰同一账户必须合并，而不是重复使用余额。核 module fee目的账、生产sink、实际收益与未花hold各自去向，剩余只按原purpose释放。最大合法图含付费许可原程序时另核其完整独立预算/槽位；上述877696/7424不能替代它。

| 持久化/并发刺激 | 独立期望 |
| --- | --- |
| Reserve后失败，Evaluate拒绝/trap或结果未决，CommitEffect前authority变化 | 各阶段原事实保持；只有真实已授权调用计费，未生产就无物料/工厂；unknown不重复dispatch、不猜零费或释放其原责任 |
| 同key同payload、同key改内容、跨入口/别名重试 | 原结果或明确conflict；不复制call/root/hold/产出，不重置T/D/累计量 |
| G1明确NotCommitted、已Committed但响应/维护失败、StatusUnknown | 未提交才弃候选；已提交安装/恢复原结果；unknown对整个旧World禁止新发布，其他scope的旧writer也不能继续，核原operation/generation后解fence |
| 准备与publication各写点失败；撤销/升级与实际完成竞争 | 只读到原完整状态或新完整状态，不得新code配旧grant、半收款或双scope；按canonical顺序重验 |
| snapshot+journal、重启与节点追赶；混代/缺source/未知schema/索引不完整 | 完整同代恢复，已提交只应用原delta/费用；缺项拒绝安装，不拿新cache/Profile/activation修旧证明 |
| 旧attempt晚到结果；已decision但delivery未完 | 保存精确原事实并按原key恢复；后续效果仍核当前权限/截止；delivery消费者不产生重复业务，late结果不回填过期完成 |
| 已闭root腾槽、之后又重发旧source/effect | 原消费tombstone和结果ref仍可exact查询，不能因当前state腾槽重复用旧量或再收费 |

在4个普通准备槽和1个控制槽持续受干扰时，检查2个独立原结果读取槽及已接受closure的保留份额仍服务原责任；队列ACK不能伪造canonical入窗，超时不延长业务期限。full层须证明最大合法输入/调用/保存图在已接受边界内可处理，记录真实peak/fuel/延迟，不能将304MiB算式或2/5s目标写成实测成绩。

**本节结束报告。** 按PF01～06列正常、独立负例、真实入口、前后账和明确证据层；分别列设计已有、代码仍缺、真实producer/部署未激活、业务/持久化/玩家验证未运行。不以现有函数名、fixture签名、静态trace通过或一个成功类型代签其余 Recipe/Product/Factory 及全部当前消费者。
