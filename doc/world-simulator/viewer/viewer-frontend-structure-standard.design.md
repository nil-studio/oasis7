# Viewer Frontend Structure Standard Design

- 对应需求文档: `doc/world-simulator/viewer/viewer-frontend-structure-standard.prd.md`
- 结构债务 task 与验证记录：GitHub task issue evidence。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#结构分层) | 按 app/pages/widgets/features/entities/shared 分工，import 从组合层指向复用层；目录按需要渐进引入。 | [本地条款](viewer-frontend-structure-standard.design.md#分层模型) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#拆分触发条件) | 生产 1200 / 测试 1600 行软阈值；触发后拆分、缩小或记录 owner/reason/nextTrigger 豁免；跨层与副作用混杂亦触发。 | [本地条款](viewer-frontend-structure-standard.design.md#split-trigger-contract) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#可接受拆分模式) | facade 只组装导出、compat wrapper、factory 与初始化委托，不接纳新业务职责。 | [本地条款](viewer-frontend-structure-standard.design.md#facade-策略) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#html-jsx-js-职责) | HTML shell、Solid JSX 展示和 JS helper/service 按职责分离；props 以 display inputs/callbacks 为界。 | [本地条款](viewer-frontend-structure-standard.design.md#组件策略) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#可接受拆分模式) | transport/auth/storage/runtime/metrics 归 owning service，浏览器 globals 和 storage keys 集中维护。 | [本地条款](viewer-frontend-structure-standard.design.md#state-service-策略) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#html-jsx-js-职责) | 抽取 helper/component 后保留行为断言；stable selector/DOM anchor 更名同步测试和 manual。 | [本地条款](viewer-frontend-structure-standard.design.md#test-策略) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#接口-数据) | 手写 source、canonical generated bundle、dist-only compat alias 与 runtime 分离。 | [本地条款](viewer-frontend-structure-standard.design.md#canonical-and-generated-artifacts) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [Build Truth 复制合同](viewer-web-build-truth.prd.md#43-script-and-bundle-contract) | 交付使用 canonical HTML/bundle 与 finalize 生成的 dist aliases，不消费 checkout root compat copies。 | [本地条款](viewer-frontend-structure-standard.design.md#dist-copy-contract) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |

审计轮次: 1

## 1. 目标与边界

让已有分层、facade、测试和生成资产义务可独立评审；只整理工程合同，不改变 UI、协议、WASM ABI、Hosted auth 或发布准入。

<a id="设计概览"></a>
## 3. 设计结构与概览
- 采用轻量 Feature-Sliced-inspired 分层作为 review 语言，不强制一次性目录迁移。
- 采用 Solid component model 作为 JSX 组件拆分基线：组件接收 display inputs 和 callbacks，避免在组件体内混入 transport、storage 和 generated artifact logic。
- 采用 Google HTML/CSS style-guide 风格作为 raw HTML/CSS 卫生底线：shell 清晰、语义稳定、格式一致。
- 采用 ESLint/Prettier 作为后续可自动化 gate 候选，但本轮只落文档标准，不新增 tooling。
- 继续以 `viewer-web-single-source-build-truth` 为 generated artifact 真值：canonical Viewer HTML/JS/claim fixture -> dist compat `software_safe.*`。

## 分层模型

```text
app
  pages
    widgets
      features
        entities
          shared
```

- `app` 负责 mount、query/bootstrap、top-level orchestration。
- `pages` 负责 Viewer entry composition。
- `widgets` 负责世界、目标、命令、诊断、聊天等屏幕区域。
- `features` 负责用户动作和可独立测试的交互能力。
- `entities` 负责 agent/location/resource/event/session/runtime-health display models。
- `shared` 负责纯 helper、constants、fixtures、storage adapters、browser capability helpers。

## 目录演进
当前 `software_safe_src/` 已经包含多个职责模块。新任务不需要先创建完整目录树；当同类文件超过 3 个或 import 边界开始泄漏时，再引入目录：

```text
software_safe_src/
  app/
  pages/
  widgets/
  features/
  entities/
  shared/
```

迁移时每次只抽一个 coherent boundary，并保留已有 import path 或提供明确 public API。

