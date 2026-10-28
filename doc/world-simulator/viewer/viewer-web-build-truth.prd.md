# Viewer Web Build Truth and Legacy Core Split

- 对应设计文档: `doc/world-simulator/viewer/viewer-web-build-truth.design.md`
- 未完成的 facade 拆分债务见 `viewer-frontend-structure-standard.prd.md#当前已登记的结构债务`；具体 task 状态由 GitHub task issue evidence 维护。
- 关联主专题:
  - `doc/world-simulator/viewer/viewer-page-module-design-2026-06-18.design.md`
  - `doc/world-simulator/viewer/viewer-pixel-world-bridge-rendering.prd.md`
  - `doc/world-simulator/viewer/viewer-web-entry-compatibility.prd.md`

审计轮次: 1

## 1. Executive Summary
- 当前 `crates/oasis7_viewer/software_safe_src/legacy_core.js` 同时承载状态、auth、反馈、控制发送、DOM 渲染与 bootstrap，多职责耦合导致主入口继续演进时改动半径过大。
- 当前 `viewer.js` 是 canonical 生成 bundle；`dist/software_safe.js` 是显式 `import "./viewer.js"` 的 generated compat alias。`build:viewer` 先构建 bundle，再由 finalize 从 canonical 页面与 claim fixture 写入 dist aliases 及 `dist/pixel-world-bridge/` 生成输入；不得再从 compat 文件反向复制 canonical bundle。
- 本专题要解决两类工程债：把 `legacy_core.js` 下沉成多模块实现；把 Web 生成产物收口为明确的单一真值流程，避免继续由 compat 名称承担 canonical 语义。

## 目标
- 将 `software_safe_src` 主入口实现从单个 `legacy_core.js` god module 拆成职责明确的子模块，同时保留现有对外 API 与测试入口。
- 将 Viewer Web 生成 bundle 的 canonical 真值收口到 `viewer.js`，把 `software_safe.js` 降级为兼容 alias，而不是反过来。
- 将 `dist/pixel-world-bridge/` 生成目录纳入与主 bundle 同一条显式 finalize 流，明确区分源码真值、generated artifacts 与 compat alias。

## 范围
- 范围内：
  - `crates/oasis7_viewer/software_safe_src/**`
  - `crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs`
  - `crates/oasis7_viewer/dist/software_safe.js`
  - `crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html`
  - `crates/oasis7_viewer/viewer.js`
  - `crates/oasis7_viewer/dist/pixel-world-bridge/**`
  - 直接消费上述文件的 Viewer Web dist / bundle / regression scripts
- 范围外：
  - runtime 协议、world DTO、Prompt / Chat / hosted access 业务语义变更
  - Pixel world bridge 的渲染行为、renderer unavailable 语义或 wasm ABI 调整
  - `viewer` / `software_safe` taxonomy rename 或 public copy 改版

## 2. User Experience & Functionality

## 3. User Stories
- As a `viewer_engineer`, I want the software-safe entry implementation split into multiple modules, so that evolving auth, rendering, or command surfaces no longer requires editing one 4k+ line file.
- As a `qa_engineer`, I want the Web bundle and pixel-world runtime artifacts to have one explicit finalize flow, so that repo-owned tests and bundle freshness checks can tell canonical assets from compat aliases.
- As a `producer_system_designer`, I want the viewer entry docs to match repo truth about `viewer.js` vs `software_safe.js`, so that the canonical browser entry no longer depends on an implementation mismatch.

## 4. Technical Specifications

### 4.1 Legacy Core Split Boundary
- `legacy_core.js` 保留为单入口 facade，允许继续作为 `main.jsx`、`pixel_world_host.jsx` 与现有测试的稳定 import path。
- 主实现必须下沉到 `software_safe_src/` 子模块，至少把以下职责拆开：
  - viewer state / locale / render hook / snapshot-derived utilities
  - auth / hosted access / session surface derivation
  - semantic feedback / gameplay summary / display model
  - DOM rendering / event binding / bootstrap composition
- 拆分过程中不得改动以下对外合同：
  - `initializeSoftwareSafeCore()`
  - `__AW_TEST__`
  - `state`
  - 现有 `main.jsx` / `pixel_world_host.jsx` / repo-owned tests 依赖的导出函数名

### 4.2 Generated Artifact Single Source of Truth
- `viewer.js` 必须成为仓库内 canonical Viewer Web bundle 名称。
- `dist/software_safe.js` 只允许作为 compat alias，且其实现必须显式指向 `viewer.js`，不能再承载独立 bundle 真值；`dist/software_safe_first_agent_claim_evidence.html` 从 canonical claim fixture 生成兼容测试页。
- `viewer.html` 是唯一 canonical 源码页面文件；build 将其复制到 dist `viewer.html` 与 compat `software_safe.html`，两者都必须引用 canonical bundle `viewer.js`。
- `dist/pixel-world-bridge/` 下的 JS / wasm bindgen 产物必须继续由 finalize 脚本生成，但其 canonical 生成边界要与 `viewer.js` 同步写死在同一条 build flow 中；该目录是生成产物 / dist 输入，不是手写源码目录。

### 4.3 Script and Bundle Contract
- 所有 Web dist / bundle / rebuild helper 必须按 canonical -> compat 的方向复制：
  - `viewer.html` / `viewer.js` 为 canonical
  - dist `software_safe.html` / `software_safe.js` / `software_safe_first_agent_claim_evidence.html` 为 generated compat outputs
- freshness / bundle manifest / browser rebuild helper 必须把 canonical `viewer.js` 纳入 source-of-truth scope。
- 若保留 checked-in generated artifact，则必须由单一 finalize 脚本负责写入，避免多个脚本分别“顺手生成”不同变体。

### 4.4 WASM/native build and resource boundary

