# 成熟世界区域供应用途验收手册

- 生命周期：`active`；Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`；Last reviewed：2026-11-05。
- 上游：[REQ-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)、[AC-WR-MW-001](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-001)；专业依据：[有限区域供应用途政策](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#regional-supply-use-policy-v1)与 AC-MRC-01～08。
- 技术依据：[区域贡献系统设计](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-need)。本手册拥有 `native_regional_supply_use_v1` 的独立结果账、来源反例、恢复和正式消费者验收方法，不重定原目标、运输、生产、资源或许可规则。

Trace refs: O7-REQ-bc53557b-b03d-465f-a11d-3b3a33ae29ce。审读基线为 `b65af49ca13f7b23f9b85a6209c21a01a4b0f61b`；执行时另记实际候选。下列静态算例和待执行方法不代表当前区域贡献能力、成熟世界路线或发行已通过。

## 1. 输入、证据层与直接来源

每次验证固定 source HEAD、tested tree/build/config、world/branch、canonical location、C0/C1 的原 commit/root/cursor/generation、当前读者与 A/B 的个人/Agent/控制来源、原 G/U revision/digest、实际工厂与 Site ledger、route、profile、预算版本和原 operation/job/receipt。原记录缺失或正式能力未实现记 `blocked`；已有能力本次未运行记 `not_run`；来源冲突或覆盖不足保留 `ambiguous/source_unavailable`。每个反例从独立前态或已注明的原提交继续，不清库存、改期限或补写完成事件帮助通过。

| 证据层 | 本层可证明的结果 | 不能替代的结果 |
| --- | --- | --- |
| 源码/合同和整数复算 | 原规则、实际接缝与算例一致，方法具有独立预期 | fixture 已运行、目标 producer 已实现或正式接线成立 |
| 受控原生 fixture | 标明直接安装的前态经过原 Transfer、Schedule、reducer 后的实际结果 | 正式鉴权、合法成熟历史、原 owner/goal/位置来源；不得将 setter 或注入 grant 作为这些来源 |
| 原 authority 与完整来源 | G/U、合法库存、当前权利、原交付/消费及完整范围可以核对 | 最终余额、截图、几个事件 ID 或测试自己生成的摘要 |
| 同候选三入口与真人理解 | Pure API、Agent Client、Viewer 各自消费同一事实；真人能解释选择和结果 | 共享 DTO、模型自述、一个入口成功或文档存在 |
| 完整成熟世界/发行 | 原 MW-1～7 等适用组合与阶段 authority 实际采用同候选证据 | 本地单条供应链、静态手册或普通 CI |

| 直接来源 | 本手册复用的真实接缝与限制 |
| --- | --- |
| [原 goal](../../../crates/oasis7_agent_api/src/game/goal.rs)、[SC-31 结算分层](../../product/world-rules-core-gameplay/industrial-demand-goals-and-settlement.prd.md#req-sc31-003) | 当前文本 goal 不提供本例有限输入目标的完整 authority。G 的结构化接受、版本/到期与 matching adapter 是目标接线；不可用自然语言或本手册的变量补签 |
| [位置/Site 状态](../../../crates/oasis7_world_state/src/state/industry_state.rs)、[建厂 reducer](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs) | 读取真实 `LocationAnchorV1`、`FactorySiteAuthorityV1` 和工厂 input/output 引用。canonical FactoryBuilt 的 input/output 指向同一 Site，不拆成两份余额 |
| [当前主体和来源检查](../../../crates/oasis7_runtime/src/world/agent_delegation.rs)、[owner 委托入口](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs) | 当前 owner 的合法 Site-source 路径与委托执行分开核对；现有 delegation 对 Transfer 的 source 限于该 Agent ledger，不能以推荐或通用 grant 代签 Site-source 写权 |
| [运输 evaluator](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs)、[路径/报价](../../../crates/oasis7_runtime/src/world/logistics.rs)、[transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) | 原 start 扣源材料/运费并占容量；原 complete 核对 pending 身份、入目的账、付 route owner 并释放容量。报价不预留，空 route 的 direct 路径不算使用 R |
| [时间/损耗](../../../crates/oasis7_world_state/src/world/pure_helpers.rs)、[原 gear 配方](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[排产](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs)、[scarcity](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy_support.rs) | 损耗由实际 material profile 调整，原配方 iron2→gear1/E4；生产输入在实际 Site，相关 scarcity delay 为0。实际 ready_at 和有效维护输入仍须读取 |
| [原生产 reducer](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[稳定线 helper](../../../crates/oasis7_world_state/src/state/industry_transition/mod.rs) | start 的真实 sink 与 completion 分开；稳定线要求至少3次相同 canonical snapshot，个人归属由来源链证明，不由世界总体 stage 推断 |
| [原 step 次序](../../../crates/oasis7_runtime/src/world/step.rs)、[产品验证/折旧](../../../crates/oasis7_runtime/src/world/economy.rs)、[其他周期](../../../crates/oasis7_runtime/src/world/gameplay_loop.rs) | 先折旧和 action，再 due economy、due transit；不能在到货前排产。产品模块还查实际 `.product.*.<kind>` fallback；折旧、其他模块与生命周期 writer 不能漏算 |
| [factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)、[原 bootstrap manifest/profile](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)、[计量公式](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs) | 参数、当前安装内容和真实执行分开。bootstrap 不覆盖既有 profile；限额不是实际费用，原未来调用的 payer/资金也不是免费默认 |
| [原请求恢复](../../../testing-manual.md#request-recovery-regression)、[存储恢复测试入口](../../../crates/oasis7_runtime/src/tests/persistence_recovery_tests.rs) | 可扩展原请求、snapshot/journal 和不中断对照；现有测试定义不说明新 goal/归因/首次性索引已接入 |

## 2. 固定正常前态与独立结果账

### 2.1 原成熟历史、G 与 U

选择已有组织、治理和历史的隔离成熟世界。A 是已完成首次持续能力的原个人：保存本人合法 FactoryBuilt、当时控制来源、三个相同 canonical snapshot 的实际完成 job 及完整窗口，按原 helper 核查。三批历史产物为 iron9/slag3，历史投入、验证和费用均保留；这只是原前史，不能因本次再次读取而新增成长。B 是不同的经鉴权个人，控制自己的 assembler；不同 Agent 名字或请求 ID 不证明个人不同。

C0 是 t9 完整处理后的切点。G/U 在 C0 前已有真实接受/预声明：B 的有限输入目标 G 接受合法已有库存 iron2，窗口为 `[t10,t18)`；U 是 B 自己接受的独立使用意图，绑定当前目标 revision/digest、原生 gear 一批、工厂、input/output Site 与用途。G 的作用是匹配输入交付，U 的作用是生产使用；G matched 后不自动启动 U，也不创造下游 terminal/reward。A 在承诺前看到 B 授权披露的用途、数量、期限、运输成本和不可逆交付/消费边界，显式选择这一份无价款、无奖励承诺的供应。

| C0 事实 | 正常输入及来源要求 |
| --- | --- |
| 同一有限地点 | A 输出 Site 与 B 输入 Site 的当前有效 authority 指向同一个 `LocationAnchorV1.location_id`，scope为 world/branch/该canonical ID；不从坐标、邻近文字或 location ID 猜区域/charter |
| A 的真实账 | 读取工厂实际 input/output ledger并去重：旧 iron9、slag3；另有 ore4/fuel1，Electricity20。保留每种材料合法来源，不把新供应伪称新生产批次；同一 Site 的余额只计一次 |
| B 的真实账与能力 | 原合法 assembler 的 input iron0、output gear0，Electricity10；正常 canonical input/output 是同一 Site。当前 builder、授权、tag、slot、output room、stage 和原 profile 均满足；B 在 C0 已可承担生产4E，不依赖未来运费收入 |
| 原 R 与占用 | 一条实际可用的有向 route：A输出Site→B输入Site，1km、capacity至少2、tariff=1E/单位、原owner为B；源/目的及实际几何独立核对，同LocationAnchor不表示零距离。原共享 allocator无冲突，global inflight也可接受；参数使用显式route绑定，auto_reroute=false |
| 材料、gear 与背景规则 | iron 实际 Medium loss10bps；正常实际材料 decay profile 为0，gear 的 maintenance sink为空，且适用规则正面允许 validation `not_required`、registry无适用validator/fallback。不靠缺owner、缺模块或删profile取得免费路径；额外合法writer及费用按下文W记录 |
| 两厂耐久与历史 | 两厂早于C0建成，保留原maintenance_per_tick=1、slots2及durability D_A0/D_B0；正常值足以容纳本窗折旧。A无新job；B除U外无待执行工作，旧pending与处置来源完整；不能只检查当前队列显示为空 |
| 首次性和继续来源 | 完整canonical beneficiary+world/branch/location+固定 `native_gear_input_supply` 历史投影，含首次缺席证明和原重复分类。若展示A未来独立iron报价，另须当前有效验证器/实际payer V、资金、容量和权限来源；V初始Data20/E10只用于报价可行性，未执行、未保护、未扣费 |

不能仅用 `set_*`、预装grant或测试写event宣称取得上述原来源。直接构造等价前态只验证组件；正式 G/U producer、原位置/建厂来源、完整 scoped reader 或首次性索引未接线时，该层阻塞。相邻 [CC-003 恢复项目](../../world-runtime/runtime/bounded-recovery-projects-and-contribution-settlement.design.md#recovery-contribution)有不同的 containment/资格/效果主责，不是本 profile 已实现的通用贡献服务；本例不建立新 lot/escrow 库存。

### 2.2 原生执行顺序

以下是假定各原步骤在指定 tick 已实际提交的序列，不是真实秒数或网络完成时限。按原 step 的 action→due economy→due transit 次序，到货 t11 后再显式提交的 gear action 在 t12 执行。实际入口延后时保留原窗口与真实 ready_at，不能移动 t18 截止来迁就样例。

| 观察点 | 独立预期，逐项回读原事实 |
| --- | --- |
| 只读 G/预览/继续候选 | 无材料、E/Data、容量或费用占用；没有新工作。传输ACK、排队或贡献被受理不等于原Transfer已开始 |
| t10 原 Transfer start | A Site iron9→7，A E20→18；原transit持有iron2，原运费escrow2E，R占用2；B input仍0、E仍10。原G的committed_unsettled0→2、remaining仍2，一次原start只记一份供应身份 |
| t11 原 arrival及matching | `floor(2×1×10/10000)=0`，B input iron0→2；R释放2，B作为原route owner收2E，E10→12，原escrow清零。合法G matching同根使delivery-settled0→2、remaining2→0、committed_unsettled2→0各一次；尚无gear或区域用途完成 |
| t12 B显式选择U并原Schedule | G已经匹配且停止追加供应。U按自身当前条件准入：B input iron2→0，builder B的E12→8，原job pending，gear仍0；start不等于U完成，也不等于下游terminal |
| t13 原 gear完成 | 以该job原ready_at确认output gear0→1、pending结束，供应材料实际用途才有资格核实；A仍iron7/slag3/ore4/fuel1，A E18，B E8。G原交付结果保留，gear归原B输出账，不转给A |
| 结果读取、重连与replay | 原材料、运费、生产消耗、gear和G满足量不再变化；同源多入口显示同一用途事实，不能产生第二个首次成长、机会、OC或治理权 |

运输与生产分账：A−2E/B+2E只是运费转移；B生产−4E才是本窗业务sink。两人E业务本体合计30→26。材料分别为A iron−2、B到货+2再生产−2、gear+1，不能将不同kind数量相加宣称物质等量。本窗口没有A的新iron job或iron-validator调用；历史验证不能作为当前调用计费，未来报价V的Data20/E10保持原值。

**完整账包含背景writer。** 上表为原业务分量；每个账本实际期末=`原期初+上述业务delta+W`，W逐条定位自动周期、模块费用、维护/衰减、其他合法动作及原资源source/sink。正常W=0须由当前合法配置和完整范围证明；不能由“本测试只提交两条命令”推出。W不为0时展示真实变化并重新核预算；若它触及B iron来源/占用闭包，则本profile的唯一用途归因失效，不删除事件或停用真实writer制造通过。

原折旧另有非零结果：C0=t9至t13四次tick，A无active job，每tick1000ppm，共`D_A0−4000`；B在t10/11/12折旧时active0各1000，t13完成前active1/slots2为1500，共`D_B0−4500`。按原checked/clamped规则和真实初值核算；正常不触及clamp。它只改durability，不额外扣base_power_draw20对应的E或材料，也不抹去A的原稳定线历史。真实输入不同须按原规则重算。

### 2.3 只读源码与算术定位

以下命令只定位实际定义，不启动业务或构造运行结果。完整query、API、恢复和真人方法仍按后续八项执行。

```bash
rg -n 'factory_has_canonical_stable_line|same_recipe_repeat_count' crates/oasis7_world_state/src/state/industry_transition/mod.rs
rg -n 'TransferMaterial|MaterialTransitStarted|ready_at' crates/oasis7_runtime/src/world/event_processing/action_to_event_core.rs
rg -n 'process_factory_depreciation|process_due_economy_jobs|process_due_material_transits' crates/oasis7_runtime/src/world/step.rs
rg -n 'resolve_product_module_for_stack|builtin_product_module_for_kind|maintenance_rate' crates/oasis7_runtime/src/world/economy.rs
```

```python
from pathlib import Path
import re