## Facade 策略
- `legacy_core.js` 可以继续作为稳定 facade，但不得继续吸收新职责。
- 可接受的 facade 内容：export assembly、compat wrapper、factory wiring、thin initialization delegation。
- 不可接受的 facade 内容：新 DOM rendering tree、新 transport loop、新 gameplay display model、新 generated artifact writer。

## 组件策略
- 组件名使用业务名词，不使用 `CommonThing`、`Panel2`、`WidgetNew` 这类临时名。
- 组件 props 应偏 display model，而不是 raw runtime snapshot 全量透传。
- 组件内可以做轻量 derived view state；复杂 derivation 下沉到 pure helper 或 entity display model。
- 大组件先按用户可见区域拆，再按数据/服务边界拆。

<a name="state-service-策略"></a>

## State / Service 策略
- WebSocket、hosted auth、storage、runtime loader、metrics、crypto、locale preference 属于 service/state module。
- service module 应优先暴露 factory 或小 public API，便于测试注入。
- Browser global 和 localStorage key 只能集中在 owning module 或 shared constants 中。

### 应用组合与状态发布

`viewer_application.js` 负责组合状态协调、展示订阅和应用启动的窄 API，
`legacy_core.js` 保留兼容 facade。transport/auth/quote/recovery 的状态、凭据与规则
仍由各 owning module 管理，不集中成通用 session/store。

- `publish` 自身只同步通知 registry，不写报价、回执或授权状态。订阅者仍可执行
  scope 变化后的安全清屏、锁定和既有受权只读恢复，不要求回调具有传递纯性。
- `coordinateAndPublish` 先调用领域协调再发布；旧 `render` / `requestRender`
  委托此路径。报价依赖协调仍在 snapshot 绑定更新之后及非 snapshot 的 auth/owner
  更新之后执行；snapshot 早期失效不能替代该路径。重复通知保持 fingerprint 幂等。
- registry 同步调用 primary，随后复制 subscribers 并按 Set 次序迭代；解绑影响后续
  调用，当前副本不变；允许同步重入，回调抛错直接传播并中止当前通知，不吞错或队列化。
- 应用保持 singleton。pending、成功或失败后重复 start 均返回原 Promise；失败被缓存，
  不重新 bootstrap。auth owner 既有 hello ACK 重试独立保留。
- 展示解绑只移除订阅/展示资源，保留应用 session、pending/unknown 原件与恢复路径；
  当前不提供 dispose、多 session 或由 presentation cleanup 发起的远程注销/release。

## Test 策略
- 抽出的 pure helper 需要 narrow unit test。
- 抽出的 component 需要 Solid UI test 或被现有 entry UI test 覆盖到 stable DOM anchor。
- 从大测试文件抽 fixture/helper 时，行为断言必须留在原测试或迁到同名 test 文件。
- 测试选择器是合同；更名要同步测试、manual 或 automation helper。

<a id="canonical-and-generated-artifacts"></a>
## Generated Artifact 策略
- 手写 source truth 只在 `software_safe_src/**`、canonical HTML shell、build/finalize scripts 和 docs 中维护。
- `viewer.js` 由 Vite/finalize 生成后作为 canonical checked-in bundle。
- `dist/software_safe.js` 是 compat alias，不承载独立实现；compat claim evidence page 由 canonical claim fixture 生成。
- `dist/pixel-world-bridge/**` 由 finalize flow 生成或同步，不作为普通 JS module 手改。

<a id="dist-copy-contract"></a>
- Delivery copy helpers consume generated dist aliases; checkout-root compatibility copies are not inputs.

<a id="split-trigger-contract"></a>
## 拆分触发与现有审计

生产源码 1200 行、测试源码 1600 行是软阈值。对 touched files 依 PRD 选择收敛或豁免；责任混杂、facade 新职责、生成逻辑旁路与 canonical/compat 方向不明也需人工审查。豁免必须有 owner、reason、nextTrigger，不能用机械切片逃避职责治理。

现有 `test:frontend-structure` 先运行 alias fixture，再运行 `scripts/frontend-structure-audit.mjs`；后者已有阈值、债务 cap/失效记录、canonical/compat 与 generated-runtime 检查。dist 不存在时跳过 runtime 分支不代表构建通过。ESLint/Prettier 仍是候选，不新增或声称启用这些 gate。当前债务以配对 PRD 所指 audit registry 为准，静态审读不宣称已清零。

