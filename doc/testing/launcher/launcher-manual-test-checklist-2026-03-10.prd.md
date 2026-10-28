# oasis7：启动器人工测试清单（2026-03-10）

审计轮次: 1

- 目录入口: `doc/testing/launcher/README.md`

## 1. Executive Summary
- Problem Statement: 启动器虽然已有较多单元/定向测试，但发布前若缺少传统互联网产品视角的人工测试清单，容易遗漏“真实用户会遇到、但代码层不易暴露”的安装、配置、交互、异常恢复与兼容性问题。若清单只停留在模块级 smoke，又会把 `Explorer`、`Transfer` 这类依赖真实数据与状态迁移的功能测得过粗。
- Proposed Solution: 建立启动器人工测试唯一清单，按 `P0 必测 / P1 应测 / P2 选测` 组织“环境预检 -> 首次启动 -> 核心流程 -> 异常恢复 -> 兼容性 -> 发布收口”闭环；同时把 `Explorer` / `Transfer` 从“大功能项”细分为“子能力矩阵”，补齐数据前置、字段断言、空态/错态判定与双证据要求。
- Success Criteria:
  - SC-1: 启动器人工测试范围覆盖安装/启动、配置、游戏与链启停、Explorer/Transfer/Feedback、异常恢复、兼容性与升级迁移。
  - SC-2: 每个测试项都具有可执行步骤、预期结果、证据要求与优先级。
  - SC-3: 人工测试结论可直接映射到 `pass/conditional/fail/blocked` 发布结论，细粒度执行项可映射到 `pass_with_data / pass_empty_expected / fail_wrong_data / fail_not_found_unexpected / blocked_env`。
  - SC-4: 文档与 `testing-manual.md`、现有 launcher 专题保持互链，执行入口清晰。
  - SC-5: 本专题文档纳入 `doc/testing/prd.index.md`，并由 PR、实际 CI 与评审记录（Issue 按需） 提供可变追溯性。

## 2. User Experience & Functionality
- User Personas:
  - `qa_engineer`：需要一份可复用、可审计、能检出细节问题的启动器人工测试清单。
  - `viewer_engineer`：需要知道哪些体验缺陷必须靠人工验证兜底，哪些问题已收敛到前端/接口/数据层。
  - 发布负责人：需要根据人工测试卡快速做放行判断，并能区分“空数据正常”与“结果错误”。
- User Scenarios & Frequency:
  - 每次启动器高风险改动后执行一轮 `P0` 人工 smoke。
  - 每次候选发布前执行 `P0 + P1` 人工回归。
  - 线上事故复盘后，按问题类型补跑对应异常与恢复项。
  - 若 `Explorer / Transfer` 有逃逸缺陷，补跑其子能力矩阵而非只重跑总览 smoke。
- User Stories:
  - PRD-TESTING-LAUNCHER-MANUAL-001: As a `qa_engineer`, I want a prioritized launcher checklist, so that I can run manual validation consistently.
  - PRD-TESTING-LAUNCHER-MANUAL-002: As a `viewer_engineer`, I want core user journeys and failure-recovery cases enumerated, so that code-level automation gaps are covered before release.
  - PRD-TESTING-LAUNCHER-MANUAL-003: As a 发布负责人, I want evidence and verdict rules attached to each item, so that release decisions are auditable.
- Critical User Flows:
  1. Flow-LMTC-001: `环境预检 -> 启动 launcher -> 校验窗口/页面/日志 ready`
  2. Flow-LMTC-002: `填写/修改配置 -> 启动游戏或链 -> 观察状态与引导`
  3. Flow-LMTC-003: `执行 Explorer / Transfer / Feedback / LLM Settings 核心交互 -> 验证结果`
  4. Flow-LMTC-004: `制造异常（端口占用/坏配置/断网/重复点击） -> 观察错误提示与恢复`
  5. Flow-LMTC-005: `汇总证据 -> 输出 pass/conditional/fail/blocked -> 回写发布结论`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 排序/计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 环境预检 | 版本、平台、构建产物、端口、依赖 | 检查执行文件、静态目录、日志目录、浏览器/网络环境 | `unchecked -> ready-to-test/blocked` | 先阻断项后功能项 | QA/发布可执行 |
