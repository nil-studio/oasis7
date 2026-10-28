# 本地启动会话连续性与恢复产品设计

## 文档身份

- 配对产品 PRD：[`local-launch-session-continuity-and-recovery.prd.md`](local-launch-session-continuity-and-recovery.prd.md)
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- Last reviewed：2026-09-13
- 专业域权威：[`Launcher 专业入口`](../../world-simulator/launcher/README.md)、[`world-runtime PRD`](../../world-runtime/prd.md)、[`testing PRD`](../../testing/prd.md)

配对产品 PRD 的 canonical 路径为 `doc/product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md`。

本文定义跨 native/Web Launcher 的稳定玩家阅读与恢复顺序，不冻结任何进程、目录、配置字段、凭据、时钟或控件实现。

## 0. 设计目标与代表性体验

本设计避免玩家把进程、配置保存或旧 session 误读为当前权威结果。代表性流程是选择入口和模式、准备、核对真实状态、遇阻后修复/等待/停止，再重新验证；下文的顺序与状态决策表说明每一步可知什么、可以做什么及不得推断的结果。

## 1. 稳定会话顺序

1. 当前入口和 primary mode。
2. 本地 session 当前状态：准备、可用、blocked、停止或恢复中。
3. 主要 blocker、可信原因和安全下一步。
4. 待验证的设置或配置及其来源、阶段与影响范围。
5. 已确认的当前 session 结果；世界结果仍由权威世界反馈表达。

运行日志、端口、路径、进程诊断和浏览器技术错误可以按需展开，但不能压过当前 session、主要 blocker 和恢复动作。

## 2. 生命周期与恢复

- 启动、停止、重试、修复、重新进入和安全返回使用可辨识的状态与结果；停止或重启不暗示世界回退、保存或重放。
- 恢复界面必须说明恢复的是本地 session、连接或配置，而非自动恢复玩家 authority、世界进度或未完成行动。
- 同一状态在 native 与 Web 可以用不同布局表达，但不得把一端的局部成功外推为另一端、另一模式或公开发行成功。

<a id="21-状态动作结果恢复决策"></a>
### 2.1 状态—动作—结果—恢复决策

所有会改变本地 session 的动作都先进入待验证状态。进程存在、页面可见、本地保存、请求受理或单个子步骤成功，只能证明对应局部事实；只有当前 authority 确认所需条件后，session 才能进入“可用”。

| 当前状态 | 可信依据与不可推断 | 玩家可做 | 禁止宣称或禁止动作 | 下一状态与恢复路径 |
| --- | --- | --- | --- | --- |
| 准备 | 已选择入口和 primary mode，启动前置条件仍待验证；不能推断服务、连接、配置或 authority 已可用 | 检查必要输入、发起启动或安全返回 | 不显示“已可玩”“已连接”或世界已恢复；不提交依赖可用 session 的动作 | 启动后进入恢复中/待验证；前置条件失败进入 blocked |
| 恢复中 | 当前启动、重试、修复、重新进入或配置应用正在验证；仅保留已确认完成的局部步骤 | 查看可信进度、取消到安全入口，或在 authority 明确允许时重试 | 不把自动刷新、进程重启、页面重开或请求受理当作可用；不并发发起会复用旧上下文的冲突恢复 | 全部所需条件经当前 authority 确认后进入可用；可诊断失败进入 blocked；证据中断进入未知/中断 |
| 可用 | 当前入口、primary mode、session、配置和适用 authority 已由当前 authority 确认；不推断世界行动或进度成功 | 进入受支持的本地体验、停止 session，或发起明确的配置变更 | 不把 session 可用提升为世界健康、公开发行就绪或未完成行动成功 | 停止请求进入恢复中/待验证，确认停止后进入停止；连接或证据失效进入未知/中断 |
| blocked | 已有可信 blocker，且当前条件不能安全继续；不能推断重试一定有效 | 采用已支持的重试、修复、重新进入或安全返回，并查看原因 | 不提供无 authority 支持的假动作；不隐藏 blocker 或无限自动重试 | 修复动作进入恢复中/待验证；无安全恢复路径时保持 blocked 并明确停止 |
| 停止 | 当前本地 session 已确认停止；不能推断世界已保存、回退、重放或未完成行动已取消 | 重新准备、返回入口，或查看适用的本地诊断 | 不复用旧 session、authority 或配置结论直接标为可用 | 重新进入先回到准备，再按当前条件重新验证 |
| 未知/中断 | 当前状态证据缺失、过期或互相冲突；旧值只能作为待验证上下文 | 重新验证、重新进入或安全返回；可查看最后可信状态及其时效 | 不沿用旧 authority、session、配置或 primary mode 继续动作；不把未知折叠成可用或停止 | 证据恢复后进入与当前事实相符的状态；无法恢复时进入 blocked 或安全返回 |

