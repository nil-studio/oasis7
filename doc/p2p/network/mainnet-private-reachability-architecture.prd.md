# oasis7 主链级非全公网 P2P 覆盖网络架构

- 对应设计文档: `doc/p2p/network/mainnet-private-reachability-architecture.design.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 2
建档日期: 2026-04-01

## 1. Executive Summary
- Problem Statement: 当前 triad 实测已经证明，家宽 / NAT / CGNAT / 企业内网节点无法稳定接收入站 UDP 时，现有“节点最好有公网地址”假设会直接把本机、边缘机房和大量运营环境排除在主路径之外。若继续把公网可达当成默认前提，oasis7 很难对标公共主链常见的 mixed-topology 现实。
- Proposed Solution: 冻结一套 public-chain-grade 的 P2P 目标态，把 `identity`、`addressability`、`transport`、`discovery`、`relay/overlay` 与 `consensus/data plane` 解耦，使 validator / full / storage / observer 节点在 `public / hybrid / private / relay_only / validator_hidden` 多种部署模式下都能成为一等公民。
- Success Criteria:
  - SC-1: 目标态明确支持 `public / hybrid / private / relay_only / validator_hidden` 五种部署模式，且 validator 不再以“必须有公网 IP”作为前置条件。
  - SC-2: Reachability 设计明确包含 `AutoNAT + hole punching + relay reservation + signed peer record` 四条能力，而不是继续依赖静态 UDP 邻居。
  - SC-3: 协议层明确拆出 `gossip plane / sync plane / blob-state plane / control plane`，不同链型只替换上层适配器，不重写底层可达性框架。
  - SC-4: 安全边界明确冻结 `validator core / sentry / relay / full-storage / observer-light` 的角色和密钥边界，禁止 relay/browser/operator public plane 持有长期共识 signer。
  - SC-5: 文档明确给出 anti-eclipse、anti-spam、peer diversity、relay budget 与 shared-network 验证要求，后续实现可直接据此拆任务和测门禁。
  - SC-6: 用户层默认只暴露 `2~3` 个简单模式，并由系统根据公网/NAT/打洞结果自动选择默认值；普通用户不必先理解 `validator_core/sentry/relay` 等正式角色。
  - SC-7: 后续 reachability 改造必须收敛到单一 `peer reachability contract`，由 signed peer record 与 runtime path observation 共同归一化出 `peer identity + role/deployment policy + ranked transport paths + publish/claim boundary`；`SignedPeerRecord`、`TransportPath`、network-tier manifest、status payload 与 mixed-topology matrix 不得各自重新定义 reachability class。
  - SC-8: mixed-topology 验证必须使用显式 path-behavior taxonomy，至少区分 `exact / proxy / manual_lab / real_env / unsupported` evidence class、`must_direct / may_direct_must_recover / must_relay / must_not_publish_public_direct / manual_lab_required / unsupported` expectation，以及 `public_direct / home_nat / cgnat / relay_only / sentry_loss / relay_exhaustion / degraded_latency_loss` 等 NAT/degradation labels。
  - SC-9: reachability/status observability 必须挂接到现有 `/v1/chain/status.observability` 或 triad summary，不新增第二套 net health truth；字段必须保持 bounded cardinality，并明确 byte/status scope，禁止把当前 wire/control-plane 统计解释成 NIC-level transport overhead 真值。

<a id="reachability-user-experience"></a>
<a id="2-user-experience-functionality"></a>
## 2. User Experience & Functionality
- User Personas:
  - `producer_system_designer`: 需要一套不退回 MVP、直接对标公共主链的网络目标态。
  - `runtime_engineer`: 需要知道底层 P2P core 应该抽象到什么层，哪些能力必须做成框架层，而不是继续散落在 runtime 业务里。
  - validator / sentry operator: 需要明确什么时候节点必须公开暴露，什么时候只需要 outbound reachability。
  - home / CGNAT / enterprise operator: 需要在没有公网 IP 的条件下仍然能加入网络、同步状态并承担受限角色。
  - 普通玩家 / 社区节点用户: 需要用简单模式加入网络，而不是先学完整的部署角色矩阵。
  - `qa_engineer`: 需要把 mixed-topology、relay fallback、anti-eclipse 和 shared-network 变成正式验证矩阵。
- User Scenarios & Frequency:
  - 每次有人提出“某个节点没有公网 IP，是否还能成为正式节点”时，先看本专题。
  - 每次设计 validator / sentry / relay 部署形态时，先看本专题角色边界。
  - 每次网络层计划从点状补丁进入框架重构时，先以本专题冻结目标态和非目标。
  - 每次新用户首次启动节点、切换网络环境或重新检测公网/NAT 条件时，系统都应自动重算默认模式。
  - 每次 network rehearsal / release-train / chaos drill 设计新门禁时，把本专题当作流量分层和 reachability 真值。
- User Stories:
  - PRD-P2P-024-A: As a `producer_system_designer`, I want one public-chain-grade private-reachability architecture, so that oasis7 不会因为家宽/NAT 现实被迫退回“全员公网节点”假设。
  - PRD-P2P-024-B: As a `runtime_engineer`, I want identity, discovery, transport and relay semantics separated from consensus logic, so that the framework can support different main-chain data planes without重写底层可达性。
  - PRD-P2P-024-C: As a validator operator, I want one explicit `validator core / sentry / relay` model, so that signing节点可以隐藏在私网后面，公开面只承担转发和抗攻击吸收。
  - PRD-P2P-024-D: As a private-node operator, I want hole punching / relay / overlay fallback frozen as first-class paths, so that no-public-IP 节点仍可稳定同步与参与网络。
  - PRD-P2P-024-E: As a `qa_engineer`, I want anti-eclipse, peer diversity and mixed-topology test gates defined up front, so that public-chain-grade claims are tied to evidence rather than直觉。
  - PRD-P2P-024-F: As a normal node user, I want `自动加入 / 私有安全 / 公网入口` 这类简单模式，并由系统自动给默认值, so that I can join the network without先理解底层部署角色。
- Critical User Flows:
  1. Flow-P2P-PRA-001: `private validator boot -> outbound connect to bootstrap/sentry -> reachability service classifies self as private -> reserve relay or attach sentry -> advertise signed peer record without exposing home IP -> join consensus/control lanes`
  2. Flow-P2P-PRA-002: `private full/storage node boot -> attempt direct reachability -> hole punch succeeds则升级 direct path；失败则保留 relay path -> sync plane 仍可追块/追状态 -> gossip 只按本角色订阅必要 topic`
  3. Flow-P2P-PRA-003: `public node receives peer record -> verify domain/signature/version/time/sequence -> preserve signed payload and record local discovery evidence separately -> apply diversity policy -> assign gossip/sync/data lanes -> monitor peer score and path quality -> re-route on relay loss or direct-path recovery`
  4. Flow-P2P-PRA-004: `bootstrap poisoning / eclipse suspicion -> peer manager detects discovery-source集中、ASN集中或 relay abuse -> downgrade suspicious peers -> force rebootstrap from independent anchors -> emit block signal for release gate`
  5. Flow-P2P-PRA-005: `new user boots node -> system probes public IP / NAT / port reachability / hole-punch viability -> system picks default user mode -> internal deployment_mode + node_role are derived -> if result implies public entry responsibility, UI asks for explicit confirmation before advertising that surface`
- Functional Specification Matrix:

| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算/判定规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| Peer identity 与 record | `peer_id/node_identity_key/network_domain/role_mask/reachability_class/public_addrs/relay_addrs/capability_lanes/source_operator/source_asn/published_at/ttl/sequence/signature` | 节点发布有版本、有效期和单调序号的签名 peer record；接收端保持签名载荷不变，独立维护本地观察与重放水位 | `draft -> published -> refreshed -> expired/revoked` | 域、签名、时间、序号、大小和数量边界全部通过才更新候选；同序号异载荷冲突，过期或回退拒绝；这是目标合同，当前 wire 缺少 sequence | 只有节点身份持有者可发布；operator/ASN 等来源自述不证明独立性，consensus signer 不直接暴露 |
| Peer reachability contract | `peer_id/deployment_mode/node_role/reachability_class/capability_lanes/ranked_paths/publish_policy/source_evidence/claim_boundary` | 从 signed peer record、transport path observation 与 network-tier policy 归一化出单一 reachability view，供 selection/status/matrix 投影 | `unresolved -> normalized -> selected -> stale/quarantined` | path label、rank 与 claim boundary 必须来自同一 contract；matrix/status 只能投影，不得重新定义 canonical reachability | 本地 peer manager 强制执行；远端声明不可覆盖本地 role/publish policy |
| Reachability service | `observed_addr/autonat_status/hole_punch_status/relay_reservation/path_quality` | 探测 direct、尝试打洞、预留 relay、维护路径排序 | `unknown -> direct/private/relay_only -> degraded/recovered` | direct 优先于 punched，punched 优先于 relay；不能打洞时自动降级 relay | 节点本地决策；relay 只提供转发，不授予签名权限 |
| Discovery fabric | `bootnodes/dht_namespace/rendezvous_topic/peer_record_cache/observed_sources/source_independence` | 从 bootnode、DHT、rendezvous 与静态 allowlist 聚合候选 peer，并保存由本机记录的来源证据 | `seeded -> converging -> healthy/degraded` | 至少两类本机实际观察到且控制域独立的 source 才满足多源健康要求；远端自述、同一来源转发和未知独立性不能凑数 | 静态 seed 例外只来自本机受信配置；bootnode/relay 提供发现材料，不授予世界信任锚或验证者权限 |
| Transport abstraction | `transport_id/directness/security/mux/qos_class/max_streams` | 在 direct、hole-punched、relay 路径上复用统一流接口 | `dialing -> established -> draining -> closed` | QUIC 为主、TCP/Noise 为回退；UDP 只可作为加速，不得成为唯一真值链路 | transport key 可轮换；长期 signer 不进入 transport session |
| User-visible deployment mode | `user_mode/auto_detect_result/allow_public_entry/high_value_node/override_source` | 给用户展示 `自动加入 / 私有安全 / 公网入口` 简化模式，并映射到正式角色语义 | `auto_detected -> proposed -> confirmed/enforced` | 默认必须自动选择；只有检测结果涉及 `public entry` 或高价值角色暴露面时才要求显式确认；普通路径不要求用户先理解底层正式角色 | 普通用户只能选简化模式；高级用户才可覆盖内部 role/deployment 配置 |
| Role policy | `deployment_mode/node_role/sentry_set/relay_budget/exposed_surface` | 根据角色限制订阅、入站、转发与公开面 | `declared -> admitted -> enforced` | validator_hidden 至少配 2 条独立 ingress path；observer 不得请求 validator-private RPC | operator 配置的是本地 P2P 角色策略，由 peer manager 强制执行；治理 validator 授权仍由其有效 registry 决定 |
| Traffic lanes | `lane_id/topic_or_stream/qos/peer_subset/replay_policy` | 分离 gossip、sync、blob/state、control 流量 | `registered -> active -> throttled/quarantined` | consensus lane 优先低抖动；blob/state lane 独立限速，不得拖垮 finality | 不同角色只开放最小必要 lane |
| Peer manager / anti-eclipse | `score/source_asn/source_operator/subnet_bucket/relay_dependence/misbehavior` | 评分、淘汰、重连、路径切换与 quarantine | `candidate -> active -> suspect -> blocked` | 同一 operator、同一 ASN、同一 `/24`、同一 relay-domain 占比超过阈值即降权或阻断，`blocked` 必须留下可审计工件 | 安全策略由本地 peer manager 执行，不能由远端覆盖 |
- Acceptance Criteria:
  - AC-1: 本专题必须明确声明“公网 IP 不是 validator/full/storage/observer 参与网络的通用前置条件”，只是一种 reachability 优势。
  - AC-2: 本专题必须冻结五种部署模式：`public`、`hybrid`、`private`、`relay_only`、`validator_hidden`，并说明各自公开面与限制。
  - AC-3: 本专题必须冻结 `validator core / sentry / relay / full-storage / observer-light` 五类角色，并明确 validator core 可完全不暴露公网入站。
  - AC-4: 本专题必须冻结一套统一 P2P core：`peer record + discovery + reachability + transport + peer manager`，业务层不得再直接依赖静态 UDP peer 列表作为唯一主路径。
  - AC-5: 本专题必须明确写出 reachability 顺序：`direct -> hole-punched -> relay`，以及 `AutoNAT / hole punching / relay reservation` 三者的关系。
  - AC-6: 本专题必须明确写出 `gossip plane / sync plane / blob-state plane / control plane` 分离，且不同链型只替换上层适配器。
  - AC-7: 本专题必须明确写出主链适配器边界，至少覆盖 `mesh gossip`、`committee direct`、`tree broadcast` 与 `blob availability` 四类数据面模式。
  - AC-8: 本专题必须明确写出 anti-eclipse / anti-spam 基线，包括 discovery-source 多样性、ASN/subnet/operator 多样性、relay budget 与 path quarantine；本地观察、远端自述与独立控制域证据必须分开，伪造多个来源标签不能使单源拓扑变为 healthy。
  - AC-9: 本专题必须明确写出 key boundary：transport/session key、node identity key、consensus signer、governance signer 彼此分离；relay、browser 和 public control plane 不得持有长期 signer 真值。
  - AC-10: 对应 GitHub Issue / GitHub Project 必须把实现拆成 identity、transport、reachability、role policy、traffic lanes、shared-network validation 等 workstreams，而不是只停留在概念描述。
  - AC-11: `doc/p2p/prd.md`、GitHub Issue / GitHub Project、`doc/p2p/prd.index.md` 与 `doc/p2p/README.md` 必须接入本专题，形成模块级追踪链。
  - AC-12: 本专题必须明确用户层默认只暴露 `2~3` 个简单模式，且默认行为是全自动检测与推荐，不要求普通用户手动理解 `deployment_mode/node_role`。
  - AC-13: 本专题必须明确高风险职责边界：系统可以全自动给出默认值，但当结果会让节点承担 `public entry / relay / sentry` 等外部暴露职责时，必须有显式确认或等价的风险同意机制。
  - AC-14: 本专题必须补齐 `peer reachability contract normalization` 的字段语义、输入/输出边界和非目标：不引入 iroh 依赖、不替换 libp2p `Multiaddr`、不把 relay/DNS/Pkarr 默认带入 oasis7。
  - AC-15: 本专题和 `testing-manual.md` 必须冻结 path-behavior matrix taxonomy，且把 `proxy != physical NAT/CGNAT proof` 写成 gate 约束；任何 public_testnet / shared_devnet claim 都必须说明 evidence class 与 claim boundary。
  - AC-16: 本专题必须说明 status observability enhancement 只暴露 bounded operator summary，例如 selected path kind/age、path transition counters、relay/direct/hole-punched mix、recent fallback reason 和 reachability confidence；若字段尚未由 runtime 实现，project 文档必须以待办 workstream 标注。
  - AC-17: 本地 seed 命中、来源归类、评分和路径归一化不得改动任何已签名字段；原发布者经缓存节点转发给第三节点后仍能按同一签名载荷验签。缓存命中不能绕过过期或撤销检查。
  - AC-18: peer record 生命周期须覆盖 TTL、未来时钟容差、乱序/重复/冲突序号、撤销、重启和版本迁移；候选缓存、来源观察、拨号重试和防回退状态有明确容量与回收条件。到限时显式降级，不以清空保护状态或重复赋予完整重试预算恢复 healthy。
- Non-Goals:
  - 不在本专题内绑定单一网络库实现，不把 “libp2p / 自研 / 混合” 之一提前冻结成唯一方案。
  - 不把当前目标降级成“能连起来就行”的家宽补丁；本专题讨论的是公共主链级目标态。
  - 不在本专题内定义单一链型的共识算法细节；这里只冻结链无关的 P2P substrate 和数据面适配边界。
  - 不把本专题的 P2P substrate / reachability green 结果单独提升为链上大世界状态底座的 S9A `module_full`、`integration_required` 或 `release_full`。
  - 不把 browser wallet、production signer custody 或 shared-network 实跑证据在此专题内冒充已实现。
  - 不要求普通用户直接配置 `validator_core / sentry / relay / full_storage / observer_light` 原始角色枚举。

<a id="discovery-evidence-boundary"></a>
### 2.1 发现证据与引导信任边界

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；本节承接 [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) 的公开服务与权威隔离，细化 PRD-P2P-024-E、AC-8、AC-17。

节点应能向 operator 解释“为何相信来源确有独立性”，不能只展示 peer 签名过的标签。远端 `discovery_sources/source_operator/source_asn` 保留为声明；本机收到记录的通道、上游节点、受信 seed 配置成员事实和实际路径另存为观察。只有具备可核验依据的 operator/ASN/relay 控制域才能贡献相应多样性；未知身份归入保守的未知组，不按任意自报名称拆成独立组。已知同一控制者提供的 bootnode 和 DHT 路径仍视为相关来源。没有足够证据时报告 degraded/unknown，并按有界本地 bootstrap 策略获取更多候选；不得伪造来源满足 healthy。

发现成功、Noise/libp2p 身份认证、seed allowlist 与同步 provider 可达均不建立 world/chain/genesis 信任锚，也不授予治理或共识权限。同步和共识仍分别执行 [同世界恢复链](../prd.md#p2p-target-recovery-trust-chain) 与 [角色权威隔离](../prd.md#p2p-target-role-isolation)。本地签名记录缓存只转发原签名载荷；本地 seed 标记不得写回远端已签名对象。

- 系统设计状态：已覆盖。
- 系统设计依据：[发现来源证据与签名不变性](mainnet-private-reachability-architecture.design.md#discovery-evidence-boundary)。
- 实现状态：部分实现。
- 实现依据：[当前实现边界](mainnet-private-reachability-architecture.design.md#discovery-peer-record-implementation-boundary)记录原签名转发、本地 seed/DHT/RR 观察及保守 unknown 域会话预算；远端声明不提升来源信任。
- 验证依据：[本地发现信任边界](../../../testing-manual.md#discovery-observation-local-trust-boundary)覆盖原签名三节点转发与自述／缺记录预算边界；完整来源独立性仍未验收。
- 剩余范围：受信控制域映射、完整来源独立性和 rendezvous ID 来源链；不由本节承诺公共网络完整抗 Sybil/anti-eclipse 或 validator 授权。

<a id="peer-record-lifecycle"></a>
### 2.2 Peer record 新鲜度、重放与容量

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；本节承接 [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) 的可验证服务材料与隔离边界，细化 AC-17、AC-18 和 NFR-P2P-PRA-5。

节点必须在发现入口、缓存命中、转发和实际拨号选择时检查记录仍在允许的有效期内，并拒绝已知的序号回退、同序异内容和撤销记录。发布端持久保存递增序号与原签名载荷；正常重启及重试不得回退或为同一次发布生成不同内容。接收端保留覆盖可重放窗口的防回退水位，缺少安全回收依据时宁可拒绝新增候选。过期记录不再作为拨号路径或多样性依据，刷新失败有明确退化原因；已有通过身份认证的会话按其独立会话/角色策略处理，单一发现系统故障不触发全量断连。

新版本记录的启用、旧版本的限时兼容和退役必须由受信网络配置明确规定；不能替旧记录合成 sequence、修改签名字段或用旧版本覆盖已接纳的新版本。所有来源共享有界的缓存、查询和重试预算，容量状态进入既有 status summary；同一逻辑发现轮次或速率窗口内，更换 provider、来源标签或重建工作项不重置累计预算。

- 系统设计状态：已覆盖。
- 系统设计依据：[Peer record 接纳、刷新与版本迁移](mainnet-private-reachability-architecture.design.md#peer-record-lifecycle)。
- 实现状态：部分实现。
- 实现依据：当前 libp2p 按 [v1 时间策略](mainnet-private-reachability-architecture.design.md#当前-v1-时间策略)检查 signed TTL、未来偏差和进程内时钟回退，限制缓存转发与发现拨号；自动查询／交换／缓存重发布另由[共享工作预算](mainnet-private-reachability-architecture.design.md#discovery-work-budget)限制并发、滚动速率与代理重试，列表输入／输出有界；[到期前刷新](mainnet-private-reachability-architecture.design.md#discovery-record-refresh)在余量内尽力请求新材料，受共享预算与目标 cooldown 约束。[V2 本机发布](mainnet-private-reachability-architecture.design.md#discovery-durable-publication-v2)提供显式初始化和持久发行序号；[接收端水位](mainnet-private-reachability-architecture.design.md#discovery-receiver-floor-v2)在显式持久兼容／V2-only 配置下防止 live 记录跨重启降序、持久隔离同序冲突并退役已迁移 peer 的 V1；[共享记录准入预算](mainnet-private-reachability-architecture.design.md#discovery-record-admission-budget)限制实际入站记录处理。默认 legacy 行为不自动启用持久策略。
- 验证依据：[发现记录时效验证](../../../testing-manual.md#discovery-record-v1-time-policy)覆盖时间边界、缓存／DHT 转发和拨号租约、活动会话保留；[接收水位验证](../../../testing-manual.md#discovery-receiver-floor-v2)覆盖持久乱序、冲突、原件重开和显式兼容策略；tombstone、可信时间及策略转换仍待验收。
- 剩余范围：版本化 tombstone、接收水位安全回收、持久可信时间、接收策略转换、跨重启累计配额、完整协议服务总预算及跨重启刷新状态；节点身份密钥失陷或本机受信存储被恶意回滚不由 TTL 单独解决。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 目标态采用五层框架。第一层是 `user mode abstraction`，给普通用户暴露 `自动加入 / 私有安全 / 公网入口` 这类简化模式，并把自动探测结果映射到底层正式语义；第二层是 `identity/discovery`，用带 TTL 的签名 peer record 把节点身份从瞬时地址中解耦；第三层是 `reachability/transport`，统一 direct、hole-punched 与 relay 路径，并对 QUIC/TCP/Noise/mux 做抽象；第四层是 `traffic lanes`，把 consensus gossip、header/block sync、state/blob transfer、control/heartbeat 分离；第五层是 `chain adapters`，让不同公共主链风格的数据面挂到同一底层 P2P core 上，而不是各自重造 discoverability 和 NAT 穿透。
- Integration Points:
  - `doc/p2p/prd.md`
  - GitHub Issue / GitHub Project
  - `doc/testing/benchmarks/mainstream-public-chain-testing-benchmark.prd.md`
  - `doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.prd.md`
  - `doc/p2p/prd.md` 与 `doc/p2p/blockchain/hosted-player-access-operator-runbook.md`
  - `doc/p2p/network/readme-p1-network-production-hardening.prd.md`
  - `doc/p2p/node/node-identity-replication-contract.prd.md`
  - `testing-manual.md`
- Edge Cases & Error Handling:
  - 自动探测结果不稳定：若公网检测、NAT 类型与打洞结果互相矛盾，默认回退到较保守的 `私有安全` 或 `relay_only`，而不是默认升级到公网入口。
  - 对称 NAT / CGNAT 无法打洞：节点必须自动降级为 `relay_only` 或 `validator_hidden + sentry`，而不是直接失去网络身份。
  - 用户拒绝承担公网入口职责：即便检测结果显示机器可公网暴露，也必须允许节点回退到非入口模式，并保持底层正式角色不越权。
  - relay reservation 过期或 relay 宕机：peer manager 必须主动切换备用 relay/sentry，并降低该 relay-domain 权重。
  - 公开地址漂移：peer record 需快速 refresh；过期地址不得继续作为优选路径。
  - bootnode / rendezvous 污染：若候选 peer 集中于单一 operator、ASN 或 relay-domain，必须触发 suspect 状态和重引导。
  - private validator 误暴露：若 validator core 被配置为公开暴露共识面，系统必须发出高严重度运维警报。
  - relay 滥用与流量挤占：blob/state lane 必须有独立带宽预算，不能拖垮 consensus/control lane。
  - DHT 或 rendezvous 部分失效：已建立连接不得因单一 discovery 子系统故障而全量重置。
  - 角色越权：observer/light 节点请求 validator-private lane、control plane 或 signer path 时必须被拒绝并记分。
- Non-Functional Requirements:
  - NFR-P2P-PRA-1: `validator_hidden` 模式必须允许“零公网入站 + 至少两条独立 outbound ingress path”加入网络；这两条路径可以是 `sentry + relay` 或 `relay + relay`，但不得只有单一路径。
  - NFR-P2P-PRA-2: 网络健康目标必须要求节点在启动后 `60s` 内收敛到足够的 discovery 视图；`public/hybrid` 节点至少看到 `8` 个候选 peer，`private/validator_hidden` 节点至少建立 `4` 条有效工作路径。
  - NFR-P2P-PRA-3: relay fallback 的控制面时延中位数不得超过同区域 direct path 的 `2x`；若超过，系统必须将该 relay-domain 标记为降级。
  - NFR-P2P-PRA-4: active peer set 中，来自同一 operator、同一 ASN 或同一 `/24` 的占比默认不得超过 `25%`；超限必须降权或阻断。
  - NFR-P2P-PRA-5: signed peer record 的 TTL 必须为正且不得超过 `1h`，reachability 状态变化后必须支持快速 refresh/revoke；发布与接收均执行该上限，并以版本化策略规定允许的未来时钟偏差，时间不可信不能直接判 fresh。
  - NFR-P2P-PRA-6: transport/session key、node identity key 与 consensus/governance signer 必须逻辑隔离；任何 relay 或 browser surface 泄露 transport key 都不得等价为节点治理权或出块权泄露。
  - NFR-P2P-PRA-7: gossip、sync、blob/state、control 四条 lane 必须支持独立的 rate limit、peer subset 和 quarantine 规则。
  - NFR-P2P-PRA-8: 默认安装/启动路径中，用户在不打开高级设置的情况下不应被迫选择底层正式角色；系统必须在 `60s` 内完成一次自动探测并给出默认用户模式。
  - NFR-P2P-PRA-9: 任何会把节点提升为 `公网入口` 的自动决策，都必须带可审计的检测依据与风险提示；无依据时默认保持保守模式。
  - NFR-P2P-PRA-10: 对于 `1` 本机 + `2` ECS 这类固定 triad/bootstrap 拓扑，runtime 必须提供显式低流量运行档位，至少能同步收紧 bootstrap redial、discovery query、peer record republish、dynamic gossip peer budget、feedback announce budget，并允许关闭 AutoNAT 这类在固定私网拓扑中只会制造控制面抖动的探测。
- Security & Privacy:
  - 所有 peer record 必须做链域隔离签名，防止跨链、跨环境重放。
  - transport 默认要求加密和双向身份确认；未认证链路不得进入 consensus/control lane。
  - 私网节点默认不公开内网地址；只有 operator 显式允许时才向受信对等方暴露私有地址提示。
  - relay 只转发，不解释业务语义，也不持有长期 signer；任何 relay route 都不能提升调用者权限。
  - sentry 与 validator core 必须分离身份与密钥；sentry 被打爆不应直接等价为 validator signer 暴露。

## 5. Risks & Roadmap
- Phased Rollout:
  - Phase 0: 冻结目标态文档、角色边界、lane taxonomy 与 mixed-topology 验证口径。
  - Phase 1: 落 peer record / discovery / reachability service，把现有静态 peer 与双路径网络接线收敛为统一 substrate。
  - Phase 2: 落 relay/sentry/validator_hidden 与 traffic lanes，把 consensus、sync、blob/state、control 分离成独立 QoS。
  - Phase 3: 落用户可见部署模式抽象与自动检测默认值，把正式角色留在高级设置和内部策略层。
  - Phase 4: 落 anti-eclipse、peer scoring、shared-network mixed-topology、chaos/release-train 验证，之后才允许更高 public-chain maturity claims。
- Technical Risks:
  - 风险-1: 若继续在业务层保留“直接发 UDP 给静态地址”的逃生路径，框架层的 role policy 与 reachability 会长期失真。
  - 风险-2: 若把 relay 设计成“万能兜底”却不做预算和多样性限制，网络会从“全公网依赖”退化成“单 relay 依赖”。
  - 风险-3: 若不把 validator core 与 sentry 身份/密钥彻底拆开，私网 validator 的安全收益会被公开面抵消。
  - 风险-4: 若不为不同链型定义数据面适配边界，后续高吞吐链型需求会再次迫使底层 substrate 分叉。

## 6. Validation & Decision Record
- Test Plan & Traceability:

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-P2P-024-A | P2PARCH-0/1 | `test_tier_required` | 文档建档、模块入口映射、peer record / deployment mode 术语冻结 | `doc/p2p` 主边界与架构口径 |
| PRD-P2P-024-B | P2PARCH-1/2/4 | `test_tier_required + test_tier_full` | transport abstraction、lane split、adapter boundary 设计与 mixed-topology 集成回归 | runtime / network substrate |
| PRD-P2P-024-C | P2PARCH-2/3 | `test_tier_required + test_tier_full` | validator_hidden、sentry、relay、full-storage、observer 角色矩阵与权限回归 | operator 部署与安全边界 |
| PRD-P2P-024-D | P2PARCH-2/3/6 | `test_tier_required + test_tier_full` | AutoNAT / hole punch / relay fallback / relay exhaustion / path failover 套件 | 私网节点可达性与恢复 |
| PRD-P2P-024-E | P2PARCH-5/6/7 | `test_tier_required + test_tier_full` | `P2PARCH-5` 负责 peer-manager/diversity/block artifact substrate 与定向 required 验证；`P2PARCH-6/7` 负责 anti-eclipse、mixed-topology、shared-network 与 chaos/release-train 证据 | public-chain-grade claims gate |
| PRD-P2P-024-E / AC-17、AC-18 | 本次文档 PR；实现任务尚未形成验收记录 | 目标为 `test_tier_required`，上线另核同窗口证据 | 伪多源与未知 operator/ASN、三节点原签名转发、TTL/时钟边界、序号回退/冲突/重启、版本降级及容量耗尽；准确用例与现有 test source 见配对设计验证映射 | discovery admission、缓存转发、peer selection；现有 matrix 通过不代签这些新增负例 |
| PRD-P2P-024-F | P2PARCH-8/9 | `test_tier_required + test_tier_full` | 用户可见部署模式、自动探测默认值、高风险职责确认与高级设置覆盖 | 部署 UX 与 role policy 可用性 |
- Decision Log:

| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-P2P-PRA-001 | 采用 `identity != address` 的 signed peer record 模型 | 继续把节点地址写死在静态 peer 列表 | 家宽/NAT/地址漂移条件下，静态地址不是稳定身份。 |
| DEC-P2P-PRA-002 | Reachability 采用 `direct -> hole-punched -> relay` 多路径排序 | 只做 direct，失败即离网 | 公共主链现实部署天然包含 NAT、CGNAT 与防火墙环境。 |
| DEC-P2P-PRA-003 | validator 采用 `validator core + sentry/relay` 模型 | 要求 validator 自身全公网暴露 | 公开入站面与长期 signer 不应强耦合。 |
| DEC-P2P-PRA-004 | 将链差异限制在数据面适配器 | 每种链型各自重做 discovery/reachability | 发现、可达性、加密和 peer scoring 是跨链共性能力。 |
| DEC-P2P-PRA-005 | 把 anti-eclipse / diversity / relay budget 写成框架层硬约束 | 先实现连通，后续再补安全策略 | 公共主链级 P2P 不能把“安全拓扑”推迟到上线后再补。 |
| DEC-P2P-PRA-006 | 用户层只暴露 `2~3` 个简单模式，默认全自动探测，底层继续保留正式角色语义 | 让所有用户直接选择 `deployment_mode/node_role`，或彻底取消内部正式角色 | 普通用户不应先学基础设施术语，但系统内部仍需要精确职责语义来守住安全边界。 |
| DEC-P2P-PRA-007 | 允许将 `P2PARCH-5` 作为 runtime substrate milestone 单独关闭，并把 mixed-topology required/full 与 release-gate claim 继续保留在 `P2PARCH-6/7` | 持续把 QA evidence gate 绑定在 `P2PARCH-5`，导致 runtime 已完成但任务状态长期失真 | runtime 实现闭环与 public-chain-grade claims evidence 是不同层级；前者应按实现真值关闭，后者继续由 QA/release gate 任务阻断。 |
| DEC-P2P-PRA-008 | 采用 oasis7 内部 `peer reachability contract` 作为 signed peer record、transport path、status 与 matrix 之间的唯一归一化边界 | 直接引入 iroh、替换 libp2p substrate，或让各 surface 各自定义 reachability | iroh 的可借鉴点是 identity-first/path-state/evidence 边界；oasis7 仍以现有 libp2p、role policy、network-tier truth 为底座。 |
| DEC-P2P-PRA-009 | path behavior matrix 只拥有 evidence/claim taxonomy，不拥有 runtime reachability truth | 让 matrix 脚本反向定义 direct/hole-punch/relay 语义 | QA gate 应防止 overclaim；canonical path label/rank 必须来自 runtime contract。 |
| DEC-P2P-PRA-010 | status observability 只扩展现有 `/v1/chain/status.observability` / triad summary 的 bounded operator view | 新建第二套 net-report/health endpoint 或导出 unbounded per-peer metrics labels | operator 需要诊断路径选择和退化原因，但网络健康真值必须保持单源、低基数、可回归。 |
