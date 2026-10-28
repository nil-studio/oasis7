# world-simulator 文档索引

产品层归属：`world-simulator` 为“智能体、世界模拟与交互”提供主专业域权威，并为“玩家接入与发行”提供 Viewer / Launcher 专业域契约，不形成并列产品入口。产品总导航见 `doc/product/README.md`。

审计轮次: 9

## 从这里开始
- 想先回答 world-simulator 在做什么、覆盖哪些边界：`doc/world-simulator/prd.md`
- 想看当前执行任务、负责人、测试层级与最新完成态：对应 GitHub task issue / Project。
- 想确认 Viewer / player-facing surface 的统一视觉设计规范：`doc/world-simulator/viewer/viewer-visual-design-spec-2026-06-05.design.md`；涉及 brand book、token、icon/status、资产准入和截图矩阵时读 `doc/world-simulator/viewer/viewer-brand-system-2026-06-05.design.md`
- 想按问题进入 Viewer 热点子域：`doc/world-simulator/viewer/README.md`
- 想按问题进入 Launcher 热点子域：`doc/world-simulator/launcher/README.md`
- 想先进入场景初始化、seed/location 或 asteroid-fragment 主题，而不是在场景专题三件套中逐项找：`doc/world-simulator/scenario/README.md`
- 想执行 Viewer、走 Web 闭环或查操作步骤：`doc/world-simulator/viewer/viewer-manual.manual.md`
- 想按子域或文件名继续查找：`doc/world-simulator/prd.index.md`
- 想给仓库外读者分享公开可读手册：`site/doc/cn/viewer-manual.html` / `site/doc/en/viewer-manual.html`

## 入口
- PRD: `doc/world-simulator/prd.md`
- 设计总览: `doc/world-simulator/design.md`
- 当前任务入口: 对应 GitHub task issue / Project
- 文件级索引: `doc/world-simulator/prd.index.md`

## 入口分工
- `README.md` 只承担 landing page 职责：告诉读者先去哪个权威入口，不重复长表索引内容。
- `prd.md` 是模块权威规格入口，适合先理解 world-simulator 的范围、主线能力与跨模块接口。
- GitHub task issue / Project 是执行台账，适合确认当前活跃任务、测试层级、阻断与最新完成项；Git history 保留历史过程。
- `prd.index.md` 是定向检索索引，适合已经知道主题后按文件名查找，不是新读者的首读入口。
- `viewer/README.md` 是 `viewer/` 热点子域的 landing page，负责把 Viewer 专题按问题分流。
- `launcher/README.md` 是 `launcher/` 热点子域的 landing page，负责把启动器、控制面、explorer、runtime 边界与自引导体验按问题分流。
- `scenario/README.md` 是 `scenario/` 子域的 landing page，负责把场景文件、world initialization、seed/location、资源生成与 asteroid-fragment 专题按问题分流。
- `viewer/viewer-visual-design-spec-2026-06-05.design.md` 是 Viewer / player-facing surface 的 canonical 视觉设计规范，负责视觉方向、层级、pixel-world 可读性与视觉评审 gate。
- `viewer/viewer-brand-system-2026-06-05.design.md` 是 Viewer 视觉规范的 brand-system companion，负责 brand book、语义 token、icon/status vocabulary、资产语言与扩展截图矩阵。
- `viewer/viewer-manual.manual.md` 是仓库内 canonical 操作手册；静态 `site/doc/**/viewer-manual.html` 仅作为公开只读镜像，不反向替代仓库权威源。

## 活跃阅读面边界
- 当前页只保留 `what / where / next / risk` 所需入口，不再直接罗列近期专题长名单。
- 高频 active 入口保留在 `prd.md`、`viewer-manual.manual.md` 和 `prd.index.md`。
- 审计留痕、历史背景或只在特定问题下才需要的专题，改为从 `prd.index.md` 的定向检索入口进入。
- 历史审读清单与 core review round logs 仅作为 archive-only evidence，用于审计追溯；日常阅读路径从 `README.md`、`prd.md`、Git、PR 与实际 CI 记录、`prd.index.md` 进入。

## 模块职责
- 维护模拟世界主入口、viewer / launcher / kernel / scenario / llm / m4 六大主题口径。
- 汇总 Web 闭环、启动器可用性、场景初始化与规则执行相关专题。
- 承接 world-simulator 与 runtime / viewer / testing 的跨模块体验收口。

## 热点子域导航
- `viewer/`：Viewer、Web 闭环、`software_safe`、2D/3D 与操作手册；先看 `viewer/README.md`，再按簇进入 `viewer-manual.manual.md` 或具体专题。
- `launcher/`：启动器、控制面、转账、explorer 与自引导体验；先看 `launcher/README.md`，再按簇进入 release/distribution、control plane、explorer、runtime 边界或 self-guided 专题。
- `llm/`：provider、loopback、本地桥接、体验等价和 direct-connect 相关口径。
- `kernel/`：规则桥接、WASM 执行、资源与 runtime 约束；已完成的 simulator rule-hook / Wasm 接线历史收敛到模块 `design.md`，当前 WASM ABI、执行器、构建与工件治理继续下钻 `doc/world-runtime/wasm/`。
- `m4/`：M4 方案与配套设计。
- `scenario/`：场景初始化、配置与模板；先读 `scenario/README.md`，再按问题进入专题。
- `prd/`：验收模板、质量趋势与补充附件。

## 高密度提示
- 本页不维护容易漂移的文件数量快照；当前模块库存与热点子目录统一以 `./scripts/doc-inventory-report.sh` 为准。
- 模块级入口只负责引导至 `viewer/README.md` 或 `launcher/README.md`，不再在根入口直接承担子域分流。
- 需要完整活跃专题清单时，进入 `doc/world-simulator/prd.index.md`；需要历史回溯时，再按具体任务或专题路径进入。

## 根目录 legacy
- root world-simulator legacy redirect shells 已删除；当前技术入口为 `doc/world-simulator/prd.md` 与 `doc/world-simulator/prd.index.md`，任务状态以 GitHub 为准。

## 共享约定
- 模块根入口、专题落位与 README/legacy redirect 的共享规则统一以 `doc/engineering/doc-governance/doc-structure-standard.design.md` 为准。
- 模块行为、Web 闭环或启动器体验口径变化时，优先更新 `doc/world-simulator/prd.md` 与对应专题 PRD/design；任务状态和验收过程记录到 GitHub task issue。手册步骤改动时，先更新仓库内 `viewer-manual.manual.md`，再评估是否同步静态镜像。
