# 指挥面板架构索引

2026-10-24 Rust 工程契约局部核对：[#5026](https://github.com/nil-studio/oasis7/pull/5026)、[#5027](https://github.com/nil-studio/oasis7/pull/5027)、[#5028](https://github.com/nil-studio/oasis7/pull/5028) 已合入首批 6 成员继承、9 条架构规则/37 个 profile 与 signed-read 五 GET 的 TS/schema 生成消费；API revision 1 / read schema revision 5。[现行设计](../../rust-governance/rust-engineering-contracts.design.md)保留原专业 owner 与开发期工具边界。[#5036](https://github.com/nil-studio/oasis7/issues/5036) 承接 path/status 回归、dev 范围及可信局部编译选择，尚待 PR 评审；不据此宣称部署、完整世界 schema 或全平台验收。

> 指挥中心的职责与目标见[产品要求](../command-center.prd.md)和[系统设计](../command-center.design.md)：云端承载四个主视图、已提交来源与个人空间，本地承载工作树、私有资料及原生工具。来源事实与个人记录各守原义；实际实现和可用性由对应交付证据说明。


这里集中列出做项目决策时需要记住的架构边界、实现入口和部署核验缺口。它是主责文档与交付事实的派生导航；具体约束、变更理由和验收仍在链接的原文中维护。

云端图形化“工程架构”模块的目标见[产品要求](../command-center.prd.md#REQ-CC-018)和[系统设计](../engineering-architecture.design.md)：从全局下钻模块、组件、代码与适用规范，沿 dot 既有内容维护核对更新。本索引含不同时间的局部事实，只作来源导航，不能单独生成标为当前完整的工程图；图须回读所选提交的主责及允许元数据。此处新增设计入口，不更新下列历史核验时间，也不表示模块已上线。

- 代码与文档核对基线：`main@598b308a23f3f0189843618d14abb1aa31c0d2de`，摘要核对日期为 2026-10-11。
- 2026-10-11 摘要轮次没有实际环境采样、部署、重启、恢复演练或模型调用；该轮所引 PR 验证不是重新运行的结果。各主题的局部核对另注日期与范围，不刷新其他主题或环境的核验时点。
- 历史环境表 Last Verified 为 **2026-07-06**，节点表为 **2026-06-23**。读取摘要的日期不能代替部署、健康或可用性的核验日期。
- 项目进度与待处理事项见[当前状态](../README.md#cc-status-attention)；本页的更新方法见[架构索引维护流程](../command-center.manual.md#cc-maintenance-architecture)。

独立 ECS 技术预览的环境准备、构建尝试和部署验收由 [交付任务 #4671](https://github.com/eng-cc/oasis7/issues/4671) 承接。本页保留架构入口；该任务的技术预览不代表正式世界开放或原测试网恢复。

## 从问题进入

Viewer 性能边界（2026-10-19 局部核对）：[性能合同](../../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#viewer-performance-contract) 已由 [#4907](https://github.com/nil-studio/oasis7/pull/4907) 合入；[实现 #4909](https://github.com/nil-studio/oasis7/pull/4909) 在 Host 到 WASM 前裁剪显示数据，并对公共 hash WASM 使用压缩与缓存，玩家状态及权限不进入公共缓存。后续补齐稳定显示裁剪、可见标签预算和独立加载实例，完整预算仍分阶段验证，线上配置未核验；改变显示容量/裁剪语义、缓存来源或初始化恢复边界时重新关注本项，不由本索引宣称部署完成。

Cargo 边界局部增量（2026-10-12）：[#4626](https://github.com/eng-cc/oasis7/pull/4626) / `133d70e45b71bd062032a734d68be1e290692b2c` 将 native 生产 cognition 状态转换转接至 Harness，World readback 与 signed-service 校验仍由 Runtime 适配器承担，旧 DTO/checkpoint 兼容保留；不新增 Harness→Oasis 依赖。支持包 profile 隔离尚在分支验证，直接 Oasis wasm32 目标及部署不因本次局部摘要变为已验证。测试入口见[测试手册](../../../../testing-manual.md)。

核心执行边界局部核对（2026-10-18，仅 [#4853](https://github.com/nil-studio/oasis7/issues/4853)）：目标所有权由 [Runtime 设计](../../../world-runtime/design.md#实现边界与共享值所有权)、[Simulator 设计](../../../world-simulator/design.md)和 [Game Client 边界](../../../world-simulator/llm/agent-game-client-boundary.design.md)承载。Simulator 与权威 Runtime 并列，Service 依赖 Runtime 与合同层；共享值和 wire 合同归 API / Proto，客户端证明消费与操作恢复归 Game Client，旧组合归档和进程组装留宿主。Simulator、Runtime / Service 及客户端迁移均已合入；[后续 #4879](https://github.com/nil-studio/oasis7/issues/4879) 已通过 [#4882](https://github.com/nil-studio/oasis7/pull/4882) 合入，完整 legacy Scheduler 具体合同与纯验证归 API / Game Client；宿主通过借用内核事件及稀疏 runtime-event 附属数据组合旧归档，只保留扩展及原切面证据归属，不保留旧模型。`journal()` 的 Rust 读取接口改为借用 view，历史编码保持；实际交付与验证范围见[项目状态](../README.md#cc-status-core-crate-extraction)。这不增加进程或 transport，也不将 Runtime 的 WASM 值模型视为完整执行支持。 当前本地候选继续落实 [Viewer 应用与宿主边界](../../../world-simulator/viewer/viewer-minimal-system.design.md#应用与宿主边界)、Runtime/Service 的 CAS/Formal Protocol 直接消费及客户端合同归位，尚未合入。

L4 工具链局部文档更新（2026-10-12，核对基线 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056`）：[testing 合同](../../../testing/prd.md#l4-build-driver)明确默认 release/正式 UI、同一实际 Hosted 执行会话、逐次 RunResult 与评审归属；[入口兼容设计](../../../world-simulator/viewer/viewer-web-entry-compatibility.design.md)保留 alias，并将测试 API 限于显式测试产物。[#4438](https://github.com/eng-cc/oasis7/issues/4438) 已以 `not_planned` 关闭，仅保留为历史文档承接来源，不再承担旧 runner/scaffold 的实施或运行验收；此处只更新设计导航，不表示新部署或试玩通过。

- 要开新环境、搬服务或调整节点：[环境与部署](#cc-architecture-deployment)。
- 要决定首次开放、扩成多节点权威或改变恢复承诺：[世界权威与状态](#cc-architecture-authority)。
- 要拆进程、调整执行边界：[服务与执行](#cc-architecture-runtime)。
- 要接入玩家、外部 Runtime 或远程控制：[身份与 Agent](#cc-architecture-agents)。
- 要升级、回退或发布：[发布与运维](#cc-architecture-release)。
- 要调整指挥面板技术路线、数据或执行能力：[指挥客户端](#cc-architecture-command-center)。

<a id="cc-architecture-business-contracts"></a>

## 业务链路的接口主责

2026-10-26 设计局部核对：以下是现有模块内的目标接口收敛，不是新增运行服务、全仓拆包或实现完成声明。文档与源码核对基线为 `b9bc4b2a6df858e27f94754d227b0fdc23ae3639`；各项实施、真实入口及兼容验收仍待对应交付验证，不更新其他主题的部署基线。

| 业务问题 | 唯一主责与下游边界 | 目标合同 |
| --- | --- | --- |
| 服务读取如何解释 cognition 历史 | Projection codec 解释现有只读值；Service 认证/裁剪，Runtime 保留执行、恢复与 raw 兼容取值 | [Cognition 读取](../../../world-runtime/design.md#cognition-projection-read-contract) |
| 玩家撤销、重试和重新授权由谁裁决 | Runtime 拥有 delegation mutation/exact-replay；Viewer 保留 session/nonce/profile 并只映射结果 | [委托结果与重试](../../../world-runtime/runtime/indirect-control-agency-execution-and-continuation.design.md#owner-delegation-result-replay-contract) |
| 断线后如何继续同一原意图 | Game Client 拥有单操作 phase/disposition；Harness 保留全 run 扫描、预算与停止策略；查询不隐含重投 | [原操作生命周期](../../../world-simulator/llm/agent-game-client-boundary.design.md#original-operation-lifecycle-port) |
| 如何判断自己的能力与下一条恢复路径 | 只读领域查询分开个人能力、世界上下文与三态恢复；presentation 不从文案或目标标签制造可行性 | [能力与恢复查询](../../../world-simulator/viewer/viewer-gameplay-release-experience-overhaul.design.md#subject-capability-recovery-query) |

配套技术边界由[网络失败语义](../../../p2p/network/readme-p1-network-production-hardening.design.md#typed-failure-contract)、[额度桥诊断](../../../p2p/token/mainchain-token-newapi-quota-bridge.design.md)、[Provider 探测](../../../world-simulator/llm/provider-loopback-http-contract.design.md)和[WASM 构建来源](../../../world-runtime/wasm/wasm-deterministic-build-pipeline.design.md)分别拥有。不能因共享“恢复/错误/接口”词汇合并为万能状态机；既有可独立修复的业务缺陷不必等待全部架构迁移。

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

浏览器 BYOK 局部候选（2026-10-25）：[新设计](../../../world-simulator/llm/self-supplied-model-connection.design.md)把模型调用和循环放在浏览器，模型 Key 留在本站 localStorage；协议 Rust/WASM 复用 canonical 签名合同，游戏 executor 的非可导出身份和原件日志分别保存在 IndexedDB，世界授权仍由既有 participant/owner/executor 网关与 Runtime 判定。部署信任 pin 固定注入，只有显式启用才增加 DeepSeek CSP origin。[实现 PR #5067](https://github.com/nil-studio/oasis7/pull/5067) 尚待 CI/合入及获准的 b1 部署试玩；既有服务器会话处理是历史路径，不证明新组合可玩，其他环境核验时间不变。

**实际部署核验：** 本组三项均未取得当日运行窗口。[历史云上清单](../../governance/environment-lanes-and-inventory-2026-05-29.md#7-当前云上清单)只用于定位；当前节点数、版本、实际角色、健康和外部可达性保持未核对。

<a id="cc-architecture-authority"></a>

## 世界权威与状态

持久单权威和 BFT 是同一世界的不同权威阶段。单权威不等于仅靠一台主机或一个存储目录保存所有已确认结果。

<a id="cc-architecture-runtime"></a>

### 服务与执行

玩法权威协议局部设计核对（2026-10-19，文档基线 `e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6`）：[Runtime 专题分工](../../../world-runtime/design.md#runtime-gameplay-authority-designs)与[八项协议入口](../../../world-runtime/prd.index.md#gameplay-authority-protocols)把治理分类、charter、工业工作、配方替代、公共 levy、外部性、贡献恢复和安全确认分配给独立 authority。各专题共享世界提交顺序和恢复事实；charter、模型建议、预览或其他专题 receipt 不自动授予新权限或代签世界效果。该增量只补目标系统设计，不改变现有 Runtime/native 制度边界；实现、正式持久性及部署仍按各原合同验证。

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-04 · 单权威到 BFT | 首次开放采用受控单权威持久世界；BFT 并行研发，后续在同世界 H/H+1 交接激活写权 | 决定首次开放、切换 signer/epoch、扩展权威集合或改变保留承诺 | [当前阶段](../../../product/world-infrastructure/prd.md#world-infrastructure-current-stage)、[权威 profiles](../../../p2p/design.md#des-p2p-authority-profiles) |
| ARCH-05 · 服务分工 | P2P 维护合法提交证明，runtime 确定性执行，game/Agent/Viewer 提交意图和消费已提交状态；链进程入口位于独立 `oasis7_chain_runtime`，纯投影证明位于 `oasis7_client_api`，世界执行适配仍归只提供库的 `oasis7`；其余 21 个入口按 bridge/provider/launcher/client/ops/viewer 职责归独立包，共享策略、观测和 credit adapter 归独立库；Launcher 解析归 `oasis7_launcher_config`、Viewer 应用、会话、恢复编排与网络服务归 `oasis7_viewer_runtime`（本地迁移候选，尚未合入）、WASM 获取/构建归 `oasis7_wasm_materializer`，独立 formal read 验证归客户端层；本地 CAS/codec/pin/index 归 `oasis7_cas`，纯 formal 协议与证明归 `oasis7_formal_protocol`，分布式持久化、端点与协调归 `oasis7_distfs`；signer 纯协议归 `oasis7_signer_protocol`、托管与主机操作归 `oasis7_local_signer`，客户端私有持久化归 `oasis7_client_storage`，Web Viewer 不承担完整节点职责（2026-10-17 源码核对；未核对部署） | 拆服务/进程、加网关、改模块归属或引入第二条执行路径 | [执行分层](../../../world-runtime/design.md#3-目标设计结构)、[P2P 边界](../../../p2p/design.md#4-边界与结构)、[高层架构](../../../../README.md#高层架构) |
| ARCH-06 · 写入与持久 | accepted、prepare、committed 分开；正式确认依赖完整记录和决定跨独立故障域持久，未知提交先查原请求 | 改存储、超时重试、写者隔离，或决定故障期间是否继续写入 | [双端持久前置](../../../p2p/design.md#des-p2p-replicated-durability-prerequisite)、[runtime 提交与恢复](../../../world-runtime/design.md#runtime-authority-commit-recovery-target) |
| ARCH-07 · 保留与恢复 | 正式持久世界保留完整合法历史；物理共享、压缩及冷热策略保持原字节、恢复依赖与独立副本。存储 profile 不授予删历史权限，同世界只读恢复不自动恢复写服务 | 调磁盘预算、回收临时材料、改变物理格式或执行迁移与灾备切换 | [完整历史产品要求](../../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)、[无损存储设计](../../../world-runtime/runtime/formal-world-storage-efficiency.design.md#des-lws-001)、[重新服务边界](../../../product/world-infrastructure/prd.md#恢复后的重新服务闸门) |

**设计状态：** 本组是当前主责文档声明的路线与目标合同；正式 profile、完整事务和灾备目标不能因为已有前置切片而标为已验收。

2026-10-25 节点生命周期局部核对：[产品角色与状态路径](../../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#node-deployment-journey)区分公开读取、完整只读、受控写者与目标 BFT。原 parent 的治理认证已随 [#5049](https://github.com/nil-studio/oasis7/pull/5049)接入 qualified 历史恢复；[#5052](https://github.com/nil-studio/oasis7/pull/5052)的 [V2 issuer 管理账本](../../../p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-issuer-ledger)提供独立 root policy、连续原命令日志、witness CAS 和旧 issuer 代际 fencing 的管理 API。它不改变 V1 provider 的现有读取语义，生产 witness、有限新鲜度 lease、可信时间、逐实例截止和 effective 撤销仍缺实现／环境证明。发现已分离原签名与本机观察，并接入 v1 时间和共享工作预算；[本机 V2 发行合同](../../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-durable-publication-v2)把显式首次初始化与日常 open-existing 分开，本机发行已随 [#5053](https://github.com/nil-studio/oasis7/pull/5053)交付；[#5054](https://github.com/nil-studio/oasis7/pull/5054)交付[共享记录准入预算](../../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-record-admission-budget)；[#5055](https://github.com/nil-studio/oasis7/pull/5055)已交付[接收端水位](../../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-receiver-floor-v2)接入持久序号、冲突隔离和防降级；[#5056](https://github.com/nil-studio/oasis7/pull/5056)已交付的[撤销扩展](../../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-tombstone-v2)接入 tombstone 原件和显式恢复，legacy 模式明确拒绝该状态。[#5057](https://github.com/nil-studio/oasis7/pull/5057)已交付 rendezvous 有界一次性触发链，只关联精确 live 载荷并提供诊断，不提升健康来源或连接权限。后续本地候选为 register／discover 接入共享出站预算，仍待必要 CI 和合入；入站服务和跨重启配额仍属缺口。[V3 provider 租约日志合同](../../../p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-provider-lease-log-v3)补充原 witness 上的迁移与有限时间租约语义，协议与本地持久 API 已随 [#5060](https://github.com/nil-studio/oasis7/pull/5060)交付，尚无生产 witness 或逐 chunk 读取接入证明。可信时间、任意磁盘回滚防护和全网撤销 ACK 仍无实现／环境证明。实际成员安装 ACK、完整同步 checkpoint 受保护 minimum 和 BFT 权威链仍以各自主责设计与 [#4975](https://github.com/nil-studio/oasis7/issues/4975) 的实现／验收缺口为准。本段没有新部署、可信时钟或独立物理故障域证据；这些边界发生变化时再核对，不刷新其他架构主题。

**已有实现：**

- ARCH-04（2026-10-26 局部核对）：正式受控 writer、公开投影和获授权完整只读同步已有[实现入口](../../../p2p/blockchain/local-node-lifecycle.runbook.md#daily-commands)。BFT 已有独立于旧 PoS 的固定集合 NodeRuntime、完整 QC 的实际执行与冷恢复；[受保护签名工程接线](../../../p2p/design.md#des-p2p-bft-protected-signer-v1)使用独立 provider journal、实例 lease 与可信计时合同。当前仅证明工程接口和受控测试范围；真实硬件/防回滚 provider、动态治理集合、权威交接、长期 checkpoint 与完整消费者验收仍未闭合，不能升级为正式 BFT readiness。实现状态及剩余义务以 [DCS-004](../../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-004) 为准。
- ARCH-05：已有有序动作、roots、execution records、checkpoint/replay 及多种 prepare/publish 路径；[当前实现边界](../../../world-runtime/design.md#5-current-implementation-boundary-and-target-gap)仍明确整体能力为 partial。
- ARCH-06：正式受控路径已有 qualified cut、保护服务与原请求恢复的实现切片，详见[当前实现边界](../../../p2p/design.md#des-p2p-formal-controlled-authority-v1)；同机双目录不能证明独立故障域。真实 P/R 故障、旧写者隔离和原结果恢复按[故障域验收](../../../p2p/design.md#des-p2p-controlled-fault-domain-acceptance-v1)分别取证，受控路径不承诺 BFT 分区期间继续写入。
- ARCH-07：已有 execution bridge v3、CAS、sidecar、retention/GC/replay 的实现合同；[本地恢复边界](../../../world-runtime/runtime/runtime-storage-footprint-governance.prd.md#storage-accept-service-boundary)不证明分布式可得性或完整灾备。

ARCH-07 局部实现核对（2026-10-19）：[PR #4870](https://github.com/nil-studio/oasis7/pull/4870) 已合入[当前存储实现](../../../world-runtime/runtime/formal-world-storage-efficiency.design.md#current-storage-implementation)，覆盖共享压缩、事务物理代、独立闭包读回与 attempt 回收、完整增量、Merkle v2、任务控制冷链和原生工件。新世界直接创建新物理表示；codec 不改变逻辑身份、独立持久条件或服务权限。已验证版本有局部容量和同机恢复证据；后续按[手动专项入口](../../../world-runtime/runtime/formal-world-storage-efficiency.manual.md#lws-manual-ci)逐场景核对网络模型、资源预算与所选规模，具体运行不外推生产延迟。当前无部署节点，真实双故障域和更大规模未完成验收；其他架构及环境基线不变。 [PR #4891](https://github.com/nil-studio/oasis7/pull/4891) 当前候选把 attempt 双端释放确认为可恢复阶段，接通有预算的默认物理维护，避免每页重复全 selector 恢复扫描，并保留原有共享布局。候选完整目录盘点包含 native、P/R、attempt、旧代和 checkpoint；短样本不证明净节省，规模与部署验收仍有缺口。

节点生命周期实现（2026-10-17 核对）：[PR #4683](https://github.com/eng-cc/oasis7/pull/4683) 接通原请求持久恢复、健康集合转换与可续做节点生命周期；本机验证范围和平台/部署 TODO 见[实施边界](../../../p2p/blockchain/persistent-world-node-lifecycle.runbook.md)。代码已合入（`da890a9125a435969b82f5a14ab3c9143cc0690e`）；具体历史验证范围以实施 PR 与 runbook 为准，当前部署与独立故障域未核验。

**实际部署核验：** 未验证正式 profile 激活、实际存储同步、故障域隔离、旧写者 fencing、备份可用性或恢复后的写服务。这些结论需要各自候选与环境证据，不能由代码数量汇总为完成率。

正式激活、独立故障域恢复和人工交接的能力边界由对应专业合同持有。[#4363 的旧规划](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)仅作历史来源；该 Issue 已停止独立跟踪，不据此恢复执行义务。

<a id="cc-architecture-world-policy-contracts"></a>

### 制造、保供与组织处置

2026-10-19 局部设计核对：精确政策由 game 专题持有，产品声明玩家承诺，Runtime 消费固定政策版本并拥有业务事实。三项复用现有权威提交与持久化边界，不能由普通材料产出、通用 Crisis 或删除联盟映射推导完成。

| 主题 | 必须保持的边界 | 专业设计入口 |
| --- | --- | --- |
| 工业取得 Agent | 原订单的资源、产能、交付身份和 claim 同链；交付提交前不产生可控制的新 Agent，quota 不直接供给 | [制造与原子交付](../../../world-runtime/runtime/agent-industrial-manufacturing.design.md) |
| 紧急保供 | 授权、冻结批次、累计需求和结算分层；到期退出不迁移旧待决权利，不重复分配或结算同一价值 | [保供授权与结算](../../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md) |
| 组织连续性 | 基础阶段与申诉请求分离；重叠对象/权利/动作 scope 保持唯一处分权，交接原子终止或收缩旧授权 | [组织处置与交接](../../../world-runtime/runtime/organization-continuity-and-disposition.design.md) |

业务实现、兼容与运行验收仍分别由 [#4850](https://github.com/nil-studio/oasis7/issues/4850)、[#4866](https://github.com/nil-studio/oasis7/issues/4866)、[#4868](https://github.com/nil-studio/oasis7/issues/4868)承接。扩大配方/物资/资产范围、改变政策版本或消费者时重新核对本组合同；当前未核验部署，不前移其他架构条目的基线。

<a id="cc-architecture-agents"></a>

## 身份与 Agent

2026-10-25 测试账号托管设计局部核对（源码基线 `73398d454082eb2106b2f1075a57bc9f377cd222`）：[产品范围](../../../p2p/blockchain/hosted-public-join-managed-identity-custody.prd.md#cloud-test-custody)与[系统边界](../../../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-test-custody-design)定义受控测试 lane；[Rust 包组织](../../../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-custody-rust)规划 core/worker 两包、四角色独立工件，并在既有 Game Client 增加可信 custody transport。Identity 独占托管账号原生会话并提供窄角色 forwarding，账号 DO 保持 ACL/授权/预算/原件权威；开户各阶段幂等，D1 仅投影。[客户端/PoP](../../../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-custody-client)、固定 native 工具和[有界自测收尾](../../../p2p/blockchain/hosted-public-join-managed-identity-custody.design.md#cloud-custody-selftest)已成设计合同，原生与 dot MCP 接线分 profile 实测。[基础候选](../../../../crates/oasis7_custody_worker/README.md)已建立两包、四角色构建及 metadata/error 合同，业务入口保持关闭；原生身份批准/来源/会话 metadata 和 frozen fence 存储已有实现；基础切片已合入 [#5078](https://github.com/nil-studio/oasis7/pull/5078)，2026-10-26 Owner 批准消费组件已由 [#5091](https://github.com/nil-studio/oasis7/pull/5091) 合入；管理认证/目录候选 [#5105](https://github.com/nil-studio/oasis7/pull/5105) 接通同源 Passkey、draft 关联与主动冻结；后续候选补 SQLite 事务内签名撤销事件、独立投递和 Owner 闭环确认，尚待完整验证，未知库存保持 pending，批准/密钥/世界执行仍关闭；CT-008/ST-02/03/04/06 因可信宿主、配对和 TestSession 收尾链缺失保持 blocked，进度集中在 [#5076](https://github.com/nil-studio/oasis7/issues/5076)；云端身份转发、秘密/事务适配、可信宿主与真实 Agent 验收仍待实现；prepared-world targeted 不代签 cold-entry 或全部 Runtime，集中托管不改变世界权威，也不提供生产资产/治理 custody。

玩家进入、游戏行动委托、operator 控制和代码开发会话分别成立。相同工具名称不能合并它们的权限与完成证据。

| 主题 | 关键边界 | 需人关注时机 | 权威入口 |
| --- | --- | --- | --- |
| ARCH-08 · 网络与权限 | URL/登录只提供接入；玩家 session、Agent 委托和 operator 权限分开，恢复不能复活旧授权 | 暴露公网 API、接账号/模型端点、改认证或委托、增加远程控制 | [Hosted 进入](../../../product/player-entry-distribution/prd.md#hosted-玩家进入与会话边界)、[会话与授权连续性](../../../product/player-entry-distribution/prd.md#player-session-authorization-continuity)、[headless 鉴权](../../../headless-runtime/design.md#6-鉴权与恢复边界) |
| ARCH-09 · 游戏 Runtime 与开发入口 | 游戏侧目标是 OpenClaw/Codex 通过官方游戏 Skill 和受授权游戏 API 持续游玩；开发指挥客户端读取交付事实并打开原开发会话 | 声明 Runtime 支持、管理多 Agent、改预算/停止行为或连接游戏 Viewer 与开发看板 | [游戏角色](../../../product/agents-world-simulation/external-agent-runtime-play.prd.md#12-角色与职责)、[首批 Runtime 目标](../../../product/agents-world-simulation/external-agent-runtime-play.prd.md#21-首批-runtime-与支持单位)、[开发会话](../command-center.design.md#DES-CC-007) |

**设计状态：** 接入、委托和角色边界由各自主责合同拥有。外部 Runtime 的首期功能清单是承诺范围，阶段标签不等于实现状态。

2026-10-21 自主入场局部核对：[身份主责](../../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#machine-registration)及 [#4954](https://github.com/nil-studio/oasis7/pull/4954) 候选已承接机器身份、canonical participant 唯一绑定/迁移、独立 owner/executor、starter、任务目标与原件恢复。本地工具代理按资源隔离配置、状态和预算；实际 Runtime 沙箱与费用须由获准 profile 约束。来源与剩余验收见 [#4946](https://github.com/nil-studio/oasis7/issues/4946)；完整 OpenClaw/Codex 冷入场未验收，候选不能当已合入/上线，禁止自动合入、部署和开放注册，既有发布暂停继续有效。

2026-10-12 局部设计增量（同上固定基线）：[首次绑定目标](../../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001) 按账号在同一世界的权威状态处理注册、返回和幂等；[首聊 service 目标](../../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-chat-005) 使用 owner 最小权威投影区分可通过、受阻和未知，并在提交时重验。注册/绑定、starter OC 领取与经济 ClaimAgent 分别成立；现有固定 starter 和不完整余额投影不能证明这些后继目标已实现。

**已有实现：**

- ARCH-08：headless ingress 已定义签名 payload、ed25519、单调 nonce 与恢复防重放；它不自动证明任意新 API 的完整授权和恢复行为。
- ARCH-08 的 [多凭证账号设计](../../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md) 使用固定 RP/origin、事务身份存储、HttpOnly 账号 Cookie 和短期签名世界授权；新注册默认关闭，不改变资产或 operator 授权。2026-10-19 核对的[导入协议](../../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#imported-key-web)沿用同一账号/会话/世界授权边界，服务端仅接收公钥和挑战签名，`import_only` 不代表客户端生成和加密备份已发布；目标部署状态仍以 capabilities 及环境验收为准。代码验证见 [#4669](https://github.com/eng-cc/oasis7/pull/4669)，部署与环境验收进度见 [#4671](https://github.com/eng-cc/oasis7/issues/4671)，验收方法见 [手册](../../../testing/manual/hosted-passkey-account.manual.md)。
- ARCH-08 的新预览世界初始化按 [合同](../../../p2p/design.md#des-p2p-fresh-preview-bootstrap) 固定真实初始材料、CAS/提交证明和节点派生服务 signer，再为 Viewer 配置独立 public-read 身份。操作见 [手册](../../../testing/manual/fresh-world-preview-bootstrap.manual.md)；这些材料不授予 Agent 写权限或正式 activation/finality。
- 2026-10-21 准入职责拆分候选：官方与外部客户端共用身份到 executor 的协议；[DevLocal 本地运营者与正式网络治理](../../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)使用不同授权历史。可信 Host 在恢复及尾日志重放前固定安全策略和公钥 pin，缓存按完整上下文隔离，安装沿节点写锁和 canonical commit；[配置手册](../../../testing/manual/single-ecs-preview-deploy.manual.md#devlocal-世界的参与者准入授权)不授予客户端管理信任根的能力。当前为待验证实现候选，本轮未变更部署；切换 authority source、恢复上下文或正式网络配置时重新核对。
- ARCH-09：[PR #4562](https://github.com/eng-cc/oasis7/pull/4562)交付 Game API 的发现合同和已实现签名读取；范围仍是显式开启的 loopback bootstrap。
- [PR #4565](https://github.com/eng-cc/oasis7/pull/4565)增加持久、owner-signed task control；初态 suspended、没有 executor。配对、私有目标、完整 context 和 action admission 不在该切片内。
- 2026-10-14 局部摘要（仅原生任务执行）：owner 私有目标控制、fenced executor/Context、物流、权威配方和 prepared claimed Agent 建厂到首产物的代码切片已交付。实际试玩源、失败与 targeted 范围从 [Game API 专业合同及证据入口](../../../world-simulator/llm/external-agent-game-api.design.md#当前实施候选原生任务执行链)读取，逐次 CI/清理流水归原交付 PR。本摘要不证明完整 TaskSnapshot、两 Runtime 首局、GUI、稳定生产或部署；部署暂停保持，其他主题基线不变。
- 两个真实游戏 Runtime 的自主首局、多轮目标和恢复仍需分别证明；开发客户端的导航与阅读验证不能替代游戏结果。

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
| ARCH-11 · 云端与本地 | Sites 网页与后端承担四视图、已提交文档/GitHub 事实和云端个人记录；本地 Tauri/Rust 辅助端承担工作树、私有资料、CLI 与 signer。浏览器与本机 IPC 分离，精确导航不构成执行或上传授权 | 改 Site 身份/访问范围、记录归属、来源授权、数据格式或本机能力边界 | [职责与身份](../command-center.design.md#DES-CC-001)、[云端个人空间](../command-center.design.md#DES-CC-017)、[本机工具](../local-tools.design.md) |

本主题于 2026-10-23 核对文档职责及 [v6 公开实现反馈](https://github.com/nil-studio/oasis7/issues/4983#issuecomment-6343356855)。云端已有个人空间和交付摘要，最终合同补齐[证据与结论边界](../command-center.design.md#cc-delivery-conclusion-boundaries)、[目标身份与依据版本分离](../command-center.design.md#cc-stable-goal-identity)、[完整文档候选发布](../document-tree.design.md#cc-document-snapshot-update)及[真实更新状态](../command-center.design.md#cc-update-health)；这些补充尚不构成实现或部署验收。实际来源基线、已有能力及未验证范围集中见[实现核对](../command-center.design.md#cc-current-implementation)；历史原生结果仍限定于[日常验证记录](../daily-work-verification.manual.md)的构建、环境和场景。

本机 context、CLI 和既有 signer 继续遵守[工具安全合同](../local-tools.design.md)：本机资料不进入云端普通记录，未知操作按原身份核对，Apple 签名及管理员集成的用户暂停保持有效。云端个人记录可导出恢复，不代表整机资料、工具账本或凭据可上传。

## 下次刷新时

- 先核对主责入口和显式后继，再核对影响本主题的实际合入、验收或部署结果；没有变化不为整张表重写状态。
- 保留来源版本和验证窗口；设计改变、代码合入、实际环境变化分别更新，旧成功不自动扩到新范围。
- 对确需决策的承诺、成本、权限、不可逆数据或真实不确定结果，在[待人关注](../README.md#cc-status-attention)说明影响与下一步。
- 决定落实回相应主责产品/系统文档，本页随源刷新；具体维护步骤沿用[持续更新流程](../command-center.manual.md#cc-maintenance-architecture)。

### 文档全局树阅读设计

[文档树合同](../document-tree.design.md)统一产品树、专业系统树、目录与明确配对关系的解释。云端读取固定提交，按可信身份保存个人阅读位置；本地工作树预览仅在设备范围索引并保存本机位置。快照与覆盖、部分读取和失效关系各自可见，来源刷新不改变条款权威或个人决定。两端验收分别核对真实浏览器与适用的原生交互，旧本地索引证据不代签云端完整文档树。

2026-10-24 单节点观测实现候选局部核对：[正式 CLI 文件报告](../../../p2p/node/node-triad-operations-observability.design.md#single-node-profile-report)接通公开 projection observer 与完整只读同步的状态投影，保留实例绑定、观察年龄与 producer 阻断；验证范围为离线文件报告及实际 producer／CLI 回归。triad HTTP profile 采集、validator／P/R、部署和现场恢复仍未据此验收；本条不刷新其他主题的证据时间。 独立本地 CLI 的持久采集和读取时年龄判断见[本地 profile 采集](../../../p2p/node/node-triad-operations-observability.design.md#local-profile-collection)。
