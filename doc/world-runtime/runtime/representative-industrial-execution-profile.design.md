# 代表性工业执行档案系统设计

- 状态：目标设计提议；RIE-IRON-1/v1 的样本参数、有限终端用途与组合策略待本 PR owner 采纳。本文是可编辑档案源，不是已签发的世界配置、runtime DTO、运行报告或全链能力声明。
- Owner：runtime_engineer；协作：gameplay_designer、M4 工业/物流/批次 owner、配方准入 owner、viewer_engineer、agent_engineer、qa_engineer。审读基线：nil-studio/oasis7@37f7719f6a68b23c6f8f6632571d82ce33a1b4e6。
- 上游：[root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117。玩家政策：[代表性 walkthrough](../../game/gameplay/gameplay-industrial-representative-execution-walkthrough.prd.md#representative-profile-policy)。方法：[专用手册](../../testing/manual/representative-industrial-execution-profile.manual.md#rie-m01)。

## 1. 问题、目标与非目标

既有合同已有 qualified lot、join、物流、output bundle 与 terminal 规则，但覆盖形状和候选字段不能替代一份确定的代表档案。本设计把同一候选的身份、输入、实际物流、合法周期、能量、输出和终端选择合在 RIE-IRON-1/v1 中，使实施者能够按同一前态实现正常结果、失败和恢复；消费者能够解释投入、所得、仍占用价值与下一动作。

本例使用既有原生 Smelter 铁锭配方。新增的是样本的组合选择，不是材料、配方、价格公式、第二套库存、需求数量账、市场、奖励或维护制度。production_only 与 terminal_admission 是两份独立接受的同初态样本，不能在已开始的 root 上翻转完成方式。首局仍使用原 starter 边界；本样本不增加新人的前置，也不使用其他首局或需求目标 PR 代签完成。

## 2. 上游约束与责任

产品拥有 SC-28 适用条件与原 required/full 验收；gameplay 拥有用途与选择；M4 拥有材料、品质/保管、物流、数量与处置；Runtime 原工作图/事务拥有唯一身份、效果和恢复；Viewer/API/Agent 只读同一投影。本文消费[工业工作图](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-model)和 [M4 合同](../../world-simulator/m4/industrial-resource-flow-contract.prd.md#4-technical-specifications)，不复制它们的通用状态机。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | 同一候选的完整工厂、两输入、有限物流、量子、电力、bundle 与用途档案 | [RI-01 具体档案](#rie-profile) | 原配方、工厂、M4 与 finality authority | 不新增目录、质量算法或全局经济默认值 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | 正常链、玩家取舍、两种完成边界、稳定与三入口同义 | [RI-02 正常与完成](#rie-completion) | 原生产/物流/终端 writer、canonical 稳定判据和消费者 | 不以生产、到货或文本投影代签终端、真实体验 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | 输入、容量、电力、mandatory 输出及终端失败；幂等、恢复与因果切换 | [RI-03 失败恢复](#rie-recovery) | 原工作图处置、ExecutionTransaction、G1 持久提交 | 不授予原 profile 未支持的退款、返工或迁移 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | required 与 full 组合方法、三阶段、分支、切换和跨窗口 lineage | [RI-04 组合扩展](#rie-full) | QA、原生后继配方和有界读取 | 方法不是现成自动化或运行通过；不接收其他 SC 全部义务 |

## 3. 当前源码事实、目标接缝与证据边界

| 已核入口 | 本例可复用的事实 | 尚需按本设计接通的组合 |
| --- | --- | --- |
| [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[duration](../../../crates/oasis7_agent_api/src/world_quote/duration.rs)、[factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs) | 精确原生 recipe ID、整数批次、4 ore + 1 fuel → 3 iron + 1 slag、8E、基准 1 tick；Smelter 的 tags、slots、throughput 与维护字段 | 所选工厂真实建成/owner/ledger/当前配方准入必须从同一权威切点绑定；源码目录不是世界内许可 |
| [qualified import/root admission](../../../crates/oasis7_world_state/src/world_rules/industrial_work_admission.rs)、[StartRoot](../../../crates/oasis7_world_state/src/world_rules/industrial_work_start.rs) | 原证明导入扣普通余额并增加共享 escrow；root 固定输入 baseline 和各输出有限容量；StartRoot 从准确 parents 消费后桥接原生产 | 完整 profile 准入、原工作图初始 parent binding 与同候选读面需组合；当前 AcceptRoot 的空 parents 不自动成为齐套，不能通过测试直接改状态冒充正式绑定 |
| [原物流](../../../crates/oasis7_runtime/src/world/logistics.rs)、[transit reducer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transit.rs) | 显式路线的损耗、时长、运费、容量、源扣减和到达结算 | 普通 TransferMaterial 不运输工业 escrow lot；本例先普通调运、后导入。目标 terminal capacity/用途要进入原接收事务，不能由普通 arrival 代签 |
| [原工作图有限终端](industrial-work-graph-and-recipe-lifecycle.design.md#industrial-backpressure-terminal) | owner-bound 有限存储、独立 terminal obligation、matching settlement、失败保留义务已有目标合同 | 本例选定 owner 库存接收用途和全 writer 容量准入；没有同名现成 endpoint 的声明 |
| [recipe lifecycle](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[module metering](../../../crates/oasis7_runtime/src/world/module_runtime_metering.rs) | 原实际生产、产品验证与模块费用仍是 owning writer | 本档案不能使缺验证器、缺资金或未知结果的 job 自动完成；实际费用/receipt 必须来自同次执行 |

这里的当前事实是固定源码审读；本次没有 World、Rust/WASM、模型、正式客户端或真人执行。目标设计可被接收不等于组合已实现。原 implementation/history 状态不因本文改变。

<a id="rie-profile"></a>
## 4. RI-01：RIE-IRON-1/v1 完整可编辑档案

### 4.1 身份、所有者与初态

本节各行共同构成唯一设计源。数字是该样本的提议配置；原配方、损耗和计费公式仍来自上表原 authority。实施/验证时把实际已接受配置与生成的 proof/receipt 作为另一个证据包保存，不把 Markdown 别名当真实签名、hash 或来源证据。

| 项目 | 本例确定选择 |
| --- | --- |
| 配置与完成变体 | profile RIE-IRON-1/v1；候选 P 为 production_only，候选 T 为 terminal_admission；分别从同一完整 C0 的隔离副本接受，world/branch、candidate、root、请求键互不混用 |
| 主体 | A 为有真实当前 owner binding 的执行者、材料 owner 和终端 recipient；L 为两条路线的合法 owner；V 为已安装 iron validator artifact 的真实 payer。本正常fixture选定三个互异agent及资源付款账本；精确身份从原resolver读取，别名不构造权限。若实际合并为同一账本，须聚合真实delta而不能套本例三份终值 |
| 地点/账本 | S、I、T 为三个真实 active/chunk-ready、A 有合法访问权的 Site；F 的实际 input/output ledger 均为 I，若不是同一 I 则不属于此 profile。S 为 ore 来源，I 是原生工厂账本，T 是 owner 库存接收账本；一个 Site 余额只算一次 |
| 初态材料 | S: iron_ore 5；I: carbon_fuel 1、iron_ore 0、iron_ingot 0、slag 0；T: iron_ingot 0。所有原料有独立合法来源。没有同材料旧库存、其他已承诺来源或未决生产/运输；不是给正式玩家自动发材料 |
| 工厂 | F 为已合法建成的 factory.smelter.mk1 实例，builder A、tags smelter/thermal、slots 2、throughput 10000bps、maintenance_per_tick 1；未退役、未暂停，耐久按 4.4 的完整窗口可支撑执行；建设成本属于 C0 之前的真实历史 |
| 配方 | exact recipe.smelter.iron_ingot；使用 C0 所选且每次边界仍有效的 recipe/profile/version/digest、工厂适配和 active 或明确允许本 scope 的 restricted admission。原生 metadata/stage gate 不替代目标六态许可 |
| 合法单位 | 1 个完整整数 batch：ore4 + fuel1 → iron3 + slag1；8E start_only_sink，基准 duration 1 tick；full-only、atomic_kit、all-output atomic，禁 partial、staged intake、split fan-out 和隐含 substitution |
| 输入槽 | 稳定 edge order 为 fuel、ore（按精确 slot ID 排序）；各自只绑定一个确定 qualified parent，数量为 1、4。父来源/到达顺序不改变 required set；两个输入不可用同一 lot 或同名余额顶替 |
| 输出业务容量 | I/iron_ingot 3、I/slag 1，分别计量；必须把已有余额、其他 pending 输出、hold 与本批义务一同检查。它们是有限业务额度，不是 i64 上界，也不是完成 terminal 的凭证 |
| 终端用途 | owner_inventory_acceptance/rie-v1：A 将本批实际 iron3 接收到自身 T 的有限合法库存，明确用途为保留可供后续独立工业选择的材料。T/iron cap3；不表示已被下一配方使用、需求已减少或交易成立。Slag1 留在 A 的 I/slag 有限 buffer，属于必须可解释的保留成本 |
| 窗口与持有 | service_window=none/best-effort；不承诺网络 ETA 或硬交付期限。输入 escrow、output room、route 与 terminal 的持有各按下文实际边界记录；预览没有任何 hold |
| 执行/读取界限 | 正常例只有一个候选 root、一个生产 stage、两 input parents、两 output branches、顺序执行两条物流腿、一个主产物 terminal obligation；没有后台补货、自动重试生成新意图或自动再次生产 |

### 4.2 有限物流与数量

路线必须是显式有效 route/path 绑定；source/destination、当前 owner、访问、几何/距离、tariff 和 capacity epoch 均从原路线 authority 读取。route 注册或客户端距离文字不证明合法路径；不允许 empty route fallback 或 auto_reroute。默认材料 profile 只作为此次固定内容来源，实际世界 profile 不匹配时重新审查，不能覆盖旧世界配置。

| 边 | 固定样本参数 | 独立预期与容量 |
| --- | --- | --- |
| e-in：S → I 的 ore | sent5；100km；route cap5；tariff1E/发送单位；ore 使用原 bootstrap High，即20bps/km | floor(5×100×20/10000)=1 loss，received4；5=4+1。原 speed100km/tick 得1tick；占 route5、运费 escrow5E，arrival 后释放5并向 L 结算5E |
| e-out：I → T 的 iron，仅 T 变体 | sent3；1km；route cap3；tariff1E/发送单位；iron 使用原 Medium，即10bps/km | floor(3×1×10/10000)=0 loss，received3；占 route3、运费 escrow3E，arrival 后释放3并向 L 结算3E，不能以零损耗推定所有物料无损 |
| fuel 本地输入 | I 已有合法 fuel1，无物流腿 | 不是免费生料或自动到货；来源和资格仍须独立证明 |

公式及全局在途上限2来自 [pure_helpers](../../../crates/oasis7_world_state/src/world/pure_helpers.rs)与原物流；本例两腿顺序执行，正常在途数不超过1，仍须检查全世界其他运输占用。单条 route 的容量不替代全局 admission。任何未来损耗配置漂移均需原 quote/fresh submit 重验，不能为了凑齐4 ore 补造少掉的1。

### 4.3 batch、quality、custody 与初始 binding

本例提议的 applicability baseline 为：unit=原 MaterialStack 的整数单位；ore 的 spec/quality/custody 为 rie-ore-spec/v1、rie-ore-qualified/v1、owner-A-at-I/v1；fuel 为 rie-fuel-spec/v1、rie-fuel-qualified/v1、owner-A-at-I/v1。它们是本例由 M4/finality authority 正向签发的精确标签，不是名称推断的检测结果，不声称添加通用品级算法。material_profile ref 必须含精确 id/version/digest，并匹配该 root 的输入 baseline；标签相同而 proof 错 world/owner/source/expiry 仍不可用。

1. e-in 原 arrival 先在 I 的普通余额产生4 ore；它只证明数量/到达，不证明合格。读取原 source/start/arrival/loss 的完整父链后，受信 authority 分别为 ore4、fuel1 签发完整 QualifiedIndustrialLotFactsV1 与匹配 finality certificate。
2. facts 固定真实 world、A 当前 owner binding、source_ledger=I、material/profile、unit/spec/quality/custody、独立 provenance_ref、quantity、finite escrow_capacity 和 expires_at/consensus_height。正常 escrow cap 为 ore4、fuel1；证据有效期覆盖本次确认与首 sink，但每次提交仍 fresh 校验。证据不足返回 unknown，明确不适用返回 not_applicable。
3. 原 ImportQualifiedLot 同一事务使 I 普通 ore4/fuel1 分别减少为0，工业共享 escrow 增加等量4/1；这是 custody transfer，不是 join sink。普通物流或旧生产此后不能再花这5个单位。导入重复或改请求键重用同 provenance 不产生第二份 lot。
4. root acceptance 固定 native plan、input baseline、profile/plan/state digest、各输出容量及实际 allowed dispositions；原工作图的初始 ParentBindingRevisionV1 绑定准确两 parents、provenance 和 ledger。当前原生 AcceptRoot 的空 parents 需由正式初始绑定接缝完成；没有该接缝就保持 blocked，不把不相关 substitution decision 或测试状态注入当齐套。
5. StartRoot 在同一快照核全部 parents 的 arrived/applicable/exclusive/remaining 后，一次消耗 escrow4/1并扣8E；桥接原 reducer 的临时普通余额只能存在于私有准备副本，不能再从真实 I 重扣一次。原成功 publication 同时保存 input sink、job、root/child/receipt 与 pending output obligations。

所有 ref 使用原版本化 domain/schema 编码。设计别名到实际 ref 的证据清单包括 world/branch、C0 state-root、actor binding、source receipt、recipe/profile/plan digest、F/I/S/T 和路线 revision、每次请求键/内容摘要及生成的 root/child refs；摘要从原 canonical 编码计算，不能用全零 hash 或当前最新配置补历史。当前 root admission 要求的各 baseline_impacts（含 quality、power、time、outputs/byproducts、logistics、buffer、terminal、value_class）全部指向此次实际发布的 profile 子声明，禁止只填一个空白总 hash。

### 4.4 能源、验证、维护与正常可完成预算

本例选择 A 起始64E；V 起始64E和128Data；L 的初始E独立记录。所有余额在 C0 已有合法来源。本例不执行建厂、买电、治理发币或证书付费采购，不依赖未来运费收入来支付首 sink。

| 成本/前置 | 本例正向选择及上界 | 实际 writer / 核验 |
| --- | --- | --- |
| 原输入物流/生产/输出物流 | A 分别5E、8E、3E；P 正常合计13E，T 合计16E。运费为 escrow→L，不当作第二次E sink | 原 transfer start/settlement 与 recipe start；每项稳定 effect identity 只一次 |
| 产品验证 | 精确安装的 m4.product.material.iron_ingot、该 artifact 实际 SHA256/manifest 与 stack_limit500；payer V 存在且有资金。只为 iron3 调用1次；slag 的此版 output policy 明示 not_required，完整 registry 无适用 slag validator/fallback | [iron module](../../../crates/oasis7_builtin_wasm_modules/m4_product_material_iron_ingot/src/lib.rs)、[模板](../../../crates/oasis7_builtin_wasm_modules/_templates/m4_product_module_template.rs)、原 resolver；不能靠删 owner/profile 取得免费路径 |
| 调用预算 | 对完整真实编码输入 I_bytes≤4096，原 manifest output≤4096、effects0、emits≤2：preflight≤10Data/4E。模板实际 output_bytes512、1emit、无effects/state：actual Data=ceil(I_bytes/1024)+2≤6，E=2；按实际收据结算 | [费用公式](../../../crates/oasis7_runtime/src/world/incremental_module_fee.rs)；限额不是实际花费，输入含完整编码封套，超限不得截断证据后继续 |
| native authority/终端 | 使用原 native finality verification 与工作图事务，不调用另一个付费 WASM，也不建立“证书免费服务”。正常配置不含额外收费服务；外部签发、模型或人工成本另记，不能折成免费游戏E | [原 authority](../../../crates/oasis7_world_state/src/world_rules/authority.rs)及原业务 writer；部署若另有收费/调用策略，则不满足本版正常前态，需新鲜预算/新样本版本 |
| 维护/背景 | 保留 F maintenance_per_tick1 和原逐tick耐久折旧；初始 durability1000000，正常窗口最多32个 canonical ticks。原每维护单位1000ppm、负载倍率最多2，故该窗折旧最多64000ppm、剩余至少936000，实际逐tick计。没有其他 pending job、额外动作/订阅模块或材料 decay；完整 writer 清单须证明本窗额外材料/E/Data delta=0 | [economy](../../../crates/oasis7_runtime/src/world/economy.rs)、[step](../../../crates/oasis7_runtime/src/world/step.rs)、当前 material/product profiles；iron/slag 的 effective maintenance_sink 必须确认为空，不由字段缺失推定 |

因此正常设计中 A 的两变体均有充足预留之外可用电量，V 的最坏调用资金也可证明满足；durability 成本另列而不伪装成零维护。32tick 是正常夹具观察上限，不是世界服务 SLA、hold 自动到期或生产承诺；超过该上限记录本次正常样本未覆盖，保留真实义务继续按恢复合同处理。

只有完整列出同窗所有 writer 并证明上述零额外 delta 后，才可预期 P 的 A=51E、T 的 A=48E、V=62E/Data按实际输入字节、L 各收5E或8E。若背景有合法额外 delta，逐项对账并重验预算；缺覆盖证明不得将未知补零、引用这些终值或继续标正常可完成。组件前态可直接构造验证规则；正式入口验收还须证明这些身份、来源和配置来自正式 authority。

<a id="rie-completion"></a>
## 5. RI-02：正常顺序、完成与玩家选择

### 5.1 同一候选的正常链

| 边界 | 状态与唯一效果 | 当时尚未成立的结论 |
| --- | --- | --- |
| 只读档案/比较 | 读精确 profile、两输入、route/容量、预算与完成方式；无 root/hold/sink/W | 没有排程接受或终端承诺 |
| 确认输入运输 | e-in 原子扣 S ore5、A5E并建立 transit5/运费escrow5，I ore仍0 | 尚未到货、未合格、未生产 |
| 原 arrival | source5=I ore4+loss1；route5释放、L+5E；arrival有原 receipt | 物流 arrival 不证明 quality/custody，不完成生产 |
| certified import + accept/bind | 普通 I ore/fuel降0，共享 escrow4/1；精确 root/baseline/parents 接受。分别保存各动作原结果 | accepted/qualified 仍未发生 input sink；未齐套不开始 |
| 首 sink/start | 全条件fresh，原子消费escrow4/1、A8E，创建1个原生job和全 bundle 的有限输出义务 | 未到 ready_at，无产物 credit、完成或稳定 |
| stage finish | 原已绑定 validator成功、全部输出去向fresh；一次将iron3+slag1写 I，production receipt回指两parents/原job | P 取得 produced/undelivered；T仍未完成；两个变体都不能据一批称稳定 |
| T：显式确认发往T | terminal capacity writer先在同一准入事务取得T/iron3的唯一容量承诺；原 e-out扣I iron3、A3E，建立transit3，slag1仍留I | 占用容量或在途不等于接收完成 |
| T：原到达T | 原物流一次credit T iron3，tariff付L并释放route3；T容量由在途reservation转为同量实际占用 | 仍为terminal_pending；这一credit不可再执行一次 |
| T：明确终端接收 | 当前A显式授权，原terminal writer校验本profile、root/production/arrival、A/recipient、T、quantity3、purpose和capacity，写一次matching settlement | 完成owner库存接收；不产生第二份材料、实际使用、需求减少、奖励或自动后继生产 |

arrival 的时刻使用原实际 ready_at：发送动作被应用的 canonical tick 加1；stage start 后最早下一 canonical tick 完成。输入到达之后才能做后续 import/start，不能把同tick执行顺序当任意可交换。正常全链可在32tick内安排，但网络迟到保持原事实，不挪动 receipt/expiry 迎合样本。

### 5.2 有限终端与两种完成边界

owner_inventory_acceptance/rie-v1 是本 PR 提议采用的 owner-bound 有限存储政策。终端是原 T Site 材料账本的受约束用途，**不新增私有库存或需求余额**。Runtime 原 TerminalObligationV1/ApplyIndustrialBoundary 目标接口负责 admission 状态和容量；原 material writer 唯一负责实际入账。实施若尚无此接缝，T 变体返回 unsupported/blocked，不把普通物流到货当等价实现。

| 项目 | P：production_only | T：terminal_admission |
| --- | --- | --- |
| 首次目标完成 | matching 同root/candidate的全bundle production receipt；iron3和slag1均有合法有限去向 | matching 本root/candidate的owner库存接收settlement，实际iron3已到T，原bundle和slag义务可回溯 |
| 产出后物理状态 | I iron3、slag1，produced/undelivered；不创建T容量hold或强迫运输 | production后同P；随后iron3在I→transit→T各只属于一个物理bucket；slag1始终单列 |
| 终端容量 | T仅为可比较的后继用途，无效果；选择T要作为新的parent-linked承诺，不复用旧完成身份 | 只有明确发货确认才持有T3；arrived pending3与已admitted3占用同一容量，不再叠加一份hold |
| 稳定 | 单批W最多推进一次生产证据，不能标production-stable | terminal settlement不增加生产W；也不凭三次terminal读取或到货称稳定 |
| 后继选择 | 保留iron并解释I空间已满；或另行确认合法T用途；先解决slag与原料/电力才可能再次生产 | 持有T中合法库存并重新评估真实下一工业用途；T满意味着不能再接一批，须有原允许的出货/消费后再确认 |

T 的所有普通入账、工业输出、导入/释放和其他写入都必须经过同一 capacity authority，不能由另一个 writer 绕开T cap3。arrival前reservation、到达后occupied、admitted是同一3单位在不同阶段的义务；admission不释放真实库存占用。原物理余额允许被别的操作花掉时，pending terminal必须有exclusive hold/守卫；消费或转移只能在接收成功后由新的受权操作执行。没有全 writer 守卫，不启用该终端 profile。

终端 owner/资格失效时不改recipient或目的地。已到货的3只在T有界pending，不能被读取当accepted库存或被另一root重复接收；authority恢复后同身份重验接收。外部接收确认是新受权动作，但复用原terminal obligation/effect key；非同一用途、数量或目的地须新candidate和旧义务明确处置。

### 5.3 稳定与当前可解释的选择

稳定只消费原 [canonical stable-line 判据](../../../crates/oasis7_world_state/src/state/industry_transition/mod.rs)及[当前candidate匹配检查](../../../crates/oasis7_world_state/src/gameplay_capability.rs)：至少3个合法相同canonical生产完成，当前candidate仍匹配且未blocked/paused，完整历史没有因果切换或所要求的中断。本例从W=0开始，只接受1批，故正常终值仍是首产/库存接收而不是稳定。若实现不能把完整RIE candidate与canonical lineage匹配，稳定结论为unknown，不能按同名recipe或quality标签推断。后续另行确认相同candidate周期可以继续原计数；改变required edge、配方/工厂能力、输出或terminal purpose从W=0开始。

| 当前选择 | 实际收益与成本 | 安全下一步/复查点 |
| --- | --- | --- |
| 按P先生产 | 得到iron3与slag1；A承担运输5E、生产8E、ore损耗1和有限空间，V承担真实验证费用 | 全bundle匹配receipt后比较保留或独立T用途；已有输出占满时不再推荐重复排程 |
| 按T接收到指定库存 | 额外承担3E运费及T容量机会成本；使这3iron在明确地点/owner用途下可保留，原I主产物空间释放 | arrival后仍须原admission结果；T满时等待真实空间变化，不许诺市场收益 |
| 缺料/容量/电力时等待 | 不新增sink；已有运输损耗、escrow、WIP或buffer继续按原身份保留，等待不等于退款或世界暂停 | 只在原缺口的权威事件、预算/容量/资格变化后fresh重查；无可负担恢复就延期 |
| 放弃尚未开始的新投入 | 原已支持的TerminateRoot/ReleaseLot仅处理未sink数量，一次释放；既有损失/费用不返还 | 有真实可行后继才显示新candidate；已开始WIP只能按已声明finish/hold路径处理 |

## 6. 接口、数据投影与消费者

| Producer → consumer | identity/ordering | 结果与错误、兼容 |
| --- | --- | --- |
| 原profile/finality/工厂/路线authority → 原工业root | exact配置、actor/world、state/plan/profile摘要及证据有效期；accept与每个不可逆边界fresh | applicable / not_applicable / unknown分开；缺authority不补默认、无legacy silent fallback |
| 原输入物流 → certified import → 原初始binding/StartRoot | 原arrival→独立qualified lot→本root两slot；重复通知仅核原provenance/receipt | custody transfer与sink分开；当前未提供的初始binding适配明确blocked |
| 原recipe/物流/terminal writer → profile投影 | root/revision/child/effect role、production/arrival/terminal parent refs、同一read anchor | projection不扣料、不转运、不接收、不调用validator；完整数据缺失为unknown，不变更世界 |
| 原投影 → Viewer/pure API/Agent | 同world/candidate/profile/revision/read-anchor及授权scope；原请求身份可恢复 | factory/recipe、两input、path/loss/capacity、cycle/power、bundle、terminal、primary blocker、成本、next_action/recheck和progression同义 |

Viewer 的[SC28文本表示](../../world-simulator/viewer/viewer-economic-targets-uiux.design.md#t-eco-04-root-sc28-profile)由此源生成。available只表示当前全部可证明；blocked必须有事实原因，unknown必须指出缺失authority。根因按最早不可逆边界选择，再用稳定的输入slot/边ID排序；终端不足派生出的“生产未完”不重复计成本。只读比较不能生成root/hold/顺位，写动作仍须真实capability和exact transport已公布。

<a id="rie-recovery"></a>
## 7. RI-03：失败、处置、并发与恢复

### 7.1 同一夹具的单变量反例

| 变化/切点 | 必须保留的效果与拒绝 | 本profile真正支持的恢复 |
| --- | --- | --- |
| ore或fuel proof unknown/not_applicable、expired/wrong-owner | 未导入不得导入；已合法导入但未齐套不sink、不扣8E、不产生WIP。此前e-in真实loss1/5E仍存在 | 取得同一来源/用途的有效证明后fresh绑定；或一次ReleaseLot返还未消费量。换source/required edge不能偷换原candidate |
| e-in route cap已满或全局inflight已满 | 新transfer原子拒绝/无扣减，无隐含队列和reservation；保持原请求结果与可读blocker | 原占用释放事件后只重评未决e-in，重报/显式确认；已到达parent或已终态请求不重跑 |
| quote后A不足8E，或F paused/失配 | 首sink前原子拒绝；escrow仍4/1，不能生产半批、欠费或从L未来收入借电 | 原有受权资源到账/能力恢复后同rootfresh重查；真实无资金路径则保留或未sink终止 |
| 开始前I/slag cap不足、owner/access失效 | 全bundle拒绝，iron与slag均无credit；输入未sink | 合法清出原占用或恢复同去向；不能丢slag换主产物成功。改变output因果要新candidate |
| start后、finish前mandatory输出失效 | 已sink4/1和8E不退款；保留一个有限WIP及两branch obligations；任何branch未合法时整bundle无credit | 同目的地恢复后finish一次；本例只启用原工作图有界hold/finish，不授予rework/salvage/return。没有可保留WIP能力须在更早start拒绝 |
| T发货前capacity/owner/资格失效 | 不扣I iron3或3E，不建立transit；已有production/slag结果保持 | 恢复同T资格/空间后fresh确认，或保留I库存并明确T目标未完成；不静默改recipient |
| T在途/到达后admission失效 | 运输按已承诺合法有限arrival/hold规则保留一次结果；T pending3或原有限transit持有，不能同时在I/T各有3 | 原authority恢复后重验同obligation；若连合法到达/保持都无法保证，必须在launch前拒绝。无支持的退运不展示为按钮 |
| 相同请求retry、reconnect、乱序arrival | 同键同内容重读；同键不同内容冲突；两个不同accepted intents不合并。迟到notice不复活closed root | 查询原result/receipt；重评trigger按原event/domain去重，不新建lot/job或terminal效果 |
| 改recipe/factory/required edge/output/terminal用途 | 原baseline与已发生效果不被改写，旧义务仍占原容量 | 原Prepare/CommitIndustrialCutover完整枚举旧hold/WIP/transit/buffer/terminal；仅已支持hold/finish/未sink释放可选，新candidate W=0；缺任一合法处置整次拒绝 |

当前 native 工业处置子集与原工作图目标必须分别读：原生准入只接受其真实支持的 Terminate 等动作；本文的WIP hold/finish是目标work-graph profile选择，未接通时不得以普通TerminateRoot销毁WIP或返还已sink。长期保留保持有限数量与可读义务，不通过清理任务悄悄删除。

### 7.2 共享容量与事务

正常例一个contender不证明争用。full反例沿原 AllocationRoundV1 同一world boundary/容量epoch收集完整contender集，提议本样本采用 accepted canonical tick、再稳定root ID排序，非调用到达顺序；每轮先完整考虑既有holds，full-only不能分半批，未获分配保持deferred和原顺位。容量释放产生一次重评trigger，重试不更新接受时点；同一容量反复可用时最早仍有效项须获得分配或带原因终止。此样本策略待owner采纳，只用于本profile，不宣布SC21通用公平策略已交付。

一次input sink、bundle settlement、transit handoff或terminal admission的图节点、材料/E/Data delta、容量、receipt、journal和publication必须使用原 [ExecutionTransaction](../design.md#62-unified-executiontransaction-architecture-p0)。不得先写terminal accepted后补材料，也不得在all-output失败时泄露主产物。样本只读计划不得新增业务余额，slot/branch/terminal effect各使用原稳定幂等键。

### 7.3 持久提交未知与恢复

沿原Runtime/G1的 NotCommitted、Committed、StatusUnknown 三值：已证明NotCommitted才按原请求规则重试；Committed重读同一结果；StatusUnknown先恢复原durable generation/receipt，不能当reject、重新造root或重扣。root publication未知时必须隔离整个旧World的writer和tick，不是只禁用本goal按钮；不得从新旧generation拼材料、控制状态、proof、capacity和receipt。

snapshot/journal完整保留 lot/provenance消费索引、两parent bindings、root/baseline、pending job/transit、两branch、费用与容量、terminal obligation及其matching索引。恢复从合法checkpoint验证闭包和守恒后继续；重复replay只重放原已提交delta，不重跑validator、再签proof、重新报价或重选route。缺parent/schema/完整索引或unknown generation时拒绝安装，消费者显示unknown并保留原身份。迟到的projection/outbox可用原receipt补发，不生成第二份世界效果。

## 8. 部署、安全与权限

profile启用要求生产者、正式接受、原写事务、全writer容量/hold守卫、持久化/恢复以及三入口读面声明同一能力版本；缺一项则只读目标档案可用、提交blocked。Owner更新不能把A、V、L或T的权限互相代替；配方准入不授予材料/终端权限，路线owner不授予source消费权。对观察者仅显示有权读取的摘要/blocker，不披露第三方lot、签名、私密queue或资源余额。

能力集按已接受变体取用：P只要求其输入、生产、I有限输出及恢复/读取接缝；未选T的route/receiver未知只使该后继选择不可提交，不反向延迟P的匹配production完成。T另要求e-out、T全writer守卫和独立terminal接收；不能因P可运行而默认T已具备能力。

目标服务复用原认证/限额/有界reader，不新开模型、WASM或隐藏免费执行池。外部费用按实际服务来源记录；无预算、未知收费或caller无法授权时不启动相应工作。sample profile不能被无签名Markdown参数覆盖，也不能重新解释已提交历史。

## 9. 质量、容量与交付物

唯一可编辑档案为本文件§4与full扩展；玩家用途源在walkthrough，Viewer只消费。本设计、[方法源](../../testing/manual/representative-industrial-execution-profile.manual.md#rie-m01)和对应文本表示共同交付；不另做截图冒充可编辑原件，不要求为未开放动作生产正式美术资产。更新材料/配方/路线或终端选择须递增profile版本、重新算守恒与预算、同步三入口样本和方法，旧证据保持原版本。

每个基础profile限制1个pending root、2个input parents、2个branches、2个先后物流job、1个terminal义务；扩展profile限制3个生产stage、8个parents、8个branches、4个物流job、3个terminal/保留义务。完整读取按原有界接口分页并验证范围/终结标记，不能取前N伪称完整，超过该声明范围需新profile而非无限扫描。历史/closed receipt归档仍需保留唯一消费证明；不能为节约记录驱逐未结义务。

<a id="rie-full"></a>
## 10. RI-04：full组合与长期兼容

full不是只把基础样本多跑一次。本节提议扩展 RIE-IRON-GEAR-MOTOR/v1，作为同一来源家族的独立target fixture；仍使用原生catalog而不新增配方。原required范围不因此被替代，基础P/T先各自成立。

| 阶段/边 | 完整材料、有限去向与依赖 |
| --- | --- |
| 初态扩展 | S ore10、I fuel2；e-in参数100km/High/cap10、tariff1，loss2、到达8、1tick；另有A的合法copper_wire3和hardware_part1在后继Assembler的输入J。J/I/T相应输出初始0，I/J/T各为实际合法Site；这些扩容/stock是本扩展提议，非基础例原地升级 |
| stage1：Smelter iron2批 | sink ore8/fuel2、16E；全bundle产iron6+slag2，I有限cap iron6/slag2；保留slag2和下一步未用iron2，不能假装全部iron都被下游消费 |
| edge1/适用性 | 精确划分iron父批：4给J、2保留I。I→J显式route1km/cap4/tariff1，Medium损耗0，1tick。J实到4后重新取得当前gear输入所需的quality/custody证明并等量导入；不能沿用ore证明 |
| stage2：已合法Assembler gear2批 | 原recipe.assembler.gear，sink iron4、8E；产gear2到J有限cap2，原duration1tick；独立stage/receipt，不复用stage1的sink或完成 |
| stage3：同Assembler motor1批 | 原recipe.assembler.motor_mk1，基础consume gear2+copper_wire3、7E；原motor product profile的maintenance_sink额外要求hardware_part1，故有效required slots是gear2、wire3、hardware1，全部独立证明/导入/齐套后一起sink；产motor_mk1 1到J有限cap1、duration1tick。不能删原维护义务来缩成两输入 |
| finite terminal | motor1经显式J→T route1km/cap1/tariff1；固定原bootstrap motor Low=5bps/km，floor(1×1×5/10000)=0损耗、收到1、1tick；T/motor cap1。保持同一physical→arrival→terminal三层。I铁余量2/slag2与T motor1分别有有限合法去向，三类最终库存逐项核对 |

此链有3生产stage、4种原始独立input（ore/fuel/wire/hardware）和4个output branch（iron、slag、gear、motor）；跨stage不可把不同材料裸数量相加。F及Assembler均已合法建成、builder A、slots2/throughput10000bps/maintenance_per_tick1、初始durability1000000；Assembler实际input/output均为J。世界在C0已有合法scale_out和目标recipe准入，不靠本次生产自动授予后继工厂/recipe。全部stock、使用权限和阶段历史必须从原authority证明。

扩展沿§4.3逐stage导入和绑定：每个材料使用 rie-full-<material>-spec/v1、rie-full-<material>-qualified/v1 及 owner-A-at-<I或J>/v1 的本profile正式baseline，各lot到达后独立签发；escrow cap分别按本stage完整量（ore8/fuel2、iron4、gear2/wire3/hardware1），前三次stage共6个独立parent，不跨stage复用input sink。stage1 iron分配4+2要有原parent-linked child数量/索引，若实现不支持受权拆分就不能激活该full profile。

扩展A仍起始64E，业务总额为输入运费10 + iron生产16 + I→J运费4 + gear生产8 + motor生产7 + J→T运费1 = 46E。V仍64E/128Data，绑定精确iron与 [motor validator](../../../crates/oasis7_builtin_wasm_modules/m4_product_component_motor_mk1/src/lib.rs) artifact/manifest；stack分别6≤500、1≤128，正常各调用1次，每次完整输入≤4096，合计preflight≤8E/20Data，actual4E/≤12Data。gear与slag的此版validation policy明示not_required且完整registry无适用fallback；不能把缺profile或缺payer当正例。motor新增hardware1从原[bootstrap product policy](../../../crates/oasis7_runtime/src/world/bootstrap_economy.rs)与[effective consume](../../../crates/oasis7_world_state/src/world_rules/industrial_work_admission.rs)复算，不新增维护公式。

扩展正常观察上限128tick；每厂最大耐久折旧256000ppm、剩余至少744000，实际负载逐tick记录。仍须正向核查原背景writer、零额外材料/E/Data delta、无其他pending或订阅调用；满足时独立预期A18E、V60E/Data按实际字节、L收15E。该上限不延长任何proof有效期或变成hard SLA；配置/loss/费用/权限漂移则保持失败，不补货、挪时或改用基础iron参数。

full方法在stage1未sink、stage1 WIP、edge1 transit、J buffer、terminal_pending各自切checkpoint；注入单一缺失/漂移、两root争用、事件重复/乱序和cause-changing cutover；至少3个parent-linked诊断窗口回读原计划/actual。恢复保持所有残余、分支、费用与旧root，旧receipt不完成新candidate。仅有基础native入口时full为未实现/未执行，不删除该义务。

## 11. 验证与接收

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 必须观察的结果 | 验证方法 | 证据与产物 | 当前限制 |
| --- | --- | --- | --- | --- | --- |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | [RI-01 具体档案](#rie-profile) | 同candidate完整事实、真实proof与预算、两输入及损耗守恒 | [RIE-M01](../../testing/manual/representative-industrial-execution-profile.manual.md#rie-m01) | 固定配置源、来源/authority清单、逐项独立算例 | 静态核对不生成有效签名或正式世界来源 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | [RI-02 正常与完成](#rie-completion) | P/T完成边界、once物理/接收分离、稳定不越级、三入口同义 | [RIE-M02](../../testing/manual/representative-industrial-execution-profile.manual.md#rie-m02) | 两独立同初态正常链、每边界receipt和授权读面 | 当前无组合运行/真人证据 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | [RI-03 失败恢复](#rie-recovery) | 首sink前拒绝、既有义务守恒、G1全World隔离、恢复不复制 | [RIE-M03](../../testing/manual/representative-industrial-execution-profile.manual.md#rie-m03) | 单变量反例、故障切点、不中断对照与replay闭包 | 只有原capability实际支持的动作才可执行 |
| [root-sc28-profile](../../product/world-rules-core-gameplay/prd.md#root-sc28-profile)；Trace refs: O7-REQ-6951ebb2-04c7-4a7e-a3af-338856dce117 | [RI-04 组合扩展](#rie-full) | 三stage/多分支、共享容量、cutover、跨窗口lineage与人类理解 | [RIE-M04](../../testing/manual/representative-industrial-execution-profile.manual.md#rie-m04) | full配置、完整结果包、实际环境/成本/介入与未覆盖 | 不以文档设计、模型模拟或其他SC成果代签真实full |

独立接收先核原文及数例、producer/consumer职责与错误路径，再分别记录设计已覆盖、实现是否存在、组件/组合/真人是否执行。文档门禁/链接通过只证明组织正确，不证明新profile激活或游戏可玩。

## 12. 取舍决策、采纳与后继

本 PR 选择原生iron而非新配方，是因为它已具有两独立输入、主/副产物和明确整数周期；选择有损输入与有限owner库存，是为了让成本、材料守恒、output与terminal差异可逐项复算，无需引入买家或奖励。新profile、容量、路线、quality/custody标签、owner库存接收用途、正常观察窗及有限争用策略都是待owner审的目标设计选择；采纳后才成为该profile的设计基线，不能写成历史已批准政策。

后继实施按原工作图/authority/ledger单写职责接通profile接受、初始binding、全部writer容量守卫和terminal接收，再接正式三入口与验证。原始SC-28 required/full及其他source依赖的义务保留；未知实现、正式入口、素材与真实执行结果按各自交付阶段记录。本次不扩大SC27候选发现、SC29站点或SC32制度范围。
