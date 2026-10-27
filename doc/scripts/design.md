# scripts 模块设计总览

审计轮次: 7

## 设计定位

本页描述 scripts 文档树的稳定抽象和阅读关系，不复制专题参数契约、
任务状态或工程 workflow 规则。

## 文档分层

| 层级 | 文档 | 职责 |
| --- | --- | --- |
| landing | `doc/scripts/README.md` | 按读者意图选择权威入口 |
| requirements | `doc/scripts/prd.md` | 定义模块边界、能力需求和验收标准 |
| architecture | `doc/scripts/design.md` | 解释模块结构与文档分层 |
| inventory | `doc/scripts/prd.index.md` | 提供仍独立维护专题的精确文件索引 |
| topic truth | `precommit/`；WASM 发布进入 `doc/world-runtime/wasm/wasm-deterministic-build-pipeline.prd.md` | 仅在需要独立专业权威时承载当前规范；通用治理语义归入模块稳定文档，旧 nightly/build-std 只从 world-runtime WASM evidence 追溯 |

工程任务生命周期不属于 scripts 模块的第二套设计层；它统一引用
`doc/engineering/workflow/source-of-truth.md`。

## 能力结构

- 开发与验证入口：为本地开发、检查和测试提供稳定 wrapper，并把正式验收边界交给测试规范。
- 任务与仓库治理 helper：实现 workflow 规范定义的机械操作，但不自行定义生命周期状态或门禁。
- 运行支撑入口：组合 launcher、runtime、provider 与 WASM 工具；稳定边界由模块 PRD/design 持有，具体参数由脚本 `--help` 和测试持有。
- 文档治理入口：通过模块索引和治理检查保持脚本、规范与验证证据可追溯。

## 阅读顺序

1. 从 `doc/scripts/README.md` 按目标选择入口。
2. 需要通用治理规则时读模块 PRD/design；pre-commit 进入本模块专题，WASM 发布进入 world-runtime canonical pipeline，历史背景按 README 分流。
3. 已知独立专题文件名时使用 `doc/scripts/prd.index.md` 精确定位。

## 稳定脚本契约

- 每个常见意图只有一个推荐稳定入口；辅助或 fallback 路径必须有明确触发条件，且不能替代 canonical 路径。
- 已发布脚本契约说明最小调用、改变验证范围的选项和失败类别；脚本行为、`--help` 与测试是可变 CLI 细节的实现权威。`dry-run`、`skip-*`、语法或 help 成功都不能证明完整门禁通过。

### Harness 启动主链路

- `worktree-harness.sh` 从解析后的当前 worktree 路径派生稳定身份，并把 runtime、artifact、browser、bundle、metadata、日志和 browser session 资源隔离到 `output/harness/<worktree_id>/`。它以 loopback endpoint 启动 lower-level launcher，并把 worktree-specific run ID、输出目录、metadata 文件和 chain node ID 传给 `run-launcher-stack.sh`。
- Harness 的 `ready` 仅表示 launcher 已发布 `STACK_READY=1` 且 Viewer HTTP/Web bridge probe 成功；`smoke` 仅证明 Viewer 可达并能采集非空 `__AW_TEST__` 状态。二者都不证明 headed S6、玩法可用、持久化、replay/recovery、共识或发布就绪。

### Harness 数据流与状态流

