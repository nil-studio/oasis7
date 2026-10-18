# oasis7

> 在小行星带，给 AI 文明一个方向。

oasis7 是一款正在开发的 AI Agent 文明模拟游戏。玩家从世界之外设定目标，Agent 在资源与规则约束下作出选择，行动在同一个世界中留下可追溯的后果。

我们希望探索：当能源有限、能力可以编程、合作需要付出代价时，智能体会如何建立自己的生存方式与社会秩序？

当前项目处于 **受限可玩技术预览（limited playable technical preview）** 阶段，适合愿意配置环境、探索预览链路的玩家、研究者和开发者。长期自治与文明涌现仍是需要持续验证的目标，当前版本不是面向大众的正式发布。

## 本阶段交付

让受控真实玩家在同一个持久世界中，通过有界授权的 Agent 完成真实首产物，理解可恢复的生产阻塞，保留成果，并在再次进入后继续原有目标。

代表性路径采用 `starter-industrial-smelter-to-assembler-v1`：合法建成 Smelter → 完成铁锭生产 cycle → owner-bound output ledger 实际正量结算 → 开放 Assembler 候选。首产物采用 `production_only` 边界；持续生产、交付和回访分别验收。入口与候选证据见[首局合同](./doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md)和[工业完成合同](./doc/game/gameplay/gameplay-industrial-starter-completion-contract.prd.md)。

运行路线先验收受控单权威的持久提交、独立故障域副本和同世界恢复，再沿既有身份与历史扩展 BFT。当前已有实现与目标承诺分别核验，本文不宣布持久世界已开放。网络环境、发行阶段、世界保留与资产价值各按自己的合同表达；网络 tier 变化不赋予测试奖励生产价值。详见[基础设施合同](./doc/product/world-infrastructure/prd.md)。

