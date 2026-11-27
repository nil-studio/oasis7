# 游戏客户端启动器运行时会话连续性设计

## 1. 问题、目标与非目标

无头 Launcher 后端（`oasis7_launcher_backend`）构造参数并托管子进程；chain runtime 保持执行与持久化权威。本设计防止停止、启动失败和恢复后继续呈现旧会话证据。进程存在、请求受理、页面可见和配置保存不证明世界健康、可玩或行动成功；不承诺无损恢复、自动迁移或发行就绪。

## 2. 上游约束与相关角色

- 专业合同：[运行时会话连续性 PRD](game-client-launcher-runtime-session-continuity.prd.md)。
- 产品主责：[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001)、[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002)。这里消费产品结果边界，不重新定义产品权利。
- 玩家状态与动作顺序消费[配对产品设计 §2.1](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.design.md#21-状态动作结果恢复决策)。runtime 拥有执行和存储；Launcher 拥有本地编排；QA 验证各层事实。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | 本地探测结果和世界结果分离；失效证据不作为当前 authority | [重新验证](#session-revalidation) | Launcher/runtime/Viewer/QA | 不证明世界最终性、身份授权或可玩 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | 停止、失败、退出后重新探测；不沿用旧会话证据 | [重新验证](#session-revalidation) | Launcher/runtime/Viewer/QA | 不实现世界存储重建或待决行动重放 |

## 3. 当前状态、目标状态与差距

已有显式 execution-world 输出、严格 stale 分类、fresh-node 建议及状态探测。原设计缺恢复后重新验证和逐项验收映射；原实现的部分退出、停止与失败路径曾保留旧 identity/replication 快照。本设计与对应实现要求在这些边界立即撤销当前证据，只有新的成功探测或当前代次响应才重新发布。具体候选的验证、合入与部署事实由 PR 和真实输出承载，本长期设计不预写发行就绪。

## 4. 边界与结构

1. Launcher 使用 node-scoped 参数与显式 execution-world 目录，避免 cwd 将产物写入源码树。
2. runtime 发布 identity、P2P、observability 和 replication 快照；Launcher 不重算世界状态。
3. native/Web 客户端消费同一控制面状态。[控制面设计](game-client-launcher-control-plane-and-machine-interface.design.md)拥有请求、结果和可见错误映射。
4. head、checkpoint、replay、GC 与 pin 的确定性归[运行态存储治理](../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md)。Launcher 不能按文件年龄清理。

## 5. 关键运行流程

<a id="session-revalidation"></a>
### 5.1 停止、重入与恢复重新验证

1. 请求使用本次配置，经部署模式、chain-enabled、并发进程和配置校验决定是否允许；运行中的重复启动拒绝，不替换其配置或进程。
2. 没有活动进程的新启动尝试先清除旧 identity、P2P、observability、replication 和 probe 时间。参数或二进制失败保留准确错误，不沿用旧证据。
3. 成功 spawn 只进入 Starting。只有当前托管进程存在且本次状态端点探测成功，才能发布新的 runtime 快照与 Ready；Ready 只表示该探测成功，不证明身份授权、primary-mode 可玩性、世界最终性或行动完成。
4. 探测失败撤销当前快照；启动宽限期内为 Starting，之后为 ConfigError/Unreachable。无可信证据不能继续显示旧 identity 为当前身份。
5. 客户端状态刷新或控制请求失败同样撤销当前证据并标为 Unreachable；不能保留 Ready。停止请求、子进程退出或进程查询失败清除当前快照与 probe 时间。停止失败保留失败诊断，不把它声明为成功停止或世界回退。下一次启动重新执行上述流程。
6. native/Web 状态轮询与控制响应携带客户端本地请求代次。控制操作开始时撤销旧快照并将 Ready 收窄为 Starting；开始及结束、证据失效时推进代次；控制请求在途时或代次已经过期的响应不应用，也不以旧失败覆盖新成功。代次只用于本地响应排序，不授予玩家权限，不改变 HTTP schema。托管控制面退出后，已排队的旧成功响应同样失效；只有新代次响应可重新验证。
7. 陈旧状态仅在当前 chain 启动尝试的 DistributedValidationFailed 与 latest state-root mismatch 同时成立时分类；日志归属见[当前尝试诊断](#current-attempt-diagnostics)。保留准确错误与非破坏性 fresh-node 建议。采用建议只是本次输入，仍须完整重新验证；不删除旧数据，不恢复旧权限或重放待决行动。

<a id="current-attempt-diagnostics"></a>
### 5.1.1 当前尝试诊断

Launcher 在接受一次没有活动 chain 的新启动尝试时建立本地诊断上下文，绑定该尝试的配置与 chain 日志来源；清除上一次尝试的分类标记。日志接收时只在该上下文内累计两个 stale 标记，跨 poll 保留，进程退出时据此分类。全局显示日志、game 日志和以前的 chain 日志均不能为当前尝试补足标记。运行中的重复 Start 被拒绝，不替换活动上下文或托管句柄。

上下文只解决本地日志归属，可复用已有进程/请求状态，不增加 HTTP 字段或持久化的身份协议。历史日志保留不等于当前 authority；新尝试失败时仍按自身的参数、端口或进程错误显示。无须修改、扫描或删除旧 execution-world。

基准 `5e930face77dd284bbba6186fd997e7e63fa9164` 的 `oasis7_web_launcher/control_plane.rs::classify_stale_execution_world` 从共用 `state.logs` 最近 64 行和当次收到的 chain 日志一起查找标记，尚不足以保证上述归属；只改为退出时的 recent logs 又会丢失同次尝试早先 poll 的有效标记。这是 [#4406 L9](https://github.com/eng-cc/oasis7/issues/4406) 的实现与验证接点，不把文档补充声明为修复。

对应 `control_plane/tests.rs` 的目标回归应独立覆盖：A stale 后 B 普通失败、A stale 后 B 成功再普通退出、同次 chain 的两个标记跨 poll、game 与不同旧尝试各有一个标记。核对当前分类和建议，并确认旧 execution-world 文件未变；状态序列化或直接构造最终分类不能替代完整日志消费时序。

### 5.2 Web 失败

初始化、刷新和控制请求失败由控制面/客户端映射为可见错误及支持的下一步，不无限加载或假成功；本设计拥有 runtime 探测与会话证据失效，表现映射消费控制面合同。时钟和刷新使用目标平台支持的实现，不定义永久轮询 API 或 WASM ABI。独立 HTTP worker 在读取请求前将接收连接显式切回阻塞模式，并设置有界读取/写入超时；监听器仍非阻塞。大 WASM 文件不能因继承的非阻塞属性在 WouldBlock 时截断。

## 6. 接口与数据合同

沿用 /api/state、控制动作和 runtime status 的既有 schema，不新增玩家模式或公共协议。identity、P2P、observability、replication 的 None 表示当前会话没有有效探测证据；客户端不得解释为世界数据删除。日志与 stale 恢复建议可保留作诊断，但不是当前 authority。配置、provider 和凭据结果阶段由各自设置 authority 承接，本设计不代签已应用。

## 7. 状态、事务与持久化

ServiceState 串行控制托管子进程和探测，快照字段在同一次状态更新内一起失效或发布。Starting/Ready/NotStarted/Disabled/ConfigError/Unreachable/StaleExecutionWorld 沿用当前技术状态，产品阅读状态由客户端组合，不新造持久化会话。

本地控制结果只证明请求或进程事实；世界 applied/persisted 以 runtime receipt/权威历史为准。此修改不改 execution-world、checkpoint、世界行动或存储格式，不把本地重启、清理和停止变成世界回滚、保存或重放。

## 8. 部署、安全与运行约束

沿用部署模式对本地 runtime 的限制。fresh node 不修改旧 execution-world；破坏性重置只能走明确确认的实现路径。诊断表面沿用凭据与敏感路径保护；查询失败不能放宽授权。浏览器状态不代签 native、另一模式或公开服务 readiness。

## 9. 质量与容量

不改变既有探测超时、启动宽限期和轮询间隔，不承诺新增 SLA。HTTP worker 每次阻塞读取沿用 3 秒超时，每次阻塞写入使用 3 秒超时；这不是整个连接的绝对总时限。慢连接失败仍可诊断，不无限等待单次 I/O。验证输入为旧成功快照加停止/失败/退出刺激；响应必须清除当前快照、保留准确错误，且新会话没有成功探测前不能 Ready。容量与确定性执行仍归 runtime authority。

## 10. 兼容、迁移与回滚

保持 JSON 字段、错误类别和恢复建议兼容，仅使失效证据返回既有空值。无数据迁移或双写；回滚代码不撤销世界事实，但会重新引入旧证据显示风险。旧消费者应已处理启动时的空快照，需验证相应 native/Web 状态消费。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [重新验证](#session-revalidation) | 失败/停止后撤销旧证据，保持真实错误 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs` 的 session_ 回归及 `crates/oasis7_client_launcher/src/main_chain_status.rs` 的 session_state_refresh_failure_invalidates_chain_evidence 、late_state_poll_cannot_restore_evidence_after_control_action 与 queued_action_snapshot_cannot_restore_exited_control_plane；同候选 native 单元测试，执行入口 [testing-manual](../../../testing-manual.md) | PR 与真实测试输出 | 不证明世界最终性、授权或可玩 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [重新验证](#session-revalidation) | 陈旧、停止、失败启动和退出后须新探测；无安全路径保持错误 | `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/control_plane/tests.rs` 会话回归和 `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/oasis7_web_launcher_tests.rs`；同候选 canonical cargo test；HTTP 大响应回归见 `crates/oasis7_launcher_backend/src/bin/oasis7_web_launcher/http_codec.rs` 的 nonblocking_connection_delivers_complete_large_response；Web 可见变化按 [testing-manual S6](../../../testing-manual.md) 验证 | PR、测试及适用浏览器记录 | 不证明无损恢复、世界重放；浏览器证据单独记录 |

设置阶段消费设置 authority；Web 错误表现消费控制面设计；公开 claim 仍须同候选跨层证据，局部进程与单元测试不能成立发行结论。

活跃实现接点：`oasis7_web_launcher/control_plane.rs`、`control_plane/chain_status_probe.rs`。canonical 局部命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_launcher_backend --bin oasis7_web_launcher`；文档检查使用 `scripts/doc-governance-check.sh`。实际结果记录在 PR，不在长期设计预写通过。

## 12. 决策、长期风险与未决问题

沿用现有技术状态和空快照表达证据失效，避免新增未经支持的授权/可玩协议。残余风险：成功状态端点探测本身不证明完整会话授权或可玩；本设计处理状态轮询与控制响应的本地排序；跨模式配置应用与世界授权仍由对应 authority 管理。Owner 为 Launcher/runtime/Viewer/QA；若未来 Ready 被用作世界动作授权条件，必须另行验证身份绑定和授权合同，不能从本设计推导。
