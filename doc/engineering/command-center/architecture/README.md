# 指挥面板架构索引

这里集中列出做项目决策时需要记住的架构边界、实现入口和部署核验缺口。它是主责文档与交付事实的派生导航；具体约束、变更理由和验收仍在链接的原文中维护。

- 代码与文档核对基线：`main@598b308a23f3f0189843618d14abb1aa31c0d2de`，摘要核对日期为 2026-10-11。
- 2026-10-11 摘要轮次没有实际环境采样、部署、重启、恢复演练或模型调用；该轮所引 PR 验证不是重新运行的结果。各主题的局部核对另注日期与范围，不刷新其他主题或环境的核验时点。
- 历史环境表 Last Verified 为 **2026-07-06**，节点表为 **2026-06-23**。读取摘要的日期不能代替部署、健康或可用性的核验日期。
- 项目进度与待处理事项见[当前状态](../README.md#cc-status-attention)；本页的更新方法见[架构索引维护流程](../command-center.manual.md#cc-maintenance-architecture)。

独立 ECS 技术预览的环境准备、构建尝试和部署验收由 [交付任务 #4671](https://github.com/eng-cc/oasis7/issues/4671) 承接。本页保留架构入口；该任务的技术预览不代表正式世界开放或原测试网恢复。

## 从问题进入

Cargo 边界局部增量（2026-10-12）：[#4626](https://github.com/eng-cc/oasis7/pull/4626) / `133d70e45b71bd062032a734d68be1e290692b2c` 将 native 生产 cognition 状态转换转接至 Harness，World readback 与 signed-service 校验仍由 Runtime 适配器承担，旧 DTO/checkpoint 兼容保留；不新增 Harness→Oasis 依赖。支持包 profile 隔离尚在分支验证，直接 Oasis wasm32 目标及部署不因本次局部摘要变为已验证。测试入口见[测试手册](../../../../testing-manual.md)。

核心执行边界局部核对（2026-10-18，仅 [#4853](https://github.com/nil-studio/oasis7/issues/4853)）：目标所有权由 [Runtime 设计](../../../world-runtime/design.md#实现边界与共享值所有权)、[Simulator 设计](../../../world-simulator/design.md)和 [Game Client 边界](../../../world-simulator/llm/agent-game-client-boundary.design.md)承载。Simulator 与权威 Runtime 并列，Service 依赖 Runtime 与合同层；共享值和 wire 合同归 API / Proto，客户端证明消费与操作恢复归 Game Client，旧组合归档和进程组装留宿主。Simulator 阶段已合入；完整 Runtime / Service 及客户端迁移仍是 [#4873](https://github.com/nil-studio/oasis7/pull/4873) 候选，实际合入与验证范围见[项目状态](../README.md#cc-status-core-crate-extraction)。这不增加进程或 transport，也不将 Runtime 的 WASM 值模型视为完整执行支持。

L4 工具链局部文档更新（2026-10-12，核对基线 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056`）：[testing 合同](../../../testing/prd.md#l4-build-driver)明确默认 release/正式 UI、同一实际 Hosted 执行会话、逐次 RunResult 与评审归属；[入口兼容设计](../../../world-simulator/viewer/viewer-web-entry-compatibility.design.md)保留 alias，并将测试 API 限于显式测试产物。[#4438](https://github.com/eng-cc/oasis7/issues/4438) 已以 `not_planned` 关闭，仅保留为历史文档承接来源，不再承担旧 runner/scaffold 的实施或运行验收；此处只更新设计导航，不表示新部署或试玩通过。

- 要开新环境、搬服务或调整节点：[环境与部署](#cc-architecture-deployment)。
- 要决定首次开放、扩成多节点权威或改变恢复承诺：[世界权威与状态](#cc-architecture-authority)。
- 要拆进程、调整执行边界：[服务与执行](#cc-architecture-runtime)。
- 要接入玩家、外部 Runtime 或远程控制：[身份与 Agent](#cc-architecture-agents)。
- 要升级、回退或发布：[发布与运维](#cc-architecture-release)。
- 要调整指挥面板技术路线、数据或执行能力：[指挥客户端](#cc-architecture-command-center)。

## 怎么读状态

| 层面 | 本页表达什么 | 不由它推导什么 |
| --- | --- | --- |
| 设计 | 当前主责文档采用的边界、目标和未决事项 | 文档存在不证明实现完成 |
| 实现 | 固定基线中的代码切片、已合入 PR 及其验证范围 | 测试入口或局部通过不证明完整产品验收 |
| 实际部署 | 指定环境、版本、节点/世界身份和窗口下的运行事实 | 合入、旧清单和本页刷新不证明当前上线或健康 |

无法核实时保留“未核对”及最近来源，不把信息缺失写成正常、失败或没有部署。下面的“需人关注时机”是阅读提示，普通已授权实施仍沿用[开发流程](../../workflow/source-of-truth.md)。

<a id="cc-architecture-deployment"></a>

## 环境与部署

先确认当前承诺和环境，再看机器与服务。项目环境、网络 tier、provider lane、玩家接入模式分别回答不同问题。

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-01 · 公开阶段 | 当前仍为受限可玩技术预览；构建包和局部测试不自动代表正式发布或赛季上线 | 邀请新用户、修改下载入口、发布公告或改变成果保留承诺 | [公开状态](../../../../README.md#项目状态)、[安装与发行](../../../product/player-entry-distribution/prd.md#安装与发行体验边界) |
| ARCH-02 · 环境分层 | local / test / production 与 network tier、Hosted 登录、provider lane 分开；测试与正式数据/凭据隔离 | 新增端点、搬机器、共用存储或凭据、调整环境成本和开放范围 | [三套环境](../../governance/environment-lanes-and-inventory-2026-05-29.md#2-三套环境总览)、[子系统映射](../../governance/environment-lanes-and-inventory-2026-05-29.md#4-子系统映射表) |
| ARCH-03 · 拓扑与角色 | 清单标签、实际进程角色和世界身份分别核对；observer 追到高 head 不等于完整执行恢复或取得写权 | 增删节点、改变 validator/observer、调整故障域或发现节点分歧 | [部署清单](../../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#22-current-operator-inventory)、[采样合同](../../../p2p/node/node-triad-operations-observability.prd.md#inventory-与采样合同) |

**设计状态：** 三项均有当前主责入口。环境分类和角色约束持续有效，表中的机器、服务和就绪结果受其实际证据窗口限制。

**已有实现：**

- ARCH-01：仓库已提供技术预览、构建与验证入口；发行支持范围继续由根 README 和玩家接入产品域拥有。
- ARCH-02：环境表记录了 Hosted 登录及网络/provider lane 的映射和历史部署；内部 production lane 不等于项目已向普通用户正式开放。
- ARCH-03：已有运维采样、部署更新与恢复脚本。runbook 的五节点 inventory 是重新核对前的执行目标，不能代替实时发现。

**实际部署核验：** 本组三项均未取得当日运行窗口。[历史云上清单](../../governance/environment-lanes-and-inventory-2026-05-29.md#7-当前云上清单)只用于定位；当前节点数、版本、实际角色、健康和外部可达性保持未核对。

<a id="cc-architecture-authority"></a>

## 世界权威与状态

持久单权威和 BFT 是同一世界的不同权威阶段。单权威不等于仅靠一台主机或一个存储目录保存所有已确认结果。

<a id="cc-architecture-runtime"></a>

### 服务与执行

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-04 · 单权威到 BFT | 首次开放采用受控单权威持久世界；BFT 并行研发，后续在同世界 H/H+1 交接激活写权 | 决定首次开放、切换 signer/epoch、扩展权威集合或改变保留承诺 | [当前阶段](../../../product/world-infrastructure/prd.md#world-infrastructure-current-stage)、[权威 profiles](../../../p2p/design.md#des-p2p-authority-profiles) |
| ARCH-05 · 服务分工 | P2P 维护合法提交证明，runtime 确定性执行，game/Agent/Viewer 提交意图和消费已提交状态；链进程入口位于独立 `oasis7_chain_runtime`，纯投影证明位于 `oasis7_client_api`，世界执行适配仍归只提供库的 `oasis7`；其余 21 个入口按 bridge/provider/launcher/client/ops/viewer 职责归独立包，共享策略、观测和 credit adapter 归独立库；Launcher 解析归 `oasis7_launcher_config`、Viewer 网络服务归 `oasis7_viewer_runtime`、WASM 获取/构建归 `oasis7_wasm_materializer`，独立 formal read 验证归客户端层；本地 CAS/codec/pin/index 归 `oasis7_cas`，纯 formal 协议与证明归 `oasis7_formal_protocol`，分布式持久化、端点与协调归 `oasis7_distfs`；signer 纯协议归 `oasis7_signer_protocol`、托管与主机操作归 `oasis7_local_signer`，客户端私有持久化归 `oasis7_client_storage`，Web Viewer 不承担完整节点职责（2026-10-17 源码核对；未核对部署） | 拆服务/进程、加网关、改模块归属或引入第二条执行路径 | [执行分层](../../../world-runtime/design.md#3-目标设计结构)、[P2P 边界](../../../p2p/design.md#4-边界与结构)、[高层架构](../../../../README.md#高层架构) |
| ARCH-06 · 写入与持久 | accepted、prepare、committed 分开；正式确认依赖完整记录和决定跨独立故障域持久，未知提交先查原请求 | 改存储、超时重试、写者隔离，或决定故障期间是否继续写入 | [双端持久前置](../../../p2p/design.md#des-p2p-replicated-durability-prerequisite)、[runtime 提交与恢复](../../../world-runtime/design.md#runtime-authority-commit-recovery-target) |
| ARCH-07 · 保留与恢复 | 正式持久世界保留完整合法历史；物理共享、压缩及冷热策略保持原字节、恢复依赖与独立副本。存储 profile 不授予删历史权限，同世界只读恢复不自动恢复写服务 | 调磁盘预算、回收临时材料、改变物理格式或执行迁移与灾备切换 | [完整历史产品要求](../../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)、[无损存储设计](../../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-001)、[重新服务边界](../../../product/world-infrastructure/prd.md#恢复后的重新服务闸门) |

**设计状态：** 本组是当前主责文档声明的路线与目标合同；正式 profile、完整事务和灾备目标不能因为已有前置切片而标为已验收。

2026-10-12 局部设计增量（实现边界核对至 `main@e01e29064517f4bf375767a19a09802335828249`）：[原候选恢复 v1](../../../p2p/design.md#des-p2p-original-candidate-recovery-v1) 要求执行前完整材料持久化、先核对原决定再隔离重放；[正式受控权威 v1](../../../p2p/design.md#des-p2p-formal-controlled-authority-v1) 与 [epoch 接管](../../../p2p/design.md#des-p2p-controlled-epoch-takeover-v1) 集中定义激活、普通消费者和存储/签名隔离。均为待实现目标，现有工程 scope 不因本次文档收敛取得正式权威；实际部署和故障域证据未刷新。

**已有实现：**

- ARCH-04：本地 journal、双端持久协议、受控历史重执行、activation/bootstrap 等前置持续落地；完整正式激活、消费者接线和 BFT 验收尚未闭合。
- ARCH-05：已有有序动作、roots、execution records、checkpoint/replay 及多种 prepare/publish 路径；[当前实现边界](../../../world-runtime/design.md#5-current-implementation-boundary-and-target-gap)仍明确整体能力为 partial。
- ARCH-06：局部双端持久证明不等于正式 CommitRef，若干原子发布切片不等于整个 World 的根事务；同机双目录不能证明独立故障域。
- ARCH-07：已有 execution bridge v3、CAS、sidecar、retention/GC/replay 的实现合同；[本地恢复边界](../../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md#storage-accept-service-boundary)不证明分布式可得性或完整灾备。

ARCH-07 局部实现候选核对（2026-10-18，仅 [PR #4870](https://github.com/nil-studio/oasis7/pull/4870)）：[当前实施与退出条件](../../../world-runtime/runtime/formal-world-storage-efficiency.design.md#current-storage-implementation)涵盖原字节共享压缩、事务物理代、发布后独立闭包读回与 attempt 回收、完整 Snapshot 增量、Merkle v2、任务控制冷链及原生执行工件。新世界直接创建新物理表示；逻辑身份、独立持久条件与重新服务权限不因 codec 改变。候选已取得局部容量与恢复证据，长历史进程门禁仍待收口；准确版本、范围和读取代价由实施 PR 记录，不能外推规模或生产延迟。该候选尚未合入，当前无部署节点，S6 真实双故障域与更大规模验收未执行；其他架构和环境基线不变。

节点生命周期实现（2026-10-17 核对）：[PR #4683](https://github.com/eng-cc/oasis7/pull/4683) 接通原请求持久恢复、健康集合转换与可续做节点生命周期；本机验证范围和平台/部署 TODO 见[实施边界](../../../p2p/blockchain/persistent-world-node-lifecycle.runbook.md)。代码已合入（`da890a9125a435969b82f5a14ab3c9143cc0690e`）；具体历史验证范围以实施 PR 与 runbook 为准，当前部署与独立故障域未核验。

**实际部署核验：** 未验证正式 profile 激活、实际存储同步、故障域隔离、旧写者 fencing、备份可用性或恢复后的写服务。这些结论需要各自候选与环境证据，不能由代码数量汇总为完成率。

正式激活、独立故障域恢复和人工交接的能力边界由对应专业合同持有。[#4363 的旧规划](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)仅作历史来源；该 Issue 已停止独立跟踪，不据此恢复执行义务。

<a id="cc-architecture-agents"></a>

## 身份与 Agent

玩家进入、游戏行动委托、operator 控制和代码开发会话分别成立。相同工具名称不能合并它们的权限与完成证据。

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-08 · 网络与权限 | URL/登录只提供接入；玩家 session、Agent 委托和 operator 权限分开，恢复不能复活旧授权 | 暴露公网 API、接账号/模型端点、改认证或委托、增加远程控制 | [Hosted 进入](../../../product/player-entry-distribution/prd.md#hosted-玩家进入与会话边界)、[会话与授权连续性](../../../product/player-entry-distribution/prd.md#player-session-authorization-continuity)、[headless 鉴权](../../../headless-runtime/design.md#6-鉴权与恢复边界) |
| ARCH-09 · 两类 Runtime | 游戏侧目标是 OpenClaw/Codex 通过官方游戏 Skill 和受授权游戏 API 持续游玩；开发指挥客户端管理代码开发会话 | 声明 Runtime 支持、管理多 Agent、改预算/停止行为或连接游戏 Viewer 与开发看板 | [游戏角色](../../../product/agents-world-simulation/external-agent-runtime-play.prd.md#12-角色与职责)、[首批 Runtime 目标](../../../product/agents-world-simulation/external-agent-runtime-play.prd.md#21-首批-runtime-与支持单位)、[开发会话](../command-center.design.md#1101-可选实验执行实现) |

**设计状态：** 接入、委托和角色边界由各自主责合同拥有。外部 Runtime 的首期功能清单是承诺范围，阶段标签不等于实现状态。

2026-10-12 局部设计增量（同上固定基线）：[首次绑定目标](../../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001) 按账号在同一世界的权威状态处理注册、返回和幂等；[首聊 service 目标](../../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-chat-005) 使用 owner 最小权威投影区分可通过、受阻和未知，并在提交时重验。注册/绑定、starter OC 领取与经济 ClaimAgent 分别成立；现有固定 starter 和不完整余额投影不能证明这些后继目标已实现。

**已有实现：**

- ARCH-08：headless ingress 已定义签名 payload、ed25519、单调 nonce 与恢复防重放；它不自动证明任意新 API 的完整授权和恢复行为。
- ARCH-08 的 [多凭证账号设计](../../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md) 使用固定 RP/origin、事务身份存储、HttpOnly 账号 Cookie 和短期签名世界授权；新注册默认关闭，不改变资产或 operator 授权。代码验证见 [#4669](https://github.com/eng-cc/oasis7/pull/4669)，部署与环境验收进度见 [#4671](https://github.com/eng-cc/oasis7/issues/4671)，验收方法见 [手册](../../../testing/manual/hosted-passkey-account.manual.md)。
- ARCH-08 的新预览世界初始化按 [合同](../../../p2p/design.md#des-p2p-fresh-preview-bootstrap) 固定真实初始材料、CAS/提交证明和节点派生服务 signer，再为 Viewer 配置独立 public-read 身份。操作见 [手册](../../../testing/manual/fresh-world-preview-bootstrap.manual.md)；这些材料不授予 Agent 写权限或正式 activation/finality。
- ARCH-09：[PR #4562](https://github.com/eng-cc/oasis7/pull/4562)交付 Game API 的发现合同和已实现签名读取；范围仍是显式开启的 loopback bootstrap。
- [PR #4565](https://github.com/eng-cc/oasis7/pull/4565)增加持久、owner-signed task control；初态 suspended、没有 executor。配对、私有目标、完整 context 和 action admission 不在该切片内。
- 2026-10-14 局部摘要（仅原生任务执行）：owner 私有目标控制、fenced executor/Context、物流、权威配方和 prepared claimed Agent 建厂到首产物的代码切片已交付。实际试玩源、失败与 targeted 范围从 [Game API 专业合同及证据入口](../../../world-simulator/llm/external-agent-game-api.design.md#当前实施候选原生任务执行链)读取，逐次 CI/清理流水归原交付 PR。本摘要不证明完整 TaskSnapshot、两 Runtime 首局、GUI、稳定生产或部署；部署暂停保持，其他主题基线不变。
- 两个真实游戏 Runtime 的自主首局、多轮目标和恢复仍需分别证明；开发 Codex adapter 的合入不能替代这些游戏结果。

**客户端与原生 Harness 实现（2026-10-17 核对）：** [共享 SDK PR](https://github.com/eng-cc/oasis7/pull/4734)已合入 `25b4d211a612d783172e6266307c71c62b8c00c0`；[#4744](https://github.com/eng-cc/oasis7/pull/4744)已合入 `a7555be3f4e6c7814b3660ca04e42130b244de61`，取代[原固定工业任务执行器候选](https://github.com/eng-cc/oasis7/tree/50e128cbe369f8f6adca6556ff99784c7f7c93ad)。执行方复用普通签名 HTTP、原操作恢复与本地有限预算；模型连接和规划在执行方，目标、executor fence、资源、世界时间和收据仍在 Runtime。原生 Harness 由可信 Launcher 经会话级模型连接显式启动，见[模型连接与运行边界](../../../world-simulator/llm/byok-model-consumption.runbook.md)；Viewer 生命周期不构成托管承诺。旧生产能力仍有明确旧入口，不能由工业切片推出全部迁移；边界与剩余范围见 [Game API 专业合同](../../../world-simulator/llm/external-agent-game-api.design.md)及 [#4726](https://github.com/eng-cc/oasis7/issues/4726)。源码拆分、世界准入、运行位置或支持能力变化时重新核对本项。

**实际部署核验：** 本轮未核对端点暴露、TLS/网关、账号存储、凭据 lane 或真实 Runtime 支持组合。原候选的私有测试节点脚本协议验证与只读浏览器验证只证明各自固定范围；后续真实供应商验证须按[BYOK 实现 PR #4812](https://github.com/nil-studio/oasis7/pull/4812)记录的 source 与窗口分别理解，不能据此推出完整自主试玩或生产部署已验收；其他条目的环境基线不变。

<a id="cc-architecture-release"></a>

## 发布与运维

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-10 · 发布与恢复 | 节点软件、世界 runtime manifest、客户端、不兼容基础协议分属不同发布 lane；换 binary 不自动激活世界规则，软件回退不回滚已提交世界 | 升级 ABI/协议、发布候选、补更 fleet、重置/回滚/恢复或重新开放写服务 | [四类发布路径](../../../world-runtime/design.md#4-runtime-activation-deployment-and-recovery-target)、[运维事实边界](../../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#stable-authority-and-evidence-boundary)、[失败与回滚](../../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#14-phase-h---failure-handling-and-rollback) |

**设计状态：** 升级、激活和恢复分别由世界与环境合同约束；技术恢复步骤不新增普通 PR 的流程门禁。

**已有实现：** 已有部署更新、有界 proof/identity 及回滚入口。runbook 当前明记 [identity-v2 与 managed peer registry provisioning 缺口](../../../p2p/blockchain/public-testnet-governed-bootstrap.runbook.md#221-bounded-204-proof-and-identity-v2-contract)，只影响依赖它的具体部署/破坏性操作，不能扩成阅读、看板刷新或所有开发任务的前置条件。

**实际部署核验：** 没有本次发布或恢复结果；当前主线的[实际部署暂缓约束](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)仍需遵守。重启、旧窗口通过和源码合入均不证明当日恢复完成。

<a id="cc-architecture-command-center"></a>

## 指挥客户端

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-11 · 客户端自身 | 独立 macOS Tauri 2 + Rust + Solid/TypeScript + SQLite；文档/Git/PR 保留事实所有权，Codex 默认关闭且属可选实验接入 | 改技术栈、引入云端/常驻 supervisor、共享团队状态、公开分发、扩版本或自动执行 | [技术栈](../command-center.design.md#43-具体技术栈)、[基础切片](../command-center.design.md#110-首个基础实现切片)、[实验接入](../command-center.design.md#DES-CC-012)、[操作说明](../../../../tools/oasis7-command-center/README.md#verification) |

**设计状态：** 配对产品/系统设计仍含目标能力，完整产品验收未完成。系统设计 §3 区分当前实现与设计差距，§11 说明验证方法和未覆盖范围；实际交付以对应 PR 和证据为准。

**2026-10-17 设计核对：** 基于 `main@0ec10379419b0aaedb9d16157b81c92f4e6e3495`，[首页投影](../command-center.design.md#DES-CC-013)与[工作详情/关联](../command-center.design.md#DES-CC-014)补齐来源身份、时效和冲突处理、决定同步闭合、一次发起及局部读取失败隔离。仓库总览只聚合获准读取的引用，不改写既有工作树、私有资料或运行控制身份；Tauri/Rust/Solid 路线保持。本机工具和文档树按[任务入口](../local-tools.design.md#DES-CC-TOOLS-005)与[主题阅读](../document-tree.design.md#DES-CC-TREE-004)组织。目标及[实现差距](../command-center.design.md#1104-日常工作闭环的实现差距)尚待落实，未增加部署、原生体验或性能验证事实。

**已有实现与验证：**

- [PR #4533](https://github.com/eng-cc/oasis7/pull/4533)：桌面基础、四视图、SQLite、真实 worktree 和有限 GitHub 窗口；记录了 macOS arm64 本地打包与原生交互验证。
- [PR #4563](https://github.com/eng-cc/oasis7/pull/4563)：固定 Codex CLI 0.137.0 的可选实验执行、原请求身份、未知结果与退出管理；真实握手/receipt 有记录，但没有成功模型输出证明。
- [PR #4572](https://github.com/eng-cc/oasis7/pull/4572)：读取显式规范后继、保留历史版本和来源理由；记录了原生界面与重启恢复验证。
- 首页的[来源阅读降级入口](../../../../tools/oasis7-command-center/ui/src/SummarySources.tsx)复用既有文档索引和来源详情，分别提示缺失摘要；不新增后台来源、执行动作或聚合判断。四区首页及完整用户任务仍未验收。
- 生命周期解析将原始声明和文档类别分开，可选派生字段兼容旧阅读版本；详见[专题设计](../document-tree.design.md#DES-CC-TREE-001)。类别不推导实现状态，目录生命周期、类型和显式配对缺口筛选沿已发布清单投影，不改变源声明。
- 2026-10-15 候选源码核对：[文件清单](../../../../tools/oasis7-command-center/core/src/document_inventory.rs)沿现有有界扫描保留逐文件读取/遗漏记录，使用独立版本化 cache key，与章节/搜索同事务发布；原 Snapshot shape 和数据库 schema 版本不变。有限 DTO 查询核对项目、来源版本/代次、章节摘要及分页筛选，规范视图可查阅缺口。独立树快照/attempt、固定提交及关系投影仍待实施，见[实施范围](../document-tree.design.md#111-验证映射表)；真实宿主与交互验证范围归实现 PR。
- 个人决定、阅读记录和未决操作与可重建缓存分开；启用连接不代表授权任务，恢复不自动重发未知副作用。

**本机工具摘要（局部核对于 2026-10-14）：** [PR #4624](https://github.com/eng-cc/oasis7/pull/4624) 已合入；内置资料、同构建可选 CLI、既有兼容 signer 的文件签名与显式私有会话输入的专业合同见[普通本机工具设计](../local-tools.design.md)。资料 root、普通索引和私有会话标记约束传播，资料保存不产生签名授权；实际安装、准备请求和 worker 提交分别核对。

候选 App 的建库、保存冲突和 CLI 安装观察只证明 PR 中记录的原候选/机器范围，不证明用户固定安装、部署、真实 signer 提交、公证或公共分发。管理员桥与 Apple 签名集成的暂停继续有效，本机用户 signer 绑定与真实签名提交仍未验收；过程记录见 [PR #4624](https://github.com/eng-cc/oasis7/pull/4624)，验收边界见[验收映射](../local-tools.design.md#11-验收与验证)。本次不刷新其他架构或环境状态。

## 下次刷新时

- 先核对主责入口和显式后继，再核对影响本主题的实际合入、验收或部署结果；没有变化不为整张表重写状态。
- 保留来源版本和验证窗口；设计改变、代码合入、实际环境变化分别更新，旧成功不自动扩到新范围。
- 对确需决策的承诺、成本、权限、不可逆数据或真实不确定结果，在[待人关注](../README.md#cc-status-attention)说明影响与下一步。
- 决定落实回相应主责产品/系统文档，本页随源刷新；具体维护步骤沿用[持续更新流程](../command-center.manual.md#cc-maintenance-architecture)。

### 文档全局树阅读设计

2026-10-13：基于 `main@b7e21b23bf0edfe86f302bb7186aab77e57b26c3` 源码核对，[文档树设计](../document-tree.design.md)采用既有 Rust 索引与有限 DTO，分离导航树和显式配对关系，快照绑定来源版本并保留部分覆盖。四产品域与专业域分别展示，读取不启动模型或写源文档。当前目录及产品/专业导航投影的实现边界见[专题设计](../document-tree.design.md#111-验证映射表)；两树展开与个人阅读恢复沿独立版本记录提供；独立树快照和固定提交仍待实施，实际桌面验证范围归实现 PR。
