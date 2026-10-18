# oasis7 Simulator：世界初始化（设计分册）设计

- 对应需求文档: `doc/world-simulator/scenario/world-initialization.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。
- 审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`（2026-10-15）；本次核对文档、源码与断言定义，未执行 Rust、真实玩家入口或节点恢复。
- 专业职责：`runtime_engineer` 维护 Simulator 初始化/场景合同；消费者接线与 QA 证据分别审读，本文不分配新的产品主责。

<a id="1-设计定位"></a>
## 1. 问题、目标与设计定位
定义世界初始化设计分册，统一场景、默认实体、设施与启动状态在初始化阶段的装配顺序。

## 2. 设计结构
- 初始化装配层：按场景与配置生成世界初始模型。
- 显式装配层：遵循 origin.enabled、agents.count 和场景资源配置；设施只遍历 `power_plants`，没有按区域/碎片场景名称补设施的默认分支。
- 配置覆盖层：允许场景与启动参数对初始化细节做有限覆盖。
- 验证回归层：通过 init 测试和 demo 校验世界初始化稳定。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [显式设施](world-initialization.prd.md#init-explicit-facilities) | 默认区域/碎片场景设施为 0，power_bootstrap 显式正例，单一矩阵 | [设计约束](#init-state-design) | Simulator 初始化维护者、场景消费入口维护者与 QA | 产品适用性：N/A；理由=本行只分配专业初始化配置的设施声明校验，不定义玩家设施获得条件；范围=Simulator 模型装配与离线场景矩阵 |
| [时间与初始事件](world-initialization.prd.md#init-time-events) | 时间 0 可有 Init 事件，snapshot 与 journal 覆盖范围分开 | [设计约束](#init-state-design) | Simulator 初始化维护者、场景消费入口维护者与 QA | 产品适用性：N/A；理由=本行只分配时间零模型与有限 Init 事件的技术覆盖，不承担正式世界恢复信任链；范围=initialize_kernel 与本地 snapshot/journal 配对 |

## 3. 关键接口 / 入口

[初始化源码](../../../crates/oasis7_simulator/src/init.rs)的 `build_world_model(&WorldConfig, &WorldInitConfig)` 返回 `Result<(WorldModel, WorldInitReport), WorldInitError>`；`initialize_kernel` 在模型成功构建后包装 `WorldKernel`。场景文件经 [WorldScenarioSpec::into_init_config](../../../crates/oasis7_simulator/src/scenario.rs)进入同一配置层。

- 配置和场景调用方提供 seed、origin、locations、fragment、agents、power_plants；模型构造方负责位置、ID、资源和设施引用校验。配置中的 owner 引用校验不是玩家授权，也不能凭配置在正式世界授予资产。
- 当前源码先 sanitize world/init config，创建 chunk 索引；依次装配 origin、自定义地点、seed-position/bootstrap chunks、Agent，再按 Agent 位置确保 chunk，最后处理显式设施及 module visual entities。不能把简化的装配顺序当作省略后两步的许可。
- fragment 使用 `seed.wrapping_add(seed_offset)`；Agent 先选择 `frag-` 候选，否则普通地点，排序后按 seed 与 Agent 序号选择。具体 frag standoff 仍由[出生专题](agent-frag-initial-spawn-position.prd.md#4-technical-specifications)拥有，不在本文重新定义。
- `WorldInitReport.locations` 当前只统计非 `frag-` 地点；验收完整地点数、fragment 与预算必须读模型，不能以 report 数量代替。location 资源装配会移除初始 electricity；Agent 资源和显式 power plant 按对应配置处理，库存与发电设施不是同一对象。

<a id="init-state-design"></a>
## 4. 约束与边界
- 初始化顺序需保持确定性。
- 场景能力只在[主矩阵](scenario-files.prd.md#场景测试覆盖矩阵2026-02-06)维护；`twin_region_bootstrap` 与 `asteroid_fragment_bootstrap` 默认设施数为 0，设施正例使用 `power_bootstrap`。
- `initialize_kernel` 保持时间 0；enabled fragment 的已生成 chunk 可追加 `ChunkGenerated(Init)`，不清空合法事件。snapshot 承载全部初始状态，journal 只按[分块生成合同](chunked-fragment-generation.prd.md)承载已声明部分。
- 初始化产物交给不同消费者时按[入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)验收；named runtime-live 的默认 Agent 与 seed model 投影不能冒充 Simulator 原模型保留。
- 场景覆盖不得绕过核心一致性校验。
- 不在本专题扩展运行时动态重建世界。

<a id="5-设计演进计划"></a>
## 5. 设计演进计划与风险
- 先收敛初始化装配顺序。
- 再对齐场景覆盖和默认注入。
- 最后固化 init 回归。
- 验收读取模型/快照中的设施、Agent、origin、chunk 与 Init 事件，不只检查解析成功；使用已有 init/persist 离线回归，独立记录实际执行范围。


## 6. 状态、失败与恢复边界

构建函数在局部模型内装配，校验失败返回 `WorldInitError`，不向调用者交付可运行的部分模型。缺出生地点、重复 ID、越界坐标、负资源、设施不存在的地点/owner 或非法参数应明确拒绝；修正输入后可重新构建。同输入复现不是向已经运行的 kernel 重放初始化的幂等许可。

初始化成功只证明时间零模型可构建。`initialize_kernel` 追加已生成 chunk 的 `ChunkGenerated(Init)`，snapshot 承载完整状态；恢复须保留配套 snapshot/journal 的长度与版本一致性。不能清空合法 Init 事件，也不能只用 chunk 事件重建全部 origin、Agent 或设施。正式链的 commit、历史版本、world identity 与验证根不由这个局部成功结果证明。

## 7. 消费、安全与兼容约束

本专题不新增网络端点、凭据或玩家权限。`WorldInitConfig` 是初始化输入，不是正式世界写权限。场景、local test 与 runtime-live 是技术消费环境，不能据此承诺多个玩家持久世界；正式世界仍须守住单一持久历史和资源 authority。既有[消费入口矩阵](scenario-files.prd.md#场景初始化与消费入口矩阵)中的原模型保留/投影差异继续适用。

产品关系边界：上表的 N/A 仅针对专业配置与时间零表示义务，不能解释为整体初始化没有产品影响。本轮未建立玩家出生、正式资源来源、跨入口一致性与持久世界恢复的准确产品叶子承接；这些关系仍未核实，由相关产品与 runtime/chain/Viewer owner 按真实消费链补齐，不借通用“确定性”或离线恢复断言冒充完成。本文不改变任何产品状态卡。

场景 ID、整数厘米坐标和现有配置格式保持不变。本轮仅补文档，不迁移旧 snapshot、不重置 seed、不重发资源；不能将恢复失败降级为重新初始化世界。配置规模由显式地点、Agent、bootstrap chunks 与 fragment 预算决定，未测量启动耗时/内存或跨平台确定性，不新增性能数字承诺。

## 8. 当前能力与未决风险

- 静态源码与已有断言能定位显式设施、错误拒绝、fragment seed 与 Init 事件；它们是验证入口，不是本候选通过证明。
- `twin_region_bootstrap`、`asteroid_fragment_bootstrap` 的零设施断言与 `power_bootstrap` 正例已存在；所有消费者是否保留这些对象须按入口矩阵另验。
- 时间零和关闭 fragment 的组合、模型完整数目及恢复负例需同候选运行；若 schema、装配顺序或 snapshot/journal 语义变化，由初始化/持久化维护者与 QA 重新审读，不能沿用旧输出。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [显式设施](world-initialization.prd.md#init-explicit-facilities) | [设计约束](#init-state-design) | 默认区域/碎片场景设施为 0，power_bootstrap 显式正例，单一矩阵 | 候选版本的[验证入口](../../../crates/oasis7_simulator/src/tests/init.rs)：运行 `twin_region_bootstrap_seeds_regions`、`asteroid_fragment_bootstrap_seeds_fragments_and_resources`、`scenarios_are_stable` 的 PowerBootstrap/plant-1 分支及 `init_seeds_power_facilities`、`init_rejects_facility_with_missing_owner`；检查零设施、显式设施和引用拒绝；Simulator 离线层，固定候选/config/seed，静态核对与实际执行分别记录 | 实施 PR 的候选、命令、fixture 与真实结果 | 不代签未执行回归、真实模型、浏览器、节点或发布验收 |
| [时间与初始事件](world-initialization.prd.md#init-time-events) | [设计约束](#init-state-design) | 时间 0 可有 Init 事件，snapshot 与 journal 覆盖范围分开 | 候选版本的[验证入口](../../../crates/oasis7/src/simulator/tests/persist.rs)：`initialize_kernel_records_chunk_generated_init_events` 检查 enabled Init 事件和 manifest/delta 一致；`kernel_persist_and_restore` 与 `restore_rejects_mismatched_journal_len` 检查配对恢复/拒绝。另以 fragment-disabled fixture 核对 time=0 和无 Init chunk 事件（该组合需补定向断言，非已有测试已覆盖）；Simulator 离线层，同候选固定输入，分别保留执行输出 | 实施 PR 的候选、命令、fixture 与真实结果 | 不代签未执行回归、真实模型、浏览器、节点或发布验收 |
