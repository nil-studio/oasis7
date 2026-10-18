# oasis7 Simulator：场景文件化（设计文档）设计

- 对应需求文档: `doc/world-simulator/scenario/scenario-files.prd.md`
- 当前任务状态与历史变更：GitHub PR、实际验证记录与 Git history；Issue 按需。

## 1. 设计定位
定义场景文件化设计，把内置 `WorldScenario` 迁移为 JSON 场景文件并作为单一权威来源。

## 2. 设计结构
- 文件模型层：用场景 JSON 描述 seed、地点生成与可选碎片配置。
- 加载接线层：内置 `include_str!` 修改需重编译；demo `--scenario-file` 在初始化时读取给定 JSON。两者共用解析与校验，不提供生产 runtime 热更新。
- 生成表达层：用 `location_generator`、origin 等字段驱动初始化。
- 有效配置层：初始化时复制 `WorldConfig.asteroid_fragment` 后仅应用 JSON 显式 override；不修改全局默认值，也不提供运行期 hot configuration。
- 测试矩阵层：维护场景到测试目标的映射与稳定性校验。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [加载与 demo 例外](scenario-files.prd.md#scenario-loading-contract) | include_str 重编译与 demo 显式文件初始化；同解析与校验 | [设计约束](#scenario-loading-design) | 对应模块维护者与 QA | 文档补齐不宣称实现或真实运行完成 |
| [消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵) | Simulator/named runtime-live/generated-world 的模型保留与默认 Agent 分开 | [设计约束](#scenario-loading-design) | 对应模块维护者与 QA | 文档补齐不宣称实现或真实运行完成 |
| [generated-world 来源](scenario-files.prd.md#generated-world-provenance-contract) | 区分当前文件存在检查与未来内容绑定、固定输入重建验证 | [来源校验边界](#generated-world-provenance-design) | Simulator 工件生成器、runtime-live 消费者与 QA | 不把现有加载成功当成已验证来源；本次不改加载器或持久格式 |
| [固定场景重现产品要求](../../product/agents-world-simulation/prd.md#sc-agent-001)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | 固定场景重现中的相同有效 seed/config/触发顺序初始化及 generated-world 来源重建切片 | [场景约束](#scenario-loading-design) | simulator/runtime 维护有效输入；Viewer 与 QA 核对权威事件及交互结果 | 不覆盖整个事件执行/replay、正式 surface 交互结果或真实 provider 确定性；产品根仍为部分覆盖 |
| [固定场景重现产品要求](../../product/agents-world-simulation/prd.md#sc-agent-001)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | 固定场景重现中的相同有效 seed/config/触发顺序初始化及 generated-world 来源重建切片 | [来源与重建](#generated-world-provenance-design) | simulator/runtime 维护有效输入；Viewer 与 QA 核对权威事件及交互结果 | 不覆盖整个事件执行/replay、正式 surface 交互结果或真实 provider 确定性；产品根仍为部分覆盖 |

## 3. 关键接口 / 入口
- `crates/oasis7_simulator/scenarios/*.json`
- `WorldInitConfig::from_scenario`
- `WorldScenario::parse`
- `oasis7_init_demo --scenario-file`

<a id="scenario-loading-design"></a>
## 4. 约束与边界
- 内置场景文件需保持单一来源。
- 普通地点按相同有效 seed、空间和生成配置可复现；chunk/fragment 另外固定 effective config 与 trigger order。
- 消费者的模型保留与默认 Agent 语义以[初始化入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)为准；纯碎片场景当前使用 Simulator 离线入口验收，不推荐普通 named runtime-live Web 回归作为零 Agent 模型证明。
- `asteroid_fragment` 是唯一规范字段名；`min_fragment_spacing_cm` 缺省继承 `50_000` cm，`<= 0` 关闭额外 spacing。effective config 必须进入固定 seed/触发序列的确定性上下文，replay 只应用 committed delta。
- 只有显式 `power_bootstrap` 数据可注入 `power_plants`；其他内置和 `asteroid_fragment*` 场景无隐式设施。该注入不改变 runtime 电力规则、storage/动作 ABI、链上经济或治理边界。
- 不在本专题扩展复杂 DSL 与版本迁移工具。

<a id="generated-world-provenance-design"></a>
### 4.1 generated-world 的当前校验与目标绑定

当前 runtime-live 在检查 provenance 文件存在后直接加载 `generated-scenario-world`，没有将 provenance 内容交给 `WorldKernel::load_from_dir` 验证。[生成器](../../../crates/oasis7_ops_tools/src/bin/oasis7_generate_world_artifact.rs) 写出的 `artifact_kind`、`scenario_id`、`seed`、`config`、`public_manifest_sha256` 和 `public_manifest_entry_count` 中，`config` 是场景 spec，后两项只绑定 public manifest；该格式没有 sidecar 内容摘要。不得据此把存在检查写成完整来源验证。

目标验证依次检查 provenance 的结构与类型、实际 manifest 的摘要与数量、有效场景输入及快照 seed/config/model 的一致性，最后以生成端记录的内容摘要绑定实际消费的 sidecar checkpoint/journal。绑定记录必须由生成路径对实际写出的内容形成，消费者对实际载入的内容重算比较；只在加载后给未知来源文件计算一个摘要不能反向证明它与 provenance 同次生成。现有六字段缺少这项关联，补齐生成/消费实现及兼容编码后才能宣称目标成立，本设计不虚构当前可读的新字段。

固定生成版本、全部有效输入与触发顺序的重建比较另行证明可复现；它与加载成功、来源内容一致是不同结论。缺字段/不支持的旧工件只能保留来源未验证的结论，解析失败或内容不匹配需给出具体原因。当前调试入口的存在检查行为不因本次文档变更而被描述为已经拒绝这些输入。

## 5. 设计演进计划
- 先迁移内置场景为 JSON。
- 再接入加载逻辑与调试入口。
- 最后维护场景测试覆盖矩阵。


## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [加载与 demo 例外](scenario-files.prd.md#scenario-loading-contract) | [设计约束](#scenario-loading-design) | include_str 重编译与 demo 显式文件初始化；同解析与校验 | 候选版本的[验证入口](../../../crates/oasis7_ops_tools/tests/oasis7_init_demo.rs)：核对自定义文件与内置场景入口、不可读/解析失败，不宣称热更新；静态核对与实际执行分别记录 | 实施 PR 的候选、命令、fixture 与真实结果 | 不代签未执行回归、真实模型、浏览器、节点或发布验收 |
| [消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵) | [设计约束](#scenario-loading-design) | Simulator/named runtime-live/generated-world 的模型保留与默认 Agent 分开 | 候选版本的[验证入口](../../../crates/oasis7_simulator/src/tests/init.rs)：核对纯碎片四 chunks、无 origin/Agent；Web 保留入口未实现则不宣称通过；静态核对与实际执行分别记录 | 实施 PR 的候选、命令、fixture 与真实结果 | 不代签未执行回归、真实模型、浏览器、节点或发布验收 |
| [generated-world 来源](scenario-files.prd.md#generated-world-provenance-contract) | [来源校验边界](#generated-world-provenance-design) | 来源存在、内容绑定与固定输入重建分别验证 | [snapshot_generated_world.rs](../../../crates/oasis7/src/viewer/runtime_live/tests/snapshot_generated_world.rs) 当前 fixture 的场景 seed 为 9、provenance seed 为 42，仅验证映射；实现候选需另补一致产物、非法/缺失 provenance、seed/config/sidecar/manifest 不匹配，以及固定生成输入重建的离线回归 | 实施 PR 的候选、实际产物摘要、输入与真实验证结果 | 现有 fixture 不证明来源校验，本次未执行绑定或重建回归 |
| [固定场景重现产品要求](../../product/agents-world-simulation/prd.md#sc-agent-001)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | [场景约束](#scenario-loading-design) | 固定完整有效输入与触发顺序，区分初始化重现、来源绑定和整段交互结果 | 复用[初始化回归定义](../../../crates/oasis7_simulator/src/tests/init.rs)，只作为相关测试源，不视为本SC通过；实施候选另记录固定输入重建及错误 provenance/seed/config/sidecar/manifest 负例，整段事件与正式交互由各 consumer 另核 | 实施 PR 的候选、有效输入、生成产物及实际对比结果 | 未运行；加载或局部 fixture 不代签根 SC-1 全部事件及玩家交互结果 |
| [固定场景重现产品要求](../../product/agents-world-simulation/prd.md#sc-agent-001)；Trace refs: O7-REQ-3cf9f857-8083-4527-9648-95e9425a8448 | [来源与重建](#generated-world-provenance-design) | 固定完整有效输入与触发顺序，区分初始化重现、来源绑定和整段交互结果 | 复用[generated-world 回归定义](../../../crates/oasis7/src/viewer/runtime_live/tests/snapshot_generated_world.rs)，只作为相关测试源，不视为本SC通过；实施候选另记录固定输入重建及错误 provenance/seed/config/sidecar/manifest 负例，整段事件与正式交互由各 consumer 另核 | 实施 PR 的候选、有效输入、生成产物及实际对比结果 | 未运行；加载或局部 fixture 不代签根 SC-1 全部事件及玩家交互结果 |

## 12. 风险与未决边界

来源文件存在不证明内容绑定；当前六字段 provenance 不携带 sidecar 内容摘要。固定输入初始化重现也不证明后续权威事件与正式玩家交互一致。生成/消费格式兼容、来源负例及全链路重现仍需实现候选与真实验证；本文只补既有场景合同到产品 SC-1 的部分承接，不增加当前运行能力。
