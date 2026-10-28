# Viewer Web Build Truth and Legacy Core Split 设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-web-build-truth.prd.md`
- 未完成的 facade 拆分债务见 `viewer-frontend-structure-standard.prd.md#当前已登记的结构债务`；具体 task 状态由 GitHub task issue evidence 维护。

审计轮次: 1

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [配对 PRD](viewer-web-build-truth.prd.md#41-legacy-core-split-boundary) | 保留 legacy_core 稳定 import/export 与初始化合同，主要实现按职责下沉；AC-1 保持部分完成。 | [本地条款](viewer-web-build-truth.design.md#模块拆分) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-web-build-truth.prd.md#42-generated-artifact-single-source-of-truth) | canonical HTML/claim fixture 生成 dist compat；viewer.js 为唯一 canonical bundle，finalize 单写入。 | [本地条款](viewer-web-build-truth.design.md#canonical-to-alias-generation) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-web-build-truth.prd.md#43-script-and-bundle-contract) | copy 使用 canonical root HTML/bundle 与生成的 dist aliases，不能从 compat 反写 canonical。 | [本地条款](viewer-web-build-truth.design.md#dist-copy-contract) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |
| [配对 PRD](viewer-web-build-truth.prd.md#43-script-and-bundle-contract) | freshness scope 含 canonical viewer.js；stale rebuild 后保留 compat 和 runtime 路径，失败显式报告。 | [本地条款](viewer-web-build-truth.design.md#browser-rebuild-fallback) | Viewer engineering；涉及行为时由 QA 按影响面选择验证。 | 产品 N/A；reason: 此工程机制本身不新增玩家义务；scope: 本行源码结构或生成资产维护合同，不免除另有产品合同的 UI、模式与体验验收。 |

## 1. 目标与边界

收口 canonical→compat 的产物来源并渐进拆分 legacy facade；不变更 UI、运行协议、WASM ABI 或 Hosted auth。此处分配 §4.1–4.3 的源码/产物工程合同；配对 PRD §4.4 的 native/WASM graph、资源 fallback、unsupported action 以及 Chat 输入恢复义务继续有效，本设计的 alias/copy fixture 不声称覆盖这些行为要求，须由各 owning 专题与适用 build/feedback/UI 验收承接。

<a id="设计概览"></a>
## 3. 设计结构与概览
- 保留 `legacy_core.js` 作为稳定 facade import path，但将主要实现拆到 `software_safe_src/` 新子模块。
- `vite` / finalize 流改为产出 canonical `viewer.js`，并由 finalize 从 canonical inputs 生成 dist compat `software_safe.js` 与 evidence page。
- `dist/pixel-world-bridge/` 继续由 finalize flow 统一生成；dist / bundle / freshness helper 全部基于 canonical 产物复制 compat alias，而不是反向复制。浏览器运行时仍以 served-dist 相对路径 `./pixel-world-bridge/` 加载。

## 模块拆分
- `legacy_core.js`
  - 目标：只负责组装导出面、调用子模块 factory、保留兼容入口。
  - 当前 AC-1 仍为部分完成：源码仍含 control、semantic command、DOM rendering 与 bootstrap；保留已有 facade 债务，不把此目标写成完成态。
- `software_safe_src/*_module.js`
  - 按 state/auth/gameplay/rendering 等职责拆分实现。
  - 允许使用 factory + dependency injection，避免 ESM 循环引用。

## 产物关系
- Canonical:
  - `viewer.html` 源码页面文件
  - `viewer.js` 生成 bundle
  - `dist/pixel-world-bridge/*` 生成 runtime
- Compat alias:
<a id="canonical-to-alias-generation"></a>
<a id="viewer-html-dist-compat"></a>
  - dist `software_safe.js`
  - 从 canonical 页面生成到 dist / bundle 的 `software_safe.html`
  - 从 canonical claim fixture 生成到 dist 的 `software_safe_first_agent_claim_evidence.html`

<a id="dist-copy-contract"></a>
## 脚本改造
- `vite.software-safe.config.mjs`
  - 产出 canonical `viewer.js`
- `finalize-software-safe-build.mjs`
  - 将 canonical `viewer.html` 复制到 dist，并在 dist 中生成 compat `software_safe.html`
  - 复制 canonical bundle 到 `viewer.js`
  - 在 dist 中生成 compat `software_safe.js` 与 claim evidence page；不在源码根目录保留 aliases
  - 继续生成 `dist/pixel-world-bridge/*`
- `copy-viewer-web-dist.sh` / `package-viewer-web-delivery.sh`
  - 将 finalize 生成在 dist 的 compat aliases 纳入 copy 与发布产物
- `run-viewer-web.sh` / `build-game-launcher-bundle.sh`
  - 继续从 fresh dist 提供 canonical 页面和 compat URL
<a id="browser-rebuild-fallback"></a>
- `agent-browser-lib.sh`
  - stale-dist fallback 在 finalize 后从 dist 复制 compat aliases
- `bundle-freshness-lib.sh`
  - 把 canonical `viewer.js` 纳入 freshness scope

<a id="风险控制"></a>
## 10. 风险与权衡
- 先保留 facade import path，避免一次性改动 `main.jsx` / `pixel_world_host.jsx` / tests 的所有 import。
- compat alias 改成显式 wrapper，避免 alias 文件重新生成时产生第二份 bundle 真值。
- 不在本轮修改 `viewer` / `software_safe` taxonomy，只收口生成产物和源码边界。

## 11. 验证与证据

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source or ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [配对 PRD](viewer-web-build-truth.prd.md#41-legacy-core-split-boundary) | [本地条款](viewer-web-build-truth.design.md#模块拆分) | 保留 legacy_core 稳定 import/export 与初始化合同，主要实现按职责下沉；AC-1 保持部分完成。 | [S6 验证选择](../../../testing-manual.md#testing-s6-qa-receiver)；人工对照导出/调用者与职责边界，按受影响模块选择 test:ui；alias fixture 不证明 facade 已完成。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-web-build-truth.prd.md#42-generated-artifact-single-source-of-truth) | [本地条款](viewer-web-build-truth.design.md#canonical-to-alias-generation) | canonical HTML/claim fixture 生成 dist compat；viewer.js 为唯一 canonical bundle，finalize 单写入。 | 运行 [alias fixture](../../../crates/oasis7_viewer/scripts/viewer-compat-aliases.test.mjs)：HTML 字节相等、JS import、claim route/字段与无 root copies。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-web-build-truth.prd.md#43-script-and-bundle-contract) | [本地条款](viewer-web-build-truth.design.md#dist-copy-contract) | copy 使用 canonical root HTML/bundle 与生成的 dist aliases，不能从 compat 反写 canonical。 | 运行 [copy fixture](../../../scripts/copy-viewer-web-dist.test.sh)，分别核对 canonical 与 dist alias 输入及复制结果。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
| [配对 PRD](viewer-web-build-truth.prd.md#43-script-and-bundle-contract) | [本地条款](viewer-web-build-truth.design.md#browser-rebuild-fallback) | freshness scope 含 canonical viewer.js；stale rebuild 后保留 compat 和 runtime 路径，失败显式报告。 | 运行 [freshness fixture](../../../scripts/agent-browser-viewer-dist-freshness-test.sh)：stub npm rebuild、缺 bridge 重建、失败负例及输出 manifest；不是实际 Vite/WASM 构建。 | 同候选 diff、所选命令/结果与未执行项分别记录。 | 静态审读及窄 fixture 不证明完整 UI、真实浏览器、Hosted 登录或发布。 |
