# oasis7 Simulator：分块世界生成与碎片元素/化合物池（设计文档）设计

- 对应需求文档: [chunked-fragment-generation.prd.md](chunked-fragment-generation.prd.md)
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位
定义分块世界生成与碎片元素/化合物池设计，让碎片世界生成具备分块组织、资源池配置与可复现行为。

## 2. 设计结构
- 分块生成层：按 chunk 组织碎片世界的生成范围与密度。
- 资源池层：为元素/化合物配置碎片生成池与分布规则。
- 新手资源生成层：以可 sanitize 的 starter core、每 chunk 保底和材质分区参数降低极端冷启动，并保持确定性与有界终止。
- 运行期补种层：按稳定周期和比例生成 committed delta，同步 location、fragment/chunk budget 与 `FragmentsReplenished` 事件。
- 精炼预览层：把 compound -> hardware 的电力成本、产出和当前第一工业目标缺口转成玩家提交前可读 quote。
- 资源恢复预览层：把 `FragmentsReplenished` 的补种时间、预计补种量和当前目标关联转成玩家等待/转移/替代路线判断。
- 初始化接线层：把分块生成结果写入场景和世界初始化路径。
- 验证回归层：校验 chunk 布局、资源分布与生成稳定性。

### 2.1 需求承接与分配表