src = Path("crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs").read_text()
gear = re.search(r'"recipe\.assembler\.gear"\s*=>\s*\((.*?)\n\s*\),', src, re.S).group(1)
assert 'MaterialStack::new("iron_ingot", 2)' in gear
assert 'MaterialStack::new("gear", 1)' in gear and re.search(r'\b4,\s*$', gear)
helpers = Path("crates/oasis7_world_state/src/world/pure_helpers.rs").read_text()
base = int(re.search(r'MATERIAL_TRANSFER_LOSS_PER_KM_BPS: i64 = (\d+)', helpers).group(1))
speed = int(re.search(r'MATERIAL_TRANSFER_SPEED_KM_PER_TICK: i64 = (\d+)', helpers).group(1))
assert 'MaterialTransportLossClass::Medium => 2_i64' in helpers
loss = 2 * 1 * (base * 2) // 10_000
duration = (1 + speed - 1) // speed
assert (loss, duration, 2 * 1, 9 - 2, 10 + 2 - 4) == (0, 1, 2, 7, 8)
assert 20 + 10 - (18 + 8) == 4
assert (4 * 1000, 3 * 1000 + 1000 * 15_000 // 10_000) == (4000, 4500)
print("static source/arithmetic only: matched")
```

## 3. 八组独立验收方法

<a id="mrc-m01"></a>
### MRC-M01：成熟阶段、真实需要与范围身份

承接 [AC-MRC-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-01)；依据 [contribution-need](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-need)。

从正常C0核本人建厂/控制/三批原snapshot、G/U接受顺序及当前目标revision/digest、location与两Site authority。分别将世界stage、meta points、直接设置repeat3、其他人的稳定线、后来的owner、邻近坐标或模板“区域有需求”替换一个来源，预期不取得成熟/需求/同scope结论。G要求新产物的变体不支持本既有库存profile；不能重新包装A旧iron使之合格。

G是原SC-31目标authority的有限适配，U是B已有的独立使用意图。缺publisher当前权利、结构化接受、U的实际gear条件、窗口或版本原件时，不能产生有效供应预览；已有普通行动及资产保留。记录每个真实来源和缺项，不能用G的文字或quote的成功字段作为自己的预期值。

<a id="mrc-m02"></a>
### MRC-M02：只读预览、有限承诺与完整成本

承接 [AC-MRC-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-02)；依据 [contribution-preview](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-preview)。

A比较这一份iron2供应和保留库存/退出：应读到B授权披露的用途、既有库存可用性、原路线/窗口、损耗、2E运费、不可逆边界、没有价款或奖励保证及后续复查条件。反复读取不锁材料、容量或资金；明确选择后仍须原Transfer在当前来源与预算下成功，不以推荐、accepted intent或队列ACK扣料。

在供应接受与原Transfer start处分别重试、断连及并发：同一已鉴权A、原intent和相同payload只回读原结果，同键不同payload拒绝；换request或两份并发请求不能为G同一剩余2单位再承诺两份。以原目标commitment、transfer/job及账本原记录为oracle，正常仍只有一次iron−2、一次2E escrow及一份在途2，不能仅等到matching时才去重。任一当前准入失败应整批无新承诺、扣料、费用或占用；不得撤销另一已合法提交的结果。

分别改变源owner/Site revision、当前权限、容量、铁损耗profile、A E不足2、B C0 E不足4、已有资源义务或U条件，预期重报价/不可用且不替玩家选择。B不得以尚未收到的tariff证明初始可行。原生source为Site时，正式当前owner路径与委托路径分别验证；现有delegated Transfer返回source unsupported时保持该能力边界，不私自改用Agent ledger、安装grant或扩大权限。

未来A独立iron报价只在全部当前事实成立时显示：除ore4/fuel1/生产8E，还核原validator、实际V/payer、manifest、实际维护和可用预算。用原上界公式复算I≤4KiB、output≤4096/effects0/emits2所得10D/4E，V20D/10E足额；这只是候选条件，查询不调用WASM、不保护或扣V资金。缺该来源时仍提供合法hold/exit，不把报价变成必需收费或新的承诺。

<a id="mrc-m03"></a>
### MRC-M03：原运输、matching交付与资源守恒

承接 [AC-MRC-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-03)；依据 [contribution-delivery](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-delivery)。

依§2逐tick核原Transfer的source−2、transit2、运费escrow2、route占用、arrival2与B收到2E。匹配当前G后只使其delivery-settled增加2/remaining归0；同一原receipt、换request或并发matching均至多一次。R owner与原路径身份不相符时，不能把旧tariff记给指定B。核所有相关资源/容量的原记录，不能只检查summary余额。

在到货前让G取消、到期、变更用途/revision或失去matching条件；普通运输若仍合法完成，材料/费用和原收据保留，G显示unmatched/原剩余量而不倒退原运输；原linked disposition解除对应未结供货承诺一次，不另造需求余额或退款。临界tick到货与expiry按原提交位置核真实胜者，不以本地时钟续期。重复交付、晚到及跨G别名不能减两次需求。G已matched即停止为G继续供应，surplus留在原账，不自动倾销、返还或启动U；保留实际下游terminal的独立状态。

<a id="mrc-m04"></a>
### MRC-M04：接收方实际使用与个人归因

承接 [AC-MRC-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-04)；依据 [contribution-use](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-use)。

先仅完成G matching，确认用途仍未完成；再由B显式提交当前U。原t11提前排产变体因action早于arrival不能消费尚未到货的材料；已拒绝结果保留，合法后继需另由B显式选择。正常t12 start核匹配的input sink2和B E−4，t13原completion核gear1，才形成可检验的区域用途。

从B实际input ledger的C0 iron0开始，读取完整writer窗口：恰一份上述credit2、唯一U job sink2及其completion。覆盖旧pending、所有转入/转出、其他recipe、维护、refund/compensation、治理及恢复delta和相关hold；同kind额外credit/debit、非匹配sink、不同ledger、缺页或未知writer均给`ambiguous/source_unavailable`。特别构造“先用A的料做另一job，再补等量铁给U”，即使期末余额和gear数量相同也不能把U归因给A。只证明合法供应被使用，不声称来自A的某一历史批次，无需新lot库存。

个人归属取原接受、source debit、原start/complete的已鉴权beneficiary/Agent/control generation。随后转控Agent、换显示名或以B当前owner读取，不得把旧A贡献重新归人；当前读权限撤销则隐藏相应内容，但不改原历史。合法业务与归因不足分开，`ambiguous`不撤回资产、改判业务失败或处罚玩家。

<a id="mrc-m05"></a>
### MRC-M05：一次用途、首次成长与anti-grind

承接 [AC-MRC-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-05)；依据 [contribution-checkpoint](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-checkpoint)。

对同world/branch的原Transfer settlement、B use job和canonical purpose反复读、并发读、换request、恢复或生成另一份报告，仍是同一用途事实。结果必须让A解释“我的库存供应使B的齿轮制造实际发生”；A保留iron7/slag3并不构成新的第二项成果，B的gear也不成为A资产。无新资源、分数、OC、认可、资格、容量优先权或治理权。

首次/重复分类另查完整的canonical beneficiary+world/branch/location+固定`native_gear_input_supply`历史投影及缺席证明。更换接收者、G/U ID、Agent/goal别名、policy revision或自称新类别不能重置首次性；任意新分类标签不得被接收为独立成长。缺完整历史时可以报告已验证的本次用途，但`first/new-growth`保持未知。原三批repeat历史不得清零；当原same_loop_repeat_count≥3且结果仍throughput_only/unclassified，停止强化同一循环。只有库存增长、运送成功、world activity、静态leverage标签或重复展示的对照均不通过成长验收；真实用途也不保证每次重复都产生新leverage。

<a id="mrc-m06"></a>
### MRC-M06：独立路线与回访的真实选择

承接 [AC-MRC-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-06)；依据 [contribution-continuation](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-continuation)。

在未加入强组织、无赞助、认可或levy前置的正常前态走完整路线；记录实际身份/依赖，而不是根据A/B不同名字假定独立。正常合作和合法2E运输费不等于major-power sponsorship。供应失败、B拒绝/取消U或没有新需求时，A仍能保留合法剩余库存、退出服务、检查原自身能力或当前真实替代；新机制不得封锁这些普通入口。

各观察点至多呈现3个实质不同、由同一当前来源支持的选择。原transit/job的ready_at或原窗口截止可作为有界回访触发；没有canonical trigger与复查点时不推荐无限wait。匹配后不自动再供G；新N须为真正的新接受用途，旧G改名不得制造新订单。可行的A未来iron报价沿MRC-M02核全部条件，只读不执行；缺数据时保留具体原因和hold/exit。只显示“再生产/继续运输”却没有真实新用途的变体不能通过。

<a id="mrc-m07"></a>
### MRC-M07：当前权利、未知结果与持久恢复

承接 [AC-MRC-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-07)；依据 [contribution-recovery](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-recovery)。

在Transfer接受前后、arrival后matching前后、U start/complete前后、只读checkpoint与回复间分别注入可定位失败；commit未知先查原operation/receipt及同generation，不能新建G、换请求再次扣料或重复match。已证无效果失败保留失败；已提交结果原样回读。权限恢复后只能按当前条件显式新选择，不能自动复活旧pending或续原t18期限。

以有原transit、已matched但U未用、U pending、用途已完成四种状态分别做snapshot+非空journal恢复，与不中断分支比较原余额、escrow/容量、G/U阶段、历史归属、source-use身份、首次/重复历史和原期限。混generation、原业务matching/必需去重索引的半事务、缺原receipt或未知required业务schema，按原恢复协议拒绝相应可写状态；仅派生ledger/first-use索引缺失或落后而原世界完整时，本证明降为`source_unavailable`并有界重建，普通世界仍可按原规则继续。replay只应用原delta，不重跑运输/生产/validator或重付tariff。读权限改变时当前读面按audience隐藏，原归因不迁移。

G/U窗口到限、当前scope/owner改变、gear失败或新validator/维护要求出现时，分开显示原交付事实、U未完成、实际已耗/在途/仍归谁所有与当前合法下一步；不得免费补料、强制退款、撤销已发生生产或伪造安全退出。到期后可在当前读权限下核实原窗口内的已发生事实，不以晚读延长任何业务权利。含真实背景W的恢复也须逐项保留，不通过静默回到业务表数值获得通过。

<a id="mrc-m08"></a>
### MRC-M08：有界来源、正式入口与玩家理解

承接 [AC-MRC-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-mrc-08)；依据 [contribution-consumers](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-consumers)。

按[唯一技术预算](../../world-runtime/runtime/mature-world-regional-contribution.design.md#contribution-budget)测试恰好与超过边界：1 G/1 A-B供应/1 transfer/1 B job、2参与beneficiary、1 route、2 factory、显式Site、窗口≤8tick、至多3继续候选；每个query共同预算128 logical records、64 event refs、每页16items/最多8fetch、request16KiB、source+proof256KiB、output64KiB。数量和字节同时受限，历史首次性、B writer闭包、pending及每个候选共用预算；不得每页/每候选重置或先扫全世界后截断。超界、缺索引、页不连续、错root、缺水位/absence证明时阻塞相应完整结论；不得静默缩scope、把缺历史当首次或把未读writer当0。实际命令仍使用原独立资源/事务预算。

在同tested build分别运行Pure API、Agent Client和Viewer：对应同一G/U、原运输/job/receipt及读点；核“预览/待决/在途/到货matched/使用pending/用途已证/归因不足”的语义、资源归属、期限、blocker、复查点和真实可选动作。当前通用industry/quote读面与模板卡片不代签新projection；Agent可读建议不自动取得Site-source执行权。正式入口缺接线则该层blocked，不构造不存在的CLI或API返回作证。

用A、B、无授权第三方以及权限刚撤回的会话分别读取全部字段、分页、缓存、错误与导出。A只得到B明确授权的最小用途、数量、期限及证明，不取得私有goal全文、上下文或额外行动权；每次当前audience重验，proof链接不能绕过读权。禁止以两次页面tick相同推同一个权威切点。

真人层沿原成熟世界与受控试玩方案，令参与者从本次真实读面解释：为什么需要这2铁、自己承担什么、到货和实际使用有何区别、哪份库存仍归自己、是否真的首次新增用途、失败后何时和如何继续。记录原任务、实际回答及误解，按原试玩判据与样本限制评估，不新造通过率。Viewer取实际界面、状态和视口证据，Agent正式执行按[原自测手册](agent-runtime-self-playtest.manual.md)核世界结果。最终在原Task/PR分别记录八项实际证据、未运行层次及整体MW-001未覆盖范围；静态算例不能替代完整成熟世界、真人或发行结论。
