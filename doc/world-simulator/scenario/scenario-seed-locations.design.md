# oasis7 Simulator：场景种子化地点生成（设计文档）设计

- 对应需求文档: `doc/world-simulator/scenario/scenario-seed-locations.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。
- 审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`（2026-10-15）；本次核对文档、源码与断言定义，未执行 Rust、真实玩家入口或节点恢复。
- 专业职责：`runtime_engineer` 维护 Simulator 初始化/场景合同；消费者接线与 QA 证据分别审读，本文不分配新的产品主责。

<a id="1-设计定位"></a>
## 1. 问题、目标与设计定位
定义场景种子化地点生成设计，让地点数量、命名和布局由 seed 驱动并保持稳定可复现。

## 2. 设计结构
- seed 驱动层：先确定场景级有效 seed；地点数量来自 `location_generator.count`，位置由有效 seed 与序号派生。
- 地点生成层：生成地点 ID、名称与位置表达。
- 场景接线层：把 seed 逻辑纳入场景文件与初始化。
- 回归校验层：验证不同 seed 下地点生成可复现且有区分度。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [普通地点确定性](scenario-seed-locations.prd.md#location-determinism) | 普通地点与 chunk trigger order 承诺分开 | [设计约束](#seed-input-design) | 场景转换与 runner 维护者、Simulator 初始化维护者及 QA | 产品适用性：N/A；理由=本行分配普通地点生成函数的输入相等性，不声明玩家世界确定性提交；范围=location_generator 与 Simulator 出生候选输入 |
| [有效 seed](scenario-seed-locations.prd.md#location-effective-seed) | override 在生成前生效，显式坐标不重生成 | [设计约束](#seed-input-design) | 场景转换与 runner 维护者、Simulator 初始化维护者及 QA | 产品适用性：N/A；理由=本行分配配置转换与显式坐标保留规则，不授予正式世界 seed 重置权；范围=WorldScenarioSpec 到 WorldInitConfig 及离线 runner 接线 |

## 3. 关键接口 / 入口

[场景源码](../../../crates/oasis7_simulator/src/scenario.rs)中 `WorldScenario::load_spec` 读取内置 JSON，`WorldScenarioSpec::into_init_config` 使用 `self.seed` 与 `config.space` 调用 `ScenarioLocationGeneratorConfig::generate_locations`，再把同一 seed 写入 init。文件加载路径返回 `ScenarioSpecError`，不把解析失败改成另一个随机场景。

- `count` 决定数量；ID 是 `id_prefix + index`，名称是 `name_prefix + 空格 + index`，profile/resources 采用默认值。相同 seed 并不使不同 count、前缀或空间成为等价输入。
- 坐标取 seed、index 与固定混合常量的 `splitmix64` 派生，分别映射到 width/depth/height，并 round/clamp 为整数厘米；`coord_from_hash` 对非正维度返回 0，常规 build_init 路径先 sanitize config。测试须记录 effective config，不能把 sanitization 前输入冒充实际空间。
- [初始化源码](../../../crates/oasis7_simulator/src/init.rs)对出生候选排序，优先 `frag-` 集合，无 fragment 则普通地点；同 seed 还需同候选集合、Agent start_index/count 与配置。允许多个 Agent 选择同地点，无地点且 count>0 返回 `SpawnLocationMissing`。本专题不改变 frag 几何偏移和玩家推荐理由。

<a id="seed-input-design"></a>
## 4. 约束与边界
- 普通地点仅在有效 seed、空间与生成配置相同时保持一致；Agent 出生另依赖相同可用地点集合与枚举顺序。
- 内置场景的 seed override 在 `into_init_config` 生成地点之前应用；显式 `init.locations` 保留调用者坐标，不随 seed 重生成，配置 sanitization 仍适用。
- chunk/fragment 不继承无条件顺序无关性：其 world seed、effective config、trigger order 与历史 committed delta 规则以[专业合同](chunked-fragment-generation.prd.md)为准。
- 地点命名与布局需保持可读性。
- 不在本专题引入复杂随机分布 DSL。

<a id="5-设计演进计划"></a>
## 5. 设计演进计划与风险
- 先固化 seed 语义。
- 再接地点生成配置。
- 最后维护稳定性回归。
- 离线用例分别验证 override 前置、模板默认、显式坐标、Agent/fragment 派生和 chunk 触发顺序边界；不把文档补齐作为 runner 接线已修证据。


## 6. 有效 seed 流程与当前差距

目标流程是先确定 template seed 或 override，在 spec 上生效，再执行地点生成和 init 转换；由 init.seed 派生 Agent 与 fragment（fragment 保留 offset）。显式 `WorldInitConfig.locations` 已经是调用者数据，override 只能改变适用随机根，不能重新解释或移动这些坐标。

固定审读基线的 [scenario_test_runner::run_loaded_scenario](../../../tools/scenario_test_runner/src/lib.rs)仍先 `WorldInitConfig::from_scenario`，再赋值 `init.seed`；这会留下模板 seed 的普通地点，而 Agent/fragment 使用 override。上游所述 override 前置是未完成的接线合同，本次文档不修代码，也不把已有测试视作覆盖该缺口。无 override 路径与显式 init 路径须独立保留，不能为修 named scenario 重写调用者地点。

## 7. 状态、失败、恢复与兼容边界

生成阶段输出配置，模型构造成功才交付模型；它不是链上 commit，也没有网络请求的 accepted/committed 状态。地点 ID 冲突、位置越界或缺出生点在初始化层明确失败，不应随机重抽掩盖错误。修正输入后重试须记录 effective seed/config 与原始错误。

显式坐标和生成配置应与 snapshot 分开：历史恢复使用原 snapshot 与合同声明的 committed delta，不用今天的默认 seed/config 重新生成剩余资源。更改 seed 是新初始化输入，不能据此清空持久世界、重发库存或自动迁移既有 Agent。场景/environment 参数不是多个玩家世界的产品入口。

旧 JSON locations 到 generator 的不兼容选择沿用原 PRD；本文不新建兼容迁移器、热更新或随机 DSL。地点数量影响分配和装配规模，未测量大规模生成耗时与内存；浮点映射的跨平台逐位一致性也未据此证明。

产品关系边界：上表 N/A 只说明专业配置算法与转换规则没有单独玩家功能 AC；不把 Agent 出生、正式世界资源和入口消费整体排除。现有产品中的确定性提交/恢复要求涉及权威输入与 commit 证明，不能由普通地点相等代签；本轮未核实这些消费方的准确产品叶子承接，继续保留[出生专题](agent-frag-initial-spawn-position.prd.md#4-technical-specifications)、[分块合同](chunked-fragment-generation.prd.md)和[消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)的原义务，由其 owner 另行对齐，不升级产品卡。

## 8. 决策与风险

保留生成前 override、显式坐标不变、普通地点与 chunk trigger order 分离三个选择。反例是先生成再改 seed，或用重新生成“恢复”历史 remaining；两者都不能因输出看似可用而通过。runner 接线修复由其维护者与 QA 在独立实现中验证；当生成算法、浮点映射、候选排序或场景 schema 变化时重新核对确定性边界与旧 fixture，不用文档版本替代运行证据。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [普通地点确定性](scenario-seed-locations.prd.md#location-determinism) | [设计约束](#seed-input-design) | 普通地点与 chunk trigger order 承诺分开 | 候选版本的[验证入口](../../../crates/oasis7_simulator/src/tests/init.rs)：`init_is_deterministic_with_seed` 对比整模型/report（其中 agents.count=0，不证明出生）；`init_requires_spawn_location` 覆盖空候选拒绝。补普通 generator 同 seed/config 相等、不同 seed 区分及固定候选的 Agent 出生对比；`init_position_contract.rs` 整数坐标测试检查显式坐标/fragment。Simulator 离线层，记录 candidate、effective config/seed、排序后候选及输出；不声称跨 chunk 顺序无关 | 实施 PR 的候选、命令、fixture 与真实结果 | 不代签未执行回归、真实模型、浏览器、节点或发布验收 |
| [有效 seed](scenario-seed-locations.prd.md#location-effective-seed) | [设计约束](#seed-input-design) | override 在生成前生效，显式坐标不重生成 | 候选版本的[验证入口](../../../crates/oasis7_simulator/src/tests/init.rs)：以`tools/scenario_test_runner/src/lib.rs` 内嵌测试的 `explicit_init_passes_without_requiring_seed_override` 为既有显式输入入口；新增 named scenario 的 override 前置、无 override 等于模板、显式坐标不变、Agent/fragment 同根四组定向断言，直接检查生成后的地点/config/model。runner helper 回归与接线修复仍待实现；固定候选离线执行，保存 seed 与模型差异，不以解析成功作为通过 | 实施 PR 的候选、命令、fixture 与真实结果 | 不代签未执行回归、真实模型、浏览器、节点或发布验收 |