| 首次启动与基础可用性 | 首屏、窗口、加载态、ready 日志、默认配置 | 首次打开 launcher 并确认关键区域可用 | `cold-start -> ready/failed` | P0 必跑 | QA 执行，开发协助定位 |
| 配置与引导 | 必填项、默认值、禁用态、引导文案 | 缺配置时验证提示、修复后验证恢复 | `invalid -> guided -> valid` | 阻断优先于美观问题 | QA/Viewer 共审 |
| 核心业务能力 | 游戏启停、链启停、Explorer、Transfer、Feedback、LLM Settings | 执行核心交互并确认结果一致 | `idle -> running/stopped/error` | 先主链路后次链路 | QA 为主 |
| Explorer / Transfer 子能力矩阵 | 数据前置、查询路径、字段映射、空态/错态 | 先造数再查数，并用 API + Web 双证据核验 | `empty -> seeded -> queried -> asserted` | 先确认数据存在，再校验细分查询分支 | QA 主导，开发辅助归因 |
| 异常与恢复 | 端口占用、断网、远端失败、重复操作、脏状态 | 制造失败并验证错误信息、回滚与可恢复性 | `error -> recovered/blocked` | 先数据安全再交互流畅性 | QA 可阻断 |
| 兼容与发布收口 | OS、浏览器、分辨率、旧配置、显式静态目录 | 跨环境复核并汇总结论 | `sampled -> signed-off/conditional` | 发布前至少覆盖目标平台矩阵 | 发布负责人确认 |
- Acceptance Criteria:
  - AC-1: 清单至少区分 `P0/P1/P2` 与 `pass/conditional/fail/blocked`。
  - AC-2: `P0` 覆盖冷启动、配置校验、游戏/链启停、错误提示、恢复与日志可读性。
  - AC-3: `P1` 覆盖 Explorer/Transfer/Feedback/LLM Settings/自引导等次核心能力，且 `Explorer / Transfer` 必须拆成子能力矩阵。
  - AC-4: 每项测试都说明证据要求（截图/录屏/日志/配置快照）；`Explorer / Transfer` 子能力项要求 API 返回与 Web 页面双证据。
  - AC-5: 文档与主测试手册、启动器专项审查文档互链。
- Non-Goals:
  - 不替代 `oasis7_game_launcher` / `oasis7_web_launcher` / `oasis7_client_launcher` 的自动化测试。
  - 不在本专题中扩展性能压测与长稳 soak 细则。
  - 不把本清单视为 UI 设计规范文档。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用；本专题聚焦人工测试执行，不要求新增 AI 模型策略。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 清单以 `oasis7_game_launcher` 与 `oasis7_client_launcher` 为主要被测对象，辅以 `oasis7_web_launcher` 控制面、`testing-manual.md` 分层口径和现有 Playwright/Web 审查结论，形成“人工体验验收 + 自动化补充”的双轨门禁；其中 Web 闭环默认优先使用 GUI Agent 驱动产品动作，再由浏览器校验状态与展示结果。
- Integration Points:
  - `crates/oasis7_launcher_backend/src/bin/oasis7_game_launcher.rs`
  - `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher.rs`
  - `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/gui_agent_api.rs`
  - `crates/oasis7_client_launcher/src/main.rs`
  - PR、实际 CI 与评审记录（Issue 按需） and git history for retired launcher full-usability closure audit samples
  - `doc/testing/manual/web-ui-agent-browser-closure-manual.prd.md`
  - `testing-manual.md`
  - `scripts/run-local-letai-game-test.sh`（本地真实 LetAI provider-backed 试玩）
  - `scripts/run-producer-playtest.sh`（制作人 / 发布前人工验收 bundle-first 入口）
  - `scripts/build-game-launcher-bundle.sh`
  - `scripts/run-launcher-stack.sh`（底层 bootstrap / 高级排障入口，支持 `--bundle-dir` 复用 bundle；不作为普通人工试玩菜单）
  - 历史已删除：`scripts/viewer-release-qa-loop.sh`
