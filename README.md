# oasis7

> 在小行星带，给 AI 文明一个方向。

[English](./README.en.md) · [官网](https://oasis7.tech/) · [本地预览](#本地预览) · [开发文档](./doc/README.md) · [MIT License](./LICENSE)

<a id="项目概述"></a>
oasis7 是一款正在开发的 **AI Agent 文明模拟游戏**。玩家从世界之外设定目标，Agent 在资源、权限与世界规则的约束下选择行动，结果留下可追溯的历史。

我们希望探索：当能源有限、能力可以编程、合作需要付出代价时，智能体会如何建立自己的生存方式与社会秩序？

## 项目状态

当前处于 **受限可玩技术预览（limited playable technical preview）** 阶段，适合愿意准备环境的玩家、研究者和开发者。真实 LLM 试玩需要本地构建、模型配置和可用服务，调用可能消耗额度或产生费用。

这不是 closed beta、public launch 或赛季上线；正式公告仍在准备中。长期自治、文明涌现与持久世界公开开放仍需分别验证。GitHub Releases 与站点下载区主要承载开发预览构建说明，安装包、源码案例与当前 main 的能力应按各自版本核对。

本页是当前公开状态的权威入口；访问条件见[玩家接入与发行](./doc/product/player-entry-distribution/prd.md)，具体运行证据见对应版本的 [PR](https://github.com/nil-studio/oasis7/pulls)、[Issue](https://github.com/nil-studio/oasis7/issues) 与原始报告。

![小行星工业聚落与远处行星的世界氛围概念图，非游戏实机截图](./site/assets/images/hero/asteroid-settlement-planet-20261015.webp)

*世界氛围概念图，非实机截图。下面的生产案例来自单独保留的受控运行记录。*

<a id="玩家模型"></a>
## 玩家如何参与

- **你给出方向：**设定目标、调整提示词，在已有能力与授权范围内提供指导。
- **Agent 选择行动：**根据环境与反馈决定下一步，处理资源不足、权限限制和执行阻塞。
- **世界记录后果：**运行时验证动作、执行规则并记录结果，玩家据此调整下一轮目标。

玩家通过间接控制参与，不能绕过运行时修改世界状态或直接操纵 Agent 的每一步。各项干预与恢复能力以[世界规则与玩法](./doc/product/world-rules-core-gameplay/prd.md)及对应实现为准。

## 一次真实的生产尝试

在 2026-10-13 的受控 CLI 测试中，独立 Agent 从已准备、已认领的世界状态出发，尝试完成第一批铁锭：

| 步骤 | 实际发生的事情 |
| --- | --- |
| 目标 | 使用起始库存建造冶炼炉并完成首批生产。 |
| 阻塞 | 第一次选择的工厂超出授权范围，报价被拒绝，没有产生建造动作。 |
| 调整 | Agent 读取实际授权，改用允许的冶炼炉，继续建造和供料。 |
| 结果 | 一批配方完成，世界记录确认产出 **3 个铁锭、1 个炉渣**。 |

案例固定源码为 `52c136a7`，只证明该准备态下的建造、供料、首批生产与请求恢复；没有验证 GUI、从零安装、公开部署或持续多厂生产。可阅读[完整原始报告](./doc/testing/evidence/native-task-starter-chain-2026-10-13/bounded-retry/README.md)与[案例导读](https://oasis7.tech/devlog/first-iron-batch/)。

<a id="从哪里开始"></a>
## 从这里开始

| 你的目标 | 推荐入口 |
| --- | --- |
| 先了解玩法与世界 | [世界规则与玩法](./doc/product/world-rules-core-gameplay/prd.md) · [白皮书式项目总览](./doc/readme/governance/readme-project-overview-whitepaper-2026-04-25.md) |
| 尝试本地预览 | 下方[本地预览](#本地预览) · [Viewer 使用手册](./doc/world-simulator/viewer/viewer-manual.manual.md#本地真实-llm-游戏测试) |
| 接入自己的 Agent | [正式游戏 Skill](./skills/oasis7-play/SKILL.md) · [游戏客户端与 API](./crates/oasis7_game_client/README.md) |
| 阅读架构或参与开发 | [架构与代码导航](#高层架构) · [开发流程](./doc/engineering/workflow/source-of-truth.md) · [测试手册](./testing-manual.md) |
| 跟踪进展或反馈问题 | [开发日志](https://oasis7.tech/devlog/) · [Issues](https://github.com/nil-studio/oasis7/issues) · [Pull requests](https://github.com/nil-studio/oasis7/pulls) |

`viewer` 与 `pure_api` 是两种访问模式，能力与可玩性按各自证据判断。外部 Agent 应先读取正式 Skill 的接入条件；已有 API 或观察能力不代表完整玩法已经可用。

## 本地预览

先准备[仓库指定的 Rust 工具链](./rust-toolchain.toml)、Node.js/npm、Python 3 和 curl。当前手册的本地真实 LLM 路径使用 LetAI，需要自己的有效配置；配置方式见[首次运行准备](./doc/world-simulator/viewer/viewer-manual.manual.md#首次运行准备)。

```bash
git clone https://github.com/nil-studio/oasis7.git
cd oasis7

# 将路径替换为自己的 LetAI 配置文件；这一步只检查本地启动条件。
./scripts/run-local-letai-game-test.sh \
  --config /absolute/path/to/letai.env \
  --no-default-proxy \
  --preflight-only \
  -- \
  --viewer-port 48420 \
  --web-bind 127.0.0.1:48421 \
  --live-bind 127.0.0.1:48422
```

预检不验证模型凭据、额度或玩法结果。`--no-default-proxy` 取消脚本补入的本机默认代理，仍保留已有代理环境。

接着按手册的[首次前台启动](./doc/world-simulator/viewer/viewer-manual.manual.md#首次前台启动)先构建 provider bridge，再启动完整本地栈。首次不要使用 `--local-world-playtest`：这个日常 preset 会复用已有构建，并默认后台运行、跳过 provider smoke。

- **打开页面：**等待本次输出目录的 `launcher/session.meta` 出现 `STACK_READY=1`，打开其中的 `GAME_URL`。
- **理解就绪：**页面和端口就绪不等于真实模型决策或首产物完成；本地世界和玩法结果按手册分别核对。
- **停止首次前台运行：**在启动终端按 `Ctrl+C`。后台 preset 的生命周期不同，详见手册。
- **模型请求：**首次启动的 provider smoke 会请求真实模型；`--no-auto-play` 仅控制自动播放，不取消这些启动请求。

## 核心亮点

- **资源有代价。** 电力、交易与资源校验让行动承担成本。[交易规则实现](./crates/oasis7_simulator/src/kernel/actions_resolution.rs)
- **世界有历史。** 事件、快照与记忆提供持久化、恢复和追查入口；记忆恢复不等于长期学习已验证。[记忆实现](./crates/oasis7/src/simulator/memory.rs) · [持久化测试源码](./crates/oasis7/src/simulator/tests/persist.rs)
- **能力可编程。** Rust/WASM 模块提供扩展路径，部署与升级受资源、权限和治理约束。[模块生命周期测试源码](./crates/oasis7_runtime/src/world/governed_module_lifecycle_transaction_regressions.rs)

这些是实现与测试入口，具体能力仍须按版本验证。

<a id="可编程层"></a>
模块开发、编译、发布和安装各有权限边界：开发策略可允许运行时源码编译，正式加固策略使用外部构建产物与验证回执。WASM 是扩展路径，并非所有原生动作的必经层。详见[运行时与模块设计](./doc/world-runtime/README.md)。

## 设计原则

<a id="1-world-first"></a>
<a id="2-emergence-first"></a>
<a id="3-persistent"></a>
<a id="4-auditable"></a>
<a id="5-extensible"></a>
世界规则约束所有行动（World-first）；在基础规则上探索协作与制度涌现（Emergence-first）；保留并恢复世界历史（Persistent）；记录关键变化供追查（Auditable）；通过模块扩展能力（Extensible）。

持续推进仍依赖节点、运行服务与决策提供方可用。玩家离线、世界服务停止和长期自治是不同问题。

<a id="仓库结构"></a>
## 高层架构

玩家意图经 Agent 决策形成动作请求，由权威世界运行时验证与提交；客户端读取对应结果与投影。

| 职责 | 主要代码入口 |
| --- | --- |
| Agent 接入、决策与模型 | [`oasis7_agent_api`](./crates/oasis7_agent_api/) · [`oasis7_agent_harness`](./crates/oasis7_agent_harness/README.md) · [`oasis7_model_provider`](./crates/oasis7_model_provider/README.md) |
| 世界模拟与执行 | [`oasis7_simulator`](./crates/oasis7_simulator/) · [`oasis7_runtime`](./crates/oasis7_runtime/) |
| 世界状态、投影与服务 | [`oasis7_world_state`](./crates/oasis7_world_state/) · [`oasis7_world_projection`](./crates/oasis7_world_projection/) · [`oasis7_world_service`](./crates/oasis7_world_service/) |
| 协议、网络、共识与存储 | [`oasis7_proto`](./crates/oasis7_proto/) · [`oasis7_node`](./crates/oasis7_node/) · [`oasis7_net`](./crates/oasis7_net/) · [`oasis7_consensus`](./crates/oasis7_consensus/) · [`oasis7_distfs`](./crates/oasis7_distfs/) |
| 客户端与 Viewer | [`oasis7_game_client`](./crates/oasis7_game_client/README.md) · [`oasis7_client_api`](./crates/oasis7_client_api/) · [`oasis7_viewer_runtime`](./crates/oasis7_viewer_runtime/) · [Viewer 前端](./crates/oasis7_viewer/) |
| WASM 扩展 | [构建](./crates/oasis7_wasm_build/) · [SDK](./crates/oasis7_wasm_sdk/) · [执行](./crates/oasis7_wasm_executor/) · [路由](./crates/oasis7_wasm_router/) |

完整 Rust 成员以 [Cargo workspace](./Cargo.toml) 为准；`crates/oasis7_viewer` 是前端目录。Web Viewer 通过网关接入，承担观察与间接控制，不是完整分布式节点。

## 本阶段交付

当前目标是让受控玩家通过有界授权的 Agent 完成真实首产物，理解并恢复生产阻塞，保留成果，再次进入后继续原目标。首产物、持续生产、交付和回访分别验收。

基础设施先验证受控单权威的持久提交、副本与同世界恢复，再按既有身份、历史与授权合同扩展 BFT；节点增加不自动获得投票或写入权，网络 tier 变化也不赋予测试奖励生产价值。

目标与依赖见[核心 PRD](./doc/core/prd.md#当前阶段交付目标与全局-p0)、[首局与继续目标](./doc/product/world-rules-core-gameplay/first-session-and-continuation.prd.md)、[工业完成合同](./doc/game/gameplay/gameplay-industrial-starter-completion-contract.prd.md)及[基础设施合同](./doc/product/world-infrastructure/prd.md)。

<a id="模拟模型"></a>
<a id="空间模型"></a>
<a id="文明设定"></a>
<a id="产品四大模块"></a>
## 深入阅读

- [产品四大模块](./doc/product/README.md)：世界规则与玩法、权威世界基础设施、智能体与交互、玩家接入与发行。
- [世界模型与规则](./doc/product/world-rules-core-gameplay/prd.md)：空间、能源、资源、玩家与 Agent 边界。
- [游戏玩法顶层设计](./doc/game/gameplay/gameplay-top-level-design.prd.md)：玩法目标与演进方向。
- [文档总入口](./doc/README.md)：产品、系统设计、操作与测试的专业入口。

## 参与贡献

欢迎提供试玩反馈、修复问题、改进文档和参与开发：

- **反馈问题：**在 [Issues](https://github.com/nil-studio/oasis7/issues) 提供版本、平台、复现步骤、预期与实际结果，以及脱敏后的日志。
- **贡献修改：**先读[开发流程](./doc/engineering/workflow/source-of-truth.md)，在独立分支修改；[测试手册](./testing-manual.md)说明适用验证。
- **理解设计：**从[文档总入口](./doc/README.md)进入对应模块，保持需求、设计与实现的一致性。

## 许可证

项目采用 [MIT License](./LICENSE)。
