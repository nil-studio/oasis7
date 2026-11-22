# Viewer Web Entry Compatibility

- 对应设计文档: `doc/world-simulator/viewer/viewer-web-entry-compatibility.design.md`
- 已完成的实施追溯见 GitHub task issue evidence comments 与 [固定版本归档](https://github.com/nil-studio/oasis7/blob/4d8c742a9f5eca75c02a4156678d71ccc7e87d53/.pm/github-project-sync/task-archive.jsonl)。

审计轮次: 3

## 目标
- 将 `viewer` 固定为当前仓库唯一正式 Web Viewer / UI 入口，并把 `software_safe` 降为兼容 alias。
- 清除退役第二 Viewer 工具链残留的双入口、兼容跳转和旧 QA 口径。
- 保证 launcher、手册、回归脚本与站点镜像围绕同一入口事实收口。

## 范围
- 范围内:
  - 发布产物 `viewer.html`（canonical）/ `software_safe.html`（compat）
  - 发布产物 `viewer.js`（canonical）/ `software_safe.js`（compat）
  - 源码入口 `viewer.html` / `software_safe_src/**`，以及由 canonical HTML / claim fixture 生成到 dist 的 compat HTML、bundle alias 与 evidence page
  - `scripts/run-viewer-web.sh`
  - `scripts/viewer-primary-web-entry-regression.sh`
  - `scripts/viewer-software-safe-step-regression*.sh`
  - `testing-manual.md`
  - `doc/world-simulator/viewer/viewer-manual*.md`
- 范围外:
  - 恢复任何退役第二 Viewer 代码、入口或静态资源。
  - 维护退役的视觉专项、截图、贴图或 theme 资产链路。
  - 扩大当前 Web Viewer 能力边界到本专题之外的新玩法。

## 接口 / 数据
- 入口 URL: `http://<host>:<port>/?ws=ws://<web-bind>`
- Launcher 输出: 必须显式注入 `render_mode=viewer`
- 静态入口: 源码中的 `viewer.html` 为 canonical；build 在 dist 中生成内容相同的 `viewer.html` 与 compat `software_safe.html`，两者均指向 canonical bundle `viewer.js`
- 构建契约: release 与显式测试产物均保持 `viewer.js` canonical / `software_safe.js` compat alias；release 不安装 `__AW_TEST__`，同一套测试状态与动作接口仅属于显式测试产物及其 opt-in，详见 [构建类型边界](#entry-build-profile-boundary)。
- 验证数据:
  - Web 入口 freshness 检查结果
  - `viewer-primary-web-entry-regression.sh` evidence
  - `viewer` 手册与站点镜像内容

## 里程碑
- M1: 删除退役第二 Viewer 代码、静态入口与相关脚本。
- M2: launcher / Web 启动 / regression 全量改为 `viewer` canonical 单入口，并保留 `software_safe` 兼容 alias。
- M3: 手册、PRD、project、testing 文档完成真值回写并通过治理校验。

## 风险
- 历史文档或审读清单继续引用已删除 3D 路径，导致 doc-governance 失败。
- launcher 或脚本残留旧 `render_mode=standard` 参数，造成入口分叉。
- `viewer` canonical 入口与兼容 alias 若在所选构建内漂移，会破坏入口兼容；将测试 API 无条件要求到 release 则会误导构建选择与回归判定。

## 1. Executive Summary
- Problem Statement: 仓库删除退役第二 Viewer 代码、静态入口与相关 QA 工具后，Web Viewer 只剩一条正式 UI 入口，但 canonical 名称长期停留在 `software_safe`，与当前产品定位不匹配。本专题需要把旧命名和兼容切换口径收口到单一事实。
- Proposed Solution: 将 `viewer` 固定为唯一正式 Web Viewer / UI 入口；Web 启动、launcher URL、manual 与 evidence 保持 canonical 入口及 `software_safe` 兼容 alias。正式 release 通过 UI 驱动，依赖 `__AW_TEST__` 的回归明确选择显式测试产物和 opt-in。
- Success Criteria:
  - SC-1: 默认 Web 入口始终落到 `viewer` canonical 入口；旧 `software_safe` 链接继续兼容。
  - SC-2: `viewer` 继续承接连接、观察、目标选择、prompt/chat/rollback、canonical gameplay summary 与 blocked/handoff surface。
  - SC-3: 活跃文档、脚本和 evidence 不再要求 `render_mode=standard` 或任何第二 Viewer 入口。
  - SC-4: canonical/compat 两入口在 release/test 两类构建中保持相同兼容边界，测试参数不能把 release 变成测试产物。

## 2. User Experience & Functionality
- In Scope:
  - `viewer.html`
  - dist-generated `software_safe.html`（compat）
  - dist-generated `software_safe.js`（compat）
  - dist-generated `software_safe_first_agent_claim_evidence.html`（compat evidence route）
  - `software_safe_src/**`
  - `scripts/run-viewer-web.sh`
  - `scripts/viewer-primary-web-entry-regression.sh`
  - `scripts/viewer-software-safe-step-regression*.sh`
  - `testing-manual.md`
  - `viewer-manual*.md`
- Out of Scope:
  - 不恢复退役第二 Viewer。
  - 不维护退役视觉专项 / screenshot / texture/theme 资产链路。

## 3. User Stories
- As a 玩家 / QA / 制作人, I want `viewer` to remain the only formal Web / UI entry, so that the browser path has one unambiguous source of truth.
- As a `viewer_engineer`, I want all Web-facing scripts and docs to point to `viewer` as the canonical name, so that no stale retired fallback contract or old naming ambiguity survives.

## 4. Technical Specifications
### 4.1 Entry Contract
- 默认 URL:
  - `http://<host>:<port>/?ws=ws://<web-bind>`
- `oasis7_game_launcher` 生成的游戏页 URL 必须显式注入 `render_mode=viewer`
- `viewer.html` 是源码的唯一 canonical 静态入口页面；build 将其复制到 dist 的 `viewer.html` 并生成兼容别名 `software_safe.html`、`software_safe.js`，同时由 canonical claim fixture 生成兼容 evidence route；源码 checkout 不保留这些 root copies

### 4.2 Capability Envelope
- 必须保留：
  - 连接状态
  - 世界摘要
  - 目标列表与详情
  - canonical gameplay summary
  - blocked / handoff surface
  - auth/bootstrap 下的 prompt/chat/rollback
  - 仅在显式测试构建和 opt-in 下提供的 `__AW_TEST__` 状态与动作契约；release 保留正式 UI 能力且不安装该 API
- 不再保留：
  - 第二 Viewer 跳转
  - 退役视觉专项入口
  - 退役视觉专项链路

### 4.3 Freshness
- source-tree Web 入口必须继续阻断 stale dist
- `run-viewer-web.sh` 继续在启动前重建 `viewer` canonical dist，并从源码 `viewer.html` 产出兼容 `software_safe.html`；源码 checkout 不保存该 alias 副本

<a id="entry-build-profile-boundary"></a>
### 4.4 构建类型与测试 API 边界

| Viewer 构建 | canonical / compat 入口 | `__AW_TEST__` | 使用与证明范围 |
| --- | --- | --- | --- |
| release | 保留 `viewer.html/viewer.js` 与 `software_safe.html/software_safe.js` alias | 不安装；即使 URL 带 `test_api=1` 也不安装。 | 默认正式 UI 与产品交付物；driver 使用受支持的玩家界面。 |
| 显式 `build:viewer:visual-test` | `.viewer-test-dist/` 内保留相同 canonical/compat 路由合同。 | 只有编译期启用测试能力且运行时明确 opt-in 才安装；未 opt-in 时不安装。 | 受限隔离自动化；记录构建与执行模式，其测试 API 结果不代签 release 体验或自主试玩。 |

标准运行时写法为 `test_api=1`；现有解析还接受去空白、转小写后的 `true/yes/on`，这些值都不改变编译期开关。alias 名称、`render_mode=software_safe` 或 freshness rebuild 同样不能选择或升级构建类型。dist copy 与 stale-dist fallback 必须维持已选构建，保留各自兼容文件，不用另一构建的 API 可用性掩盖缺失能力。

此处承接 [L4 入口兼容要求](../../testing/prd.md#l4-entry-compatibility)；L4 默认 release/UI 与 Hosted session 编排由 testing PRD/design 负责。基线 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056` 已有编译期 API 隔离，但 L4B runner 仍存在构建/驱动与登录前置缺口；本次只修正文档合同，运行修复与验证继续由 [#4438](https://github.com/eng-cc/oasis7/issues/4438) 跟踪。

## 5. Acceptance Criteria
- AC-1: `run-viewer-web.sh` 构建并服务 `viewer` canonical dist，同时保留 `software_safe` 兼容副本。
- AC-2: `viewer-primary-web-entry-regression.sh` 只验证默认入口与 `viewer` 契约，不再验证 `render_mode=standard`；对 `software_safe` 仅做兼容通过处理。
- AC-3: 运行态 `renderMode`、launcher URL、手册和发布产物都不再把 `software_safe` 当作 canonical UI 名称。
- AC-4: Viewer 手册与站点镜像不再引用已删除的退役视觉专项脚本。
- AC-5: 对同候选的 canonical/compat × release/test 四格分别检查：release 始终不安装测试 API；显式测试产物在 opt-in 开启时暴露同一接口集合与调用合同，关闭时不安装。
- AC-6: 构建、交付复制和 stale-dist rebuild 保持所选类型与全部兼容路径；不覆盖另一类产物或静默切换 profile 来使回归通过。
- AC-7: 本 PRD、配套 design 与测试手册的构建/driver/opt-in 口径一致；验收记录区分已核查的文档、实际产物与真实浏览器运行，缺失实现不得写成通过。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_SIMULATOR-039 | T0-T6 | `test_tier_required` | `./scripts/doc-governance-check.sh` + browser regression + 残留 grep | Web 入口、脚本与文档真值 |
| PRD-WORLD_SIMULATOR-039 / AC-5..7 | #4438 | 文档静态核对；实现后按所选构建验证 | [L4-C05 构建/入口四格矩阵](../../../testing-manual.md#l4-toolchain-validation) | release/test、canonical/compat、opt-in 与文档一致性；计划不等于已执行 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| `DEC-WS-039-1` | `viewer` 成为唯一 canonical Web / UI 入口，`software_safe` 仅保留兼容 alias | 保留已删实现的第二 Viewer 口径，或继续让 `software_safe` 作为 canonical 名称 | 当前仓库已无第二条正式 UI 入口，实现与公开命名必须一致。 |
| `DEC-WS-039-2` | 同一入口兼容合同覆盖 release/test，测试 API 仅属于显式测试产物和 opt-in | 由 `test_api` URL 参数把正式发布物变成测试构建 | 与当前编译期隔离一致，并使 release UI 证据和受限测试证据可分别验收。 |