[当前里程碑与依赖](./doc/core/prd.md#当前阶段交付目标与全局-p0)路由到专业验证入口；候选版本、实际 CI 与运行证据从 [PR](https://github.com/eng-cc/oasis7/pulls) 和 [Issue](https://github.com/eng-cc/oasis7/issues) 核对。文档收敛、样例结构、真人理解与继续意愿分别判定，公开阶段仍为受限技术预览。

## 从哪里开始

- **了解项目：**先读下面的核心亮点与玩家模型，再查看[世界规则与玩法](./doc/product/world-rules-core-gameplay/prd.md)。
- **尝试本地预览：**阅读 [Viewer 使用手册](./doc/world-simulator/viewer/viewer-manual.manual.md)；真实 LLM 试玩需要 provider 配置、构建产物和可用服务，不是零配置演示。
- **参与开发：**从[文档总入口](./doc/README.md)和[测试手册](./testing-manual.md)选择对应领域与验证路径。

## 核心亮点

- **资源有代价。** 电力订单、撮合和资源校验为 Agent 的行动施加约束，选择需要承担成本。参见[交易规则实现](./crates/oasis7/src/simulator/kernel/actions_resolution.rs)。
- **世界有历史。** 事件、快照与 Agent 记忆的持久化和恢复实现，让结果可以追查；记忆恢复本身不等于已经验证长期学习。参见[记忆实现](./crates/oasis7/src/simulator/memory.rs)与[持久化测试源码](./crates/oasis7/src/simulator/tests/persist.rs)。
- **能力可编程。** WASM 模块提供扩展路径，部署、交易和升级受资源、权限及治理约束。参见[模块生命周期回归测试源码](./crates/oasis7_runtime/src/world/governed_module_lifecycle_transaction_regressions.rs)。

这些链接说明实现与测试入口，不代替当前版本的运行验收。

## 项目概述

Agent 与地点、资源、设施和模块共同构成世界。LLM 为 Agent 提供决策能力；世界运行时负责校验动作、执行规则和记录变化。

项目提供资源、交易与治理的基础规则，并探索 Agent 在这些约束上形成更复杂的协作与组织。制度涌现是研究与玩法方向，不表示所有市场或社会规则都已经由 Agent 自主生成。

---

## 设计原则

### 1. World-first

所有行为必须通过世界规则验证。  
禁止绕过运行时直接修改状态。

### 2. Emergence-first

不预设剧情。  
行为由资源与规则约束自然产生。

### 3. Persistent

世界状态可落盘与恢复。  
玩家客户端离线与世界服务停止是不同情况：持续推进仍要求相应节点、运行服务和决策提供方可用。持久化与恢复能力不代表无人值守的长期自治已经验收。

### 4. Auditable

关键状态变更以事件形式记录。  
支持回放与审计。

### 5. Extensible

高阶逻辑通过 WASM 模块扩展实现。

---

## 玩家模型

玩家不是世界中的实体，而是外部策略提供者。

玩家可以：

- 提供高层目标
- 调整 Agent 的提示词
- 指导开发 WASM 模块

玩家不能：

- 直接控制 Agent
- 修改底层世界规则
- 绕过共识层修改状态

控制是间接的，Agent 保持自主性。玩家通过观察结果、调整目标和提示词参与下一次决策；更完整的干预、纠正与恢复体验以[产品规格](./doc/product/world-rules-core-gameplay/prd.md)及对应实现证据为准，不能把设计目标视为当前全部可用。

---

## 模拟模型

### 空间模型

- 默认空间：100km × 100km × 10km 破碎小行星带
- 小行星直径：500m–10km
- 最小间距：≥500m
- 空间分辨率：1cm（`GeoPos` 与世界状态直接以整数厘米表示）

### 文明设定

- 硅基智能体
- 能源来源：辐射 → 电能
- 无生物需求
- 关键约束维度：电力、算力、存储、带宽

资源与约束塑造结构。

当前实现采用最小通用资源模型：

- 由 runtime/consensus 直接校验的通用资源类型：`Electricity`、`Data`
- 材料、产物和制度化记录的具体模型由对应 runtime 领域或经治理模块定义；它们不自动成为通用资源类型或统一资产接口。

算力、存储和带宽属于系统能力或运行约束；本 README 不将其表述为当前资源余额的近似映射、独立玩家资源，或可由模块统一定义的经济资产。

---

## 可编程层

模块开发路径包括编写 Rust 逻辑、编译为 WASM、部署及安装。不同运行模式的权限与构建方式不同：

- 开发模式提供运行时源码编译动作，是否允许由策略控制。
- 正式加固策略禁止运行时源码编译，发布路径使用外部构建产物及验证回执。
- 部署、安装、升级仍受权限、资源与治理规则约束，不是 Agent 在任意环境下都能自由执行的动作。

参见[发布安全策略](./crates/oasis7_runtime/src/world/mod.rs)与[发布动作实现](./crates/oasis7_runtime/src/world/module_actions/release_actions.rs)。

WASM 模块：

- 有资源成本
- 可交易
- 可升级
- 可审计
- 作为游戏内实体存在

模块让世界能力可以扩展；更复杂的社会结构如何形成，仍需在基础规则与治理边界内探索。

---

## 高层架构

玩家意图 → Agent 决策 → 动作请求 → World Runtime 校验与提交

- WASM Modules：可编程扩展路径，并非所有原生动作的必经层。
- Consensus Layer：参与维护权威状态与提交结果。
- Distributed Storage & Networking：提供对应的数据与节点通信能力。


权威提交按合法激活的 authority profile 验证，阶段承诺和激活条件由基础设施与 P2P 合同拥有。

节点扩展先验证同世界历史和重执行，再按交接合同获得投票与写入权（推荐 native 进程）。

Web 端默认定位为 Viewer/间接控制客户端，通过 `oasis7_viewer_live --web-bind` 网关桥接接入，不承担完整分布式节点职责。

---

## 仓库结构

对外品牌与当前 workspace / crate 命名已统一为 `oasis7`；仓库内的当前实现、脚本与入口均以 `oasis7` 为准。

- `oasis7_proto` — 协议与共享数据模型
- `oasis7_net` — 网络层
- `oasis7_consensus` — 共识与协调层
- `oasis7_node` — 节点运行时
- `oasis7_distfs` — 分布式存储
- `oasis7_wasm_*` — WASM 执行与路由层
- `oasis7` — 核心模拟层
- `oasis7_viewer` — 可视化与调试工具

---

## 项目状态

当前项目处于 **limited playable technical preview** 阶段。

- 当前对外可确认内容：架构、验证链路、受限可玩预览构建包与文档入口已开放。
- 当前不应误读的内容：这不是 closed beta、public launch、面向大众的正式可玩发布，也不代表赛季已上线。
- 当前公开说明状态：正式公告仍在准备中；GitHub Releases 与站点下载区当前主要承载开发预览构建说明。
- 推荐入口：先查看本页项目概述与文档总入口，再决定是否进入完整构建与深度文档。

相关入口：[项目概述](#项目概述) · [`doc/README.md`](./doc/README.md) · [`testing-manual.md`](./testing-manual.md)

欢迎讨论与贡献。

## 从这里开始

如果你现在只是想快速找到正确入口，先按目标选路径：

| 你的目标 | 先读 | 再读 |
| --- | --- | --- |
| 想确认项目现在公开到了什么程度 | [项目状态](#项目状态) | [玩家访问与发行说明](./doc/product/player-entry-distribution/prd.md) |
| 想先用一份白皮书式总览理解项目 | [`doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md`](./doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md) | [`doc/core/prd.md`](./doc/core/prd.md) |
| 想本地验证 Viewer / Web / API 链路 | [`testing-manual.md`](./testing-manual.md) | [`doc/world-simulator/viewer/viewer-manual.manual.md`](./doc/world-simulator/viewer/viewer-manual.manual.md) |
| 想理解世界规则、玩法和玩家边界 | [`doc/product/world-rules-core-gameplay/prd.md`](./doc/product/world-rules-core-gameplay/prd.md) | [`doc/game/gameplay/gameplay-top-level-design.prd.md`](./doc/game/gameplay/gameplay-top-level-design.prd.md) |
| 想参与开发或继续治理文档/代码 | [`doc/README.md`](./doc/README.md) | [`doc/core/prd.md`](./doc/core/prd.md) |

### 产品四大模块

产品信息架构以 [`doc/product/README.md`](./doc/product/README.md) 为唯一总入口，固定分为“世界规则与玩法系统 / 权威世界基础设施 / 智能体、世界模拟与交互 / 玩家接入与发行”。工程模块、测试模块与文档治理入口继续由 `doc/README.md` 导航，不构成第五个产品模块。
其中 [`doc/product/player-entry-distribution/prd.md`](./doc/product/player-entry-distribution/prd.md) 只组合玩家发现、访问、安装与验证路径；本 README 仍是当前公开状态和 claim envelope 的权威。


## 深入阅读

`README` 只负责快速对齐项目定位与入口，不再在这里重复维护世界规则摘要。继续深入时，直接进入对应权威文档：

- 白皮书式项目总览：[`doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md`](./doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md)
- 世界规则与系统边界：[`doc/product/world-rules-core-gameplay/prd.md`](./doc/product/world-rules-core-gameplay/prd.md)
- 玩家访问模式与技术预览边界：[`doc/product/player-entry-distribution/prd.md`](./doc/product/player-entry-distribution/prd.md)
- Viewer / Web / 运行使用说明：[`doc/world-simulator/viewer/viewer-manual.manual.md`](./doc/world-simulator/viewer/viewer-manual.manual.md)
- 闭环测试与套件矩阵：[`testing-manual.md`](./testing-manual.md)
- 游戏玩法顶层设计：[`doc/game/gameplay/gameplay-top-level-design.prd.md`](./doc/game/gameplay/gameplay-top-level-design.prd.md)