- native-only runtime、live server、node 与持久化依赖不得重新进入 wasm Viewer dependency graph；Web 不支持的动作必须返回可诊断反馈，不能静默丢弃。
- 从 bundle 中移出的字体等浏览器资源必须由 canonical finalize/served-asset flow 提供；加载失败必须有可用 fallback 或可见诊断，不能把缺失资源伪装成成功构建。
- build/finalize 变更至少验证 `npm --prefix crates/oasis7_viewer run build:viewer`、`test:feedback-contract` 与 freshness 证据；UI 行为另用适用 `test:ui`，Rust Bevy bridge 用实际 `pixel_world_bridge` package，后端协议用实际 `oasis7` 测试。不存在 Rust `oasis7_viewer` package，不得用失效 check 命令或任意测试替代原断言。历史 byte-size/trunk 对比不是当前预算。

### Chat Web 稳定性验收边界

- 输入和发送不得使页面卡死；pending、失败原因与恢复动作须可见，不误发/重复发送、不静默重放。构建/freshness 通过不证明真实输入无卡死、IME 完整兼容或恢复闭环；这些按 Viewer 手册与 semantic test API 的专项 UI/external headed 验收处理。
- 旧锁重入及 epaint profile 处置仅为历史实现，不是当前 Viewer 构建要求。Launcher 仍使用 eframe；本文退役旧 Viewer 说明不授权删除 epaint 配置、egui vendor 或 Launcher 依赖。

## 接口 / 数据
- 源码入口：
  - `crates/oasis7_viewer/software_safe_src/main.jsx`
  - `crates/oasis7_viewer/software_safe_src/pixel_world_host.jsx`
  - `crates/oasis7_viewer/software_safe_src/legacy_core.js`
- 生成产物：
  - `crates/oasis7_viewer/viewer.js`
  - `crates/oasis7_viewer/dist/software_safe.js`
  - `crates/oasis7_viewer/dist/software_safe_first_agent_claim_evidence.html`
  - `crates/oasis7_viewer/dist/pixel-world-bridge/*`
- 相关脚本：
  - `crates/oasis7_viewer/scripts/finalize-software-safe-build.mjs`
  - `scripts/run-viewer-web.sh`
  - `scripts/agent-browser-lib.sh`
  - `scripts/build-game-launcher-bundle.sh`
  - `scripts/bundle-freshness-lib.sh`

## 里程碑
- M1：冻结拆分边界、canonical artifact 命名与 compat alias 关系。
- M2：完成 `legacy_core.js` facade + 子模块拆分。
- M3：完成 finalize/build/dist 脚本对 canonical bundle 的一致性调整。
- M4：回跑 repo-owned UI tests、build、bundle freshness 与相关 smoke。

## 风险
- `legacy_core.js` 仍保留大量 render/bootstrap 组装逻辑，若一次性过拆，最容易引入 UI contract 漂移或测试夹具失效。
- canonical `viewer.js` 与 dist compat aliases 若被其他脚本再次反向复制，或根目录重新保存 generated aliases，会让 freshness / bundle manifest 重新失真。
- `dist/pixel-world-bridge/` 属于 finalize 生成的 Viewer dist runtime；若 finalize flow 没有成为唯一写入口，后续很容易再次出现“bundle 已更新但 runtime 目录还是旧的”分叉。
- 2026-06-13 rebaseline: `legacy_core.js` 已拆出 constants/routes、初始 state shape 与 auth/CBOR crypto helper，但仍保留 control / semantic command / DOM rendering / bootstrap 组装逻辑；当前风险是“部分 facade split 已完成但剩余实现仍集中”，后续拆分必须继续保留 `state`、`initializeSoftwareSafeCore()`、`__AW_TEST__` 与现有测试入口合同。

## 6. Acceptance Criteria
- AC-1: `legacy_core.js` 不再包含全部主入口实现，而是退化为 facade / export assembly；主实现已下沉到多个职责模块。2026-06-13 状态：constants/routes、初始 state shape 与 auth/CBOR crypto helper 已下沉，`legacy_core.js` 仍保留 control / semantic command / DOM rendering / bootstrap 组装，AC-1 按部分完成并继续追踪剩余 viewer 工程债处理。
- AC-2: `viewer.js` 成为仓库内 canonical Viewer Web bundle 名称；`dist/software_safe.js` 仅作为显式 compat alias。
- AC-3: 源码 `viewer.html` 与 canonical claim fixture、生成的 dist `viewer.html` / `software_safe.html` / `software_safe.js` / `software_safe_first_agent_claim_evidence.html`、dist rebuild helper、bundle 打包脚本和 browser regression helper 均按同一 canonical/compat 关系工作；源码 checkout 不依赖 tracked compat copies。
- AC-4: `dist/pixel-world-bridge/` generated runtime 继续可用，且其来源明确绑定到 finalize flow，而不是被当成手工维护源码；浏览器 served-dist 路径仍保持 `./pixel-world-bridge/`。
- AC-5: 现有 `npm --prefix crates/oasis7_viewer run test:ui`、`npm --prefix crates/oasis7_viewer run build:software-safe` 与相关 repo-owned Node/browser helper 回归通过。
- AC-6: wasm graph 不含 native-only runtime/server 依赖；外置资源由 canonical finalize flow 交付，unsupported action 与资源加载失败都留下显式诊断。

## 7. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-WORLD_SIMULATOR-046 | `task_97820fd5e09a450aadcf988a968faad8` | `test_tier_required` | `npm --prefix crates/oasis7_viewer run test:ui` + `npm --prefix crates/oasis7_viewer run build:software-safe` + repo-owned Node contract test + `git diff --check` | Viewer Web 主入口模块边界、canonical bundle flow、pixel-world generated runtime copy chain |
