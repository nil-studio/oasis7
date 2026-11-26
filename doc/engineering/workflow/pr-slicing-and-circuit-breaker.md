# PR Slicing, Scope-Lock & Circuit-Breaker Specification（暗发布切片、契约锁边界与熔断规范）

本分册由[开发流程规范](source-of-truth.md#small-complete-pr)挂接，定义跨层与中大型改动的**前置契约 Draft PR 锁边界**、**三段式暗发布切片（Dark / Adapter / Cutover）**、**L0–L3 架构环爆炸半径上限**及**自动熔断与只读拆骨协议**，由本地与 GitHub Actions 共用的 `scripts/check-pr-boundaries.sh`（`.github/workflows/pr-boundary-gate.yml`）执行机器校验。通用授权、评审与合入规则仍以 `source-of-truth.md` 为准。

## 1. 前置契约 Draft PR 工作流（Scope-Lock Before Coding）

为防止代码 Issue 在拆解后因未核算硬门禁而在实现期跑偏、临场拆微 PR 或跨层越界，同时避免多切片基线过期：

1. **Issue 全景锚定（前置锁定预计 PR 总数与逐片边界）：** 在代码 Issue 新建或拆解阶段（见[预计 PR 边界](source-of-truth.md#planned-pr-scope)），须对照当前 `main` 盘点真实受影响的 `crates/*`、所属 `L0..L3` 架构环、消费者扇出与预计非锁文件改动行数，显式写出**预计 PR 总数（`N`）**及有序切片序列（逐片列明 `PR k/N`、`Slice-Type`、`Allowed-Crates` 白名单、预计行数/文件数、核心交付物与 `Active-Path-Impact`、`Non-Goals`），严禁留“实施时视情况拆分”等模糊占位：
   - 仅当改动同时满足 `<= 30` 个代码文件、`<= 3` 个 `crates/*`、`<= 2` 个相邻架构环（`max_ring - min_ring <= 1`）且符合对应单切片行数与活跃路径上限时，方可规划为 `预计 1 PR`；
   - 受影响消费者超过 `3` 个 Crate 或跨越非相邻环时，须在拆单时即按每批 `<= 3` 个相邻环 Crate 拆出确定的多批 `Adapter` / `Cutover` 序列；若单个子 Issue 预排切片超过 **3–4 个串行 PR** 且包含可独立交付的能力簇或消费者组，应直接拆分为独立[原生 sub-issue](source-of-truth.md#code-task-sub-issues)；
   - **存量/在途 Issue 接手补锚：** 对尚未给出明确剩余 PR 数量与逐片白名单的历史开放 Issue，执行者在接手或新开下一个 PR 前，必须先盘点当前 `main` 剩余差异并在原 Issue 补齐剩余 `1..N` 切片计划，再启动编码。
2. **单飞契约 Draft PR（`1:1` 映射预定切片）：** 每次仅针对计划序列中**当前动工的第 `k/N` 个切片**基于最新 `main` 创建空提交（`git commit --allow-empty`）并建立 Draft PR，在 PR 描述顶部原样锁死该切片预定的 `Slice-Type`、`Allowed-Crates` 与 `Planned-Slice: k/N` 后，再在分支内编写实现代码；若复用预先创建的空壳分支，动工第一步必须执行 `git fetch origin main && git rebase origin/main` 刷新基线。编码前或编码中若发现真实依赖与原定切片数量或 `Allowed-Crates` 不符，须**先停笔**在原 Issue 说明原因并更新切片计划（已越界分支按第 4 节拆骨），严禁无计划私自扩写白名单外 Crate、擅自增拆微 PR 或合并多个预定切片。
3. **零改动契约态免检：** 当分支处于 `0` 文件改动的纯契约占位态时，`PR Boundary Gate` 仅核验 `Slice-Type` 与 `Allowed-Crates` 元数据完整性即短路放行，重型编译 CI 自动跳过。

## 2. 三段式“暗发布”切片模型（Dark-Launch Slicing）

凡涉及跨层改动或预计非锁文件改动 `> 300` 行的任务，必须拆分为串行 PR 序列：

| 切片类型 | PR 标题前缀 / 元数据 | 行数上限（剔除锁文件与静态 Fixture） | 允许范围与活跃路径红线 | 人工审阅策略 |
| :--- | :--- | :--- | :--- | :--- |
| **增量暗代码单** | `[Slice: Dark]` | `<= 800` 行 | 新增模块、可选协议字段、内部结构、纯逻辑及单测。**零现网行为变更**，严禁挂载活跃入口或删改旧签名；同模块完整暗实现应合并单次交付，禁止切碎。 | 快速扫视活跃入口零改动 + CI 全绿即合 |
| **兼容适配单** | `[Slice: Adapter]` | `<= 600` 行 | 在投影、服务、客户端或 Harness 补齐类型转换与双路径读取兼容，保持主干旧链路 100% 可用。 | 核对边界转换与兼容性 Fixture |
| **状态机切换单** | `[Slice: Cutover]` | `<= 150` 行 | 将已合入 `main` 的暗代码接线至 `actions_resolution`、运行时路由或 Viewer 入口并清理旧开关。**严禁顺手夹带新业务逻辑或无关重构。** | 集中精力深度审查规则与安全边界 |

*(注：独立小修补与纯文档/机械维护可分别使用 `[Slice: Bugfix]` `<= 400` 行、`[Slice: Docs]` / `[Slice: Chore]` `<= 1200` 行；其中 `[Slice: Docs]` 禁止修改任何 `crates/*` 文件，`[Slice: Chore]` 仅限无语义变化的纯机械维护如测试/样式文件切分、工具配置与格式化，**严禁将新业务/宿主实现或跨层入口切换伪装为 `[Slice: Chore]` 规避 `Dark -> Adapter -> Cutover` 串行切片**。)*

## 3. L0–L3 架构环隔离与白名单红线（Architecture Ring & Whitelist Limits）

PR 爆炸半径按 `crates/*` 划分为四个相邻架构环（Cargo 细粒度单向依赖与禁止边仍由 [Rust 工程契约方向矩阵](../rust-governance/rust-engineering-contracts.design.md#boundary-matrix)与 `scripts/cargo-dev-architecture.json` 裁定）：

- **L0（协议、底层原语与网络共识）：** `oasis7_proto`, `oasis7_node`, `oasis7_net`, `oasis7_consensus`, `oasis7_distfs`（及底层原语/存储密码学支撑：`oasis7_domain_types`, `oasis7_formal_protocol`, `oasis7_cas`, `oasis7_context`, `oasis7_custody_core`, `oasis7_local_signer`, `oasis7_secure_envelope`, `oasis7_signer_protocol`, `oasis7_observability`）
- **L1（世界模拟、运行时与 WASM 扩展）：** `oasis7`, `oasis7_simulator`, `oasis7_runtime`, `oasis7_wasm_build`, `oasis7_wasm_sdk`, `oasis7_wasm_executor`, `oasis7_wasm_router`（及同环 WASM/宿主支撑：`oasis7_wasm_abi`, `oasis7_wasm_materializer`, `oasis7_wasm_store`, `oasis7_builtin_wasm_modules`, `oasis7_executable_image`, `oasis7_runtime_host`）
- **L2（世界状态、投影与服务门面）：** `oasis7_world_state`, `oasis7_world_projection`, `oasis7_world_service`（及服务/桥接/Worker：`oasis7_chain_runtime`, `oasis7_custody_worker`, `oasis7_dev_test_worker`, `oasis7_hosted_access`, `oasis7_letai_credit_adapter`, `oasis7_newapi_bridge_service`, `oasis7_provider_local_bridge`）
- **L3（Agent、客户端、启动器与 Viewer）：** `oasis7_agent_api`, `oasis7_agent_harness`, `oasis7_model_provider`, `oasis7_client_api`, `oasis7_game_client`, `oasis7_viewer_runtime`, `oasis7_viewer`（及客户端/工具/渲染桥接：`oasis7_viewer_codec`, `oasis7_browser_game_client`, `oasis7_client_cli`, `oasis7_client_launcher`, `oasis7_client_storage`, `oasis7_launcher_backend`, `oasis7_launcher_config`, `oasis7_launcher_ui`, `oasis7_ops_tools`, `oasis7_protocol_tools`, `oasis7_provider_tools`, `pixel_world_bridge`）

**硬性跨度与防越界约束：**

1. 实际修改的每个 `crates/*` 必须严格落在 Draft PR 预设的 `Allowed-Crates` 白名单内（纯文档/脚本填写 `none`），严禁编码途中私自扩权修改白名单外的 Crate。
2. 单个 PR 最多触碰 **3 个** `crates/*` 成员，且最多跨越 **2 个相邻架构环**（`max_ring - min_ring <= 1`，严禁 `L0 + L2`、`L0 + L3` 或 `L1 + L3` 跨层穿透）。
3. 例外豁免：全仓机械性重命名或底层公共依赖升级，须由人类维护者在 PR 添加 `pr-boundary-override` 标签显式放行（`dependabot[bot]` 自动依赖 PR 默认豁免），Agent 严禁自行添加该标签或绕过门禁。

## 4. 自动熔断阈值与“只读拆骨”止损协议（Salvage Protocol）

编码或 Rebase 期间触发以下任一条件，必须**立即停止往原分支追加提交（STOP）**：

- 触碰了 `Allowed-Crates` 之外的 Crate，或有效行数、Crate 数量、架构环跨度超出上限导致 `./scripts/check-pr-boundaries.sh` 失败；
- 分支落后 `main` 超过 **40 个 Commit** 且出现跨文件语义冲突（门禁脚本在落后 `> 25` 个 Commit 时输出漂移告警，落后 `> 60` 个 Commit 时直接触发硬熔断失败）；
- 连续 **2 次** Rebase 或修补 CI 引入新的编译/测试退化。

**存量巨型 PR / 熔断分支拆骨流程：**

1. 将原分支冻结为 **只读标本（Read-Only Donor Branch）**，禁止继续 Rebase 整个分支或在原分支叠加修补提交。
2. 基于最新 `main` 依次切出新分支提取合入：`Salvage-1 [Slice: Dark]`（纯类型/独立逻辑+单测） -> `Salvage-2 [Slice: Adapter]`（兼容层） -> `Salvage-3 [Slice: Cutover]`（`<= 150` 行最小接线），闭环后关闭原巨型 PR 并按清理规范回收标本分支。
