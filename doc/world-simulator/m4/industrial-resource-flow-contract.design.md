# M4 工业资源流转合同设计

- 对应需求文档: `doc/world-simulator/m4/industrial-resource-flow-contract.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

<a id="1-设计定位"></a>
## 1. 目标与设计定位

将工业生产、材料账本与物流、电力 owner 归属收敛为一个可下钻的 M4 专业入口；它只整理现行合同，不改变 runtime、WASM、Viewer 或玩法行为。

## 2. 设计结构

- 工业规则层：Recipe/Product/Factory 模块与工厂状态。
- 资源流转层：多账本、调运约束、在途结算与兼容。
- 方案经济层：按同一 root/revision 对齐 planned/admissible/committed/actual 成本、损耗、产出价值与终端结算，不替代价格或数值平衡权威。
- 电力归属层：owner-held electricity、辐射电厂、Location pool 与储能路径下线。
- 可读性债务层：保留 GitHub #2166 quote 缺口，避免把历史设计文本误读为已实现 UI/ABI。
- 追溯层：completed source triplets 由 Git history 与 GitHub task evidence 追溯。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 未覆盖范围 |
| --- | --- | --- | --- | --- |
| [工业报价账本语义](industrial-resource-flow-contract.prd.md#industrial-quote-ledger-semantics) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95; O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 精炼净变化、逐种缺口、实际消费账本、转移失败无部分效果与物流解释分别成立。 | [报价计算与消费者](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | Simulator/Runtime 领域计算、Viewer/Agent 消费者与 QA 独立验收。 | 不改资源种类、规则数值、ABI、供给政策或宣称实现通过。 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | 未提交预览可解释，pending 不冒充领域承诺；只有当前支持且 fresh revalidation 通过的动作可提交。 本行产品关联限定为仅意图 pending 不冒充世界/领域承诺的适用部分；原完整阶段范围仍见[跨资源承诺阶段](../../product/world-rules-core-gameplay/prd.md#跨资源承诺、失效与重复提交边界)，其余义务不由本子项代签。 | [报价计算与消费者](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | Runtime 颁发权威 identity/outcome，产品拥有玩家阶段语义。 | 不自行生成 root/revision 或扩展完整工业执行系统。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 未提交预览可解释，pending 不冒充领域承诺；只有当前支持且 fresh revalidation 通过的动作可提交。 本行产品关联限定为仅首局报价后 fresh 重验及重复无副作用的适用部分；原完整阶段范围仍见[跨资源承诺阶段](../../product/world-rules-core-gameplay/prd.md#跨资源承诺、失效与重复提交边界)，其余义务不由本子项代签。 | [报价计算与消费者](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | Runtime 颁发权威 identity/outcome，产品拥有玩家阶段语义。 | 不自行生成 root/revision 或扩展完整工业执行系统。 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | 同参数预览/提交与实际消费口径属于同一工业因果链。 | [方案绑定](#starter-quote-submit-binding) | gameplay 参数；M4 报价；Runtime settlement；Viewer/Agent 消费。 | 不新建 Plan/receipt 协议，不开放未接线动作。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 方案适用性与查询身份分开，事实漂移和迟到响应不替换当前方案。 | [方案绑定](#starter-quote-submit-binding) | M4 报价；Runtime 重验；Viewer/Agent 查询关联。 | 不用客户端估算或旧 allowed 代替当前准入。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | 结果未知保留原请求核对，重试、回放和换向不复制或误归因既有结果。 | [恢复映射](#request-outcome-observation-separation) | Runtime identity/settlement；Viewer/Agent 恢复。 | 不从缺回执推导未生效，不新增恢复权限。 |

## 3. 关键接口 / 入口

- `BuildFactory`、`ScheduleRecipe`、`TransferMaterial`、`ProductValidated`。
- `factory.power.radiation.mk1`、`PowerPlant` 与 owner electricity 入账。
- `MaterialLedgerId`、在途材料事件、模块请求兼容字段。
- `doc/game/gameplay/gameplay-top-level-design.prd.md` 的玩家可读性合同和 GitHub #2166 债务路由。

## 4. 约束与边界

- 不重引 `PowerStorage`、`DrawPower`、`StorePower` 或 Location electricity pool。
- 不从 completed documentation 推断当前 quote、Viewer、LLM/provider、ABI 或 release readiness。
- 不重平衡规则；具体顺序、字段、hash/manifest 与回放语义仍由 runtime/ABI canonical authority 拥有。
- 不把预览、生产 receipt 或客户端估算当作终端价值；成本/价值归因只沿 M4、Recipe/Factory 与 runtime 的既有 receipt/lineage 组合表达。

<a id="5-设计演进计划"></a>
## 5. 设计演进与未决边界

先由专业实现分别收口 quote/readability、兼容与验证，再由对应 owner 更新当前结论；本专题只提供稳定的文档入口与边界。

<a id="industrial-quote-calculation-design"></a>
## 6. 现有报价的领域计算与消费者合同

本节承接 [PRD 的账本与净缺口条款](industrial-resource-flow-contract.prd.md#industrial-quote-ledger-semantics)，只定义待修正或验证的领域映射，不把文档修订当作实现通过。Simulator WorldKernel 与 Runtime World 各从自己的当前权威状态计算；消费者不另算一套“可执行性”。

### 6.1 Simulator 精炼净变化

对同一个 owner、snapshot 与 BuildFactory 目标，令 `D` 为动作前可用于建厂的 Data、`M` 为精炼投入、`H` 为按现有公式得到的产出、`B` 为该目标的 Data 成本。先验证精炼原有 amount、owner、电力、最小产出和溢出条件，再只读计算：

```text
data_before = D
data_after = D - M + H
shortfall_before = max(B - data_before, 0)
shortfall_after = max(B - data_after, 0)
```

before 不能使用 `D-M`，after 不能从已经截零的 before 只减 gross H。`partial_progress` 要求 `0 < shortfall_after < shortfall_before`；`enough_to_advance` 要求原缺口为正且本次消除该缺口。当前 `EconomyConfig::sanitized` 的产率上限及 `compute_refine_compound_outputs` 保证合法输入的 `H <= M`，因此 `data_after <= D`；这两种目标进展分类在当前共享 Data 模型不可达，不能要求其正例或为凑正例改变产率/资源种类。合法转换成功与改善建厂前置分别验收，无改善或恶化走 `poor_power_tradeoff` 并解释净损失。`recommended_refine_amount` 用同一净效果评估，没有有益的合法精炼量时推荐暂缓，不生成 amount=0 的非法精炼动作。保留现有公式和字段名，不重建独立 compound/hardware ResourceKind。

回归从同一合法 snapshot 分叉，以真实 BuildFactory 可负担性核对 before/after；2500 Data、50 Electricity 的默认反例应显示 `0 -> 3`，不再把 5→3 作为预期。合法转换成功须验证净 Data 不增加、缺口不缩小与分类/推荐一致；低电力、最小产出、overflow、已有目标资源与只读不改变世界继续独立验证。

### 6.2 Runtime 材料缺口与消费账本

`market_quote_decision_preview` 消费规范化、按当前材料同一性聚合的需求；每个 kind 先得到 `max(requested - selected_available, 0)`，再进行有溢出保护的总和。独立行情仍按现行 `build_material_market_quotes` 比较 local/world 来源；具体 `ScheduleRecipe` 的缺口与 allowed 则必须使用其实际消费账本，不能把市场覆盖判断当作排产准入。signed `saturating_sub` 仅防溢出，不能替代逐项截零。

报价若用于 ScheduleRecipe，preferred consume ledger 必须与该 factory/recipe 实际 input ledger 一致。当前直接和 module-backed 新提交均从 factory input ledger 扣料；Agent 只表示发起主体时，不能用其余额代替 factory 库存，world ledger 有料也不能绕过该准入。工厂短缺时 `submission_allowed=false`，world cover 只支持提示受支持的显式补料；材料实际入厂后重新报价与校验，不能通过 `submit_with_world_supply` 隐式补料或切换消费账本。独立市场查询须明示账本与条件边界。DTO 从同一排产结果生成 contributions、total shortfall、allowed 与 recommendation，Viewer/Agent 在报价到达时读取这份结果，不等额外 snapshot 才展示，也不重算不同账本的短缺；此处是报价修正目标，不宣称现有消费者已满足。

Focused fixtures 分别覆盖本地富余、世界富余、一盈一缺、精确覆盖、普通短缺、重复材料聚合；所有分项/总量非负，盈余不抵销短缺，allowed 与推荐一致。另以 Agent/factory 余额不同及 factory 短缺/world 富余的反例证明实际消费 ledger：补料前排产拒绝且两边库存不变，受支持的显式补料实际入厂后才可重报价、校验并提交。quote 调用不得产生 sink、hold 或世界更新。UI 到达即刷新需 consumer 自己的独立测试，内核通过不能代签。

### 6.3 转移与物流解释

Simulator `TransferResource` 在源扣减前完成 from/to 支持的 ResourceKind、owner、共址、余额与目标入账检查；Location Electricity 提前结构化拒绝。源/目标变化作为同一转移结果发布，后续失败不能留下半转移；自转移不使用两个独立快照相互覆盖。通过公开正数提交验证 Agent→Location 拒绝、原源余额不变、没有 ResourceTransferred，再检查合法 Agent→Agent Electricity、Data、自转移和余额不足。保留既有事件/历史兼容，不改变 ActionRejected 的资源无效果语义。

Runtime 物流 quote 的 path binding 与可执行性分开投影：空 binding 的合法 direct/unbound 路径显示中性“无需已绑定路径”等同义说明，真正 route unavailable 才显示阻塞。priority reason 按 producer 的 route/material/explicit/keyword 分类独立解释；无法提交时不展示无条件 ETA。分别验证合法 direct、真实 unavailable、各 reason 和旧零距离 lane，消费者不得自己新增 route、blocker projection 或默认执行动作。

### 6.4 预览到承诺的关联

首次预览用 candidate/config/snapshot 标识比较对象；新 root/revision 仅在 Runtime 权威 accepted outcome 颁发后进入承诺和 actual 对账。重读已有 operation 时可引用其已颁发 root，不能因第一次预览没有 root 而拒绝解释，也不能为通过预览自行构造 root。请求入队/pending、领域承诺和 receipt-backed settlement 服从[产品阶段合同](../../product/world-rules-core-gameplay/prd.md#跨资源承诺、失效与重复提交边界)。blocked/unknown 候选保留无副作用预览及缺失事实；只有当前支持且 fresh revalidation 通过的动作才可提交。

文档验收检查上述公式、账本与阶段在 PRD/design/消费者说明一致，并运行 `./scripts/doc-governance-check.sh`。未来实现验收复用上述有区分力的离线 fixtures，按实际消费者补 UI/Agent 证据；本节不要求把后续完整工业系统实现作为这次文档交付条件。

<a id="starter-quote-submit-binding"></a>
### 6.5 当前工业方案的预览、提交与实际结果

本节承接首局 [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002)、[005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) 及 [AC-FIRST-INDUSTRIAL-003](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#ac-first-industrial-003)。它复用本节的领域报价和 Runtime 既有 identity/receipt，不新增全局 Plan 协议、客户端世界写权或另一份成本计算。

```text
当前目标与对象 → 规范动作参数 → 权威只读预览 → 同参数提交并重验
                                      ↓
                         原请求权威结果 → 实际账本与结算
