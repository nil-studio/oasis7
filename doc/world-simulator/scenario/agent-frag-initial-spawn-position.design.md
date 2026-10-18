# oasis7 Simulator：Agent Frag 初始站位优化（设计文档）设计

- 对应需求文档: `doc/world-simulator/scenario/agent-frag-initial-spawn-position.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

## 1. 设计定位
定义 Agent Frag 初始站位优化设计，统一新生 Agent 在碎片场景中的出生位置选择、避让规则与可重复性。

## 文档适用范围

本文继续作为配对 PRD 的专业出生/初始化设计；本轮仅补身份、接收关系与验证定位，不作生命周期迁移或发布判断。初始化选择/位置由 runtime_engineer 维护，玩家理由由 gameplay_designer / viewer_engineer 承接并由 qa_engineer 独立取证；corpus 启发式 routing 不转移产品语义主责。历史 task/Git evidence 继续拥有实施状态。

## 2. 设计结构
- 出生采样层：根据场景与碎片布局生成候选站位。
- 避让约束层：避免与障碍、边界或已有实体发生冲突。
- 首局理由层：把推荐 frag 的材质预期、距离优势和第一工业目标关联转成玩家可读提示。
- 落位确认层：在运行时初始化阶段确定最终出生点。
- 回归验证层：校验不同 seed 与碎片布局下的站位稳定性。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-4d98352e-f146-4672-b159-b99545708c78；[确定性选择](agent-frag-initial-spawn-position.prd.md#fsp-selection-contract) | `build_world_model` 有 frag 候选优先，否则沿既有非 frag 策略；同输入可复现 | [选择](#fsp-selection-design) | runtime_engineer；世界初始化与场景输入 | 产品适用性 N/A；reason=本行仅为离线初始化候选选择算法；scope=build_world_model 候选排序与 seed 选取，不接收玩家认领、资源 provenance 或统一 seed 方案；相邻玩家理由另行接收 |
| Trace refs: O7-REQ-3bef4fa6-57cd-41f6-baa3-74272ea8adcc；[standoff 与边界](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract) | 数据初始化上方偏移与确定性边界降级；Viewer 消费另行接收 | [站位](#fsp-standoff-design) | runtime_engineer / viewer_engineer；整数厘米与 SpaceConfig | 产品适用性 N/A；reason=本行只核初始化整数坐标与边界算法；scope=fragment_spawn_pos 的上方坐标和既有降级，不含 Viewer 消费保留或玩家可读结果；后者单列接收 |
| Trace refs: O7-REQ-f6aebb85-7a07-4e0b-a3cf-f23655f293a2；[FS-7 条件子义务](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#fs7-starter-resource-reason) | 仅推荐首个 frag 时解释粗粒度材质价值、当前可达性与第一工业目标关联，不承诺精确掉落 | [理由](#fsp-reason-design) | gameplay_designer / viewer_engineer / qa_engineer；首局 profile 与资源分布策略 | 不接收整个 FS-7、保证产出或强制采矿；不以距离优化代签理解 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[舞台相关对象可读](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | 在本专题适用的 Viewer 消费中保留已初始化 standoff，不强制贴回表面；为相关 Agent/frag 可辨认提供有限设计依赖 | [Viewer 保留](#fsp-viewer-preservation-design) | viewer_engineer / qa_engineer；专业出生 PRD §2 原约束 | 不从产品要求反推所有 surface 固定坐标；不保证任意相机无遮挡或整个 stage001/AC；产品状态不变 |

## 3. 关键接口 / 入口
- 初始站位生成入口
- 碎片/边界约束读取
- starter frag reason: `target_frag_id`、`expected_material_hint`、`starter_value_reason`、`distance_or_accessibility_reason`、`first_recipe_relevance`
- Agent 初始化落位
- 站位回归用例

## 4. 约束与边界
- 同一 seed 下站位应可复现。
- 出生点选择不得破坏既有场景边界。
- 推荐 frag 的玩家提示不得承诺精确掉落，只能给粗粒度材质预期和第一工业目标关联。
- 不在本专题扩展完整导航系统。

<a id="fsp-selection-design"></a>
### 4.1 确定性选择的专业边界

[配对 PRD 选择合同](agent-frag-initial-spawn-position.prd.md#fsp-selection-contract)保持不变。当前 [build_world_model](../../../crates/oasis7_simulator/src/init.rs)按前缀收集并排序候选，再按 seed 与 Agent 序号选择；源码定义不是测试通过证明。它不等于推荐最近或经济最优 frag，也不证明 chain-committed provenance。统一 seed 专题的四项 [Open Decisions](unified-world-seed-fragment-runtime.design.md#9-open-decisions)保持未决，不由局部初始化作选择。

<a id="fsp-standoff-design"></a>
### 4.2 坐标、边界与消费保留

[原 standoff 合同](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract)继续拥有数值与降级。当前 [fragment_spawn_pos](../../../crates/oasis7/src/simulator/frag_spawn.rs)为 5000cm、2000cm、frag 中心的确定性路径；该源码说明不新增产品数值或改写原需求。初始化错误由原构建入口拒绝，不能交付部分有效模型。该函数的边界 contains 检查不能证明一般障碍/实体避让已经实现；原避让设计义务与实现证据分开。Viewer 保留偏移须独立验证，不从初始化断言推断所有投影视角可读。

<a id="fsp-reason-design"></a>
### 4.3 推荐理由的条件接收

[专业提示合同](agent-frag-initial-spawn-position.prd.md#fsp-starter-reason-contract)与 DEC-FSP-001 原五字段保持不变。推荐 frag 时解释粗粒度材质预期、为什么适合第一步、当前距离/可达性理由与第一工业目标关联；缺材质预期或目标关联按原 `starter_frag_hint_missing` 表达，引导缺失不能改变出生或采矿。当前首局按所选 recipe/profile 的真实依赖准备输入，不额外强制来源采矿；理由只表达可能支持的方向，不是掉落或生产保证。该局部设计不改变产品状态或宣称实际入口已支持。

<a id="fsp-viewer-preservation-design"></a>
### 4.4 Viewer 保留初始化位置的有限消费关系

原出生 PRD §1 要求减少 Agent 与 frag 重叠遮挡、提高开局可见性，§2 已要求 Viewer 保留初始化 standoff、不强制贴回 frag 表面。[产品舞台](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001)要求相关 Agent/地点/路线在正式 surface 可读；其 §2 原文要求当前相关对象及选中/聚焦状态可辨认。这里仅将既有位置保留约束作为这一读取结果的有限设计依赖，不把产品语义变成所有 surface 固定坐标规则，也不宣称位置保留足以满足完整 AC。产品的原阶段、来源/权限和现有状态全部保持。

验证计划为 planned：Viewer/QA 在固定候选、原声明支持的 surface 对账初始化输入与渲染位置，确认没有强制贴回；分别记录相机/缩放条件下相关对象可辨认的观察。当前尚未核实直接覆盖该消费路径的测试源码或执行记录，不借 init 断言代签。多 surface、完整目的/blocker/下一步、观察权限和正式入口组合仍由原产品与 Viewer authority 独立验证。

## 5. 设计演进计划
- 先固化站位选择规则。
- 再补避让与边界守卫。
- 最后沉淀场景回归。

<a id="fsp-verification"></a>
## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-4d98352e-f146-4672-b159-b99545708c78；[选择合同](agent-frag-initial-spawn-position.prd.md#fsp-selection-contract) | [选择](#fsp-selection-design) | 有 frag 优先与同输入确定性；无 frag 保留原策略 | test_tier_required；已读源码断言 [init_agent_frag_spawn.rs](../../../crates/oasis7_simulator/src/tests/init_agent_frag_spawn.rs) 的 `agents_prefer_fragment_spawn_locations_when_fragments_exist` 仅检查 frag 前缀；planned 同候选重复构建、无 frag 与缺地点负例，固定 seed/config 的 Simulator 离线层；本轮未运行 | 实施 PR 记录候选、输入、命令与独立输出 | 已有断言不覆盖完整确定性矩阵、正式 claim 或 seed-chain 接线 |
| Trace refs: O7-REQ-3bef4fa6-57cd-41f6-baa3-74272ea8adcc；[站位合同](agent-frag-initial-spawn-position.prd.md#fsp-standoff-contract) | [站位](#fsp-standoff-design) | 上方偏移与边界两级降级；Viewer 消费另列 | test_tier_required；已读 [fragment_spawn_positions_stand_off_above_surface](../../../crates/oasis7_simulator/src/tests/init_agent_frag_spawn.rs) 检查 x/y 相同、z 更高、间距 2000..5100cm 与边界内；planned 固定50m/20m/中心输入；本轮未执行或新增断言 | runtime 实施 PR 的离线矩阵，候选/config/环境分别标明 | 区间断言不证明精确5000或两个fallback分支；不证明任意相机无遮挡、避让或真实体验 |
| Trace refs: O7-REQ-f6aebb85-7a07-4e0b-a3cf-f23655f293a2；[FS-7 条件子义务](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#fs7-starter-resource-reason) | [理由](#fsp-reason-design) | 推荐首个 frag 时五字段与缺失提示、不承诺掉落/产出 | planned；test_tier_required 沿 [正式 surface 验证手册](../../../testing-manual.md) 的 L4A synthetic 内部闭环层 准备正常、缺材质、缺目标关联与不同 seed/布局样例；固定候选和适用正式 surface，核对粗粒度价值、距离理由和配方方向；source/fixture 尚待实施，不伪造已有测试 | gameplay/Viewer/QA 实施 PR 的合同结果与独立玩家阅读观察，记录候选、surface、输入和真实输出 | 本文/初始化测试不证明理由已展示、独立理解、全部 FS-7、完整首局或真实留存 |
| Trace refs: O7-REQ-24cbdd98-cf69-4f7b-8342-6216b1b91616；[舞台相关对象可读](../../product/agents-world-simulation/player-readable-world-stage.prd.md#req-agent-stage-001) | [Viewer 保留](#fsp-viewer-preservation-design) | 仅保留初始化 standoff 的消费约束，局部服务相关对象可辨认 | planned；沿[正式 surface 验证手册](../../../testing-manual.md) 的 L4A synthetic 内部闭环层的层级要求及 §4.4 计划在固定候选、原支持的 Viewer surface 对账输入位置与渲染位置，检查未被强制贴回表面，并独立观察相关 Agent/frag 是否可辨认；真实 source/fixture 尚待核实，未执行 | Viewer/QA 实施 PR 记录 surface、相机、输入、输出及真实观察；合同与可读性分别给结论 | 不把离线初始化断言、截图或坐标一致代签完整 stage001/AC、所有相机/所有 surface 或产品 readiness |

本轮是文档与静态断言核对，所有行为执行结果均未产生。原演进计划、历史实施状态与统一 seed 未决方案不因身份登记改变；未来实现改变坐标、选择、配置、权限或 fallback 时须回到各专业合同另行审读。

## 12. 风险与未决边界

统一 seed 四项未决、正式 claim 接线与现有避让实现覆盖保持原状态。站位与理由的源码存在或文档身份完整不能替代同候选的行为、入口与理解证据；本轮不新增数值、默认 fallback、权限、导航或产品承诺。
