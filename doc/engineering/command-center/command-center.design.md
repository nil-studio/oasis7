# 指挥客户端系统设计

- 文档 ID：`DESIGN-ENGINEERING-COMMAND-CENTER`
- 状态：`draft`；目标架构，局部实现与验证范围见 §11，完整产品验收未完成
- owner_role：`runtime_engineer`；产品归 `producer_system_designer`，外部接续归 `agent_engineer`
- module：`engineering`
- 上游：[指挥客户端产品设计](command-center.prd.md)、[PRD-ENGINEERING-034](../prd.md#PRD-ENGINEERING-034)
- 当前实现审读基线：`822f6b49d1a5fb2b866b6c6902ffc8ed024d043b`；2026-10-18 历史交互候选 `52dd4aed915ce5ca75e00ddff6ba5e1918ebdec6`；2026-10-17 历史核对 `main@0ec10379419b0aaedb9d16157b81c92f4e6e3495`；初始设计基线 `7039bc376882ab8f84f88ca014f6d7ce642539a8`，既有方案 `56a33562436ffa4b65a66d05e61f26a849b16728`
- last_reviewed_at：2026-10-18
- 初始设计与评审来源：[PR #4518](https://github.com/eng-cc/oasis7/pull/4518)

本文把客户端形态、Rust 技术调研和架构方案收敛为一份系统设计。产品行为和验收由配对 PRD 主责；本文定义如何承接，不把目标状态写成已交付事实。通用授权、开发、验证和交付仍由 [workflow/source-of-truth](../workflow/source-of-truth.md) 主责，客户端内部状态不增加上游任务准入或审批门禁。

## 1. 问题、目标与非目标

指挥中心读取真实本地文档/Git、GitHub PR/CI，保存个人决定、关系、问题与阅读位置，让用户看清进展和下一步。执行留在原 Codex 工具；本 App 仅保存明确外部定位并提供打开/复制入口。来源事实、个人陈述、读取范围与未知结果分别展示。

2026-10-19 收缩取消自管 Codex app-server、任务控制、运行请求同步/回复和 Codex 退出协调；旧库不删除不重放。独立 ToolHost 的工具任务退出保护继续保留，见[本机工具设计](local-tools.design.md#DES-CC-TOOLS-003)。不扩展游戏 Agent、通用 IDE、远端 supervisor 或公开分发承诺。

## 2. 上游约束与相关角色

产品负责人维护用户承诺及范围；系统实现维护领域、存储和生命周期；接入实现维护 GitHub 的版本与能力及外部定位边界；验证工作覆盖对应条款的真实环境与故障情形。角色描述用于定位专业责任，不新建强制串行交接。

已有仓库与外部来源保持事实所有权。工程工具沿 engineering 专业域组织，产品四域、现行规范主责、Issue 可选和实际 PR/CI 交付方式不变。

### 2.1 需求承接与分配表

| 上游条款 | 独立义务与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-6e70287f-a156-4544-997d-5c1752e5efde；[REQ-CC-001](command-center.prd.md#REQ-CC-001) | App 独立启动，打开正确项目和工作树，恢复个人上下文；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=App 独立启动，打开正确项目和工作树，恢复个人上下文 | [DES-CC-001](#DES-CC-001) | [tests/foundation.rs](../../../tools/oasis7-command-center/core/tests/foundation.rs)；runtime_engineer；Tauri / 系统 Git | 不以游戏栈或终端后端为前提 |
| Trace refs: O7-REQ-2d1f3577-785e-40ac-8d25-ba0d414511f6；[REQ-CC-002](command-center.prd.md#REQ-CC-002) | 来源、版本、覆盖与失败分别保留；局部可用；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=来源、版本、覆盖与失败分别保留；局部可用 | [DES-CC-002](#DES-CC-002) | [tests/foundation.rs](../../../tools/oasis7-command-center/core/tests/foundation.rs)；各 adapter；上游查询能力 | 不推定完整全局快照 |
| [REQ-CC-003](command-center.prd.md#REQ-CC-003) | 当前与长期目标关联到适用验收，进度有可核实分母；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=当前与长期目标关联到适用验收，进度有可核实分母 | [DES-CC-003](#DES-CC-003) | [tests/foundation.rs](../../../tools/oasis7-command-center/core/tests/foundation.rs)；producer_system_designer；core PRD / 交付事实 | 不按提交数或活动量计算完成率 |
| Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；[REQ-CC-004](command-center.prd.md#REQ-CC-004) | 规范效力、历史理由、个人决定与正式回写可区分；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=规范效力、历史理由、个人决定与正式回写可区分 | [DES-CC-004](#DES-CC-004) | [tests/document_lifecycle.rs](../../../tools/oasis7-command-center/core/tests/document_lifecycle.rs)；文档主责；显式替代与源版本 | 不另建通用决策登记系统 |
| Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；[REQ-CC-004](command-center.prd.md#REQ-CC-004) | 决定版本、规范回写、实现同步及个人关注分别保存，旧数据恢复不复活已处理项；产品适用性：N/A；reason=工程客户端合同不定义游戏玩家能力；scope=决定版本、规范回写、实现同步及个人关注分别保存，旧数据恢复不复活已处理项 | [cc-decision-attention-state](#cc-decision-attention-state) | runtime_engineer；旧个人记录及具体证据来源 | 个人确认和停止关注不改源任务/规范/实现事实 |
| Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；[REQ-CC-005](command-center.prd.md#REQ-CC-005) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-005](#DES-CC-005) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；[REQ-CC-005](command-center.prd.md#REQ-CC-005) | 同一来源事实与有效个人关联一次组合到共享工作 DTO；产品适用性：N/A；reason=工程客户端呈现合同不定义游戏玩家能力；scope=关系覆盖、同版呈现与有界来源检查 | [cc-presentation-projection](#cc-presentation-projection) | runtime_engineer；现有 WorkProjection、个人记录与有限 bridge | 不新建任务身份、模型推导或来源权限 |
| Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；[REQ-CC-006](command-center.prd.md#REQ-CC-006) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-006](#DES-CC-006) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；[REQ-CC-006](command-center.prd.md#REQ-CC-006) | 真正待回复、待核实及知悉变化分组，处理后返回原工作且重开可恢复；产品适用性：N/A；reason=工程客户端合同不定义游戏玩家能力；scope=真正待回复、待核实及知悉变化分组，处理后返回原工作且重开可恢复 | [cc-attention-projection](#cc-attention-projection) | UI 实现 / qa_engineer；原请求和个人处理记录 | 无可读接口时个人陈述不冒充外部回执 |
| Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；[REQ-CC-007](command-center.prd.md#REQ-CC-007) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-007](#DES-CC-007) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；[REQ-CC-007](command-center.prd.md#REQ-CC-007) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [cc-start-intent](#cc-start-intent) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| [REQ-CC-008](command-center.prd.md#REQ-CC-008) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-008](#DES-CC-008) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| [REQ-CC-009](command-center.prd.md#REQ-CC-009) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-009](#DES-CC-009) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | 中文短查询、IME、键盘和有界长列表可用；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=中文短查询、IME、键盘和有界长列表可用 | [DES-CC-010](#DES-CC-010) | [src/App.test.tsx](../../../tools/oasis7-command-center/ui/src/App.test.tsx)；UI 实现 / qa_engineer；真实 macOS | 浏览器模拟不证明系统体验 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | 舒展默认、主次层级、宽窄及放大布局由原生截图与操作核对；产品适用性：N/A；reason=工程客户端视觉合同不定义游戏玩家能力；scope=四尺寸、150%/200%及主区/来源返回 | [cc-visual-composition](#cc-visual-composition) | UI 实现 / qa_engineer；真实 macOS App 与当前产品密度选择 | 数值不是已验收事实；VoiceOver 按用户选择不在范围 |
| [REQ-CC-011](command-center.prd.md#REQ-CC-011) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-011](#DES-CC-011) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| [REQ-CC-012](command-center.prd.md#REQ-CC-012) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [DES-CC-012](#DES-CC-012) | 核心兼容回归、宿主命令与 UI 检查 | 不提供 Codex 运行控制 |
| Trace refs: O7-REQ-ce7b612a-3316-4c40-9009-c0d295ff0ed6；[REQ-CC-013](command-center.prd.md#REQ-CC-013) | 首页聚合重点、成果、真实请求及返回工作，三场景可用；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=首页聚合重点、成果、真实请求及返回工作，三场景可用 | [DES-CC-013](#DES-CC-013) | producer_system_designer；主责文档/状态摘要与来源适配器 | 不随刷新运行模型或伪造排序、事实时间 |
| Trace refs: O7-REQ-fdf061f8-ebdf-46e3-91a5-a2311e2bcb93；[REQ-CC-014](command-center.prd.md#REQ-CC-014) | 明确关联复用、失效修复、统一详情及已有现场接续；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=明确关联复用、失效修复、统一详情及已有现场接续 | [DES-CC-014](#DES-CC-014) | application/domain、UI与受支持原工具定位 | 不从分支名移交运行控制，不自动合并个人/私有数据 |
| Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；[REQ-CC-015](command-center.prd.md#REQ-CC-015) | 本机工具作为独立辅助入口接续原工作；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=本机工具作为独立辅助入口接续原工作 | [DES-CC-015](#DES-CC-015) | 本机工具设计、context与既有 signer | 原暂停和私有隔离合同继续约束；工具不成为全部工作的前置 |
| Trace refs: O7-REQ-729d655d-d8a8-4c44-9ea7-32578dc10f3f；[REQ-CC-016](command-center.prd.md#REQ-CC-016) | 文档全局树、配对与覆盖透明；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=文档全局树、配对与覆盖透明 | [DES-CC-016](#DES-CC-016) | Rust 索引、文档 owner、Solid UI / QA | 不由树存在推断完成、全局完整或原生体验通过 |


## 3. 当前基线、目标状态与差距

文首固定基线保留历史来源；本轮目标以 §1 的产品收缩为准。现有文档/Git/PR/CI、个人决定与问题、文档树及本机工具继续使用；自管 Codex 生产入口退役。实际实现、原生验证和剩余范围由当前 PR 记录，旧验收不能代替本轮结果。

## 4. 边界与结构

<a id="DES-CC-001"></a>

### 4.1 App、核心与外部来源

Trace refs: O7-REQ-6e70287f-a156-4544-997d-5c1752e5efde；承接 [REQ-CC-001](command-center.prd.md#REQ-CC-001) 的适用义务。

桌面 App 由 SolidJS 界面、Tauri 宿主、Rust 核心与 SQLite 组成。宿主负责窗口、菜单栏、目录选择、受限 IPC 和经校验的外部导航；核心读取本地文档/Git 与只读 GitHub PR/CI，并保存个人决定、问题、关系和阅读位置。

Codex 是外部执行工具。本 App 不启动 `codex app-server`，不提供连接器配置、发任务、追加输入、中断、运行请求同步/回复或退出协调。已有会话只保存用户明确提供的原工具定位；打开或复制定位不构成连接、接管、运行状态读取或完成证明。

RepositoryScope 只描述明确选择的共同仓库与工作树阅读范围。原 Project.id、environment、git_common_dir 和私有库注册身份不合并或迁移；同 remote 不自动合并不同 clone。主线与工作树来源分别显示，不能把总览基线当作执行授权。切换项目只改变阅读范围，不能向外部工具发送内容。

### 4.2 代码落点与职责

以下路径承载客户端实现。新增模块名是本设计的实施落点，不表示文件已存在；保持一个独立核心 crate，不抽出通用任务服务。

| 路径 | 职责 |
| --- | --- |
| `tools/oasis7-command-center/Cargo.toml`、`Cargo.lock` | 小型独立 workspace，成员 `core` 与 `src-tauri` |
| `tools/oasis7-command-center/core/src/` | 领域、应用、适配器、存储、来源协调 |
| `tools/oasis7-command-center/src-tauri/` | 薄宿主、配置、production capabilities、打包 |
| `tools/oasis7-command-center/ui/` | 独立前端包、生成 DTO、有限 bridge、界面测试和 lockfile |
| `tools/oasis7-command-center/tests/fixtures/` | 文档、GitHub 和兼容回归样本 |

| 现有落点 → 目标职责 | 具体改动边界 |
| --- | --- |
| `core/src/work_projection.rs`、`daily_work.rs` / `daily_work_types.rs` → 呈现投影 | 保留来源 WorkRef、逐字段证据与个人记录；增加一次组合的首页/列表/详情及独立来源检查 DTO，见 §4.4 |
| `core/src/index.rs`、`github.rs` | 继续负责有界来源读取；补来源事件/观测/拉取时间、窗口与已知对象补读，不在 adapter 内决定目标完成 |
| `core/src/lib.rs` → 应用协调与存储接口 | 分离读取候选和提交，处理关系修复、决定同步证据、view revision；个人数据写入使用 record revision/CAS |
| `src-tauri/src/commands.rs` | 复用锁外 I/O 和有界发布；查询已读快照，保存时原子校验个人/决定版本，不因呈现查询重新扫描 |
| 外部会话定位与宿主导航 | 校验 `codex://threads/<UUID>` 后打开；其他定位仅复制，不提供运行控制 |
| `ui/src/App.tsx`、`WorkOverview.tsx` → 共享工作区组件 | AppShell 管导航与主内容；HomeOverview/WorkList/WorkDetailPane 消费同一投影；SourceInspector、DecisionSync、ToolTask 按动作进入，不在 JSX 再算一套完成/关注状态 |
| `LocalTools.tsx`、`DocumentInventory.tsx` | 接入原工作返回上下文；配置/诊断按需展开，具体工具与文档树合同沿配对设计 |

实现时在根 workspace 明确排除应用路径，按 [Cargo workspace](https://doc.rust-lang.org/cargo/reference/workspaces.html) 规则维护独立锁文件。沿用仓库 Rust 与 Cargo/sccache 缓存策略，不改写 `CARGO_TARGET_DIR` 或为每项任务建新缓存；不依赖模拟器、游戏 Agent 协议或 Viewer 启动。

### 4.3 具体技术栈

| 层 | 选择 | 所有权与取舍 |
| --- | --- | --- |
| 桌面 | Tauri 2 受支持稳定补丁 | macOS 系统 WebView；生产静态资源，无常驻 HTTP / Node 后端 |
| UI | SolidJS、TypeScript strict、Vite | 四视图、输入、键盘、焦点、路由与显示状态 |
| 控件 | Kobalte、TanStack Solid Table / Virtual、CSS 变量 | 语义控件与按需虚拟化；简单列表不强加复杂组件 |
| 异步与错误 | Tokio、tokio-util、thiserror | 共享 I/O runtime、有界通路、取消与清晰错误 |
| DTO | serde / serde_json、ts-rs | Rust 定义；TS 类型生成、有限 bridge 和真实 JSON 契约各负其责 |
| 持久化 | SQLite、rusqlite bundled | 专用数据库线程独占连接；避免同步查询阻塞 UI 或协议 I/O |
| 文档与变化 | pulldown-cmark、notify | 结构化章节、受限渲染；事件提示配合重新读取 |
| 来源访问 | 系统 Git CLI、reqwest + rustls | 参数数组、机器输出；HTTP 超时、分页、条件请求和限额 |
| 凭据与诊断 | Keychain；keyring-core + apple-native-keyring-store；tracing | 明确平台 provider，凭据只在 Rust；诊断限量脱敏 |
| 验证与分发 | Rust tests、Vitest、WebdriverIO Tauri service；Tauri bundler / updater | 分层验证、macOS 安装包；更新按需加入 |

[Solid](https://docs.solidjs.com/concepts/intro-to-reactivity)、[Kobalte](https://kobalte.dev/docs/core/overview/introduction)、[TanStack Table](https://tanstack.com/table/latest/docs/framework) / [Virtual](https://tanstack.com/virtual/latest/docs/framework) 支持当前交互组合；选 Solid 的理由是适配工作负载和已有工程经验，未声称比 React 更快。[ts-rs](https://github.com/Aleph-Alpha/ts-rs) 只承担类型生成，不生成完整命令客户端或运行时校验。[rusqlite](https://github.com/rusqlite/rusqlite) bundled 统一 SQLite 构建，数据库线程、队列与事务公平性由应用实现。

具体 patch、MSRV、Cargo feature、插件兼容性和许可证组合在实现锁定时核对，不能把 `latest` 或 Viewer 声明范围当作已验证锁定组合。当前不引入 ORM/数据库池、向量库、SSR、生产 Vite 服务或通用插件框架。

<a id="cc-presentation-projection"></a>

### 4.4 共享呈现投影与查询边界

Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；承接 [REQ-CC-005](command-center.prd.md#REQ-CC-005) 的同一工作事实组合；首页和导航分别沿 [DES-CC-013](#DES-CC-013)、[DES-CC-014](#DES-CC-014)。

在已有 `WorkProjection` 上增加可重建、类型化的 `WorkPresentation`：先核对来源 scope/generation 和持久私有排除，再读取同一个人 revision，最后一次组合有效目标关系、字段结论、真实请求、事件和可用动作，发布首页、工作页与详情共用的 viewRevision。Rust 拥有该组合；Solid 只作布局、选择、筛选、草稿和展开，不并行查询个人记录后自行推导第二套关系、冲突或成果。保存个人记录只重算已取得来源上的投影，不重跑 Git/gh/扫描。

`WorkItem.goals` 必须包含当前有效 `work_relations_v1` 关系及其原依据/定位状态；旧 associations 仅在明确兼容导入规则下参与。个人修复、移除标记和已确认多目标按 §7.5 优先组合，不能被 legacy 空值或旧关系刷新覆盖。关系不可读时保留“关系范围未知”，不退成无关联；建议仍独立于已确认关系。已知 WorkRef/条款直接打开其详情/依据；新建关系先读已有关联和当前范围的有界候选，无须重新搜索已经明确的对象。

`query_work_presentation` 返回独立 schemaVersion、RepositoryScope、source revision 集合、personalRevision、viewRevision、分区状态和有界页面；DTO 从 Rust 生成并执行 JSON 契约核对。它复用现有来源及存储，不新建任务主表或权限系统。旧 `query_work_projection` 及其 schema 1 响应继续可读；新 UI 不混用不同 revision 的首页、详情和个人关系。旧响应晚到只能留在原 scope，当前来源检查不修改已显示事实。

`query_source_inspector` 单独版本化，输入允许范围内的 SourceRef、原 revision 和 cursor，只读取已有索引/缓存中的原文、字段时间、coverage、完整路径/OID及可公开诊断。打开检查不执行扫描、远程补读或模型；版本不可取得时说明原版本缺口，由用户沿现有刷新入口明确补读。每次最多 50 条/256 KiB，cursor 绑定来源与版本；超限分页并说明范围，秘密或私有正文继续在进入普通 DTO 前排除。检查结果失败只影响该来源面板，不清掉主工作、草稿或其他来源。

这些组件名表示当前 App/WorkOverview 的职责拆分，可沿现有目录实现；不建立通用布局平台或另一套数据模型。SourceInspector 按 §9.1 在详情内或替换次级面板打开，DecisionSync/ToolTask 的输入与结果继续使用原记录/操作合同。

## 5. 关键运行流程

<a id="DES-CC-004"></a>

### 5.1 打开项目、索引规范与记录决定

Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；承接 [REQ-CC-004](command-center.prd.md#REQ-CC-004) 的适用义务。

打开项目后，按 [项目地图](../../../skills/oasis7-command-center/references/project-map.md) 找到当前主责入口与显式引用；目标读取 core PRD 当前阶段章节，规范效力读取主责身份、范围和替代关系，不按 mtime 排序。已提交基线和 dirty 工作树分别索引与展示，切分支或修订变化使相关投影失效。

[pulldown-cmark](https://docs.rs/pulldown-cmark/latest/pulldown_cmark/) 产生可定位的标题、段落、表格、代码与链接块；解析语法不自动决定条款效力。已有条款 ID 优先，标题定位须处理重复；行号只辅助跳转。保存原版本引用与可取得内容，历史工作可回到当时条款并找到明确后继。

按真实阅读版本计算“自上次查看变化”；没有历史内容或明确阅读记录时说明缺口。用户决定草稿、已明确决定、正式条款更新、实现同步分别记录。首版通过原文编辑入口或明确的 Codex 工作回写主责文档，只以实际结果更新来源；保存草稿不改规范效力，也不创建另一个通用决策 registry。

决定的三条状态、证据和终态遵守 [§7.4](#cc-decision-attention-state)。确认意图仅改变决定状态；后续补理由、关联来源或查看决定均不改原确认时间。正式回写和实现同步分别记录适用范围及证据，来源中未发现关联只表示未核实，不能让该决定永久出现在“需要我决定”。

[notify](https://docs.rs/notify/latest/notify/) 事件只提示失效，合并去抖后重读。初次载入、唤醒、重新聚焦、手动刷新和检测到遗漏时校准；必要时受限轮询。不索引 `target`、`node_modules` 或 Git objects，不能把全目录递归监听当作一致性保证。

### 5.2 Git、GitHub 与刷新发布

Git 采用 [status](https://git-scm.com/docs/git-status) 的 `--porcelain=v2 -z --branch`、[worktree](https://git-scm.com/docs/git-worktree) 的 `list --porcelain -z` 等机器输出。Rust 用参数数组、明确 cwd、超时和输出上限调用，处理空格/换行路径与 detached HEAD。读取不隐含 fetch、切分支、reset、提交或清理；本地远端引用只是最近所知状态。

GitHub 优先经 `gh api --method GET` 调用 REST，不使用 `gh repo view`、`gh pr list` 等隐式 GraphQL 查询。仓库定位读取选中工作树的 GitHub origin 并用 REST 仓库身份核实；默认仅取 PR 列表第 1 页、最多 100 项，不批量展开所有 PR 的检查或评审。用户在工作详情明确读取某个 PR 的 CI 时，才对其原 HEAD 有界读取 check-runs 与 combined statuses 各第 1 页、每页最多 100 项，分别保存 GitHub 关联提交、读取时间和截断范围；`head_sha` / combined-status `sha` 不是实际 checkout 对象的执行回执，未取得测试对象证明时 tested OID 保持未知；未读取不作成功空或评审通过。PR HEAD 或项目/来源 generation 变化时，丢弃迟到的 CI。触及限额时保留旧事实并明确提示，不自动重试或回退 GraphQL。继续复用固定只读参数的 gh adapter；进一步扩展分页范围或改用共享 reqwest Client 时仍按 host/account 绑定，并遵循[条件请求、分页和退避建议](https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api)。桌面首版没有公网 webhook 接收服务，使用克制轮询，优先当前可见与显式关联的 PR/检查；Issue/Project 按需。近期列表没有包含某个已知 PR 时，详情可在该来源获准且有能力的条件下按其真实身份补读；列表截断、404 和未读取分别保留，不能由缺席推断未合入或删除。

初始刷新预算：每个 Git/gh 子进程或 HTTP 请求最多 10 秒，单来源刷新总期限 30 秒；列表每页最多 100 项、一次最多 10 页，已知对象补读一次最多 20 个且计入同一总期限。当前 gh 默认读取首 100 个 PR，显式后续第 2–10 页和已知 PR 按身份补读已实现；这些有界入口不代表已读取全量仓库事实。到达页数/时间预算返回已读范围和剩余 cursor/原因；用户明确“继续读取”开启有界新尝试，不能在后台无限追页。限额按来源返回的 retry-after 显示下次可重读时间，不在截止后持续等待或忙重试；分页期间来源变化保留各页实际版本/时间，不宣称原子快照。

一次刷新按“读取候选 → 核对版本/范围 → 事务应用 → 发布 view revision”处理。慢 generation 不能覆盖新快照。分页记录时间范围与完整度；即使页已读完，也不宣称各字段属于同一瞬间。403、404、限额、超时、部分页和成功空结果分别处理，失败保留最近成功数据。

<a id="DES-CC-006"></a>

### 5.3 工作操作与待处理请求

<a id="cc-attention-projection"></a>

<a id="DES-CC-009"></a>

Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；承接 [REQ-CC-006](command-center.prd.md#REQ-CC-006) 的适用义务。

Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；承接 [REQ-CC-006](command-center.prd.md#REQ-CC-006) 的关注呈现、处理与恢复。

待我处理只聚合用户保存的个人问题、明确需要决定的事项和实际可读来源中的关注内容；普通变化与待补事实分别展示。没有个人问题时显示轻量空状态，不显示“Codex 未连接/请求未知”的常驻面板，也不虚构等待输入数量。

个人回答只更新个人记录。来源是否已变更由其实际读取结果核实；打开原工具后返回不自动宣称外部已处理。不读取旧 runtime 请求，不提供协议回复入口。不同问题保留原 ID，刷新或重开不复活已处理个人事项。

### 5.4 关闭、完整退出与恢复

关窗保留菜单栏入口，恢复原阅读位置；完整退出保存个人数据并完成独立工具任务安全核对后退出。App 没有自管 Codex 进程，不再依据旧活跃/未知 Codex 运行记录显示等待、停止或后台执行选择；独立 ToolHost 的工具任务退出保护仍有效。外部 Codex 工作的生命周期由原工具管理，关闭或退出本 App 不发送停止、恢复或重放指令。

重开恢复个人记录及可用来源；旧运行记录在存储中保留，但不恢复连接、会话控制或请求协调。

### 5.5 持续状态维护与架构阅读

按[持续更新流程](command-center.manual.md)维护两个派生仓库入口：`command-center/README.md` 承载长期/短期目标、当前进展、近期变化与人工关注；`command-center/architecture/README.md` 承载关键设计与部署索引。两者通过原有文档适配器读取并下钻源条款，不建立第二套任务表、产品权威或部署库存。

来源事实改变后，负责当前工作或回顾的执行者核对影响，在已有授权内维护主责和摘要；普通客户端刷新仅重新读取，不能自行写入仓库、创建 PR、SSH 采样或启动模型。合入后状态在下一次刷新协调，不为每次合入递归生成状态 PR。目标、已合入代码、实际测试、部署、目标验收使用各自来源。

摘要声明的文档基线、来源观测窗口与实际环境核验分别保留。文件索引的 revision/读取时间只说明读取了哪个摘要版本；没有读取其引用的 PR/环境，不能刷新其内部事实时间。原始内容中缺少时间、覆盖或来源时展示缺口，不推测。当前四区和通用摘要入口已存在；§4.4 的统一呈现、§5.6 的自然日成果窗口与 §9.1 的视觉层级仍属本增量目标。后台采集不是本轮基础首版的必需能力，不由本节文字宣称完成。

架构索引从事项进入原有主责条款，分别展示设计边界、已有实现与实际部署证据。显式后继读取沿 §11.0.2 复用；索引条目不能仅凭文档 active 或引用关系成为权限、写入权、部署就绪或完成证明。历史清单保留原采样日期，未知环境状态不补造实时拓扑。

<a id="DES-CC-013"></a>

### 5.6 首页判断投影

Trace refs: O7-REQ-ce7b612a-3316-4c40-9009-c0d295ff0ed6；承接 [REQ-CC-013](command-center.prd.md#REQ-CC-013) 的适用义务。

application 在既有来源快照之上生成 `HomeProjection`，四个区域及默认交互由 [REQ-CC-013](command-center.prd.md#REQ-CC-013) 与[首页布局](command-center.prd.md#cc-home-layout)主责。目标定位沿 core 当前阶段及显式主责引用；状态摘要沿现有稳定章节读取，复用 Markdown 块和链接，而非把文件全文直接放到首页。摘要无法结构化提取时提供该区正文的阅读入口及覆盖说明，保留其他已解析区域；只完成该降级不能宣称首页整体验收通过。

投影中的每个结论分别保留原文定位、源 revision、事实性质、摘要内部核验时间和读取时间。已核实 PR/CI 更新其对应字段，不覆盖主责目标；与摘要冲突时保留双方版本及冲突字段，不用一个“最新时间”覆写整条记录。摘要自己的归纳不升级为运行事实。

重点排序输入只有主责显式阶段顺序、用户已保存置顶及组内来源顺序；缺少优先级时保留并列。下一步引用现有主责或个人选择，分析建议单独带性质且不改变源关系。近期结果按明确 PR/交付身份聚合，保留事件时间和比较基线。待处理投影复用原请求身份和 §5.3 的已处理结果；人工关注、事实缺口和普通提醒分组，观察不全不生成虚构请求。

默认“近七日”为用户时区的今天及前六个自然日，显示具体日期、时区和“今天截至本次观察”。查询固定 timezone、asOf 与换算后的 UTC 半开区间，夏令时按日历边界处理，不能直接减 7×24 小时；用户改变窗口或时区仅重算投影。成果按源 eventAt 倒序，相同时以稳定事件身份排序；按原 WorkRef/事件身份归并重复观察，同一 PR 的提交/检查在其交付结果下展开，不叠加为多个已完成目标。不同真实事件保留各自时间，未知 eventAt 单列“本次观察到，事件时间未知”，不用 fetchedAt/lastReadAt 补造；“自上次查看”只有明确同范围阅读基线时才单独可用。

阅读基线保存实际展示的 WorkRef/事件集合、窗口/cursor、源/view revision 与用户阅读时间；观察到来源和用户已读分别记录。首页只展示三项时不能用一个 `new Date()` 把未展示的分页/截断窗口全标已读；进入其他页后只提交该已展示范围的阅读记录。来源覆盖改变而无法比较时说明缺口，仍可看固定日期成果，不生成完整“自上次查看变化”的保证。

| 分区 | 投影输入与输出 | 缺口/动作边界 |
| --- | --- | --- |
| 当前重点与下一步 | 目标原条款、已核实成果、剩余条件、显式下一动作、关联 WorkRef；默认最多三项，其余可展开 | 无明确排序保留并列；不从 PR 数量推算优先级，不让用户先填写关系才显示可读取目标 |
| 最近成果变化 | 按用户选择的比较窗口、事件身份与适用版本聚合真实结果，链接同一工作 | 同一 PR 的提交/检查/合入不重复算成果；没有事件时间则不伪入窗口，改列“本次新观察到”并说明时间未知 |
| 需要我处理 | §7.4 的仍需用户回答事项与数量，事实缺口和知悉信息分组 | 没有真实待答项可以为空；来源不可用显示该范围未知，不能显示“全部处理完” |
| 返回工作 | 已确认工作关系、用户近期进入位置和未发送草稿存在性 | 不包含私有正文、秘密或私有线程；没有历史时给可读工作入口，不创建空任务 |

`HomeProjection`、`WorkDetailProjection` 和 `AttentionProjection` 从同一已提交来源集合及个人记录 revision 生成，并发布共同 `viewRevision`。各区返回自己的可用/部分/旧结果和缺口，来源集合并非原子远端时刻；一处失败不清空其他区。投影缓存键包括 schema、RepositoryScope、实际阅读基线、来源 revision 集合、个人记录 revision 和比较窗口，缓存缺失可重算。一次保存/处理成功使相关投影失效；旧计算结果提交前须核对全部输入 revision，不能重新引入已处理问题。

项目选择和连接配置只在首次打开或用户展开时占据主要空间；身份、SHA 和诊断按需展开，当前判断受影响的具体缺口直接显示。查询先返回已验证身份下的最后成功投影及分区状态，再异步校准来源；新恢复路径未完成身份校验时只提供带“待核对”标记的安全阅读，不开放执行。后台全量扫描不作为每次 `query_view` 的前置。

<a id="DES-CC-014"></a>

### 5.7 统一工作详情、关联与原工具接续

Trace refs: O7-REQ-fdf061f8-ebdf-46e3-91a5-a2311e2bcb93；承接 [REQ-CC-014](command-center.prd.md#REQ-CC-014) 的适用义务。

application 以 §6.2 的 WorkItem 原来源身份构造[统一工作详情](command-center.prd.md#cc-work-detail)。首屏先返回工作目的、当前可核实结果、主要剩余条件和可用下一动作；来源/目标关系、PR/CI、会话、真实请求及操作回执作为同一详情的区域。原文、完整 SHA、协议身份和诊断可展开，不抢占首屏判断。不同视图只传相同工作引用和返回位置；详情不创建新任务表或将多个源状态压成统一完成值。未产生 PR 的明确会话仍可作为工作入口，后续取得显式 PR 关系再关联到原工作。

关联保存来源双方身份、原依据版本、关系种类、提出方式（源明确引用、用户确认或建议）、适用工作树及当前可定位性。读取到 URL 只证明链接存在；只有正文明确表达的服务目标、验收或会话关系才能投影为该语义。候选查询优先当前工作已有关系、来源明确引用和用户选择范围，不默认展开上百个全仓复选框。相同端点/种类/范围关系去重，重复刷新只更新观测；具体修改、冲突和移除沿 §7.5。采用个人决定的回写证据仍引用真实源 revision，未知同步结果不自动转为已完成。

外部会话入口先使用已支持且获准的来源接口或用户明确保存的引用；详情仅显示明确外部定位及其打开/复制能力。没有会话能力时保留 PR/工作树定位与缺口。导航校验复用有限来源打开命令及允许的协议/目标，不能运行任意 shell 或解析私有库来宣称接管；没有受支持的深链就提供可复制定位和实际原工具入口，不伪造跳转成功。新入口格式需独立校验，不能把任意文档 URL 作为命令执行。

原工具打开结果只证明操作系统接受导航请求；返回 App 后只重新读取有权取得的来源事实，不标记外部工作已恢复或完成。

详情进入 §9.1 的主阅读区；首页、工作列表、关注、关系和工具返回都通过原 WorkRef 定位同一详情，不把它追加在长来源列表底部。返回上下文保存入口、筛选、列表/详情位置和焦点；切换同一工作保留页签，选择其他工作先恢复该工作的状态。工具动作及新增输入表单按动作打开，结束后回到原 WorkRef/页签并显示实际结果。

现行个人编辑只涉及决定、问题、关系与本机工具；输入留存沿各自入口，不提供模型任务输入表单。旧导航中的任务输入、每工作运行草稿与私有派生内容只兼容保留，不迁入现行个人编辑、不提供认领或发送入口。工作选择、筛选与阅读位置继续恢复。

<a id="cc-start-intent"></a>

### 5.8 一次明确发起与结果回读

Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；承接 [REQ-CC-007](command-center.prd.md#REQ-CC-007) 的外部工具接续边界。

此锚点保留供既有引用定位，原自管发起合同已退役。继续工作只打开原 PR、工作树或已明确记录的 Codex 原会话入口；无法使用受支持链接时提供准确定位的复制入口。不得自动创建新会话、发送提示词或把“已打开”显示为“已恢复/已执行”。返回后只刷新真实可读取的文档、Git 与 GitHub 结果。

## 6. 接口与数据合同

<a id="DES-CC-002"></a>

### 6.1 来源合同与错误

Trace refs: O7-REQ-2d1f3577-785e-40ac-8d25-ba0d414511f6；承接 [REQ-CC-002](command-center.prd.md#REQ-CC-002) 的适用义务。

关键对象包含 `SourceRef`：来源类别、主机/账号/环境、对象身份、可得 revision、事实性质与 coverage。事实性质区分源事实、Agent 汇报、模型归纳、用户选择和待核实关联。未取得的字段保留未知，不生成伪造时间或身份；[产品来源状态](command-center.prd.md#cc-source-states)所需字段在 Rust 统一产生，不由各页面自行解释。

| 字段组 | 语义与更新条件 |
| --- | --- |
| `sourceRevision` / `observedAt` | 实际读取的源版本与该事实被来源/采样者核实的时间；摘要正文有独立核验时间时保留它，缺失则未知 |
| `fetchedAt` / `lastAttempt` / `lastSuccess` | 本 App 拉取、尝试及成功读取的本地时间；重新读到同一旧正文只更新读取元数据，不改变正文事实的 `observedAt` |
| `eventAt` / 事件身份 | 合入、测试完成或其他来源明确事件的实际时间；未知时不能用文件 mtime、拉取时间或排序位置替代 |
| `coverage` | 仓库/账号、选定工作树/基线、对象或查询范围、已读页及游标/窗口、截断/遗漏/失败原因；“查询空”只在成功且该声明范围完整时成立 |
| `generation` / `viewRevision` | 本地读取代次和已提交投影版本；只比较同一来源键/读取范围的代次，不把本地计数当上游事件顺序 |
| `FactCell<T>` | 值、原 SourceRef、依据版本、事实性质、适用范围、核实/旧结果/冲突/未知状态及原因；每个结论可独立回溯 |

时间统一存可解析 UTC 值及来源时区/精度信息，UI 按用户时区呈现；没有上游时间时保留空值。不同来源时钟与精度不足以证明严格先后，不用“最近读取”覆盖跨源冲突。`stale` 必须带原因：如关联源已出现新版本、超出该来源声明的有效窗口、刷新失败沿用上次结果；仅有日期较老而无时效合同的规范不能自动退役。日期范围变化使成果窗口重新投影，不能改写历史事实。

| 字段 authority | 可更新的字段 | 不得顺带推出 |
| --- | --- | --- |
| 当前主责文档/明确验收条款 | 目标、适用条件、规范效力及显式下一步 | PR/CI 现状、实际部署或验收通过 |
| 派生状态摘要 | 其原核验窗口内的说明与判断，保留摘要性质 | 新拉取时即为当前事实，或凭摘要修改正式要求 |
| Git / GitHub | 对应工作树状态、PR source HEAD、PR 状态、真实事件；CI 的测试对象/结果按返回的实际字段保存 | 未读页没有工作、合入代表部署或整体目标完成；缺 tested OID 时声称测试覆盖新 HEAD |
| 原运行时/操作回执 | 原实例/thread/turn/request 的观察及该操作结果 | 所有外部会话状态、所有后台工作结束或产品验收 |
| 用户明确选择/确认 | 置顶、个人计划、关系与有证据的核实结论 | 改写源状态、接管会话或授予新权限 |

读取到比摘要窗口更新的 PR 合入事实时，用它展示该 PR 的状态和事件；摘要旧的“待合入”保留为过期依据，并指出冲突字段。目标、部署和验收仍保留各自来源，不能整条记录 last-write-wins。两个来源对同一权限范围内的字段冲突而无法判定时并列双方依据，将受影响结论设为待核对；其他字段继续可用。

来源加载、成功空、部分、旧结果、错误、未接入分别可见。错误 DTO 返回 `category`、受影响来源/字段、最后成功范围、必要诊断引用和有限 `recoveryAction`，区分未安装、未认证、无权限、目标不在覆盖范围、限额、超时、存储故障及版本冲突；UI 在原操作附近提供打开配置、重读该来源、查看原工具或解决冲突的具体入口。适合重读不等于适合重发副作用，不能仅凭 `retryable` 自动重发输入。

ID、Git OID、外部节点和本地 cursor 用明确字符串表示；可能超出 JavaScript 安全整数的值使用约定字符串等 JSON 编码。ts-rs 的 TypeScript `bigint` 不代表 JSON 自动转换。Rust 在实际协议入口解码，bridge 验证命令/参数/错误与 DTO 的 JSON 往返。

<a id="DES-CC-003"></a>

### 6.2 目标、规范与交付关联

| 对象 | 身份与关联规则 |
| --- | --- |
| Goal / Milestone | 引用当前主责条款、长期/当前阶段及显式关系；身份沿源文档/条款，不按首页卡片另发目标 ID |
| Rule / DocumentSection | 仓库/阅读范围、路径、条款 ID 或稳定章节定位、revision/content hash、适用范围及后继；标题/行号变化不自动替换历史依据 |
| WorkRef / WorkItem | WorkRef 为来源命名空间与原对象身份：GitHub host/repository/PR 或 Issue，或原会话 SessionKey。WorkItem 是目的、关联目标、规范、PR/CI、会话、请求、证据和可用动作的派生组合；PR 无 Issue 仍独立成立，分支不作主键 |
| EvidenceLink | 引用真实源对象/版本、适用 AC/范围与结果性质；有 URL 仅证明链接存在，相关性和结论核实另行记录 |
| DecisionRecord | 保留原决定 ID、内容/理由/范围、确认时间与修订；决定本身、规范回写、实现同步三条状态及各自证据独立，见 §7.4 |
| 状态摘要 / 架构索引 | 普通仓库文档及其稳定章节；记录派生性质、来源基线/核验时间/范围，引用 Goal、WorkItem 和主责条款，不创建新的目标或任务身份 |
| 首页 / 工作详情投影 | 组合已有身份及结果；各结论保留来源，项目总览不隐去工作树与环境边界 |
| 关系定位与修复 | 原始双方身份及版本、关系种类、提出/确认来源、记录 revision、解析状态和移除标记；见 §7.5，不删除原依据 |
| 原工具入口 | 工具/来源命名空间、环境和明确定位；仅判断可打开或可复制 |

同一工作可服务多个目标；建议依赖不自动成为阻塞。仅当适用验收集合、分母与结果均可核实时显示验证比例，未核实部分单列。文档 active、PR merged、CI passed、turn completed、部署和目标验收各说明自身事实，互不代替。

显式关系可以把同一 PR 的多个会话组合到一个详情，仍保存全部原 WorkRef/SessionKey；同分支或同标题不证明属于同一工作。没有 PR 的会话先以原会话引用展示，后续出现明确 PR 关系时旧入口可转到该组合详情，原引用和返回位置继续有效。关系有歧义时保留独立工作和候选，不分配“统一任务 ID”来掩盖不确定性。临时输入预览只有草稿/operation 身份，未产生源工作前不计为进行中成果。

PR source HEAD、实际测试 SHA、main 前进和验收分别关联：旧 SHA 绿灯不覆盖新提交；main 前进也不自动否定同一 source HEAD 的适用 CI，解释服从当前流程主责。

<a id="DES-CC-005"></a>

### 6.3 会话身份、运行绑定与 Codex 边界

Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；承接 [REQ-CC-005](command-center.prd.md#REQ-CC-005) 的适用义务。

会话引用是外部定位信息，保留原工具、原标识与用户确认的工作关系，不扫描全机会话，不按同分支推断绑定，不读取实时会话状态。仅有旧自管记录时不得导出旧输入或私有内容到普通工作投影。

原 runtime instance、turn、request、operation 等存储身份保留其历史含义，不转换成当前可控对象。普通页面与新查询不得消费这些旧记录产生待答、活跃工作或退出阻塞。

<a id="DES-CC-007"></a>

### 6.4 应用 IPC 与能力校验

Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；承接 [REQ-CC-007](command-center.prd.md#REQ-CC-007) 的适用义务。

IPC 只暴露项目/文档/Git/GitHub 查询、个人记录、文档树、本机工具及外部导航的必要命令。删除 Codex 连接、配置启用、建会话、发输入、中断、请求回复、运行订阅、协调退出等宿主命令和 UI bridge；不以隐藏按钮保留可调用控制入口。

外部导航校验协议与定位参数，使用明确允许的原工具入口；来源文本不能触发任意本机命令或 shell 初始化。个人问题保存、决定 CAS 和本机工具授权保持各自合同。

## 7. 状态、事务与持久化

<a id="DES-CC-011"></a>

### 7.1 三类本地数据与事务边界

| 数据组 | 内容 | 清理与恢复 |
| --- | --- | --- |
| 可重建快照 | 章节、FTS、PR/CI 投影、已读取会话摘要 | 可分来源重建；保留最近成功时间与范围 |
| 个人持久数据 | 布局、筛选、阅读版本/位置、手工关联、草稿及已确认未回写的 DecisionRecord | 独立备份与迁移，索引清理不得删除 |
| 操作记录 | operation、原目标、上游回执、未知结果、待协调关联 | 重开仍保留，已协调后才按保留策略清理 |

按逻辑数据组隔离，普通、旧 runtime 与 tools SQLite 文件不合库。普通记录保持现有版本与身份；旧 runtime 文件不删除不重放，不作为工作或待答来源。ToolJobs 仍沿独立短事务和退出保护合同。新增个人记录使用有界 DB 请求与短事务，存储失败不冒充成功。

快照替换和对应 source 元数据在同一事务提交后发布内容绑定的 view revision；查询不能看到半批更新。缓存重建只作用目标表组，不能删整个库修一个索引。决定从草稿变为用户已确认时仍属个人持久数据，内容、理由、范围与真实确认信息不能降为可重建缓存；重启和索引重建均保留。凭据、敏感展示及诊断边界见 §8。

首页/工作详情仅缓存可重建组合；关联确认/修复/移除、决定同步证据及用户选择留存的返回位置是个人记录。知悉已读、个人问题已处理及“已在原工具处理”的个人陈述保存为独立 `work_attention_v1` personal 记录，保留原问题/请求引用、scope、状态、记录 revision 和时间；旧 runtime 请求/秘密回复只在原操作存储保留，不复制到普通关注记录。个人修改先在同一短事务写记录及其 revision，再使关联投影失效；应用回执只在事务提交后显示已保存。物理写入失败与事实尚未核实是两类状态。范围切换不取消已获准的个人保存，但结果必须回到原 scope，不能覆盖新页面的记录。

### 7.2 流量控制与故障隔离

<a id="cc-host-concurrency"></a>

文档解析、GitHub 子进程、DB、分页与来源发布保持有界队列和锁外 I/O；取消或切换项目后拒绝旧结果覆盖新范围。读取失败保留最近成功事实，个人写入失败保留当前输入并显示实际错误。

不存在协议帧、runtime 控制队列或运行事件 writer；删除其生产启动路径，不用旧运行状态锁住普通来源查询和退出。

<a id="DES-CC-008"></a>

### 7.3 操作记录与不确定结果

原 Codex 操作/请求/StartIntent 数据只作兼容保留，不删除、不迁移为个人问题、不恢复处理、不重放发送。旧 enabled=true、active、unknown 或未决请求均不启动进程、不出现可控入口、不阻塞退出。旧库打开与重建来源缓存不能改写这些历史记录。

来源读取结果未知仍按真实覆盖显示，个人写入成功必须依据实际事务结果。外部工具的运行结果不可由本地按钮、个人陈述或历史回执推定。

<a id="cc-decision-attention-state"></a>

### 7.4 决定、同步证据与关注状态

Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；承接 [REQ-CC-004](command-center.prd.md#REQ-CC-004) 的决定修订、同步证据与个人展示状态。

三条决定状态分别承接 [REQ-CC-004](command-center.prd.md#REQ-CC-004) 与 [REQ-CC-006](command-center.prd.md#REQ-CC-006)。决定本身属于个人明确意图；规范及实现结果仍由各自来源拥有，不能用一个 `confirmed` 值统管它们。

| 轨道 | 状态与转移 | 完成/变化的证明 |
| --- | --- | --- |
| 决定本身 | `draft → decided`；后续明确替代/撤回用 `superseded` / `withdrawn` 并保留原记录，不退回未确认草稿 | 用户明确确认保存内容、范围与确认时间；一般编辑增加修订及编辑时间，不改原确认时间。实质新决定单独记录并关联前身 |
| 正式条款回写 | `unknown / notRequired / pending / evidenceLinked / verified / needsReview` | 未知不等于待批准；确认不适用须保留理由/范围。关联原 PR/文档 revision 后仅为有证据入口；核实该版本实际承接决定的适用范围后才为 verified，来源失效/明确修订冲突转 needsReview |
| 实现同步 | 同上，独立于条款回写 | 关联真实改动、版本及适用测试/结果，说明只证明的范围；PR 合入不自动证明语义符合，未运行验收保留未知。来源改变只重核受影响义务，不因 main 前进撤销所有已有结果 |

`verified` 的依据是可回读的具体来源加明确核实结论，记录核实者/来源、时间、范围和记录 revision；App 只读刷新最多发现/更新证据入口，不凭关键词自动核实整个决定已实现。证据无法读取时保留上次核实结果作为历史依据，当前显示“待核实/不可读”，不能无条件计为当前同步完成；明确反证或不适用版本进入 needsReview。范围部分覆盖须逐项保留余项，不能用一项 CI 覆盖全部同步条件。旧 `notVerified` 导入为 unknown，保留旧确认信息，不造 pending 工作或虚假完成。

决定/证据表单加载时取得 `decisionRevision` 与按固定字段/编码计算的 digest，覆盖用户实际阅读的内容、理由、范围与确认/替代状态；保存携带该原值、个人 expectedRecordRevision 和具体证据源版本。Host 在同一写事务内读取当前决定记录并核对两类版本，再保存结论及原依据；不能只在事务前 reload 或依赖 Application 内存快照。服务端可重算 digest 作比较，不能把旧表单的 expected digest 换成最新值后接受。查看后改了决定/范围、另一视图写入或证据版本变化，都返回受影响差异并保留输入，用户重读后再明确确认；普通关系保存变化不应无故使未变的决定内容失去其版本身份。

新 Host 的全部决定编辑、确认、替代/撤回和证据入口，包括兼容命令，汇入同一原版本事务/CAS；新决定以明确的“原记录不存在”条件创建。旧版无阅读版本字段的记录仍可读取并保留历史，但缺 expected 版本的旧表单写请求返回需重新查看的兼容错误，不能由服务器补当前版本来接受，更不能新标 verified/notRequired。旧证据缺少用户阅读版本绑定时保留原结论及旧证据时间作为历史、当前标明待核对，不补造“用户已看过当前版本”的证明。

全部适用同步义务已核实或有理由地不适用时，决定可从待同步列表进入历史；个人停止/恢复关注则仅修改 `work_attention_v1` 的展示偏好，不改变三条状态或原请求。恢复关注仍显示真实未决义务，已隐藏项可从完整范围核对；停止关注不伪造来源完成，不产生外部请求回复。

当前关注投影只包含明确个人问题，保留原问题 ID、WorkRef、种类、状态与个人记录 revision；不包含旧 runtime 请求。

首页“需要我处理”只显示仍需用户输入且原请求/问题未终止的 open 项；等待来源核实的项进入“待核实”，resolved/superseded 保留历史入口。用户已作决定即结束该个人问题的待答义务，规范回写未完成可保留工作剩余条件；只有实际出现需用户选择的新问题才建立新的待答项。知悉变化可标记已读，不修改源工作；事实未知不能批量转换成需要批准。

个人回答与关闭保存原问题和版本；刷新和缓存重建不复活已处理问题。个人陈述不证明外部已回复或规范已回写。

<a id="cc-link-lifecycle"></a>

### 7.5 关联生命周期与并发修改

关系的“是谁提出/确认”与“当前能否定位”分别保存，避免把源明确引用、个人确认和猜测混成一个 active 状态。

| 事件 | 持久/派生结果 | 用户与来源边界 |
| --- | --- | --- |
| 读到明确源引用 | 保存源端点、关系种类、源版本和语义依据；由该来源投影，仍可与个人关系并存 | 普通链接不升级为依赖；源更新后重读，其历史依据不丢失 |
| 出现建议/用户确认 | 建议保留候选和理由，确认后保存个人 relation revision、确认时间与范围 | 建议不参与完成/阻塞判断；确认不改源文件，不授予会话控制 |
| 条款修订/重命名 | 优先同一显式稳定身份，再核对明确后继及适用范围；保存新定位和原依据 | 唯一且范围可核实的后继可更新解析定位，并显示变化；语义替换或多候选须用户修复，不能靠标题相似强行迁移 |
| 缺失/冲突/读取失败 | 保留原关系，解析状态为 `missing / ambiguous / unavailable / changed` 并带候选/原因 | 未读取不是删除；失败不清空已确认关系。只有相关工作显示缺口 |
| 修复/变更 | 用户从原关系查看差异并确认新端点/种类/范围，CAS 更新记录 | 原端点与版本留存；失败保留选择，冲突不静默覆盖另一视图的修改 |
| 移除 | 个人关系记录 removal 标记；源关系可保存个人隐藏/不采用选择 | 不删除源 PR/会话/条款、不伪称源已取消关系；同一关系重读或仅源 commit 变化不复活已移除项，真正新声明可列为变化但不自动恢复个人关系 |

关系键为两端稳定身份、种类及适用范围，原文版本作为依据版本另存；刷新不能因为行号变化生成重复关系。修复/移除后的个人记录先提交再失效首页/详情投影；同一 work 同时服务多个目标仍引用同一源工作。用户只隐藏关系时，原源声明及其真实性仍可在来源详情核对。

## 8. 部署、安全与运行约束

生产 WebView 仅载入打包静态资源，Node/npm 只用于构建测试。浏览器开发预览用同一 DTO 的 fixture/replay provider；真实系统能力在 Tauri 路径验证，不为预览额外建设常驻 HTTP 服务。

GitHub 可复用用户选定的已登录 gh 账号，通过官方 [gh auth token](https://cli.github.com/manual/gh_auth_token) 在 Rust 短期使用；gh 缺失不影响本地能力。可另选具有实际所需仓库权限的 token；长期保存走 Keychain，数据库仅留引用。

采用 [keyring-core](https://docs.rs/keyring-core/latest/keyring_core/) 与 [apple-native-keyring-store](https://docs.rs/apple-native-keyring-store/latest/apple_native_keyring_store/) 的 `keychain` feature，显式安装真实平台 provider。mock/sample store 不进生产；系统存储失败时可明确退为会话内使用，不能静默落明文。切换账号、主机或环境重新识别来源，展示身份及读取范围。

依据 [Tauri capabilities](https://v2.tauri.app/security/capabilities/) 与 [CSP](https://v2.tauri.app/security/csp/) 设置生产窗口权限，把 app commands 纳入 `AppManifest::commands`，使用含已知安全修复的受支持稳定补丁。Rust 再次校验路径/身份/能力，不暴露任意 shell、SQL、全磁盘读写、任意 URL 代理或协议方法透传。

仓库文档、PR 内容和日志仅为展示数据；Markdown 原始 HTML 禁用，代码与高亮转义，默认不自动请求远程图片，外链经协议校验由系统浏览器打开。来源文字不触发本机指令。tracing 诊断按容量轮转，只记必要来源、operation ID、类别与耗时，过滤 token/敏感参数，不默认保存完整提示词或 transcript，不自动上传。

从 Finder 启动不假定继承交互 shell 的 PATH、代理或 SSH agent。显示 Git/gh 的实际路径与版本，允许明确选择路径；不执行用户 shell 初始化脚本来修复环境。

macOS 产物为 `.app` / `.dmg`，按实际目标硬件验证 arm64 和需要支持的 x86_64。对外分发采用 [Developer ID 签名、公证与 stapling](https://v2.tauri.app/distribute/sign/macos/)，不假定本次已有证书或发布凭据。首版可手动安装；需要自动更新时用 [Tauri updater](https://v2.tauri.app/plugin/updater/) 与静态 manifest，更新包签名和 Apple 代码签名分别验证，重启遵守 §5.4。

## 9. 质量与容量

<a id="DES-CC-010"></a>

<a id="cc-visual-composition"></a>

### 9.1 中文、可访问性与信息密度

Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；承接 [REQ-CC-010](command-center.prd.md#REQ-CC-010) 的适用义务。

[产品信息层级](command-center.prd.md#cc-ui-hierarchy)通过 AppShell 与共享工作区落实为“标题/结果/下一动作 → 相关依据与范围 → 原文/诊断”。本节数字是实施目标，不能引用为已验收样式。默认采用舒展密度，不通过缩字或隐藏关键状态塞进首屏；紧凑密度只由用户明确选择，视口变窄不自动切换密度。

| token / 布局 | 默认实施值与行为 |
| --- | --- |
| 文字 | 正文 16px，辅助信息至少 13px，页面标题 24px、区块标题 18px；正文行高约 1.6。紧凑密度正文仍至少 14px，任务名称、待答内容与重要警告不降为小字 |
| 留白与控件 | 间距级数 4/8/12/16/24/32px；默认卡内 24px、主组间 24–32px，主控件高度至少 40px。显式紧凑可使用 36px 控件，保留可辨的点击与焦点范围 |
| 表面与状态 | 主题变量区分窗口背景、阅读面、次级面、分隔线、选中/焦点及成功/警告/未知；正文对比目标至少 4.5:1，控件边界和焦点至少 3:1。状态使用一致 SVG 图标加短文本，不只靠色，不用 emoji 或无分母的完成环 |
| 常规 1280×830 / 1440×900 | 左导航约 200px，内容横向留白 24–32px、分区间距 24px。扣除导航/留白后可用主区宽度至少 960px 才允许列表+详情：列表 360–420px，详情至少 560px；宽度不足立即换单主区 |
| 紧凑 1024×768 / 728×627 | 1024 可收为有名称提示的 72px 导航栏，728 使用可打开的导航抽屉；主区依次显示列表或详情，保留明确“返回工作列表/首页”。不得维持三列窄面板或整页横向滚动 |
| 放大与长内容 | 150%/200% 放大按实际可用 CSS 宽度切布局，不抵消字体放大。长中文任务名自然换行，必要收起时提供全文入口，不裁掉区分工作所需信息；只有代码/原始数据块允许局部横向滚动 |

App 提供“显示 / 文字与界面大小”入口，100% 为默认，支持选择 150%、200% 及复位；这是本轮待实现的能力，沿受支持的 WebView/布局能力让倍率在 App 内实际生效并保存个人显示偏好。实现与验收记录实得 CSS 视口和布局，不能用截图后放大、系统放大镜或只在测试 DOM 注入样式代签该入口；调整大小不重新加载来源或触发工作动作。

首页四个语义区不渲染成等权重的文字框：当前工作、目的和下一步占主要阅读区；真实待答在有内容或覆盖缺口时形成清楚的次区，突出准确已知数量及少量具体请求；最近成果使用真实事件时间与短结果的纵向时间线；返回入口用工作名称、上次位置和草稿存在性组成轻列表。主区选中工作后直接进入同一详情；宽屏可保留旁列表，不能把详情挂在原四区和来源列表之后。关注数量存在未读范围时同时显示该范围未知，不把数量写成全局完整值。

WorkList 每页默认 20 项、最多 50 项；首页默认最多三项重点、三条成果事件、三个返回入口和两项真实待答预览，其余有带范围的“查看全部/继续读取”。一项默认只呈现名称/目的、当前结果、必要剩余条件和一个主要动作；工作名不按固定行高裁断。真正待答及高影响警告始终在所属工作摘要或关注区可见；条目超预算时保留准确已知数量、第一项实质内容和直达完整处理列表，不把警告全部藏进“更多”。这些是默认条目、字段与动作预算，不限制读取真实完整内容。

WorkDetailPane 的固定摘要顺序为工作名称/目的、当前可核实结果、主要剩余条件与下一动作；下方用“概览 / 依据 / 历史”二级页签。概览展示工作与显式目标/PR/会话关系，已知端点直接打开；依据显示可读条款及影响判断的版本差异；历史承载真实结果事件、个人变更与操作回执。关联编辑、决定同步、个人问题和 ToolTask 按动作打开；表单有清楚取消/返回，不与全部资料常驻堆叠。

SourceInspector 由具体事实的“查看依据”打开，在宽屏替换次级面板，在紧凑布局替换主区并带返回；不创建第三列。来源摘要先展示读到什么、覆盖/时效和具体缺口，完整路径、SHA、协议身份与 raw JSON 在其内按需展开，复制、定位原文和原工具打开保持可用。未安装/未连接仅在相关动作处显示可执行恢复步骤，不默认展开不可执行的完整配置表单。旧 App 中“最多100个 PR/章节”的重复主列表退为来源检查/检索入口，日常工作均用 WorkRef 进入共享详情；来源能力与原文检索保留。

规范阅读复用现有 MarkdownText 和文档索引：当前主题正文在主阅读区呈现，标题、段落、列表、引用和代码块有独立层级，正文最大行宽约 48em，短段之间有可辨留白。目录/配对可按布局保留为次区，完整元数据和原始 Markdown 按需展开，不以一整块等宽源文本替代正文。受限渲染、代码转义、禁用 raw HTML 与远程媒体自动加载仍沿 §8；排版不引入可执行内容或新的来源权威。

保存、失败和未知结果贴近触发动作，用可读状态区域通知；输入错误留在相关字段，成功定位到原工作结果。返回恢复选择、筛选、位置与焦点，页签/面板切换保留所属工作的输入；焦点不能跳到页面底部长列表。图标按钮有可读名称，键盘可进出来源检查/工具动作，关闭后回到触发控件；加载与重读保留已可读内容，空、部分、旧、错误和未接入分别使用 §6.1 状态。

中文文档采用 [SQLite FTS5 trigram](https://sqlite.org/fts5.html#the_trigram_tokenizer) 子串索引，标题、路径与已有条款 ID 优先；少于三个 Unicode 字符的“规范”“合入”等查询回退到选定范围内的参数化子串查询，限制结果、支持取消与去抖。unicode61 不被当作完整中文分词，trigram 也不被当成覆盖两字搜索。

IME composition 期间 Enter 只用于候选操作，不能误提交搜索/发送；保留已获用户确认的真实 IME 结果。新增视觉验收检查受影响的输入/焦点路径，不将历史 IME 再列为未完成；VoiceOver 按用户 2026-10-18 的选择移出产品与验收范围。长文档、列表、日志及来源跳转的视觉与交互变化仍须由真实 App 证明，浏览器 mock 不替代系统体验。

### 9.2 容量预算与可观测性

实现必须为来源分页、解析池、DB 请求、日志窗口、UI 在途批次、诊断与缓存设显式容量/时间界限；异常大输入产生可解释错误或截断范围，不无限占用。来源与存储溢出按 §7.2 降级，不静默丢失个人问题。

验证记录机器、系统、构建模式、数据规模、持续时间、冷启动/检索时延、滚动阻塞和整个 App 进程树资源；在首个切片中确定实际阈值并记录证据。当前没有包体、内存或帧率实测，不能宣称某框架已优于另一框架。容量优化先定位解析、I/O、DB、投递或渲染瓶颈，再决定局部调整。

| 场景 | 环境与刺激 | 可判定的预期响应 | 指标/验证入口与当前范围 |
| --- | --- | --- | --- |
| 日常任务理解 | PRD 代表性真实项目，至少三项工作及旧/缺失来源；macOS 正式 App | 已可用数据下五分钟内形成重点/成果/剩余条件判断，进入正确工作；处理中断、返回结果与恢复原位置 | 阈值来自 [整体验收](command-center.prd.md#cc-product-acceptance)，记录真实耗时/额外搜索、切换和输入；当前未验证 |
| 慢来源隔离 | 固定候选、假 gh/GitHub 延迟 10 秒或手动释放读取屏障，同时查询已缓存详情、保存关系 | 在慢读取解除前可返回缓存查询与已提交个人保存；同源重复/满额立即明确拒绝，旧源保留 | Core/host 受控屏障断言，记录队列高水位与操作顺序；不是原生绝对时延证明 |
| 迟到/竞争发布 | 刷新 A 后切到 B，A 晚到；刷新期间保存决定、修复关联或变更账号 | A 不覆盖 B，旧个人 revision 不覆盖新保存；来源与投影事务一致 | 临时 worktree/fake source 与 CAS 故障回放；未证明真实 API 完整度 |
| 大范围/部分读取 | 代表性仓库实际规模加超扫描/分页/显示预算样本 | 已读内容可用，数量/窗口/遗漏范围准确；不把最后 100 PR 当全仓完成度 | 记录文件数/字节、页数、工作数、输出上限及截断；不从单样本推导容量上限 |
| 存储与退出压力 | DB 队列满/事务失败、旧活跃/未知数据与工具任务并存 | 个人输入保留，旧记录不重放，独立 ToolHost 退出保护正常 | 核心与宿主回归；真实 App 退出验证 |

对实际 macOS 性能不预设未经测量的毫秒、内存或帧率通过线；实现 PR 固定硬件/系统/构建、冷暖样本窗口及分位数，先定位 I/O、DB、投递和渲染耗时，再由产品/实现负责人确定目标。上述有界配置和受控顺序断言可独立验证，不能因此报告整体性能已达标。

## 10. 兼容、迁移与回滚

<a id="DES-CC-012"></a>

### 10.1 渐进交付与连接兼容

现行范围是本地阅读、GitHub PR/CI、个人记忆与问题，以及原工具接续。Codex 自管连接器已退役，不是默认关闭的可选功能，也不预留远端 supervisor、协议升级或生产稳定化承诺。GitHub 可独立未接入，基础本地能力继续可用。

旧用户启用偏好不再生效；旧数据库/操作/请求/私有标记保持原值。退出和来源缓存重建不重放旧请求，不删除历史库。回退旧安装包可能重新提供其旧能力，因此本版本不声称能约束旧程序；使用者应在原外部工具核对仍在运行的工作。

### 10.2 数据迁移与版本回退

本增量采用兼容扩展：保留 `Snapshot` 和现有个人键的 shape，新增独立 `work_projection_v1` 可重建 cache 键，以及 `work_relations_v1`、`decision_followthrough_v1`、`work_navigation_v1`、`work_attention_v1` personal 记录。名称表示本地记录格式，不新增任务数据库或公共运行协议。RepositoryScope 仅保存显式来源引用/阅读偏好，旧 Project.id、runtime scope、私有库注册身份与现有操作表不迁移或合并；私有正文及线程不进入这些普通投影记录。

上述个人分组记录在文首实现基线已存在；工作来源当前生成临时 DTO，`work_projection_v1` 仍是可重建缓存的设计落点。本轮在需要缓存时使用 `work_presentation_v1`，不同时保存两份不同语义的工作事实；另加按原工作键保存的 `work_drafts_v1` personal 扩展，来源检查使用独立 DTO schemaVersion，不提升全库版本。决定证据增加可缺省的 `readBinding { schemaVersion, decisionRevision, digest, sourceVersions }`；新 UI 查看旧决定可取得当前内容派生版本后明确核实，旧证据缺 readBinding 则按 §7.4 保留历史与待核对，不回填用户确认。DTO/命令变更与 Rust 类型生成同步发布，旧读取入口继续支持，缺关键前置的新写入明确拒绝。

保持现有 SQLite 版本，旧 runtime 操作、请求与私有标记不改写。新增普通记录不启动旧 writer；本轮不作破坏性迁移。

| 数据情形 | 迁移/读取规则 | 回退或失败处理 |
| --- | --- | --- |
| 首次使用新记录 | 缺扩展记录时按旧决定/关联逐项读取，以旧 key、原 ID 与内容摘要做幂等导入；原确认时间、历史引用保留 | 没有证据的同步状态映射 unknown；不制造待答任务或已完成状态。导入事务失败保留旧记录，仍可阅读旧入口 |
| 旧关系与新关系并存 | 旧 URL/section IDs 作为原依据，定位结果独立核实；新 revision 只写扩展记录，源关系从当前源重建 | 不强制重写旧决定/关联。旧 App 若后来编辑 legacy 记录，新 App 比较导入摘要并标明冲突，不能静默覆盖新修复/移除/证据 |
| 旧运行草稿 | 原navigation与每工作记录保持原值，不迁移到决定/问题编辑 | 不认领、不显示为待发送任务、不删除、不重放 |
| 缓存缺失/损坏/未来版本 | 仅重建支持版本的派生缓存；未知 personal 版本明确拒绝编辑并保留原值 | 不以默认空值覆盖不可读个人记录；有影响的详情降级，其余来源/旧视图继续使用 |
| 回退至文首固定源码基线的 App | schema 2 保持兼容，旧 App 忽略本轮呈现/每工作草稿扩展，保留其已有四区、关系/同步、操作及私有属性合同 | 旧 App 独立写库不受新 Host 拒绝控制；再次升级比较实际决定/证据 version 与 digest，保留历史及冲突，不能沿用与当前决定不匹配的 verified。新稿不由旧单草稿反向覆盖，当前结论重新适用 §7.4 |
| 实际需要不兼容迁移 | 先核对旧/新安装版本与数据库版本，备份个人/操作记录，停止所有受影响 writer，在事务内迁移并验证 | 失败保留原库；旧版不能理解新 schema 时用匹配备份或修复版，不做破坏性降级。升级后新增记录/未决操作留副本供核对 |

投影/个人格式、生成 DTO 和同包 UI 按明确版本切换，新功能读取失败可退到来源入口；不双写两套有不同语义的完成状态。真实旧库、关闭/已启用偏好、未决操作、私有线程标记和晚到事件均须在实际迁移候选上验证。恢复备份不能撤销已发生的模型/签名效果；按原目标核对仍未知的副作用，禁止用陈旧快照重放。

旧运行草稿记录只保留其原格式和值，本轮不提供编辑、认领或发送界面；未知个人格式不以空值覆盖。决定/个人问题及本机工具输入沿各自容量和存储失败合同处理，不复制旧操作输入、私有线程或秘密。

<a id="cc-system-verification"></a>

## 11. 验证设计与可追溯性

实现与验证入口列在下表及 §11.0；入口存在不等于完整 AC 已通过。实现 PR 记录实际版本、环境和结果，最后一列保留尚未证明的范围。目标架构中的远端、生产分发与完整系统体验仍按其适用条件验收。

### 11.0 首个基础实现切片

实现落点为 [tools/oasis7-command-center](../../../tools/oasis7-command-center/README.md)，包含独立 Rust workspace、SQLite、Solid/TypeScript 和 Tauri 宿主；基础版本不提供 Codex 执行连接器。四视图读取本地来源、显式连接 GitHub、保存决定与多目标关联，不把未接入会话解释为空任务或允许控制。

本节描述历史基础切片及其覆盖；本轮 PRD 将完整基础首版收敛为日常工作闭环。已有四个视图和自动化不能替代专用首页、统一工作详情或三场景用户验收。

- Rust 自动化入口：`env -u RUSTC_WRAPPER cargo test --locked --manifest-path tools/oasis7-command-center/Cargo.toml -p oasis7-command-center-core`，覆盖中文短查询、FTS5、dirty 内容、缓存重建、阅读历史、已确认决定、项目隔离、未来 schema 拒绝、路径边界及 GitHub 故障保留。
- UI 入口：`npm --prefix tools/oasis7-command-center/ui test` 与 `run build`，覆盖状态、来源和中文组合输入的提交边界；Rust DTO 由 `export_types` example 生成，CI 核对漂移。
- 桌面与权限入口：[独立 macOS CI](../../../.github/workflows/command-center.yml) 和 [打包/验收步骤](../../../tools/oasis7-command-center/README.md#verification)。配置限定打包资源和主窗口的有限命令，不提供通用 shell/SQL/文件代理。
- 基础 GitHub adapter 复用已有 gh 登录，并以固定只读参数调用 gh；当前仅支持 github.com 的最近最多 100 个 PR。账号和仓库绑定到同一主机，完整度明确为部分窗口。此次没有新增长期 token 输入，故不安装另一份凭据 provider；原有凭据仍由 gh 存储。直接 HTTP 的条件请求、全量分页与节流并未实现，后续扩展不能把当前窗口当完整快照。

下表的完整 AC 仍包含尚未验证的系统体验、生产分发与可选执行能力；上述局部自动化覆盖不将整条验收标为通过。真实桌面结果和限制由实现 PR 记录。

### 11.0.1 可选实验执行实现

历史实验曾采用固定 Codex 0.137.0 stdio adapter，相关旧验证仅说明当时构建的协议与故障范围。当前产品已移除其生产模块、宿主命令、界面和启用配置，不沿用旧验证宣称现行能力。历史记录见 [日常闭环验证记录](daily-work-verification.manual.md)，保留原日期和限定结论。

### 11.0.2 显式规范后继读取

文档索引保留文档身份块显式声明的生命周期、后继目标与理由，并将其作为来源事实显示。声明不等于规范效力、实现同步或验收通过；没有声明、声明冲突、超过读取界限或目标未进入当前索引时保留未知，不用时间或标题相似度推导替代关系。

历史阅读版本保留当时的正文与元数据。界面另行标识当前文档声明的来源版本和提交基线，再按当前索引中的完整路径和明确章节身份定位后继；历史声明不能冒充当前声明，当前来源缺失也不自动采用历史声明作为现行规则。后继链接的路径与 fragment 均只解码一次百分号编码后定位；非法编码、非 UTF-8、含空白或控制字符的 fragment 保留诊断，不开放跳转。读取不修改正式规范、不访问外部链接或启动任务。此切片补齐显式后继的可读入口，不宣称所有规范冲突已经自动消解。

### 11.0.3 持续维护文档入口

[项目状态](README.md)、[架构索引](architecture/README.md)及[持续更新流程](command-center.manual.md)提供可直接阅读的维护载体；Skill 和工程导航连接到相同正文。文档索引可以读取纳入其 tracked 范围的 Markdown，仍受 §11.0 的覆盖限制。本次文档交付验证链接、条款映射、来源及派生 corpus 对象，不等于专用汇总 UI、自动过期判断、计划任务或实际部署采样已经实现。

### 11.0.4 日常工作闭环的实现差距

本轮收缩须证明原工具定位仍可用，个人问题、决定/关系、文档树、真实 PR/CI 不受影响；既有运行记录不删除不重放。原发任务预览、冻结模型输入、反向请求与退出协调不再属于待完成范围。普通个人草稿与决定版本绑定继续按各自现行合同验证。

视觉、文档树以及本机工具尚未证明部分沿对应需求和实际 PR 继续记录，不用本次删除替代它们的整体验收。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立义务与适用条件 | 验证方法与当前入口 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-6e70287f-a156-4544-997d-5c1752e5efde；[REQ-CC-001](command-center.prd.md#REQ-CC-001) | [DES-CC-001](#DES-CC-001) | App 独立启动，打开正确项目和工作树，恢复个人上下文；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=App 独立启动，打开正确项目和工作树，恢复个人上下文 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-2d1f3577-785e-40ac-8d25-ba0d414511f6；[REQ-CC-002](command-center.prd.md#REQ-CC-002) | [DES-CC-002](#DES-CC-002) | 来源、版本、覆盖与失败分别保留；局部可用；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=来源、版本、覆盖与失败分别保留；局部可用 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| [REQ-CC-003](command-center.prd.md#REQ-CC-003) | [DES-CC-003](#DES-CC-003) | 当前与长期目标关联到适用验收，进度有可核实分母；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=当前与长期目标关联到适用验收，进度有可核实分母 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；[REQ-CC-004](command-center.prd.md#REQ-CC-004) | [DES-CC-004](#DES-CC-004) | 规范效力、历史理由、个人决定与正式回写可区分；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=规范效力、历史理由、个人决定与正式回写可区分 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；[REQ-CC-004](command-center.prd.md#REQ-CC-004) | [cc-decision-attention-state](#cc-decision-attention-state) | 决定版本、规范回写、实现同步及个人关注分别保存，旧数据恢复不复活已处理项；产品适用性：N/A；reason=工程客户端合同不定义游戏玩家能力；scope=决定版本、规范回写、实现同步及个人关注分别保存，旧数据恢复不复活已处理项 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；[REQ-CC-005](command-center.prd.md#REQ-CC-005) | [DES-CC-005](#DES-CC-005) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；[REQ-CC-005](command-center.prd.md#REQ-CC-005) | [cc-presentation-projection](#cc-presentation-projection) | 同一来源事实与有效个人关联一次组合到共享工作 DTO；产品适用性：N/A；reason=工程客户端呈现合同不定义游戏玩家能力；scope=关系覆盖、同版呈现与有界来源检查 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；[REQ-CC-006](command-center.prd.md#REQ-CC-006) | [DES-CC-006](#DES-CC-006) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；[REQ-CC-006](command-center.prd.md#REQ-CC-006) | [cc-attention-projection](#cc-attention-projection) | 真正待回复、待核实及知悉变化分组，处理后返回原工作且重开可恢复；产品适用性：N/A；reason=工程客户端合同不定义游戏玩家能力；scope=真正待回复、待核实及知悉变化分组，处理后返回原工作且重开可恢复 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；[REQ-CC-007](command-center.prd.md#REQ-CC-007) | [DES-CC-007](#DES-CC-007) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；[REQ-CC-007](command-center.prd.md#REQ-CC-007) | [cc-start-intent](#cc-start-intent) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| [REQ-CC-008](command-center.prd.md#REQ-CC-008) | [DES-CC-008](#DES-CC-008) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| [REQ-CC-009](command-center.prd.md#REQ-CC-009) | [DES-CC-009](#DES-CC-009) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | [DES-CC-010](#DES-CC-010) | 中文短查询、IME、键盘和有界长列表可用；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=中文短查询、IME、键盘和有界长列表可用 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | [cc-visual-composition](#cc-visual-composition) | 舒展默认、主次层级、宽窄及放大布局由原生截图与操作核对；产品适用性：N/A；reason=工程客户端视觉合同不定义游戏玩家能力；scope=四尺寸、150%/200%及主区/来源返回 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| [REQ-CC-011](command-center.prd.md#REQ-CC-011) | [DES-CC-011](#DES-CC-011) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| [REQ-CC-012](command-center.prd.md#REQ-CC-012) | [DES-CC-012](#DES-CC-012) | 外部接续与旧数据保留；产品适用性：N/A；reason=工程客户端边界收缩；scope=原定位、个人问题与不重放 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-ce7b612a-3316-4c40-9009-c0d295ff0ed6；[REQ-CC-013](command-center.prd.md#REQ-CC-013) | [DES-CC-013](#DES-CC-013) | 首页聚合重点、成果、真实请求及返回工作，三场景可用；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=首页聚合重点、成果、真实请求及返回工作，三场景可用 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-fdf061f8-ebdf-46e3-91a5-a2311e2bcb93；[REQ-CC-014](command-center.prd.md#REQ-CC-014) | [DES-CC-014](#DES-CC-014) | 明确关联复用、失效修复、统一详情及已有现场接续；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=明确关联复用、失效修复、统一详情及已有现场接续 | [核心来源与兼容测试](../../../tools/oasis7-command-center/core/tests/foundation.rs)、[工作投影测试](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[原生验证方法](command-center.prd.md#cc-product-acceptance)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；[REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-015](#DES-CC-015) | 本机工具作为独立辅助入口接续原工作；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=本机工具作为独立辅助入口接续原工作 | [本机工具界面测试](../../../tools/oasis7-command-center/ui/src/LocalTools.test.tsx)、[工具退出测试](../../../tools/oasis7-command-center/src-tauri/src/tool_exit.rs)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |
| Trace refs: O7-REQ-729d655d-d8a8-4c44-9ea7-32578dc10f3f；[REQ-CC-016](command-center.prd.md#REQ-CC-016) | [DES-CC-016](#DES-CC-016) | 文档全局树、配对与覆盖透明；产品适用性：N/A；reason=开发指挥客户端的工作、来源及会话管理，不定义游戏玩家能力；scope=文档全局树、配对与覆盖透明 | [文档树界面测试](../../../tools/oasis7-command-center/ui/src/AppDocumentReading.test.tsx)、[原生验证方法](command-center.prd.md#AC-CC-016)；按适用场景验证 | 当前实现PR的source/build、命令、实际结果 | 未运行的原生场景不计通过，旧证据不外推 |

### 11.2 实施时的验证层与关键反例

确定性回归覆盖旧库兼容、无 Codex 命令/进程、无历史请求投影、个人问题与外部定位；执行核心测试、生成 DTO 核对、前端测试/build 与原生宿主检查。真实本地 App 使用实际仓库和 GitHub 来源检查首页、工作详情、个人问题与菜单栏关闭/退出/重开；演示入口不能代替实际效果。

旧验收记录不改为本次通过；实现 PR 保留实际命令、source HEAD、结果与未覆盖范围。不需运行模型来验证已删除的模型控制能力。

<a id="cc-implementation-dependencies"></a>

### 11.3 实施依赖与最小可用结果

先修订本产品/系统边界，再删除核心与宿主生产控制路径，更新 UI、DTO、测试和使用说明。最小完整结果必须同时没有可操作的旧命令和常驻断连接口，保留原工具接续、个人问题和真实来源读取；只隐藏界面不足以完成收缩。

## 12. 决策、长期风险与未决问题

### 12.1 技术路线比较与决定

以下是 2026-10-10 一手资料基础上的工程判断，不是实测排名。选择主要服务中文文档、diff、日志、多列表和可持续桌面分发。

| 路线 | 已有能力与成本 | 本次决定 |
| --- | --- | --- |
| Tauri 2 + Rust + Solid/TS | Rust 核心、系统 WebView、桌面权限和发布路径；须管理 Rust/TS 契约与 WebKit 差异 | 主路线；匹配文本工作负载和既有经验 |
| [GPUI + GPUI Kit](https://gpui-kit.com/docs/) | 原生 Rust UI，已有表格/列表、Markdown、代码编辑、Dock、AccessKit 等；[GPUI 上游](https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md)仍需控制升级，安装更新仍需应用装配 | UI 必须 Rust 且禁止 WebView 时的主要备选 |
| [Dioxus Desktop](https://dioxuslabs.com/learn/0.7/guides/platforms/desktop/) | Rust UI 与业务，桌面仍为系统 WebView；需要评估成熟 JS 控件互操作 | 接受 WebView 且要求 Rust UI 时的备选 |
| [egui / eframe](https://github.com/emilk/egui) | 立即模式 UI、输入、虚拟化与辅助技术入口；长文档和复杂工作台需要装配 | 可用于精简诊断工具，本次不作为主界面 |
| [iced](https://github.com/iced-rs/iced/releases) | 已有 IME、表格/布局、Markdown 与测试能力；复杂文本和桌面集成仍需切片验证 | 可行，但本次组合成本不占优 |
| [Slint](https://docs.slint.dev/latest/docs/slint/reference/elements/styledtext/) | Rust 业务加声明式 UI；文档呈现范围和[分发许可](https://github.com/slint-ui/slint/blob/master/LICENSES/LicenseRef-Slint-Royalty-free-2.0.md)需与实际用途匹配 | 当前长文档工作负载带来额外装配 |
| [Electron + Rust](https://www.electronjs.org/docs/latest/tutorial/process-model) | 可固定 Chromium，有桌面 Web 生态；新增 Node 主进程及 Rust 通信边界 | 强依赖 Electron 控件或固定 Chromium 时再评估 |

原生 Rust 路线已有有用工作台组件，不能用“没有 Markdown/表格/可访问性”排除。GPUI Kit 若进入实施须固定成套版本并核对[打包](https://gpui-kit.com/docs/packaging/)与[更新](https://gpui-kit.com/docs/auto-update/)职责。选择 Tauri 是当前范围的组合成本判断；不同时开发第二套 UI。

Rust 核心 + Solid满足本次“Rust 优先”偏好；UI 全 Rust 尚不是硬要求。React 仅在关键组件或维护者实际经验带来明确收益时替换 Solid，领域与来源不随之更换。默认 ts-rs 生成 DTO，只有类型工具的发布稳定性和维护收益已明确时才重评完整桥接生成方案。

<a id="CC-CODEX-PROMOTION"></a>

### 12.2 重新决策条件与未决事项

旧 Codex 集成生产升级/远端托管路线已退役。只有用户另行明确需要由本 App 执行模型任务时，才重新提出产品范围、所有权、安全和验证设计；本次不保留备用连接器或 supervisor。

Apple 签名/管理员集成仍按既有暂停范围处理；公开分发、证书、更新和跨平台根据实际需求单独验证。

## 13. 本机工具集成增量

<a id="DES-CC-015"></a>

Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；承接 [REQ-CC-015](command-center.prd.md#REQ-CC-015) 的适用义务。

本机资料、同包 CLI、已有 signer 的普通调用、后台操作与私有运行线程合同见[本机工具集成](local-tools.design.md)。资料由原 context 文件与历史主责，App 的内置能力和已安装 CLI 分别观察。Apple 签名及管理员助手已按用户要求暂停，未开放特权安装/管理；完整首装签名闭环尚未验收。

辅助工具入口复用 §5.7 的工作引用和返回位置，任务入口、就地反馈、独立导出/恢复输入及可信签名请求沿 [DES-CC-TOOLS-005](local-tools.design.md#DES-CC-TOOLS-005)。工具结果保留原 operation/job 与来源范围，返回工作只更新真实有关结果；工具安装、资料保存或签名准备不使工作/PR/目标自动完成。私有资料只在受控工具界面展示，旧派生线程不进入普通投影，不进入普通首页或共享工作投影。

<a id="DES-CC-016"></a>

## 文档全局阅读增量

Trace refs: O7-REQ-729d655d-d8a8-4c44-9ea7-32578dc10f3f；承接 [REQ-CC-016](command-center.prd.md#REQ-CC-016) 的适用义务。

[REQ-CC-016](command-center.prd.md#REQ-CC-016) 的产品/系统设计树与目录查漏由[专题系统设计](document-tree.design.md)承接，默认阅读与诊断分层由 [DES-CC-TREE-004](document-tree.design.md#DES-CC-TREE-004)承接。文首固定源码已有文件清单、目录、显式产品/专业树、配对详情及阅读恢复；[覆盖统计](../../../tools/oasis7-command-center/core/src/index_coverage.rs)提供候选、读取、遗漏、原因计数和有界路径样本，仍受当前读取基线与扫描预算约束。

工作详情沿同一源版本打开具体规则，文档入口返回原阅读位置或原工作，不把文档列表作为处理问题的替代路径。全局树的独立成功快照、明确提交/untracked 基线等未闭合合同及实际能力见专题设计 §11；显式树存在不证明全仓语义配对、默认阅读体验或真实桌面整体验收。

默认正文复用现有 MarkdownText 与主题/索引合同，按 [视觉与阅读布局](#cc-visual-composition) 的字号、行宽、标题/段落/列表/代码层级放入主阅读区；目录、来源、历史和原始 Markdown 按需进入。§8 的安全转义、禁用 raw HTML 与远程媒体自动加载保持，排版不另建文档树或改变规范效力。


### 当前工作投影实现边界

文首基线的 `core/src/work_projection.rs` 已以原 PR/公开会话身份生成临时 DTO，保留 instance/turn、逐字段证据和持久私有排除；`WorkOverview.tsx` 已有四区、统一详情入口及关系操作，旧 personal associations 与新 relations 的呈现组合仍有 §3 所列缺口。新的 typed 呈现与视觉工作区沿 §4.4、§9.1，不另建任务主表。

扫描/Git/REST 锁外、有界来源与短交互槽、范围/代次/个人 revision 校验、原生重开恢复、外部工具引用及同仓显式工作树读取已有对应实现和 §11.0.4 历史证据。同源重复/满额当前明确拒绝，不以未实现的后台后继队列解释恢复。每工作草稿、完整预览绑定、决定阅读版本 CAS 和本轮视觉布局仍待实现验证；旧记录保留与原生 IME 结果不因此被抹去。测试入口见 §11.1，文档补齐不代签新 UI 的原生结果。

外部 Codex 导航只接受明确保存的 `codex://threads/<UUID>`，严格拒绝附加路径、query、fragment 或非法 UUID；普通文本定位仅复制。来源内容不取得本机命令权限。
