# 持久世界与节点生命周期实施记录

日期：2026-10-12。对应需求见 [正式网络分层 PRD](formal-network-tiers-testnet-mechanism.prd.md)，部署边界见 [现行 governed bootstrap runbook](public-testnet-governed-bootstrap.runbook.md)。

本轮依据同日《oasis7 正式持久世界与节点生命周期优化方案》实施。交付围绕同一部署优化目的，原子接通正式持久世界运行与恢复、节点接入、成员追踪与部署管理；根据 2026-10-14 用户快速合入要求，不按 crate 数量机械拆分。本文记录已接通的入口及未完成的验收，不把工程 prerequisite 解释为正式开放。

用户选择先完成代码和本机验证。本轮没有真实故障域、生产部署或公开开放的验收结论。

Trace refs：沿用 [AC-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#ac-dcs-002) 与 [AC-DWE-002](../../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-002)，设计承接见 [原 candidate 恢复](../design.md#des-p2p-original-candidate-recovery-v1) 与 [受控 epoch 恢复](../design.md#des-p2p-controlled-epoch-takeover-v1)。本 PR 验证其本机基础设施部分；完整产品验收、新配方/模块行动连续性、正式自主试玩及真实部署矩阵仍按后续范围单独验收，不把 Trace ID 当任务身份或通过证明。

## 恢复优化与后续验证边界

当前 native restore 复用已认证 current/parent Result envelope 的既有有界节点材料，仍核对内容摘要、完整根和最终 head；冷恢复新 RPC 从 777 次降至 6 次，协议期限不变。Business Index 的 257 个空树哈希只依赖固定 codec 与深度，进程内只构建一次；不缓存业务根、对象权威或资格，全部根与证明校验仍逐次执行。完整版本、构建和 CI 结果由 [PR #4683](https://github.com/eng-cc/oasis7/pull/4683)承载。

原生历史读取适配器完整代理既有内容缓存读取与回填，维持共享 64 MiB/16384 项及局部 16 MiB/4096 项上限。读取持久 publication 的 proof 时，每次先按原私有路径和 no-follow/64 MiB 限额读取当前完整文件，再用域分离 BLAKE3 与长度绑定解析 memo；冷 miss 保留完整 CBOR 预算与字段解码，hit 仍核对所请求的位置。memo 仅保留未受信任的解析 proof，最多 256 项和 16 MiB 序列化权重（不是精确堆上限）；不保留 material、authority、Qualified 或失败结论。签名、权限、opaque cursor、历史与切面校验每次照常执行，文件篡改、删除或权限变化不能被旧缓存掩盖。

fixture v87 / production v30 / endpoint release v14 的普通提交/重启、完整八阶段崩溃恢复、副本失联恢复、signer handoff、Frozen/Prepared 副本存活实际场景均有通过证据。旧组合长历史有两次失败：一次第 67 次提交的 RPC EOF（published/双端 head 仍为 70，原 pending 71 保留），另一次完成 70 次变更及最终重开后的普通签名查询超时。后者采样确认重复构造空哈希链的 CPU 热点；不能据此断言旧 EOF 根因已经确定。长历史须由优化后构建及最终 source HEAD CI 重新验证，不沿用这些失败组合签过。

- TODO：继续定位偶发传输 EOF 的服务端错误，并为测试 oracle 提供安全可重建的持久现场；必须保留原请求并按既有 unknown/reopen 合同续做，不增加协议期限或替代 candidate。
- TODO：测试 helper 的名义 20 秒轮询预算目前不能中断同步冷读取；为该读取增加可中断的测试预算并证明超时诊断，保持生产协议期限与资格校验不变。

## 冷证明路径读取与部署边界

在 [代码 PR #4683](https://github.com/eng-cc/oasis7/pull/4683) 合入前，本节新路径 RPC 与 runtime 仅为候选；代码合入本身也不表示部署已启用。只有该 PR 已合入且匹配的 P/R 端点与节点工件均已部署后，下面的运行合同才适用；文档先合入不启用该能力。长历史场景的最新候选、实际验证结果与历史失败处置由该 PR 的验证记录承载，本文旧版“待重测/待验”记录不用于判断当前候选，也不代签其他 TODO、最终 CI、真实部署或整套验收。

冷路径使用有界只读路径 RPC，单条路径最多 257 个节点，沿用原单节点与 64 MiB 总字节限制；每次远端取路径仍验证双端签名上下文、minimum、完整 root/key 路径及合法 absence，缺节点不能充当不存在。验证成功后只回填原有界内容缓存，不缓存资格或负面判定；原 SDK 30 秒及 RPC 期限不变。本机同版本场景验证不能作为生产 SLO。

部署含此能力的新 runtime 前，先部署支持该 RPC 的匹配版本 P/R 端点，并核对两端版本。旧端点不支持新 RPC 时 fail closed；没有 EOF fallback 或 capability 协商，不能据文档更新推断旧端点兼容。TODO：补充真实混版本正向升级与回退验证；本机同版本通过不代表该矩阵已通过，原权限与资格保持。

## 本机正式入口验收进度（2026-10-14）

这些结果来自不同的串行测试与本机进程运行，不能合并解释为整套部署计划已通过。历史失败只在下表注明的场景仍未重测时作为未解决项；已修复的旧错误不作为当前阻断。

| 范围 | 最新证据 | 状态 |
| --- | --- | --- |
| `oasis7` 正式库 | 2026-10-14 测试模块拆分后 canonical `v9`：80 项通过，109.45 秒，含 effect 身份、retention 拆分及 cursor 重基；严格 library clippy `v1` 通过，36.62 秒；日志 `/tmp/oasis7-lifecycle-formal-lib-canonical-20261014-v9.log`、`/tmp/oasis7-lifecycle-lib-clippy-canonical-20261014-v1.log` | 通过该测试组与 library clippy，binary 另验 |
| 正式读取及 cursor 恢复 | canonical `formal_read` 库组 `v7`：16 项通过，11.93 秒，含 33 个实际签名提交后的安全重基、fork/older/stale-CAS 拒绝；fixture `v31` 三项实际 CLI/IO 测试通过：正式 watch 1.14 秒、工程 watch 0.37 秒、替换后目录同步失败报告 0.04 秒；正式场景含断线重启、旧响应/旧切面及重复、重排和绑定篡改游标拒绝；日志 `/tmp/oasis7-lifecycle-watch-cli-fixture-20261014-v3.log` | 该库组及三项实际场景通过 |
| Launcher typed 开发配置 | 2026-10-14 canonical `v3`：176 项通过、1 项忽略，1.08 秒；含新适配器的 dev node/local-provider 参数及 typed 配置、formal synthetic 拒绝、路径穿越/原始空白 ID 拒绝；严格 launcher clippy `v2` 通过，4.13 秒；日志 `/tmp/oasis7-lifecycle-launcher-full-canonical-20261014-v3.log`、`/tmp/oasis7-lifecycle-launcher-clippy-canonical-20261014-v2.log` | 通过该 unit 测试组与 production clippy，真实入局未验 |
| 开发及脚本适配合同 | fixture `v29`：typed 配置两项、正式 signer 完整性一项、live/显式 P2P hint 两项、inspect 无副作用一项通过；node-start 与 observer-sync shell 测试通过，日志 `/tmp/oasis7-lifecycle-node-start-adapter-20261014-v1.log`、`/tmp/oasis7-lifecycle-observer-sync-adapter-20261014-v1.log` | 参数/适配与拒绝合同通过，shell stub 结果不作为真实节点部署证据 |
| 真实持久化格式探针 | 2026-10-14 canonical `shipping_compatibility_descriptor_` `v4`：4 项通过，30.63 秒；真实 driver 产生 World、checkpoint、Snapshot/Journal CAS，验证合法滞后缓存可读取、权威 CAS 替换拒绝、selector 0/999 拒绝、latest 低于更高异常记录拒绝，探针前后原目录和文件 SHA 不变；日志 `/tmp/oasis7-lifecycle-shipping-compat-probe-canonical-20261014-v4.log` | 通过该实际 decoder/CAS 集成测试组，Package 真实升级/回退仍待验 |
| full-sync 合同及离线探针 | 2026-10-14 canonical `formal_full_sync` `v2`：24 项通过、4 项忽略，5.87 秒；实际签名 HTTP 的在线备用端点/错误 pin 验证，以及在线取得 successor proof 后停服的完整离线 C0/CAS 重放探针；日志 `/tmp/oasis7-lifecycle-fullsync-contract-canonical-20261014-v2.log` | 通过此组合，忽略的普通 CLI 子进程另验；离线探针不声明联系过 provider |
| `oasis7_chain_runtime` formal 测试 | canonical no-run 构建所得 fixture `v31` 串行 formal 组：123 项通过、20 项忽略，16.73 秒；日志 `/tmp/oasis7-lifecycle-runtime-formal-fixture-20261014-v31.log`，含实际格式探针、正式 watch 和原有归档/grant 回归；忽略项不计入通过 | 通过该测试组 |
| `oasis7_distfs` canonical | 空子树、Business CAS 打包及 TCP 小包合并调整后完整库串行 `v7`：199 项通过、0 项失败、10 项忽略，428.79 秒，含真实 shared-index 非法第二边、可写 staging 拒绝及 SIGKILL 落盘边界；TCP 小包合并调整后 all-targets 严格 clippy `v10` 通过，3.20 秒。端点 release `v9` 已构建（52.01 秒），opt-level=3、test=false、debug-assertions=false | 通过该测试组及构建，忽略项另需实际运行 |
| `oasis7_node` canonical | clock 恢复后 `84646`：517 项通过，89.98 秒；定向 clock 回归 3 项通过 | 通过该库测试组 |
| Native TCP 传输 | 本机定向恢复通过，但最新 d88 Linux CI 的历史材料恢复仍触发原 30 秒期限；已有 Result envelope 的节点被重复进行 778 次 RPC。7de774f08 使用仅来自 Qualified target/immediate-parent Result envelope 的有界、摘要校验节点缓存；缺失节点仍经认证 RPC，30 秒期限不变。定向场景 8.04 秒通过，远端读取少于 64；损坏缓存拒绝测试及 all-targets 严格 clippy 通过；独立评审无 P0/P1 | 本机定向通过；最新 CI 仍需确认，不能用旧串行通过结果代签 |
| Native 双端实际进程 | release 端点 `v5` 加 canonical test harness：双端资格确认后 SIGKILL/重开 `v6` 通过，1.49 秒；恢复导入及连续 epoch 原请求续交 `v4` 两项通过，4.33 秒。受保护 provider 为本机协议模拟 | 通过这三项真实子进程场景 |
| Native 长期持久化 C10 | 业务 pack 收窄后的 canonical 显式运行 `v6` 通过，346.21 秒：71 条 Qualified、68256671 bytes、五次重开，当前 head 71 仍取回 position 2 的原结果；使用真实本地存储及原生库，不是 World/Wasmtime 长期进程测试 | 通过该原生库场景 |
| R-only Frozen/Prepared 实际运行 | `41571`：152.21 秒；最大结果延迟 57 ms、inclusion 18 ms、整轮 749 ms | 通过该场景 |
| Zero/genesis 实际运行 | `46981`：33.52 秒 | 通过该场景 |
| C03 持久重开 | 完整八阶段实际运行 `60429` 通过，536.86 秒；每阶段均由原请求恰好一次 Qualified，并精确重开两次。`primary_published` 另有强制删除本地 C0 材料缓存后的实际通过（80.95 秒） | 八阶段通过 |
| 损坏候选拒绝 | `corrupt_existing_candidate_or_frozen_artifact_refuses_prepare_without_promotion`，运行 `3416`：通过，3.69 秒 | 通过该定向回归 |
| World 长期历史 | `v13`：653.22 秒通过，70 次真实签名更新，捕获切面 73 条 Qualified、102233085 bytes，热 Tick 32 条；第 16/32/48/64 次及最终重开核验原请求、冷历史与恰好一次效果。端点使用 canonical release 构建，harness/runtime 为 debug test artifact，当时使用 20 秒测试等待预算（非生产提交 SLO）；debug 端点整轮仍受延迟限制 | 通过该构建组合，最新源码待重测 |
| World 初始激活 CLI | `1581`：实际 Prepare 中断后原请求重试、重复激活及签名冲突拒绝通过，40.98 秒 | 通过该场景 |
| Full-state sync CLI | 原 C0 加入/重开、移动 head、死 seed 转独立固定信任 alternate 三项实际通过；`v4`：合法 epoch 更新后停止、安装新 grant、从不变 C0 重开通过，6.81 秒 | 四项分别通过，完整脚本待验 |
| 单副本失联只读 | `v2`：SIGKILL R 后新提交暂停，签名状态报告 degraded；指定原已发布切面可读，未指定切面请求拒绝。R 原目录重开后原请求恰好一次提交，24.15 秒 | 通过该场景 |
| 健康集合转换及原生安装 | actual v17：41.10 秒通过双端安装、激活、跨 epoch Normal、重启及原请求冷查询，原 CommitRef/effects 不变。组合为 runtime production v20、fixture v54、endpoint release v13；日志 `/tmp/oasis7-lifecycle-healthy-actual-20261014-v17.log`。HTTP worker 使用固定 8 MiB 栈，签名、请求预算与原安装期限不变 | 此真实进程场景通过；CLI 与 owner 故障场景另验 |
| 健康集合实际管理 CLI | actual v22：100.32 秒通过原 operation 计划/续做、独立 P/R 安装激活、恢复后的 epoch 2 owner、首个 Qualified Normal、固定 C0 完整证明及原请求结果不变；fixture v80 / runtime production v27 / endpoint release v14，日志 `/tmp/oasis7-lifecycle-healthy-cli-20261015-v22.log`。H−1 由逐步验证的 C0→H lineage 提供，H+1 通过历史精确切面独立双读 | 此完整实际 CLI 主链通过；新 owner 修改另由下述 v25 故障组合验证 |
| successor writer lock 故障续做 | actual v25：104.88 秒通过真实锁占用导致首次启动失败、释放锁后按原 operation 重试、原失败回执留存、新 incarnation Ready、固定 C0/authority/epoch 与原 transition 不变及 H+1 Normal 验证；fixture v83 / runtime production v29 / endpoint release v14，日志 `/tmp/oasis7-lifecycle-healthy-owner-writer-lock-20261015-v25.log`。启动器仅接受匹配新 Child PID/出生身份的回执，旧失败回执不冒充新进程结果 | 此真实失败重试场景通过 |
| old owner SIGKILL 故障续做 | actual v25：118.58 秒通过旧 owner 真实 SIGKILL、原 transition/operation 续做、独立 P/R 安装激活、successor Ready 与 H+1 Normal、原请求结果不变；同为 fixture v83 / runtime production v29 / endpoint release v14，日志 `/tmp/oasis7-lifecycle-healthy-owner-sigkill-20261015-v25.log` | 此真实进程终止场景通过；不代签断电或跨故障域验收 |
| 健康集合 RPC 边界恢复 | native install `v3`：1 项通过，3.50 秒；真实 TCP 受保护 checkpoint/CAS 恢复激活后的 H，旧恢复入口缺健康凭证拒绝，新入口核对 typed activation、C0、exact H 与 successor context；canonical all-targets clippy `v5` 通过，4.82 秒 | 原生 TCP 组合通过，管理安装 RPC 待接入 |
| 健康集合普通读取 | `healthy_durable_set_read_` `v3`：3 项通过，0.33 秒；验证 Qualified H/H-1、双 seal/双安装、inventory 一致、首个 Normal 和固定 C0 的 V2 重放；清除旧资格缓存后从当前签名索引恢复原结果，C0 引用保持恒定 | 读取协议夹具通过，完整实际进程组合待验 |
| Package upgrade 事务 | actual fixture v14：39.94 秒通过真实 stage/stop/promote、候选启动失败、精确原映像回滚及重启。promotion receipt 摘要补齐 helper wire 中显式 null 的候选字段，精确 preimage 检查保留；日志 `/tmp/oasis7-lifecycle-package-actual-fixture-20261014-v14.log`，fixture v54。私有测试映像 strip 后 347797192 bytes，共享编译产物未修改 | 失败启动回滚通过；不同 SHA shipping 完整升级另验，真实不同版本矩阵为 TODO |
| shipping 生命周期脚本 | `local-node-lifecycle.test.sh` actual v8 通过真实启动/恢复、未证明持久 World 在 stage 阶段拒绝、错误 pin 拒绝及纯计划候选 smoke；runtime production v23 / fixture v68，日志 `/tmp/oasis7-lifecycle-shipping-script-20261014-v8.log`。不具有精确 execution record/CAS 根的 compatibility_observer 夹具未被改造成正向升级证据 | 脚本上述范围通过；qualified World 真正不同 SHA 的成功升级为 TODO |
| 旧目录迁移 | `v16`：12 项迁移单元测试通过；实际脚本 `v2` 使用 128 MiB reward 数据并 SIGKILL 迁移进程后原请求续做通过，核对身份、原 head、历史、CAS、replication、pending/outbox。验证读取原始 persisted World，保留可选 simulator mirror 语义 | 通过该构建组合，最新完整源码待验 |
| 健康闭包读回复核 | canonical `healthy_copy_` 两项、`registered_` 三项通过；真实 walker 拒绝同一 Index 对象第二条路径上的非法深度，权限测试拒绝可被其他用户写入的目录/CAS，同时允许旧 0755/0644 读取 | 定向回归通过，完整安装进程待验 |
| 普通节点兼容生命周期 | `local-node-lifecycle.test.sh`：真实 start/restart、原动作重试、query-only 账本恢复通过；Node canonical clippy 通过，4.19 秒 | 通过上述验证 |
| Effect 身份与派发 | 三项 birth/receipt/legacy 字节回归通过，0.11 秒；扩大 effect 回归 95 项通过，2.64 秒；派发回归 22 项通过，0.83 秒；真实丢回复/SIGKILL/重开 `26533` 通过，22.41 秒；无合同 `47864` 通过，22.89 秒；丢账本 `92718` 通过，24.02 秒 | 通过上述场景 |
| Protected signer handoff | `57074`：40.28 秒实际运行通过，状态到达 `successor_qualified_readback_verified` / `succeeded` | 该 handoff 场景通过 |

之前报告的 ancestry/minimum 错误、初始 artifact CAS 缺失与 handoff RPC deadline/current-checkpoint 回滚均为旧轮结果；ancestry bridge、persistent raw-CAS 和精确签名切面缓存修复已有上述定向或实际证据。预热前移和外部派发接入已通过当前 formal 测试组；较早的 C03 八阶段尚未在这次接入后完整重跑。这些结果不外推为最新源码的长期历史、所有 handoff 交错或完整部署计划已通过。

长历史测试已将原请求的模块调用断言绑定到该请求的 exact Qualified Frozen/candidate Journal，避免把并发空 Tick 的 Wasmtime 调用错误归因于管理请求。启动优化从独立的受保护 P/R probe 获得一致 head 提示，正式恢复客户端的原最低锚保持不变；缓存安装仍要求从 C0 完整验证前缀及精确上下文，不扩大超时或缓存。

外部 effect 新出生身份包含 era，receipt 绑定完整 World、原 intent 与 payload；旧 `None` 的序列化和签名保持原字节。派发仅接受 Qualified outbox 中明确的 target 与 receiver contract 摘要，本机账本不授予资格。生产自动派发要求预授权发送者、幂等原身份 POST 和完整签名查询；本地账本丢失或节点更换不得改变永久 effect digest。外部回执独立于 World receipt，未确认结果不会直接改 committed World。真实回复丢失、SIGKILL、重开已通过：接收方执行与 POST 各一次，重开查回原签名回执，原 CommitRef 不变。无幂等合同场景从首次启动到重开均无 POST。丢账本场景用父进程内的生产 transport 与预授权 sender92 查询原 Qualified outbox，随后真实 runtime 仍以 sender90 重开；它不代表正式 Node 换密钥或授权集迁移已验收。

这仍是本机代码验证，未完成部署计划。健康集合管理 CLI 已有上述 v22 实际通过证据；其余最新源码长期历史、完整 full-sync 组合，以及 handoff 的剩余故障/交错负例仍为后续 TODO，单次 handoff 成功不代表这些场景通过。没有真实故障域、生产部署或公开开放的验收结论。

全量同步仍有整次树/传输和本地历史容量界限，启动仍从 C0 验证完整保存历史；本轮回收不能被解释为任意大小世界或无限历史同步已完成。World CAS 的路径预检不外推为所有存储路径都有 syscall 级 O_NOFOLLOW。Node Package 已采用目录/文件描述符的 no-follow 读取、具名项身份重验和实际加载映像核验；跨重试工件身份仍绑定路径与 SHA，允许字节完全相同的 inode 替换。

有限 Tick 热尾上限为 32 条，每次候选最多归档 64 条旧证书；累计数使用 u64，不以扩大累计常量替代归档。恢复先绑定原已确认 Snapshot 和 CAS reader，再校验完整原前缀及冷热边界。缓存只保存内容寻址字节，不能替代 root、授权、签名或双端资格校验。

本轮集成的本机验收脚本为 `scripts/formal-live-local-tests.sh`，覆盖真实端点子进程及串行 Wasmtime 组合，且已纳入 required tier 调用路径。以上结果不代表脚本覆盖的所有流程均已在同一版本完整通过；长期历史与待验场景仍见上表及本节末尾状态。

## 后续 TODO 与验收边界（2026-10-14）

用户授权小遗留在实施文档跟踪；这些项不表示当前已通过，也不解除切换、回滚与恢复正确性及 required CI 的合入要求。

- TODO：按真实提交大小和状态规模测量写入至 Qualified 发布的延迟，再确定生产 SLO。长期历史夹具的新请求采用绝对 120 秒等待预算，20/40/60/80/100 秒输出原请求及发布/Node pending 阶段诊断；任务等待与 graceful stop 均为绝对 120 秒夹具预算，不是生产 SLO，SDK、RPC 和生产期限不变。stop 仅重试明确 Pending，保留同一 runtime owner 与原停止工作线程，成功后才重开；其他错误立即失败。完整 70 次精确发布与冷历史断言仍须通过，120 秒后仍缺结果或停止未完成不能按性能 TODO 豁免。
- TODO：在明确获准的真实部署环境验证 P/R 物理故障域、受保护 signer/HSM、服务管理器与部署回退。验收为原 C0/身份不变、故障后原请求恰好一次恢复及独立资格读回；本机协议模拟不代签。
- TODO：在具有精确已提交 execution record/CAS 根的 qualified World 上验证真正不同 SHA 的正向 shipping 升级。当前 compatibility_observer 夹具只具备 materialized World 而没有执行资格，探针必须在停止原节点前拒绝；不同 SHA 的计划或拒绝、同映像真实失败启动回滚均不代签成功升级。不得通过放宽未证明 World 的兼容性检查完成该项。
- TODO：补充 macOS 以外平台的 executable-image/no-follow 路径与真实不同版本产物升级矩阵。验收为合法 prior/candidate 通过、错误加载映像及不兼容持久格式在停止前拒绝；shell stub 不代签真实升级。
- TODO：验证首启 bootstrap 已写入 seed World、但尚未提交 execution record/CAS 时的中断恢复。当前兼容性探针明确拒绝该未证明切面；本轮失败启动回滚测试限于 bootstrap 写入前，不代签该后续场景。验收须绑定获批 seed 与冻结原配置，不能放宽未知 World 的兼容性拒绝。
- TODO：独立测试工作区的 rustls 补丁升级与兼容矩阵验证。根生产 `Cargo.lock` 已由 0.23.45 升级到 0.23.46，`cargo deny check advisories` 通过；`tests/libp2p-compat/{legacy,candidate}/Cargo.lock` 仍为 0.23.45，这些 libp2p fixture 不在根 workspace advisory 检查范围内；`scenario_test_runner` 已迁入根 workspace，使用根锁文件。本轮未更新这些历史/独立 fixture，不能宣称所有仓库依赖图已清零。
- TODO（P2）：补齐 `--formal-live-config` 与 list CLI 参数的互斥检查。当前 scalar 参数冲突会拒绝，但 `--replication-network-peer` / `--node-validator` 等 list 参数不进入 seen，可能被接受后忽略；正式运行仍只消费固定配置，不授予额外权限。验收应明确拒绝这些混用参数，并覆盖 list 回归。
- TODO（P2）：将 inventory + tier manifest 缺 registry 的错误提前到参数校验；目前完整 authority binder 仍拒绝该组合，但拒绝发生在 runtime lock/keypair 准备之后。inventory 单独输入已补齐早期拒绝，不能以该项 TODO 绕过 authority binding。
- TODO：将本机诊断日志整理为可复用的操作故障排查步骤，保留阶段及原 operation 身份，避免仅凭超时判断故障原因。
- TODO（P2）：owner 启动当前分别为首次不可变身份及 runtime 恢复重放同一固定 C0 历史。可在保持 opaque 资格、writer lock 和错误拒绝的条件下复用验证结果，减少重复读取；本轮不以未验证 marker 替代完整重放。
- TODO：补齐 main 新增的 `GameGoalPublish` / `GameExecutorControl` 冷查询索引及保护记录的整体容量规划。当前两类记录保留在热状态，不删除前驱；原 32 条及字节上限保持，超限明确拒绝且保留世界状态。由于 planner 只计算可归档后缀，可能保守拒绝；验收应证明合法长链完整查询、热容量有界与原控制身份不变，不能靠扩大上限或删除控制记录解决。
- TODO（ST-02/ST-04，blocked）：验证正式玩家 Skill 支持的私有 Formal Game API 与 attached executor binding 后，组织独立 Agent 对 retained task/nonce/control 的目标、问答、断连回读和恢复自主试玩。现有普通合同测试不代签；当前 delivered Skill 指向 LocalProvider/LetAI，不能证明本轮 formal 消费者。本次仍交付最初定义的本机持久化与管理基础设施，不开放完整 formal 玩法。

## 合入收口评审（2026-10-14）

- CI 入口由独立 `merge_scope_review` 审阅；新增 owner fault 测试漏选的 P1 已补入 `healthy_durable_set_owner_ --ignored`，复审通过。入口 SHA-256：`56b892772b773d33780b9cd87ac34bc89941489d536d4bffe04569882f2c5081`。
- `formal_business` 独立复审 `wasm_review` 的 healthy closure 修复：完整纳入 normal/retained delta 的签名中间写入，hash、深度、预算和权限校验保留，无 P0/P1。copy/helper SHA-256：`e5ae0dc9b2841f2ecdc871cb8e125ad2358e25ee046aabafd0560c0737fde7b4`；回归 SHA-256：`e5fbe8eaa40f7cdcf72fa1cf407f9d22cf2cbbb3de732d29ed5a2ab857686629`。wrapper 定向八项通过，3.64 秒；不代签实际激活。
- `wasm_review` 独立复审 dev/test 的 `sha2@0.11.0` 单包优化：没有修改 release/packaging、算法、features、锁文件或映像校验；无 P0/P1。Cargo.toml SHA-256：`18001c0ac9ad770c82a0c93ab7f424e1ce1449b2341fb0a81bbb1be810535caf`。实际回滚耗时仍待验证。

## 本轮独立源码评审（2026-10-13）

2026-10-14 `wasm_review` 独立复核最后的 archive preseed 与 CLI 拒绝规则，无 P0/P1。preseed 逐逻辑 ID 使用生产 reader 验证，按源 dev/ino 只复制一次物理 pack，硬链接只在独立目标内部创建；logical/physical 各 512 MiB 上限，不复制 authority/head/checkpoint 元数据。`raw_seed.rs` SHA256 为 `70d28da91b746986910e6d25bb04fe644ed97e32bc725f382d14a1185819cb9b`，调用者为 `f0dc132b03ab937b294fc2dd48aa01e1ba8e8a95c59c806269d37f6ff412334f`。CLI inventory-only 提前走已有 typed engineering 拒绝，带 manifest 路径仍执行完整 authority binding；`cli.rs` 为 `9972038425ff5956184fdc0efc577903c7a9ad557a920db8b0f962238cd5d32b`。根 rustls 0.23.46 版本/校验和变更由 `node_delivery` 和 `wasm_review` 复核；本机 advisory 及 a349 CI rust-baseline 通过，最终 source HEAD CI 另验。actual CLI v20 已证明双端安装/激活读回分别约 1.33/1.28 秒，但后续 moving-current 读取竞争仍失败，不计为整条主链通过。

2026-10-15 `wasm_review` 独立复核首个 Normal 的历史读回修复，无 P0/P1。仅新 Normal 验收改用 opaque activation 绑定的 H+1 历史证据：P 定位完整已认证记录，R 读取同一精确 record ID；验证 successor、当前 protected durable-set 摘要、完整 H 父记录/finalizer、Qualified Normal Execution、材料及永久索引。既有 current-head 恢复仍严格要求同一 checkpoint，历史读取允许同一受保护身份下合法前进，不创建当前信任锚。canonical 实际 TCP 定向测试通过 1 项，4.11 秒，覆盖 current 已到 4 时固定读取 3、错误 ID/位置拒绝；日志 `/tmp/oasis7-lifecycle-first-normal-historical-20261015-v1.log`。同次复审确认 observer 测试只在原 2 秒期限内等待真实保存追上已捕获高度，observer 保持阻塞，生产行为未修改；最终 required CI 另验。

同日 `wasm_review` 独立复核 owner writer-lock 失败身份修复，无 P0/P1。首次回执前复用 `startup_anchor` 的固定 C0 完整 proof/material 重放，只接受由重放推导的 opaque activation successor；验证失败发生在创建 attempt receipt 之前。后续 runtime open 失败时仍以此已验证 authority 绑定失败回执；成功 Ready 仍来自独立恢复后的 runtime。无锁阶段只读，不写 World/writer 状态；immutable identity、固定 trust/config 和 writer lock 校验保持。`formal_live_history.rs` 复审 SHA 前缀 `0472c786`，owner 为 `363b286d`；重复重放成本列为 P2 TODO，不影响资格边界。

后续启动代复审确认：启动器捕获新 Child 的出生身份，每次只解释与新 PID、出生身份及完整 instance 一致的持久回执；旧 attempt 回执仍留存，但不能作为新进程 Ready/failed。新进程退出却没有匹配回执时返回 UNKNOWN 及诊断，原启动期限未扩大。`wasm_review` 未发现该生产修改的 P0/P1；owner SHA 前缀 `8d762b8d`、attempt 为 `11f249dc`。canonical fixture 的 PID/birth 拒绝定向测试通过 1 项；实际 writer-lock v25 通过原失败回执与 attempt 2 的完整身份核对。

最终源码收口后的 canonical required clippy v17 通过，9.22 秒（`-p oasis7 --tests --features test_tier_required -- -D warnings -D clippy::correctness -D clippy::suspicious`）；distfs all-targets strict clippy v17 通过，6.99 秒。日志分别为 `/tmp/oasis7-lifecycle-required-clippy-20261015-v17.log` 和 `/tmp/oasis7-lifecycle-distfs-strict-clippy-20261015-v17.log`。v25 实际进程之后的生产差异仅为相同启动代核对移入 attempt 模块、等价 let-chain 及窄范围 lint 注解；fixture 增加新 worker 在回执前退出时立即拒绝，未扩大期限或改变资格。实际 v25 回执证明没有以隐藏该退出来完成验收；最终 HEAD 的远程 required CI 仍须独立通过。

最终 fixture v85 canonical 编译及工程 authority 拒绝、PID/birth 两项直接回归通过；日志 `/tmp/oasis7-lifecycle-final-fixture-focused-20261015-v85.log`。observer 保存竞态修复的 canonical `cargo test -p oasis7_node --lib blocking_consensus_progress_observer_is_bounded_coalescing_and_never_stalls_tick` 通过 1 项，0.89 秒，日志 `/tmp/oasis7-lifecycle-node-observer-canonical-20261015-v1.log`。最终 doc-governance、Rust 文件大小及 diff 检查均通过；远程最终 HEAD CI 是独立合入门禁。

`storage` 对非本人编写的历史 Published 缓存接线进行只读安全/兼容审查：精确 opaque cut、完整 Qualified proof、当前权限与双端 Query 保持；未发现阻断。随后增加的代码仅为测试计时，`storage` 核对的 SHA256 为 `34394860f6d9867a88516eba8834f967408eb9b54d82737341df9ec3591c9190`。编译时补入缺失的 `FormalRecordKindV1` 导入，当前 SHA256 为 `7166ca63d7148cb28a50db13b1a4dec98e1b1b99736ec49979dd1dae3d2fa09e`。损坏本地文件、聚合上限与双 Query 强制路径的专门运行证据仍待补齐。

`wasm_review` 对非本人编写的索引迭代、Recovery 历史资格及历史材料恢复进行只读审查，发现历史增量缺少原父根重建验证；补强后复审未发现 P0/P1 阻断。父根来自旧记录已承诺 parent ID 的精确记录体，不从当前 head 取根，不创建父记录资格。定向正例与错误 base、缺节点和换父根负例通过；评审不代替实际组合验收。

| 复审源文件 | SHA256 |
| --- | --- |
| `business_index.rs` | `02a728b0911abe8f1cfc042bae027e1b9c6880988084ccd94b713ef446742ca2` |
| `business_history_committed.rs` | `0cc561e771b8f0c1089316b2a44bc7340a3b0f8d98a7d330f666fe278db4ce13` |
| `business_material.rs` | `6b22951d4e697bfc859e9c0edec398c6a911ed75c0e9c1308301d4a2db11b7ca` |
| `endpoint_committed_material.rs` | `8a861e9a840afcd69b2ec174305a1b45cbb80ce91c6ef11a82c1bb8683d9e0c3` |
| `client_committed_history.rs` | `16e524aba99a3a1765560b64a481b05f72adc30d29bff8cf91faa980d1eb9ee8` |

`storage` 独立复核 CI 接线：required tier 保留原检查并追加正式验收脚本，真实 public ignored parents 显式执行，内部 child 不混入；端点与 runtime executable 从 cargo JSON 精确解析，串行构建/运行。范围规划 17 项测试通过。脚本 SHA256 `845ff46d2d6bfb0ed5e18415d6bc88b90a5b163e09d039877628766e68423186`，`ci-tests.sh` 为 `275d744a550a5f9d4830896e97588a8560ca129d0e53a18bb53a26e0947ea369`，scope 配置为 `0c5b1ef6afdade1ce1daaa14d44674850957f317464de055d6f748de07843359`。完整脚本和远程 CI 没有本轮通过结论。端点库 all-targets canonical clippy 已通过。

## 端点服务与旧工程配置

`oasis7_durability_endpoint` 同时提供旧 `replicated_durability_prerequisite` 工程协议和本轮 formal native profile 的独立持钥进程。正式 Node 通过经过校验的外部 C0、manifest v2 和完整初始物料运行；恢复必须具有原恢复权限和受保护端点证据。端点不生成世界、密钥或信任材料。正式配置及运行入口见 [本机节点生命周期 runbook](local-node-lifecycle.runbook.md)。下面保留的配置片段仅说明旧工程协议，不能用作正式信任锚或正式开放证据。

构建、运行入口：

```sh
env -u RUSTC_WRAPPER cargo build -p oasis7_distfs --bin oasis7_durability_endpoint
./target/debug/oasis7_durability_endpoint --config ./primary.endpoint.json
```

配置结构如下；所有信任值和 minimum_head 必须来自该工程协议的原始可信材料。示例中的占位符不是可用身份。程序不生成密钥、世界或信任材料。

```json
{
  "schema_version": 1,
  "trust": {
    "world_id": "original-world",
    "chain_id": "original-chain",
    "genesis_digest": "<original 64-character lowercase hex digest>",
    "authority_epoch": 4,
    "writer_key": "<original writer public key>",
    "primary_id": "primary",
    "primary_key": "<original primary public key>",
    "replica_id": "replica",
    "replica_key": "<original replica public key>"
  },
  "role": "Primary",
  "data_dir": "./primary-data",
  "signing_key_file": "./primary.key",
  "minimum_head": {
    "position": 0,
    "decision_hash": "<verified genesis anchor digest>",
    "qualified": false
  },
  "listen": "127.0.0.1:9101",
  "timeout_ms": 2000
}
```

Replica 使用自己的 role、目录、公钥和私钥。每个服务只读取自己的 endpoint key；RPC 客户端只持 writer key。端点拒绝 signer、角色、世界或 epoch 不匹配。相对路径以配置文件目录解析。私钥须为当前用户所有的私有普通文件；已有数据目录须为私有目录。新端点文件与目录分别以 0600/0700 创建。

服务和客户端均只接受 loopback 地址。跨主机使用必须由受认证、加密的隧道终止在本机 loopback；签名 RPC 本身不提供内容保密。不能直接绑定公网地址或把完整私有对象闭包开放为公共节点读取 API。

`transport::RemoteEndpoint` 与 `ReplicatedCoordinator<RemoteEndpoint>` 复用本地端点的 prepare、决定、proof 保存和原请求查询流程。每个 RPC 绑定原始信任、精确 endpoint role/identity、挑战、minimum anchor 和操作内容。响应由该端点签名并绑定原请求摘要。超时或响应校验失败不能成为 qualified 结果。

同一 slot 已准备候选后，另一个候选在端点持久入口被拒绝。原候选可重试。查询 qualified 必须同时验证原 request identity、候选签名和完整双端证据；响应封套签名不能替代内部证据。

guarded runtime 配置 v2 可填写 `remote_endpoints: {primary, replica, timeout_ms}`，并省略本地 primary/replica 的目录与 key 字段；混用远端和本地端点字段被拒绝。实际执行 driver 复用远端协议，协调进程不读端点私钥。它仍是明确的工程 profile，不能据此宣布正式游戏 runtime 已完成跨故障域提交。

v1 保持原完整配置摘要，已持久 continuation 不重新解释。v2 的执行绑定覆盖 node 身份、调度间隔、原激活与 release policy；目录、监听地址和私钥文件位置不参与绑定，实际公钥仍按固定 trust 验证。调度或 signer 变化不能借搬路径绕过。

v2 执行前将原 continuation、动作、pre-state 完整闭包和执行绑定保存到两端。writer 授权、双端冻结回执和闭合 FrozenCertificate 必须都持久保存，之后才运行实际 Wasmtime。候选证明 v2 绑定原冻结摘要，不能替换动作或 predecessor。已保存完整候选优先原样续交；仅冻结完成且无候选时从原输入重放，保留中断目录，不重新调用模型或选择动作。v1 缺候选的旧恢复拒绝仍保留。

冻结执行绑定还包含显式 Wasmtime policy（fuel、内存、输出与存储参数）及当前执行工件摘要。确定性重放不以 wall-clock 超时产生业务结果，仍实施有限 fuel/内存/输出预算。不同工件不能重放未完成冻结；完整候选则无需重新执行即可沿原证明续交。工件摘要按 `current_exe()` 所指文件读取，部署必须保留不可变工件，运行期间不得原地改写或替换该路径；这不是远程证明，也不能单独证明已映射代码与替换后的路径一致。正式升级和恢复仍须完成授权及工件保留合同。

## 存储与恢复边界

端点新增分段、校验链绑定的不可变 delta 帧。落盘顺序为私有临时文件、写入、文件同步、原子 rename、目录同步。写入不确定后保留 unknown/poisoned，须重新打开并查询原请求。

原 snapshot 保持可读，迁移后作为不可变基底。超过旧 64 条/16 MiB 的历史可恢复、查询原请求并继续 prepare；原请求不同内容继续拒绝。归档不裁剪，原请求与对象证据一直保留。

restore 允许相同证据历史的已完成重试和中断续做。目标已有历史必须是输入的完全相同 qualified 前缀；prepared 尾部、冲突、额外历史或更短输入均拒绝，不覆盖已有记录。

新增 v2 存储以端点签名的热 checkpoint 绑定当前 head、候选/frozen 引用、永久请求 radix trie 根和 revision。对象为不可变、按摘要寻址的有界 archive；历史输入按需读取。head 的签名归档节点绑定 binary-lifting 前序引用，minimum 与原请求的历史归属通过有界路径核对。未被 checkpoint 引用的孤儿文件不能成为权威或伪造去重。

旧分段 journal 的 segment seal 仅为边界 checksum；其原始文件保留，迁移为 v2 后不作为日常全历史 replay 路径。请求索引与归档永久保留，本轮不执行裁剪。`export_evidence` 显式物化完整历史，普通热状态仅保留一个候选/当前记录与冻结输入。历史 reader 的单次验证和缓存仍有有界预算；长期历史场景已在表中指定构建组合完成超过 64 条/16 MiB 的实际验证。全量同步仍受公开传输与本地容量预算约束，不能据此宣称无限历史或任意大小世界。

## 正式合同层与准入边界

`controlled_authority::formal` 使用独立签名域与类型。CanonicalRecord 的身份只覆盖逻辑记录体；Normal、CompletePrepared 和 ReplayFrozen 的签名/证明封套不会改写同一记录身份。Decision 只含双端 final receipt，不能提升为 Qualified：双端都保存完整 Decision 并签出 ProofComplete 后，Qualified 证明才成立，随后端点发布该 head。完整证明保存期间的单端状态不得显示 committed。

输入 Frozen 的 governing manifest 必须等于父记录的输出 governing manifest，执行策略也必须沿合法历史保持一致。每次输入闭包的 ExecutionManifest 摘要与 Cognition governing manifest 是两个不同承诺。实际重放 policy 明确启用 NaN canonicalization 与 deterministic relaxed SIMD，并绑定不可变执行工件；已有 WASM 执行器测试 31 项通过、1 项性能探针忽略。本机实际 formal 执行闭包组合仍在验证中，不作为正式准入收据。

普通读取使用外部指定 InitialActivationTrust 与 independently pinned read checkpoint，再验证 Qualified 历史与 audience/reader-key 绑定的投影 Merkle 证明。权限代数在投影叶内绑定，不能用物理 authority epoch 替代 World 的 capability generation。响应提供的服务签名或 issuer key 不能自行建立初始信任。原 engineering scope 的普通客户端拒绝规则保留；正式 Submit/Lookup/Changes 缺少各自证明合同的路径须显式拒绝。

fencing 的生产门槛包括实际持久实例、单调 floor 和受保护的签名表面。仅远程 oracle 声明强 capability，但端点仍持有可导出的本机 SigningKey，无法阻止旧进程离开 guard 使用复制的 key。此路径必须返回 `FENCING_UNPROVEN`；文件私有权限、目录 fingerprint、签名 enum 或本机测试均不能替代 nonexportable key custody。当前原生端点已接入受保护 signer 接口，只有其签名 custody 与单调 checkpoint 验证通过后才能进入正式 ready；本机 oracle 的协议测试不代替真实防回滚部署证据。issuer 的唯一接续同样要求独立、不可回滚且保留原证书的 witness；本机模拟 witness 只验证协议状态机。

## CLI 与成员查询

flat CLI 的 85 项 OptionSpec 目录统一 canonical/alias、类型、冲突规则、默认值与 help；assignment dispatch 与目录做精确覆盖验证。CLI 拒绝字符串缺值吞掉下一开关、重复标量、正反开关冲突和重复身份绑定。peer 等可重复列表继续可用。以 `--` 开头的字面值采用 `--node-id -- --literal`。

fetch requester 正名为 `--replication-fetch-requester-public-key`；wrapper/observer 配置使用 `REPLICATION_FETCH_REQUESTERS_CSV`。旧拼写暂为有警告的薄适配，冲突的新旧 env 被拒绝。权限仍仅为 fetch；该配置不能授予 writer 或 validator 身份。旧名退役边界见现行 bootstrap runbook。

`WorldGovernanceMembershipView::read` 从单一已验证 committed World 读取原候选、既有 resolver 选择、治理 epoch 和有效 registry。调用者先校验证明并将 CommitRef 的 CAS state_root_ref 解析到真实 state root，adapter 校验 root、world/runtime binding、manifest、finality 与 generation。解析错误直接返回，缺 registry 不回退到 node config。

`RuntimeMembershipApplicationView::startup_projection` 只报告配置角色和启动 registry 来源。当前没有安全 runtime install acknowledgement，`supports_safe_runtime_update=false`，`applied_consensus_membership=None`。治理有效不能显示为 BFT 已采用。现已提供 `validator list/status/wait/watch` 诊断入口，使用现有 `OASIS7_WORLD_SERVICE_ENDPOINT`、`PUBLIC_KEY`、`WORLD_ID`、`GENESIS_DIGEST`、`SCOPE`（必须 public）和 `READ_PRIVATE_KEY` 配置，所有字段采用 `OASIS7_WORLD_SERVICE_` 前缀。服务支持已独立证明的正式 public projection；guarded pinned history 只作为显式工程诊断，必须使用 `--allow-controlled-prerequisite`，默认客户端仍拒绝工程 profile。未知 capability/profile 明确拒绝，不读取本地 registry 冒充正式视图。

watch 为有界、只在 commit 变化时输出的 snapshot polling。`--cursor` 是 minimum CommitRef，不保证重放所有中间转换；`first_observed_in_this_run` 与实际首次生效位置区分，后者未证明时为 null。公开 candidate 只包含白名单字段，不返回 operator owner 或自由文本 revocation reason；原 WorldService projection 的私有字段过滤保留。旧 guarded 客户端遇到新的 capability 枚举可能 fail closed，本轮未交付版本协商。独立 node init/start/status/stop 已接通 compatibility observer、formal public observer 和只读 full-state sync；具体身份、grant 与 ready 边界见 [本机节点生命周期 runbook](local-node-lifecycle.runbook.md)。

每次成员读取使用 OS 随机生成的 32-byte observation nonce；调用者与服务签名都绑定包含该 nonce 的原始查询。旧响应不能用于新的查询。没有有效 registry 时，治理有效性保持 unknown/null，不解释为确定的未生效。

保存的正式 cursor 最多包含 32 帧、16 MiB。预算耗尽时，watch 可从当前配置的外部 C0 独立重读，完整验证后证明新历史包含原客户端的确切 head/authority 与已认证 minimum，并以缓存 compare-and-swap 原子安装新的 journal；失败保留旧缓存及文件。输出与写入文件必须对应同一提交。当前重基依赖单次 inline 历史，C0 到 head 超过 128 步或缺历史仍返回 `HistoryUnavailable`；调用方提供历史页的验证 API 不等于自动网络分页，不能宣称任意长度历史的 watch 重启续读已完成。33 个签名提交的库级重基测试及短历史断线重启的实际 CLI 子进程已通过；超过 inline 128 步的自动网络续页仍未交付。

## 剩余整体验收

| 范围 | 验收与后续边界 |
| --- | --- |
| 本次本机代码交付 | 健康 durable set 的双安装激活、跨 epoch Normal 读取、原 operation 故障续做；Package stage/promote 与真实失败启动回滚；最新 source HEAD 的 required CI。真实主链失败仍阻断合入。 |
| 后续 TODO | C01—C20、O01—O19、U01—U10 在真实部署环境中的剩余故障交错与平台组合，以及已明确 blocked 的正式 Skill/executor 自主试玩。表中旧场景保留实际版本范围，不外推为最新完整部署通过。 |

2026-10-14 用户授权快速推进合入，小遗留在实施文档标记 TODO。本次以关联的文档 PR 与代码 PR 交付本机代码及管理基础设施（代码候选 #4683）；满足真实主链、独立评审和 required CI 后推进合入。合入不表示整套部署方案、生产故障域或完整 formal 玩法验收已完成。未实现的 BFT signer/成员写动作继续明确拒绝，不能以本地配置冒充生效。

2026-10-14 独立试玩前置预检读取了候选正式玩家 Skill `site/skills/oasis7.md`，SHA-256 为 `c103ef66284e18d23cfc9b6d39ad4bcad9c922d7e18eeb5e93762d12caa3dc92`。官方入口 `./scripts/run-local-letai-game-test.sh --local-world-playtest` 使用私有本机 LocalProvider 世界，不连接 formal/public testnet，但需要真实 LetAI 凭据与付费推理，公开说明含默认自动充值；本轮没有新增付费预算授权，因此未启动该入口。预检子任务未读取实现，但环境仍能访问源码，隔离级别为 `instruction_only/source_visible`。此结果仅是前置预检，不能代签自主试玩；正式网络玩家 Skill 及真实执行 profile 仍未在本轮证明。

## 本机验证命令

```sh
env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib --bin oasis7_durability_endpoint
env -u RUSTC_WRAPPER cargo test -p oasis7 --lib governance_membership --no-default-features --features test_tier_required
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --features node-libp2p,test_tier_required parse_options
env -u RUSTC_WRAPPER cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --features node-libp2p,test_tier_required controlled_live -- --nocapture
env -u RUSTC_WRAPPER cargo test -p oasis7_wasm_executor --features wasmtime
./scripts/p2p-triad-node-start.test.sh
./scripts/p2p-public-testnet-local-observer-sync.test.sh
./scripts/p2p-public-testnet-local-observer-runtime-preflight.test.sh
./scripts/p2p-public-testnet-observer-triad-rollout.test.sh
```

distfs CI 原 library 测试与 clippy 保留，并包含 endpoint binary。协议故障注入覆盖实际文件同步/rename 边界；多进程测试覆盖独立端点、原身份查询、退出/重启和断连。SIGKILL/进程退出不能证明磁盘断电或真实跨故障域隔离。完整正式游戏路径后续仍须按开发规范执行适用的实际自主试玩与环境验收。

2026-10-12 本机记录：最终端点 canonical 命令通过（library 109 passed / 4 ignored，binary 2 passed）；实际 runtime/Wasmtime 组合测试通过（7 passed / 1 ignored，ignored 为由测试启动的 SIGKILL helper）；WASM executor 30 passed / 1 ignored。此前 runtime 组合测试发现旧 schema-v2 拒绝断言失效及两次轮询等待超时；断言已按新增 v2 支持修正，协调器补充最后 reconciliation error 诊断，避免重负载并行运行后完整组合通过。等待超时的根因尚未独立证实，不能把单次通过解释为已消除所有时序风险。