- Edge Cases & Error Handling:
  - 首次启动缺少必要配置：必须出现可操作的引导，不得只显示裸错误。
  - 端口占用或依赖缺失：必须 fail-fast，且日志可定位。
  - 重复点击开始/停止：不得卡死、重复拉起子进程或产生不可恢复状态。
  - 远端链/反馈接口失败：必须提示失败原因，且保留重试路径。
  - 旧配置/坏配置文件：必须给出恢复方案，至少允许用户回到可编辑状态。
  - `--viewer-static-dir` / 环境变量静态目录不一致：必须记录实际使用来源并验证最终页面可用。
  - `Explorer` 查询不得只校验“页面打开了”；必须区分 `空数据正常`、`查不到但应能查到`、`字段错映射`、`只总览正常但明细错误`。
- Non-Functional Requirements:
  - NFR-LMTC-1: `P0` 人工 smoke 应在 30 分钟内完成首轮结论。
  - NFR-LMTC-2: 发布前 `P0 + P1` 证据必须可追溯到截图、日志或录屏。
  - NFR-LMTC-3: 文案、禁用态和恢复引导应能让非作者完成基本排障。
  - NFR-LMTC-4: 人工清单不得与现有自动化结论冲突；冲突时以更保守结论为准。
  - NFR-LMTC-5: `Explorer / Transfer` 细粒度用例必须支持重复执行，避免随机空数据导致假通过。
- Security & Privacy: 人工测试不得在截图、录屏、日志中泄露私钥、鉴权 token 或本地隐私目录。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP (LMTC-1): 建立专题规格、项目拆解与首版人工测试清单；专题完成后，本 PRD 保留当前执行口径，完成态拆解由 PR 与实际验证记录 与 Git 历史承接。
  - v1.1 (LMTC-2): 对齐 `P0/P1/P2`、证据要求与发布 verdict 口径。
  - v2.0 (LMTC-3): 与主测试手册、启动器审查专题建立互链并收口。
  - v2.1 (LMTC-4): 后续按逃逸缺陷持续补充清单项。
  - v2.2 (LMTC-5): 将 `Explorer / Transfer` 升级为子能力矩阵，补齐前置数据、结果分级和归因入口。
- Technical Risks:
  - 风险-1: 若清单只列 happy path，仍会遗漏恢复链路问题。
  - 风险-2: 若不区分 `P0/P1/P2`，执行成本会高到难以持续。
  - 风险-3: 若没有证据要求，结论容易主观化、无法复盘。
  - 风险-4: 若不先造数再校验 `Explorer / Transfer`，容易把“空结果”误判成“接口通过”。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-LAUNCHER-MANUAL-001 | LMTC-1/2/5 | `test_tier_required` | 清单结构、子能力矩阵与优先级口径审阅 + 主手册互链检查 | 启动器人工测试执行一致性 |
