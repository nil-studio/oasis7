# 成熟世界同候选经济审计验收手册

- 生命周期：`active`；Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`、`blockchain_engineer`；Last reviewed：2026-11-05。
- 上游：[REQ-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-005)、[AC-WR-MW-005](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-005)；专业依据：[同候选经济审计合同](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#prd-game-015-mature-world-economic-audit)与 [AC-EA-01–08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#economic-audit-acceptance)。
- 技术依据：[成熟世界经济审计设计](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-identity)。本手册拥有同候选有限窗口，以及全资源长窗口的独立核算与跨域验收方法；不重定原货币、生产、权限、治理、征费或发行规则。

Trace refs: O7-REQ-65c69f9e-3752-4b41-b2de-ed99092994e5。下述算例是待执行方法；静态审读和独立整数复算不产生运行通过结论。审读基线为 `f4a3b3919d8a86a8617b551ec031b0809e1b268d`。

## 1. 输入、证据层与原始来源

每次验证固定 `request_key/request_digest`、schema/profile revision、candidate 的 source OID/tested tree/artifact/config、world/genesis/branch、C0/C1 的 qualified commit/state root/original history cursor 与 era、真实 ledger/domain/unit 和 factory/治理范围、读者当前 authority 及预算版本。起止状态、完整写入集合和适用规则必须来自同一组合；两个截图上的 tick 相等不证明同一切点。用原 rooted range descriptor 核验页连续性、计数、水位、生成版及证明级别；客户端提交的若干 event ID 不证明全集。

保存完整输入及原来源引用、实际入口、原 operation/action/job/receipt、逐账本前后量、独立算式、覆盖及异常。独立 oracle 从原规则和原件正向计算，再与被测输出比较；不调用被测汇总器生成预期，不按其最终余额倒推出原动作或治理批准。

| 证据层 | 证明什么 | 未覆盖边界与结果要求 |
| --- | --- | --- |
| 静态审读、算术复算 | 原定义、分支和算例数值一致，方法可评审 | 不证明 fixture 已执行、完整来源可取得、正式消费者已实现 |
| 受控原动作 fixture | 隔离输入经原 action/event/writer 后的实际结果与核算吻合 | 必须说明合法前态如何建立；直接 setter、注入 grant/批准状态只属于更窄组件测试，不通过本手册的合法来源前置 |
| 原 authority 与完整窗口来源 | 同一候选/切点、真实前态、完整 writer/账户集合及实际规则可核 | 缺证明、缺权限或源不支持为 `blocked`，不能因组件算对而通过 |
| 正式只读消费者 | 正式鉴权读取/导出及适用 Agent/Viewer/QA 消费同一报告、异常和补证关联 | 内部 Rust 方法、生成文件、HTTP ACK 或 UI 文案不证明这层；能力已有而本次未运行记 `not_run` |
| 成熟世界、多角色与恢复 | 原持续能力历史、适用 S8/S9/S10、跨角色/节点、恢复和玩家理解的同候选证据 | 四 tick、单节点和受控配置不代替这些实际证据 |
| 发行及公开 claim | 原 release authority 采用适用的完整组合证据 | 任何局部正常、空告警或本手册完成均不自动产生发行通过 |

| 原 authority / 直接消费者 | 定位与复用范围 | 必须保留的限制 |
| --- | --- | --- |
| 材料账本与原写入 | [`MaterialLedgerId`](../../../crates/oasis7_agent_api/src/world_action/ledger.rs)、[`material_transfer.rs`](../../../crates/oasis7_world_state/src/state/industry_transition/material_transfer.rs)、[`recipe_lifecycle.rs`](../../../crates/oasis7_world_state/src/state/industry_transition/recipe_lifecycle.rs)、[`factory_lifecycle.rs`](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs) | 按实际 ledger 扣入；canonical FactoryBuilt 的 input/output 都引用同一个 `Site(site_id)`，不是两份库存 |
| 原配方、工厂与时间推进 | [recipe catalog](../../../crates/oasis7_agent_api/src/world_quote/recipe_catalog.rs)、[duration](../../../crates/oasis7_agent_api/src/world_quote/duration.rs)、[factory catalog](../../../crates/oasis7_agent_api/src/world_quote/factory_catalog.rs)、[`economy.rs`](../../../crates/oasis7_runtime/src/world/economy.rs)、[native 排产](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_economy.rs) | 配方转化、原 Electricity 扣取、pending/complete 与每 tick 折旧分开核算；展示属性不自动产生额外收费 |
| 原 owner 与 delegation | [`owner_delegation_control.rs`](../../../crates/oasis7_runtime/src/world/owner_delegation_control.rs)、[`agent_delegation.rs`](../../../crates/oasis7_runtime/src/world/agent_delegation.rs) | 当前签名 InstallDelegation 支持 TransferMaterial/ScheduleRecipe，不支持 BuildFactory；测试安装 grant 不证明正式授权来源 |
| OC 配置与货币写入 | [config](../../../crates/oasis7_agent_api/src/world_action/main_token.rs)、[原 action evaluator](../../../crates/oasis7_runtime/src/world/event_processing/main_token.rs)、[转账/财库分发 evaluator 与原 proposal guard](../../../crates/oasis7_runtime/src/world/event_processing/action_to_event_core_main_token.rs)、[monetary publication](../../../crates/oasis7_world_state/src/world/main_token_monetary_publication.rs)、[governance publication](../../../crates/oasis7_world_state/src/world/main_token_governance_monetary_publication.rs) | genesis/vesting、发行、财库分发、个人转账和 fee 各有不同供给及账户效果；不把 setter 或 event 注入当成合法原动作 |
| 旧 OC 内部审计 | [`main_token_economy_audit.rs`](../../../crates/oasis7_runtime/src/world/main_token_economy_audit.rs) 及[原测试](../../../crates/oasis7_runtime/src/tests/main_token_economy_audit.rs) | 旧 mint/burn 是累计数；缺 epoch 返回 0、分母至少取 1、比例钳制、测试直接设置供给/财库，均不能代证本手册的完整窗口 |
| 既有领域和组合判据 | [同目标恢复方法](production-recovery-comparison-acceptance.manual.md#production-recovery-03)、[原工作处置](../../world-runtime/runtime/industrial-work-graph-and-recipe-lifecycle.design.md#recipe-work-disposition)、[测试总册](../../../testing-manual.md)、[原 release gate](../../../crates/oasis7_runtime/src/world/operability_release_gate.rs) | 复用原失效/损失与 claim 语义，不新增补偿、惩罚、重试业务或统一发行开关 |

## 2. `native_economic_window_v1` 的正常输入

选隔离世界的四 tick 窗口：C0 为 t0 完整处理后的切点，C1 为 t4 完整处理后的切点，t1…t4 分别为 t0+1…t0+4；审计写入范围为 `(C0,C1]`。C0 的建造、资源取得、授权和治理前史作为因果原件单列，不计入本窗口新增流量。读取全部相关 writer，包括自动折旧和生产状态；不能为得到下表而删掉真实的其他写入。出现未适配但影响已声明 scope 的 writer 时，本次完整结论阻塞。

### 2.1 C0 的真实可达前态

| 对象 | 必要前态与来源 |
| --- | --- |
| 主体、位置和持续能力 | 游戏 Agent `A_game`、其当前 player owner、site S、factory F 及位置/站点 authority 均取原 ID/revision/source；F 是 C0 前合法建成的 `factory.smelter.mk1`，builder 为 A_game，无 pending job。保留原 Build start/complete、建造材料/电力/许可来源与实际 recipe profile。若声称成熟世界，另附原持续能力和成功批次历史；不能写一个 `mature=true` 代替 |
| 材料 ledger | `MaterialLedgerId::Agent(A_game)` 编码 `agent:<A_game>`，原 FactoryBuilt 的 input/output 均为 `MaterialLedgerId::Site(S)`、编码 `site:<S>`。取实际引用并去重，不臆造 `factory:<F>:input`。A_game 有 ore4/fuel1，site 中这两种输入为 0；iron_ingot/slag 的旧余额记为基线，本批新增量为 0。所有期初材料有原初始化/取得记录，旧产物不清空 |
| 能量、配方和工厂能力 | A_game 的 Electricity 至少 8，来自原资源初始化/取得链；独立于 OC 和同名材料。canonical 一批为 `iron_ore:4 + carbon_fuel:1 → iron_ingot:3 + slag:1`、Electricity8、基础时长1 tick、slots2。保留实际 ProductProfile 的 production maintenance sink；正常例要求该 sink 实际为空。工厂 `maintenance_per_tick=1` 仍保留，C0 durability 为原值 D0，D0 足够容纳本窗 4500 ppm 折旧；不能把维护参数改成 0 |
| 原 delegation | 若使用显式授权路径，C0 前从当前 owner 的已鉴权 `apply_signed_delegation_control` 取得三份有效 InstallDelegation 原件：对象分别为 `agent:<A_game>->site:<S>:iron_ore`、`agent:<A_game>->site:<S>:carbon_fuel`、`<F>`，动作分别为 TransferMaterial、TransferMaterial、ScheduleRecipe。保留 host 认证、current owner/source、grant revision/period/有效期、原 native quote 和 causal decision binding。前两项零距离报价为 0，但原 grant `limit_units` 仍须大于 0；排产需有至少 8 的剩余预算。budget 授权不创造 Electricity。禁止用 `install_test_native_delegation_grant` 或 cognition JSON 注入替代 |
| 建厂来源的独立边界 | 当前签名 InstallDelegation 不产生 BuildFactory grant。F 的正常前史须有原合法建设来源；只有预装 Build grant/profile 的组件 fixture 不通过正式建厂来源层。不能因本窗只读或排产 grant 可用就补签建厂、安装 site authority；缺原件时保留该层 `blocked` |
| OC 账户 | 用 `A_oc`、`B_oc` 指向原实际 monetary account ID；与 A_game 的资源账分别标识，字母相同不表示同一本账。初始化合法受控 config：initial_supply1000，epochs_per_year10；原 Genesis allocation 一个唯一 bucket、ratio10000、recipient A_oc、cliff0/linear_unlock0，start 不晚于原 Claim 时间。原 InitializeMainTokenGenesis 后 vested1000/circ0；再原 ClaimMainTokenVesting（有效新 nonce）得到 A_oc liquid1000/vested0、circ1000/total1000、treasury0、issued0/burned0。保留完整原件，禁止 `set_main_token_supply`、`set_main_token_treasury_balance` 或直接写余额 |
| OC 政策及治理 | 受控旧 inflation 的 base/min/max 均为300bps、feedback gain0；新政策三者均800bps、gain0；其余合法 config 保持相同，epochs_per_year10、max_supply 不截断本例。原 split 为6000/2000/1500/500bps。P/P2 按原治理机制已得到可用的 Approved 及完整来源，P 供本窗政策动作、P2 供财库分发；不得直接安装 Approved 状态或按余额猜授权。只证明原机制实际授权范围，不扩大为未证明的宪制治理合法性 |

这里的1000、300/800bps及10 epochs/year都是受控配置输入，不是正式世界价格、通胀或政策修改。正式来源验证必须按实际 config、实际授权及真实前态重算。通过原动作构建的 native fixture 也不自动证明外部 signed/HTTP producer 已接线；尤其原 OC 初始化/vesting 的入口不能代签发行、政策或分发入口。

### 2.2 四 tick 实际动作与独立预期

按原入队顺序执行并保存实际 action/receipt/commit 位置；原 step 先推进 time、处理折旧，再处理 action 与到期 job。若正式入口把动作分到不同 tick，必须按真实序列重算，不能强套本表。

| 时点 | 原动作/原自动 writer | 独立预期 |
| --- | --- | --- |
| t1 | `UpdateMainTokenPolicy(P)`，其实际安排生效索引 E=t1+2=t3；A_game→site:S 依次 TransferMaterial ore4、fuel1，再 native ScheduleRecipe(F, iron_ingot, 一批) | 转移两端等量，随后 site 扣 ore4/fuel1；A_game 的 Electricity−8，job ready_at=t2，未产生 iron/slag。转移参数 `distance_km=0`、`route_id=None`、`route_ids=[]`、`auto_reroute=false`；保留合法 direct 路径及权限依据，不把调用者填 0 当成空间证明 |
| t2 | `ApplyMainTokenEpochIssuance(epoch_index=E−1, actual_stake_ratio_bps=6000)`；到期 recipe 按原 pending 完成 | 旧率发行 `floor(1000×300/(10×10000))=3`；split stake/node/eco/security 为1/0/0/2。site 新增 iron_ingot3/slag1，pending 结束并保留完成 receipt；资源准入后的本地 scarcity delay 为0，不借 world fallback |
| t3 | `ApplyMainTokenEpochIssuance(epoch_index=E, actual_stake_ratio_bps=6000)` | 新率发行 `floor(1000×800/(10×10000))=8`；split 为4/1/1/2。发行只增加 total 和 treasury，circ 仍1000；两次合计 treasury5/1/1/4、total1011 |
| t4 | `DistributeMainTokenTreasury(P2)` 从 `staking_reward_pool` 分2给 B_oc；再 `TransferMainToken(A_oc,B_oc,10,新有效nonce)` | 财库−2、B_oc+2、circ+2，不再次 mint；个人转账只使 A_oc−10/B_oc+10。产物留在原 site ledger，不增加 site→B 的转移或授权 |

`epoch_index` 是发行请求入参；E 的安排边界、实际 commit/time、distribution 的 `distributed_epoch=World time` 与治理 `time/epoch_length_ticks` 分别保存。数值恰好相同不使它们成为同一时间坐标；窗口成员资格由原切点及提交位置决定。

本节 stake/node/eco/security 分别是原 [`main_token.rs`](../../../crates/oasis7_world_state/src/main_token.rs) 的 `staking_reward_pool`、`node_service_reward_pool`、`ecosystem_pool`、`security_reserve`，只是表内缩写，不新建 bucket；fee 负例的新增7进入原 `gas_fee_treasury`。

| 核算项 | C0→C1 的独立结果 |
| --- | --- |
| 逐材料/资源变化 | Agent A_game：ore−4、fuel−1、Electricity−8；site:S：转入+4/+1再原消耗−4/−1，输入净0；输出新增 iron3/slag1。Data 若完整窗口确无 writer 才记变化0；不把缺记录填0 |
| 生产状态/能力 | t1 start 后 Running/pending；t2 completion 后 Idle/settled。每 tick 折旧 `maintenance_per_tick×1000×(10000+min(active_jobs×10000/slots,10000))/10000`：t1前active0→1000，t2完成前active1→1500，t3/t4各1000；四个原 `FactoryDurabilityChanged` 合计4500ppm，C1=D0−4500ppm |
| OC 期末账户与财库 | A_oc liquid990、B_oc liquid12、vested/restricted0；treasury stake3/node1/eco1/security4，总9；circulating1002、total1011、issued11、burned0。`990+12=1002`、`1002+9=1011`；C0原1000不计为本窗收入 |
| 单位与边界 | 材料按 kind 分开，recipe 是转换向量，不要求 ore+fuel 数量等于 iron+slag；Electricity、Data、OC、durability 不相加。两次内部转移不成为全域新增材料。工厂 base_power_draw20、报价 tax/index 不是实际扣款 writer，不另造 E80、税或 fee |

## 3. 八项方法

<a id="economic-audit-01"></a>
### EA-01：同候选、完整窗口和来源身份

承接 [AC-EA-01](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-01)；依据 [economic-audit-identity](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-identity)。

1. 用第2节正常来源固定一次请求，独立核 C0、全部 rooted 范围页、C1、规则/profile 和 writer taxonomy。对每个已声明 ledger/unit、factory、治理主题列出期初证据、所有适用 writer、期末证据及 supporting record keys；确认同一 site input/output 只列一次。完整无事件范围才能产生“该项本窗0次”的判断。
2. 保持数字相同，分别替换 world/genesis/branch、candidate config、C0 root、C1 era/cursor、规则 revision；再提供同 tick 不同 commit 的状态。预期不能混成同一次完整审计，保留错配来源和受影响 scope。
3. 删除中间页、隐藏一个原事件、移除 coverage watermark、提供重复页或客户端挑选的 event ID 集合；即便最终余额碰巧配平，仍不能报告覆盖完成。由原 range authority 证明空范围的对照可成立；404、读取失败、未启用和缺页不是空范围。
4. 在独立原来源分支加入已声明 scope 内的未知 writer，或原支持 writer 的未支持版本；预期阻断受影响的完整结论。不能删该 writer、临时缩 scope、仅查未受影响账户后沿用原请求说完整。

保存身份比较与逐来源覆盖表。任何 C0 合法前态尚未建立时，正常预期仍仅是可评审方法，不签已运行正常。

<a id="economic-audit-02"></a>
### EA-02：非 levy、转换向量与独立 OC 分账

承接 [AC-EA-02](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-02)；依据 [economic-audit-accounting](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-accounting)。

1. 按第2节原动作逐阶段复算，分别核源/目标账本、单位、owner、转移 ID、job、实际输入/输出和 Electricity owner。A_game 的输入→site→生产消耗不能扣两次；site 的 input/output 同 ledger 不能重复累加余额。原材料 quote、目标产量或 ACK 不进入实际流量。
2. 在 t1 start 后建立另一个明确标识的 C1_short：已扣 ore4/fuel1/E8、pending 未完成，产物仍为0。预期解释已消耗及待决生产，不补产物、不退款；这不是原四 tick 请求的成功替代。跨 cut 的 pending/settled 不得双计为自由库存和义务。
3. 独立引入原完成/产物校验失败案例，保留实际 `FactoryProductionBlocked` 及 `consumed_lost` 原处置。先按该原案例自己的 plan/power 核已扣输入和能量，再核无成功产出；不能把已有 module 失败样例的 power10/输入套入本例 canonical E8，也不能把失败改写成“未消费”。
4. 按原 genesis/vesting、3/8发行、分发2、转账10独立复算 OC。逐账户与各 treasury bucket 配对；累计 issued/burned 减 C0 才是窗口变化。令收到财库分发的账户不存在于 C0，核原 distribution 创建账户的来源；不能将 C0 不存在视作“读取失败默认0”。
5. 用同一真实转移重复 receipt 及相同 ID 不同 payload 的独立负例核去重/冲突；不重复制造 source/sink。账本外、受限余额、在途或 escrow 需其原类型，不按普通 liquid/free 库存抵销；尚未适配而影响本 scope 时阻塞。

正常层应可独立得到材料转换向量及 OC `990/12/9/1002/1011`。无原换算 authority 时不能发布统一“净创造价值”数字。相同余额结果不能替代原资金/材料来源。

<a id="economic-audit-03"></a>
### EA-03：关键变化、折旧和旧指标含义

承接 [AC-EA-03](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-03)；依据 [economic-audit-changes](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-changes)。

1. 对正常例核四个 durability 原值/新值、tick、reason、active job/slots及实际生产状态。应解释4500ppm变化和生产状态变迁，不把它们藏在材料余额不变中；不能把 durability 变化自创为 native ScheduleRecipe 的时长、准入或收费因果。
2. 原生产 ProductProfile maintenance sink 与工厂 maintenance_per_tick 分别记录；前者实际为空不表示后者为0。独立引入真实非空 sink、另一个工厂或额外 active job 时，按新增原 writer 与负荷重算；适配不足明确阻塞，不继续套4500ppm。
3. 对旧 OC report 逐字段保留原语义：mint/burn/net_flow是累计，issued按调用者 epoch 查一条记录，treasury distributed按原记录的 World time 标签筛选。正常 C1 的累计 issued11刚好等于本窗增量，是因为本例 C0 issued0；用 C0 已有历史发行的独立例证明两者不可普遍等同。新窗口汇总不能改名冒充原字段。
4. 原比率按实际 numerator/denominator、整数向下取整、10000钳制及原 thresholds 核对；阈值相等时旧逻辑不触发，严格大于才触发。单独构造阈值边界和钳制例，保留未经钳制数值及原展示语义，不能修改正式阈值使样例绿。
5. 用真实空供给/未初始化来源或独立组件零分母例检查：旧 report 的 `total_supply.max(1)`/零记录兼容行为不证明有效比例或健康。新解释应明确未定义/来源不适用与所缺前置，不以0%、无样本、空告警得到正常；组件例不宣称合法成熟世界。

保存原指标与窗口重算的并列表、分母和阈值来源。quote 的 `governance_tax_bps` / cost index 只解释报价影响；没有实际 levy/tax writer 就不计为已付款。

正常 C1 上明确以 `epoch_index=E=t3` 请求旧 helper 时：mint11/burn0/net11；net比率 `floor(11×10000/1011)=108bps`，该索引发行8、发行比率79bps；财库分发发生在t4，因此 helper(E)的分发量/比率为0，完整窗口分发总量仍为2。旧默认 thresholds 为2500/1000/1200bps，不放宽它们迁就通过。若改查t4，分发量2/比率19bps，但本例没有t4发行记录，helper的issued0仅是缺记录默认；不能把这组字段当成完整正常窗口证明。

<a id="economic-audit-04"></a>
### EA-04：治理批准、实际使用与经济效果

承接 [AC-EA-04](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-04)；依据 [economic-audit-governance](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-governance)。

1. 正常例从 P 原提案/批准到实际 UpdateMainTokenPolicy、scheduled config、E−1/E两次 issuance及原 publication逐项连接；独立旧率/新率算出3/8。P2从原批准到 distribution ID、staking bucket−2、B_oc+2/circ+2；A_oc→B_oc10另有账户授权/nonce来源，不归作 P2 再发行。
2. 对照“P已批准但未提交政策动作”“动作被拒绝”“已安排但窗口在适用索引之前”“发行执行但原批准/适用配置证据缺失”。分别报告批准、拒绝、未来条件或证据缺口，不按批准时间直接记+8或根据余额猜政策生效。
3. 将发行请求数值标签、实际 commit/time、distribution World time、治理 epoch 故意错配；提供窗外同标签或另一世界记录。预期以完整原身份和切点判窗，不按整数相同拼接治理效果。缺 E−1/E 中任一预期原记录时保留缺口，旧 helper 的0不补证。
4. 在原机制允许的独立负例中检查无效 proposal、重复 distribution ID、被拒绝或尚不可用的授权；保留原拒绝和零业务效果，不新发代替动作。原机制只验证到哪个范围就解释到哪个范围；不得把 helper 的 Approved 状态推成其未证明的完整治理合法性。

若治理改变本 scope 的资格、成本、义务、权限或替代路线，必须取相应原 effect，而不是仅列 proposal 标题；尚无 adapter 时保留具体未覆盖项。审计本身不执行提案、更新政策、分发或授权。

<a id="economic-audit-05"></a>
### EA-05：缺证、确定异常与有限结论

承接 [AC-EA-05](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-05)；依据 [economic-audit-verdict](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-verdict)。

| 独立输入/反例 | 独立预期与实际证明边界 |
| --- | --- |
| 正常材料和 OC 原来源都完整 | 仅完成声明 scope/候选/四 tick 核对；不推出平衡、公平、长期健康或发行 |
| 删除应有 issuance record，旧 report 得0/无alert | 保留发行来源缺口；不能改成已证明零发行。完整窗确无发行的另一个请求，需原无事件证明且不得伪称本例3/8已执行 |
| OC配平但材料有未解释少量/多量，或少一次原折旧 | 对对应材料/能力保留异常或覆盖缺口；OC正常不抵消。仅允许明确注明范围的 OC 子结论 |
| 同余额但不同 candidate/root，或原件相互冲突 | 记录错配/冲突并阻断受影响结论，不挑选更好看的一份覆盖其他记录 |
| 一次 longrun绿、单节点/单专题报告、只生成文件 | 保留各自原覆盖范围；不能提升为 MW-005 完整审计、跨节点恢复或 release claim |

另从正常动作序列建立 fee 负例：在 t4 的分发/转账之后、同一 tick 内按原 `SettleMainTokenFee(GasBaseFee,10)` 执行，原 burn policy仍为3000bps。独立预期 burn3、fee treasury+7；原 writer 实际只令 circulating1002→992、total1011→1008、total_burned0→3、treasury9→16，A_oc990/B_oc12均未扣款。四 tick折旧仍4500ppm；若实际落到t5，需按真实时间另算，不能假装未发生第五次折旧。

在完整账户/受限余额闭包仍为 liquid1002、vested/restricted0 的前提下，`liquid−circulating=10` 且 `liquid+treasury−total=10` 是可证异常；同时原 fee/event 无 payer 与账户配对写入，是来源缺口。两者必须并列保留，不能因缺 payer 把已知 residual 异常降成未知，也不能虚构 A_oc 支付10来配平。若账户闭包本身不完整，则只报告已经证明的供给/财库变化及缺口，不无证据宣称完整 residual。此负例验证识别，不修复原业务 writer。

每项结论保存可定位原因、实际 affected scope、已证事实、未证事实及补证出口。异常和缺证可并存；审计拒绝完整 claim 不新增原生产/转账的禁令、补偿或惩罚。

<a id="economic-audit-06"></a>
### EA-06：当前读权、同时可行的预算与截断

承接 [AC-EA-06](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-06)；依据 [economic-audit-budget](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-budget)。

1. 正常小窗口须同时落在设计预算内：至多64 tick、256 source logical items（nested entries同计，其中events≤128、原件单项≤4KiB）、64 ledger/unit keys；source call≤64次、单页≤16 items，range header/point read/失败或空页/proof获取均计调用；raw≤1MiB、decoded≤2MiB、proof≤2MiB/4096 nodes、workspace≤1MiB、report≤128KiB、request≤16KiB，每任务保留8MiB、并发4共32MiB。记录实际调用、页、计数、字节、proof工作量、峰值workspace及wall time；2s host deadline是待实现验证的失败边界，不是已测性能承诺。不得分别宣称各上限可达就假定它们同时可用。
2. 将前态原件、三份grant、治理前史、各 nested distribution/输入输出项、coverage proof和重试/缺页/权限失败计入同一预算；不能只数主事件行。实际证明编码、resident对象或完整小闭包超预算时记阻塞/超限，不能省略旧建造、授权或治理前史使正常例看似可运行。
3. 对 records/bytes/proof节点/source calls/单页items/deadline/并发分别做限内及limit+1的原source对照，包含第65次调用、第17条单页item、大单项、异常多nested entries、重复分页、decode膨胀、取消。预期在超额读取或分配前停止，或返回明确不完整及合法续取标识；调用预算耗尽不能真的发出第65次IO后再计数，不先全World/全journal扫描再截断，不产生无界cache、任务或重试。
4. 读者有/无权、读取中撤权、owner generation变化、源timeout/不支持/未完页分别测。每次补页/读取和恢复均重新按当前读权；报告和缓存不能泄露隐藏主体、余额、数量侧信道或原私有record key。权限不足不等于没有经济活动。
5. 允许分段时证明各段属于同一候选/窗口、完整无缺口且去重后才合并；改窗口/范围形成新请求。正常报告、来源详情和导出均只读，核 domain state root、原operation数/nonce/余额未被审计调用改变。

保存实际计量结果及超限位置。文档预算和静态 envelope 不代替运行测量；本方法不授权扩大预算或读取权限。

<a id="economic-audit-07"></a>
### EA-07：同一窗口补证、重启和历史保留

承接 [AC-EA-07](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-07)；依据 [economic-audit-recovery](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-recovery)。

1. 从EA-01缺中间页或EA-04缺治理原件得到有历史身份的受阻结果，恢复该原范围的权威材料后，用新的 `request_key` 携带 `previous_report/revision`，保持原candidate/world/branch/C0/C1/scope和算法版本；request digest同时绑定新增材料及前一报告。相同补证key/digest重试幂等返回该补证任务/结果，同key异内容拒绝，不覆写旧finished task或在旧key下静默重算。新结果关联旧报告和每项原缺口、补入的source key及解决依据；旧缺口与异常记录保留，不能覆写成“从未失败”。
2. 对提交后结果未知、读取中取消、服务重启、分页续取和导出中断分别先查原request/result；重算至多重复只读核算，不重复 Transfer/Recipe/Issuance/Distribution/Fee。观察原world操作数、nonce、balances和root，确认审计重试没有业务效果。
3. 更换候选、branch、C1、scope或规则版本时生成明确的新核对关系，不沿用旧正常标识。原件永久缺失时保留未核验；不能以新世界正常窗口覆盖旧问题，也不能把旧histories裁掉来得到完整性。
4. 恢复时撤权、authority revision变化或预算不足，仍按当前条件阻塞；旧token/cache不恢复读权。补证能解释新增事实，但不能抹掉EA-05已证明的fee残差；修复原业务后形成的新窗口也不是旧窗口自动通过。

保存请求到结果、补证到旧缺口的关联和重启前后domain state差分。没有实际恢复环境时记 `not_run` 或能力缺失的 `blocked`，不以序列化文件存在代签。

<a id="economic-audit-08"></a>
### EA-08：正式只读消费者与成熟世界/发行边界

承接 [AC-EA-08](../../game/gameplay/gameplay-mature-world-progression-contract.prd.md#ac-ea-08)；依据 [economic-audit-consumers](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-consumers)。

1. 在正式实现可用时，沿实际鉴权读取/导出、QA及适用 Pure API/Agent Client/Viewer入口消费同一报告，记录真实route/producer/consumer版本和candidate。比对identity、scope、覆盖、逐项事实、异常与补证关系一致；不能只比字段名或共享DTO。当前内部OC helper或native action可调用，不证明这些入口已经完整采用。
2. 由具读权的玩家/治理参与者独立回答：本批输入在哪里消费、产物归哪个实际ledger；为什么两个输入转移没有创造材料；3/8来自哪些适用规则；分2/转10为何不是再mint；4500ppm说明什么；fee反例哪些确定异常、哪些缺证；哪些范围仍不能判断。记录原答案与未理解处，不把生成解释文本当真人理解证据，不扩写首局/Viewer主设计。
3. 同候选核各角色实际证据：gameplay的解释与边界、runtime/QA的全窗与oracle、Agent/Viewer的正式只读采用、blockchain的原治理/货币及适用跨节点/恢复来源。需要某层而没有实现/环境/权限时明确该层blocked；只有本次未执行则not_run。任何单层成功不得勾完产品整卡。
4. 核原S8/S9/S10及release/公开claim消费者仍使用各自有效authority和组合证据；审计报告仅作有范围的输入。尝试把四tick、单节点、组件fixture或空OC告警标为“长期健康/发行通过”，预期拒绝扩大claim并显示未覆盖层，不触发新release verdict或经济动作。

最终记录至少包含本手册八方法逐项状态、实际source/tested组合、每项完整度/异常、证据层和未覆盖范围。仅当原MW-005要求的适用组合证据都成立，才由原产品/发行authority判断其各自完成度；本手册或设计PR本身不代签。

## 4. 全资源与长窗口组合

本章接续原 AC-EA-01～08 和[全资源设计](../../world-runtime/runtime/mature-world-economic-audit.design.md#economic-audit-full-scope)，使用显式 `complete_economic_window_v2`。原八方法与四 tick 样例保持原范围。以下为待执行方法，不表示真实 reader、正式入口、长期负载或多节点已通过。

<a id="economic-audit-09"></a>
### EA-09：全部资源、writer 与持有类别

固定 source/tree/artifact/config、World/genesis/branch、C0/Cn、schema/adapter/算法 revision、当前读权和有限预算。先由独立审阅者沿 [WorldState](../../../crates/oasis7_world_state/src/state.rs)、[DomainEvent](../../../crates/oasis7_world_state/src/events/domain_event.rs)、[WorldEventBody](../../../crates/oasis7_world_state/src/world_event.rs)及真实 command/publication/自动阶段列出该候选所有可写来源，与被测映射逐项比较；不从被测报告输出反推期望全集。

至少覆盖 Electricity/Data、四类物料账本及实体来源、生产/建造/运输/合同、全部实际 OC 持有类别、积分/奖励/Power reserve/bridge、module/机体/能力、治理/组织/授权变化。逐 family 取合法形成、消费、转移/重分类、失败或无效果，以及原支持的恢复例；某类本世界未启用时，验证同根配置、成员及无事件证明，不能只从正常脚本未调用它推断为零。当前无法合法产生/读取的来源明确 blocked，组件可注入例不得计入正式正常覆盖。

每个例都保存原旧值/新值、原 effect、独立方程、真实单位、当时规则及前置授权；模块/计量与业务效果分账，失败也核已发生费用。特定负例包括：遗漏一个自动 writer、隐藏新成员、把镜像当资产、把同 site 输入/输出算两次、将验证成功当产出、只取旧发行 counter、将 bridge 按名字算增发、用人工 setter 冒充原来源。任何一项不得被其他 family 正常抵消。

在能合法建立的窗口内分别核 genesis 与 vesting：genesis 的原形成和 ordinary issued counter 分开；vesting 只按实际持有类别移动。复用 EA-05 的 fee10 原件，要求缺 payer 与已证 residual10 同时保留。旧算法对缺记录的默认0，不能替代新来源的完整无事件证明。

<a id="economic-audit-10"></a>
### EA-10：160 tick、分段一致性与跨段义务

在合法同候选历史选择 C0/C64/C128/C160 四个已提交切点；实际 cuts 以 qualified commit 为准，不能用 tick 数字制造不存在的状态。以三时间段核对相同父范围，再以另一合法划分复核：逐单位总流量、首尾余额、全部 findings 与原义务应相同，原每次舍入不随划分改变。各段必须包含真实自动阶段，不能为了保持表中单列数值删除发电、维护或治理效果。

下面只是一列 iron_ingot direct-transfer 的独立算术预期，不是完整世界执行 fixture。原直接转移的权限/位置/数量前置由合法原件提供；其他资源和自动写入仍按 EA-09 全部核对：

| 切点与该段实际变更 | A余额 | B余额 | 该段跨账户流量 |
| --- | --- | --- | --- |
| C0 | 20 | 0 | 期初来源单列 |
| (C0,C64]：A→B 3 | 17 | 3 | A−3，B+3 |
| (C64,C128]：A→B 5 | 12 | 8 | A−5，B+5 |
| (C128,C160]：B→A 2 | 14 | 6 | A+2，B−2 |
| 父窗口 | 14 | 6 | A净−6，B净+6，内部转移净形成0 |

另选原支持的生产/运输/escrow，使实际开始和完成落在不同段；独立保存跨 cut 的 job/原投入/位置/未消费量/期限与结算状态。原生产已消费而尚未产出时不造 WIP 可用余额，下段只计实际产出；在途开始扣源、完成入目标与原损失各记一次；跨 C160 未完成保留真实期末义务。具体数量与成本从该原合法案例重算，不把本表转移值套入其他规则。

逐一注入：删中间段、同tick换commit、倒置/交叠段、漏key分区、邻接opening/closing不一致、旧水位、删除已关闭义务的处置链、原effect重复/异内容、先合量再取整。预期父范围不完整或有可定位异常。再让两段在同单位出现已证+10与−10差额，要求父报告保留两项异常，即使末端净残差0。缺证段恢复后可消除其来源缺口，但不能覆盖另一段已证异常。

<a id="economic-audit-11"></a>
### EA-11：预算、并发续办与崩溃恢复

使用设计 §13.4 的同一预算版本，记录计划生成、全部source调用、失败/重读、proof节点/字节、decoded、resident/暂存、执行时长和累计计量。分别核各limit及limit+1、32leaf/4并发、4096leaf计划、较大原子提交、过大原件/关联/findings；真实闭包超限记受阻，不能删字段或拆原子效果。单项最大值不能推导总内存/暂存可同时满足。

对同一plan/revision并发提交两次续办，注入来源I/O之后计量/结果尚未保存、leaf保存未知、取消和host崩溃。核每个有效attempt在I/O前已原子预留其范围和有限累计额度；冲突者先查询，不另消费。无法确定实际消耗时按预留上界保守计入，未知不能释放预算；恢复查询同attempt与同leaf摘要，不重复计算为新的已完成范围、不清零失败用量。任何显式额度扩充须保留旧累计使用。

读取中和重启后分别撤权、变更reader generation、移除原件或耗尽evidence空间；预期不继续读取/披露，保留允许公开的缺口与旧版本，不把无权改成无事件。恢复原材料后仍沿原plan/cuts/算法生成新版本关联；换窗口或候选形成明确新请求。审计的所有续办/重算均不得新增世界nonce、业务event、收费或receipt；对照实际domain state，而不只看接口返回码。

<a id="economic-audit-12"></a>
### EA-12：版本、节点与正式消费者

选同一候选支持的真实规则激活/历史窗口，使其横跨两版已采用规则；固定每个原commit的manifest、schema、source映射及原迁移/linked replacement关系。合法旧结果按旧版核算，新效果按实际新规则核算，pending保留原身份/期限/已付成本；移除历史工件、篡改迁移引用或用最新规则算全部旧事件时，预期该范围受阻且旧异常/历史不改写。

从两个合法节点读取同一正式历史，比较原qualified roots、范围、证据等级和原effect身份；重复副本不重复入账。给出不同branch、落后水位或相冲突材料，预期保留冲突；两个service assertion不能提升为独立formal proof。涉及多World/chain时，各子范围与桥接原件独立核验，不相加同名单位或用单侧回执证明另一侧到账；没有该场景前置就保持此层blocked，不用单节点替代。

最后由 Viewer/Agent/QA 和发布 evidence 的实际入口读取同一父报告与revision：比较范围、覆盖、全部异常、来源和续办结果。只有两段完成时玩家应看到剩余范围，不能见到全域正常；读者应能解释余额与跨段义务、两项相抵异常及版本差别。记录真实操作与答案，不把生成文字代替真人理解。全部段通过只证明本次审计范围，经济平衡、长期健康和发行仍沿原authority及其实际证据判断。
