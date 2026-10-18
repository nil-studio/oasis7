# 日常工作闭环验证与剩余验收

- 文档 ID：`MANUAL-ENGINEERING-COMMAND-CENTER-DAILY-WORK-VERIFICATION`
- 状态：`active`
- owner_role：`qa_engineer`
- module：`engineering`
- last_reviewed_at：2026-10-18
- 产品合同：[整体验收](command-center.prd.md#cc-product-acceptance)
- 系统合同：[验证映射](command-center.design.md#cc-system-verification)
- 实施范围：[Issue #4848](https://github.com/nil-studio/oasis7/issues/4848)

## 版本、环境和结论

实现源码候选 `52dd4aed915ce5ca75e00ddff6ba5e1918ebdec6`。原生任务发现并修复了长目标挤占首页及启动恢复竞态；完整交互复测使用 `ebec894d85392c52e2fc82ca9111f075abbc9171`，随后候选补齐近期打开排序、旧版决定完整归档、来源限流和个人错误隔离，并修正仅复制外部定位的主动作及只比较两份主责摘要（不把验证记录的引用当新冲突），补齐已索引正文相对链接导航与最初入口焦点返回。原生工具页切换曾出现表单可见但 AX 字段缺失，改为按任务挂载后实测签名 Job、身份、grant 与上下文标签重新进入 AX；无效 SignContext 严格拒绝并就地反馈。后续最终打包与 CI 的确切 head 由实施 PR 的构建回执记录，不能把较早工件说成最终工件。

环境为 macOS 26.5（25F71）、arm64；保留 Tauri/Rust/Solid 架构。常规窗口约 1280×830 logical，窄窗口实测约 728×627。验收 App 使用独立 `com.oasis7.command-center.acceptance4848` 数据目录，不修改原 App 数据库。源码阅读项目是实际 oasis7 工作树，tracked Markdown 索引约 610 文件、9529 章节；这些数量属于本次窗口，不是全仓或未来容量保证。

**本 issue 的实现、自动化及适用原生交互验收已完成。** 用户于 2026-10-18 在本任务中回复“IME我试了可以”，确认真实中文输入法实测通过；这是用户人工验收证据，不是自动化代签。用户未另报输入法名称、逐键记录或不同构建，因此该确认按本任务提供的 Acceptance App 场景记录，不扩展为其他环境保证。按用户同日明确要求及 [REQ-CC-010](command-center.prd.md#REQ-CC-010)，VoiceOver/读屏已移出产品及验收范围；既有中文正文、Tab 焦点和 AX 证据仍只证明各自范围。Apple 签名、管理员助手、额外付费模型与部署没有新增授权或执行。

## 来源样本与证明范围

| 样本 | 实际来源 | 可证明与不可证明 |
| --- | --- | --- |
| 三项 PR | 2026-10-18 REST 按身份读取 [#4870](https://github.com/nil-studio/oasis7/pull/4870)、[#4854](https://github.com/nil-studio/oasis7/pull/4854)、[#4847](https://github.com/nil-studio/oasis7/pull/4847) | 保存实际 HEAD、merged_at、updated_at 与独立观察时间。三项均已合入；没有由此核实部署、目标验收、评审或全部必要 CI |
| 一项 PR 服务两个目标 | 隔离 personal 记录，端点为实际已读取章节 | 明确标为个人验收关系，不声称上游 PR 正文已确认该语义 |
| 两个同分支不同会话 | 显式标记“受控来源回放”的两个历史身份 | 验证不互相覆盖与原执行身份保留，不冒充外部 App 的实际会话 |
| 一个外部现场 | 明确保存普通工具名称与定位 | 仅复制/打开定位；无消息读取、实时观察或控制权 |
| 已确认未同步决定 | 隔离验收决定：基础闭环保持模型默认关闭 | 确认、正式条款和实现同步分别存在，不重新计为待批准 |
| 旧摘要冲突 | 验收窗口读取的 README/架构摘要当时称 #4870“尚未合入”，REST 为 MERGED（后续 main 已更新该摘要；历史样本以本记录的源码候选为准） | 在原字段并列保留版本和核对入口；只纠正 PR 合入轨道 |
| 真正协议请求 | 从仓库 `codex_runtime.rs` fixture 提取的独立本机协议进程，关闭测试 helper；无模型 provider | 原生 App 实际执行 initialize、thread/start、turn/start 和原 request 回复；消息明确标记受控回放，不代替真实模型/外部现场验证 |

普通 PR 不要求存在 Issue；自管和外部定位会话不要求存在 PR。工作身份由 WorkRef 决定，同分支不是控制权证明。私有线程及其派生字段在普通投影、个人读取和发起记录中排除；标记从实际独立 runtime DB 只读读取，不查询错误的普通 DB。

## 原生任务记录

计时使用自动化动作与后续观察的墙钟间隔，是上界样本，包含工具观察间隔；不宣传基准性能。首次加载与已可用数据后的理解分开记录。

| 用户任务 | 操作及观察 | 结果与额外成本 |
| --- | --- | --- |
| 开始一天 | 查看四区和 #4870 旧摘要冲突，进入统一详情，读取两个目标及“部署/目标验收尚未核实”，直接打开原 PR | 数据可用后约 20 秒内形成判断并进入正确工作。无额外状态搜索、无手填已有 PR、无空白会话。浏览器实际出现 #4870 的正确 URL/标题 |
| 处理中断：个人问题 | 在 #4870 工作中保存“合入后是否仍需保留部署/独立故障域未知”的验收问题，进入待处理，明确回答，刷新并查看历史 | 约 20 秒完成回答与回看；一次视图切换、一次问题录入。已处理项不重新待答，PR 的未知验收条件仍保留 |
| 处理中断：原协议请求 | 显式启用隔离 fixture，预览 `request` 后发起一次；原请求 `9007199254740993` 绑定同 instance/thread/turn，输入 A 并回复；刷新核对 | 回复后约 14 秒内观察到“来源已处理”。trace 仅一次 thread/start、一次 turn/start、一次原请求回复；刷新无重发。来源已处理不冒充模型结果 |
| 恢复工作 | 保存筛选 `storage`、原 #4870 详情、阅读/滚动位置及用户明确留存的中文普通草稿，完整退出后重开 | 最终恢复原视图、筛选、工作、草稿与留存选择，没有自动发送。复测可用观察上界约 78 秒，含完整本地扫描和其他观察间隔；不是启动时延保证。实验预览展开状态需再点一次，正文仍保留 |

首次实际打开工作树在约 19 秒内观察到本地来源；完整重开还需来源读取/投影恢复，不能用“可用数据后五分钟”掩盖这段时间。发现的恢复竞态是父级第二次来源读取取代首次恢复，修复后被取代/失败读取不消耗恢复标记，用户新输入优先。此顺序已有回归。

## 辅助交互和错误边界

- 原生窄窗口检查了长实例/操作身份、回执、警示和按钮换行，未见横向裁切；大段目标与冲突正文已折叠，完整依据可下钻。
- 原生 Tab 从筛选进入当前工作按钮；进入详情聚焦工作区域，返回按当前同身份按钮恢复；来源详情另有同 source ID 焦点回退。
- 普通中文正文与多行草稿在完整重开后保留。此前自动化尝试系统输入切换没有形成候选证据；随后用户人工确认 IME 可用，按上述来源计入验收；VoiceOver 未执行的历史事实保留，现已移出产品和验收范围；未更改系统键盘/安全设置。
- 源队列满立即返回可定位错误；已有内容仍可读，普通保存不排在扫描/GitHub 后。加载中的空白不宣称“已读成功空”。
- 正文相对链接与锚点只导航明确索引目标；缺失、歧义或越仓显示反馈。来源内跳转保留最初入口，关闭详情返回原按钮，已有 App 回归。
- 工具按资料、备份恢复、签名、验签、CLI、操作核对分任务；普通 UI/桥接验证覆盖独立前提、固定导出范围与秘密失效。未执行实际签名或特权安装。

## 可重复检查入口

在仓库根运行，使用共享 Cargo 缓存，不修改 CARGO_TARGET_DIR：

```sh
env -u RUSTC_WRAPPER cargo test --manifest-path tools/oasis7-command-center/Cargo.toml --workspace
env -u RUSTC_WRAPPER cargo clippy --manifest-path tools/oasis7-command-center/Cargo.toml --workspace --all-targets -- -D warnings
env -u RUSTC_WRAPPER cargo fmt --manifest-path tools/oasis7-command-center/Cargo.toml --all -- --check
env -u RUSTC_WRAPPER cargo run --manifest-path tools/oasis7-command-center/Cargo.toml -p oasis7-command-center-core --example export_types
npm --prefix tools/oasis7-command-center/ui test -- --maxWorkers=1 --testTimeout=30000
npm --prefix tools/oasis7-command-center/ui run build
python3 tools/oasis7-command-center/scripts/test_package_context_cli.py
```

本轮本地 Rust workspace 212 项、UI 160 项通过；包含原有隔离/兼容回归，不把数量当实际用户验收。重点入口为 [daily_work](../../../tools/oasis7-command-center/core/tests/daily_work.rs)、[协议故障/恢复](../../../tools/oasis7-command-center/core/tests/codex_runtime.rs)、[工作投影](../../../tools/oasis7-command-center/core/tests/work_projection.rs)、[REST 窗口](../../../tools/oasis7-command-center/core/src/github_tests.rs)、[宿主慢源与饱和](../../../tools/oasis7-command-center/src-tauri/src/commands.rs)、[UI 恢复](../../../tools/oasis7-command-center/ui/src/WorkOverview.test.tsx)、[部分同步义务](../../../tools/oasis7-command-center/ui/src/DecisionFollowthroughPanel.test.tsx)、[工具秘密/范围](../../../tools/oasis7-command-center/ui/src/LocalTools.test.tsx)和[严格 SignContext](../../../tools/oasis7-command-center/core/tests/sign_context_import.rs)。

独立安全/兼容评审发现并修复了双 DB 私有标记查询、未决意图窗口截断/永久锁定、决定 CAS 事务外窗口、ready 状态不匹配；最终实现候选复审未见阻断。实际 CI、合入及最终构建 head 由实施 PR 记录；本文件不代替这些事实。

## 验收收口

实现已随 [PR #4881](https://github.com/nil-studio/oasis7/pull/4881) 合入 `88965b8f27871f4d11c2d8e22855d0b6c52a2e0f`；用户人工 IME 确认补齐此前最后一项交互验收。提供给用户的 Acceptance App 源码为 `815f8b874b6145fe73c9a50701e881d8d2bbd438`，本轮后续仅更新文档。同步产品状态及本记录、完成文档 PR 的必要检查与合入后关闭 #4848；专题其他增量、签名、管理员安装、部署及付费模型不在本 Issue 的收口范围。