## 外部规范适配
- Feature-Sliced Design: 采纳 layer/slice/public API 思想，不照搬完整目录和术语。
- Solid docs: 采纳组件、props、JSX 的框架语义。
- Google HTML/CSS style guide: 采纳 shell/HTML/CSS 卫生原则。
- ESLint/Prettier: 作为后续 gate 候选，不在本次 docs-only 任务新增依赖。
- Atomic Design: 仅作为视觉系统词汇参考，不作为代码架构主轴。
- Airbnb JS/React: 可参考 JS/JSX 命名和 lint 思路，不作为 Solid 主规范。

<a id="风险控制"></a>
## 10. 风险与权衡
- 不用阈值制造大爆改；阈值只在 touched files 上触发收敛或豁免记录。
- 不把 checked-in generated bundle 当作普通 source file 审查。
- 不用机械切片替代真实职责边界。
- 不让视觉 IA 文档承担代码结构治理职责；本标准只管 source/module/test/artifact 边界。

## 11. 验证与证据

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source or ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#结构分层) | [本地条款](viewer-frontend-structure-standard.design.md#分层模型) | 按 app/pages/widgets/features/entities/shared 分工，import 从组合层指向复用层；目录按需要渐进引入。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；人工审查 touched imports、公用 API 与责任归属；按抽取边界选 narrow test，不能用行数审计证明语义分层。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#拆分触发条件) | [本地条款](viewer-frontend-structure-standard.design.md#split-trigger-contract) | 生产 1200 / 测试 1600 行软阈值；触发后拆分、缩小或记录 owner/reason/nextTrigger 豁免；跨层与副作用混杂亦触发。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；运行 test:frontend-structure 并核对 registry/cap，人工逐项核对非行数触发条件。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#可接受拆分模式) | [本地条款](viewer-frontend-structure-standard.design.md#facade-策略) | facade 只组装导出、compat wrapper、factory 与初始化委托，不接纳新业务职责。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；核对公开导出及调用者，抽取后运行相应 test:ui/narrow tests；接口保持与职责分离须分别审查。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#html-jsx-js-职责) | [本地条款](viewer-frontend-structure-standard.design.md#组件策略) | HTML shell、Solid JSX 展示和 JS helper/service 按职责分离；props 以 display inputs/callbacks 为界。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；人工核对组件 props、markup 与 domain derivation，选择对应 component/entry 测试。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#可接受拆分模式) | [本地条款](viewer-frontend-structure-standard.design.md#state-service-策略) | transport/auth/storage/runtime/metrics 归 owning service，浏览器 globals 和 storage keys 集中维护。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；审读 owning module 与 factory 注入边界，选择实际受影响 service 测试；不改变 auth 业务语义。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#html-jsx-js-职责) | [本地条款](viewer-frontend-structure-standard.design.md#test-策略) | 抽取 helper/component 后保留行为断言；stable selector/DOM anchor 更名同步测试和 manual。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；逐一比较旧/新断言、DOM anchor 消费者及所选测试；可见 UI 改动另触发浏览器层。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-frontend-structure-standard.prd.md#接口-数据) | [本地条款](viewer-frontend-structure-standard.design.md#canonical-and-generated-artifacts) | 手写 source、canonical generated bundle、dist-only compat alias 与 runtime 分离。 | 运行 [alias fixture](../../../crates/oasis7_viewer/scripts/viewer-compat-aliases.test.mjs)，核对 HTML 相等、JS import、claim 字段/route 及 root alias 不存在；结构审计仅在 dist 存在时检查生成 runtime。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [Build Truth 复制合同](viewer-web-build-truth.prd.md#43-script-and-bundle-contract) | [本地条款](viewer-frontend-structure-standard.design.md#dist-copy-contract) | 交付使用 canonical HTML/bundle 与 finalize 生成的 dist aliases，不消费 checkout root compat copies。 | 运行 [copy fixture](../../../scripts/copy-viewer-web-dist.test.sh)，检查 canonical/compat/CSS、可选 WASM manifest 与 archive；不以 fixture 代表实部署。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
