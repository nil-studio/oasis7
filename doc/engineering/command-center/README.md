# oasis7 项目状态与指挥入口

> 指挥中心的职责与目标见[产品要求](command-center.prd.md)和[系统设计](command-center.design.md)：云端承载四个主视图、已提交来源与个人空间，本地承载工作树、私有资料及原生工具。来源事实与个人记录各守原义；实际实现和可用性由对应交付证据说明。


本页帮助项目负责人在五分钟内看清长期方向、当前主线、已交付变化和需要关注的边界。
它是带来源和核验时间的滚动摘要；目标、规范、任务、部署和验收继续由原有主责文档及实际记录承载。

**当前判断：项目仍处于受限可玩技术预览。**
主线是同一持久世界中的首产物、阻塞恢复和回访接续；代码前置持续交付，正式开放与真人闭环尚未验收。
正式持久世界的部署仍按已有用户决定暂缓；独立 ECS 技术预览的部署与验收进度由 [交付任务 #4671](https://github.com/eng-cc/oasis7/issues/4671) 承接，不在本页维护执行日志。来源：[公开阶段](../../../README.md#项目状态)、[当前里程碑](../../core/prd.md#当前阶段收口优先级stage-closure-backlog)、[#4363 历史进展](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。

| 想了解什么 | 入口 |
| --- | --- |
| 下次如何持续更新 | [持续更新流程](command-center.manual.md) |
| 整体分层、部署、存储、权限与恢复 | [顶层架构索引](architecture/README.md) |
| 云端指挥中心与本地辅助端的产品及技术方案 | [产品设计](command-center.prd.md) · [系统设计](command-center.design.md) |
| 如何运行和操作已有客户端 | [客户端使用说明](../../../tools/oasis7-command-center/README.md) |

<a id="cc-status-cloud-local"></a>

### 指挥中心自身的职责边界

2026-10-22 文档核对：云端负责跨设备四视图、统一工作与依据、已提交规范、个人问题/决定/关联及阅读偏好；本地负责工作树、未提交预览、私有资料与原生工具。数据归属及安全导航见[系统设计](command-center.design.md#DES-CC-001)。本次只修订合同，现有应用及真实浏览器可用性未因此完成验收；已有实现和证据边界见[核对入口](command-center.design.md#cc-current-implementation)，实施与数据切换由[迁移 Issue #4983](https://github.com/nil-studio/oasis7/issues/4983)跟踪。不改变本页其他项目目标或部署状态的核验时间。

<a id="cc-status-baseline"></a>

## 核验基线与覆盖范围

Viewer 性能局部核对（2026-10-19，仅 [设计 #4907](https://github.com/nil-studio/oasis7/pull/4907) 与 [实现 #4909](https://github.com/nil-studio/oasis7/pull/4909)）：性能设计已合入；有界显示投影、更新合并及公共 WASM 压缩缓存已由 #4909 合入，尚未部署；后续补齐关键语义优先裁剪、标签限制和加载 attempt 预算/实例隔离。完整大世界、受控网络与冷暖加载预算仍未验收，能力范围和剩余条件见[入口性能要求](../../../product/player-entry-distribution/access-modes-and-release-readiness.prd.md#req-entry-performance-001)。不改变受限预览与世界准入结论；其他条目核验时点不变。

文档局部收敛（2026-10-12，仅 #4363）：以 `main@e01e29064517f4bf375767a19a09802335828249` 核对首局、首次绑定/首聊及持久权威合同，本次变更补齐下文第 1、2 项的目标设计与验收映射。新增目标尚待实现及适用验收；未取得新的部署或真人结果，其他条目的核验时点不变。

局部增量（2026-10-12，仅 Cargo 拆分）：[#4626](https://github.com/eng-cc/oasis7/pull/4626) 已合入 `133d70e45b71bd062032a734d68be1e290692b2c`，统一生产 cognition memory/continuation 核心并保留 Runtime authority；对应 main Rust [38212520926](https://github.com/eng-cc/oasis7/actions/runs/38212520926) 已成功。本分支正在补支持包 profile 选测隔离，尚未合入或完成 V0。用户确认提速收益已验证；实际自主试玩明确不在此次 Cargo V0 验收范围，不据此宣称实玩通过。该增量不刷新下面其他项目/部署事实的核验时间。

<a id="cc-status-core-crate-extraction"></a>

核心 crate 局部增量（2026-10-18，仅 [#4853](https://github.com/nil-studio/oasis7/issues/4853) 与 [后续 #4879](https://github.com/nil-studio/oasis7/issues/4879)）：共享 API 与准入归属整理 [#4865](https://github.com/nil-studio/oasis7/pull/4865)、原生 Simulator 与宿主兼容适配 [#4871](https://github.com/nil-studio/oasis7/pull/4871)、完整 Runtime / World Service 及客户端证明消费迁移 [#4873](https://github.com/nil-studio/oasis7/pull/4873) 均已合入，#4853 已关闭；#4873 合入提交为 `177eeb11c17ed2f2da3670b31b0f1d57eaf1190f`。后续 #4879 已通过 [#4882](https://github.com/nil-studio/oasis7/pull/4882) 合入，提供完整 legacy Scheduler 具体合同与独立客户端 profile，并将宿主归档改为内核事件借用视图和稀疏 runtime-event 附属数据，不再长期保留旧内核模型。固定本地负载下恢复后的宿主存活请求分配约降 54%；这是特定 dev profile 的分配器测量，不能推断线上 RSS 降幅。实际合入、完整测试与测量范围仍以该任务及其实现 PR 为准。不将结构拆分提升为新游戏能力、完整 Runtime WASM 执行或部署验收。其他项目及环境的核验时间不变。 当前本地候选继续落实 [Viewer 应用与宿主边界](../../../world-simulator/viewer/viewer-minimal-system.design.md#应用与宿主边界)、Runtime/Service 的 CAS/Formal Protocol 直接消费及客户端合同归位，尚未合入。

局部文档更新（2026-10-12，仅 [#4438](https://github.com/eng-cc/oasis7/issues/4438)）：以 `27db6f894bd106e9a4fee9ee3062cd8fd86e1056` 核对 L4 工具链，补齐[构建/驱动、实际 Hosted 会话及结果合同](../../testing/prd.md#l4-build-driver)与[事实/体验判定边界](../../testing/prd.md#l4-evidence-judgment)，同步旧试玩入口。该 Issue 已以 `not_planned` 关闭，作为历史文档承接来源，不再承担新的 runner/scaffold 修复或验收义务；本次没有新试玩或部署结论，不刷新其他条目的核验时间。

| 来源 | 本轮基线 |
| --- | --- |
| 仓库文档与代码 | main [`598b308a23f3f0189843618d14abb1aa31c0d2de`](https://github.com/eng-cc/oasis7/tree/598b308a23f3f0189843618d14abb1aa31c0d2de)，提交于 2026-10-11 18:40:57 +08:00 |
| GitHub 可变记录 | 2026-10-11 18:52–18:54 +08:00 读取；#4363 最新评论更新于 18:51:33；PR 状态与代码基线分开表达 |
| 最近变化窗口 | 上述 main 基准前的 30 条提交，事件时间覆盖 2026-10-10 04:18:00 至 2026-10-11 18:40:57 +08:00；选取下文 8 项有意义变化 |
| 历史局部补充 | 2026-10-12 核对 [PR #4624](https://github.com/eng-cc/oasis7/pull/4624) 的本机工具候选与 `affa7d1` arm64 同包 App 验证，当时未合入；2026-10-14 的合入摘要见下文，不前移上述全局或游戏/部署来源窗口 |
| 未覆盖 | 全部历史 PR、全部项目任务、当前线上拓扑/健康、真实故障域演练、同候选真人闭环；未重新运行上述 2026-10-11 窗口各 PR 记载的验证 |

本页的仓库相对链接用于继续阅读当前主责正文；需要复核本轮结论时，按上面的固定提交查看对应路径。
GitHub 记录可能晚于代码基准，未合入分支不计入 main 已实现能力。
最近 30 条提交是明确的取样窗口，不代表全量项目快照，也不是“自上次查看以来”的完整变化。
后续刷新沿[时间与缺失处理](command-center.manual.md#cc-maintenance-freshness)保留各来源自己的核验时间。

<a id="cc-status-goals"></a>

## 长期方向与当前阶段

长期方向是让玩家通过 Agent，在同一持续世界中形成有代价、可追溯的影响。
成长围绕“建立可恢复能力、服务区域需求、取得有限区域影响”展开；文明尺度共同项目是自愿扩展。
长期自治与文明涌现仍需验证，不能从代码规模或文档采纳推断已实现。来源：[产品承诺](../../product/world-rules-core-gameplay/prd.md#1-产品承诺)、[项目定位](../../../README.md#oasis7)。

| 产品域 | 长期用户结果 | 主责入口 |
| --- | --- | --- |
| 世界规则与玩法系统 | 玩家行动产生可归因成果，并形成新的能力、区域价值和下一阶段方向 | [产品承诺与路线图](../../product/world-rules-core-gameplay/prd.md#4-路线图) |
| 权威世界基础设施 | 同一世界保持唯一、确定、持久、可验证，并可沿原历史恢复 | [基础设施承诺](../../product/world-infrastructure/prd.md#1-产品承诺) |
| 智能体、世界模拟与交互 | Agent 理解并行动，玩家能观察、指导、纠正和接续其工作 | [智能体产品承诺](../../product/agents-world-simulation/prd.md#1-产品承诺) |
| 玩家接入与发行 | 用户能了解、进入并持续使用有证据支持的产品路径，清楚访问和发行边界 | [四域职责](../../product/README.md#四大产品模块)、[公开状态](../../../README.md#项目状态) |

**当前阶段目标**：受控真实玩家通过有界授权 Agent，在同一持久世界取得真实首产物，
理解并恢复生产阻塞、保留成果，并在再次进入后继续原目标。来源：[core 当前阶段](../../core/prd.md#当前阶段收口优先级stage-closure-backlog)。

代表性路径为 `starter-industrial-smelter-to-assembler-v1`：
Smelter 合法建成 → 铁锭生产 cycle → owner-bound output ledger 实际正量结算 → 开放 Assembler 候选。
首产物采用 `production_only`；持续生产能力、交付、真人理解和回访分别验收。
来源：[本阶段交付](../../../README.md#本阶段交付)、[生产与交付边界](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-004)。

<a id="cc-status-progress"></a>

## 当前主线与进展

以下主线消费既有里程碑和实际交付记录，不在这里重新排序或建立一套任务状态。
[#4363](https://github.com/eng-cc/oasis7/issues/4363) 已以 `not_planned` 关闭，只是历史规划/文档来源；当前能力和限制回到对应主责文档与实际 PR/验证记录，不恢复旧 Issue 的剩余义务。
“已有代码”“适用验证通过”“正式开放”“真人接受”分别表达，不用 PR 数量换算项目完成率。

### 1. 持久单权威与同世界恢复

2026-10-21 节点生命周期局部核对，源码基线 `b42a9e3fb8c8cd60679936c5f46c2fde6f62cf1e`：正式受控 writer、原请求恢复、公开投影与完整只读同步已有实现入口，准确支持范围见[节点能力与操作入口](../../p2p/blockchain/local-node-lifecycle.runbook.md#daily-commands)和[持久世界实施边界](../../p2p/blockchain/persistent-world-node-lifecycle.runbook.md)。这些代码和历史局部验证不等于真实独立故障域、完整消费者恢复或正式开放已验收；本轮没有部署、恢复演练或新运行结果。

本次文档把[部署角色与状态路径](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#node-deployment-journey)、[全状态恢复及容量](../../p2p/design.md#des-p2p-full-sync-checkpoint-capacity-v1)、[授权与撤销](../../p2p/blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-read-authorization)、[发现来源与记录生命周期](../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-evidence-boundary)和[监控新鲜度](../../p2p/node/node-triad-operations-observability.design.md#report-freshness)接回主责合同。发现来源的本地实现候选已分存原签名与本地 seed/DHT/RR 观察，未知控制域保持降级并计入会话配额；[实现边界](../../p2p/network/mainnet-private-reachability-architecture.design.md#discovery-peer-record-implementation-boundary)已接入 v1 TTL／未来偏差、进程内时钟与转发／拨号租约候选，仍保留持久可信时间、序号和受信归属映射缺口，自动拨号现收敛到有效记录或精确本机配置，未知中间路由等待新依据；候选不代表部署。新增持久防回退、撤销完成及故障验收规则仍待实现；后续按[受控故障域验收](../../p2p/design.md#des-p2p-controlled-fault-domain-acceptance-v1)证明原世界与已确认结果保留，不能以同机双目录代替独立硬件故障域。

正式 profile 的实际部署仍暂缓；开放条件继续由[基础设施当前阶段](../../product/world-infrastructure/prd.md#world-infrastructure-current-stage)持有。[#4363](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)只保留为历史来源，本段不刷新其他主线或环境的核验时间。

存储实现边界核对（2026-10-19）：[PR #4870](https://github.com/nil-studio/oasis7/pull/4870) 已合入原字节共享压缩、分批物理代与安全回收、完整 Snapshot 增量、Merkle v2、任务控制冷链及原生工件闭包，所测版本的同机进程恢复与长历史检查通过。固定小规模对象命名空间样本有空间收益及读取成本，准确版本和分布见实施 PR。后续验证由[手动存储专项 CI](../../world-runtime/runtime/formal-world-storage-efficiency.manual.md#lws-manual-ci)组织核心存储、同机网络故障模型和可选容量档位，运行是否通过以各次完整结果为准。当前无部署节点，S6 真实独立双故障域、更大规模及其他未测指标不因代码合入或某轮 CI 绿色取得验收；本段不刷新其他主线或环境基线。 [PR #4891](https://github.com/nil-studio/oasis7/pull/4891) 当前候选补双端 pin 释放恢复、有预算的默认物理维护与分页收敛，并测同一正式 writer 完整目录维护前后的净占用；短样本仍净增长，不能将对象表示收益当作完整节点节省，最终验收及长跑结果以该 PR 为准。

### 2. 真实首产物、阻塞恢复与回访

starter 候选、owner 目标持久化等前置已有代码；Web guest 启动/只读世界推进和 Hosted 修复分别已合入。
同一候选、同一正式世界上的实际生产、授权纠偏、阻塞恢复与目标接续尚未组合验收。
下一步核对实际 owner/Agent 入口与首产物链路；guest 启动不能代替 owner onboarding 或游戏操作。
来源：[成果承接持续能力](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#req-first-industrial-008)、[#4363](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)、[#4566](https://github.com/eng-cc/oasis7/pull/4566)。

2026-10-12 合同收敛：[当前首产物与后续专题](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#starter-profile-applicability) 分别确定适用验收；[同世界逐账号首次绑定与返回](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#player-session-first-agent-entry) 与 [service 首聊权威状态](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-chat-005) 补齐后继目标。固定 starter 注册与缺余额投影的现实现仍待接线验证，不能用空世界单账号或本地完整状态测试代签第二玩家、未知状态及跨入口首局。

### 3. 当前验证与消费者收口

[#4568](https://github.com/eng-cc/oasis7/pull/4568) 已合入，自身最终 CI 的通过范围可追溯。
观测时 [#4571](https://github.com/eng-cc/oasis7/pull/4571) `b251e295` 未合入、新版本 CI 待完成；
依赖它的 [#4567](https://github.com/eng-cc/oasis7/pull/4567) `8ea69adb` 与 [#4569](https://github.com/eng-cc/oasis7/pull/4569) `4e94a2e6` 仍为草稿。
下一步由原负责人处理实际新源验证与合入；旧绿色结果不替代新版本，也不因此新增人工审批。

### 4. 受控真人两次会话

Web Passkey 多凭证账号把账号登录与世界控制分开，邮箱为可选凭证。2026-10-19 核对的[会话内导入合同](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#imported-key-web)允许已绑定 Ed25519 公钥挑战登录，私钥不持久化；受保护客户端生成和加密备份仍未发布，具体部署可用性须读取目标 capabilities。实现与代码验证见 [#4669](https://github.com/eng-cc/oasis7/pull/4669)，部署进度及交付阻断项见 [#4671](https://github.com/eng-cc/oasis7/issues/4671)。协议边界由 [账号设计](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md) 和 [验收手册](../../testing/manual/hosted-passkey-account.manual.md) 承接；技术预览不代签正式邀请资格。

新建独立 DevLocal 世界的身份、创世材料和 WorldService 消费者绑定由 [预览初始化合同](../../p2p/design.md#des-p2p-fresh-preview-bootstrap) 与 [操作手册](../../testing/manual/fresh-world-preview-bootstrap.manual.md) 承接。该入口不恢复旧世界或激活正式 authority profile；部署和验收结果继续在交付任务中维护。

目标和观察口径已定义；持久世界与实际体验满足首次开放条件后，才开展目标玩家两次会话观察。
本轮未取得真人完成证据。届时需区分独立完成、提示、代操作、未完成和未覆盖，并区分安排的第二次会话与自发回访。
来源：[core 受控真人验证工作包](../../core/prd.md#当前阶段收口优先级stage-closure-backlog)、[#4363 历史验收范围](https://github.com/eng-cc/oasis7/issues/4363)。

### 5. BFT 与同世界交接

顶层合同允许 BFT 与受控单权威路径并行研发，先验证同一历史，再按 H/H+1 交接转移写权。2026-10-21 局部设计补齐[轮次与锁定](../../p2p/design.md#des-p2p-bft-round-lock-v1)、[崩溃后签名安全](../../p2p/design.md#des-p2p-bft-signing-recovery-v1)和[BFT 成员生效](../../p2p/design.md#des-p2p-bft-validator-transition-v1)；BFT 内部的 h+2 成员应用与 profile 间 H/H+1 交接分别成立。
当前 PoS 原型不能据此取得正式 BFT 保证，独立验证者协议、持久证书与同世界交接尚未验收；该目标不自动成为已满足条件的受控单权威试玩前置。
来源：[基础设施路线图](../../product/world-infrastructure/prd.md#4-路线图)、[#4363 历史记录](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。

### 6. 外部游戏 Runtime 的 owner 目标控制前置

自主入场局部核对（2026-10-21）：[身份主责](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md#machine-registration)及已合入的 [#4954](https://github.com/nil-studio/oasis7/pull/4954) 承接机器身份、canonical participant 唯一绑定/迁移、独立 owner/executor、starter、任务目标与原件恢复。本地工具代理按资源隔离配置、状态和预算；实际 Runtime 沙箱与费用须由获准 profile 约束。来源与剩余验收见 [#4946](https://github.com/nil-studio/oasis7/issues/4946)；完整 OpenClaw/Codex 冷入场仍未验收，本次未刷新线上开放范围。

准入职责拆分（2026-10-21 局部设计核对）：[准入合同](../../game/gameplay/gameplay-agent-claim-economy-contract.design.md#des-claim-entry-001)明确官方与外部客户端共用接入协议；DevLocal 的显式本地运营者授权与正式网络治理授权分别验证，不以缺治理历史要求重建现有世界。实现候选通过独立历史、恢复前上下文和节点写锁内提交承接；配置见 [ECS 手册](../../testing/manual/single-ecs-preview-deploy.manual.md#devlocal-世界的参与者准入授权)。候选尚待验证与合入，不表示已部署、已开放完整玩法或完成受限 Runtime 实玩。

世界舞台局部更新（2026-10-17）：主体绑定、Web 恢复及探索切片已在 [PR #4825](https://github.com/nil-studio/oasis7/pull/4825) 合入；远端来源、设施/路线及视口索引已在 [PR #4834](https://github.com/nil-studio/oasis7/pull/4834) 合入。本轮继续补齐增量/删除通知、工业聚合和显式事件锚点，当前尚未合入。功能状态为已实现，待完整测试；正式独立试玩仍缺私有世界/GameClient 配置，原生容器和容量实测未完成。用户已要求保留试玩缺口推进合入，不表示整体验收或部署完成。范围见[产品摘要](../../product/agents-world-simulation/player-readable-world-stage.prd.md#stage-implementation-status)，不刷新其他核验时点。

局部摘要核对于 2026-10-14，仅本项：owner 私有目标控制、原生 fenced 执行、物流、权威配方及 prepared claimed Agent 建厂到首产物的代码切片已交付。实际游玩仍限定各自候选与 prepared-world 场景；失败、超时、cleanup 和普通 CI 修复分别回到 [执行 PR](https://github.com/eng-cc/oasis7/pull/4628#issuecomment-6162407180)、[物流 PR](https://github.com/eng-cc/oasis7/pull/4634#issuecomment-6162407791)、[生产 PR](https://github.com/eng-cc/oasis7/pull/4650#issuecomment-6162408442)、[建厂链 PR](https://github.com/eng-cc/oasis7/pull/4662#issuecomment-6162409087)。不将这些 targeted 结果提升为完整首局、两 Runtime parity、GUI 或生产部署验收。

完整协作 TaskSnapshot、身份注册/配对及公开 Game HTTP 写入口尚未由这些切片证明；部署暂停继续有效。支持范围与证据入口见 [Game API 专业合同](../../world-simulator/llm/external-agent-game-api.design.md#当前实施候选原生任务执行链)和[协作与私有目标合同](../../world-runtime/runtime/player-runtime-collaboration.design.md#6-接口与数据合同)。本次不刷新全局和环境核验基线。

2026-10-15 局部候选核对：[#4734](https://github.com/eng-cc/oasis7/pull/4734)已合入 `25b4d211a612d783172e6266307c71c62b8c00c0`，完成共享客户端抽离；[工业任务执行器候选](https://github.com/eng-cc/oasis7/tree/50e128cbe369f8f6adca6556ff99784c7f7c93ad)由现有节点组合启动，复用受权客户端并显示运行位置、预算和阻断状态。执行器由 [#4744](https://github.com/eng-cc/oasis7/pull/4744)继续迁移，真实 HTTP 脚本工业链、独立 CLI/SDK 建厂对照与重启恢复已通过，执行器候选尚未合入；真实模型、完整生产能力迁移、性能比较和整体验收仍未完成。执行器不取得世界权威，关闭 Viewer 不代表停用节点进程，也不代表云端托管；2026-10-15 用户明确当前以 Codex 等外部 Agent 接入为主要入口，自有 Agent 入口打磨与真实模型测试留作后续 TODO，本轮未发出付费请求；剩余验证与消费者迁移仍由 [#4726](https://github.com/eng-cc/oasis7/issues/4726)跟踪。本项不刷新其他项目或部署事实。

<a id="cc-status-world-policy-contracts"></a>

### 7. 制造、保供与组织连续性的设计基线

2026-10-19 局部核对，仅覆盖下列三个 Issue 及 `main@e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6` 的相关设计和实现入口。文档补齐首版政策及技术合同，服务于额外 Agent 成长、危机中的有限连续运行和组织权利保护；不改变当前首产物主线或公开阶段。

| 主题 | 产品与系统入口 | 当前边界与后续承接 |
| --- | --- | --- |
| 额外 Agent 制造 | [制造规则](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004) · [订单与原子交付](../../world-runtime/runtime/agent-industrial-manufacturing.design.md) | 配方、成本和产能由专业政策持有；订单到新身份/控制权的生产接线及验证仍由 [#4850](https://github.com/nil-studio/oasis7/issues/4850)承接。 |
| 紧急保供 | [保供规则](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md) · [授权、批次与结算](../../world-runtime/runtime/emergency-supply-authorization-and-settlement.design.md) | 首版触发、优先级、窗口和补偿退出已在设计中明确；业务实现和同候选验收仍由 [#4866](https://github.com/nil-studio/oasis7/issues/4866)承接。 |
| 组织连续性 | [组织保护规则](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md) · [处置与授权交接](../../world-runtime/runtime/organization-continuity-and-disposition.design.md) | 活动、保护、接替、资产和部分交接政策已在设计中明确；业务实现和同候选验收仍由 [#4868](https://github.com/nil-studio/oasis7/issues/4868)承接。 |

三项均保留原有已实现子范围，文档覆盖不改变实现或验收状态；未取得新的业务运行、部署或真人证据。其他项目与环境的核验时间不变。

<a id="cc-status-changes"></a>

## 最近做了什么

下列 8 项均已由 GitHub 的实际合入状态与 merge commit 核对，且出现在固定 main 的最近 30 条提交中。
时间为北京时间（+08:00）；验证描述引用各 PR 记录，本次没有重新运行，也没有把合入解释为部署。

| 合入时间 | 变化及对当前工作的价值 | 事实边界与来源 |
| --- | --- | --- |
| 10-11 18:40:57 | 指挥面板读取文档 lifecycle/successor/reason，保留历史并定位明确继任条款 | 声明不自动取得规范权威；无法唯一、安全定位时仍 unknown。 [#4572](https://github.com/eng-cc/oasis7/pull/4572)，`598b308a` |
| 10-11 16:34:37 | 修复 Hosted 完成唤醒与已提交请求重放，避免重复执行和持久化 | 自身最终验证不代替后来 CLI/report 组合源；部署仍暂缓。 [#4568](https://github.com/eng-cc/oasis7/pull/4568)，`fa56ad2e` |
| 10-11 12:02:27 | 修复发行 Web 启动后端合同，验证实际 WASM 渲染与签名公开世界读取推进 | 覆盖隔离 QA guest/configuration 路径，不证明 owner onboarding、正式世界激活或真实游戏完成。 [#4566](https://github.com/eng-cc/oasis7/pull/4566)，`29f380b5` |
| 10-11 11:54:03 | 增加 owner-signed GameTaskCreate，经 World Service/consensus 持久化任务控制 | 初态 suspended，无目标或 executor；完整 context、action admission、配对未包含。 [#4565](https://github.com/eng-cc/oasis7/pull/4565)，`543120f8` |
| 10-11 11:46:12 | 指挥面板加入默认关闭的实验 Codex runtime，支持显式连接/输入/中断及原请求身份恢复 | 固定 CLI 0.137.0；真实握手/receipt 有记录，但模型 turn 返回 `other`，没有成功模型输出证明。 [#4563](https://github.com/eng-cc/oasis7/pull/4563)，`7828a2a0` |
| 10-11 10:28:14 | Game API 公布 OpenAPI 3.1 及已实现签名读取能力与限制 | 只读 bootstrap 不等于完整网络游戏行动闭环；后续 task-control 增量见 #4565。 [#4562](https://github.com/eng-cc/oasis7/pull/4562)，`679ba9ed` |
| 10-11 01:17:05 | 本地 signer 增加 Ed25519 生命周期、轮换、加密导入导出及恢复管理 | 代码与测试交付；没有真实密钥/策略操作或 host 升级。 [#4539](https://github.com/eng-cc/oasis7/pull/4539)，`9341adc8` |
| 10-11 00:04:47 | 指挥面板桌面基础实现：Tauri/Rust/SQLite/Solid，读取项目来源、有限 PR 窗口并保存阅读/决策 | 有 macOS arm64 本地打包和原生验证；公证/公开分发、旧系统/x86_64/VoiceOver 未验收。 [#4533](https://github.com/eng-cc/oasis7/pull/4533)，`2b334ed3` |

[#4518](https://github.com/eng-cc/oasis7/pull/4518) 是指挥客户端产品/系统设计来源；
后续 #4533、#4563、#4572 已有实现，不能继续把客户端整体描述为“仅方案”，也不能据此断言全部设计完成。
按事件时间阅读 PR：#4533 当时尚未实现的 runtime，后来由 #4563 增量交付。

### 本次候选增量（2026-10-12）

2026-10-14 局部摘要：[PR #4624](https://github.com/eng-cc/oasis7/pull/4624) 已合入，新增内置私有资料、同构建可选 CLI、兼容既有 signer 的文件签名接入，以及显式私有开发会话输入；普通本机工具边界见[当前设计](local-tools.design.md)。
本次在 `affa7d1` 的 macOS arm64 同包原生 App 核对了真实建库、条件保存冲突后的草稿保留和 CLI 安装。
这些验证结果只覆盖原候选与所列场景；PR 合入事实见上述 2026-10-14 局部摘要，不能据此把旧候选验证扩大为后来 main、部署、公共发行或全部本机工具验收。

管理员桥与 Apple 签名集成由用户明确暂停；本机用户尚未绑定 signer，真实签名提交未验收。
该候选不执行特权安装、custody 管理或生产变更，原有游戏、部署及其他 PR 的核验时点保持不变。
来源：[候选 PR 与验证范围](https://github.com/eng-cc/oasis7/pull/4624)、[暂停范围与验收边界](local-tools.design.md)。

### 指挥客户端：日常闭环与可读性改造

局部核对基线为 `main@822f6b49d1a5fb2b866b6c6902ffc8ed024d043b`（2026-10-18）。[PR #4881](https://github.com/nil-studio/oasis7/pull/4881) 已交付首页四区、共享工作详情、关联修复、决定同步与历史、原工具定位、一次明确发起和有界来源读取；[PR #4885](https://github.com/nil-studio/oasis7/pull/4885) 按用户人工 IME 确认收口 [#4848](https://github.com/nil-studio/oasis7/issues/4848)。原生任务、受控协议及构建范围见[验证记录](daily-work-verification.manual.md)，其结果不代表后续设计已实现。

当前改造目标是降低文字与操作的同时暴露量，使用户从清晰的重点、成果、待答和已有工作进入同一详情。默认信息层级、图形表达、阅读密度和任务验收由[中文与清晰阅读要求](command-center.prd.md#REQ-CC-010)及[首页要求](command-center.prd.md#REQ-CC-013)拥有；[系统设计](command-center.design.md#cc-visual-composition)落实布局、组件和投影边界。现行决定只保存个人内容、理由、范围、正式文档关联及确认/替代历史；统一工作列表承接PR，来源区只管理连接、账号、读取范围与错误。实际功能及未证明范围以[当前系统设计](command-center.design.md#cc-current-implementation)为准，旧版交付结论保留原范围。

本段只刷新客户端主题；VoiceOver 保持用户明确排除，Apple 签名/管理员集成的已有暂停继续有效，未增加模型、部署或其他环境验证事实。

### 文档全局树设计（2026-10-13，文档范围）

2026-10-15 候选源码核对：[REQ-CC-016](command-center.prd.md#REQ-CC-016) 的工作树文件清单与逐项覆盖已有 [Core 查询](../../../tools/oasis7-command-center/core/src/document_inventory.rs)及[规范视图](../../../tools/oasis7-command-center/ui/src/DocumentInventory.tsx)承接，可按标题/路径和读取结果筛选、分页，查看空文件/遗漏并进入同版本章节。清单与章节/搜索同事务发布，旧缓存缺失或损坏不删除个人数据；能力边界见[专题设计](document-tree.design.md#111-验证映射表)。详情已展示文件头“配对系统设计”链接、声明行和文件级读取诊断，跨行/超预算字段标记解析不完整，旧缓存提示刷新；带锚点目标区分唯一、缺失和歧义，可打开同摘要目标章节并返回原按钮；设计文件头的精确“上游”字段另组展示需求文档与设计引用，保留声明行和诊断，正文承接表及树导航的当前范围见下文。这不提供固定提交基线或独立树快照。真实桌面的实际候选、环境、验收范围和合入事实由实现 PR 承载，未据此刷新其他项目/部署事实；完整 AC-CC-016 仍待验收。

<a id="cc-status-attention"></a>

## 需要人关注

### 需要决定

在本轮已读取的范围内，没有发现新增且已核实的即时待批准事项；当前 CI 等待和普通修复继续由原负责人推进。
后续若改变阶段目标、正式开放范围或部署安排，应回到对应主责条款和已有操作入口形成具体决定。
这里记录真实问题，不把正常 CI、文档刷新或已有授权工作包装成新的许可请求。来源：[开发流程边界](../workflow/source-of-truth.md#1-适用范围与规则边界)、[#4363 历史安排](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。

持久世界部署优化的代码候选见 [PR #4683](https://github.com/eng-cc/oasis7/pull/4683) 和[架构索引](architecture/README.md#cc-architecture-authority)，2026-10-15 核对为待合入。当前授权范围为代码与本机验证；小遗留按[实施文档 TODO](../../p2p/blockchain/persistent-world-node-lifecycle.runbook.md)跟踪，真实部署、完整产品及自主试玩验收仍未完成。

### 待补事实

| 需要补齐的信息 | 为什么影响决策 | 处理入口 |
| --- | --- | --- |
| 正式部署候选、环境、故障域、实际保留/恢复与旧写者隔离结果 | 决定能否从代码前置进入正式持久世界开放；当前没有线上状态核验 | [架构索引](architecture/README.md)、[#4363](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860) |
| 同候选首产物、阻塞恢复、目标接续与真实玩家观察 | 决定邀请范围和是否继续、修复或收窄；局部实现与 guest 启动不足以回答 | [当前阶段](../../core/prd.md#当前阶段收口优先级stage-closure-backlog)、[首局验收](../../product/world-rules-core-gameplay/first-session-and-continuation.prd.md#ac-first-industrial-004) |
| #4571 及消费者的新源 CI/合入结果 | 更新代码进展，识别仍存在的具体回归；这属于后续取证，不是新增审批 | [#4571](https://github.com/eng-cc/oasis7/pull/4571)、[#4567](https://github.com/eng-cc/oasis7/pull/4567)、[#4569](https://github.com/eng-cc/oasis7/pull/4569) |

### 知悉变化

- **玩法权威协议有了独立设计承接。** 2026-10-19 局部设计核对基于 `e4bfdb8d2f03a3ccdc4a61a622881c19dbd272c6` 及 8 个原 issue；[专题入口](../../world-runtime/prd.index.md#gameplay-authority-protocols)补普通治理、工业工作/配方、charter、替代转换、levy、外部性、贡献恢复和安全确认的逻辑记录、提交与恢复边界。当前增量是系统设计，原实现状态与验收缺口继续有效；费率、资格阈值和上线参数仍由各自主责按专题依赖确定，未取得当日部署或玩法验收证据。
- **部署暂停已经决定。** 不创建正式世界、生产密钥或启动实际故障域演练；已有授权的代码、验证、评审及正常合入继续。来源：[已有用户限制](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6097891450)、[最新延续](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。
- **游戏 Agent 与开发会话分开看。** Game API 的读取/发现/空任务切片不证明 OpenClaw/Codex 完成游戏自主首局；历史指挥面板实验 runtime 也不是游戏 Runtime 验收。来源：[外部 Runtime 产品目标](../../product/agents-world-simulation/prd.md#external-runtime-play)、[真实首期完成要求](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-010)、[#4563](https://github.com/eng-cc/oasis7/pull/4563)。
- **架构方案与实际部署分开看。** 当前应持续查阅[架构索引](architecture/README.md)中的主责合同与部署核验入口；设计采纳、代码合入、环境验证和公开发行分别成立。来源：[基础设施事实边界](../../product/world-infrastructure/prd.md#53-事实边界与未决承接)。

下一次维护按[持续更新流程](command-center.manual.md#cc-maintenance-steps)增量核对上述变化与未知项。
已解决事项移出当前关注，历史通过 Git 和原 PR 追溯；没有实质变化时不制造新的提交。

文件清单支持按已读文件头显式文档 ID 检索；原声明/源行与有限重复诊断沿[文档树设计](document-tree.design.md)展示。同 ID 文件不合并，重复范围限同次已读候选，不代表仓库全量唯一或实现验收；旧清单需刷新。明确正文承接表可按行查看两侧链接、原行样本和同版本章节；仅覆盖精确标题/列名，超限与歧义有诊断，不证明语义承接完整。目录层级支持按路径逐层/分页浏览 PRD、设计与结构入口，搜索保留祖先，文件详情复用同版本来源；不推断语义归属。产品树从四域入口表与根 PRD 活跃专题展开，专业系统树从工程模块矩阵与明确模块导航展开；多父入口保留，匹配按文件去重，未归属与断链/未读/循环诊断可见，详情仍沿同版本章节。导航不提升为配对或主责，两树支持按需展开、键盘操作及独立个人阅读恢复；支持范围见[显式导航投影](document-tree.design.md#显式产品与专业导航投影)。独立树快照及固定提交基线仍待补齐，完整验收范围归专题设计。
