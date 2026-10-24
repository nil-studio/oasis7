# oasis7 主链级非全公网 P2P 覆盖网络架构（设计文档）

- 对应需求文档: `doc/p2p/network/mainnet-private-reachability-architecture.prd.md`
- 对应GitHub Issue/Project task truth: GitHub Issue / GitHub Project

审计轮次: 1

## 上游约束、结构边界与审读范围

专业 owner 为 P2P/runtime；消费者为 node、chain adapters、operator status 与 QA matrix。配对 [PRD §2](mainnet-private-reachability-architecture.prd.md#2-user-experience-functionality) 定义部署模式、角色、路径和用户模式义务。本设计以下承接聚焦既有 substrate；用户模式 UI、显式确认与端到端产品可用性不由这些条款单独证明。

本文定义 discovery 证据来源、签名不变性及 peer record 生命周期的目标合同与验收边界。实现落点中的日期结论保留其历史范围；已接入能力及剩余差距见 [当前实现边界](#discovery-peer-record-implementation-boundary)。原章节锚点继续保留。

## 设计目标
- 把“没有公网 IP 的节点如何成为主链级网络一等公民”从部署特例提升成框架层能力。
- 为 oasis7 冻结一套可同时承载 mixed-topology 与多链型数据面的统一 P2P substrate。
- 该 P2P substrate 是链上大世界状态底座的网络/数据面子线，不单独证明 S9A `module_full`、`integration_required` 或 `release_full`。

## 目标原则
1. 身份与地址解耦：节点身份由签名 peer record 与 node identity key 定义，而不是由瞬时 IP:port 定义。
2. Reachability 是连续谱，不是二元判断：`public`、`hybrid`、`private`、`relay_only`、`validator_hidden` 都是正式模式。
3. 路径优选但不路径绑定：系统优先 direct，其次 punched，再次 relay；业务层永远看到统一的逻辑连接。
4. 角色先于实现：validator/sentry/relay/full-storage/observer 的公开面、密钥边界和 lane 权限先冻结，再决定具体库接法。
5. 链型适配在上层：Ethereum/Cosmos 类 mesh gossip、HotStuff/committee direct、Solana 类 tree broadcast、blob/DA lane 都挂在同一 substrate 上。

## 协议栈分层
| 层 | 组件 | 责任 |
| --- | --- | --- |
| `L0 identity` | `node identity key`、signed peer record、role claim | 认证节点链内身份并绑定角色与可达性声明；不授予治理或共识权限 |
| `L1 discovery` | bootnodes、DHT、rendezvous、peer record cache | 聚合候选 peer、形成多源发现视图 |
| `L2 reachability` | AutoNAT、observed addr、hole punch、relay reservation | 判定 directness 并维持可工作路径 |
| `L3 transport` | QUIC、TCP/Noise、mux、stream/session abstraction | 提供统一安全传输与多路复用 |
| `L4 peer manager` | diversity policy、score、quarantine、path selection | 控制谁能进入 active peer set，以及用哪条路径 |
| `L5 traffic lanes` | gossip、sync、blob/state、control | 给不同业务流量独立的 QoS、rate limit 和 peer subset |
| `L6 chain adapters` | mesh gossip、committee direct、tree broadcast、blob availability | 承载不同公共主链风格的数据面 |

## 节点角色模型
| 角色 | 对外暴露 | 必须能力 | 禁止能力 |
| --- | --- | --- | --- |
| `validator core` | 可为零公网入站 | outbound 到 sentry/relay、consensus signer、committee lane | 不应承担大规模公开入站或浏览器 public plane |
| `sentry` | 公网或 hybrid | inbound/outbound 转发、anti-DoS 缓冲、policy enforcement | 不持有 validator 共识 signer |
| `relay / anchor` | 公网 | relay reservation、bootstrap、AutoNAT assist、rendezvous | 不解释业务权限，不持有长期 signer |
| `full-storage` | public/hybrid/private 皆可 | header/block/state/blob sync、serve range/proof | 不得冒充 validator 或 governance signer |
| `observer-light` | public/private | 轻量 sync、只读 gossip、client session | 不得请求 validator-private control lane |

## Peer Record 模型
| 字段 | 说明 |
| --- | --- |
| `peer_id` | node identity 的稳定标识 |
| `network_domain` | 由受信网络配置固定；当前 wire 使用 `world_id/network_id`。其与 chain/genesis 的映射由正式网络身份合同拥有，peer 自述不能建立映射或 genesis 信任 |
| `role_mask` | validator core / sentry / relay / full-storage / observer-light 声明 |
| `reachability_class` | `public/hybrid/private/relay_only/validator_hidden` |
| `public_addrs` | 可公开 direct dial 的地址 |
| `relay_addrs` | 可通过 relay 抵达的地址或 reservation 引用 |
| `capability_lanes` | `consensus_gossip/sync/blob_state/control` 可服务能力；缺省时按角色回填默认 lane 集 |
| `published_at_ms` / `ttl_ms` / `sequence` | 有效期及刷新、撤销、地址漂移顺序；sequence 由可选 V2 签名 envelope 与本机持久发行提供；接收端持久水位仍待实现，不能为旧记录回填 |
| `signature` | 链域隔离签名 |

设计约束:
- peer record 默认短 TTL，reachability 变化必须允许快速刷新。
- 私网地址默认不公开；若需暴露给 sentry/relay allowlist，必须作为受限提示而不是公共字段。

<a id="discovery-evidence-boundary"></a>
## 发现来源证据与签名不变性

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；承接 [PRD 发现证据边界](mainnet-private-reachability-architecture.prd.md#discovery-evidence-boundary) 与 [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)。本节为目标合同；签名认证发布者身份，不认证发布者自述的来源独立性。

### 输入与所有权

| 数据 | producer / owner | consumer 与边界 |
| --- | --- | --- |
| 原始 signed record、签名载荷 digest | 节点身份密钥持有者；proto 拥有版本化 wire，net 验签 | cache/peer exchange 保留原载荷与签名。解析后的视图只读；补标签、排序、默认 lane 和路径归一化放到派生视图，不能改变转发时的签名含义 |
| `DiscoveryObservations`（内部模型，部分实现） | net 的实际接收/查询/静态配置入口 | 按网络域、peer ID、record digest 关联接收通道、上游 peer、首次/末次观察时间和本地配置版本；本地静态 seed 成员事实另按网络域与 peer ID 保存，不因换 record 丢失 |
| operator/ASN/relay 控制域依据 | 本地策略认可的配置/登记依据及实际路径观察 | peer manager 使用带来源、有效期和 `claimed/verified/unknown` 状态的归类；远端字符串只进入 claimed，不得覆盖 verified 或生成独立组 |
| peer health 与来源摘要 | peer manager 消费上述模型 | selection/status/QA 读取同一结果；不从 status 反写来源，也不允许 peer exchange 把别人的本地观察转成自己的 verified 事实 |

独立性规则：

1. `StaticBootstrap` 只表示当前本机受信 seed 配置中有该身份；收到对方同名标签不能授予该例外。来自 DHT、rendezvous 或 peer exchange 的记录，分别由本机实际入口登记来源；DHT 返回签名记录只证明发布者，不能证明整个 DHT 的控制独立性。
2. 同一签名记录经同一上游反复转发只刷新该观察，不增加来源数。两种通道必须分别有实际观察和控制域独立性的依据，才能贡献两类独立来源；已知同 operator/relay 控制的两个入口合并计数。无独立依据时输出 unknown/degraded，不用不同标签推断独立。
3. operator/ASN 无依据时进入保守的未知组；claimed 标签保留用于诊断，不能规避集中度配额。实际拨号地址的 subnet 和已选 relay path 仍是各自观察真值；经 relay 看到的中继地址不能充当远端节点真实 ASN。第三方 ASN 映射或 operator 登记须有受信来源和时效，签了字符串不等于验证了归属。
4. 冷启动允许在本地策略限定的 seed 集合、并发数、拨号次数和时间窗口内获取首批材料。该 bootstrap 例外保持 degraded，只能帮助发现/请求可验证数据，不免除域、签名、时间、大小和角色检查；不能为了凑足多样性而让所有候选先成为 active。证据不足时有界重试并保留已验证只读同步能力，适用角色的共识参与闸门由共识层另判。
5. source observation 到期、seed 配置移除或路径/依据失效时重算健康。只对当前有效观察计数；记录 digest 更新后，旧记录的接收事实不自动变成对新记录的观察，本机静态成员事实则独立核对当前配置。

缓存和转发必须保持签名载荷不变。节点 A 收到 seed B 的有效记录，即使 B 没写 `StaticBootstrap`，A 也只在本地观察中记录 seed 事实；转发给 C 时保留 B 的原签名载荷，C 独立验签、时间和域。C 只能把本次传输视为自己的 peer-exchange 观察，不能继承 A 的本地 seed 身份。任何发现路径都不能替换外部固定的 world/chain/genesis、checkpoint 或治理/validator 授权输入。

<a id="peer-record-lifecycle"></a>
## Peer record 接纳、刷新与版本迁移

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；承接 [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle) 与 [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)。以下的 sequence、撤销和持久水位是新增目标设计，当前 `SignedPeerRecord` 不具备完整实现。

### 发布与验证顺序

目标新版本以独立 schema/签名域绑定网络域、peer ID、全部声明、`published_at_ms`、`ttl_ms`、单调 `sequence` 和 record 状态。发布端在 node identity 对应的受保护存储中先持久预留递增序号及待发布载荷，再生成并持久保存原签名结果，之后才对外发布。崩溃可留下序号空洞，但不得复用已分配序号签不同内容；重试重发同一载荷。状态或地址变化使用更高序号。序号耗尽、存储损坏或可检测的水位回退使发布停止，不重置序号或自动换身份规避问题。

接收端按顺序执行：

1. 在有限输入字节、地址/字段数量、解码深度、并发验证和单来源速率预算内解析；未知版本或不支持的签名域拒绝。
2. 验证 identity public key 与 peer ID 绑定、完整签名载荷、固定网络域及角色/公开地址 policy。链路身份通过不能代替 record 验签。
3. 验证 `0 < ttl_ms <= 1h`，时间加法使用检查溢出的运算。版本化本地策略显式给出允许的未来偏差；超过偏差拒绝，`now >= published_at_ms + ttl_ms` 即过期。容差不延长已声明的到期时间。已知时钟倒退或时间来源不可信时暂停依赖新鲜性的接纳并报告 degraded；已判过期的同一载荷不因时钟倒退恢复。
4. 按 `(network_domain, peer_id)` 比较已接纳的 sequence/digest：更低拒绝；相等且 digest 相同为幂等命中；相等且 digest 不同为签名冲突，保留旧记录作证据并隔离该 peer 候选，不按到达顺序选胜者或继续贡献健康来源；更高且其他检查通过才更新。时间戳不能覆盖 sequence，也不能用重新收到旧记录延长 TTL。
5. 将新记录、关联观察和防回退水位作为一致的接纳结果安装；需要跨重启保存的水位在候选成为可拨号/可转发前持久化。任何验证/保存失败均不替换原已验证记录，不改变原水位。

撤销使用同身份、同网络域、更高 sequence 的签名 tombstone，撤销所覆盖的低序记录和其候选地址；tombstone 不携带可用路径。重新发布必须使用更高序号并重新满足当前策略。该撤销只管理发现声明，不撤销 validator membership、治理权或已固定历史；密钥失陷/正式角色撤销由各自 authority 合同处理。

### 缓存、重启与资源边界

每条记录保存声明到期时间和本机观察时间。DHT 保存期限不得长于记录剩余 TTL；DHT 默认过期策略不能替代本地验时。查询去重只能复用仍有效且满足刷新余量的记录；接近到期触发带抖动的有界刷新。过期/撤销后从拨号候选、响应缓存及多样性计数中移除，不因旧 dedupe 标记永久阻止重新发现。发送缓存响应和排队后的实际拨号前重新检查有效性，避免检查与使用间跨过到期边界。

各网络 profile 明确候选记录总数/字节、单 peer 地址与来源观察数、并发查询/验证、拨号重试、持久水位和诊断明细上限。每个逻辑发现轮次及速率窗口有累计预算，切换来源/provider 或重建该轮次的工作项不重置预算。先回收过期材料和低优先级未准入候选；仍到限则拒绝新增或延后工作，并输出 bounded reason，不以无上限 map 或无限重试维持健康表象。

防回退水位与有效记录分开保留，至少覆盖所有已接纳记录/撤销材料可能重放的时间窗及允许时钟偏差；活动 peer 和受信 seed 的必要水位持续保留。只有依据可信时间下界、最大 TTL 和已接受版本证明旧载荷已不可能再次通过接纳时，才允许回收非活动 peer 水位；水位容量不足且不能安全回收时拒绝新候选。接收端回收水位不允许发布端重置序号。重启重新验证持久材料的格式、域、digest 与剩余有效期，时间/存储异常保持 degraded；普通本地文件水位不声称抵抗受信主机的任意恶意回滚。

有效期结束针对发现记录，不自动撤销已认证的活动会话。既有会话继续受会话身份、当前角色、peer health、路径及资源策略约束；不能凭过期记录新建路径、承诺多样性或获得新权限。DHT/rendezvous 故障不触发全量断连，出现实际身份/角色撤销或已证明攻击时按对应策略隔离。

### 当前 v1 时间策略

现有 `oasis7-peer-record-v1` 保持原始 schema 和签名域，采用固定 `v1-time-policy-1`：`published_at_ms >= 0`、`0 < ttl_ms <= 3_600_000`，检查到期加法溢出，允许最多 `30_000ms` 未来偏差，`now >= expiry` 拒绝。偏差仅允许发布者时钟略快，不延长签名声明的 expiry；旧版不满足这些约束的记录也拒绝，不静默修改时间字段或重新签名。这个兼容收紧不是 sequence 协议升级。

每个网络实例显式持有本机 wall-clock 高水位；已观察到倒退或不可用时间时暂停依赖 fresh 的接纳、缓存响应及发现拨号，只有 wall-clock 追上已观察高水位才恢复。接纳时另以单调时钟锁定剩余生存期，同一原始载荷重复到达只保留更早的截止点。缓存与排队路径在使用时重新检查 wall-clock 和单调截止点，已过期记录不能因倒退或重试恢复。此状态为进程内防护，不证明系统时间绝对可信，也不覆盖重启或恶意整机回滚；持久可信时间和 sequence 高水位仍由后续版本承接。

空闲事件循环也以不超过一秒的 tick 刷新过期发现的健康投影；真正缓存响应与拨号仍逐次即时检查。发现缓存最多 1024 条、单条 signed payload 最多 16 KiB；先回收失效且无活动会话的记录，到限拒绝新候选，不驱逐活动会话以腾空间。有效记录和活动会话元数据分开使用：过期声明不得响应、贡献来源或新拨号；既有会话继续按已认证链路及当前权限/路径配额服务，其能力声明只作为原会话的限制依据。静态配置地址拥有独立生命周期，不因同 peer 的记录过期被撤销；远端新地址不得借静态 seed 身份变为永久路径。记录拨号不主动向 Kademlia 永久发布地址，连接成功也不把其端点提升为永久配置。底层 Kademlia 或其他 behaviour 仍可能记住协商端点与未完成查询地址，因此在组合 behaviour 的最终 outbound 地址解析边界再次检查：自动发现／请求重连拨号仅使用当前有效记录的精确地址或本机配置的精确 bootstrap 地址；未知中间路由节点须先取得记录或配置，不能靠旧路由缓存继续拨号。该保守策略可能降低未知路由节点的发现可用性，但不切断既有会话；明确由本地调用方提供地址的显式拨号保持原入口语义，记录拨号仍在调用边界验租约。过滤先应用于所有 behaviour 返回的附加地址，自动 DialOpts 自带的不适用地址拒绝整次拨号，保留正常失败通知，不吞掉协议状态。DCUtR 协商及 AutoNAT 通过其自身验证后的回拨，以及 relay client 继续执行已获准的本地 transport 拨号／监听操作，按实际子协议发出的 ConnectionId 标记单次原始地址来源；最终入口消费标记后允许这次协议操作的原始目标，但仍过滤全部附加缓存地址，不授予新的永久来源或健康权限。标记只由对应子协议 poll 生成，不从远端角色、地址标签或 Dialer/Listener 角色推断；失败也清理，防止复用。TTL 在原始发现候选连接释放时检查，不取消已启动且仍受既有 transport 超时约束的连接操作；relay 子拨号不是新的发现租约，也不声称在每个子步骤续签时效。尤其 relay DialOpts 启用了地址补充，单次来源例外只覆盖原始 relay 端点，旧 Kademlia 替代地址仍必须过滤。失效地址跟踪可以回收，因为未列入允许集合的自动候选始终拒绝，而不会重新变成默认放行的 unknown。

DHT peer-record 的本地保存与转发期限不晚于签名剩余 TTL，其他 DHT 对象的期限不受本条改变。

<a id="discovery-work-budget"></a>
### 当前发现工作预算

现有 libp2p 网络实例采用固定 `v1-discovery-budget-1`，在自动发现工作排入底层协议前统一接纳。目标是限制候选 fanout 和代理重试放大；不改变签名 wire，也不将预算余量视为健康或授权依据。

- 同一实例、所有发现来源共享单调时钟的滚动 60 秒窗口，最多启动 128 项工作，并发最多 32 项。完成或失败只释放并发，不返还窗口消耗；切换代理、DHT／RR 入口或重建工作项不能重置窗口。新实例重新计数，不声称跨重启限速。
- 自动 provider 查询、记录查询、记录交换和缓存转发重发布属于发现工作。本机发布／provider 公告、应用显式 DHT／业务 RR、rendezvous register／discover 协议操作及底层协议内部报文不由此预算逐项／逐包计量；现有 transport／协议限制继续适用。该边界不等于全网连接、字节或验证 CPU 的总预算。
- 单目标缓存代理链最多尝试 4 个不同代理；成功或失败后的新逻辑请求仍受实例滚动窗口约束。到限延后，保持现有已认证会话；不为腾预算全量断连，也不承诺攻击下的公平性或收敛期限。
- 缓存发现列表 wire 输入最多 16 KiB、128 项，先核输入尺寸并有界解码，再去重和派生工作。响应端最多返回 128 个当前有效的 ID；这不是完整分页枚举 API。DHT provider 结果累积也最多 128 个候选。
- 三类 RR 请求各自的 cooldown 表最多 1024 个 peer（direct record、cached record、cached ID list，合计最多 3072 项）；到期项先回收，无安全空位则拒绝新增调度，不驱逐仍有效的限速记录。拒绝不能制造 pending 标记或请求流量统计。
- 并发／窗口占用、容量上限、累计启动数及并发／窗口拒绝计数通过既有网络 metrics 的 `discovery_work` 摘要投影，禁止 peer ID 或远端字符串作为无限维度 label。预算拒绝不替代签名、域、TTL、peer health 或角色检查。

并发仅在对应协议实际终态释放；不能按本机超时清空账本而让仍在运行的协议工作失去计量。上述速率窗口和代理链限制承接累计工作上限；跨轮次持久配额、rendezvous 协议操作总预算、入站服务／验证总预算、抖动刷新以及受信控制域依据仍属后续范围。

<a id="discovery-record-refresh"></a>
### 当前 v1 到期前刷新

当前进程内候选缓存另保存单调时钟刷新计划，不修改 signed payload。`v1-record-refresh-1` 使用原始 lease deadline，提前量为 `min(ttl / 5, 60s)`，再加不超过该提前量一半的稳定抖动；接纳时剩余寿命已经落入刷新区间的记录可以立即进入待刷新集合。抖动使用接收实例自己的随机性并在本次接纳时固定，避免所有接收者仅由远端 peer ID 算出相同计划；该随机性不作为安全或控制域独立性证明。相同签名载荷重复安装保持原始 lease、刷新计划和重试下限，不因 DHT 返回旧副本而重获有效期或重新抽取抖动。只有实际接纳的新签名载荷重新排期，v1 时间戳仍不能代替待实现的 sequence 防回退合同。

已有一秒到期 tick 在 `peer_record_template` 存在且 `discovery_query_interval_ms > 0` 时调度到期前刷新；`<= 0` 继续关闭周期网络发现，到期清理仍运行。每次 tick 最多处理 4 个已到刷新时刻的有效候选，按下次可尝试时间及稳定 peer ID 排序；一次尝试后至少等待 5 秒单调时间，预算拒绝也不得在每个事件循环立即重试。刷新状态随最多 1024 条候选保存，不新增无限 peer map。

已连接目标通过既有 local-record RR 入口请求发布者当前签名记录；断开目标通过显式的 DHT 刷新入口，仅绕过“已有有效缓存”的查询抑制，仍检查 pending 去重。它们均受[共享发现工作预算](#discovery-work-budget)和既有 RR 目标 cooldown 约束，刷新没有隐含优先额度，也不借缓存来源授予信任。DHT 可能只返回尚未过期的同一旧载荷，此时不宣称刷新成功或延长 TTL。

刷新是尽力减少候选空窗，不承诺持续可用：小于一秒 tick 或既有 60 秒 RR cooldown 的短 TTL、预算耗尽、网络失败和发布者未更新都可能在取得新材料前到期。失败／延后保留原记录、路径与原租约直到真实到期；不提前从健康中删去仍有效的材料，也不延长签名声明或全量断开已认证会话。过期仍执行即时响应／拨号检查和既有到期清理。该计划为进程内状态，不覆盖重启防重放、可信绝对时间或持久发布序号。

### 版本兼容与交付顺序

1. 先在不改变现有签名含义的前提下分离本地来源观察，并对当前已签名的时间字段落实 TTL/时钟/缓存检查；既有记录缓存不得被原地补 source 或 sequence。
2. 为新版本另定 schema、签名域、协议协商及网络配置中的启用条件。兼容窗口明确允许的旧版入口、seed/peer 范围与退役条件；旧版不具备 sequence 保护，不能被标成已满足新合同，不能静默绕过时间检查。
3. 已接纳新版本的 peer 不接受旧版覆盖。兼容许可和已迁移状态须跨重启保持；不能因清 cache 或重建本地观察重新得到旧版许可。兼容窗口结束后严格拒绝旧版；不在旧签名下填充新字段，也不对远端记录重新签名掩盖变更。
4. net/proto 共同完成 wire 与接纳，chain-runtime 完成发布序号持久化，node/status 消费本地派生视图；实现 PR 同时覆盖迁移、原签名转发、各入口的时间检查及高 churn 容量用例后再升级支持声明。

<a id="discovery-record-admission-budget"></a>
### 本进程 peer-record 准入预算

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。`peer-record-admission-1` 为本机网络实例的记录解析／验签／接纳建立一个共享滚动窗口：60 秒内最多 512 次准入，有真实可识别上游时每个 peer 最多 256 次；上游跟踪表最多 512 项，只清理窗口内已无计费事件的项，不驱逐活跃项以重置配额。计时使用单调时钟，准入前移除 age >= 60 秒的计费事件；换 provider、协议入口或查询重试不重建预算，每个实际收到的重试／重复 payload 都单独计费；单进程重启后的新窗口不宣称跨重启配额或持久防回退能力。

同一 `Arc<Mutex<PeerRecordAdmissionBudget>>` 覆盖 peer-record DHT store put、DHT 自动发现与公开 `get_peer_record` 结果，以及特殊 RR 请求／响应。准入发生在首个 payload 解码／验签前，拒绝不修改旧 store、候选 cache 或 DiscoveryObservation；已准入但随后格式／签名失败的尝试不返还额度。一个请求／响应／put 为一次有界工作单元，其中的固定次数重复验证不重复收费；候选安装入口须线性消费绑定真实来源、原始 payload digest、工作单元与准入时间的不可复制凭据，不能被新的生产调用方直接绕过或误配到其他载荷。后续持久 floor 写入仍须在同一已授权 candidate 安装路径内，不从本凭据推导独立写入权。DHT store 后的另一次查询结果是新的工作单元。RR ID 列表最多 128 项，一次合法列表及后续 128 条记录响应可在默认单上游额度内完成，但仍受共享总量及已有出站工作预算约束。

上游仅取自 libp2p 认证的 RR 外层 peer、或 Kademlia 结果的实际来源 peer，不能使用记录自述的 peer ID、publisher、operator 或 ASN。`RecordStore::put` 不提供发送者、Kad 本地记录来源为 None 时，只能使用 global 额度，不宣称这些路径具有逐来源限额。逐 peer 拒绝不消耗其他 peer 的 global 额度；新来源只有在总量与跟踪容量均允许时才加入表。预算锁不覆盖验签、网络或存储操作。拒绝通过现有错误／有界诊断区分总量、来源与跟踪容量，观测不能产生无界来源标签。

该预算不等同于整个 Kademlia 入站吞吐、所有公开 API 查询启动／并发、跨重启配额或永久 sequence-floor 容量管理；已有出站发现工作预算继续独立生效。接收端持久水位、冲突隔离、tombstone 与 V1→V2 接纳迁移仍待实现，不能凭本预算代签。验证见[记录准入预算](../../../testing-manual.md#discovery-record-admission-budget)。

<a id="discovery-durable-publication-v2"></a>
### V2 本机发布序号与兼容边界

本机发布切片沿用 Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。`SignedPeerRecord` 保留 V1 原字段，增加可选严格 `publication` 元数据：`schema_version = 2`、正且单调的 `sequence`、首版仅支持的 `state = live`。没有元数据时继续使用原 canonical CBOR 与 `oasis7-peer-record-v1|` 签名域；存在元数据时只按 `oasis7-peer-record-v2|` 和完整 `(schema_version, sequence, state, PeerRecord)` 签名／验证。未知版本、未知状态、零序号或 V2 验证失败均拒绝，不回退 V1 验签，也不为旧记录补序号。首版不发布或接纳 tombstone 语义；完整撤销协议仍由上文生命周期合同承接。

启用由本机网络配置显式选择。legacy V1 保持原行为；V2 要求固定 identity key、record 模板与私有持久发布目录，已发布的标准 host 网络路径通过 typed `DevelopmentRuntimeConfigV1.p2p_peer_record_publication`／CLI `--p2p-peer-record-publication` 显式选择 `v1_legacy` 或 `v2_durable`，旧配置默认 legacy；V2 使用稳定 runtime root 下的 `p2p/peer-record-publication`，经 node adapter 传给 net。独立 `FormalLiveConfigV1` 当前不构造该 libp2p 网络，此开关不宣称已给正式 authority profile 接通 P2P。持久绑定只固定 peer/public key 与 world/network，不把整个可变声明模板当成身份锚；同域同身份调整 TTL、role 或来源等声明后可重开并以更高序号发布，仍须通过本机当前角色／地址 policy。旧未决载荷按其原始完整字节与固定域验证，不能拿新模板改写旧序号。初始化创建的目录链逐层持久同步父目录，并在写入前拒绝 symlink 祖先；目录持有稳定 OS 生命周期锁；并发第二实例、symlink、权限或绑定异常、损坏状态与序号耗尽均阻断 V2 启动／发布，不自动重建或降级 V1。

首次启用须以一次性的 `--initialize-peer-record-publication` 操作显式建立绑定的私有发布状态；该操作要求选择 `v2_durable`、持久 identity 与固定 runtime root，只初始化后退出，不启动世界或网络，也不把初始化动作写入可重复执行的 typed runtime 配置。初始化和普通启动须应用相同 manifest 默认值后再固定 world/network，不能以默认 world 初始化另一绑定。初始化在创建永久锁和状态前，以私有、有界、canonical 的同域 `Initializing` 意图原子落盘并同步目录；初始化者持有该意图 inode 的独占锁，续办者必须先获得该锁并验证完整意图。锁后及发布前核对 fd 与当前路径的 device/inode 相同；意图发布使用原子 no-replace 语义，绝不覆盖已有意图，防止已 unlink 的旧 fd 与替换路径分属不同锁。仅当同绑定且没有已发行材料（状态缺失且无 Ready，或状态为零序号且无 pending）时，显式初始化可续办中断阶段；损坏／错绑定意图、已发行状态或 Ready 缺失原状态均拒绝归零。完整 state0 与 Ready 同步后，仍持锁持久清除意图；普通打开在取得稳定锁前后都拒绝未完成意图。最早暂存文件仅在无永久锁／state／Ready、文件有界且私有、独占锁可得时处理，不能泛删目录。初始化续办中，完整但未知版本／字段／状态或损坏的临时意图与临时 state 必须保留原件并拒绝；仅可证明为写入截断的未发行暂存内容可按上述前置条件续办，不能把 schema 解码失败统称为崩溃残片。重复初始化已有完整状态须在创建意图前无副作用拒绝，包括普通实例正在持锁时；不得留下新的意图阻塞后续正常打开。日常 V2 启动只打开既有目录、稳定锁与状态；完整目录、state 或 marker 丢失／不一致均报错，不能重新建空目录和零序号作为恢复。丢失原状态时恢复原件；初始化入口不是同身份重置或丢失状态的修复方式。显式持钥者恶意重建／回滚整个存储不由普通本地存储保证覆盖。首版仅在已实现私有权限与目录 fsync 合同的平台启用 V2，其他平台明确拒绝该模式，legacy V1 不受影响。

统一发布器覆盖初始、周期、监听地址、relay 与外部地址变化等本机入口：先以文件和父目录 fsync 原子预留递增序号及完整物化 payload，再签名并持久保存原 signed record，之后才允许对外 put。崩溃恢复只能重签同一预留 payload，或明确消耗该序号留下空洞；不能以新地址／时间覆盖旧序号。网络重试与恢复重发保持原 signed bytes；新地址或下一周期属于新发布意图，可用更高序号替代发送结果不确定的旧意图。过期预留不得通过重写时间重新生效，须消耗原号后另分配。Kademlia `Quorum::One` 不是全网安装确认，不以丢失 ACK 无限阻塞后续新发布；状态和未决材料有固定字节上限，不保存无界发布历史。V2 的远端 `RR_GET_LOCAL_PEER_RECORD` 查询只读取内存中已有、未过期且匹配当前声明与地址的持久签名原件；缺失或过期时返回不可用，等待本机刷新。远端查询不得签发新序号、签名或触发磁盘读写／fsync，避免未受信 peer 驱动同步持久化。转发远端缓存记录保持远端原签名与序号，不经过本机分配器。

该切片只提供正常持久存储上的本机发行连续性，普通文件与 OS 锁不抵抗同密钥／磁盘的恶意快照回滚。接收端跨重启 sequence 高水位、同序冲突隔离、tombstone、V2-only 接纳迁移与可信时间仍待接入两个准入入口及独立保留策略；能够验证 V2 签名不表示已具备这些防重放保证。验证方法见 [持久发布验证](../../../testing-manual.md#discovery-durable-publication-v2)。

<a id="discovery-peer-record-implementation-boundary"></a>
### 当前实现边界

[接收链](../../../crates/oasis7_net/src/libp2p_net/discovery.rs)保留发布者的 `SignedPeerRecord`，缓存、DHT republish 和 RR 转发均不添加本地来源标签。[本地观察](../../../crates/oasis7_net/src/libp2p_net/peer_discovery_observations.rs)按原 signed payload 精确关联 DHT 实际响应上游、直接 record-exchange 对端或缓存代理对端，去重保存末次观察时间；同一 peer 的新 payload 不继承旧观察。本地 seed 身份仅来自当前 bootstrap 配置，配置移除后例外消失，不从远端 `StaticBootstrap` 自述或代理节点的信任状态继承。

本地观察最多保存 1024 个 peer、每个 payload 最多 8 个 ingress，单条序列化 signed payload 最多 16 KiB；到限驱逐最旧观察（不提高剩余记录信任），超大记录或已跟踪 peer 的观察时钟回退拒绝接纳且不替换原记录。该容量是本地观察预算，不是整个发现候选、连接和持久防回退状态的完整容量保证。

[健康与集合准入](../../../crates/oasis7_net/src/libp2p_net/peer_manager_active_set.rs)不把远端 `discovery_sources/source_operator/source_asn` 当可信独立来源。实际通道可显示为本地观察，但没有可核验控制域依据的通道／上游不计独立来源，并输出 `DiscoveryIndependenceUnverified`；当前没有受信 operator/ASN 映射，公开归属字段为 unknown，集中度统一计入 unknown 桶。仅受信 seed 配置可贡献一类 bootstrap 来源；多个 seed 不自动证明多个控制域。即使来源计数阈值设为零，未知独立性的记录也不显示 healthy。

来源不足的健康状态与已建立会话的传输准入分开：允许符合实际路径及 unknown 域预算的既有会话继续获取材料，健康仍降级。每个会话都计入路径、relay、operator/ASN 分母与桶；无 record 或远端自报 `observer_light` 不能豁免这些预算。候选按稳定 peer ID 顺序准入，配额拒绝进入 quarantine，不以缺 record 提前跳过统计。该策略改变旧版依赖自述标签的 healthy 结果及 observer 配额豁免；没有新增 wire 字段或重签旧记录。

[PeerRecord](../../../crates/oasis7_proto/src/distributed_dht.rs) body 保留既有 `published_at_ms/ttl_ms`；legacy V1 签名不含 sequence。新增可选 V2 envelope 与[本机持久发布器](../../../crates/oasis7_net/src/libp2p_net/durable_peer_record_publisher.rs)，由标准 host 显式配置、首次初始化后打开现存状态，覆盖正常重启发行连续性；正式 authority profile 的 P2P 组装不由该入口代签。libp2p 已接入上述 v1 时间策略、到期缓存过滤、DHT store 转发检查与记录路径租约，候选缓存上限 1024、单条 16 KiB。活动会话保留限制性元数据且不再贡献来源；测试内 InMemoryDht 并非该真实网络时间入口。持久可信时间、首次观察与配置版本、rendezvous ID-only 来源链、接收端持久 sequence/tombstone/高水位、rendezvous 协议操作总预算、跨重启配额和跨重启迁移仍待实现；自动 DHT／RR／缓存重发布采用[共享发现工作预算](#discovery-work-budget)，限额延后不清空已建立会话；该预算不覆盖全部网络操作。缓存候选按[到期前刷新策略](#discovery-record-refresh)尽力提前获取新材料，保留短 TTL／cooldown 和失败时可能到期的限制。本切片不宣称完整来源独立性、抗 Sybil 或生命周期保证。验证方法见 [本地发现信任边界](../../../testing-manual.md#discovery-observation-local-trust-boundary)，三节点 loopback TCP 不代签真实公网或独立物理故障域。

## Peer Reachability Contract Normalization
本节是 2026-06-15 iroh-inspired follow-up 的设计落点。借鉴点只限于 identity-first address boundary、显式 selected path 与 reachability evidence；不引入 iroh 依赖，不替换 libp2p，也不采用 iroh relay/DNS/Pkarr 默认。

### Contract 目标
`PeerReachabilityContract` 是 oasis7 内部归一化视图。本阶段实现范围是 signed peer record 的 identity-first record normalization：把远端声明的 identity、role/deployment/reachability、capability lanes 与 ranked transport paths 收到一个内部 contract 中，避免 path ranking/dedupe 逻辑散落。目标态再逐步把 runtime-observed selected path、local deployment/role policy 与 network-tier publish policy 作为显式输入接入同一投影边界。

| 字段 | 来源 | 用途 |
| --- | --- | --- |
| `peer_id` | signed peer record / libp2p identity | 稳定身份，不随地址漂移 |
| `deployment_mode` | peer record；目标态叠加 local policy | 判定 `public/hybrid/private/relay_only/validator_hidden` 公开面 |
| `node_role` | peer record；目标态叠加 local role admission | 判定 validator/sentry/relay/full-storage/observer 权限 |
| `reachability_class` | peer record declaration；目标态叠加 local observation | 表达当前可达性分类；远端声明不可覆盖本地探测 |
| `capability_lanes` | peer record effective lanes | request peer selection 与 lane serve/request gate |
| `ranked_paths` | `TransportPath` normalization | 当前首版实现的 canonical path ranking/dedupe；`direct -> hole-punched -> relay` |
| `publish_policy` | 目标态：deployment/network-tier manifest | 决定哪些地址可公开、哪些只能作为 allowlist hint；本阶段不改变 publish policy |
| `selected_path` | 目标态：runtime path selector/status projection | 当前正在使用的 path kind/flavor 与 selected-at 时间；本阶段由 status projection 输出 |
| `claim_boundary` | matrix/status projection | 指出该 peer/path 能支持的 claim 类型，例如 `exact`, `proxy`, `manual_lab_required` |
| `source_evidence` | 目标态：[本机来源观察](#discovery-evidence-boundary)，与远端声明分开 | 来源新鲜度、控制域独立性与未知原因；不把远端标签或他人本地观察当作本机 verified 事实 |

### 真值链
1. `SignedPeerRecord` 继续是远端声明与签名真值，原签名载荷不可修改；声明本身不证明来源独立性。
2. `TransportPath` 继续是 libp2p path materialization 与排序真值。
3. 本阶段 `PeerReachabilityContract` 归一化 signed peer record 与 ranked `TransportPath`，不改变 wire schema、publish policy 或 network-tier admission。
4. request peer selection 可先消费 `ranked_paths` 投影；debug/status/matrix 继续消费现有 runtime/status truth，但不得重新定义 direct/hole-punched/relay 标签。
5. 目标态若需要 local policy、selected path age、transition reason 或 publish boundary，先扩展 contract 输入/投影，再选择性投影到 status；不要先新增第二套 status truth。

### 非目标与防重复真值
- 不把 libp2p `Multiaddr` 替换成 iroh address 类型。
- 不让 matrix 脚本或 triad report 反向定义 canonical reachability。
- 不把 relay reservation 等同于 `public reachable`。
- 不把 proxy drill 成功等同于 physical NAT/CGNAT 覆盖。
- 不导出 unbounded per-peer Prometheus labels；peer-level 明细只允许 bounded snapshot/top-N 或 debug-only artifact。

## Reachability 生命周期
1. 启动时通过 bootnode / relay / observed addr 获取自我外部视图。
2. Reachability service 判定 `unknown/public/private/symmetric_nat`。
3. 能 direct 的节点直接发布 public addrs。
4. 不能 direct 的节点尝试 hole punching；成功则发布 punched-capable addrs。
5. 仍失败则进入 relay reservation，发布 relay route 并将自身标记为 `relay_only` 或 `validator_hidden`。
6. path quality 持续观测；若 direct 恢复，优先切回 direct，但保留 relay 作为热备用。

## 路径选择与传输策略
| 优先级 | 路径 | 适用场景 | 说明 |
| --- | --- | --- | --- |
| `P0` | direct QUIC | public/hybrid、已恢复直连 | 低延迟主路径 |
| `P1` | hole-punched QUIC | NAT 但可打洞 | 对业务透明，仍视作 direct-like |
| `P2` | relay QUIC/TCP tunnel | CGNAT、企业内网、临时故障 | 必须预算和多样性控制 |
| `P3` | delayed sync fallback | 极端受限环境 | 只允许 sync，不得承担实时 validator lane |

设计约束:
- UDP 可作为 QUIC/datagram 加速面，但不得作为唯一真值 path。
- transport/session key 应频繁轮换，不等价为 node identity 或 consensus signer。

## Traffic Lanes
| Lane | 典型载荷 | peer subset | QoS 特征 |
| --- | --- | --- | --- |
| `consensus gossip lane` | proposal、vote、commit、finality hints | validator、sentry、少量高分 full node | 低抖动、低队头阻塞 |
| `sync lane` | header range、block range、state checkpoint | full-storage、sentry、private full | 可重试、支持长流 |
| `blob/state lane` | DistFS、DA、snapshot、proof | storage、archive、proof provider | 独立限速，不能压制 consensus |
| `control lane` | heartbeat、peer record refresh、reachability probe | role-aware | 高优先、低带宽 |

实现落点（2026-04-02 / P2PARCH-4）:
- `oasis7_proto::distributed_net` 现在冻结 `NetworkLane` / `NetworkLaneQosClass` / topic+protocol classifier，作为 lane registry 的共享真值。
- `PeerRecord` 现在显式支持 `capability_lanes`；若旧 record 未声明，则按 `node_role` 回填默认 lane capability，保持向后兼容。`observer-light` 只默认声明 `consensus_gossip + control`，不再被当作 `sync/blob_state` 服务提供者。
- `oasis7_net` 现在按 lane 选择 subscription inbox 配额，并在 req/resp 路径优先筛掉不具备对应 lane capability 的 peer record。
- `oasis7_node` 现在把 replication / consensus 的 topic-protocol 绑定提升为 traffic lane registry，并在 runtime config 的 `node_role_claim` 上执行 lane publish/subscribe/request/serve 权限校验；`observer-light` 仍可主动请求 `sync/blob_state`，但不会注册对应 serve handler，也不能通过 `feedback_p2p` 绕过 `blob_state` lane 的 publish/subscribe gate。

<a name="anti-eclipse-anti-spam-基线"></a>

## Anti-Eclipse / Anti-Spam 基线
- Active peer set 的多源健康要求至少来自两类有独立性依据的本机实际 discovery 观察，按 [来源证据合同](#discovery-evidence-boundary) 去重；不足时如实降级，不能用远端标签补齐。
- 同一 operator、同一 ASN、同一 `/24` 的 active peer 占比默认上限 `25%`。
- validator_hidden 至少维持两条独立 ingress path，且不能同属一个 relay-domain。
- relay 采用 budget 和 quota；blob/state 流量不得吞噬 consensus/control 预算。
- 发现异常时，peer manager 要按阈值把节点置为 `suspect` 或 `blocked`，而不是继续乐观接纳。

以下历史落点描述既有 substrate。其 source/operator/ASN 输入仍有 [当前实现边界](#discovery-peer-record-implementation-boundary) 所列差距；不能据历史 milestone 或标签集中度测试宣称已验证来源独立性。

实现落点（2026-04-07 / P2PARCH-5 第二个切片）:
- `oasis7_net` 新增 `PeerManagerPolicy` / `PeerManagerPeerHealth` / `PeerManagerHealthIssue` substrate，在 libp2p worker 内基于已发现 peer record 与 active transport path 计算本地 peer health snapshot。
- 当前已接线的 fail signatures 包含：`single-source active set`、单 peer `single-source discovery`、IPv4 `/24` 集中、relay-domain 集中、operator 集中、ASN 集中、relay budget 超限。
- runtime 默认把同一 `operator / ASN / /24 / relay-domain` 的 active peer 占比 `>25%` 视为 `suspect`，占比 `>=50%` 视为 hard-`blocked`，并在 peer health 中保留触发阈值。
- req/resp peer 选择现在会在 lane capability 过滤后，继续优先选择 `active/candidate` peer，把 `suspect` 压到最后，并直接排除 hard-`blocked` peer；只有 `MissingPeerRecord` / record-exchange-pending 这类 soft-`blocked` peer 会在没有更健康候选时作为 bootstrap fallback。
- discovery ingress 现在不再对 `RoutingUpdated` / rendezvous registration 暴露的裸地址做 speculative dial；runtime 会先拿到并校验 signed peer record，再按 peer health 决定是否拨号，避免 `MissingPeerRecord => Blocked` 语义被旁路。
- `suspect/blocked` peer 现在也不会污染 discovery dial dedupe 状态；同一地址若后续随更健康的 peer record 刷新回来，仍可重新进入首拨决策。
- active-set quarantine 现在已开始生效：当已连接 peer 刷新为 `suspect` 或已验证的 hard-`blocked` 时，runtime 会主动 `disconnect_peer_id`，并在 `ConnectionClosed` / `OutgoingConnectionError` 上抑制 failover 与 retry，避免 quarantined peer 被本地状态机立即拉回 active set。
- peer health 对外发布前会先剔除未准入 active peer：待校验的 `MissingPeerRecord` 连接与本轮判定出的 quarantined active peer 不再参与同轮 `/24`、relay-domain、relay budget 的健康统计，避免瞬时污染其他健康 peer。
- chain runtime 默认 peer record 现已支持 `source_operator/source_asn` 输入，peer manager 会在本地归一化标签后参与 diversity 统计。
- runtime 现已为 `blocked` peer 维护可调试的 block artifact，至少跨 peer-manager 重算保留 `peer_id/status/issues/path/operator/asn/first_blocked_at/last_blocked_at/last_cleared_at`；当前仍未升级为跨重启 banlist 或 release-gate 证据存储。
- `P2PARCH-5` 当前已按 runtime substrate milestone 收口；剩余 `mixed-topology / fail-signature` required/full 套件与 release evidence contract 不再阻断 `P2PARCH-5` 关闭，而是继续留在 `P2PARCH-6/7`。
- `P2PARCH-6` 现已新增 `scripts/p2p-mixed-topology-matrix.sh`：QA 会把 `private/validator_hidden/relay_only` 角色边界、bootstrap poisoning、relay exhaustion 与 path failover 组装成 `required` exact matrix，再把 triad/triad_distributed 的 disconnect/restart longrun 组装成 `full` proxy matrix。该 matrix 会明确标注 `proxy != dedicated sentry/NAT lab`，避免把当前可执行近似 drill 误写成最终 mixed-topology 实证。

## Path Behavior Matrix Taxonomy
Matrix 是 evidence/claim taxonomy，不是 runtime reachability truth。它的职责是把“当前证据能支持什么说法”写清楚。

| 维度 | 允许值 | 说明 |
| --- | --- | --- |
| `evidence_class` | `exact / proxy / manual_lab / real_env / unsupported` | `exact` 为 deterministic repo test；`proxy` 为当前可执行近似 longrun；`manual_lab` 需要专门 NAT/sentry lab；`real_env` 为同窗口真实网络证据；`unsupported` 明确不承诺 |
| `path_expectation` | `must_direct / may_direct_must_recover / must_relay / must_not_publish_public_direct / manual_lab_required / unsupported` | 描述粗粒度 claim；case-specific route sequence 留在 `expected_route` |
| `reachability_pair` | `public_direct / home_nat / cgnat / relay_only / validator_hidden / cloud_public / mixed_validator_observer` | 描述 topology pair 或角色组合 |
| `degradation_class` | `none / sentry_loss / relay_exhaustion / degraded_latency_loss / restart_pause_disconnect / bootstrap_poisoning` | 描述弱网、故障或攻击近似场景 |
| `claim_boundary` | `required_exact / full_proxy / physical_nat_pending / shared_window_partial / public_testnet_blocked_or_ready` | 直接限制对外或 release gate 语言 |

Gate 语义：
- `test_tier_required` 只接受 `coverage=exact` 且 deterministic 的 case。
- `test_tier_full` 可以包含 `proxy` 和 `real_env`，但必须在 summary 中写明哪些 claim 仍需要 `manual_lab` 或 dedicated evidence。
- `public_testnet` 或 `shared_devnet` 升级 claim 必须同时引用 matrix summary、same-window evidence 和 producer/QA pass-uplift decision；不能只改 case 状态。

## Reachability Status Observability Projection
Status observability 只投影 contract 与 runtime-selected path 的 bounded summary，默认接入现有 `/v1/chain/status.observability` 与 triad merged summary。

建议字段：
- `selected_path_kind`: `direct / hole_punched / relay / unknown`。
- `selected_path_age_ms` 或窗口摘要，避免 per-peer 高基数序列。
- `path_transition_counters`: `direct_to_relay`, `relay_to_direct`, `direct_to_hole_punched`, `hole_punched_to_relay`。
- `active_path_mix`: 当前 direct/hole-punched/relay active counts。
- `recent_fallback_reason`: bounded enum，禁止自由文本无限扩张。
- `reachability_confidence`: `observed_direct / relay_reserved / punched_recently / proxy_only / manual_lab_required` 等 operator-facing 摘要。
- 目标新增 bounded summary：来源证据类别与 unknown 数量、有效/过期/撤销记录数、缓存使用量/上限、刷新失败与时间/序号/版本拒绝计数。peer ID、record digest、operator 自报字符串不得成为无限 metrics label；诊断只保留有上限的明细。上述新增字段尚未由本次文档变更实现。

边界：
- byte split 只能在现有 metrics scope 可证明时输出；不得把 wire/control-plane bytes 说成 NIC-level overhead。
- status 不是第二套健康权威；release claim 仍回到 matrix/shared-network gate。

## 适配多链型的数据面
| 适配器 | 典型链型 | 说明 |
| --- | --- | --- |
| `MeshGossipAdapter` | Ethereum/Cosmos 风格 | 使用 topic mesh + req/resp sync |
| `CommitteeDirectAdapter` | HotStuff/BFT 风格 | 委员会成员之间建立更强的定向链接 |
| `TreeBroadcastAdapter` | 高吞吐 leader-fanout 风格 | 上层控制广播树和 fanout，不重写底层 reachability |
| `BlobAvailabilityAdapter` | DA / DistFS / proof network | 独立 blob/state lane 与预算 |

## 对 oasis7 的迁移含义
1. `oasis7_node` / `oasis7_net` 不再暴露“静态 UDP peer 列表就是主路径”的默认语义。
2. consensus、replication、DistFS 都通过统一的 traffic-lane API 请求逻辑连接，而不是各自决定 transport 真值。
3. 当前 triad / legacy shared-network rehearsal / hosted access 都转成 deployment profile，而不是继续依赖特例脚本。
4. 本地家宽节点默认走 `private` 或 `validator_hidden` 模式，不再被迫伪装成 `public`。

## 当前结论
- 当前允许:
  - `public-chain-grade private-reachability architecture is specified`
  - `mixed-topology is a first-class target`
  - `validator no-public-IP operation is architecturally allowed via sentry/relay model`
- 当前禁止:
  - `all nodes must expose public IP`
  - `single relay dependency counts as public-chain-grade`
  - `current implementation already satisfies this architecture end to end`

## 需求承接与验证范围

承接表只列上述既有设计实际承担的子义务，不改变产品功能状态，也不宣称覆盖产品要求的全部验收。验证表是方法与证据目标，不是本次运行结果。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 无公网入站的 validator core 通过 sentry/relay 参与；转发、存储和 observer 不获得共识 signer 或私有控制权限 | [设计条款](#节点角色模型) | P2P 角色策略、validator governance；QA/ops 同拓扑验证 | 不覆盖公开 validator 准入、完整 pruning/archive 及用户模式 UI |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 公网 IP 不是通用参与前提；保持 direct、hole-punched、relay 顺序及 delayed-sync 的 lane 限制 | [设计条款](#路径选择与传输策略) | runtime path selector；operator 维护允许的部署 profile | 不保证任意 NAT pair 可直连，不由 relay reservation 推导公网可达 |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | peer manager 根据既有 source/operator/ASN/subnet/relay-domain 与 budget 规则隔离异常 peer，远端不得覆盖本地 admission | [设计条款](#anti-eclipse-anti-spam-基线) | P2P peer manager；QA fail-signature cases | 不证明跨重启 banlist、经济安全或完整抗攻击能力 |
| [PRD 发现证据边界](mainnet-private-reachability-architecture.prd.md#discovery-evidence-boundary)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 来源独立性由本机实际观察及可核验控制域依据建立；seed 例外不改签名、不授予世界或角色权威 | [发现来源证据与签名不变性](#discovery-evidence-boundary) | net ingress/peer manager；本地 seed policy 与受信 operator/ASN 依据 | 已接入不变签名、本地 seed 与 DHT/RR 观察、未知域健康和会话预算；受信控制域映射、持久时间与完整生命周期仍待实现，不覆盖完整抗 Sybil |
| [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | TTL/时钟/序号/撤销在接纳、缓存、转发及拨号入口一致；水位重启不降、缓存有界、版本迁移不改旧签名 | [Peer record 接纳、刷新与版本迁移](#peer-record-lifecycle) | proto wire；net 验证与缓存；chain-runtime 发布持久化；受信时钟/本机存储 | 现有 v1 时效接纳／缓存／转发／拨号已接线；V2 本机发行 wire／持久化按下列独立条款接入，接收端持久迁移仍待实现；不把普通本地存储当硬件防回滚或 validator 防双签机制 |

| [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 记录解析／验签／接纳共享有界窗口，真实来源限额与 global 限额不因入口切换重置。 | [本进程准入预算](#discovery-record-admission-budget) | net DHT store、DHT 结果与 RR 实际入口 | 不证明跨重启配额、全 Kad 请求预算或接收端持久防重放。 |
| [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 本机 V2 发布预留／签名先持久化，所有重发布入口共享绑定与递增序号；V1 原签名语义保持不变。 | [V2 本机发布序号](#discovery-durable-publication-v2) | chain-runtime / node adapter / net publisher / proto；本机私有存储与 OS 锁 | 不证明接收端持久防重放、tombstone、可信时间或恶意整机回滚安全。 |

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | [设计条款](#节点角色模型) | 无公网入站的 validator core 通过 sentry/relay 参与；转发、存储和 observer 不获得共识 signer 或私有控制权限 | [testing-manual S9B](../../../testing-manual.md#testing-p2p-mixed-topology-matrix)；在同一 source/config 上执行 `./scripts/p2p-mixed-topology-matrix.sh --tier required`，核对 private/validator_hidden/relay_only 的 exact role/lane policy cases；不证明 validator 已经由 sentry/relay 实际连接或 signer 安全，仍需适用 real-env/dedicated 证据；full proxy 与 dedicated lab 分开判定 | required/full summary、case evidence class 与失败签名 | 不覆盖公开 validator 准入、完整 pruning/archive 及用户模式 UI；本轮未运行行为测试 |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | [设计条款](#路径选择与传输策略) | 公网 IP 不是通用参与前提；保持 direct、hole-punched、relay 顺序及 delayed-sync 的 lane 限制 | [testing-manual S9B](../../../testing-manual.md#testing-p2p-mixed-topology-matrix)；required path-failover cases 核对 route sequence；full 依 S9B 选择 proxy/real-env，物理 NAT 覆盖须另有 dedicated lab | matrix summary 的 path_expectation、expected_route 与 claim_boundary | 不保证任意 NAT pair 可直连，不由 relay reservation 推导公网可达；本轮未运行行为测试 |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | [设计条款](#anti-eclipse-anti-spam-基线) | peer manager 根据既有 source/operator/ASN/subnet/relay-domain 与 budget 规则隔离异常 peer，远端不得覆盖本地 admission | [testing-manual S9B](../../../testing-manual.md#testing-p2p-mixed-topology-matrix)；同候选 required bootstrap-poisoning、relay-exhaustion 与角色边界 cases；核对 suspect record 的 dial dedupe、预算与集中度判定；required 不证明 live 隔离执行或公网抗攻击 | case stdout/stderr/log、测试断言与 summary | 不证明跨重启 banlist、经济安全或完整抗攻击能力；本轮未运行行为测试 |
| [PRD 发现证据边界](mainnet-private-reachability-architecture.prd.md#discovery-evidence-boundary) | [发现来源证据与签名不变性](#discovery-evidence-boundary) | 自述不能成为独立来源；本地 seed 标记不能改变原签名或被第三节点继承 | 已实现签名不变与本地信任切片：按 [验证手册](../../../testing-manual.md#discovery-observation-local-trust-boundary) 执行；后续按同一手册补齐 PRA-EVIDENCE-01～03 的剩余条件；同一实现候选使用固定配置和可控时钟，执行 net 单元及三节点 record-exchange 合同测试，分别核对 cache、转发、selection 结果 | 原签名载荷 digest、第三方验签结果、实际来源分组及 degraded reason；记录 source/config/test ID | 三节点 loopback TCP 覆盖原签名经缓存转发及 seed 不继承；控制域映射、观察到期和完整 PRA-EVIDENCE-03 仍待实现，不证明公网来源独立性 |
| [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle) | [Peer record 接纳、刷新与版本迁移](#peer-record-lifecycle) | 所有入口拒绝过期、回退、冲突及降级；持久水位与缓存容量安全，失败不污染旧记录 | planned：在现有 [peer record 测试文件](../../../crates/oasis7_net/src/libp2p_net/tests/peer_record_tests.rs) 增补 PRA-RECORD-01～05；同一 proto/net/chain-runtime 候选使用确定性时钟、崩溃注入和超过配置上限的高 churn 输入，实际 TCP record-exchange 另核排队后到期 | 每入口拒绝/接纳状态、旧 digest/水位不变、重开后序号、缓存/字节/重试峰值；明确预算及 wire 版本 | 新 wire 与场景待实现，本次未运行；不声称真实公网时钟、恶意主机回滚或无限规模支持 |

| [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [本进程准入预算](#discovery-record-admission-budget) | 所有记录入口共享窗口，真实上游限额不可由自述身份绕过，拒绝不安装候选。 | [准入预算验证](../../../testing-manual.md#discovery-record-admission-budget)；确定性窗口、真实 TCP RR/DHT 与 store/API 共享路径 | 受测 candidate、入口／实际来源、准入与拒绝原因、窗口和跟踪数量、原候选不变；未知来源只报告 global 证据 | 不是跨重启配额、所有 Kad 操作预算或持久 floor 验收。 |
| [PRD 生命周期条款](mainnet-private-reachability-architecture.prd.md#peer-record-lifecycle)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [V2 本机发布序号](#discovery-durable-publication-v2) | 预留、签名持久化与发送之间崩溃不复用序号签不同内容；生产 host 显式启用并传递稳定目录，远端转发保持原件。 | 按 [持久发布验证](../../../testing-manual.md#discovery-durable-publication-v2) 执行同候选 proto/net/node/chain-runtime 测试；覆盖重启、并发、状态损坏／权限／绑定、序号耗尽、V1 固定向量、V2 篡改／未知版本、实际发布入口和原签名转发。 | 固定配置与 source、原始 payload／签名、重开前后序号、故障点与退出码；分别记录本地存储和真实网络证据范围。 | 普通文件不证明快照防回退；接收端高水位、tombstone、长期运行容量与公网故障域未由本机发布测试代签。 |

<a id="discovery-peer-record-validation"></a>
### 11.2 增量验收场景（待实现）

以下是实现 PR 必须补齐的目标场景 ID，不是已存在的测试函数名或本次运行记录。既有测试来源与层级见上表；peer health 派生断言可沿现有 [peer health refresh 测试](../../../crates/oasis7_net/src/libp2p_net/tests/peer_health_refresh_tests.rs) 扩展，测试文件存在不表示下列新增场景已实现。当前实现导航另见 [实现边界](#discovery-peer-record-implementation-boundary)；文档检查通过不能替代行为结果。

| 场景 ID | 输入与故障点 | 必须观察到的结果 |
| --- | --- | --- |
| PRA-EVIDENCE-01 | 单一上游提供多条有效签名记录，均自述 StaticBootstrap+Dht，并逐条更换 operator/ASN；另构造同一 operator 经两个通道转发 | claimed 标签不产生本地 seed 权限或独立组；已知相关来源合并，未知归属保持 unknown/degraded，不能变为 healthy |
| PRA-EVIDENCE-02 | B 的原记录不含 StaticBootstrap；A 的受信配置包含 B，A 缓存后转发给 C | A 的本地 seed 事实成立但 B 的签名载荷 digest 不变；C 验签通过且只登记自己的 peer-exchange 观察，不继承 A 的 seed 例外 |
| PRA-EVIDENCE-03 | 两个有独立依据的实际入口、重复转发、观察到期和 seed 配置移除；冷启动时来源尚不充分 | 有效独立观察才贡献计数；重复不增、失效即重算；冷启动在有界例外中获取材料并保持 truthful degraded，不授予共识或 checkpoint 信任 |
| PRA-RECORD-01 | TTL 为零/负数/超过 1h、时间溢出、未来偏差边界、恰好到期、缓存命中后或拨号排队后到期、已知时钟回退 | 对接纳/缓存响应/实际拨号一致判定；容差不延长 expiry；过期/时间异常不能使旧记录重新 fresh，失败保留原有效记录和水位 |
| PRA-RECORD-02 | sequence 乱序、同序同载荷、同序异载荷、更高序撤销及更高序重新发布 | 低序拒绝、同载荷幂等、冲突隔离且不替换；撤销淘汰旧路径，重新发布不复用序号；收到旧记录不延长 TTL |
| PRA-RECORD-03 | 发布端在预留序号/签名保存/发送之间崩溃；接收端在水位安装前后崩溃并重启 | 发布允许空洞而不复用已分配序号签不同内容；接收不暴露尚未持久保护的记录，重开拒绝有效期内的低序旧记录；损坏或可检测回退停止相应发布/接纳 |
| PRA-RECORD-04 | 旧版兼容窗口、已迁移 peer 收到旧版、清缓存/进程重启、兼容许可到期 | 兼容范围显式且有限，时间检查仍执行；旧版不覆盖新版本，不合成 sequence 或改签名；重启不恢复已退役许可 |
| PRA-RECORD-05 | 高 churn 超过记录/字节/观察/水位上限，恶意来源切换与重复刷新；DHT 单系统故障 | 各维度及累计轮次预算有界；仅在安全回收条件成立时清水位，否则拒绝新增；过期 dedupe 不阻止重新发现，单一发现故障不全量断开既有认证会话 |

## 决策、风险与未覆盖义务

采用 identity-first 和分层 substrate，使地址漂移不改变节点身份，且链型适配不重写可达性真值；代价是角色、lane、路径与 QA claim 必须分别验证。沿用上文“当前结论”：架构已指定不等于端到端已实现。runtime owner 在 contract 输入或 status 投影变化时复核归一化边界；QA/ops 在拓扑、NAT 条件或发布 claim 变化时重核 exact/proxy/real-env 与专用实验室证据。用户模式与风险确认仍回到配对 PRD 及 S9C，不由本表替代。

本次选择将远端签名声明与本地观察分存，避免本地补标签破坏可转发签名或将自述升级为拓扑证据；代价是必须维护有时效的来源依据，并允许证据不足时保持 degraded。TTL、序号与有界缓存分别限制过期重放、更新回退和资源消耗；它们不能互相替代，也不能独立抵抗节点身份密钥失陷。新 wire 的协商/兼容开关、各 profile 的数值预算及实际部署依据须在实现 PR 固定并验证；本次不提升现有公开网络或 formal full-state sync 的支持范围。
