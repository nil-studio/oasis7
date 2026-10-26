# Viewer Minimal System Demo 设计

- 对应需求文档: `doc/world-simulator/viewer/viewer-minimal-system.prd.md`
- 历史 demo 实施与验证记录：GitHub task issue evidence。

## 1. 设计定位
定义最小化 Viewer Demo 数据生成方案：通过一个确定性 CLI 生成 `snapshot.json` + `journal.json`，让新用户可以低摩擦完成“生成数据 -> 启动 server -> 打开 UI”的闭环。

## 2. 设计结构
- CLI 入口层：`oasis7_viewer_demo` 提供场景选择和输出目录参数。
- 演示脚本层：按确定性策略选择 agent/location 并生成至少一个事件。
- 持久化输出层：产出 `snapshot.json` 与 `journal.json` 供 `oasis7_viewer_server` 消费。
- 文档引导层：README/quick start 给出最小端到端运行路径。

## 3. 关键接口 / 入口
- `oasis7_viewer_demo [scenario] [--out <dir>]`
- `snapshot.json`
- `journal.json`
- `oasis7_viewer_server` / `oasis7_viewer`

## 4. 约束与边界
- 这是离线 replay demo，不包含 live simulation server。
- 演示脚本只需保证低摩擦和有事件，不追求复杂玩法。
- 输出必须可重复生成，避免示例数据每次漂移。
- UI/Bevy 功能扩展不在本专题范围。

## 5. 设计演进计划
- 先实现 demo 数据生成 API。
- 再补 CLI 与持久化测试。
- 最后把最小运行步骤写进文档入口。


## 6. 目标、上游与当前边界

Owner：viewer_engineer；消费者为 demo CLI、离线 Viewer server 与运行手册。本文解决开发者获取可重复回放输入的问题，不承接正式玩家 live、原生客户端或发布验收。原第 5 节是历史演进顺序，不表示当前实现完成状态。