停止、重启、清理和恢复只改变本地边界，不撤销、确认、保存或重放权威世界结果。任何恢复后的配置、authority 和 primary mode 都必须重新验证；历史值只能帮助解释和填充待验证上下文。

### 2.2 设计验收样例

- D-LSC-1：准备、恢复中、可用、blocked、停止及未知/中断均能映射到可信依据、可用动作、禁止推断和下一恢复路径；没有 current authority 确认时不能进入可用。
- D-LSC-2：启动子步骤成功、Web 初始化失败、配置尚未应用和停止后重新进入四个样例，均不会被表达为世界已恢复、玩家行动已确认或旧 session 可直接复用。
- D-LSC-3：当没有安全恢复路径时，产品只保持 blocked、停止或返回受支持入口，不展示会复用旧 authority、session、配置或 primary mode 的假动作。

## 3. 设置边界

- 设置按“当前来源 → 草稿/暂存 → 提交 → 权威结果 → 恢复”组织。
- 配置操作的目标和影响范围在提交前可读；凭据、私密路径和原始 provider/runtime 诊断默认不作为玩家层内容展示。
- 只有专业 authority 确认时才显示已应用或可继续；本地保存、页面刷新或请求受理保持各自语义。

## 4. Web 失败表达

- 初始化、轮询或渲染失败必须从加载/可用状态中分离，并给出当前 authority 支持的重试、返回或其他恢复入口。
- 自动恢复、重新打开页面或重新启动本地服务只能说明该本地步骤已尝试；不能形成世界已健康或可玩的视觉暗示。

### 4.1 验证边界与未决

- 验证证据必须把本地 session、primary mode、当前 authority 与世界结果分别对账，覆盖启动失败、陈旧状态、Web 初始化失败、停止后重新进入和没有安全恢复路径的 blocked 样例；进程存在或页面可见不能单独证明可用。
- 未决：具体 Launcher 控件、错误分类、存储和恢复协议仍由 Launcher、runtime、WASM、Viewer 与 QA authority 决定；产品层只保留可理解状态、禁止推断和安全下一步。

## 5. 非承诺

- 不规定 Launcher 界面、按钮、步骤、CLI、子进程、signal、目录、文件、端口或浏览器 API。
- 不规定 LLM/provider 字段、secret 存储、WASM 时间实现、错误签名、重试次数、自动恢复策略或测试证据格式。
- 不把 Launcher 状态或当前技术表面提升为 primary-mode 可玩性、发布就绪或公开 claim。
## PRD REQ/AC fragment mapping

This design delegates product requirements and acceptance semantics to its paired PRD. Every row below is a navigable fragment mapping; this design does not create a second requirement authority.

| PRD requirement | PRD acceptance |
| --- | --- |
| [REQ-ENTRY-LAUNCH-001](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [AC-ENTRY-LAUNCH-001](local-launch-session-continuity-and-recovery.prd.md#ac-entry-launch-001) |
| [REQ-ENTRY-LAUNCH-002](local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [AC-ENTRY-LAUNCH-002](local-launch-session-continuity-and-recovery.prd.md#ac-entry-launch-002) |
