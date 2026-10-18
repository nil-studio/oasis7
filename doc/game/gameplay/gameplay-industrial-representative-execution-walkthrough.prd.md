# 代表性工业配方执行 Walkthrough 合同

- 上层产品映射：承接 [`world-rules-core-gameplay` 的代表性配方执行档案](../../product/world-rules-core-gameplay/prd.md#代表性配方执行档案把分散规则收成一条可验收工业链) 与 SC-28；本合同把跨域档案转成玩家可执行、可复盘的单一路径。
- Authority 分工：产品层拥有档案形状与完成承诺，M4 拥有材料账本、批次、物流边、容量、损耗、buffer 与 terminal 事实，`world-runtime` 拥有状态、事件、receipt、持久化与 replay；gameplay 只拥有玩家顺序、取舍、收益、失败恢复与 progression 语义。
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history；本文件是玩法合同，不把目标规则或未来组合验证写成当前实现事实。

## 1. 目标与边界

产品 SC-28 已要求一份把工厂、配方、两类原料、物流、合法执行单位、主/副产物、终端用途与 progression 收成完整只读投影的代表性档案。Gameplay 需要再回答“玩家先看什么、比较什么、确认后何时获得什么、失败后如何恢复、为什么继续”，否则各条规则虽可独立验收，玩家仍无法走完一条工业链。

本合同只定义一个不绑定具体材料名称的 deterministic walkthrough。它不创建第二套 recipe、材料或物流权威；所有字段必须回指产品、Recipe/Factory、M4 与 `world-runtime` 的当前 authority。缺任一关键 authority 时，整条路径按 `incomplete/unknown/blocked` 处理，不得由 surface 以默认值补齐。

## 2. 代表性 fixture

验收 fixture 固定覆盖形状：一座具备目标 capability 的 factory、一个 `active` 或作用域内 `restricted` recipe version、两类独立 required inputs、至少一条有容量约束或损耗的有效 logistics edge、一个 canonical executable cycle/quantum、一个主产物与至少一个必须处置的副产物 branch，以及一个明确 purpose/recipient 的 terminal。固定的是覆盖形状，不冻结材料、配比、产率、价格、绝对容量、tick 或字段编码。

fixture 的档案必须同时读出 factory/recipe authority 与 fit、两类 input 的 provenance/适用性/join 结论、path/loss/capacity 与复查边界、合法 cycle 的 full/partial policy 与 power mode、output bundle/branch 的 owner/destination/policy、terminal admission 边界、当前 blocker、已占用/将消费价值、`next_action`、`next_recheck` 与 `progression_effect`。未选路径不取得 hold、吞吐顺位、input sink 或稳定进度。

## 3. 五阶段玩家 walkthrough

| 阶段 | 玩家动作与比较 | 即时收益、失败成本与下一动力 |
| --- | --- | --- |
| 1. `inspect_profile` | 查看 factory、recipe、两类输入、path、cycle、power、bundle、terminal 与目标关联；区分 `available`、`blocked`、`unknown` | 收益是理解首个可验证成果与真实成本；缺 authority 进入 `incomplete/unknown/blocked`，不能排程、扣料或领奖；下一步是补证、补料、修复前置或延期 |
| 2. `compare_plan` | 只比较 profile 真实支持的 `schedule_declared_cycle`、`prepare_or_source_missing_inputs`、`transfer_or_wait_for_capacity`、`resolve_output_destination` 与 `defer` | 每项展示预计合法产出、仍占用/将消费价值、物流损耗/容量、电力/时间风险、稳定与终端影响；下一步是确认一个计划或选择恢复/延期，不把推荐当承诺 |
| 3. `fresh_confirm` | 玩家明确确认所选 candidate/cycle；提交前以同一 authority revision fresh revalidate factory fit、两类 input、path/capacity、power、output/terminal 与权限 | 成功只创建一次可追溯承诺；报价后漂移只能 fresh requote 或无副作用 atomic reject，不得静默换 recipe、换来源、改路、降级或产生部分 sink；下一步是等待真实执行边界 |
| 4. `observe_settlement` | 读 `accepted/scheduled`、生产执行、`production_settled`、`terminal_pending` 与 `delivery/terminal_settled`，按 profile 选择等待、补前置、解决输出去向或延期 | `accepted/scheduled` 不推进完成；matching production receipt 只让 `production_only` 在声明边界完成首产物并标为 `produced/undelivered`，只有 canonical `W` 的稳定条件成立后才标为 `production-stable`；`terminal_pending` 不给 delivery/terminal 收益。缺输入、边容量、power、mandatory branch 或 terminal 时，保留或按 profile 单次处置 WIP/transit/buffer，并显示损失/占用与复查点 |
| 5. `choose_next_use` | 在已声明完成边界后比较继续稳定当前能力、结算 terminal、交易/本地服务或进入下一工业目标；未完成时只能走 profile 支持的恢复动作 | 完成收益是一次可归因的 production 或 matching delivery/terminal settlement 与一个新用途/下一目标方向；非 matching receipt、preview、pending 不发奖励。下一动力必须来自新用途、交付或恢复成功，而不是重复查看/重跑同一 preview |

`production-only` profile 只允许 matching production receipt 完成一次首产物并标为 `produced/undelivered`，不能减少 delivery demand 或发 terminal reward；稳定条件另行决定何时标为 `production-stable`，不得反过来延迟首产物完成。`terminal-admission` profile 必须等匹配的 delivery/terminal settlement。未声明 profile 或无法证明 boundary 时保持 blocked，不能从 bucket 名称推断完成。

<a id="single-constraint-decision-transfer"></a>
### 3.1 成功基线后的单约束决策理解样例

本配对样例承接产品 [2.2.5 与 FS-16](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#225-首局决策理解迁移)。它使用本合同已有的两类 required inputs 与准备缺料动作，不新增材料、数值或玩法机制；仅在当前 authority 和 fresh evidence 支持该情境时执行，否则保留 `target-contract` 或未覆盖结论。

| 配对阶段 | 已有事实与玩家决策 | 应观察的因果理解 |
| --- | --- | --- |
| 成功基线 | 声明 `production_only` profile，沿五阶段走完一个合法 cycle，取得 matching production receipt；成果保持 `produced/undelivered`，稳定与交付仍按各自条件判断 | 首产物已完成，尚未交付；这个 receipt 不证明稳定，也不能用于完成下一 cycle |
| 单约束变化 | 下一次同一 candidate 的 cycle 中，仅一类 required input 未齐套，另一类输入可用；factory/recipe、path/capacity、power、output 与权限保持可比并 fresh revalidate。只在缺料是唯一新增约束时采用此样例 | 当前 blocker 是下一 cycle 缺少该类输入，不是上一 cycle 生产失败；不能在未齐套时重复排程期待产出 |
| 下一安全动作 | 玩家依据现行 profile 比较 `prepare_or_source_missing_inputs` 与 `defer`；存在受支持且可达的补料路径时准备该输入，再复查齐套与其他前置；没有安全路径时延期或回到重新定目标 | 上一 cycle 已结算产物不被撤销；另一类输入的可用/占用状态、已消费或损失部分按账本解释，不承诺“全部资源都还在”。补料本身不等于排程/生产成功，下一次提交仍须 fresh confirm |

这是验收 fixture 的前后对照，不要求在玩家会话中故意扣走输入或强制失败。若下一 cycle 与基线无法隔离为单一变化、补料路径缺 authority，或必须换 recipe/来源等形成新 candidate，应按既有 continuation/cutover 合同处理并记录样例未覆盖，不强凑迁移结论。其他 completion profile 需另行取得其匹配完成基线，不能套用本样例的 production receipt。

记录应保留同一配对样例的入口、candidate/cycle/profile、前后 authority revision、matching receipt、唯一变化与未变化条件，以及玩家解释/选择/行动和对应事实依据。对“链路完成”“理解 blocker/保留价值/下一安全动作”“继续意愿”分别给出已观察、未观察或不支持的结论；另记正常引导、额外提示或代操作的内容、介入时点与前后表现，不能把照做提示当作独立理解。正常指导持续可用，不用隐藏恢复信息换取所谓无提示通过；继续意愿需可归因的选择或表达，不能由循环成功、停留时间或再次确认推断。

`test_tier_required` 可验证这对 fixture 的状态与后果：基线 receipt、单一缺料、未齐套无新 sink/产出、保留价值及安全动作条件。适用入口的组合验证沿 `test_tier_full` 要求执行；玩家理解与意愿按 testing 的 playability 层级另记样本性质和未覆盖范围。静态合同、确定性 fixture、模型/Agent 观察与真人证据各自保持原有 claim boundary，本节没有实际试玩或 current capability 的通过结论。

## 4. 失败恢复与状态守恒

- 两类 input 未齐套、批次不适用、物流 edge 满/失效、power 或 factory fit 漂移、mandatory output destination 失效、terminal owner/资格/容量失效时，首个不可逆 sink 前只能延期或 atomic reject；已存在的 WIP、in-transit、buffer-held、terminal-pending 或 settled branch 只能按 profile 支持的 hold、等待、改道、返工、return、reroute/handoff/reject、salvage 或终止各处置一次。
- `terminal_pending` 必须保留有限、可追溯的产物与占用，以及 recipient、destination、admission、capacity obligation 与 provenance，直到 matching settlement 或一次受支持处置；不得免费销毁、无限堆积、自动转卖、静默改道/改绑、提前或伪造交付，或用 production receipt 冒充 terminal settlement。任何恢复动作都要指出 primary root、已消费/仍占用/已损失价值和 `next_recheck`。
- 因果变化（factory capability、recipe version、required edge、output branch 或 terminal purpose）建立 parent-linked 新 candidate 并从 `W=0` 开始；仅补齐同一 candidate 的缺料、容量或电力前置才可保持 root continuation。本合同不重新定义既有换线、来源、批量、需求或 terminal 处置合同。

## 5. Current/target evidence cutline

当前 bounded evidence 仅能指向 `crates/oasis7_simulator/src/types.rs:630-641` 的 `BuildFactory`/`ScheduleRecipe` action 形状，以及 `actions_resolution.rs:361-447` 的建厂校验与通用成本处理；这不证明两输入 join、受限物流、output bundle、terminal settlement 或 progression walkthrough 已在 runtime、Viewer 或 Agent 组合实现。当前 gameplay surface 只能把本合同作为 target contract，不得宣称 current complete。

目标 evidence 需要一份 fresh composite runtime + QA fixture：同一 candidate 从档案读取、玩家选择、提交、执行、production/terminal receipt 到下一动作均可追溯，并能证明 profile/authority 缺失时 fail closed。Viewer、pure API 与 Agent 需从同一 snapshot/revision 得到等义档案与结果后，才能把该 walkthrough 标为 current-evidence-backed。

## 6. Exactly-once、replay 与跨 surface 验收

- 同一 root、candidate、cycle、profile 与 authority revision 至多产生一次承诺、input sink、production bundle、delivery/terminal settlement、progression 或 reward；重复确认、重连、Agent retry、snapshot restore、事件乱序与 replay 只能重读原处置。
- Preview、accepted/scheduled、production_settled、terminal_pending 与 delivery/terminal_settled 必须互斥表达其完成边界；不得把 production、delivery 与 terminal settlement 合并计数，也不得用旧 receipt 完成新目标。
- Viewer、pure API 与 Agent 对档案字段、阶段、primary blocker、机会成本、`next_action`、`next_recheck`、完成 profile 与 `progression_effect` 保持同义；任一缺 authority 时均返回 `incomplete/unknown/blocked`，不能自造安全候选。

## 7. Required / full acceptance

`test_tier_required` 至少覆盖：完整两输入 fixture 启动一个合法 cycle 并产生一个 output bundle；任一 input `unknown/not_applicable` 时无 sink/WIP；物流容量释放只重评未决边；power 或 factory fit 在提交前漂移时 requote/atomic reject；mandatory byproduct destination 失效时遵守 atomic/split policy；production-only 与 terminal-admission 分别只在各自边界完成；`terminal_pending` 不发 delivery/terminal reward，并在 owner/资格/容量失效时保留 recipient/destination/admission/capacity obligation/provenance、仅执行一次受支持处置；重复 submit、reconnect、Agent retry、restore 与 replay 不复制材料、产出、settlement、progression 或 reward；Viewer/pure API/Agent 对上述结果保持 parity。

`test_tier_full` 扩展到三阶段 `join -> stage -> transit -> buffer -> terminal`、多个 output branch、容量争用、WIP/transit/buffer 处置、因果 cutover 与持久化恢复，并证明旧 receipt、reservation、稳定窗口和奖励不会跨 candidate 迁移。

## 8. Non-goals 与 residual risk

本合同不新增 runtime/API/schema/action，不规定配比、产率、价格、物流/寻路、容量、队列、terminal 或 settlement 算法，不设计 UI 布局，不新增 recipe/catalog/lifecycle，也不改 starter completion、site selection、input reserve、source allocation、quality/custody、substitution、batch、demand、byproduct、changeover 或 pipeline checkpoint 合同。

残余风险是当前各 authority 可能只有局部实现证据，导致 walkthrough 暂时只能标为 target/unknown；未来 recipe、receipt 或 terminal profile 变更时必须重新生成同一 fixture 的 composite evidence，避免玩家看到可选动作却无法获得一致结算与下一动力。