| PRD-TESTING-LAUNCHER-MANUAL-002 | LMTC-2/3/5 | `test_tier_required` | 覆盖项对照启动器现有能力面、实际 GUI Agent 闭环结果与专项审查文档 | 核心链路、Explorer/Transfer 子能力与异常恢复验收 |
| PRD-TESTING-LAUNCHER-MANUAL-003 | LMTC-2/3/4/5 | `test_tier_required` | 证据模板、结果分级、文档治理与失败归因入口检查 | 发布结论可追溯性 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-LMTC-001 | 清单按 `P0/P1/P2` 分层 | 平铺所有用例 | 分层更适合发布前快速裁剪与执行。 |
| DEC-LMTC-002 | 人工清单聚焦“自动化薄弱区” | 重复罗列已有单测能力 | 避免重复投入，突出人工价值。 |
| DEC-LMTC-003 | 每项必须要求证据 | 仅记录口头结论 | 保证复盘、阻断和回归可追溯。 |
| DEC-LMTC-004 | Web 闭环优先使用 GUI Agent 驱动，再用浏览器验状态 | 直接依赖 Canvas 点击自动化 | GUI Agent 更稳定，更适合重复执行与问题归因。 |
| DEC-LMTC-005 | `Explorer / Transfer` 使用“先造数、再查数、再校验字段”的细粒度方法 | 仅校验总览页是否打开 | 可以显著降低假通过，缩小问题定位范围。 |

## 7. 人工测试清单（执行版）

### 7.1 执行说明
- 发布结论枚举：`pass` / `conditional` / `fail` / `blocked`
- 细粒度执行结果：`pass_with_data` / `pass_empty_expected` / `fail_wrong_data` / `fail_not_found_unexpected` / `blocked_env`
- 优先级：`P0 必测` / `P1 应测` / `P2 选测`
- 证据最小集：至少包含 1 份截图或录屏，必要时附 launcher 日志、配置快照、错误文案。
- 本地试玩入口按 `2 + 1` 口径执行：本地真实 LetAI provider-backed 试玩默认执行 `./scripts/run-local-letai-game-test.sh`；制作人试玩 / 发布前人工验收默认执行 `./scripts/run-producer-playtest.sh --open-headed`（脚本退出时会自动关闭该浏览器会话；默认会用 `--use-angle=gl,--ignore-gpu-blocklist` 固定硬件 WebGL 路径，若 renderer 仍为 `SwiftShader` / software renderer 则按环境阻断处理；若本地 bundle 缺少 freshness manifest 或已落后于当前工作区源码，则该入口会先自动重建）；QA / subagent evidence 使用 `./scripts/worktree-harness.sh up`，再用 `GAME_URL="$(./scripts/worktree-harness.sh url)"` 捕获 URL 后按需执行 `./scripts/run-game-test-ab.sh --url "$GAME_URL"`，不作为普通人工试玩入口。
- `scripts/build-game-launcher-bundle.sh`、`<bundle>/run-game.sh`、`./scripts/run-launcher-stack.sh --bundle-dir <bundle>` 只保留给手动控制 bundle、底层排障或专项回归；未传 `--bundle-dir` 的源码模式仅用于开发回归和问题复现。
- `Explorer / Transfer` 默认控制链路：优先使用 GUI Agent 或等价稳定接口驱动动作，再由 Web 页面与原始返回做双证据核验。
- 建议记录字段：`Case ID`、执行环境、构建版本、执行人、结果、证据路径、缺陷 ID、备注。

### 7.2 细粒度方法论补充
- 子能力拆分：不要把 `Explorer` 或 `Transfer` 当成单个功能点；每个查询、明细页、搜索命中类型、状态迁移都单独记 case。
- 数据前置：先明确当前是 `无数据场景`、`已造单笔数据场景` 还是 `多笔/混合场景`，再执行查询。
- 双证据：同一 case 至少保留 1 份 API / GUI Agent 原始返回和 1 份 Web 页面截图，防止只看 UI 无法归因。
- 字段断言：涉及链上数据时，至少核验 `height / hash / tx_hash / action_id / account_id / amount / status` 中与场景相关的字段。
- 状态链断言：除最终态外，优先记录 `loading / empty / success / not_found / error / recoverable` 中间态。
- 失败归因入口：执行记录中显式标注“更像前端展示、路由参数、GUI Agent action、Explorer API、数据索引”哪一层异常。

