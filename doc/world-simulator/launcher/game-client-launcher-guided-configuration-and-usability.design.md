# 客户端启动器引导配置与可用性设计（当前 authority）

> 对应需求: `doc/world-simulator/launcher/game-client-launcher-guided-configuration-and-usability.prd.md`
> 历史迁移、验证与 task 状态：GitHub task issue evidence。

## 目标与范围

让玩家区分配置草稿、控制面确认与可信的成功画像，并在现有 native/Web 表面按真实 blocker 采取已有动作。本设计仅承接下述配置真值与恢复子范围，不定义世界规则或新的账户/存储产品。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [配置真值与恢复](game-client-launcher-guided-configuration-and-usability.prd.md#launcher-config-draft-truth) | Restore 保留草稿，成功画像属于对应已接受运行配置，Autofill 只选真实 game | [草稿与确认设计](#launcher-config-draft-design) | Launcher / 控制面 / native 文件与 Web storage / QA | 不改变运行中的旧进程、世界数据或建立新画像产品 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | Restore 只改草稿；poll 不抹掉 dirty；成功画像绑定已接受配置与对应运行实例；只接收本地配置结果及恢复子义务 | [已有设计](#launcher-config-draft-design) | Launcher 控制面/本地存储/QA；session 与世界 authority 独立 | 不证明 provider 鉴权、runtime 凭据注入、完整 session 或世界结果 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | Restore 只改草稿；poll 不抹掉 dirty；成功画像绑定已接受配置与对应运行实例；只接收本地配置结果及恢复子义务 | [已有设计](#launcher-config-draft-design) | Launcher 控制面/本地存储/QA；session 与世界 authority 独立 | 不证明 provider 鉴权、runtime 凭据注入、完整 session 或世界结果 |

## 表现结构

1. **语言与配置层**：现有语言状态驱动当前文案；配置预检将既有阻断项留在启动/配置表面，并保持已有高级配置入口可达。
2. **事实状态层**：launcher 忠实呈现已有 ready、未就绪、disabled、blocked/error 与 observer 状态，连同既有原因和可达动作；不得把状态卡片、日志或历史结果表示为成功。
3. **自引导层**：onboarding、任务提示、错误卡与 CTA 只编排当前已存在的动作；可跳过/重置的引导不能绕开配置、权限或 runtime 门槛。
4. **响应式层**：关键状态、阻断说明和操作入口随当前 native/Web 布局保持可读；实际信息层级、CTA 文案与窄屏验收由 game_visual_interaction_designer 定义。

## 控制与信息边界

<a id="launcher-config-draft-design"></a>
### 草稿、确认配置与成功画像

本节实现[配对 PRD 的配置真值要求](game-client-launcher-guided-configuration-and-usability.prd.md#launcher-config-draft-truth)。
`self_guided.rs` 的基准 `5e930fa` 实现按 `game_running` 直接保存 `self.config`，Restore 未明确
标记本地 dirty；这说明现有局部状态不足以证明“最近成功”。以下为修复合同，非已验收声明。

1. 保留独立的编辑态与控制面确认态。用户输入或 Restore 改变的是编辑态；normalize 后按同一
   相等性规则判断是否仍有未确认差异。普通 poll 只更新确认态，不能抹掉 dirty 草稿。
2. 将 last-successful 的输入绑定到已接受配置与相应运行实例。保存使用已确认的配置快照，
   不从渲染帧上的 `self.config` 取值；确认不足时保持之前画像，不能借全局 Running 标志推断。
3. 控制面确认当前提交配置后再合并编辑状态；若用户在提交后又编辑，不清掉新编辑的 dirty。
   native/Web 使用同一结果语义，各自保留已有文件/storage 适配和失败提示。
4. Autofill 的候选须来自已知 game binary 位置或显式选择，候选类型/路径校验和启动预检复用
   同一规则。GUI 自身存在只能证明 Launcher 已安装，不是 game fallback。

验证复用 `self_guided.rs`、`main_tests_onboarding.rs`、native/Web 状态消费和配置预检测试：
以可控服务端 poll 贯通 C1 Restore→旧快照、C2 Running A→dirty B→接受 C；测试真实按钮
handler 的状态结果和最终 native/Web 存储内容。L5 使用注入的候选路径，不需要实际启动 GUI
或游戏；新字段不必对外暴露，不改变运行中的 A 进程或世界数据。

## 表现层边界

- launcher 表现层不定义 world 规则、链角色权限、runtime 状态演化、持久化或网络/发布政策。
- `observer`、blocked、disabled 和未就绪只按当前实现的结构化含义呈现；不构成玩家访问、node admission、网络健康或 readiness 结论。
- 不新增自动修复、默认值写回、后台轮询、远端遥测、演示编排或持久化续跑合同；本节只约束已存在的 Restore/last-successful 画像，不创建新的配置存储产品。
- 此次迁移不改变 DOM、样式、布局或交互。未来可见变更必须按 `testing-manual.md` S6 获取浏览器证据和视觉角色验收。

## 代码接点

- 当前实现入口：`crates/oasis7_client_launcher/src/{config_ui.rs,launcher_core.rs,main_app_shell.rs,self_guided*.rs,app_process.rs,app_process_web.rs}`。
- 本设计记录表现层边界，不冻结具体函数、字段、组件树、存储方式、HTTP 路径或请求频率。

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 验证方法、test/manual source 与环境 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [配置真值与恢复](game-client-launcher-guided-configuration-and-usability.prd.md#launcher-config-draft-truth) | [草稿与确认设计](#launcher-config-draft-design) | C1 Restore→poll、C2 Running A→dirty B→接受 C、L5 缺 game 而 GUI 存在分别验证 | [测试手册](../../../testing-manual.md) launcher 定向回归与可见变化的 S6；上述 `self_guided.rs`、`main_tests_onboarding.rs` 和配置测试贯通真实 handler/poll/storage，L5 用路径注入；planned | 实现 PR 的当前候选和各表面状态/存储结果 | 本次未运行，不证明世界可玩或 provider 可用 |
| Trace refs: O7-REQ-728f73a0-995a-4995-a2be-ae525a08dab1；[REQ-ENTRY-LAUNCH-001](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-001) | [已有设计](#launcher-config-draft-design) | Restore 只改草稿；poll 不抹掉 dirty；成功画像绑定已接受配置与对应运行实例 | [测试手册](../../../testing-manual.md)；原 C1/C2/L5 handler、poll 与 native/Web storage 矩阵，同候选分别验证，适用可见变化另验 S6 | 实现 PR 的真实 handler/状态/存储结果；planned，本轮未运行 | 配置读回或画像成功不能代签全部设置应用、跨 profile、世界恢复或发行 |
| Trace refs: O7-REQ-1d9a996e-dd9d-4d59-9b3b-b79191f17676；[REQ-ENTRY-LAUNCH-002](../../product/player-entry-distribution/local-launch-session-continuity-and-recovery.prd.md#req-entry-launch-002) | [已有设计](#launcher-config-draft-design) | Restore 只改草稿；poll 不抹掉 dirty；成功画像绑定已接受配置与对应运行实例 | [测试手册](../../../testing-manual.md)；原 C1/C2/L5 handler、poll 与 native/Web storage 矩阵，同候选分别验证，适用可见变化另验 S6 | 实现 PR 的真实 handler/状态/存储结果；planned，本轮未运行 | 配置读回或画像成功不能代签全部设置应用、跨 profile、世界恢复或发行 |

## 决策、风险与未决

沿用编辑态与确认态分离、按已接受运行配置保存画像的决定；主要风险是 poll 覆盖未确认草稿、Running 被误当新配置成功及 GUI 路径被误作 game。解除条件为现有 C1/C2/L5 的真实 handler/poll/storage 验证；provider 鉴权、runtime 注入、世界结果与发行证据仍由各自 authority 提供，不能从局部配置结果外推。
