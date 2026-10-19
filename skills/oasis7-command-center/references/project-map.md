# 项目入口地图

## 使用方式

这是一份当前主责入口的查找地图。项目状态保存在[项目状态页](../../../doc/engineering/command-center/README.md)，关键设计与部署信息保存在[架构索引](../../../doc/engineering/command-center/architecture/README.md)；不要在 Skill 里再保存一份阶段、节点清单或已完成任务。

每次执行先解析用户选择的仓库、工作树和版本，再读取有关来源。远程文档固定到实际读取提交，GitHub/环境记录分别保留观测时间。摘要页的文件读取时间不是其中事实的核验时间；历史快照不能称为最新 main 或当前线上状态。

## 按问题读取

| 信息 | 入口 | 解释边界 |
| --- | --- | --- |
| 日常状态、目标与最近变化 | [项目状态](../../../doc/engineering/command-center/README.md) | 派生摘要，先看基线和覆盖，再核对受影响的原始来源 |
| 顶层设计与部署边界 | [架构索引](../../../doc/engineering/command-center/architecture/README.md) | 设计、实现、实际部署分别解释；索引不接管条款权威 |
| 怎样持续维护 | [持续更新流程](../../../doc/engineering/command-center/command-center.manual.md) | 工作收尾与回顾中的方法；不另建台账、审批或日报库 |
| 开发通用流程 | `doc/engineering/workflow/source-of-truth.md`；`AGENTS.md` | 流程只由前者拥有；入口、脚本和 Skill 不另设条件 |
| 项目定位与公开阶段 | `README.md` 的“项目状态” | 愿景、安装包、局部测试不能代替当前对外承诺 |
| 当前阶段目标、全局 P0 与依赖 | `doc/core/prd.md` 的“当前阶段交付目标与全局 P0” | 不默认采用头部历史里程碑或旧轮次 |
| 全局架构与产品分类 | `doc/core/design.md`；`doc/product/README.md` | core 作全局路由，技术细节下钻主责域；工程工具不增加第五产品域 |
| 世界规则与玩法 | `doc/product/world-rules-core-gameplay/prd.md` | 长期目标、循环、资源与制度；具体玩法下钻 `doc/game/prd.md` |
| 世界基础设施 | `doc/product/world-infrastructure/prd.md` | 持久、确定、恢复与一致性承诺；下钻 P2P/world-runtime 合同 |
| 智能体与世界模拟 | `doc/product/agents-world-simulation/prd.md` | 游戏 Agent、外部 Runtime 与玩家交互；不代表开发会话 |
| 玩家接入与发行 | `doc/product/player-entry-distribution/prd.md` | 发现、安装、访问及发行组合；公开阶段仍核对根 README |
| 环境与运维 | `doc/engineering/governance/environment-lanes-and-inventory-2026-05-29.md`；`doc/p2p/blockchain/public-testnet-governed-bootstrap.runbook.md` | 清单带自身日期；未采样环境不宣称部署就绪 |
| 实际工作与交付 | Git、PR；检查与评审从原 GitHub 页面核实，Issue/Project 按需 | 只有 PR 的工作也能展示；原页面保留各项验证的实际范围 |
| 验证方法 | `testing-manual.md`；`doc/testing/prd.md` | 按改动选择测试，普通 CI 为默认；不把完整游戏栈当看板前置 |

从产品和专业入口继续沿显式引用下钻。规范效力按条款范围、主责身份和明确后继确定；`active` 是文档身份，不能推导能力已验收。发现冲突写清来源和影响，不先要求整理全仓后才能使用看板。

## 当前实现入口

| 入口 | 用途 | 核对方法 |
| --- | --- | --- |
| [tools/oasis7-command-center](../../../tools/oasis7-command-center/README.md) | 独立桌面客户端、来源索引、四视图与原工具外部定位 | 先读实际 README、依赖和验证记录；完整边界见[系统设计 §11](../../../doc/engineering/command-center/command-center.design.md#11-验证设计与可追溯性) |
| `site/index.html`、`site/en/index.html` | 公共静态站点 | 从 `doc/site/design.md` 查公开内容与构建边界；不自动放入私有开发状态 |
| `crates/oasis7_viewer/` | 玩家 Viewer 与游戏交互 | 只有修改 Viewer 才使用其依赖和专项验证，不把它作为开发任务状态源 |

独立客户端的目标技术栈、进程和恢复合同在正式 PRD/design 中维护。早期设计快照中“尚无程序”的描述不能覆盖后续实现节与真实 PR；同样，代码目录存在不能证明签名分发、完整系统体验或全部模型任务已通过。

Viewer 的 Director、Agent Activity、Intent 和 Launcher 的 `gui-agent` 面向游戏世界。游戏外部 Runtime 产品从 `doc/product/agents-world-simulation/external-agent-runtime-play.prd.md` 进入；开发 Codex 会话按[外部接续方法](external-handoff.md)识别，不能混用身份、能力或验收结果。

## 迁移与旧材料

- 上传的旧 `world-rule.md` 与 `agent_world_*` 命名只帮助理解历史意图，沿当前产品/专业索引寻找接收条款；可能方向不自动成为当前规则。
- 文档中的历史任务格式、强制 Issue/Project 或 `.pm` 描述与通用流程不一致时，回当前 `source-of-truth.md` 核对；不要重启退役任务注册表或回执链。
- 路径变化沿当前索引、显式后继和 Git 历史定位，不恢复删除文件来凑齐地图。
- `AGENTS.md` 可能引用用户本机文件；仅在实际环境可用且相关时读取，不把某台机器的绝对路径写成可移植依赖。

## 现有方法与维护

按需读取 `.agents/skills/README.md`，复用实施、自测、调试、完成核实和专业评审方法，不串固定角色链。根 `skills/` 是非默认专家参考库，不全量加载，也不依赖安装第三方 Skill。

地图仅在入口或主责关系变化时更新。日常目标、进度、近期交付、需要人关注的事项和部署核验时间，统一更新前述状态页及架构索引。只读查询直接给结果；持久文档修改按已有授权和普通分支/PR 工作完成。
