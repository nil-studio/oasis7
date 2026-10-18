# oasis7 项目状态与指挥入口

本页帮助项目负责人在五分钟内看清长期方向、当前主线、已交付变化和需要关注的边界。
它是带来源和核验时间的滚动摘要；目标、规范、任务、部署和验收继续由原有主责文档及实际记录承载。

**当前判断：项目仍处于受限可玩技术预览。**
主线是同一持久世界中的首产物、阻塞恢复和回访接续；代码前置持续交付，正式开放与真人闭环尚未验收。
正式持久世界的部署仍按已有用户决定暂缓；独立 ECS 技术预览的部署与验收进度由 [交付任务 #4671](https://github.com/eng-cc/oasis7/issues/4671) 承接，不在本页维护执行日志。来源：[公开阶段](../../../README.md#项目状态)、[当前里程碑](../../core/prd.md#当前阶段收口优先级stage-closure-backlog)、[#4363 历史进展](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。

| 想了解什么 | 入口 |
| --- | --- |
| 下次如何持续更新 | [持续更新流程](command-center.manual.md) |
| 整体分层、部署、存储、权限与恢复 | [顶层架构索引](architecture/README.md) |
| 指挥客户端的产品目标与技术方案 | [产品设计](command-center.prd.md) · [系统设计](command-center.design.md) |
| 如何运行和操作已有客户端 | [客户端使用说明](../../../tools/oasis7-command-center/README.md) |

<a id="cc-status-baseline"></a>

## 核验基线与覆盖范围

文档局部收敛（2026-10-12，仅 #4363）：以 `main@e01e29064517f4bf375767a19a09802335828249` 核对首局、首次绑定/首聊及持久权威合同，本次变更补齐下文第 1、2 项的目标设计与验收映射。新增目标尚待实现及适用验收；未取得新的部署或真人结果，其他条目的核验时点不变。

局部增量（2026-10-12，仅 Cargo 拆分）：[#4626](https://github.com/eng-cc/oasis7/pull/4626) 已合入 `133d70e45b71bd062032a734d68be1e290692b2c`，统一生产 cognition memory/continuation 核心并保留 Runtime authority；对应 main Rust [38212520926](https://github.com/eng-cc/oasis7/actions/runs/38212520926) 已成功。本分支正在补支持包 profile 选测隔离，尚未合入或完成 V0。用户确认提速收益已验证；实际自主试玩明确不在此次 Cargo V0 验收范围，不据此宣称实玩通过。该增量不刷新下面其他项目/部署事实的核验时间。

<a id="cc-status-core-crate-extraction"></a>

核心 crate 局部增量（2026-10-18，仅 [#4853](https://github.com/nil-studio/oasis7/issues/4853)）：共享 API 与准入归属整理 [#4865](https://github.com/nil-studio/oasis7/pull/4865) 已合入 `665183c94c4d1c5359dfa6b4234db08b4d55bf34`，原生 Simulator 与宿主兼容适配 [#4871](https://github.com/nil-studio/oasis7/pull/4871) 已合入 `3c86ecd24657ee05556694b5ed0d41a470cf7b8e`。完整 Runtime / World Service、客户端证明消费及消费者迁移由草稿 [#4873](https://github.com/nil-studio/oasis7/pull/4873) 承接；本轮核对候选为 `97d3eb99f8cfb09460b8b47940c28c5cc8425b07`，仍在验证、尚未合入。剩余回归与同条件编译影响测量以该 PR 为准，不预设提速比例，不将结构拆分提升为游戏能力、完整 Runtime WASM 执行或部署验收。其他项目及环境的核验时间不变。

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

已有 manifest、journal/replica、离线 activation verifier、受保护本地工程执行等前置交付。
正式 initial activation 与普通消费者接线、不完整候选崩溃恢复、独立故障域保留/恢复、旧写者隔离和人工交接仍待完成。
下一步先在已有代码与验证范围内收口缺口；正式 profile 的实际部署仍暂缓、开放验收尚未完成，不能把本地双目录当成独立硬件故障域。
来源：[基础设施当前阶段](../../product/world-infrastructure/prd.md#world-infrastructure-current-stage)、[#4363 历史条件](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。

2026-10-12 目标设计收敛：[原候选恢复](../../p2p/design.md#des-p2p-original-candidate-recovery-v1) 固定执行前完整材料与原请求重放；[正式权威 v1](../../p2p/design.md#des-p2p-formal-controlled-authority-v1) 和 [epoch 接管](../../p2p/design.md#des-p2p-controlled-epoch-takeover-v1) 明确激活、普通证明消费者及旧写者隔离顺序。下一步按这些主责合同实现与验证；现有工程证明、离线 verifier 和本地双目录仍不足以完成正式开放。

存储优化局部候选核对（2026-10-18，仅 [PR #4870](https://github.com/nil-studio/oasis7/pull/4870)）：原字节共享压缩、分批物理代与安全回收、完整 Snapshot 增量、Merkle v2、任务控制冷链及原生工件闭包已在该候选实现并完成局部验证，长历史进程门禁仍待收口。固定小规模对象命名空间样本已测得显著空间收益，同时有读取成本；准确版本、收益与分布见实施 PR。当前没有部署节点，真实独立双故障域与更大规模验收未执行。候选尚未合入，不扩大 main 的已交付或部署结论；下一步按[当前实施与退出门槛](../../world-runtime/runtime/formal-world-storage-efficiency.design.md#current-storage-implementation)修复并完成适用验证，本段不刷新其他主线或环境的核验时间。

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

Web Passkey 多凭证账号把账号登录与世界控制分开，邮箱为可选凭证。实现与代码验证见 [#4669](https://github.com/eng-cc/oasis7/pull/4669)，部署进度及交付阻断项见 [#4671](https://github.com/eng-cc/oasis7/issues/4671)。协议边界由 [账号设计](../../p2p/blockchain/hosted-public-join-key-based-onboarding.design.md) 和 [验收手册](../../testing/manual/hosted-passkey-account.manual.md) 承接；技术预览不代签正式邀请资格。

新建独立 DevLocal 世界的身份、创世材料和 WorldService 消费者绑定由 [预览初始化合同](../../p2p/design.md#des-p2p-fresh-preview-bootstrap) 与 [操作手册](../../testing/manual/fresh-world-preview-bootstrap.manual.md) 承接。该入口不恢复旧世界或激活正式 authority profile；部署和验收结果继续在交付任务中维护。

目标和观察口径已定义；持久世界与实际体验满足首次开放条件后，才开展目标玩家两次会话观察。
本轮未取得真人完成证据。届时需区分独立完成、提示、代操作、未完成和未覆盖，并区分安排的第二次会话与自发回访。
来源：[core 受控真人验证工作包](../../core/prd.md#当前阶段收口优先级stage-closure-backlog)、[#4363 历史验收范围](https://github.com/eng-cc/oasis7/issues/4363)。

### 5. BFT 与同世界交接

顶层合同允许 BFT 与受控单权威路径并行研发，先验证同一历史，再按 H/H+1 交接转移写权。
独立验证者协议、持久证书与同世界交接尚未验收；它不自动成为已满足条件的受控单权威试玩前置。
来源：[基础设施路线图](../../product/world-infrastructure/prd.md#4-路线图)、[#4363 历史记录](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。

### 6. 外部游戏 Runtime 的 owner 目标控制前置

世界舞台局部更新（2026-10-17）：主体绑定、Web 恢复及探索切片已在 [PR #4825](https://github.com/nil-studio/oasis7/pull/4825) 合入；远端来源、设施/路线及视口索引已在 [PR #4834](https://github.com/nil-studio/oasis7/pull/4834) 合入。本轮继续补齐增量/删除通知、工业聚合和显式事件锚点，当前尚未合入。功能状态为已实现，待完整测试；正式独立试玩仍缺私有世界/GameClient 配置，原生容器和容量实测未完成。用户已要求保留试玩缺口推进合入，不表示整体验收或部署完成。范围见[产品摘要](../../product/agents-world-simulation/player-readable-world-stage.prd.md#stage-implementation-status)，不刷新其他核验时点。

局部摘要核对于 2026-10-14，仅本项：owner 私有目标控制、原生 fenced 执行、物流、权威配方及 prepared claimed Agent 建厂到首产物的代码切片已交付。实际游玩仍限定各自候选与 prepared-world 场景；失败、超时、cleanup 和普通 CI 修复分别回到 [执行 PR](https://github.com/eng-cc/oasis7/pull/4628#issuecomment-6162407180)、[物流 PR](https://github.com/eng-cc/oasis7/pull/4634#issuecomment-6162407791)、[生产 PR](https://github.com/eng-cc/oasis7/pull/4650#issuecomment-6162408442)、[建厂链 PR](https://github.com/eng-cc/oasis7/pull/4662#issuecomment-6162409087)。不将这些 targeted 结果提升为完整首局、两 Runtime parity、GUI 或生产部署验收。

完整协作 TaskSnapshot、身份注册/配对及公开 Game HTTP 写入口尚未由这些切片证明；部署暂停继续有效。支持范围与证据入口见 [Game API 专业合同](../../world-simulator/llm/external-agent-game-api.design.md#当前实施候选原生任务执行链)和[协作与私有目标合同](../../world-runtime/runtime/player-runtime-collaboration.design.md#6-接口与数据合同)。本次不刷新全局和环境核验基线。

2026-10-15 局部候选核对：[#4734](https://github.com/eng-cc/oasis7/pull/4734)已合入 `25b4d211a612d783172e6266307c71c62b8c00c0`，完成共享客户端抽离；[工业任务执行器候选](https://github.com/eng-cc/oasis7/tree/50e128cbe369f8f6adca6556ff99784c7f7c93ad)由现有节点组合启动，复用受权客户端并显示运行位置、预算和阻断状态。执行器由 [#4744](https://github.com/eng-cc/oasis7/pull/4744)继续迁移，真实 HTTP 脚本工业链、独立 CLI/SDK 建厂对照与重启恢复已通过，执行器候选尚未合入；真实模型、完整生产能力迁移、性能比较和整体验收仍未完成。执行器不取得世界权威，关闭 Viewer 不代表停用节点进程，也不代表云端托管；2026-10-15 用户明确当前以 Codex 等外部 Agent 接入为主要入口，自有 Agent 入口打磨与真实模型测试留作后续 TODO，本轮未发出付费请求；剩余验证与消费者迁移仍由 [#4726](https://github.com/eng-cc/oasis7/issues/4726)跟踪。本项不刷新其他项目或部署事实。

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

### 指挥客户端产品收敛（2026-10-17，设计目标）

以 `main@0ec10379419b0aaedb9d16157b81c92f4e6e3495` 核对产品、系统设计与实现入口。目标仍是“首页看清重点—统一工作详情—接续原工具或自管执行—回看结果”；[产品需求](command-center.prd.md#REQ-CC-013)补齐首页字段、动作和异常状态，[关联需求](command-center.prd.md#REQ-CC-014)约定复用、修复与跨视图接续，[待处理需求](command-center.prd.md#REQ-CC-006)区分真实输入、已决定事项与同步缺口。[系统设计](command-center.design.md)承接工作投影、来源时效、持久化兼容及并发读取；工具和文档阅读分别沿[任务入口](local-tools.design.md#DES-CC-TOOLS-005)与[主题阅读](document-tree.design.md#DES-CC-TREE-004)调整。

已有原文视图、手工关联和实验 runtime 是实现基础；首页现有[来源阅读降级入口](../../../tools/oasis7-command-center/ui/src/SummarySources.tsx)仍不能替代四区投影、统一工作详情及[完整用户任务](command-center.prd.md#cc-product-acceptance)。具体范围见[差距表](command-center.design.md#1104-日常工作闭环的实现差距)。以上是设计目标与源码观察，不表示功能实现、PR 合入、桌面或性能验收，不刷新其他目标、PR 与环境事实。Apple 签名/管理员集成的已有暂停继续有效。

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

- **部署暂停已经决定。** 不创建正式世界、生产密钥或启动实际故障域演练；已有授权的代码、验证、评审及正常合入继续。来源：[已有用户限制](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6097891450)、[最新延续](https://github.com/eng-cc/oasis7/issues/4363#issuecomment-6107216860)。
- **游戏 Agent 与开发会话分开看。** Game API 的读取/发现/空任务切片不证明 OpenClaw/Codex 完成游戏自主首局；指挥面板实验 runtime 也不是游戏 Runtime 验收。来源：[外部 Runtime 产品目标](../../product/agents-world-simulation/prd.md#external-runtime-play)、[真实首期完成要求](../../product/agents-world-simulation/external-agent-runtime-play.prd.md#req-ext-010)、[#4563](https://github.com/eng-cc/oasis7/pull/4563)。
- **架构方案与实际部署分开看。** 当前应持续查阅[架构索引](architecture/README.md)中的主责合同与部署核验入口；设计采纳、代码合入、环境验证和公开发行分别成立。来源：[基础设施事实边界](../../product/world-infrastructure/prd.md#53-事实边界与未决承接)。

下一次维护按[持续更新流程](command-center.manual.md#cc-maintenance-steps)增量核对上述变化与未知项。
已解决事项移出当前关注，历史通过 Git 和原 PR 追溯；没有实质变化时不制造新的提交。

文件清单支持按已读文件头显式文档 ID 检索；原声明/源行与有限重复诊断沿[文档树设计](document-tree.design.md)展示。同 ID 文件不合并，重复范围限同次已读候选，不代表仓库全量唯一或实现验收；旧清单需刷新。明确正文承接表可按行查看两侧链接、原行样本和同版本章节；仅覆盖精确标题/列名，超限与歧义有诊断，不证明语义承接完整。目录层级支持按路径逐层/分页浏览 PRD、设计与结构入口，搜索保留祖先，文件详情复用同版本来源；不推断语义归属。产品树从四域入口表与根 PRD 活跃专题展开，专业系统树从工程模块矩阵与明确模块导航展开；多父入口保留，匹配按文件去重，未归属与断链/未读/循环诊断可见，详情仍沿同版本章节。导航不提升为配对或主责，两树支持按需展开、键盘操作及独立个人阅读恢复；支持范围见[显式导航投影](document-tree.design.md#显式产品与专业导航投影)。独立树快照及固定提交基线仍待补齐，完整验收范围归专题设计。