### 7.3 P0 必测（发布前最低门槛）
| Case ID | 检查项 | 建议步骤 | 预期结果 | 证据 |
| --- | --- | --- | --- | --- |
| P0-01 | 冷启动可用 | 从干净环境启动 launcher | 能在合理时间内打开窗口/页面并进入 ready | 首屏截图 + 启动日志 |
| P0-02 | 首次启动默认态 | 不做任何操作观察主界面 | 关键区块（状态、按钮、配置区）可见且无明显错位 | 首屏截图 |
| P0-03 | 缺必填配置引导 | 清空必要配置后点击开始 | 出现明确禁用态或引导，不允许静默失败 | 引导截图 |
| P0-04 | 修复配置后恢复 | 按引导补齐配置再尝试启动 | 禁用态解除，允许继续操作 | 前后对比截图 |
| P0-05 | 游戏启动 happy path | 配置有效后点击开始游戏 | 状态从未启动进入运行中，日志可见 | 运行态截图 + 日志 |
| P0-06 | 游戏停止 happy path | 游戏运行后点击停止 | 状态回到已停止/未启动，无残留错误 | 停止态截图 |
| P0-07 | 链启动 happy path | 启动链运行时 | 链状态进入 ready/running，可继续后续操作 | 运行态截图 |
| P0-08 | 链停止 happy path | 链运行后点击停止 | 状态正确回退，不残留僵尸状态 | 停止态截图 |
| P0-09 | 重复点击防抖 | 快速重复点击开始/停止 | 不重复启动、不锁死、最终状态一致 | 录屏或连续截图 |
| P0-10 | 端口占用失败提示 | 人为占用关键端口后启动 | fail-fast 且错误信息可定位端口问题 | 错误截图 + 日志 |
| P0-11 | 依赖缺失/路径错误 | 指向错误二进制或静态目录 | 清晰报错并可返回编辑态 | 错误截图 |
| P0-12 | 静态资源实际可用 | 验证当前静态目录来源并打开页面 | 页面资源加载完整，无明显白屏/协议错配 | 页面截图 + 配置记录 |
| P0-13 | 日志可读性 | 制造一次失败并查看日志区/日志文件 | 能定位失败阶段、原因和下一步动作 | 日志摘录 |
| P0-14 | 恢复能力 | 失败后修复环境再重试 | 无需重装即可恢复到可运行状态 | 前后结果记录 |
| P0-15 | 关闭后再次启动 | 完整关闭 launcher 再次打开 | 第二次启动行为与首次一致，无脏状态继承 | 二次启动截图 |

### 7.4 P1 应测（候选发布建议补齐）
| Case ID | 检查项 | 建议步骤 | 预期结果 | 证据 |
| --- | --- | --- | --- | --- |
| P1-01 | Explorer 总览可达 | 进入 Explorer 首页 | 首页可加载，关键字段有值或空态明确 | 截图 |
| P1-02 | Explorer 搜索总入口 | 输入区块/交易/地址关键词 | 能区分命中、空结果、非法输入三种状态 | 截图 |
| P1-03 | Transfer 表单校验 | 输入空值、负值、非法值 | 出现表单校验提示，不发送无效请求 | 截图 |
| P1-04 | Transfer happy path | 使用有效账户执行一次转账 | 生命周期状态推进正确，可见 accepted/final 等结果 | 截图 + 状态记录 |
| P1-05 | Transfer 失败路径 | 制造余额不足/nonce 异常/接口失败 | 明确提示失败原因，可再次重试 | 错误截图 |
| P1-06 | Feedback 提交成功 | 填写有效反馈并提交 | 成功提示明确，本地/远端路径符合预期 | 提交结果截图 |
| P1-07 | Feedback 提交失败兜底 | 模拟远端失败 | 至少保留本地记录或明确失败提示 | 结果截图 + 本地文件路径 |
| P1-08 | LLM Settings 保存恢复 | 修改设置后关闭重开 | 配置可保存并恢复，坏值有校验 | 前后截图 |
| P1-09 | 自引导/Startup Guide | 触发新手引导或阻断 CTA | 引导顺序合理，能帮助用户完成修复 | 录屏或截图 |
| P1-10 | 文案与状态本地化 | 切换中英文或检查默认文案 | 核心状态文案一致、无明显占位符/错语 | 截图 |
| P1-11 | 窗口尺寸适配 | 在常见分辨率/缩放下使用 | 关键按钮、表单、日志区不遮挡 | 多分辨率截图 |
| P1-12 | 断网/服务不可达 | 运行中断网或停远端服务 | 状态变化可理解，恢复后可继续操作 | 录屏 + 日志 |

