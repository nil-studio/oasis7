# 游戏客户端启动器运行时会话连续性

审计轮次: 5

> 本文是 Launcher 托管 chain runtime、本地 execution-world 输出、受控恢复与 Web/WASM 会话连续性的当前专业 authority。它不定义玩家入口承诺、世界规则、共识算法、WASM ABI、LLM 凭据存储格式或发布 readiness。

- 对应设计文档: `doc/world-simulator/launcher/game-client-launcher-runtime-session-continuity.design.md`
- 历史迁移、验证与 task 状态：GitHub task issue evidence。

## 目标

Launcher 负责配置和受控进程编排；`oasis7_chain_runtime` 负责链执行、执行世界持久化与运行时状态。托管的游戏与 Web Launcher 必须将 execution world 显式定向到 `output/chain-runtime/<node_id>/reward-runtime-execution-world`，不能依赖调用进程的 cwd。

## 范围

本专题覆盖本地会话的进程责任、输出落位、状态错误分类、恢复建议，以及浏览器 Launcher 的平台兼容边界；不把这些实现细节提升为产品、网络、结算、持久化 SLA 或公开服务承诺。

## 当前专业合同

### 托管进程与输出

- Launcher 只构造、启动、停止并观察既有 chain runtime；它不重算世界状态、共识或 execution-world 数据。
- `oasis7_chain_runtime` 是 node/consensus/execution-world 的进程入口，Launcher 拥有其编排与 session continuity；`oasis7_viewer_live` 是独立的 Viewer live/Web 进程，可通过声明的 chain-status/submit client endpoints 消费 committed chain world 或提交动作，但不得内嵌 node、consensus gate、reward-runtime worker、topology、persistent execution world，也不得把 Viewer event drive 当作 consensus tick。
- 托管启动必须为 chain runtime 传递显式 `--execution-world-dir`，其 node-scoped 输出规则为 `output/chain-runtime/<node_id>/reward-runtime-execution-world`。直接手工运行 runtime 的 cwd 行为不由 Launcher 保证。
- chain runtime 的状态、余额、存储指标和执行错误以现有 runtime status 响应为真值；Launcher 只能呈现或转发，不得由空结果推断运行时健康。

### Stale execution world 与恢复

- 仅当**当前 chain 启动尝试**所属进程的日志同时表明 `DistributedValidationFailed` 与 latest state-root mismatch 时，Launcher 才把失败分类为 `stale_execution_world`；端口、参数、二进制或其他启动失败必须保持其原有错误语义。两个标记可以分属同次尝试的不同 poll，不能来自 game 日志或不同启动尝试。
- 新启动尝试的诊断独立于历史日志。A 曾 stale 后，B 的 address-in-use、普通退出或先成功再退出都不能继承 A 的分类或恢复建议；拒绝重复 Start 时则保留仍在运行的原进程及其诊断归属。旧日志可以继续显示为历史，不能作为新尝试的当前失败证据。
- 默认恢复建议是非破坏性的 fresh node id 与相应 status bind。它不修复、更改或删除旧执行世界。
- 重置 execution world 属于破坏性操作，必须由明确确认的实现路径执行；本专题不授权按目录、时间或猜测自动清理运行时数据。
- latest head、checkpoint、replay、GC、storage profile、pin set 与恢复一致性由 [运行态存储体积治理](../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md) 定义。Launcher 只消费其已发布状态，不能替代该运行时合同。

上述日志归属补齐 [#4406 L9](https://github.com/eng-cc/oasis7/issues/4406) 的目标合同。验收须实际经过跨尝试日志接收、poll 与退出分类；只预置状态或检查序列化不证明诊断隔离。它不改变恢复机制，也不授权清理 execution-world。

### Web/WASM 会话兼容

- 浏览器 Launcher 的时间和状态刷新路径必须使用该目标可支持的实现，且状态刷新失败必须可诊断、不得令页面崩溃或无界并发堆积。
- 受影响的 Web/WASM 改动须保留目标编译及真实浏览器的状态/控制台证据；具体时钟类型、轮询间隔、请求防抖字段、控制台签名和测试命令是实现与验证事实，不是本合同的稳定 API。

## 接口 / 数据

- 托管 runtime 参数：`--execution-world-dir` 与既有 node-scoped 输出路径。
- 进程边界：正式/本地 chain-enabled gameplay 由 Launcher 编排 `oasis7_chain_runtime`；`oasis7_viewer_live` 不接受旧 node/consensus ownership，但可用 `--chain-status-bind` 观察 committed world，并在同时配置 status bind 时用 `--chain-submit-bind` 作为客户端提交端点。
- Launcher 状态：既有 chain runtime status、`stale_execution_world` 分类和建议配置；具体 payload 以运行中实现为真值。
- 运行态存储状态：profile、storage metrics、degraded reason 与 replay summary；字段和恢复规则由运行态存储治理专题定义。

## 里程碑

- 已完成：链运行时/Launcher 职责拆分、显式 execution-world 输出、严格 stale 分类与 fresh-node 建议、浏览器 WASM 时间兼容修复。
- 维护：后续进程、持久化或浏览器实现变更必须按对应专业 owner 和验证证据重新确认。

## 风险

- 不承诺 chain 可达、世界可玩、状态最终性、跨重启完整历史、任意高度可恢复、无数据丢失、公开服务、mainnet 或发行就绪。
- 不把 node id、端口、CLI、目录路径以外的配置、信号、进程超时、LLM secret、WASM clock 或 polling 参数暴露为产品承诺。
- 新增或改变 runtime 持久化、恢复、GC、共识、浏览器计时或跨进程信号语义时，必须由 runtime、WASM 或相应平台 owner 重新评审并提供受影响测试证据。

## 追溯与验证

- 实现入口：`crates/oasis7_client_launcher/`、`crates/oasis7_launcher_backend/src/bin/oasis7_{game_launcher,web_launcher}.rs`、`crates/oasis7_chain_runtime/src/host.rs` 及其拆分模块和测试。
- 运行时持久化、回放和存储的现行要求与验证方法：[存储治理 PRD](../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md) 与[设计](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md)。执行结果从对应 PR、CI 与评审记录追溯；旧专题项目记录已退出当前树，从该专题的 Git history 追溯，不能当作当前可打开的验证入口，也不由此推定旧候选或失败已经通过。
- 文档变更验证：`./scripts/doc-governance-check.sh && ./scripts/readme-link-check.sh && git diff --check`。
- 本稳定专题吸收的日期化 Launcher session 变更及其完成证据由 Git history 和 PR、实际 CI 与评审记录（Issue 按需） 追溯。
