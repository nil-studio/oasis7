# 工程文档总入口（模块设计）

本文件用于导航各模块设计文档与执行文档。所有新需求与在研需求均以模块 PRD 为唯一入口。

## 当前交付路径

玩家通过有界授权的 Agent，在同一持久世界完成首个铁锭成果、恢复生产并回访继续。发行状态以[根 README](../README.md#项目状态)为准。代表性链的 `production_only` 边界见[首局合同](product/world-rules-core-gameplay/first-session-and-continuation.prd.md)，实际生产结算与候选验收见[工业合同](game/gameplay/gameplay-industrial-starter-completion-contract.prd.md)。

[core 当前目标与里程碑](core/prd.md#当前阶段交付目标与全局-p0)解释交付依赖。持久单权威和工业体验核验可并行；首次开放须两者通过，BFT 验收决定后续写权交接。运行模式与恢复承诺由[基础设施](product/world-infrastructure/prd.md)和 [P2P](p2p/prd.md)拥有。`current` 表示已有实现，`partial` 表示尚未闭合能力，`target` 表示待验收承诺；passed 结果必须对应 PR/Issue 的候选与证据窗口。

## 快速阅读路径（推荐）
1. 先读本文件，获取导航。
2. 读根 `README.md`，先确认当前公开状态、技术预览边界与公开说明准备态。
3. 读 `site/index.html`，确认公开站点当前入口、预览验证路径与下载区口径。
4. 读 `doc/core/prd.md`，获取项目全局设计总览（模块地图、关键链路、关键分册）。
5. 若从产品视角阅读，先进入 `doc/product/README.md`，从固定四大产品模块选择唯一入口。
6. 进入目标工程模块 `doc/<module>/prd.md`，确认问题定义、方案、验收标准与技术边界。
7. 若目标模块已补齐 `design.md`，继续读模块设计总览，确认模块总体设计、分层和主链路。
8. 继续读 PR、实际 CI 与评审记录（Issue 按需），确认任务拆解、PRD-ID 映射、依赖与状态。
9. 按需下钻模块子文档（`doc/<module>/**/*.md`）。
10. 对照系统测试策略：`testing-manual.md` 与 `doc/testing/prd.md`。
11. 若已知 `task_uid`，读取对应 PR、实际 CI 与评审记录（Issue 按需）；未知具体任务时，先看模块 PR、实际 CI 与评审记录（Issue 按需）。

## 按目标进入
| 你的目标 | 第一入口 | 第二入口 | 说明 |
| --- | --- | --- | --- |
| 想先知道项目当前公开状态与技术预览边界 | `README.md` | `site/index.html` | 先确认“现在能看什么”，再决定是否深入仓库文档 |
| 想持续掌握目标、进度、近期变化和关键部署/架构决定 | [项目状态](engineering/command-center/README.md) | [架构索引](engineering/command-center/architecture/README.md)与[持续更新流程](engineering/command-center/command-center.manual.md) | 带来源时间的派生阅读入口，目标/规范与交付事实仍回原主责 |
| 想确认本地 / test / 正式三套环境边界 | `doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md` | `testing-manual.md` + `doc/p2p/prd.md` | 先看项目三环境总览，再按 hosted-login / network tier / launcher lane 下钻 |
| 想参与功能开发或治理任务 | `doc/core/prd.md` | `doc/<module>/prd.md` + PR、实际 CI 与评审记录（Issue 按需） | 先看全局目标，再进入目标模块 |
| 想明确 OpenClaw、Codex 等外部 Runtime 如何持续玩游戏 | [智能体产品目标](product/agents-world-simulation/prd.md#external-runtime-play) | [外部 Runtime 产品分册](product/agents-world-simulation/external-agent-runtime-play.prd.md) + [LLM/provider 专业合同](world-simulator/llm/README.md) | 先读目标、首期流程与验收，再看协议、适配和实际能力证据 |
| 想做本地验证、回归或验收 | `testing-manual.md` | `doc/testing/prd.md` | 手册负责 suite 选择，testing 模块负责测试体系建模 |
| 想调试 Viewer / Web 链路 | `doc/world-simulator/viewer/viewer-manual.manual.md` | `doc/testing/manual/web-ui-agent-browser-closure-manual.manual.md` | 前者是专项操作手册，后者是 Web 闭环步骤 |
| 想补过程上下文或追溯具体任务决策 | PR、实际 CI 与评审记录（Issue 按需） | PR、实际 CI 与评审记录（Issue 按需） | 先靠正式追踪定位任务，再看 task-scoped evidence |

## 产品模块入口

产品层只从 `doc/product/README.md` 进入，并固定为四个产品模块。产品文档按以下规则持续收敛：

- `doc/product/` 是四个模块的产品组合层目录；其他目录保留专业规则、实现与验证合同。既有散落内容只在出现当前重复权威、失效引用或实际语义缺口时有界处理，不把旧迁移批次恢复为默认工作。
- 新建或改写四模块相关的产品承诺、产品设计、跨域组合和端到端验收内容时，必须归入对应的 `doc/product/<产品模块>/`。
- `doc/product/<产品模块>/` 不限于单个 `prd.md`，可以按长期稳定的主题分册，形成由模块入口、主 PRD 和专题分册组成的文档树；分册必须由模块入口可达并回链主 PRD。
- 设计文档应按模块和长期主题收敛，避免新建带日期的短期小功能设计碎片；既有碎片应逐个完成语义合并、引用修复和安全删除。
- 治理以消除当前权威冲突和失效入口为目标，不以减少文件数量验收。专业规则、实现合同、测试/运维和历史证据保留独有职责；只有接收方完整承接内容且消费者已同步后，才删除重复来源。
- 产品正文不得与其他目录重复或混写；下方矩阵仅作为工程与治理模块导航，不是产品文档的替代入口。

## 工程模块入口矩阵
| 模块 | PRD 主文档 | 设计主文档 | 设计关注点 |
| --- | --- | --- | --- |
| core | `doc/core/prd.md` | `doc/core/design.md` | 项目全局总览与跨模块治理基线 |
| engineering | `doc/engineering/prd.md` | `doc/engineering/design.md` | 工程规范、质量门禁、文件治理 |
| game | `doc/game/prd.md` | `doc/game/design.md` | 玩法循环、规则层、发行可玩性 |
| headless-runtime | `doc/headless-runtime/prd.md` | `doc/headless-runtime/design.md` | 无界面运行链路与生产稳定性 |
| p2p | `doc/p2p/prd.md` | `doc/p2p/design.md` | 网络、共识、分布式存储 |
| playability_test_result | `doc/playability_test_result/prd.md` | `doc/playability_test_result/design.md` | 可玩性测试数据与收口闭环 |
| readme | `doc/readme/prd.md` | `doc/readme/design.md` | 对外口径与文档入口一致性 |
| scripts | `doc/scripts/prd.md` | `doc/scripts/design.md` | 自动化脚本能力与维护规范 |
| site | `doc/site/prd.md` | `doc/site/design.md` | 站点体验、内容发布、SEO |
| testing | `doc/testing/prd.md` | `doc/testing/design.md` | 分层测试体系与发布门禁 |
| world-runtime | `doc/world-runtime/prd.md` | `doc/world-runtime/design.md` | 运行时内核、WASM、治理与审计 |
| world-simulator | `doc/world-simulator/prd.md` | `doc/world-simulator/design.md` | 世界模拟、Viewer、LLM 与场景系统 |

## 目录结构说明
- `doc/<module>/prd.md`：模块设计主文档（唯一 PRD 入口）。
- `doc/<module>/design.md`：模块总体设计入口（结构、分层、主链路，ROUND-006 逐步补齐）。
- Git、PR、实际 CI 与评审记录：代码交付与验证事实；Issue/Project 按跟踪需要使用，不是建档、修改或验证的前置。
- `doc/<module>/prd.index.md`：模块文件级 PRD 索引（活跃专题文档可达入口）。
- `doc/<module>/**/*.md`：专题设计、实现方案、复盘与历史说明。
- `doc/<module>/README.md`：模块目录索引（按主题子目录导航）。
- PR、实际 CI 与评审记录（Issue 按需）：维护执行过程与验证事实。旧 `task_uid` 仅按历史记录追溯，不依赖退役 `.pm` 映射作为当前入口。
- `doc/devlog/README.md`：历史归档摘要入口；原始 `doc/devlog/2026-*.md` daily 文件已删除，日级细节从 Git history、相关模块 PR、实际 CI 与评审记录（Issue 按需）追溯，不再作为运行态真值。
- `doc/.governance/*-allowlist.txt`：文档组织门禁基线（根目录与模块根目录平铺文件冻结清单）。
- `doc/**/archive/` 不作为默认文档结构；历史专题仅在模块目录内保留并在索引中标注。少量 manifest-backed evidence asset archive（例如退役视觉证据图片）只作追溯证据，不作为当前 release / viewer / gameplay 首读入口。

## 顶层目录分类与例外

`doc/.governance/top-level-directory-registry.json` 是所有一级物理目录的机器可读登记；本页保留按读者目标维护的人工导航，而不是生成完整目录转储。`product` 只含四个逻辑产品模块，不是工程模块矩阵的第五类。

- 历史摘要例外：`doc/devlog/README.md`；不作为运行态任务或证据真值。
- 短周期视觉评审例外：`doc/ui_review_result/README.md`；只在存在可评分样本的轮次使用，长期结论回写到 `world-simulator`、`playability_test_result` 或 Git、PR 与实际 CI 记录。
- 所有其他一级专业/治理/测试目录及其 owner、入口与存在理由见 registry；新增、删除或重分类目录时必须同步更新 registry、本页和 landing page。

## 共享规则
- 新功能或行为变更先明确要求与验收，并更新实际受影响的产品或专业主责条款；实现、测试和相关文档在同一交付范围内维护，事实记录使用 PR、实际 CI 与评审记录（Issue 按需）。
- 新增或实质修改的跨层要求使用同一全局 Trace ID 关联声明、设计和任务，保留已有 PRD-ID 与局部锚点；Task 可以直接是 PR，简单编辑不为凑齐层级新建要求或任务。追踪约定从 `doc/engineering/doc-governance/README.md` 进入。
- 设计内容应优先收敛到对应产品模块或专业模块中可长期维护的模块级 `design.md` / 权威专题；避免为短期小功能新建带日期的设计碎片。既有碎片宜按语义合并、修复引用并在安全时删除；只有承担可独立长期治理职责的专题，才保留窄范围的专题设计文档，而不以临时任务范围建档。
- `doc/` 根目录只保留当前总入口；历史路径、已删除入口与迁移过程从 Git history 和 PR、实际 CI 与评审记录（Issue 按需）追溯，不在活跃导航重复列举。
- 模块根入口、专题落位、README 职责与 legacy redirect 的共享治理规则统一从 `doc/engineering/doc-governance/README.md` 进入，再按问题下钻到规范正文或对应专题。