### 7.5 Explorer / Transfer 细粒度子能力矩阵

#### 7.5.1 数据前置场景
| Data Set ID | 场景 | 造数方式 | 最低断言 |
| --- | --- | --- | --- |
| DS-EMPTY-001 | 空链/空历史 | 不执行任何 transfer | `overview` 可达；空态文案正确 |
| DS-TX-001 | 单笔已知 transfer | 通过 GUI Agent 提交 1 笔可追踪 transfer | 可拿到 `action_id`、`tx_hash` 或等价标识 |
| DS-TX-002 | 多笔 transfer | 连续提交多笔 transfer | 列表顺序、筛选与搜索命中不串线 |

#### 7.5.2 Explorer 子能力矩阵
| Case ID | 子能力 | 数据前置 | 建议步骤 | 预期结果 | 结果分级 | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| EXP-01 | `overview` 总览 | `DS-EMPTY-001` 或 `DS-TX-001` | 打开总览页并记录关键字段 | 页面可达；字段有值或空态说明一致 | `pass_with_data` / `pass_empty_expected` | API 返回 + 页面截图 |
| EXP-02 | `blocks` 列表 | `DS-TX-001` | 打开区块列表并记录最新高度/哈希 | 列表非空，最新区块字段可见 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| EXP-03 | `block detail by height` | `DS-TX-001` | 用已知高度查询区块详情 | 能命中对应区块；字段与列表/总览一致 | `pass_with_data` / `fail_not_found_unexpected` | API 返回 + 页面截图 |
| EXP-04 | `block detail by hash` | `DS-TX-001` | 用已知哈希查询区块详情 | 能命中对应区块；高度/哈希一致 | `pass_with_data` / `fail_not_found_unexpected` | API 返回 + 页面截图 |
| EXP-05 | `transactions` 列表 | `DS-TX-001` | 打开交易列表并核对最新 transfer | 列表含目标交易，排序合理 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| EXP-06 | `transaction detail` | `DS-TX-001` | 用 `action_id`、`tx_hash` 或等价键查详情 | 明细字段与 transfer 提交结果一致 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| EXP-07 | `address detail` | `DS-TX-001` | 查询发送方与接收方地址 | 双方余额/历史/关联交易可解释 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| EXP-08 | `search` 命中类型 | `DS-TX-001` | 分别搜索地址、交易标识、区块标识 | 命中类型正确，不串到错误详情页 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| EXP-09 | `mempool` 空态/有态 | `DS-EMPTY-001` 或提交后瞬时 | 观察 mempool 结果 | 空态有提示；有态时记录到交易痕迹 | `pass_with_data` / `pass_empty_expected` | API 返回 + 页面截图 |
| EXP-10 | `not_found` 语义 | 任意 | 输入格式合法但不存在的高度/哈希 | 明确提示 not found，而非静默空白或错误详情 | `pass_empty_expected` / `fail_wrong_data` | API 返回 + 页面截图 |
| EXP-11 | 非法输入语义 | 任意 | 输入非法高度/非法哈希格式 | 返回参数错误或可理解提示，不进入错误详情态 | `pass_empty_expected` / `fail_wrong_data` | API 返回 + 页面截图 |