本表承接资源恢复预览、旧精炼账本边界及中心坐标与预算归属的受影响条款；其他生成与预算设计继续遵循前述结构和约束。owner 为 simulator 与 Viewer 维护者，服务权威投影由 world-service 维护者负责。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-FRFR-1](chunked-fragment-generation.prd.md#req-frfr-1) | 根据实际执行后端区分已知禁用与能力未知，均不得推荐等待补种。 | [DES-FRFR-CAPABILITY](chunked-fragment-generation.design.md#des-frfr-capability) | Viewer / world-service 的执行与投影边界。 | 不增加 live 补种执行器或 RPC。 |
| [REQ-FRFR-2](chunked-fragment-generation.prd.md#req-frfr-2) | named/generated 入口必须可判断支持范围，启动 seed 不作为当前服务预算。 | [DES-FRFR-SOURCE](chunked-fragment-generation.design.md#des-frfr-source) | bootstrap 与 WorldServiceProjection。 | 不修复 named scenario 的全部初始快照投影。 |
| [REQ-FRFR-3](chunked-fragment-generation.prd.md#req-frfr-3) | 完成 tick 及正常恢复后的下一机会严格未来。 | [DES-FRFR-TIME](chunked-fragment-generation.design.md#des-frfr-time) | WorldKernel step、snapshot、journal。 | 不识别任意手工构造的中间快照。 |
| [REQ-FRFR-4](chunked-fragment-generation.prd.md#req-frfr-4) | 在非 due 时计算下一目标量，缺预算和时间溢出不能返回成功零值。 | [DES-FRFR-ESTIMATE](chunked-fragment-generation.design.md#des-frfr-estimate) | 现有 compute_replenish_target 与 chunk budget。 | 不保证候选接纳数量、材料或落点。 |
| [REQ-FRFR-5](chunked-fragment-generation.prd.md#req-frfr-5) | 预览只读、旧建议失效，补种结果按目标 chunk 的实际增量核验。 | [DES-FRFR-READONLY](chunked-fragment-generation.design.md#des-frfr-readonly) | 现有 Viewer 失效处理与 kernel committed delta。 | 本地组件测试不证明真实服务或浏览器端到端闭环。 |
| [中心与预算归属](chunked-fragment-generation.prd.md#fragment-center-budget-contract) | 初始化/补种/消费的最终中心归属一致，内部取整不跨账 | [落位设计](#fragment-center-budget-design) | Simulator 生成器、kernel 与 replay 维护者 | 不重算历史 committed delta；不改变半径可跨块规则 |
| [旧精炼账本边界](chunked-fragment-generation.prd.md#legacy-simulator-refining-ledger) | 同一 Data 余额的净变化与独立 M4 材料账本分开；before 不预扣投入 | [精炼账本解释](#legacy-simulator-refining-design) | Simulator kernel 与报价消费者 | 本次不修报价实现，不改变 M4 材料类型或账本 |
| [固定场景初始化切片](../../product/agents-world-simulation/prd.md#sc-agent-001)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | 相同有效 seed、配置和 chunk trigger order 的初始化可重现；origin/seed/bootstrap/Agent 必需预生成按原合同 | [既有生成承接](#chunk-generation-existing-obligations) | Simulator；[场景加载与来源重建](scenario-files.design.md#generated-world-provenance-design) | SC-1 整体仍部分覆盖；不代签全部权威事件或交互结果 |
| [几何与物理量](chunked-fragment-generation.prd.md#chunk-physical-contract) | cm 整数、物理公式、化合物到元素、生成即定量和 fragment/chunk 同步扣减守恒；并沿原专业条款 [资源库存模型](chunked-fragment-generation.prd.md#chunk-budget-contract) | [既有生成承接](#chunk-generation-existing-obligations) | Simulator 物理与资源账本维护者 | 不选择新材料、恢复率或经济平衡；不等于链 provenance |
| [CG5 初始化](chunked-fragment-generation.prd.md#chunk-init-contract) | 原初始化顺序、canonical 配置名和有效 spacing 配置接入；不含 floor/core 参数 | [既有生成承接](#chunk-generation-existing-obligations) | Simulator/scenario；SC1 仅初始化 | 不承诺首产物可达；中心与邻块另沿原 CG7 条款 |
| [专业生成参数](chunked-fragment-generation.prd.md#chunk-floor-contract) | sanitize、有限保底、core/材质区间与有界尝试；原参数与约束不变 | [既有生成承接](#chunk-generation-existing-obligations) | Simulator 生成器 | 产品 N/A；理由：原专业条款明确这些参数不进入产品承诺；范围：仅此生成参数和尝试约束，不排除实际资源数量/来源的产品消费者 |
| [CG9](chunked-fragment-generation.prd.md#chunk-hard-budget-contract) | fragment/block 三档硬上限与既定裁剪顺序，init/action 路径分别验证 | [既有生成承接](#chunk-generation-existing-obligations) | Simulator 生成与 QA | 产品 N/A；理由：CG9 定义生成性能/有界终止的专业机制而非玩家额度；范围：仅硬上限与裁剪，实际数量/预算仍有产品消费者，CG10 replay 不在此处 |
| [补种落账](chunked-fragment-generation.prd.md#运行期-frag-补种与落账) | 原周期/比例/spacing、location 与双级预算及事件原子一致，恢复使用既成 entries | [补种实际结果](#des-frfr-readonly) | kernel/replay；预览沿 REQ-FRFR-1..5 | 不新增 live 执行能力；异常批次原子性须独立负例，不能以正常 replay 代签 |
| [产品既有能力与事实切片](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth) | 仅已知禁用与来源未知的反馈；时间/数量仍由其余 FRFR 条款分配 | [对应设计](#des-frfr-capability) | 原 Simulator/Viewer/world-service owner | 不覆盖全部 source profile、allocation 或 handoff |
| [产品既有只读与真实复查切片](../../product/world-rules-core-gameplay/prd.md#source-replenishment-effect-recheck) | 仅预览无效果、失效撤下和按目标 chunk 的真实增量复查 | [对应设计](#des-frfr-readonly) | 原 Simulator/Viewer/world-service owner | 不覆盖全部 source profile、allocation 或 handoff |
| [产品既有生成场景的可测量物理真值](../../product/world-rules-core-gameplay/prd.md#generation-physical-truth) | 仅原文“一套可测量的物理真值”及确定、可追溯且不被表现抽象改写的映射；本切片适用于既有 chunk/fragment 场景的物理与资源量事实。 | [专业设计](#chunk-generation-existing-obligations) | 原 Simulator 与对应报价/Viewer 消费者 | 仅既有专业合同细化，不改变参数或正式 profile |
| [产品既有来源作用域与数量的同一权威事实](../../product/world-rules-core-gameplay/prd.md#source-location-quantity-truth) | 仅原 source 评估中身份、作用域、预计可满足量及权威/估计边界，以及 chunk/fragment 生成与预算由既有专题拥有的切片；消费与表现不得把另一来源/作用域的数量冒充当前事实。 | [专业设计](#fragment-center-budget-design) | 原 Simulator 与对应报价/Viewer 消费者 | 仅既有专业合同细化，不改变参数或正式 profile |
| [产品既有来源准备的成本与当前目标影响](../../product/world-rules-core-gameplay/prd.md#source-current-goal-impact) | 仅原 source 评估中时间/资源/物流成本及对当前配方/首个工业目标的影响可读；本切片接收当前专业 profile 的真实目标影响，不以正产出自动冒充目标推进。 | [专业设计](#legacy-simulator-refining-design) | 原 Simulator 与对应报价/Viewer 消费者 | 仅既有专业合同细化，不改变参数或正式 profile |

## 3. 关键接口 / 入口
- chunk 生成配置
- 碎片资源池定义
- `AsteroidFragmentConfig`: 每 chunk 保底、starter core 半径/密度、材质分布策略、补种周期与比例
- 确定性保底与跨 26 邻块 spacing 校验入口
- `FragmentsReplenished { entries }`: 补种落账与 replay 的 committed delta
- `refine_quote` / `refine_preview`: `compound_mass_g`、`electricity_cost`、`hardware_output`、`electricity_after`、`hardware_shortfall_before`、`hardware_shortfall_after`、`first_goal_relevance`、`recommended_refine_amount`、`refine_value_class`
- `resource_replenishment_quote` / `fragment_refill_preview`: `chunk_coord`、`target_frag_id`、`current_frag_remaining_summary`、`chunk_remaining_summary`、`next_replenish_tick`、`ticks_until_replenish`、`estimated_replenished_frag_count`、`estimated_replenished_resource_hint`、`next_industrial_goal_relevance`、`wait_cost_summary`、`recommended_resource_action`
- 世界初始化生成入口
- 生成回归用例

## 4. 约束与边界
- 同一 seed 的生成结果必须稳定。
- 生成器和场景只使用规范配置名 `asteroid_fragment`，不保留旧字段兼容别名。
- `min_fragment_spacing_cm` 默认 `50_000` cm；场景未设置时继承 `WorldConfig`，有效值 `<= 0` 关闭额外 spacing。候选需同时检查已放置碎片、26 邻块与 boundary reservation。
- 保底候选必须受固定尝试上限、spacing 与 chunk 数量预算约束；不可达时允许不足，但不得无限尝试。
- 补种只在启用的 asteroid-fragment runtime、有效周期和未满 chunk 上发生；location 与两级资源预算必须和事件原子一致。
- replay 应用事件中的既成补种明细，不隐式重算生成结果。
- 同 seed、effective config 与 chunk trigger order 的回归必须稳定；历史 replay 使用 committed delta，不能按迁移后的默认值重算布局。
- 资源池变更不得破坏既有场景可加载性。
- 精炼预览只解释现有 `RefineCompound` 公式和当前目标缺口，不改变产率、电力成本、动作 ABI 或回放语义。
- `refine_quote_missing` 是玩家侧机会成本可读性缺口，不代表要新增完整加工链或市场系统。
- 补种预览只解释现有 `FragmentsReplenished` 节奏和当前目标关联，不改变补种周期、补种比例、分布权重、资源守恒或 replay 语义。
- `resource_replenishment_quote_missing` 是玩家侧等待/转移/替代路线可读性缺口，不代表要新增动态经济调参或完整资源市场。
- 不在本专题扩展复杂程序化地形系统。

<a id="fragment-center-budget-design"></a>
### 中心坐标与预算归属

Trace refs: [O7-REQ-218aeae3-6a52-423a-9b69-9570975edc3c](../../product/world-rules-core-gameplay/prd.md#source-location-quantity-truth)

初始化与补种共享同一厘米坐标落位约束：候选转换为整数 cm 后、进入 spacing/接纳与预算落账前，必须校验 `chunk_coord_of(center) == generating_chunk`。内部块采用半开边界，不能让 round 到内部上边界的中心留在原 chunk 预算；若采用确定性归一或拒绝候选，须受现有有界尝试与有效配置约束，并在相同 seed/config/trigger order 下可复现。世界最外沿沿用现有 clamp，碎片半径跨界仍合法。

接纳后的 `Location.pos`、fragment 索引和初始化/补种预算必须使用同一个最终中心；后续消费按该中心定位同一预算。不能仅修预算或仅修显示坐标，也不能在 replay 重新随机生成。此设计对应 [#4497](https://github.com/eng-cc/oasis7/issues/4497) 的边界取整缺口；实际归一策略、生成版本兼容与 fixture 由实现 PR 验证，本文不宣称现有生成器已修。

<a id="legacy-simulator-refining-design"></a>
### 4.1 旧精炼账本解释

Trace refs: [O7-REQ-1c30e45e-99f4-4353-8f80-380f23243cbe](../../product/world-rules-core-gameplay/prd.md#source-current-goal-impact)

旧 Simulator 动作中的 compound 输入与 hardware 输出都落在同一 owner 的 `ResourceKind::Data`。预览应从动作提交前余额 `D` 读取 before，以真实扣入加出后的 `D - M + H` 读取 after，再按同一目标 `B` 比较净缺口；不得将 `D - M` 称作 before，或仅因 `H > 0` 显示 `partial_progress`。当前报价实现尚使用预扣投入的 before，纠正它需要独立实现和回归，本次只补齐消费者合同。

M4 的 `MaterialStack.kind` 标签和独立材料账本遵循 [PRD 的领域边界](chunked-fragment-generation.prd.md#legacy-simulator-refining-ledger)，不由旧 Simulator 操作名推导资源映射。数据投影与玩家提示均需明确所消费的执行路径；此说明不修改产率、电力成本、ABI、实际落账或回放。

<a id="fragment-refill-preview-design"></a>
## 5. 资源恢复预览与执行边界

本节规定当前代码的能力判断、报价和失效语义。沿用 `PreviewFragmentReplenishment`、`FragmentRefillPreviewPreflight`、`FragmentRefillPreview` / `FragmentRefillPreviewResponse` 与 `GameplayActionError`，不新增协议字段、调度器或第二套生成算法；实际版本、运行结果与发布状态由 PR 和真实验证记录承载。

<a id="des-frfr-capability"></a>
### 5.1 实际后端能力

Trace refs: [O7-REQ-ff33b61a-c81c-46f8-8e34-928af72858ef](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth)

产品消费者仅为[既有补充决策切片](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth)；本节仅覆盖当前声明的后端与状态，不扩到全部 source profile。

[`handle_fragment_refill_preview`](../../../crates/oasis7/src/viewer/runtime_live/fragment_refill_preview.rs) 先保留现有签名、session key、玩家 Agent 绑定和访问校验，再判断执行后端。`config.world_service.is_none()` 的本地 live 由 `RuntimeWorld::step` 推进，没有 fragment replenish 执行路径，因此返回 disabled。响应中的两个可选时间字段为 `None`，`replenishment_due=false`，不推荐 `wait_current_chunk`。`wait_cost_ticks=0` 与数量 `0` 只表示明确禁用后的既有协议数值；UI 显示没有等待选项，不能解释成免费即时补种。

world-service 模式的能力和资源来源无法由本地 Runtime 状态证明。当前 `WorldServiceProjection` 不发布权威 chunk/fragment 预算及对应补种能力，因此 handler 返回现有 `fragment_refill_preview_projection_failed` 错误并说明 unavailable。该错误不宣告服务本身禁用，也不尝试用临时 kernel 报价。真正持有当前完整模型并通过 `WorldKernel::step` 执行补种的消费者继续使用 kernel quote。

<a id="des-frfr-source"></a>
### 5.2 启动与当前状态来源

产品消费者仅为[既有补充决策切片](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth)；本节仅覆盖当前声明的后端与状态，不扩到全部 source profile。

[`bootstrap_runtime_live_world`](../../../crates/oasis7/src/viewer/runtime_live/support.rs) / `bootstrap_runtime_world_with_chunk_runtime` 的 generated-world 分支保留 seed model；named scenario 的初始模型投影是独立边界。本次两种本地入口均按实际执行能力返回 disabled，不依赖 seed 是否存在来推断资源已耗尽。响应使用 `current_chunk_resources_unavailable` / `current_fragment_resources_unavailable` 摘要和空元素条目，UI 将前者显示为当前资源余量不可用；空条目不表示已核验库存为零。

world-service 即使偶然保留了含 chunk budget 的 `seed_model`，也不能据此开放预览。启动数据只描述初始化，`runtime_state_to_simulator_model` 的兼容映射不能补出服务当前权威碎片预算。未来开放 live 正向预览的解除条件是服务发布权威资源状态与实际执行能力，并完成该入口的跨周期验证；普通模型保留或初始化截图不满足此条件。本次不修改 bootstrap/mapping 或服务 RPC。

<a id="des-frfr-time"></a>
### 5.3 完成 tick 与下一机会

产品消费者仅为[既有补充决策切片](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth)；本节仅覆盖当前声明的后端与状态，不扩到全部 source profile。

[`quote_fragment_refill_preview`](../../../crates/oasis7_simulator/src/kernel/fragment_refill_quote.rs) 读取处理完成的 kernel 状态。正常 `WorldKernel::step` / `step_intents_batch_from_envelopes` 返回前已完成当 tick 的补种尝试，包括候选全部被拒绝而没有 `FragmentsReplenished` 事件的情形。

对启用的周期 `I > 0` 和当前时间 `t`，下一机会使用 `t + (I - t % I)`：非边界向前到下一周期，恰在边界则前进完整周期。checked addition 溢出时拒绝报价，不返回当前 tick。此读取边界下 `replenishment_due=false`，等待成本等于下一机会减当前时间。

从完成快照与 journal 恢复后，按恢复结果的时间和模型使用相同公式；replay 继续应用已提交增量。该合同不增加 `last_replenish_tick`、snapshot/journal 字段或版本，不重跑补种修正预览，也不声称 `from_snapshot` 可以判定任意手工构造中间态是否处理完成。

<a id="des-frfr-estimate"></a>
### 5.4 数量、预算与错误

产品消费者仅为[既有补充决策切片](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth)；本节仅覆盖当前声明的后端与状态，不扩到全部 source profile。

启用条件仍是 kernel 的 asteroid-fragment runtime 开关、正周期、正比例和正数量上限。报价复用 [`compute_replenish_target`](../../../crates/oasis7_simulator/src/kernel/fragment_replenish.rs)，以所请求 chunk 的现有碎片数和缺口计算下一机会目标量，不以 due 作为计算开关。满 chunk 是已知零；生成时仍执行边界、spacing 与确定性候选校验，实际接纳可能低于目标，材料分布不作保证。

必须取得该 chunk 的 `chunk_resource_budgets` 才能返回其当前资源摘要；缺失预算直接返回不可用错误，不再默认空预算。未知 chunk 保持既有拒绝路径。已知禁用和已知零可以使用整数 `0`；未知来源走错误路径，不给旧整数 DTO 增加哨兵或新的 unknown schema。Web 对无效数量作 unavailable 防御展示并提示刷新，对已知零说明当前预计不新增碎片。

<a id="des-frfr-readonly"></a>
### 5.5 只读、失效和实际结果

Trace refs: [O7-REQ-dcbc5b97-bbfb-4345-b411-926bd9dff3d9](../../product/world-rules-core-gameplay/prd.md#source-replenishment-effect-recheck)

产品消费者仅为[既有补充决策切片](../../product/world-rules-core-gameplay/prd.md#source-replenishment-effect-recheck)；本节仅覆盖当前声明的后端与状态，不扩到全部 source profile。

预览不消费世界动作 nonce、不推进时间、不更新 fragment/chunk 预算、不生成补种回执。kernel 的重复读取保持 snapshot 和 journal 不变，live 的已鉴权禁用/不可用结果不修改 Runtime 状态。客户端沿用新快照和重连时的 quote 失效逻辑；匹配到本次预览错误时清理旧 quote，卡片在 pending、error 或坐标变化后撤下旧建议。

真实补种仍由 [`maybe_replenish_fragments`](../../../crates/oasis7_simulator/src/kernel/fragment_replenish.rs) 更新 location 与两级预算并写入 `FragmentsReplenished { entries }`。一个事件可能覆盖多个 chunk，验收按 `entry.coord` 核对目标 chunk 数量和预算，不能把全世界补种量计入当前报价。恢复与 replay 不重新生成历史增量。

## 6. 兼容与剩余边界

现有 wire 字段、鉴权、持久 schema、事件与 replay 格式保持兼容；变化是 local live 不再承诺不存在的补种，service 未知来源不再被静态 seed 填充，kernel 预览给出严格未来机会和真实目标量。客户端继续把补种预览视为只读 preflight。

named scenario 初始快照投影的完整性、world-service 的权威碎片投影与 live 补种执行均不由本设计宣称完成；相关能力改变时，simulator、Viewer 与 world-service 维护者需复核来源和执行边界。当前验证不证明真实网络服务、浏览器实际游玩、任意中间快照恢复或性能阈值。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

本表定义验证计划与未覆盖范围；本次实际候选提交、命令、环境和结果记录在 PR 与实际测试输出。结构检查仅证明关系可解析。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-FRFR-1](chunked-fragment-generation.prd.md#req-frfr-1) | [DES-FRFR-CAPABILITY](chunked-fragment-generation.design.md#des-frfr-capability) | 已知本地无执行能力时禁用；服务未知时不可用。 | [auth_actions_fragment_replenishment.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_fragment_replenishment.rs)：`runtime_fragment_refill_preview_disables_local_named_scenario_without_mutation`、`runtime_fragment_refill_preview_does_not_classify_unknown_service_projection_as_disabled`，使用候选版本真实构造和签名请求，服务负例只验证 handler、不发 RPC。 | 当前 PR 的固定候选提交与 Rust 测试输出。 | 不证明服务实际执行补种。 |
| [REQ-FRFR-2](chunked-fragment-generation.prd.md#req-frfr-2) | [DES-FRFR-SOURCE](chunked-fragment-generation.design.md#des-frfr-source) | 实际 named/generated 入口不把 seed 当作当前执行能力或预算。 | [snapshot_generated_world.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/snapshot_generated_world.rs)：`generated_world_sidecar_initializes_runtime_live_snapshot_map`；与 named 场景签名负例及带真实 fragment seed 的 service 负例配对，在候选版本运行。 | 当前 PR 的生成世界 fixture 和 Rust 测试输出。 | 不证明 named 初始快照全部资源投影。 |
| [REQ-FRFR-3](chunked-fragment-generation.prd.md#req-frfr-3) | [DES-FRFR-TIME](chunked-fragment-generation.design.md#des-frfr-time) | 完成 tick 100、正常恢复与 replay 后下一机会为 200。 | [fragment_replenishment_quote.rs](../../../crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs)：`fragment_refill_preview_tracks_real_step_replenishment_and_restored_boundaries`，候选版本真实 kernel 初始化后推进 100 次；时间溢出由 `fragment_refill_preview_rejects_unrepresentable_future_tick` 覆盖。 | 当前 PR 的 kernel step、snapshot/replay 测试输出。 | 不证明任意手工构造中间快照的完成状态。 |
| [REQ-FRFR-4](chunked-fragment-generation.prd.md#req-frfr-4) | [DES-FRFR-ESTIMATE](chunked-fragment-generation.design.md#des-frfr-estimate) | 非 due 估计下一目标量；满 chunk 已知零；缺预算不可用。 | [fragment_replenishment_quote.rs](../../../crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs)：`fragment_refill_preview_reports_wait_cost_when_replenishment_is_not_yet_due`、`fragment_refill_preview_does_not_recommend_waiting_for_a_full_chunk`、`fragment_refill_preview_rejects_unknown_budget_instead_of_reporting_zero_resources`；Web 补充验证见表后。 | 当前 PR 的候选 Rust 与聚焦 Vitest 输出。 | 不保证实际候选量或材料，也不证明 Web 接入正向 live 补种。 |
| [REQ-FRFR-5](chunked-fragment-generation.prd.md#req-frfr-5) | [DES-FRFR-READONLY](chunked-fragment-generation.design.md#des-frfr-readonly) | 重复预览只读，事件按目标 chunk 核验，失败/过期后移除旧建议。 | [fragment_replenishment_quote.rs](../../../crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs)：`fragment_refill_preview_is_deterministic_and_does_not_mutate_kernel_state` 与真实 step 测试，在候选 kernel 验证重复读取和按 chunk 核验增量；Web 补充验证见表后。 | 当前 PR 的只读、增量及 Web 状态测试输出。 | 不证明真实网络重连或浏览器游玩闭环。 |
| [中心与预算归属](chunked-fragment-generation.prd.md#fragment-center-budget-contract) | [落位设计](#fragment-center-budget-design) | 内部 face/edge/corner、max−0.5cm 两侧及世界外沿都落到正确预算 | 按 [S7 场景回归](../../../testing-manual.md)在实现候选上增加确定性边界 fixture，分别核对初始化、补种、后续消费与 committed replay；当前仅审读合同 | 实施 PR 的候选、seed/config/order、最终中心、chunk 与两级预算断言结果 | 本文无固定 seed 运行或实现修复结果，不代签真实世界/Viewer 验收 |
| [旧精炼账本边界](chunked-fragment-generation.prd.md#legacy-simulator-refining-ledger) | [精炼账本解释](#legacy-simulator-refining-design) | 按同一 Data 余额复算动作前后净缺口，不能混用 M4 材料标签 | [kernel_regressions.rs](../../../crates/oasis7_simulator/src/tests/kernel_regressions.rs) 中现有 quote 测试需随实现修正，覆盖投入降低 Data、净缺口未缩小而有正产出、目标已满足；对照实际成功动作后的余额，静态公式核对与测试执行分别记录 | 实施 PR 的固定候选、D/M/H/B、前后余额、报价及真实断言结果 | 当前旧测试断言不证明目标 before/after 正确，本次未运行或修正这些测试 |
| [固定场景初始化切片](../../product/agents-world-simulation/prd.md#sc-agent-001)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | [既有生成承接](#chunk-generation-existing-obligations) | 相同有效输入和顺序的 chunk/预算重现 | [tests/chunking.rs](../../../crates/oasis7_simulator/src/tests/chunking.rs)、tests/init.rs：grid/coord/bounds/seed、bootstrap_chunks 与初始化断言，分别固定有效输入和入口 | 候选、输入、触发顺序、布局和预算比较 | 未运行；不证明全部交互/权威事件、正式链或玩家结果 |
| [几何与物理量](chunked-fragment-generation.prd.md#chunk-physical-contract) | [既有生成承接](#chunk-generation-existing-obligations) | 生成 total=remaining，消费同步扣减与 overdraw 拒绝 | [tests/fragment_physics.rs](../../../crates/oasis7_simulator/src/tests/fragment_physics.rs)；tests/init.rs 的 consume_fragment_resource_keeps_fragment_and_chunk_conservation；对应实际物理/消费源核对 | 固定输入、物理量、双级前后预算与拒绝断言 | 公式/守恒 fixture 不等于完整 block 非重叠、经济或链权限验证 |
| [CG5 初始化](chunked-fragment-generation.prd.md#chunk-init-contract) | [既有生成承接](#chunk-generation-existing-obligations) | 初始化顺序与有效配置 | [tests/init.rs](../../../crates/oasis7_simulator/src/tests/init.rs) 的 bootstrap、seed、agent 与 reservation 断言，固定 seed/config/order；源码定位未运行 | 候选、输入与初始化布局 | 不证明全部玩家结果或正式链 |
| [专业生成参数](chunked-fragment-generation.prd.md#chunk-floor-contract) | [既有生成承接](#chunk-generation-existing-obligations) | sanitize、core、有限保底、材质区间 | [tests/asteroid_fragment.rs](../../../crates/oasis7_simulator/src/tests/asteroid_fragment.rs) 的 floor/core/sanitize/zoned fixture，固定有效配置和 seed；未运行 | 接纳/拒绝、终止与生成结果 | 不保证首产物或所有邻域；参数非承诺不豁免验证 |
| [CG9](chunked-fragment-generation.prd.md#chunk-hard-budget-contract) | [既有生成承接](#chunk-generation-existing-obligations) | 三档硬预算、既定降级与 init/action 路径 | [tests/init.rs](../../../crates/oasis7_simulator/src/tests/init.rs) 的 max_fragment/block/multichunk 断言；性能按原 DoD 另测 | 各块/碎片上限、裁剪后物理量和预算、实际性能证据 | 未运行；补种 block 截断与 OOM/耗时未核实；CG10 replay 单列冲突 |
| [补种落账](chunked-fragment-generation.prd.md#运行期-frag-补种与落账) | [补种实际结果](#des-frfr-readonly) | 正常 entries 恢复与异常批次不部分落账分别验证 | [tests/persist.rs](../../../crates/oasis7/src/simulator/tests/persist.rs) 的 replenish replay 与 fragment_replenish.rs apply 路径；异常重复 ID/失败批次需独立负例，原 FRFR 测试沿 §11.1 | 原事件 entries、目标 chunk/location/双级预算、异常前后模型 | 正常恢复不能代签异常整批原子性；不证明 live 补种 |
| [产品既有能力与事实切片](../../product/world-rules-core-gameplay/prd.md#source-assessment-replenishment-truth) | [对应设计](#des-frfr-capability) | 仅已知禁用与来源未知的反馈；时间/数量仍由其余 FRFR 条款分配 | [真实测试源](../../../crates/oasis7/src/viewer/runtime_live/tests/auth_actions_fragment_replenishment.rs)；沿本表对应 FRFR 原行的具名用例、固定候选和实际入口执行，本次未运行 | 实施候选的签名请求/step/事件/快照与原断言输出 | 不证明真实网络、浏览器或全部 profile |
| [产品既有只读与真实复查切片](../../product/world-rules-core-gameplay/prd.md#source-replenishment-effect-recheck) | [对应设计](#des-frfr-readonly) | 仅预览无效果、失效撤下和按目标 chunk 的真实增量复查 | [真实测试源](../../../crates/oasis7_simulator/src/tests/fragment_replenishment_quote.rs)；沿本表对应 FRFR 原行的具名用例、固定候选和实际入口执行，本次未运行 | 实施候选的签名请求/step/事件/快照与原断言输出 | 不证明真实网络、浏览器或全部 profile |
| [产品既有生成场景的可测量物理真值](../../product/world-rules-core-gameplay/prd.md#generation-physical-truth) | [专业设计](#chunk-generation-existing-obligations) | 仅原文“一套可测量的物理真值”及确定、可追溯且不被表现抽象改写的映射；本切片适用于既有 chunk/fragment 场景的物理与资源量事实。 | [真实测试源](../../../crates/oasis7_simulator/src/tests/fragment_physics.rs)；沿同表原专业条目的现有断言或明确待补边界 fixture，固定候选输入，未运行 | 实施 PR 的物理/数量/预算或 D/M/H/B 前后净变化断言 | 不代签实现修复、全部跨层或玩家端到端 |
| [产品既有来源作用域与数量的同一权威事实](../../product/world-rules-core-gameplay/prd.md#source-location-quantity-truth) | [专业设计](#fragment-center-budget-design) | 仅原 source 评估中身份、作用域、预计可满足量及权威/估计边界，以及 chunk/fragment 生成与预算由既有专题拥有的切片；消费与表现不得把另一来源/作用域的数量冒充当前事实。 | [真实测试源](../../../crates/oasis7_simulator/src/tests/init.rs)；沿同表原专业条目的现有断言或明确待补边界 fixture，固定候选输入，未运行 | 实施 PR 的物理/数量/预算或 D/M/H/B 前后净变化断言 | 不代签实现修复、全部跨层或玩家端到端 |
| [产品既有来源准备的成本与当前目标影响](../../product/world-rules-core-gameplay/prd.md#source-current-goal-impact) | [专业设计](#legacy-simulator-refining-design) | 仅原 source 评估中时间/资源/物流成本及对当前配方/首个工业目标的影响可读；本切片接收当前专业 profile 的真实目标影响，不以正产出自动冒充目标推进。 | [真实测试源](../../../crates/oasis7_simulator/src/tests/kernel_regressions.rs)；沿同表原专业条目的现有断言或明确待补边界 fixture，固定候选输入，未运行 | 实施 PR 的物理/数量/预算或 D/M/H/B 前后净变化断言 | 不代签实现修复、全部跨层或玩家端到端 |

Web 补充验证使用候选版本的 [fragment_refill_preview_card.test.jsx](../../../crates/oasis7_viewer/software_safe_src/fragment_refill_preview_card.test.jsx)，覆盖 REQ-FRFR-4 的已知零、无效数量与合法未来时间约束，以及 REQ-FRFR-5 的 pending/error/坐标失效展示；[fragment_refill_preview_state.test.js](../../../crates/oasis7_viewer/software_safe_src/fragment_refill_preview_state.test.js) 覆盖匹配预览错误后清除旧 quote。卡片只有在启用、数量为安全正整数、下一机会和正等待时间可用且一致时才展示等待建议，disabled/零量改为转向，未知数量或时间提示刷新重评；未知数量的指标本身显示不可用。

## 12. 设计演进边界

继续使用既有分块生成、资源池、初始化和确定性回归。若未来为 live 增加运行期补种，先由实际后端提供可追溯的当前预算与执行能力，再以真实启动、预览、跨周期事件和快照解除 unavailable；该演进需要单独的实现与验证，不从本次报价修正隐含推导。


<a id="chunk-generation-existing-obligations"></a>
## 13. 既有生成义务的有限承接

Trace refs: [O7-REQ-b4be4414-7def-4831-8302-f9523e82f944](../../product/world-rules-core-gameplay/prd.md#generation-physical-truth)

本节只补齐既有合同身份、适用入口和验证导航，不改变前述生成、预算、FRFR、中心归属或精炼义务。合同采用与实现验收分开：未修实现或未执行测试仍保留为未核实，不要求重新选择已采用合同。全篇尚有[回放规范冲突](#chunk-replay-conflict-register)，本次不宣称完整闭合。

### 13.1 需求承接与分配表

既有无冲突义务的追加关系见 §2.1 原表；原 FRFR、中心与精炼关系保留。

FRFR-1..5、中心归属与 legacy refine 的具体义务、owner 和验证继续由 §2.1/§11.1 原表承接，不重复改写。中心不变量已采用但归一/拒绝策略与兼容验证待实施；refine before 使用完整 D 已采用，旧报价偏差不转成待重新选择的业务合同。

### 13.2 验证映射表

本表是计划与源码断言定位，未执行任何业务测试。选择同一固定候选、有效配置、seed、触发顺序、实际入口和恢复版本；源码存在、局部 fixture 和历史通过记录不能替代本候选结果。

追加验证关系见 §11.1 原表；上述固定候选与未运行边界适用于追加行。

<a id="chunk-replay-conflict-register"></a>
### 13.3 回放规范冲突登记

[CG6 的 ChunkGenerated 重建规则](chunked-fragment-generation.prd.md#chunk-cg6-replay-contract)与[CG7 的生成/补种 delta 泛句](chunked-fragment-generation.prd.md#chunk-cg7-replay-wording)同时保留，尚不能作为同一个已一致的恢复合同验收。前者及 CG10 明确从固定生成上下文重建并比对 seed/counts/budget；后者泛称生成与补种只应用 committed delta。当前 [replay.rs](../../../crates/oasis7_simulator/src/kernel/replay.rs) 的 ChunkGenerated 调用 generate_chunk_fragments，FragmentsReplenished 则 apply entries；这是当前机制定位，不替规范作取舍。

泛句由 [#2764](https://github.com/nil-studio/oasis7/pull/2764) 文档整合加入；该补丁未撤销 CG6，也未定义新事件载荷、版本迁移或 target/current 分界。本次未核实存在替代决定。待 Simulator/replay 规范 owner 明确两条原文的适用范围或真实 supersession；本次不将任一条降为 N/A、删除、自动解释为新目标，亦不改变源码/schema。统一正式资源 remaining 的链 delta 目标不能自动消解本专题初始布局恢复的矛盾。冲突保留期间不宣称本设计全文规范闭合。


### 13.4 范围与未决风险

本次有限范围是既有非冲突义务的身份与准确分配。物理真值、source 作用域/数量、成本和目标影响以原产品句的有限切片接收；不借 SC7 标题、M4 材料域或新增公式。CG5 将 SC1 初始化与明确不进入产品承诺的生成参数拆分，CG9 专业性能机制不吞入 CG10 回放。回放冲突仍见 §13.3，owner 尚需裁决；本次没有选取 supersession、事件载荷、版本或正式 profile 新策略。实际实现与业务验收未运行，不宣称全篇语义一致或端到端完成。
