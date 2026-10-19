# 指挥客户端系统设计

- 文档 ID：`DESIGN-ENGINEERING-COMMAND-CENTER`
- 状态：`draft`；目标架构，局部实现与验证范围见 §11，完整产品验收未完成
- owner_role：`runtime_engineer`；产品归 `producer_system_designer`，外部接续归 `agent_engineer`
- module：`engineering`
- 上游：[指挥客户端产品设计](command-center.prd.md)、[PRD-ENGINEERING-034](../prd.md#PRD-ENGINEERING-034)
- 当前实现审读基线：`main@8d7ccdd979c5d4022cd465cce600cfa25bc08862`；历史构建与交互结果独立保存在[验证记录](daily-work-verification.manual.md)。
- last_reviewed_at：2026-10-19
- 初始设计与评审来源：[PR #4518](https://github.com/eng-cc/oasis7/pull/4518)

本文把客户端形态、Rust 技术调研和架构方案收敛为一份系统设计。产品行为和验收由配对 PRD 主责；本文定义如何承接，不把目标状态写成已交付事实。通用授权、开发、验证和交付仍由 [workflow/source-of-truth](../workflow/source-of-truth.md) 主责，客户端内部状态不增加上游任务准入或审批门禁。

## 1. 问题、目标与非目标

指挥中心将本地文档/Git、GitHub PR、个人问题、决定与关系放在同一工作上下文。Tauri桌面App负责来源阅读、个人保存及准确外部定位，让用户看清当前结果、剩余条件和下一步。

CI检查在原PR的GitHub页面查看，开发执行由原Codex工具负责；App提供明确外部入口。本机工具独立遵守其授权、私有资料和安全退出合同。工程通用CI、评审与合入要求继续由开发流程主责。

## 2. 上游约束与相关角色

产品负责人维护用户承诺及范围；系统实现维护领域、存储和生命周期；接入实现维护 GitHub 的版本与能力及外部定位边界；验证工作覆盖对应条款的真实环境与故障情形。角色描述用于定位专业责任，不新建强制串行交接。

已有仓库与外部来源保持事实所有权。工程工具沿 engineering 专业域组织，产品四域、现行规范主责、Issue 可选和实际 PR/CI 交付方式不变。

### 2.1 需求承接与分配表

| 上游条款 | 独立义务与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-6e70287f-a156-4544-997d-5c1752e5efde；[REQ-CC-001](command-center.prd.md#REQ-CC-001) | 独立App、真实项目身份与阅读恢复；产品适用性：N/A；reason=工程客户端合同；scope=独立App、真实项目身份与阅读恢复 | [DES-CC-001](#DES-CC-001) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-2d1f3577-785e-40ac-8d25-ba0d414511f6；[REQ-CC-002](command-center.prd.md#REQ-CC-002) | 来源版本、覆盖、失败与局部可用；产品适用性：N/A；reason=工程客户端合同；scope=来源版本、覆盖、失败与局部可用 | [DES-CC-002](#DES-CC-002) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| [REQ-CC-003](command-center.prd.md#REQ-CC-003) | 目标、适用验收与有依据的进度；产品适用性：N/A；reason=工程客户端合同；scope=目标、适用验收与有依据的进度 | [DES-CC-003](#DES-CC-003) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；[REQ-CC-004](command-center.prd.md#REQ-CC-004) | 规则后继、个人决定与替代历史；产品适用性：N/A；reason=工程客户端合同；scope=规则后继、个人决定与替代历史 | [DES-CC-004](#DES-CC-004) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；[REQ-CC-005](command-center.prd.md#REQ-CC-005) | 同一原工作身份的列表与详情；产品适用性：N/A；reason=工程客户端合同；scope=同一原工作身份的列表与详情 | [DES-CC-005](#DES-CC-005) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；[REQ-CC-006](command-center.prd.md#REQ-CC-006) | 个人问题保存、回答、关闭及返回；产品适用性：N/A；reason=工程客户端合同；scope=个人问题保存、回答、关闭及返回 | [DES-CC-006](#DES-CC-006) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；[REQ-CC-007](command-center.prd.md#REQ-CC-007) | 准确外部定位与受限系统导航；产品适用性：N/A；reason=工程客户端合同；scope=准确外部定位与受限系统导航 | [DES-CC-007](#DES-CC-007) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| [REQ-CC-008](command-center.prd.md#REQ-CC-008) | 旧用户记录保留、隐私隔离与不重放；产品适用性：N/A；reason=工程客户端合同；scope=旧用户记录保留、隐私隔离与不重放 | [DES-CC-008](#DES-CC-008) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| [REQ-CC-009](command-center.prd.md#REQ-CC-009) | 菜单栏恢复及独立工具安全退出；产品适用性：N/A；reason=工程客户端合同；scope=菜单栏恢复及独立工具安全退出 | [DES-CC-009](#DES-CC-009) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | 中文、信息层级、缩放与键盘；产品适用性：N/A；reason=工程客户端合同；scope=中文、信息层级、缩放与键盘 | [DES-CC-010](#DES-CC-010) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| [REQ-CC-011](command-center.prd.md#REQ-CC-011) | 个人数据、内容安全与凭据边界；产品适用性：N/A；reason=工程客户端合同；scope=个人数据、内容安全与凭据边界 | [DES-CC-011](#DES-CC-011) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| [REQ-CC-012](command-center.prd.md#REQ-CC-012) | 本地优先、独立来源与局部可用；产品适用性：N/A；reason=工程客户端合同；scope=本地优先、独立来源与局部可用 | [DES-CC-012](#DES-CC-012) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-ce7b612a-3316-4c40-9009-c0d295ff0ed6；[REQ-CC-013](command-center.prd.md#REQ-CC-013) | 首页重点、成果、个人问题与返回；产品适用性：N/A；reason=工程客户端合同；scope=首页重点、成果、个人问题与返回 | [DES-CC-013](#DES-CC-013) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-fdf061f8-ebdf-46e3-91a5-a2311e2bcb93；[REQ-CC-014](command-center.prd.md#REQ-CC-014) | 关系复用、失效修复与原工作接续；产品适用性：N/A；reason=工程客户端合同；scope=关系复用、失效修复与原工作接续 | [DES-CC-014](#DES-CC-014) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；[REQ-CC-015](command-center.prd.md#REQ-CC-015) | 独立本机工具、私有资料与原工作往返；产品适用性：N/A；reason=工程客户端合同；scope=独立本机工具、私有资料与原工作往返 | [DES-CC-015](#DES-CC-015) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |
| Trace refs: O7-REQ-729d655d-d8a8-4c44-9ea7-32578dc10f3f；[REQ-CC-016](command-center.prd.md#REQ-CC-016) | 文档树、明确配对、覆盖与阅读恢复；产品适用性：N/A；reason=工程客户端合同；scope=文档树、明确配对、覆盖与阅读恢复 | [DES-CC-016](#DES-CC-016) | 客户端实现及对应来源/工具 | 源事实与个人记录各守原义 |

## 3. 当前基线、目标状态与差距

<a id="cc-current-implementation"></a>

当前源码已有独立App、受限文档/Git读取、中文检索、GitHub PR分页/已知对象补读、个人关系/问题/决定记录、原工具定位、阅读恢复及舒展工作区。主机命令、DTO与UI在同一包内发布；[命令注册](../../../tools/oasis7-command-center/src-tauri/src/main.rs)、[工作投影](../../../tools/oasis7-command-center/core/src/work_projection.rs)和[个人记录](../../../tools/oasis7-command-center/core/src/daily_work.rs)是本节核对入口。

| 对象 | 当前边界与剩余范围 |
| --- | --- |
| 共享工作投影 | 由已读PR与明确外部工作引用组成，逐字段保留证据；当前goal字段仍沿旧associations生成，尚未完整证明所有消费者统一组合新relations |
| 个人记录 | 按项目/环境保存个人revision并拒绝旧版本写入；个人决定保存内容、理由、范围、正式条款定位及确认/替代历史；旧同步证据只保留原存储，不进入当前DTO、查询或写入 |
| 阅读恢复 | 工作选择、列表/详情位置、树偏好和项目上下文已有记录；未保存编辑的重开行为按实际入口说明，不承诺多工作模型输入编辑 |
| 文档树 | 文件清单、目录、显式产品/专业树和配对可读；固定提交/untracked基线及独立树成功快照等仍沿[专题设计](document-tree.design.md) §11 |
| 桌面与工具 | 真实系统交互、规模、签名与分发按适用PR验证；工具首装和暂停范围见[本机工具设计](local-tools.design.md)，历史测试不代替当前整体验收 |

## 4. 边界与结构

<a id="DES-CC-001"></a>

### 4.1 App、核心与外部来源

Trace refs: O7-REQ-6e70287f-a156-4544-997d-5c1752e5efde；承接[REQ-CC-001](command-center.prd.md#REQ-CC-001)。

Solid/TypeScript负责四个主视图与辅助工具；Tauri宿主负责目录选择、菜单栏、有限IPC及系统导航；Rust核心负责有界来源读取、个人记录与SQLite；领域模型独立于Tauri和SQL行。

Project以实际仓库、规范化工作树、git_common_dir与environment识别，分支仅为属性。RepositoryScope只组合用户明确选择的同仓工作树阅读引用，不因remote相同合并不同clone，也不改写原Project.id或私有库注册身份。切换阅读范围保留原工作与定位；路径失效时重新核对身份再恢复。

### 4.2 代码落点与职责

以下路径承载客户端实现。新增模块名是本设计的实施落点，不表示文件已存在；保持一个独立核心 crate，不抽出通用任务服务。

| 路径 | 职责 |
| --- | --- |
| `tools/oasis7-command-center/Cargo.toml`、`Cargo.lock` | 小型独立 workspace，成员 `core` 与 `src-tauri` |
| `tools/oasis7-command-center/core/src/` | 文档/Git/GitHub适配、个人记录、工作投影与存储 |
| `tools/oasis7-command-center/src-tauri/` | 薄宿主、配置、production capabilities、打包 |
| `tools/oasis7-command-center/ui/` | 独立前端包、生成 DTO、有限 bridge、界面测试和 lockfile |
| `tools/oasis7-command-center/tests/fixtures/` | 文档、GitHub 和兼容回归样本 |

| 现有落点 → 目标职责 | 具体改动边界 |
| --- | --- |
| `core/src/work_projection.rs`、`daily_work.rs` / `daily_work_types.rs` → 呈现投影 | 保留来源 WorkRef、逐字段证据与个人记录；增加一次组合的首页/列表/详情及独立来源检查 DTO，见 §4.4 |
| `core/src/index.rs`、`github.rs` | 继续负责有界来源读取；补来源事件/观测/拉取时间、窗口与已知对象补读，不在 adapter 内决定目标完成 |
| `core/src/lib.rs` → 应用协调与存储接口 | 分离读取候选和提交，处理关系修复、个人决定与view revision；个人数据写入使用 record revision/CAS |
| `src-tauri/src/commands.rs` | 复用锁外 I/O 和有界发布；查询已读快照，保存时核对当前已实现的个人record revision；个人决定按实际保存版本保留历史，不因呈现查询重新扫描 |
| 外部会话定位与宿主导航 | 校验 `codex://threads/<UUID>` 后打开；其他定位仅复制，不提供运行控制 |
| `ui/src/App.tsx`、`WorkOverview.tsx` → 共享工作区组件 | AppShell 管导航与主内容；HomeOverview/WorkList/WorkDetailPane 消费同一投影；SourceInspector、个人决定、ToolTask 按动作进入，不在 JSX 再算一套完成/关注状态 |
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
| 凭据与诊断 | gh登录；有界诊断 | 凭据沿原工具管理，诊断脱敏且不自动上传 |
| 验证与分发 | Rust tests、Vitest、WebdriverIO Tauri service；Tauri bundler / updater | 分层验证、macOS 安装包；更新按需加入 |

[Solid](https://docs.solidjs.com/concepts/intro-to-reactivity)、[Kobalte](https://kobalte.dev/docs/core/overview/introduction)、[TanStack Table](https://tanstack.com/table/latest/docs/framework) / [Virtual](https://tanstack.com/virtual/latest/docs/framework) 支持当前交互组合；选 Solid 的理由是适配工作负载和已有工程经验，未声称比 React 更快。[ts-rs](https://github.com/Aleph-Alpha/ts-rs) 只承担类型生成，不生成完整命令客户端或运行时校验。[rusqlite](https://github.com/rusqlite/rusqlite) bundled 统一 SQLite 构建，数据库线程、队列与事务公平性由应用实现。

具体 patch、MSRV、Cargo feature、插件兼容性和许可证组合在实现锁定时核对，不能把 `latest` 或 Viewer 声明范围当作已验证锁定组合。当前不引入 ORM/数据库池、向量库、SSR、生产 Vite 服务或通用插件框架。

<a id="cc-presentation-projection"></a>

### 4.4 共享呈现投影与查询边界

Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；承接[REQ-CC-005](command-center.prd.md#REQ-CC-005)。

`query_work_projection`从已发布来源生成`WorkProjection`，保留project/environment、source coverage、逐字段SourceFact、viewRevision与personalRevision。`WorkItem`是原工作身份的派生阅读对象，不是任务主表；页面通过原WorkRef定位同一详情。

目标呈现须组合当前有效的个人关系、移除标记与明确来源关系；同一工作在首页、列表、详情使用一致结果。当前goals组合缺口见§3，不用界面自行拼接伪称全消费者已一致。保存个人记录后只重算已取得来源，不重新扫描Git或GitHub。

原文与来源详情复用当前索引/缓存，保留原版本、范围与返回位置。缺少原版本时明确说明，并由用户沿已有来源刷新入口补读；来源详情失败只影响该面板。

## 5. 关键运行流程

<a id="DES-CC-004"></a>

### 5.1 打开项目、索引规范与记录决定

Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；承接 [REQ-CC-004](command-center.prd.md#REQ-CC-004) 的适用义务。

打开项目后，按 [项目地图](../../../skills/oasis7-command-center/references/project-map.md) 找到当前主责入口与显式引用；目标读取 core PRD 当前阶段章节，规范效力读取主责身份、范围和替代关系，不按 mtime 排序。已提交基线和 dirty 工作树分别索引与展示，切分支或修订变化使相关投影失效。

[pulldown-cmark](https://docs.rs/pulldown-cmark/latest/pulldown_cmark/) 产生可定位的标题、段落、表格、代码与链接块；解析语法不自动决定条款效力。已有条款 ID 优先，标题定位须处理重复；行号只辅助跳转。保存原版本引用与可取得内容，历史工作可回到当时条款并找到明确后继。

按真实阅读版本计算“自上次查看变化”；没有历史内容或明确阅读记录时说明缺口。个人决定保存内容、理由、范围和正式文档关联，草稿/确认/替代历史沿[§7.4](#cc-decision-attention-state)。确认时间不因后来编辑、查看或关联变化而重写；来源原文的修改在原工具完成。

[notify](https://docs.rs/notify/latest/notify/) 事件只提示失效，合并去抖后重读。初次载入、唤醒、重新聚焦、手动刷新和检测到遗漏时校准；必要时受限轮询。不索引 `target`、`node_modules` 或 Git objects，不能把全目录递归监听当作一致性保证。

### 5.2 Git、GitHub 与刷新发布

Git 采用 [status](https://git-scm.com/docs/git-status) 的 `--porcelain=v2 -z --branch`、[worktree](https://git-scm.com/docs/git-worktree) 的 `list --porcelain -z` 等机器输出。Rust 用参数数组、明确 cwd、超时和输出上限调用，处理空格/换行路径与 detached HEAD。读取不隐含 fetch、切分支、reset、提交或清理；本地远端引用只是最近所知状态。

GitHub 经固定只读 `gh api --method GET` 参数调用 REST，不使用隐式 GraphQL。选中工作树的 origin 经 REST 仓库身份核实；读取 PR 列表、显式后续页与已知 PR，保留 host/repository/PR、HEAD、状态及原 PR URL。列表截断、404 和未读取分别保留，不能由缺席推断未合入或删除。限额保留最近成功事实，不自动重试。



GitHub来源诊断仅显示连接、账号、当前读取范围、分页/已知对象补读入口与错误。所有读取的PR只在统一工作列表/详情呈现，来源区不生成重复PR行或独立选择状态。补读成功更新同一来源快照与工作列表，失败保留最后成功事实。

初始刷新预算：每个 Git/gh 子进程或 HTTP 请求最多 10 秒，单来源刷新总期限 30 秒；列表每页最多 100 项、一次最多 10 页，已知对象补读一次最多 20 个且计入同一总期限。当前 gh 默认读取首 100 个 PR，显式后续第 2–10 页和已知 PR 按身份补读已实现；这些有界入口不代表已读取全量仓库事实。到达页数/时间预算返回已读范围和剩余 cursor/原因；用户明确“继续读取”开启有界新尝试，不能在后台无限追页。限额按来源返回的 retry-after 显示下次可重读时间，不在截止后持续等待或忙重试；分页期间来源变化保留各页实际版本/时间，不宣称原子快照。

一次刷新按“读取候选 → 核对版本/范围 → 事务应用 → 发布 view revision”处理。慢 generation 不能覆盖新快照。分页记录时间范围与完整度；即使页已读完，也不宣称各字段属于同一瞬间。403、404、限额、超时、部分页和成功空结果分别处理，失败保留最近成功数据。

<a id="DES-CC-006"></a>

### 5.3 个人问题与处理结果

<a id="cc-attention-projection"></a>

个人问题保留原ID、关联WorkRef、原因、状态、时间与record revision。新增、回答或关闭只写个人记录；决定在规范与决策独立保存，不参与个人问题数量。无问题时用轻量空状态，有内容时才在首页形成关注入口。来源变化与个人陈述分别保留，处理后回到原工作。

### 5.4 关闭、完整退出与恢复

<a id="DES-CC-009"></a>

关窗隐藏主窗口并保留菜单栏恢复入口；完整退出保存个人记录并调用独立ToolHost退出核对。工具任务仍运行或效果未知时沿[本机工具生命周期](local-tools.design.md#DES-CC-TOOLS-003)显示真实选择与结果。外部Codex由原工具管理。

重开恢复项目、已保存阅读位置与个人记录，并显示实际来源覆盖。旧数据的保留与使用规则集中见§10。

### 5.5 持续状态维护与架构阅读

按[持续更新流程](command-center.manual.md)维护两个派生仓库入口：`command-center/README.md` 承载长期/短期目标、当前进展、近期变化与人工关注；`command-center/architecture/README.md` 承载关键设计与部署索引。两者通过原有文档适配器读取并下钻源条款，不建立第二套任务表、产品权威或部署库存。

来源事实改变后，负责当前工作或回顾的执行者核对影响，在已有授权内维护主责和摘要；普通客户端刷新仅重新读取，不能自行写入仓库、创建 PR、SSH 采样或启动模型。合入后状态在下一次刷新协调，不为每次合入递归生成状态 PR。目标、已合入代码、实际测试、部署、目标验收使用各自来源。

摘要声明的文档基线、来源观测窗口与实际环境核验分别保留。文件索引的 revision/读取时间只说明读取了哪个摘要版本；没有读取其引用的 PR/环境，不能刷新其内部事实时间。原始内容中缺少时间、覆盖或来源时展示缺口，不推测。当前四区与摘要入口已有实现；其目标与未证明范围分别由§3、§5.6和§9.1承接。

架构索引从事项进入原有主责条款，分别展示设计边界、已有实现与实际部署证据。显式后继沿当前已读文档的完整路径与章节定位，缺失、歧义或不安全目标保留具体诊断；索引条目不能仅凭文档 active 或引用关系成为权限、写入权、部署就绪或完成证明。历史清单保留原采样日期，未知环境状态不补造实时拓扑。

<a id="DES-CC-013"></a>

### 5.6 首页判断投影

Trace refs: O7-REQ-ce7b612a-3316-4c40-9009-c0d295ff0ed6；承接 [REQ-CC-013](command-center.prd.md#REQ-CC-013) 的适用义务。

application 在既有来源快照之上生成 `HomeProjection`，四个区域及默认交互由 [REQ-CC-013](command-center.prd.md#REQ-CC-013) 与[首页布局](command-center.prd.md#cc-home-layout)主责。目标定位沿 core 当前阶段及显式主责引用；状态摘要沿现有稳定章节读取，复用 Markdown 块和链接，而非把文件全文直接放到首页。摘要无法结构化提取时提供该区正文的阅读入口及覆盖说明，保留其他已解析区域；只完成该降级不能宣称首页整体验收通过。

投影中的每个结论分别保留原文定位、源 revision、事实性质、摘要内部核验时间和读取时间。已核实 PR 更新其对应字段，不覆盖主责目标；与摘要冲突时保留双方版本及冲突字段，不用一个“最新时间”覆写整条记录。摘要自己的归纳不升级为运行事实。

重点排序输入只有主责显式阶段顺序、用户已保存置顶及组内来源顺序；缺少优先级时保留并列。下一步引用现有主责或个人选择，分析建议单独带性质且不改变源关系。近期结果按明确 PR/交付身份聚合，保留事件时间和比较基线。待处理投影复用原请求身份和 §5.3 的已处理结果；人工关注、事实缺口和普通提醒分组，观察不全不生成虚构请求。

默认“近七日”为用户时区的今天及前六个自然日，显示具体日期、时区和“今天截至本次观察”。查询固定 timezone、asOf 与换算后的 UTC 半开区间，夏令时按日历边界处理，不能直接减 7×24 小时；用户改变窗口或时区仅重算投影。成果按源 eventAt 倒序，相同时以稳定事件身份排序；按原 WorkRef/事件身份归并重复观察，同一 PR 的提交/检查在其交付结果下展开，不叠加为多个已完成目标。不同真实事件保留各自时间，未知 eventAt 单列“本次观察到，事件时间未知”，不用 fetchedAt/lastReadAt 补造；“自上次查看”只有明确同范围阅读基线时才单独可用。

阅读基线保存实际展示的 WorkRef/事件集合、窗口/cursor、源/view revision 与用户阅读时间；观察到来源和用户已读分别记录。首页只展示三项时不能用一个 `new Date()` 把未展示的分页/截断窗口全标已读；进入其他页后只提交该已展示范围的阅读记录。来源覆盖改变而无法比较时说明缺口，仍可看固定日期成果，不生成完整“自上次查看变化”的保证。

| 分区 | 投影输入与输出 | 缺口/动作边界 |
| --- | --- | --- |
| 当前重点与下一步 | 目标原条款、已核实成果、剩余条件、显式下一动作、关联 WorkRef；默认最多三项，其余可展开 | 无明确排序保留并列；不从 PR 数量推算优先级，不让用户先填写关系才显示可读取目标 |
| 最近成果变化 | 按用户选择的比较窗口、事件身份与适用版本聚合真实结果，链接同一工作 | 同一 PR 的提交/检查/合入不重复算成果；没有事件时间则不伪入窗口，改列“本次新观察到”并说明时间未知 |
| 需要我处理 | §7.4 的仍需用户回答事项与数量，事实缺口和知悉信息分组 | 没有个人问题项可以为空；来源不可用显示该范围未知，不能显示“全部处理完” |
| 返回工作 | 已确认工作关系、用户近期进入位置和未发送草稿存在性 | 不包含私有正文、秘密或私有线程；没有历史时给可读工作入口，不创建空任务 |

`HomeProjection`、`WorkDetailProjection` 和 `AttentionProjection` 从同一已提交来源集合及个人记录 revision 生成，并发布共同 `viewRevision`。各区返回自己的可用/部分/旧结果和缺口，来源集合并非原子远端时刻；一处失败不清空其他区。投影缓存键包括 schema、RepositoryScope、实际阅读基线、来源 revision 集合、个人记录 revision 和比较窗口，缓存缺失可重算。一次保存/处理成功使相关投影失效；旧计算结果提交前须核对全部输入 revision，不能重新引入已处理问题。

项目选择和连接配置只在首次打开或用户展开时占据主要空间；身份、SHA 和诊断按需展开，当前判断受影响的具体缺口直接显示。查询先返回已验证身份下的最后成功投影及分区状态，再异步校准来源；新恢复路径未完成身份校验时只提供带“待核对”标记的安全阅读，不保存到未核实项目。后台全量扫描不作为每次 `query_view` 的前置。

<a id="DES-CC-014"></a>

### 5.7 统一工作详情、关联与原工具接续

Trace refs: O7-REQ-fdf061f8-ebdf-46e3-91a5-a2311e2bcb93；承接 [REQ-CC-014](command-center.prd.md#REQ-CC-014) 的适用义务。

application 以 §6.2 的 WorkItem 原来源身份构造[统一工作详情](command-center.prd.md#cc-work-detail)。首屏先返回工作目的、当前可核实结果、主要剩余条件和可用下一动作；来源/目标关系、PR、外部定位、个人问题及保存结果作为同一详情的区域。原文、完整SHA、来源身份和读取诊断可展开，不抢占首屏判断。不同视图只传相同工作引用和返回位置；详情不创建新任务表或将多个源状态压成统一完成值。未产生 PR 的明确会话仍可作为工作入口，后续取得显式 PR 关系再关联到原工作。

关联保存来源双方身份、原依据版本、关系种类、提出方式（源明确引用、用户确认或建议）、适用工作树及当前可定位性。读取到 URL 只证明链接存在；只有正文明确表达的服务目标、验收或会话关系才能投影为该语义。候选查询优先当前工作已有关系、来源明确引用和用户选择范围，不默认展开上百个全仓复选框。相同端点/种类/范围关系去重，重复刷新只更新观测；具体修改、冲突和移除沿 §7.5。正式文档关联保留原条款定位，打开时显示实际已读版本；不可读时保留原ID及具体缺口，不产生决定同步状态。

外部定位与系统导航复用[DES-CC-007](#DES-CC-007)，工作详情仅展示明确的打开或复制入口；同一工作可保存多个准确定位，不能靠同分支猜测会话关系。

详情进入 §9.1 的主阅读区；首页、工作列表、关注、关系和工具返回都通过原 WorkRef 定位同一详情，不把它追加在长来源列表底部。返回上下文保存入口、筛选、列表/详情位置和焦点；切换同一工作保留页签，选择其他工作先恢复该工作的状态。工具动作及个人编辑表单按动作打开，结束后回到原 WorkRef/页签并显示实际结果。

个人编辑覆盖决定、问题、关系与本机工具，输入留存沿各自入口；工作选择、筛选与阅读位置继续恢复。旧记录的保存与隐私约束统一见[数据兼容](#DES-CC-008)。

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
| Git / GitHub | 对应工作树状态、PR source HEAD、PR 状态与真实 PR 事件；CI 在原 PR 的 GitHub 页面查看 | 未读页没有工作、合入代表部署或整体目标完成；App不据旧检查字段声称测试覆盖 |
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
| WorkRef / WorkItem | WorkRef 为来源命名空间与原对象身份：GitHub host/repository/PR 或 Issue，或原会话 SessionKey。WorkItem是目的、目标、PR事实与明确外部定位的派生组合；PR 无 Issue 仍独立成立，分支不作主键 |
| EvidenceLink | 引用真实源对象/版本、适用 AC/范围与结果性质；有 URL 仅证明链接存在，相关性和结论核实另行记录 |
| DecisionRecord | 保留原决定 ID、内容/理由/范围、确认时间与修订；决定内容、理由、范围、正式文档关联、确认与替代历史，见§7.4 |
| 状态摘要 / 架构索引 | 普通仓库文档及其稳定章节；记录派生性质、来源基线/核验时间/范围，引用 Goal、WorkItem 和主责条款，不创建新的目标或任务身份 |
| 首页 / 工作详情投影 | 组合已有身份及结果；各结论保留来源，项目总览不隐去工作树与环境边界 |
| 关系定位与修复 | 原始双方身份及版本、关系种类、提出/确认来源、记录 revision、解析状态和移除标记；见 §7.5，不删除原依据 |
| 原工具入口 | 工具/来源命名空间、环境和明确定位；仅判断可打开或可复制 |

同一工作可服务多个目标；建议依赖不自动成为阻塞。验证比例只在适用集合、分母和结果可核实时显示；文档active、PR merged、外部汇报、部署和验收各说明自身事实。

显式关系可以将同一PR的多个外部定位连接到工作详情，保留所有原引用；同分支或同标题不证明同一工作。没有PR的外部工作仍保留原定位，出现明确关系时可关联，歧义时保持独立。工程验收事实沿实际来源核对，PR状态不替代目标完成。

<a id="DES-CC-005"></a>

### 6.3 原工作身份与外部定位

WorkRef保留来源命名空间和原对象身份：PR采用host/repository/number，外部工作保留用户明确记录的namespace/project/environment/thread定位。WorkItem组合purpose、result、remaining、source_url及明确目标关系；分支、同标题和相同文本不用于自动合并工作。

当前工作查询只组合已选择项目的PR与明确外部工作，外部定位不带实时运行状态。历史私有记录隔离按§10处理。

<a id="DES-CC-007"></a>

### 6.4 应用 IPC 与能力校验

Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；承接[REQ-CC-007](command-center.prd.md#REQ-CC-007)。

IPC使用有限命令完成项目/文档/Git/GitHub PR查询、个人保存、文档树、本机工具与外部导航。参数由Rust定义并生成同包TypeScript DTO；不提供通用shell、SQL或任意文件代理。来源文本仅为数据。

保存的 `codex://threads/<UUID>` 只在严格校验无附加路径、query、fragment且UUID合法后打开；其他定位只复制。PR使用已核实仓库身份的原链接。系统导航成功只代表请求被接受，错误保留原工作与可复制定位。

## 7. 状态、事务与持久化

<a id="DES-CC-011"></a>

### 7.1 三类本地数据与事务边界

| 数据组 | 内容 | 处理边界 |
| --- | --- | --- |
| 可重建来源 | 文档索引/FTS、Git与PR来源缓存 | 原子发布内容与元数据，保留最近成功时间与范围；重建不删除个人记录 |
| 个人记录 | 决定、问题、关系、外部定位、阅读版本/位置与显示偏好 | 原project/environment保存，使用record revision拒绝旧版本写入；失败保留当前输入 |
| 工具操作 | ToolJobs、原operation/job及未知结果 | 独立工具存储和退出核对，沿本机工具合同恢复 |

普通与工具SQLite不合库；事务只覆盖所属数据组。个人写入先提交记录及revision，再发布保存结果；切换范围不取消已获准保存，晚到结果仍归原scope。已确认决定的内容、理由、范围和确认时间是个人记录，不是可重建缓存。历史兼容集中见§10。

### 7.2 流量控制与故障隔离

<a id="cc-host-concurrency"></a>

文档解析、GitHub 子进程、DB、分页与来源发布保持有界队列和锁外 I/O；取消或切换项目后拒绝旧结果覆盖新范围。读取失败保留最近成功事实，个人写入失败保留当前输入并显示实际错误。

同源重复读取或满额返回明确拒绝，已缓存阅读和无依赖个人操作保持可用，不虚构后台后继队列。

### 7.3 保存结果与工具操作

个人写入成功以实际事务提交为准，失败保留当前输入并明确原因。来源读取或外部导航没有确认结果时保留对应未知状态，不能修改缓存宣称源已改变。本机工具仍使用其原operation/job核对未知副作用；工具操作账本保持独立。

<a id="cc-decision-attention-state"></a>

### 7.4 个人决定与替代历史

个人决定使用原决定ID，保存内容、理由、适用范围、正式文档关联及历史版本。草稿可继续编辑；确认时保存真实确认时间。实质内容变化形成新的待确认或替代版本，原决定和确认时间保留，不能就地改写成后来结论。

决定记录由规范与决策入口阅读/保存，不参加个人待办的关注投影。正式文档关联仅用于打开依据，不推导实现状态或自动写回文档。现行命令、DTO和UI不提供双轨同步、证据版本、全范围checkbox或决定关注动作；个人问题、目标关系与本机工具保持原合同。

已保存内容与替代历史是个人持久数据，缓存重建不删除。旧同步/证据/关注记录保留原值供兼容，不重新保存、转换或投影；读取失败保留原数据，不能用空值覆盖。存储冲突/失败保留当前编辑并明确实际结果。

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

GitHub复用用户已登录的gh账号，读取固定只读REST端点；凭据继续由gh管理，本App不提供长期token输入或另存凭据。缺少gh不影响本地阅读，切换账号或仓库重新核对来源身份与读取范围。

依据 [Tauri capabilities](https://v2.tauri.app/security/capabilities/) 与 [CSP](https://v2.tauri.app/security/csp/) 设置生产窗口权限，把 app commands 纳入 `AppManifest::commands`，使用含已知安全修复的受支持稳定补丁。Rust 再次校验路径/身份/能力，不暴露任意 shell、SQL、全磁盘读写、任意 URL 代理或协议方法透传。

仓库文档、PR 内容和日志仅为展示数据；Markdown 原始 HTML 禁用，代码与高亮转义，默认不自动请求远程图片，外链经协议校验由系统浏览器打开。来源文字不触发本机指令。tracing 诊断按容量轮转，只记必要来源、operation ID、类别与耗时，过滤 token/敏感参数，不默认保存完整提示词或 transcript，不自动上传。

从 Finder 启动不假定继承交互 shell 的 PATH、代理或 SSH agent。显示 Git/gh 的实际路径与版本，允许明确选择路径；不执行用户 shell 初始化脚本来修复环境。

macOS 产物为 `.app` / `.dmg`，按实际目标硬件验证 arm64 和需要支持的 x86_64。对外分发采用 [Developer ID 签名、公证与 stapling](https://v2.tauri.app/distribute/sign/macos/)，不假定本次已有证书或发布凭据。首版可手动安装；需要自动更新时用 [Tauri updater](https://v2.tauri.app/plugin/updater/) 与静态 manifest，更新包签名和 Apple 代码签名分别验证，重启遵守 §5.4。

## 9. 质量与容量

<a id="DES-CC-010"></a>

<a id="cc-visual-composition"></a>

### 9.1 中文、可访问性与信息密度

Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；承接 [REQ-CC-010](command-center.prd.md#REQ-CC-010) 的适用义务。

[产品信息层级](command-center.prd.md#cc-ui-hierarchy)通过 AppShell 与共享工作区落实为“标题/结果/下一动作 → 相关依据与范围 → 原文/诊断”。本节为布局目标，实际已测构建与结果沿§11记录。默认采用舒展密度，不通过缩字或隐藏关键状态塞进首屏；紧凑密度只由用户明确选择，视口变窄不自动切换密度。

| token / 布局 | 默认实施值与行为 |
| --- | --- |
| 文字 | 正文 16px，辅助信息至少 13px，页面标题 24px、区块标题 18px；正文行高约 1.6。紧凑密度正文仍至少 14px，任务名称、待答内容与重要警告不降为小字 |
| 留白与控件 | 间距级数 4/8/12/16/24/32px；默认卡内 24px、主组间 24–32px，主控件高度至少 40px。显式紧凑可使用 36px 控件，保留可辨的点击与焦点范围 |
| 表面与状态 | 主题变量区分窗口背景、阅读面、次级面、分隔线、选中/焦点及成功/警告/未知；正文对比目标至少 4.5:1，控件边界和焦点至少 3:1。状态使用一致 SVG 图标加短文本，不只靠色，不用 emoji 或无分母的完成环 |
| 常规 1280×830 / 1440×900 | 左导航约 200px，内容横向留白 24–32px、分区间距 24px。扣除导航/留白后可用主区宽度至少 960px 才允许列表+详情：列表 360–420px，详情至少 560px；宽度不足立即换单主区 |
| 紧凑 1024×768 / 728×627 | 1024 可收为有名称提示的 72px 导航栏，728 使用可打开的导航抽屉；主区依次显示列表或详情，保留明确“返回工作列表/首页”。不得维持三列窄面板或整页横向滚动 |
| 放大与长内容 | 150%/200% 放大按实际可用 CSS 宽度切布局，不抵消字体放大。长中文任务名自然换行，必要收起时提供全文入口，不裁掉区分工作所需信息；只有代码/原始数据块允许局部横向滚动 |

App 提供“显示 / 文字与界面大小”入口，100% 为默认，支持选择 150%、200% 及复位；沿受支持的WebView/布局能力让倍率在 App 内实际生效并保存个人显示偏好。实现与验收记录实得 CSS 视口和布局，不能用截图后放大、系统放大镜或只在测试 DOM 注入样式代签该入口；调整大小不重新加载来源或触发工作动作。

首页四个语义区不渲染成等权重的文字框：当前工作、目的和下一步占主要阅读区；个人问题在有内容或覆盖缺口时形成清楚的次区，突出准确已知数量及少量具体问题；最近成果使用真实事件时间与短结果的纵向时间线；返回入口用工作名称、上次位置组成轻列表。主区选中工作后直接进入同一详情；宽屏可保留旁列表，不能把详情挂在原四区和来源列表之后。关注数量存在未读范围时同时显示该范围未知，不把数量写成全局完整值。

WorkList 每页默认 20 项、最多 50 项；首页默认最多三项重点、三条成果事件、三个返回入口和两项个人问题预览，其余有带范围的“查看全部/继续读取”。一项默认只呈现名称/目的、当前结果、必要剩余条件和一个主要动作；工作名不按固定行高裁断。真正待答及高影响警告始终在所属工作摘要或关注区可见；条目超预算时保留准确已知数量、第一项实质内容和直达完整处理列表，不把警告全部藏进“更多”。这些是默认条目、字段与动作预算，不限制读取真实完整内容。

WorkDetailPane 的固定摘要顺序为工作名称/目的、当前可核实结果、主要剩余条件与下一动作；下方用“概览 / 依据 / 历史”二级页签。概览展示工作与显式目标/PR/会话关系，已知端点直接打开；依据显示可读条款及影响判断的版本差异；历史承载真实结果事件、个人变更与操作回执。关联编辑、个人决定、个人问题和ToolTask 按动作打开；表单有清楚取消/返回，不与全部资料常驻堆叠。

SourceInspector 由具体事实的“查看依据”打开，在宽屏替换次级面板，在紧凑布局替换主区并带返回；不创建第三列。来源摘要先展示读到什么、覆盖/时效和具体缺口，完整路径、SHA、协议身份与 raw JSON 在其内按需展开，复制、定位原文和原工具打开保持可用。未安装/未连接仅在相关动作处显示可执行恢复步骤，不默认展开不可执行的完整配置表单。PR只在统一工作列表呈现；GitHub来源区保留连接、账号、范围补读和错误。文档原文检索保持独立入口。

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

### 10.1 独立来源与局部可用

<a id="DES-CC-012"></a>

本地文档/Git和个人记录可独立使用；GitHub PR与本机工具按实际配置接入，各自失败只影响所属区域。每项来源显示账号、范围、最近成功与当前错误，保留可用旧事实。客户端不因缺少外部工具而退成全局未知。

### 10.2 数据迁移与版本回退

<a id="DES-CC-008"></a>

SQLite及个人格式按版本读取，不为重建来源缓存删除决定、关系、问题或阅读记录；未来版本拒绝编辑并保留原值。当前普通记录沿原Project.id/environment及版本化personal键保存，旧关系导入保留原依据、确认时间与冲突信息。

旧Codex运行/请求/操作/输入和私有标记保留原存储，不删除、不发送、不作为当前工作、待答或退出依据；旧启用偏好不启动执行。已存在私有标记仍从实际旧runtime库只读核对，无法安全判定时不公开相关内容。旧CI字段允许兼容反序列化但不进入当前工作投影；来源重读只生成当前PR字段。

旧 `decision_followthrough_v1` 行与原revision冻结保留，当前普通个人状态的revision仍随正常保存前进。仍要求各personal family具有同一revision的旧App，在回退后可能拒绝读取个人工作状态；此兼容策略不承诺无缝降级。原数据没有删除，当前App继续按现行格式正常使用，不能为让旧版接受而重写旧证据或回退当前个人revision。

不兼容迁移须先核对所有实际writer和版本，备份个人/工具数据，在事务中完成并验证；失败保留原库。回退安装包只能保证其可理解范围，恢复旧备份不能撤销已发生的外部或工具副作用。

<a id="cc-system-verification"></a>

## 11. 验证设计与可追溯性

实现与验证入口列在下表及 §11.0；入口存在不等于完整 AC 已通过。实现 PR 记录实际版本、环境和结果，最后一列保留尚未证明的范围。分发与完整系统体验仍按实际范围验收。

### 11.0 当前验证入口与证据边界

[核心回归](../../../tools/oasis7-command-center/core/tests/foundation.rs)、工作/个人记录测试、前端测试与build、生成DTO核对、原生宿主测试和打包按改动选择。正式命令见[工具说明](../../../tools/oasis7-command-center/README.md#verification)。真实App使用实际仓库与GitHub PR完成阅读、个人保存、外部定位及关窗/退出/重开验证；浏览器fixture只证明其标明的模拟范围。

[历史验收](daily-work-verification.manual.md)保留原构建和环境，不改写为当前能力或当前通过结果。当前实现差距集中见§3，源码与测试入口存在不代表目标已经实现或验证。

### 11.1 验证映射表

| 上游条款 | 本设计条款 | 独立义务与适用条件 | 验证方法与当前入口 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-6e70287f-a156-4544-997d-5c1752e5efde；[REQ-CC-001](command-center.prd.md#REQ-CC-001) | [DES-CC-001](#DES-CC-001) | 独立App、真实项目身份与阅读恢复 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/foundation.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-2d1f3577-785e-40ac-8d25-ba0d414511f6；[REQ-CC-002](command-center.prd.md#REQ-CC-002) | [DES-CC-002](#DES-CC-002) | 来源版本、覆盖、失败与局部可用 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/work_projection.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| [REQ-CC-003](command-center.prd.md#REQ-CC-003) | [DES-CC-003](#DES-CC-003) | 目标、适用验收与有依据的进度 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/work_projection.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-eb004bfb-41c7-4e8f-aa5b-1f52cec6cf08；[REQ-CC-004](command-center.prd.md#REQ-CC-004) | [DES-CC-004](#DES-CC-004) | 规则后继、个人决定与替代历史 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/daily_work.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-a6126ad0-2f2d-47ad-a504-5e9ceff92cff；[REQ-CC-005](command-center.prd.md#REQ-CC-005) | [DES-CC-005](#DES-CC-005) | 同一原工作身份的列表与详情 | [适用测试入口](../../../tools/oasis7-command-center/ui/src/WorkOverview.test.tsx)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-649217d9-4c9c-43fb-898e-4fd3606a0bf5；[REQ-CC-006](command-center.prd.md#REQ-CC-006) | [DES-CC-006](#DES-CC-006) | 个人问题保存、回答、关闭及返回 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/daily_work.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-417e1bc6-1dac-4b80-891a-cef1f770bf55；[REQ-CC-007](command-center.prd.md#REQ-CC-007) | [DES-CC-007](#DES-CC-007) | 准确外部定位与受限系统导航 | [适用测试入口](../../../tools/oasis7-command-center/ui/src/WorkOverview.test.tsx)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| [REQ-CC-008](command-center.prd.md#REQ-CC-008) | [DES-CC-008](#DES-CC-008) | 旧用户记录保留、隐私隔离与不重放 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/work_projection.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| [REQ-CC-009](command-center.prd.md#REQ-CC-009) | [DES-CC-009](#DES-CC-009) | 菜单栏恢复及独立工具安全退出 | [适用测试入口](../../../tools/oasis7-command-center/core/src/tools/jobs/tests.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-76d54ed7-4028-460a-b22b-37a9088ad245；[REQ-CC-010](command-center.prd.md#REQ-CC-010) | [DES-CC-010](#DES-CC-010) | 中文、信息层级、缩放与键盘 | [适用测试入口](../../../tools/oasis7-command-center/ui/src/App.test.tsx)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| [REQ-CC-011](command-center.prd.md#REQ-CC-011) | [DES-CC-011](#DES-CC-011) | 个人数据、内容安全与凭据边界 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/foundation.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| [REQ-CC-012](command-center.prd.md#REQ-CC-012) | [DES-CC-012](#DES-CC-012) | 本地优先、独立来源与局部可用 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/foundation.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-ce7b612a-3316-4c40-9009-c0d295ff0ed6；[REQ-CC-013](command-center.prd.md#REQ-CC-013) | [DES-CC-013](#DES-CC-013) | 首页重点、成果、个人问题与返回 | [适用测试入口](../../../tools/oasis7-command-center/ui/src/WorkOverview.test.tsx)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-fdf061f8-ebdf-46e3-91a5-a2311e2bcb93；[REQ-CC-014](command-center.prd.md#REQ-CC-014) | [DES-CC-014](#DES-CC-014) | 关系复用、失效修复与原工作接续 | [适用测试入口](../../../tools/oasis7-command-center/core/tests/daily_work.rs)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；[REQ-CC-015](command-center.prd.md#REQ-CC-015) | [DES-CC-015](#DES-CC-015) | 独立本机工具、私有资料与原工作往返 | [适用测试入口](../../../tools/oasis7-command-center/ui/src/LocalTools.test.tsx)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |
| Trace refs: O7-REQ-729d655d-d8a8-4c44-9ea7-32578dc10f3f；[REQ-CC-016](command-center.prd.md#REQ-CC-016) | [DES-CC-016](#DES-CC-016) | 文档树、明确配对、覆盖与阅读恢复 | [适用测试入口](../../../tools/oasis7-command-center/ui/src/AppDocumentReading.test.tsx)；按本条目标选择反例，原生场景另测 | 实际PR的source/build、命令、范围与结果 | 入口不代表完整验收通过；未覆盖场景保留未知 |

### 11.2 实施时的验证层与关键反例

确定性回归覆盖来源失败/部分读取、超预算、切项目后的迟到结果、个人revision冲突、关系失效、外部定位拒绝、旧缓存读取与隐私隔离。本机工具另测原operation/job未知结果与退出冻结。

按[产品整体验收](command-center.prd.md#cc-product-acceptance)在真实App执行开始一天、处理个人问题、恢复工作及工具往返，记录实际source HEAD、机器/构建、来源范围、动作和结果。文档检查不代替原生行为；工程必要CI和评审沿开发流程执行。

## 12. 技术决定与实际风险

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

### 12.2 实际风险

WebKit布局、Finder环境、原生权限、可选工具、打包签名与规模限制均须在真实App证明。公开分发、自动更新和其他桌面平台按实际需求验证，Apple签名/管理员集成保持原暂停范围。

## 13. 本机工具集成增量

<a id="DES-CC-015"></a>

Trace refs: O7-REQ-fd3b34c6-4fb4-4614-8d7b-744a86d29c3a；承接 [REQ-CC-015](command-center.prd.md#REQ-CC-015) 的适用义务。

本机资料、同包 CLI、已有 signer 的普通调用、后台操作与私有资料隔离合同见[本机工具集成](local-tools.design.md)。资料由原 context 文件与历史主责，App 的内置能力和已安装 CLI 分别观察。Apple 签名及管理员助手已按用户要求暂停，未开放特权安装/管理；完整首装签名闭环尚未验收。

辅助工具入口复用 §5.7 的工作引用和返回位置，任务入口、就地反馈、独立导出/恢复输入及可信签名请求沿 [DES-CC-TOOLS-005](local-tools.design.md#DES-CC-TOOLS-005)。工具结果保留原 operation/job 与来源范围，返回工作只更新真实有关结果；工具安装、资料保存或签名准备不使工作/PR/目标自动完成。私有资料只在受控工具界面展示，普通工作只保留允许公开的最小结果。

<a id="DES-CC-016"></a>

## 文档全局阅读增量

Trace refs: O7-REQ-729d655d-d8a8-4c44-9ea7-32578dc10f3f；承接 [REQ-CC-016](command-center.prd.md#REQ-CC-016) 的适用义务。

[REQ-CC-016](command-center.prd.md#REQ-CC-016) 的产品/系统设计树与目录查漏由[专题系统设计](document-tree.design.md)承接，默认阅读与诊断分层由 [DES-CC-TREE-004](document-tree.design.md#DES-CC-TREE-004)承接。文首固定源码已有文件清单、目录、显式产品/专业树、配对详情及阅读恢复；[覆盖统计](../../../tools/oasis7-command-center/core/src/index_coverage.rs)提供候选、读取、遗漏、原因计数和有界路径样本，仍受当前读取基线与扫描预算约束。

工作详情沿同一源版本打开具体规则，文档入口返回原阅读位置或原工作，不把文档列表作为处理问题的替代路径。全局树的独立成功快照、明确提交/untracked 基线等未闭合合同及实际能力见专题设计 §11；显式树存在不证明全仓语义配对、默认阅读体验或真实桌面整体验收。

默认正文复用现有 MarkdownText 与主题/索引合同，按 [视觉与阅读布局](#cc-visual-composition) 的字号、行宽、标题/段落/列表/代码层级放入主阅读区；目录、来源、历史和原始 Markdown 按需进入。§8 的安全转义、禁用 raw HTML 与远程媒体自动加载保持，排版不另建文档树或改变规范效力。