#### 7.5.3 Transfer 子能力矩阵
| Case ID | 子能力 | 数据前置 | 建议步骤 | 预期结果 | 结果分级 | 证据 |
| --- | --- | --- | --- | --- | --- | --- |
| TX-01 | 表单合法性校验 | 任意 | 输入空值、负值、非法账户 | 前端阻断或明确报错，不发送脏请求 | `pass_empty_expected` / `fail_wrong_data` | 请求日志 + 页面截图 |
| TX-02 | 提交返回语义 | `DS-EMPTY-001` | 提交单笔 transfer | 返回含 `action_id`、状态或等价追踪键 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| TX-03 | 状态推进 | `DS-TX-001` | 轮询 transfer 状态 | 能看到 `pending/accepted/final` 或等价推进，不倒退乱跳 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| TX-04 | 历史列表 | `DS-TX-001` | 打开 transfer history | 最新记录出现且字段与提交结果一致 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| TX-05 | 失败路径可恢复 | 制造余额不足或 nonce 错误 | 提交失败后修正参数再提交 | 错误原因清晰，修复后可成功再次提交 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |
| TX-06 | Explorer 反查闭环 | `DS-TX-001` | 从 transfer 结果反查 explorer | transfer 与 explorer 中的交易标识、地址、金额一致 | `pass_with_data` / `fail_wrong_data` | API 返回 + 页面截图 |

### 7.6 P2 选测（专项或回归增强）
| Case ID | 检查项 | 建议步骤 | 预期结果 | 证据 |
| --- | --- | --- | --- | --- |
| P2-01 | 升级覆盖旧配置 | 使用旧版本残留配置启动新版本 | 兼容成功或给出明确迁移/清理提示 | 截图 |
| P2-02 | 多平台抽样 | 在目标 OS/浏览器矩阵抽样执行 `P0` | 关键链路结论一致 | 平台记录 |
| P2-03 | 长时间保持开启 | 保持 launcher 空闲/运行一段时间 | 无明显卡死、日志爆炸或状态漂移 | 观察记录 |
| P2-04 | 高频切换功能页 | 快速切换 Explorer/Transfer/Feedback 等页签 | 不出现明显状态串线或残留 | 录屏 |
| P2-05 | 弱网/高延迟体验 | 注入慢网或高延迟 | Loading、超时、重试提示合理 | 录屏 |
| P2-06 | 可访问性与易用性 | 检查键盘可达、焦点、颜色对比、错误提示位置 | 基础可达性无明显阻断 | 截图 + 备注 |

### 7.7 结果判定建议
- `pass`：`P0` 全通过，且无会影响发布的 `P1` 高风险问题；`Explorer / Transfer` 关键子能力不存在 `fail_wrong_data` 或 `fail_not_found_unexpected`。
- `conditional`：`P0` 全通过，但存在可绕行的 `P1/P2` 风险，或子能力仅在特定数据/环境下失败，需要附带限制条件发布。
- `fail`：任一 `P0` 失败，或存在数据安全/恢复失败/主链路不可用问题，或 `Explorer / Transfer` 出现关键字段错误、查不到已知数据、状态推进错误。
- `blocked`：环境、构建、依赖或外部条件不满足，当前轮次无法对产品结论负责。

## 原文约束点映射（内容保真）
- 用户诉求“从传统互联网产品人工测试视角给启动器做一份清单” -> 第 1 章 Problem/Solution/SC。
- 原始清单起草请求“先不要提交，只先产出清单”是该次请求的授权边界，保留来源；后续提交仍须由对应任务授权，本文不授予提交、合入或发布许可。
- 前序结论“启动器已有自动化，但流程级人工覆盖仍需补位” -> 第 2 章 Flow + 第 7 章人工测试清单。
- 本轮新增结论“当前方法论过粗，需补细粒度缺陷拦截” -> 第 7.2 / 7.5 章。