- Harness `state.json` 持有 `worktree_id`、`viewer_url`、PID、runtime/artifact/bundle/browser 路径、`meta_file` 与 `boot_mode` 等机器可读状态；`launch_mode` 属于 launcher `session.meta` / `--json-ready`。`browser_dir` 是 harness 分配路径，当前浏览器隔离权威仅是 worktree-derived `agent-browser` session namespace，不承诺独立 browser profile/cache。
- `up` 先写 `booting`，随后以 `preparing -> building_bundle|starting_launcher -> waiting_metadata -> ready` 的 phase-coded progress 写入 `state.json`。`--startup-timeout`（默认 300 秒）约束 metadata readiness deadline；状态会保留当前 phase、可跨进程比较的 epoch deadline、最近 progress message 与 poll attempt，避免把固定轮询黑盒化。`status`、`url`、`logs`、`smoke` 读取该状态；`down` 终止记录的 harness/launcher 进程和浏览器 session，但保留 runtime 与 evidence artifacts。端口分配在仓库拥有的 harness 权限范围内由本地锁和 repository-family registry 串行化，并在 bind 前持久化 reservation，避免同一仓库 harness 之间的重复分配；新 reservation 持久化 owner identity，旧的 PID-only state/reservation 仍可读取，但 live identity-less ownership 会 fail-closed 并保留给 operator recovery；只有 owner PID 已死亡或 identity 可证明不匹配时才可回收。这不是 OS 层持续 socket reservation，外部非-harness 进程在 probe 与 bind 之间仍可能发生 probe-to-bind race。
- `smoke --timeout` 是整个真实 `agent-browser` 操作序列的共享绝对 deadline，而不是仅写入摘要的标签；open、networkidle wait、state eval 和 screenshot 均由可移植 watchdog 约束。内部 watchdog 超时返回 `124`，公开 `smoke`（及 `up` 调用该 smoke 的失败路径）当前归一为退出码 `1`，写入 `phase=smoke_failed`；调用方结合 `last_smoke_error` 区分超时与普通失败，成功才恢复 `phase=ready`。四阶段的离线桩应分别验证公开退出码、原因与清理，helper 的 `124` 测试不证明 CLI 透传。这仍只证明 Viewer 可达与 `__AW_TEST__` 状态采集，不替代 headed S6。

### Wrapper 与运行模式边界

- Launcher wrapper inventory：`run-launcher-stack.sh` 保留为底层 bootstrap；`worktree-harness.sh` 是隔离 QA/subagent API；`run-producer-playtest.sh` 是 bundle-first 制作人/发布前入口；`run-game-test-ab.sh` 是自动化哨兵；`viewer-primary-web-entry-regression.sh`、`viewer-software-safe-step-regression.sh`、`viewer-post-onboarding-qa.sh` 与 headless smoke、chat regression 均有独立的玩家/QA/视觉证据调用。当前没有可由 caller/docs 证明为纯 alias 的 wrapper，因此本轮不删除入口。
- Chain-enabled 是 launcher 默认模式，并要求执行世界持久化就绪；`--chain-disable` 只是 Viewer/page-play mitigation，不能作为 chain-enabled standalone-world 证据。`trusted_local_only` 是显式本地 playtest 模式，不是 hosted-public 默认值。

## 集成边界

- 工程生命周期：`doc/engineering/workflow/source-of-truth.md`
- 测试策略与 suite 选择：`testing-manual.md`
- 文档结构门禁：`scripts/doc-governance-check.sh`
- 跨模块工程需求：`doc/engineering/prd.md`

新增能力时先确定它属于模块级契约还是专题契约；只有跨多个专题的稳定
结构才回写本页，参数、端口、临时兼容路径和任务完成明细留在各自 owner 文档。

### 子模块工作区清理

Trace refs: O7-REQ-a805d965-b040-44ba-9af1-68ca3c96b2d8；要求声明见[开发流程规范](../engineering/workflow/source-of-truth.md#submodule-worktree-cleanup)。

`resource-cleanup-executor.py` 复用现有 path/branch/HEAD、交付、材料与进程检查，在显式 `--allow-submodules` 下增加递归 gitlink 与模块 Git 数据核对。只有当前 worktree 私有的模块数据可随它删除；已反初始化的数据仍检查本地历史。预览只读取，执行前重新核对后先非强制 deinit，再单次 force remove，最后按原 ref 事务删除 topic 分支。进程 census 在同一 30 秒绝对预算内持续核验新增或 PID 复用的原生身份，覆盖收敛后才允许清理；原生身份暂时不可读最多短暂重试约 1 秒（仍受总预算限制），只有 ESRCH 才认定退出。Darwin 上 libproc 可能无法读取尚未被父进程回收的 zombie；仅在绝对路径 `/bin/ps` 返回单条精确 PID 与明确 `Z` 状态时，允许将这个已释放文件描述符和 cwd 的内核状态排除出资源持有者 census，而不宣称 PID 已消失。不按进程名称、存在时长、空输出或查询失败作此判断；同一绝对预算内的后续 census 仍核对 PID 复用后的新原生身份。其他持续不可读、覆盖不全或超时均保留。子模块数据或进程可见性不明即保留，失败不会回退到 rm 或双重 force。验证用临时真实 Git 仓库覆盖干净成功、未提交/未推送及并发拒绝路径。