```

| 数据或身份 | 唯一来源与消费责任 | 不得混用 |
| --- | --- | --- |
| 方案内容 | gameplay 规范动作与合法 factory/recipe/batches；Viewer/Agent 展示和提交同一组参数 | 可编辑查询表单不自动配置固定 CTA；不能在提交时替换批量、对象或消费 ledger |
| 查询归属 | 现有 quote 请求关联及对应 candidate/config/snapshot；若缺关联能力，由现有 transport 补最小请求关联 | 相同参数可有多次查询；后到不等于更新，方案内容不是查询 request identity |
| 材料成本与可用量 | Recipe/Factory 的实际需求和有效 factory input ledger，经领域计算产生 contributions/shortfall/allowed | 不用 Agent/world 余额冒充工厂输入，不用其他种类盈余抵销缺口 |
| 电力与预计产出 | 同一领域排产校验/报价提供需求、限制和预计产物；缺失时保留 unknown | UI 不照配方常量补算，更不能将预计产出视为实际入账或收益 |
| 提交与成果归属 | Runtime 原请求 identity/digest、权威 outcome 与对应 action/job/领域 receipt | 查询身份、candidate、player-facing feedback ID、canonical intent 和 settlement ID 不能互换 |

固定方案的参数由现有动作构造入口统一提供给预览与提交，避免两处各自维护常量。服务端会自动选择工厂时，预览复用同一解析规则；不能取列表第一个、用显示名称猜测或另外维护无校验的跨端映射。开放编辑前必须证明编辑值进入同一提交路径和后继预算核对；未接入的编辑维持独立查询语义。submit 在当前 authority 上重验原参数；影响成本、权限、完成边界或目标的变化返回重评/明确拒绝，不自动执行替代方案。

**失效与在途分离。** 参数或对象变化使旧方案报价不再可提交；snapshot 变化使适用性需要复核，但不等于查询从未发出。消费者必须保留原查询归属并把响应标为待复核，或显式将该 attempt 标为已废弃并提供重新查询；不能只清空 quote、置 idle 后无声丢弃仍在途结果。迟到响应只更新它所属且仍被允许展示的上下文，不能覆盖较新的方案/查询；权限失效优先脱敏，保留请求身份不等于保留无权读取的数据。合法响应到达即更新对应预览，不等待另一次 world snapshot 才展示。

**成本与结算对照。** 以同一方案的已发布预览、实际提交参数和原请求结果核对有效消费账本、实际扣减、产物与电力。accepted/scheduled 只证明自身阶段，生产成功按领域 receipt；差异必须指出来自哪个实际执行范围或权威状态变化，不由 UI 强行对齐预期。quote 缺少有效 consume/consume ledger 等字段时，先复用同方案已发布的权威预览；确需扩展只读 DTO 才增加最小兼容字段，并定义旧消费者的 unknown。具有独立 delegation 前提的 native quote 不能被 Viewer 借用来取得额外授权。首产物、稳定能力、Assembler 建成及交付仍独立判定。只有归属精确匹配时成果可解释为本次玩家选择的后果；已证明但未能精确归因的 owner 成果仍可展示为归属成果，不串到新意图。

<a id="request-outcome-observation-separation"></a>
### 6.6 工业请求的未知结果与恢复映射

本节在工业链中应用 [Runtime DES-WR-IA-001](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-001)、[006](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-006) 和 [007](../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#des-wr-ia-007)；请求身份、持久化、授权与领域结算的 authority 仍由原设计拥有，不新增三套玩家状态机。

| 需要分别核对的事实 | 可以证明什么 | 不能据此推导什么 |
| --- | --- | --- |
| 请求处理 | 原请求已收到、接受、拒绝或仍待核对 | 接受或排队不等于工厂开工，更不等于产物已结算 |
| 世界执行与结算 | 明确范围内的动作应用、扣减、生产或结算事实 | 部分 receipt 不证明整个目标完成；排产应用不等于首产物结算 |
| 观察完整性 | 当前结果是否新鲜、完整并适用于同一请求/对象 | 断连、超时、缺回执或陈旧快照不证明世界未生效 |

缺少适用权威证据时显示结果尚未确认；只有权威拒绝或其他明确无效果结果才能标记对应范围未生效。关闭界面、停止后续委托或换目标不取消已提交动作；取消本身也需要其权威结果。先按原请求 identity/digest 核对已有 disposition，精确幂等重试只读原结果或按既有合同安全处理，参数不同不得沿同一身份重做。若原请求核对或安全重试入口不受支持，显示限制而非假恢复按钮。

已结算范围保留原 owner/intent/action/job/receipt 因果，未结算范围按当前授权和前置条件决定继续、重评或终止；当前权限决定可读范围与新执行资格，不追溯改写历史结果。刷新到新 snapshot 不是核对原请求。换向后旧结果迟到仍归原目标，不能用“最近结果”覆盖新意图，也不能重复扣料、产出、里程碑或庆祝。自然离场与回访读取这些已有事实，不由客户端生成离线收益或复活旧授权。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [工业报价账本语义](industrial-resource-flow-contract.prd.md#industrial-quote-ledger-semantics) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95; O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [报价计算与消费者](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | 同一快照的真实净余额、逐项缺口、消费账本和失败无部分效果。 | 待执行：扩展 [Simulator 精炼 fixtures](../../../crates/oasis7_simulator/src/tests/kernel_regressions.rs)、`simulator/tests/kernel_actions.rs`、`runtime/tests/market_quote_decision_preview.rs` 与 `runtime/tests/logistics_transfer_quote.rs`，按 §6.1–6.3 的正负例绑定实际 candidate；消费者到达态另用 `viewer/runtime_live/tests/transfer_material_quote.rs` 等相应 surface 验证。 | 实施 PR 的 snapshot 分叉、请求/quote/receipt、账本前后值与实际 consumer captures。 | 这些文件是扩展位置，文档未运行业务测试，不代签完整 UI/Agent parity 或发布。 |
| [REQ-WR-IA-001](../../product/world-rules-core-gameplay/indirect-control-agency-and-continuation.prd.md#req-wr-ia-001) Trace refs: O7-REQ-e3193bfb-799b-4a55-bb9d-c7fa59e90185 | [报价计算与消费者](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | 首次预览不依赖未颁发 root，blocked/unknown 可解释，pending 与 accepted 领域承诺分开。 本行产品关联限定为仅意图 pending 不冒充世界/领域承诺的适用部分；原完整阶段范围仍见[跨资源承诺阶段](../../product/world-rules-core-gameplay/prd.md#跨资源承诺、失效与重复提交边界)，其余义务不由本子项代签。 | 待执行：按 [测试手册](../../../testing-manual.md) 的当前工业/消费者场景绑定实际 candidate 与受支持动作，分别捕获未提交候选、pending、权威 accepted 和 receipt-backed settlement；缺失阶段明确记录 blocked/unverified。 | 各阶段的实际 request、snapshot、权威 identity/outcome 与 Viewer/Agent 投影。 | 方法不使 target 工业协议、root 或全量动作变成现行能力。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [报价计算与消费者](industrial-resource-flow-contract.design.md#industrial-quote-calculation-design) | 首次预览不依赖未颁发 root，blocked/unknown 可解释，pending 与 accepted 领域承诺分开。 本行产品关联限定为仅首局报价后 fresh 重验及重复无副作用的适用部分；原完整阶段范围仍见[跨资源承诺阶段](../../product/world-rules-core-gameplay/prd.md#跨资源承诺、失效与重复提交边界)，其余义务不由本子项代签。 | 待执行：按 [测试手册](../../../testing-manual.md) 的当前工业/消费者场景绑定实际 candidate 与受支持动作，分别捕获未提交候选、pending、权威 accepted 和 receipt-backed settlement；缺失阶段明确记录 blocked/unverified。 | 各阶段的实际 request、snapshot、权威 identity/outcome 与 Viewer/Agent 投影。 | 方法不使 target 工业协议、root 或全量动作变成现行能力。 |
| [REQ-FIRST-INDUSTRIAL-002](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-002) Trace refs: O7-REQ-b99516b0-4b36-4aa1-84e4-efc07837cd95 | [方案绑定](#starter-quote-submit-binding) | 预览、实际提交参数与有效消费账本保持一致；承接 AC-FIRST-INDUSTRIAL-001 的工业因果链。 | 待执行：扩展 [材料报价 fixtures](../../../crates/oasis7_runtime/src/tests/market_quote_decision_preview.rs) 的 §6.2 账本对照，并沿 §6.5 的真实 quote/submit 消费者捕获同参数正例、有效投入及产出；绑定实际 candidate/runtime 与有效授权，字段缺失记录 blocked。 | 实施 PR 的 request/quote/submit、authority、账本差异与 receipt；消费者证据独立记录。 | 领域单测不证明实际 GUI、真实 Agent 自主试玩或真人理解。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [方案绑定](#starter-quote-submit-binding) | 事实漂移、查询失效与响应乱序不导致旧参数提交；承接 AC-FIRST-INDUSTRIAL-003。 | 待执行：按 [测试手册](../../../testing-manual.md) 在当前 candidate 的 Viewer quote/submit 消费者中执行参数/对象变化、snapshot 更新、查询在途失效、权限失效及旧响应迟到，核对 §6.5 的请求归属和重验；记录未覆盖入口。 | 实施 PR 的查询时序、适用状态、实际提交参数及消费者 captures。 | 固定消息回归不代签 GUI、真实 Agent 自主试玩或真人理解。 |
| [REQ-FIRST-INDUSTRIAL-005](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-005) Trace refs: O7-REQ-9a0630f6-b261-420b-85a1-1dd426cf3b48 | [恢复映射](#request-outcome-observation-separation) | 原请求未知不等于未生效；已结算和待决范围分开，恢复不产生重复效果；承接 AC-FIRST-INDUSTRIAL-003。 | 待执行：以 [snapshot_progress.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/snapshot_progress.rs) 为投影回归落点，配合 Runtime `agent_intent_v2.rs` 与 `economy_factory_lifecycle/receipts.rs` 扩展已生效但响应丢失、仍未知、部分结算、换向后旧结果迟到四例；绑定实际 candidate/runtime，恢复入口缺失记录 blocked。 | 实施 PR 的原请求 identity/digest、各范围 disposition/receipt、账本前后值及 Viewer/Agent 各自观察。 | 投影与领域回归不证明全部恢复入口、真实自主试玩或真人理解。 |