当前代码入口为 `crates/oasis7_viewer_runtime/src/viewer/demo.rs::generate_viewer_demo`：初始化场景、计划动作、执行队列、调用 `save_to_dir`，再返回 actions/events 摘要。CLI 由 `crates/oasis7_viewer_runtime/src/bin/oasis7_viewer_demo.rs` 消费；参数缺值、未知场景及生成错误以非零退出反馈。源码存在不是端到端运行通过证据。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance | 具体 obligation 与适用条件 | 本设计条款 | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [Demo 输入与输出](viewer-minimal-system.prd.md#in-scope) | 在有可用 Agent 的演示场景生成确定性快照/事件文件，供离线回放消费；产品适用性：N/A；reason=开发者离线数据生成辅助不定义正式玩家功能承诺；scope=demo CLI 与本地回放数据 | [生成流程](viewer-minimal-system.design.md#demo-flow) | simulator 场景初始化与 persistence；viewer_engineer 维护 CLI | 不覆盖 live streaming、UI 构建及正式发布 |

<a id="demo-flow"></a>
## 7. 关键流程、状态与持久化

1. 选择场景和输出目录，按最低 Agent ID 计划动作；多地点尝试供电与移动，单地点退化为 transfer。
2. kernel 执行动作并产生 journal；拒绝事件也是可观察事件，不可写成移动成功。
3. 持久化由 kernel `save_to_dir` 委派到 `save_to_dir_with_chain_resource_context`：先写 `snapshot.json`，再写 `journal.json`；`persist.rs::write_json_to_path` 序列化后通过 `fs::write` 直接写入，会覆盖已有同名文件，没有跨文件原子提交或失败回滚。中途失败可能留下部分输出或新旧混合数据，CLI 仅在两次保存均成功返回后打印摘要。
4. 消费者只对成功生成的数据对启动离线 server 并确认可加载；失败输出仅作诊断，不能当作完整回放输入或旧证据仍完整的证明。每次尝试应由调用者选择独立目录，避免重跑继续覆盖失败现场。

无 Agent 时 planner 返回空动作；因此“至少一个事件”须以支持的演示场景实际输出验证，不能扩展到任意自定义空场景。持久化格式/回放顺序由 simulator 的快照和 journal 合同拥有，demo 不创建第二套 authority。

## 8. 部署、容量与兼容约束

在本地工作目录运行，输出路径由调用者选择；程序不提供旧文件保护。调用者应为本次生成选择独立目录，不将他人的运行证据目录作为输出目标。小规模离线样例不证明高密度世界容量。参数与 `snapshot.json`/`journal.json` 名称保持原合同；场景或持久化 schema 改变时同时复核 server 加载，需要恢复时，只能使用事先另存并验证完整的数据对；直接覆盖后的原文件不保证可恢复，不能混用新旧文件。

## 11. 验证设计

### 11.1 验证映射表

| 上游 requirement | 本设计条款 | 独立 obligation 与适用条件 | 验证方法 / source / scenario / candidate | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [Demo 输入与输出](viewer-minimal-system.prd.md#in-scope) | [生成流程](viewer-minimal-system.design.md#demo-flow) | 支持的有Agent演示场景按确定性策略生成可加载的快照/journal；拒绝事件不冒充移动成功 | [测试手册](../../../testing-manual.md)；执行 `cargo test -p oasis7_viewer_runtime plan_demo_actions`，再生成默认场景并由 server 读取同目录文件；L1 动作计划及本地离线加载；记录实际 source、scenario、输出目录与 server/UI 构建；使用 Cargo canonical wrapper 约定 | 实际PR/验证记录：planner测试命令/退出码、scenario与生成文件摘要、事件数、server加载日志；声称UI打开时附真实界面证据 | planner单测仅覆盖多地点/单地点分支；不证明空Agent场景事件保证、跨文件原子提交、live或UI发布 |

## 12. 决策、风险与未决范围

保持确定性小样例，而不是引入复杂脚本语言。资源不足可能产生拒绝事件；空 Agent 场景不能套用“至少一个事件”保证。当前文档规范化不重跑 demo、Rust 或 UI，历史 task evidence 保持原验证候选与范围，不由本表升级为已验收。

## 应用与宿主边界

`oasis7_viewer_runtime::viewer` 拥有 Viewer 应用状态、会话与控制、投影、网络监听及恢复编排，直接消费 `oasis7` 的无界面宿主。核心不依赖或重导出 Viewer 应用；仓库消费者使用实际 owner 的 Rust 路径，不保留 `oasis7::viewer` 兼容回边。

核心 `runtime_host` 维护通用世界 bootstrap、Gameplay 请求到 Runtime Action 的组装、进程目录锁与持久 replay ledger、Runtime receipt 回读和 Harness 权威适配。完整 `WorldSnapshot`、事件及 Runtime/Player/Chain 组合归档仍由核心 Simulator 宿主持有。Viewer 的协议响应在应用边界组合该归档；客户端只消费自身所需合同和明确的解析模型。

共享进程锁由 `oasis7_hosted_access::auth_storage` 实现，nonce ledger 和 execution world writer 使用相同原语。它只支持同主机 Linux/macOS/Windows 的受信本地文件系统，须提供内核文件锁、原子 rename 和所用持久化操作；不承诺 NFS/SMB、网络共享盘或多机分布式互斥。目录及祖先由部署方控制，不允许运行期间移动、删除、替换或通过不同文件系统别名访问锁锚点；同权限恶意进程和外部文件系统修改不在此协作锁的隔离能力内。

获取前规范化普通路径末尾斜杠/点组件并解析父目录，使同一目标的这些写法共享锚点；拒绝目标自身符号链接/reparse point 和普通文件 hardlink，Windows 已存在目标还须解析完整路径以统一短文件名；拒绝组件末尾点/空格、ADS 冒号、DOS 设备名以及非本地磁盘命名空间等不确定表示。路径解析失败不得获取。对每个规范目标路径，永久保留 `<path>.lock.guard` 普通文件，使用 Rust 标准库 `File::try_lock`（Unix `flock` / Windows `LockFileEx`）获取独占锁。锁文件不截断、不删除、不换 inode；文件句柄从首次 owner 检查之前持有到 guard 清理之后。所有 owner 检查、过期回收、quarantine 删除和新 owner 发布均在该内核互斥范围内；失败、未知 owner、进程存活状态不明、权限/符号链接/非普通文件、无法获取或验证 OS 锁均不得返回可写 guard。PID/token 仅用于旧状态判断与清理归属，不能代替 OS 锁。健康 owner 不回收，PID 复用保守拒绝；崩溃释放内核锁后，下次获取在互斥范围内回收确认死亡的旧 owner。owner 发布或父目录 sync 失败不授予写资格，清理只涉及本次所有物；quarantine 清理失败拒绝本次获取。

升级必须先停止所有使用旧目录协议的 writer，不能混跑不认识 `.lock.guard` 的旧二进制；仅有新进程持锁不能约束旧回收者。未知/损坏 owner 留待停止全部 writer 后人工核对，不凭超时推定死亡；保留永久 guard 文件。fork 子进程不得使用继承 guard 写入或继续运行服务，应立即 exec/exit；子进程关闭继承句柄不能主动 unlock 或删除父进程 owner。guard 不可跨进程转移。这里不改变 nonce 原件、claim/consume 身份、未知提交结果和世界持久化合同。

纯请求与签名验证由既有 Proto/Client API 拥有；纯验签不检查磁盘 replay ledger，Viewer 宿主适配在验签后继续执行 nonce 未使用检查。Harness 与 Game Client 不获取 Runtime World，也不承接受信 receipt 验证。Chain 宿主直接查询 World 的 canonical agent context，不经 Viewer 转发。

验证边界包括：核心无 Viewer 依赖、Runtime/World Service 的生产闭包无 DistFS、协议签名字节和归档兼容回归、迁移后 Viewer 测试的实际执行及默认/无默认 feature 检查。修改 Viewer 普通实现只使应用及实际应用消费者失效；编译耗时须按真实源码变化测量，不能从 no-op check 推断提速比例。此迁移不改变游戏行为与正式 Skill/API，因此不以新增模型试玩代替合同和恢复回归。
