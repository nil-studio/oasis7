# `world-simulator/viewer` 热点子域入口

更新时间: 2026-10-12

## 从这里开始
- 想先理解 Pixel-world 对玩家承诺的首读层级、空间关系来源、可归因因果与诊断边界：读 [`doc/product/agents-world-simulation/player-readable-world-stage.prd.md`](../../product/agents-world-simulation/player-readable-world-stage.prd.md)；具体 DTO、LOD、renderer 与验证仍回到本目录专业文档。
- 想确认 Viewer / player-facing surface 的整体视觉方向、层级、pixel-world 可读性与视觉评审 gate：先读 `viewer-visual-design-spec-2026-06-05.design.md`；涉及本轮 image2 视觉目标、首屏总体设计与分模块落地时继续读 `viewer-page-module-design-2026-06-18.design.md`；涉及 brand book、token、icon/status、资产准入和截图矩阵时继续读 `viewer-brand-system-2026-06-05.design.md`
- 想执行 Viewer、走 Web 闭环、看命令或手工步骤：先读 `viewer-manual.manual.md`
- 想确认正式浏览器主入口、`viewer` / `software_safe` 兼容边界或弱机/CI 默认路径：先读 `viewer-web-entry-compatibility.prd.md`
- 想确认 Viewer 前端 `js/html/jsx` 文件何时拆分、怎么抽组件/模块、generated artifact 与 compat alias 怎么评审：先读 `viewer-frontend-structure-standard.prd.md`
- 想确认 `legacy_core.js` 拆分边界、`viewer.js` / `software_safe.js` canonical/compat 关系，或 `pixel-world-bridge` generated runtime 真值：先读 `viewer-web-build-truth.prd.md`
- 想确认 pixel-world 稀疏快照下的 agent 派生坐标、关系线与 rendered DOM 定位：先读 `viewer-pixel-world-semantic-positioning.prd.md`
- 想确认 pixel-world Fragment terrain、Location 逻辑锚点与缩放 LOD 分层：先读 `viewer-pixel-world-fragment-lod.prd.md`
- 想确认 pixel-world 面向商业化游戏首屏如何呈现目标、下一步、玩家杠杆与诊断降噪：先读 `viewer-pixel-world-player-readable-rendering.prd.md`
- 想确认 pixel-world 下一轮如何表达玩家因果、行动反馈、生产可读性与商业化长期循环：先读 `viewer-pixel-world-player-leverage-production-readability-2026-05-28.brainstorm.md`
- 想确认 runtime live / event-driven / step-control 现行口径：先读 `viewer-control-plane-split-live-playback.prd.md`；操作、进程与 runtime-live 观测边界读 `viewer-manual.manual.md`，Agent/provider 语义读 `../llm/decision-provider-contract.prd.md`
- 想确认本人、其他玩家与匿名请求的反馈归属，以及预检和提交的边界：产品读[反馈连续性](../../product/agents-world-simulation/player-readable-surface-continuity.prd.md#req-agent-surface-003)，专业要求读[动作反馈目标](viewer-control-plane-split-live-playback.prd.md#player-action-feedback-targets)，实现方案读[控制面设计](viewer-control-plane-split-live-playback.design.md#gameplay-feedback-isolation)。实现、定向回归和真实多人验收分别核实，文档导航不代替运行证据。
- 想处理锚点重复枚举、depot 范围圈实体复用或 selected/corner-frame 视觉回归：读[增量与回归要求](viewer-pixel-world-bridge-rendering.prd.md#render-reconcile-and-visual-regression-requirements)及配对设计；claim evidence 测试页的[已有分类](viewer-pixel-world-bridge-rendering.prd.md#claim-evidence-classification)不再作为缺口。
- 想先理解 Agent 对话、预设/草稿、Prompt/目标调整与反馈恢复的产品语义：读 [`Agent 对话与 Prompt 控制`](../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md) 及其配对产品设计；当前 surface、协议、操作边界与自动化合同继续读 `viewer-manual.manual.md` 和 `viewer-web-semantic-test-api.prd.md`
- 想确认 Viewer terminal shell 的目标与 current-vs-target 边界：先读 `viewer-gameplay-release-experience-overhaul.prd.md`；旧 `immersion-phase2~10` 阶段三件套不再作为首读或 active 索引入口。
- 终态实现状态：`world_feed/v1` runtime/Viewer projection 与 `#viewer-world-feed` 已落地；Director verifier/state machine 与 `/api/public/director/capability` fail-closed boundary 已落地，但可信 issuer 缺失时生产成功进入仍为 `capability_blocked`。WebGL2 在 GPU-enabled 环境走 `ready`，GPU-disabled 环境明确显示 `Renderer Unavailable`；具体操作与证据边界读 `viewer-manual.manual.md`。
- 想精确找某份专题文档，而不是按问题阅读：回到 `../prd.index.md`

## 入口分工
- 当前页只承担 `viewer/` 子目录 landing page 职责，不复制完整长表。
- `viewer-manual.manual.md` 是 Viewer / Web 闭环 / operator 的 canonical 操作手册。
- `../prd.index.md` 是 world-simulator 模块完整文件级索引，适合已知主题后按文件名查找。
- `../README.md` 是 world-simulator 模块级 landing page，负责跨 `viewer / launcher / llm / kernel / scenario / m4` 分流。

## 密度快照
- 治理前快照（`scripts/doc-inventory-report.sh`，2026-04-17）:
  - `doc/world-simulator/viewer/`: 296 份 Markdown
  - `doc/world-simulator/`: 549 份 Markdown
- 当前复算（`scripts/doc-inventory-report.sh`，2026-07-03）:
  - `doc/world-simulator/viewer/`: 199 份 Markdown
  - `doc/world-simulator/`: 457 份 Markdown
- 当前子域属于仓库最高密度热点路径；治理目标是持续压缩首读路径，并在证据链完整时删除不再承载当前入口语义的旧专题。

## 首读主题簇

### 1. 操作手册与执行闭环
- 首读入口: `viewer-manual.manual.md`
- 适合问题:
  - 怎么启动 Viewer
  - Web 闭环怎么跑
  - `viewer` canonical 入口、`software_safe` alias / bilingual URL / test API 怎么使用
- 说明: 如果你是来“操作”而不是“做治理判断”，这里通常是第一入口。

### 2. 视觉方向与评审 gate
- 首读入口:
  - `viewer-visual-design-spec-2026-06-05.design.md`
  - `viewer-page-module-design-2026-06-18.design.md`
  - `viewer-brand-system-2026-06-05.design.md`
- 适合问题:
  - Viewer / player-facing surface 的统一视觉方向是什么
  - 世界、目标、Agent、路径、回执、诊断的视觉层级怎么排
  - pixel-world、2D 地图、移动端和视觉 review gate 应该按什么标准验收
  - 本轮 image2 视觉目标如何拆成首屏总体设计、Stage Hero、Pixel World Board、Command Strip、Action Receipt、Targets、Details 和 Diagnostics
  - brand book、语义 token、icon/status vocabulary、资产语言和截图矩阵如何执行
  - 当前可见改动需要按什么截图矩阵与视觉 review gate 取得新证据

### 3. `viewer` 与正式 Web 主入口
- 首读入口:
  - `viewer-web-entry-compatibility.prd.md`
  - `viewer-web-semantic-test-api.prd.md`
  - `viewer-web-build-truth.prd.md`
- 适合问题:
  - 为什么正式 Web 默认走 `viewer`
  - 弱机 / CI / 无 GPU 环境下的 canonical 路径是什么
  - 浏览器 fatal、语义测试接口、正式主入口怎么对齐

### 4. runtime live / event-driven / control
- 首读入口:
  - `viewer-control-plane-split-live-playback.prd.md`
  - `viewer-manual.manual.md`
  - `../llm/decision-provider-contract.prd.md`
- 适合问题:
  - runtime live 现在哪些能力已经接管
  - event-driven 阶段的主文档是哪份
  - step/control/live playback 的现行边界是什么
  - Viewer 与 `oasis7_chain_runtime` 的进程、chain-link 与 Launcher 编排边界是什么；跨进程 session/recovery 继续读 `../launcher/game-client-launcher-runtime-session-continuity.prd.md`
- 说明: 历史 event-driven phase8/9/10、runtime-world LLM bridge、Viewer/node decouple、两轮 Web build pruning 与 runtime-fatal 三件套均已按专业边界回填至当前稳定 authority 并物理删除；当前 `oasis7_viewer_live` 是 Viewer/debug runtime-world surface，不应从旧阶段文件、旧 CLI 或旧 byte-size 样本倒推现行口径。

### 5. chat / prompt / contextual command
- 首读入口:
  - `../../product/agents-world-simulation/agent-conversation-and-prompt-control.prd.md`
  - `../../product/agents-world-simulation/agent-conversation-and-prompt-control.design.md`
  - `viewer-manual.manual.md`
  - `viewer-web-semantic-test-api.prd.md`
- 适合问题:
  - Agent 对话、预设草稿和 Prompt/目标调整在产品层如何区分
  - 当前聊天入口、contextual command、Prompt profile 与协议怎样组织
  - 输入法、回车发送、预设编辑这些问题该去哪里看
- 说明: 产品层先读新产品专题；当前 SolidJS surface、权限、preview/apply/rollback 和自动化合同以手册与 semantic test API 为准。文本复制、中英文、输入隔离、Chat IME 去重与稳定性要求由上述入口承接；Quote 属于 contextual Command/Console，Search 是 Targets 内 `#entity-search` 过滤能力。

### 已吸收的旧 Viewer 实现追溯

- EGUI 右侧面板、Chat IME 桥接、Chat Web 锁重入和产业图谱语义缩放共八份旧文档已退役。有效输入要求进入手册、semantic test API 与 Agent 对话/Prompt 产品专题；稳定性进入 Web Build Truth；产业关系与空间层级目标进入 Player-readable World Stage 和 Pixel-world player-readable rendering。
- 固定历史基线为 `25fbcd7cf590c5e1b1248119d407cc7c34e8e634`（第三轮复审 main，含 PR #4154）。可用 `git show <该OID>:doc/world-simulator/viewer/<旧文件名>` 读取原文：`viewer-egui-right-panel`、`viewer-chat-ime-egui-bridge`、`viewer-chat-web-deadlock-resolution`、`viewer-industry-graph-semantic-zoom`，各含 `.prd.md` 与 `.design.md`；原任务/截图/实施证据从 GitHub task issue evidence 和 `.pm/github-project-sync/task-archive.jsonl` 追溯。固定基线是历史证据，不声明当前实现或全场景验收通过。

### 6. release / 体验收口
- 首读入口:
  - `viewer-gameplay-release-experience-overhaul.prd.md`
- 适合问题:
  - 首局体验、release readiness、viewer 主体验的主文档是什么
  - 哪些沉浸阶段已经物理合并，哪些不再是独立首读入口
- 当前状态摘要：Player 已是 fullscreen、world-first 单舞台，单一 Next Move 主动作及从属
  Objective / Player Leverage 上下文与按需
  `World / Targets / Command` drawer 属于当前入口；旧左/中/右三栏仅可能出现在
  显式 Director 工具视图。#3315 已以 focused implementation tests 修复三处
  World Feed / 多资源摘要缺陷；测试证据与历史 #3248 浏览器边界见上述 PRD。#3315
  已用外部 Chrome 在 headed `1440x1000` 与 `390x844` 复核 GPU 不可用 fallback；这不是
  `ready=true` WebGL2/Cinematic 正常路径、probe 后异步 GPU 失败或 tablet `641–1240`
  宽度的放行证据。后续 #3624/#3638/#3668 的证据按其实际范围保留，不因旧环境限制
  推定全部仍未实现。2026-10-12 源码基准 `5e930fa` 已包含窄屏 Next Move 第一行、
  上下文第二行及 P1-A 的 display-model/组件；结构存在不代签当前行为验收。
  持续视觉约束包括权威可选 world_read 摘要、单一 Next Move
  CTA、默认 command/chat 与收起 Diagnostics，以及保留内部 hooks 的
  `Cinematic View / Minimal HUD`；不新增 synthetic Power/Data/Consensus 或
  runtime/protocol 字段。

### 7. split Web delivery / optional WASM
- `pixel_world_bridge_bindgen_bg.wasm` 约 63.5 MB，故意不进入玩家 native installer 或单独的 `web-dist/`。
- release、mainnet、testnet workflow 产出 `oasis7-viewer-web-delivery.tar.gz`，内含 Viewer Web 根目录与并列的 `viewer-optional-payload/`；`optional-payloads.json` 以相对路径、SHA-256、字节数和 `viewer-web-build` provenance 声明可用 payload。
- 发布归档保留内容 fingerprint 与文件清单，但不写入 checkout-specific 的 `sourceLatestPath` / `sourceLatestMtimeNs`；这些字段只服务本地 freshness 检查。
- 部署 Web 时应解压并以 archive 根目录作为静态根；不要只复制 `web-dist/`。若 payload 真实缺失，manifest 保持 `available: false`，Viewer 必须进入明确的 Renderer Unavailable fallback。

## 定向检索边界
- 已知准确文件名时，直接查 `../prd.index.md`；本页不替代完整索引。
- 旧 2026-03-11 模块状态 closure / viewer-to-producer handoff 文档已退役删除；当前 Viewer 合同以本目录 PRD/design/manual 为准，活跃任务状态与历史专题从 `../prd.index.md`、PR、实际 CI 与评审记录（Issue 按需） 与 `.pm/github-project-sync/task-archive.jsonl` 进入。
- 如果某个主题已经出现“主文档物理合并”，应优先进入主文档，而不是从旧阶段文档开始；release immersion phase2~10 的历史追溯从主文档、`../prd.index.md` 历史说明、core review logs、git history 与 PR、实际 CI 与评审记录（Issue 按需） 进入。

## 维护约定
- 新增 Viewer 专题后，若改变了默认首读路径，应同步更新本页。
- 任何改变 Viewer 默认视觉方向、player-facing screen flow、pixel-world 层级或视觉评审 gate 的任务，必须同步评估是否更新 `viewer-visual-design-spec-2026-06-05.design.md`。
- 任何改变 Viewer brand book、语义 token、icon/status vocabulary、资产语言或截图矩阵的任务，必须同步评估是否更新 `viewer-brand-system-2026-06-05.design.md`。
- 本页只维护簇级入口，不维护完整文件清单。
- 若未来 `viewer/` 内部继续分裂出更高密度簇，再另开簇内治理专题，而不是把本页扩写成长表。
