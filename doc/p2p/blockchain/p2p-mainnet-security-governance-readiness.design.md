# 主网安全、治理与创世就绪度设计

- 对应需求文档：`doc/p2p/blockchain/p2p-mainnet-security-governance-readiness.prd.md`
- 对应GitHub Issue/Project task truth：GitHub Issue / GitHub Project

## 设计原则

1. 将历史专题按 authority 合并，而不是将历史完成状态升级为当前状态。
2. 保持 `registry-first`：world-state registry 是治理 membership、stake、finality signer 与 controller policy 的运行真值。
3. 以 fail-closed 处理 signer/custody、genesis binding、ceremony 和 QA 证据缺失。
4. 将正式网络层级、运行步骤和公开沟通分别下钻到 formal-network-tier、runbook 和相应专业权威。

## 合并结构

| 历史主题 | 吸收后的稳定章节 | 不应丢失的限制 |
| --- | --- | --- |
| crypto baseline | 资产与密码学基线 | 局部原语/授权完成不等于系统级安全。 |
| MAINNET readiness | 主网进入条件 | `not_mainnet_grade` 直到所有跨域条件具备。 |
| signer custody | Signer custody 与治理真值 | preview bootstrap 不得成为 production custody。 |
| governance signer | Signer custody 与治理真值 | registry-first、admission、quorum、rotation/revocation。 |
| genesis ceremony | 创世、ceremony 与 QA | `not_mint_ready` 直到 binding、ceremony 与 QA pass。 |
| signed transaction authorization | 资产与密码学基线 | HTTP parsing、canonical auth proof、shared-layer re-verification 与 nonce enforcement，以及 native/wasm 同协议本地产签和缺失/partial bootstrap 的 fail-closed submission 必须同时存在；不等于 custody/mainnet。 |

## 实现和验证下钻

- 资产授权：本专题“资产与密码学基线”的 signed transaction contract。
- 客户端签名路径：native 复用本地 signer helper；wasm 使用 `window.__OASIS7_VIEWER_AUTH_ENV` 的受信本地 bootstrap，并以与 native 相同的 canonical payload/域前缀生成签名。缺失或 partial bootstrap 必须在页面侧阻断，不得发出 unsigned transfer POST；这不等同于 production custody。
- Node/replication 的可恢复签名合同：`doc/p2p/blockchain/p2p-blockchain-p2pfs-hardening.*`。
- formal tier、manifest 和 public-testnet checklist：`doc/p2p/blockchain/formal-network-tiers-testnet-mechanism.*` 及 companion runbook。
- registry 操作和 evidence：`scripts/governance-registry-drill.sh`、`scripts/governance-registry-live-drill.sh` 与 `doc/testing/evidence/`。

任何实现改动均须分别由 runtime、blockchain ops、QA 与适用的 product/LiveOps authority 审核；本文不提供实现替代方案或放行判断。

## 审读基线与当前差距

- Owner role：`runtime_engineer`、`blockchain_ops_engineer`；验证由 `qa_engineer` 独立核对。
- 本次文档/源码审读基线：`4e94163a3dff9ba69f6959b406ca3fcedf8aa097`，日期 `2026-10-15`。当前实现说明与历史阶段记录分别保留；本次不执行签名、部署、恢复或构建，不推导新的运行完成态。
- 产品关系只承接下表明确列出的局部义务；功能整体仍为部分覆盖，现有产品实现状态与原验收层级不变。
- 下表产品 N/A 仅限专业签名校验与 ceremony 证据方法，不表示转账玩家体验、资产权属或公开发行没有产品影响；这些消费关系未在本批准确建立，仍待对应产品与专业 owner 核对。原签名、资产授权、创世与 QA 专业义务全部保留，不因此豁免。

## 问题、目标与上游分工

目标是把局部 crypto、governance 与 genesis 证据连接到各自准入条件，同时保留 not_mainnet_grade / not_mint_ready。配对 PRD 的现行合同拥有安全与阶段判定；formal network tier 拥有网络分级；QA 拥有实际 pass；产品/LiveOps 拥有公开口径。本设计不生成任何准入授权。

<a id="readiness-current-capability-boundary"></a>
### 当前节点部署能力与后继目标

以下源码边界固定于 `b42a9e3fb8c8cd60679936c5f46c2fde6f62cf1e`；只说明本设计采用的实现输入，不声称已执行测试或完成部署。后续三个目标条款不得倒写为当前能力。

| 能力 | 当前实现边界 | 后继目标与未完成部分 | 源码依据 |
| --- | --- | --- | --- |
| validator 治理主体认证 | 四类 admission Action 有 controller ID 和状态机检查；通用 RuntimeAction 的 auth-required 白名单未包含它们；未证明公开 HTTP 可直接提交这些 Action | 所有外部输入路径验证 committed policy 下的治理 proof；不能将字段相等描述为完整主体认证 | [Action 合约](../../../crates/oasis7_agent_api/src/world_action/action.rs)、[通用提交](../../../crates/oasis7_node/src/node_runtime_core.rs)、[World 发布](../../../crates/oasis7_runtime/src/world/governance_registry_publication.rs) |
| 治理集合与本机应用 | World 可导出 effective registry；`supports_safe_runtime_update=false`、`applied_consensus_membership=None`，启动投影没有组件安装确认 | 在协议边界持久、原子安装并给出真实应用确认；在线更新与一般化 rotation 尚未建立 | [成员投影](../../../crates/oasis7_runtime/src/governance_membership.rs)、[启动 registry 接线](../../../crates/oasis7_chain_runtime/src/host/governance_registry.rs) |
| 私有 full-state grant | v1 绑定 issuer/world/chain/node/role/authority epoch 与最长 30 天 lease；更高 `grant_epoch` 只在客户端安装时排序 | 源端撤销代际、多 provider 完成条件及有界授权新鲜度 lease 尚未实现 | [grant v1](../../../crates/oasis7_client_api/src/world_service_full_sync.rs)、[源端授权](../../../crates/oasis7_chain_runtime/src/host/formal_live_full_sync.rs)、[安装](../../../crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync.rs) |

治理 World 是 membership、stake、signer 和 controller policy 的事实 owner；consensus/runtime 负责采用并证明实际应用；读取 grant issuer 只拥有被初始 activation trust 授予的读取授权职责；provider 执行读取门禁。此角色分配不把 grant issuer、RPC 服务或本机初始化提升为世界写者。网络参与和私有状态获取是不同权限；本机密钥的故障恢复由[节点身份设计](../node/node-identity-replication-contract.design.md#des-nir-bootstrap)负责。

<a id="readiness-registry-isolation"></a>
## 治理权限与状态转换

本节局部承接 DCS-003 的受治理 validator authority：world-state registry 优先于本地 seed/config，membership、stake、finality signer 和 controller policy 必须经治理验证。生命周期为 apply → approved_candidate → probation_ready → active → revoke/rotate；候选或 probation 不能代替 active。服务节点、local signer bootstrap 与单个有效签名不能取得最终写入权。完整公开服务暴露面、轮换证明与 BFT quorum 的组合仍须 P2P/runtime 与 QA 验证。

<a id="readiness-governance-authorization"></a>
### 治理主体认证与提交目标

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；承接[专业治理授权条款](p2p-mainnet-security-governance-readiness.prd.md#readiness-governance-authorization)与[REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)。以下是目标接口义务，尚未交付新的 proof wire schema。

治理命令必须带可复验的授权 envelope，其 canonical 签名域独立于 transport、consensus vote 和普通资产转账。签名覆盖协议版本、固定 world/chain/genesis、完整动作摘要、controller、授权 policy 摘要／版本、已验证 parent CommitRef、nonce 和有效提交位置范围；candidate、node、stake、旧／新 signer、activation 意图与撤销对象不能留在签名之外。阈值和准许的 signer 取自该世界的 controller registry，不接受调用方自报 threshold、临时 NodeConfig 或新 signer 自我授权。提交位置使用合法 profile 已定义的编码；BFT 使用 block height，受控单权威使用其正式提交位置，不能用 local tick 或机器时间制造位置。

验证一份候选提交的全部治理动作时，先从已验证 parent 状态和其中已合法排程的 policy 转换确定该提交位置的 policy，并在本批处理期间固定；BFT 高度 `h` 对应从已提交 `h-1` 解析的 `P(h)`。只有早先已提交且此刻合法生效的 policy 才能适用；本批产生的新 policy、controller 或 membership 不能给本批任何动作补授权。proof 的 parent、policy 摘要和有效位置必须与此上下文相符；排队期间上下文失效的动作须重新取得授权，不能通过执行顺序或本地缓存追认。nonce 检查在同一批次中按 canonical 顺序去重，并与成功的治理效果一起持久化；认证失败不消费 nonce、不发布 event 或 registry。

所有可接收外部输入的路径都必须保留并重验该 proof：HTTP／CLI 的早期校验只提供反馈，通用 NodeRuntime 提交和正式执行／复制接收仍按同一规则核验。执行层仅从已核验 envelope 构造治理输入，不能把 legacy raw Action 解码成功当成认证成功。历史 replay 使用原记录的 parent、policy 与证明，验证原来已成立的决定；不是要求旧动作满足当前 policy。bootstrap 的直接 registry 设置仅属于下述受约束首启路径，不能成为运行中绕过授权的入口。

公开候选申请如被允许，使用独立的无权威申请权限；批准、排程激活、轮换、撤销仍需要上述 controller policy。rotation 把旧／新 signer、节点与生效边界绑定在同一治理决定中。缺 proof、错误世界／parent／policy、threshold 不足、重复 signer、过期、nonce 冲突或 proof/action 不一致均 fail closed；unknown outcome 按原 action identity 查询，不生成第二个治理效果。

#### 当前受控 profile-v2 实现范围（#4975）

四类 validator admission 使用 `ConsensusActionAuthEnvelope::GovernanceAction`，签名协议为 `GovernanceAuthorizationV1`。固定 CBOR tuple 的域为 `oasis7.governance.authorization.v1`，覆盖版本、world/chain/genesis、精确 parent canonical position/record ID/Snapshot root、完整 controller registry 摘要、controller、nonce、有效 canonical position 范围和完整原始 typed Action；签名表不进入自身签名载荷。signer 和 threshold 只从已验证 parent Snapshot 中的 registry 取得，重复 signer wire key、非规范十六进制与超限 proof 被拒绝。受控位置来自 qualified parent 的 `position + 1`，不使用执行 height 或 local tick。

NodeRuntime 在入队前通过默认拒绝的 execution hook 调用真实 driver/World 验证；本地提案前重验已排队动作，并按 canonical action ID 顺序过滤过期 parent、重复或递减的 controller nonce，释放对应队列预算。正式执行与复制接收保留 typed proof，并在 World 再验。整个候选一直使用私有 verified parent 对象保存的原 controller registry；当前 World 中的新 registry 不能给同批动作补授权。成功效果、controller nonce 高水位和状态根投影由同一个 prepared publication 安装；失败不得消费 nonce 或发布治理效果。

治理事件保留 `GovernanceAuthorizedActionV1` 中的原始动作和 proof；发布／重放重新计算确定性事件，防止规范化字段、推导撤销对象或 epoch 被换掉。旧 Snapshot 的空 nonce map 与旧 ActionEnvelope 的缺失 proof 字段保持原有编码；新治理动作缺 proof 一律拒绝。历史无签名事件只兼容显式 replay 的旧前缀，首个有签名治理事件后不再接受无签名后缀。有签名 tail 必须附着原来的 qualified parent；普通无信任 `from_snapshot` 不能借 proof 自报的 parent 恢复授权，缺原 parent 时停下并走 verified formal Snapshot 恢复。跨多个 parent 的原始 journal tail 自动重放尚未实现，不能把单一 parent 的重放测试当成完整历史恢复验收。

这是受控 profile-v2 的认证切片：旧读者不理解新增 auth variant，必须先升级 verifier/runtime，再启用有签名治理提交；没有把旧 raw Action 或旧无签名历史改写为已认证决定。profile-v1、BFT admission、policy/key rotation、组件实际安装 ACK 和全入口公开服务验证仍是 #4975 的后续义务；本切片不新增公开 HTTP/CLI 提交端点，也不开放新的写权限。测试区分真实签名 qualified anchor 的 host 集成与仅检查执行顺序的 synthetic parent 单测，不声称实机独立故障域或部署验收。

源码：[签名合约](../../../crates/oasis7_agent_api/src/world_action/governance_authorization.rs)、[World 验证与重放](../../../crates/oasis7_runtime/src/world/governance_authorization.rs)、[提案前重检](../../../crates/oasis7_node/src/node_engine_governance_auth.rs)、[执行解码](../../../crates/oasis7_chain_runtime/src/host/execution_bridge/driver_replicated_input.rs)。Trace refs: `O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3`。

<a id="readiness-membership-application"></a>
### 成员应用、撤销与轮换目标

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3；承接[专业成员应用条款](p2p-mainnet-security-governance-readiness.prd.md#readiness-membership-application)与[REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)。集合生效算法的唯一 authority 是[P2P BFT 转换](../design.md#des-p2p-bft-validator-transition-v1)，本节只定义治理输入与运行组件安装。

正式目标 BFT 更新在高度 `h` 最终提交后固定于 `h+2` 生效，`h/h+1` 使用旧集合。首版同一 world 至多有一个尚未激活的 scheduled 转换；其他申请／批准可保留 pending，必须在前次激活后重新按当时上下文验证才可排程。`activation_epoch` 是申请意图：只有协议能确定 `h+2` 符合该 epoch 时才可提交 scheduled 更新；无法映射或已经错过边界时拒绝排程／保留未排程意图，不把 epoch 当第二个自行延迟的开关。节点不得本地改写已提交的 `h+2`。

| 逻辑状态 | 事实 owner 与证明 | 允许的运行行为 |
| --- | --- | --- |
| candidate／approved／pending | 已认证治理动作及已提交 World 状态 | 没有由此取得的新投票权 |
| scheduled | 高度 `h` 的合法提交，绑定旧／新 registry digest、`h+2`、stake 与 signer 映射 | 准备下一集合；继续按旧集合处理 `h/h+1` |
| protocol-effective | 经合法历史确认已经到达 `h+2`，目标集合由协议解析 | 网络必须使用新集合；本机尚未安装不推迟网络生效 |
| runtime-applied | 实际共识／写入组件完成一致安装并持久确认同一 source commit、set digest 与 activation height | 本机可在其实际角色及同步条件允许时使用新集合；应用确认本身不证明整体 readiness |
| application-blocked | 缺材料、冲突或安装失败；保留最后可信位置与 blocker | 到边界后停止本机投票／提议／相关追加，允许符合独立合同的只读；不得继续旧权重或降阈值 |

目标安装接口接受经证明的 source CommitRef、old/new set digest、activation height、适用 profile、成员/stake/signer 与依该角色策略导出的写入权限。先持久记录安装意图，再准备完整新视图，在协议边界以一个可恢复的原子切换提交该视图；签名器、vote/proposal 验证器及相关写者 allowlist 必须引用同一视图。fetch／读取权限仍按其独立 role policy 计算，不能复制 validator 集合冒充全部授权。任一消费者缺少必要能力或不能确认一致视图时，应用保持 blocked。

完成确认由真实使用该视图的组件产生，绑定 source CommitRef、activation height、实际 set digest、协议 epoch、runtime instance 与安装代际；从 config／World projection 复制字段不能构造确认。持久确认之后才发布 `runtime-applied`。重复同一安装幂等；同 activation height 的不同 digest 是冲突，不能最后写入者覆盖。crash/restart 从已提交转换与原安装意图恢复，若无法证明已切换则先停签，再验证／重装同一视图；不能只读取旧配置恢复投票。

撤销／轮换后，旧 signer 对 `h+2` 及以后新消息不再有效，但此前合法历史按历史集合验证。equivocation 报告本身只是证据，不触发本机单方面删 stake／改 quorum。受控单权威 profile 的 epoch、writer fencing 与接管继续由[受控 epoch 接管](../design.md#des-p2p-controlled-epoch-takeover-v1)拥有，本安装接口不得作为单权威到 BFT 的隐式升级。当前没有 safe runtime install API；上述状态是目标语义，不是现有 CLI 新增枚举。

<a id="readiness-full-sync-read-authorization"></a>
### 私有 full-state 读取授权与撤销目标

Trace refs: O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8；承接[专业读取授权条款](p2p-mainnet-security-governance-readiness.prd.md#readiness-full-sync-read-authorization)与[REQ-DCS-006](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-006)。本节使用既有独立 initial activation issuer 信任边界，只授予读取；不增加 world 写入、validator 或治理权。

**授权真值与接口。**目标 issuer 持久维护以 `(world, chain, authority_epoch, node_public_key, role)` 为键的授权记录，包含单调 `grant_epoch`、active/revoked、签名 policy revision/digest。issuance、replace、revoke 在同一个单调存储序列中串行决定，签名只有在对应决定持久后才可返回；重复同一请求返回原决定，代际相同但内容不同拒绝。provider 对 active grant 要求代际精确相等，同时继续验证 issuer、世界、当前 authority epoch、role、node 与原 lease。revoke 即使没有新 grant 也推进代际并置 revoked；较高代际 replace 不允许旧 grant 继续使用。authority epoch 改变仍要求新 grant，不能把权限代际当作 authority epoch。

目标管理入口在进入上述序列前核验由既有 issuer 信任边界授权的管理主体及操作域，授权绑定 world/chain/authority epoch、完整目标记录、操作内容和防重放请求 identity；HTTP／CLI 之外的内部调用也不能绕过此检查。读取 grant 的持有者、node key 的持有者或 provider 服务身份本身没有 issuance／replace／revoke 权限，不能以被撤销对象自己提供的 grant 取得管理权；缺管理授权、越域或重放冲突在授权记录变更前拒绝。本条约束后继接口，不声称当前已存在未经认证的公网签发入口。

授权 registry、revision 和完整 provider lease 签发记录必须绑定一个由 initial activation trust 独立认证的、不可回退的当前序列锚，使用受保护单调存储／独立 witness，不能只依靠 issuer 与 provider 各自可回滚的本地文件。锚绑定授权状态摘要、完整签发日志摘要与 issuer 安装代际；每次决定／签发先核验并原子推进该锚，旧 issuer 实例必须被 fence，不能继续签发可接受的新 lease。provider 的 fresh lease 需带该当前锚对本次 incarnation/nonce 的有效绑定，单有同一个 issuer key 的新签名不足。issuer 从旧备份恢复时必须先证明不低于已生效撤销与全部未到期 lease 的持久下界，无法取得锚或缺少匹配日志则停发；新 provider 或同时回滚的 provider 也不得接受旧授权。

目标在线读取授权签名者应使用由该既有信任根明确委托的专用受保护 key；委托绑定 world/authority epoch、读取域、准许 role、有效范围及 lease 上界，不授予世界激活／governance／vote 权。provider 同时验证委托与上述当前序列锚。v1 initial issuer 直接签 grant 的历史语义保持；新的 lease 机制不能要求把高权限 initial activation 私钥交给每个 provider 或常规请求进程。委托与锚的编码属于新能力版本的实现输入，当前 v1 不支持。

**有界新鲜度。**每个 provider 只有在持有 issuer 签发的短期授权新鲜度 lease 时才能放行私有数据。lease 绑定 provider 身份、运行 incarnation、随机请求 nonce、授权 registry revision/digest、world/authority epoch 和有限有效期；长度不得超过部署授权 policy 规定的 `max_authorization_staleness`，该值及可保证的时钟误差界必须在启用服务前明确、可读且可验证，缺失或无界则禁用该读取能力。此 lease 独立于最长 30 天的客户端 grant lease，不能用后者代替撤销传播上界。

issuer 原子记录每次签发的 `(provider, incarnation, lease-id)`、版本和最晚有效边界，lease-id 唯一绑定完整签名 lease；同 provider 的重叠 lease、旧 incarnation 和复制实例的未到期授权都不能被新签发覆盖。已持久 revoke 后不能再为旧授权状态续租。provider 以受校验的时间边界和从请求发出时开始计量的单调计时限制 lease，延迟响应不重置有效期，时钟回退／不确定性不能延长权限；截止判定所依赖的时间保证不能与被恢复的进程／VM 快照一同回退。进程重启必须以新 incarnation/nonce 在线取得 fresh lease，旧盘中的 lease 不可直接恢复服务。分区时只能使用尚有效的 lease；到期立即停止新的授权放行。无法验证 issuer 当前授权视图、未知 policy revision、摘要冲突或本地时钟保证失效时 fail closed，不能回退缓存 positive authorization。

**逐次放行和流边界。**请求者还须通过已认证 node session 或绑定本次 provider/world/请求摘要与新鲜 challenge 的持钥证明，证明控制 grant 指定的 node key；issuer 在 grant 上的签名不能替代请求者身份认证。manifest、object 与每个有界 chunk 在源端完成授权决定，核对该请求者、grant 和当前有效 lease 所绑定的 registry；连接建立、此前 manifest 成功或客户端配置更新不能代替权限检查。安装新撤销版本和 chunk 放行使用同一串行门禁：生效后不得产生新的允许决定，尚未放行的排队 chunk 中止。已经在此前合法放行并进入传输缓冲区的字节可能继续到达，撤销不删除已经下载的 Snapshot/Journal。provider 的应用确认必须指出该门禁的 cutoff，而不只是配置文件已写入。

| 撤销阶段 | 成立条件 | 不能声称的结果 |
| --- | --- | --- |
| requested | issuer 已收到请求，尚无持久决定 | 不能声称旧授权已失效 |
| revocation-committed | issuer 持久发布 revoked/更高代际并停止旧状态续租 | 分区 provider 的既有 lease 可能仍有效，不能声称全网停止读取 |
| enforcement-pending | 按 issuer 完整签发记录枚举所有仍可能服务旧授权的 `(provider, incarnation, lease-id)` 及最晚有效边界 | 同名 provider 的新实例 ACK 不关闭旧实例；发现列表删除、控制面不可达或客户端停止不等于隔离 |
| effective | 每个相关 lease 均有精确实例的门禁 cutoff 且其不可回退／旧实例 fence 可证明，或该 lease 的全部实际服务能力已可靠隔离，或该 lease 已在可信时间界下到期且受验门禁保证停止放行 | 不能收回以前合法放行的字节，不能省略未知实例／复制实例／未知 lease |

ACK 只关闭其明确绑定的 `(provider, incarnation, lease-id)`；同 incarnation 的多个 lease 也逐一列举。允许在 lease 到期前据 ACK 关闭时，provider 的 cutoff 必须处于不可随进程／VM 快照回退的保护边界，且可证明使用同一 lease 的旧／复制实例已不能服务。只有普通本机持久文件或新实例 ACK 时，不具备该提前关闭条件，必须保留 pending，直到对应旧 lease 的最晚有效边界确实过去。不能用 issuer 停止续租代替对已发 lease 的处置。

可靠隔离必须作用于该 lease 全部实际数据服务能力，例如经验证的服务停止／网络访问封锁；仅修改 inventory、DNS 或 bootstrap 列表不足。以 lease 到期完成撤销时，依据是 issuer 的完整签发记录、固定 TTL/不可回退时间界与 provider 到期停读合同的验证，不把“没收到响应”当停止证明。控制面无法确定是否仍有未过期 lease 时保持 enforcement-pending。返回结果应包含原请求、授权代际、policy revision、完成范围和剩余 `(provider, incarnation, lease-id)`／最晚边界；公开投影不暴露私有 grant 内容或持有者清单。

多 provider 的启动与重新加入先验证授权记录单调高水位并取得 fresh lease，再服务；恢复旧快照、切换 endpoint 或复用旧连接不能恢复已撤销代际。授予未撤销的其他 node/role 不随本次撤销扩大或无故丢失；独立 role 的撤销只影响被绑定范围。操作失败可按原 identity 查询／重试，不能以失败结果回滚已经持久的 revoke。

<a id="readiness-auth-genesis"></a>
## 接口、提交与恢复边界

资产请求的 public_key/signature 与 from_account_id 必须匹配；canonical auth proof 在 shared NodeRuntime 再验，余额、nonce、amount 与 same-account 规则仍执行。native/wasm 均先本地产签；缺失/partial bootstrap 在 POST 前失败，不降级 unsigned。客户端 bootstrap 不等于 custody。Genesis import 仅适用于空 execution world；已有 snapshot/journal world 只能走专用 registry migration/import，不覆盖恢复状态。Ceremony 的 slot、bucket、recipient/controller、signer policy 和 QA evidence bundle 缺任一项就保持阻断；pending_binding 等状态不能被历史 gate completion 升级。

“空 execution world”包含经既有 network bootstrap 身份约束核验、尚无 tick consensus record 且 journal 为空的 genesis-only 初始材料；生成 snapshot 本身不授权覆盖任意已有世界。一般 import 检测到已持久化世界且缺 registry 时拒绝，network bootstrap 只补缺失项，已有 registry 不覆盖。已有提交历史的迁移必须使用专用、经授权的转换，不能删 journal 或重命名目录伪造首启。当前落实位置是 [governance_registry.rs](../../../crates/oasis7_chain_runtime/src/host/governance_registry.rs) 的 `import_genesis_validator_registry`、`import_network_tier_governance_bootstrap` 与 `world_is_bootstrap_only`；这些检查不代替运行中治理 proof。

## 状态、事务与持久化

治理认证使用 committed parent policy 的不可变上下文；通过认证后的效果和 nonce 由 World 的同一次事务／提交持久化。成员应用拥有本机安装意图与组件确认，但不能改写链上 scheduled/effective 状态；只有全部适用组件使用同一视图才提交本机 applied。读取授权 registry、provider lease 签发记录和撤销决定由 issuer 持久保存，provider 高水位／cutoff 由 provider 保存；崩溃、恢复、重复请求不得降低任何授权代际。

以上是三种不同的运行状态，不共用一个“已批准／完成”标记。nonce 或 action outcome 未知时按原 identity 查询；成员安装失败先停签；revoke 已持久但传播未完保持 pending。正常恢复只续做原决定，不能生成新授权、回滚撤销或把启动配置当作安装确认。

## 部署、安全、兼容与剩余风险

production custody、rotation/revocation/audit、真实 registry activation、ceremony 和 QA pass 必须分别提供证据；本次没有运行这些动作，也不读取或生成 private signer material。兼容 local/dev/preview 便利路径不意味着允许它成为 production truth。单次 drill、三等权 topology 或 signature 原语不能证明完整 BFT/f=1、主网或 mint readiness；上述主网进入条件不因文档规范化改变。

## 质量、兼容与迁移边界

治理 proof 的签名数由所引用 policy 限定，payload/候选队列沿用入口有界限制；成员首版最多一个 scheduled transition。读取接口保留现有 chunk、object、history path 和总传输预算；增加授权检查不能把旧预算改成无限。issuer 的 provider 签发记录必须完整覆盖所有未到期 lease，不能用容量淘汰丢失撤销上界；容量不足时拒绝新签发。新鲜度 lease 的有限上界属于启用合同，实际延迟与可用性代价须在匹配部署验证，本文不宣称 SLO。

治理 envelope、成员安装能力与可撤销读取能力都需要显式版本／能力识别和消费者升级。正式入口在相应协议激活后拒绝无 proof 的旧治理输入；历史有效输入保留其原版本校验路径，不把新要求追溯应用于旧历史。新目标未实现前保持当前能力标识，不静默接受后再忽略。

`FormalNodeReadGrantV1` 的现行签名域、编码及最大 lease 不因本文改变。旧 source 不支持逐节点撤销或 provider freshness lease，要求本目标的客户端／部署必须拒绝降级；新协议激活前不能声称已提前撤销仍有效的 v1 grant。跨版本回退不得降低 issuer/provider 的授权高水位、重启已撤销服务或自动重发 grant；不能安全承接持久授权状态时停读并报告不兼容。当前 `supports_safe_runtime_update=false` 和 applied `None` 必须保留到真实组件实现及验收完成。

## 设计选择与剩余风险

采用 committed policy 认证、协议固定生效点和源端有界撤销，是为了使攻击／崩溃／分区路径与正常路径有同一个可复验决定。拒绝仅比较 controller 字符串、用 World effective projection 代替组件应用、用客户端换 grant 或 provider 列表变更代替撤销的方案。需要承担的成本是过期请求重新授权、未完成安装节点停止投票，以及授权服务分区超过 lease 上界后私有读取暂停；这些是权限边界的结果，不以放宽校验补可用性。

生产 signer custody、issuer/provider 的可信时间与故障域、实际网络隔离、全部版本兼容和 BFT 组合仍需实现及部署证据；协议目标与当前差距不能由本文件的 active 状态消除。本文不定义新的运维审批流程、任务台账或公开 readiness 结论。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | registry-first 与 validator activation 保留治理 authority，不从 local/config 或服务角色取得写权。 | [registry 隔离](#readiness-registry-isolation) | governance / runtime / QA | 仅 registry 优先级与 activation 子范围；服务暴露面、完整 BFT/rotation 证明未覆盖。 |
| [治理授权专业要求](p2p-mainnet-security-governance-readiness.prd.md#readiness-governance-authorization)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | 四类 admission 及 rotation 的外部输入按固定 committed policy、完整动作与 nonce 验权，拒绝 batch 自授权。 | [治理主体认证](#readiness-governance-authorization) | Runtime / Node / governance policy owner / QA | 受控 profile-v2 已接入签名 admission、固定 parent policy 和原子 nonce；BFT、rotation、组件 ACK、多 parent 历史恢复与公开服务全入口证明仍未完成。 |
| [成员应用专业要求](p2p-mainnet-security-governance-readiness.prd.md#readiness-membership-application)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | h 提交、h+2 生效与组件持久安装一致，轮换／撤销边界后不继续旧 signer／stake。 | [成员应用](#readiness-membership-application) | P2P [集合转换](../design.md#des-p2p-bft-validator-transition-v1) / runtime / signer / ops / QA | 目标；不实现共识算法，不把 current applied None 当空集合；不覆盖单权威到 BFT 升级。 |
| [私有读取授权专业要求](p2p-mainnet-security-governance-readiness.prd.md#readiness-full-sync-read-authorization)；Trace refs: O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8 | 每次放行验证源端代际、role 和有界新鲜度；撤销完成覆盖全部 provider 的应用／实际隔离／lease 到期。 | [读取授权与撤销](#readiness-full-sync-read-authorization) | Initial activation issuer / full-sync providers / client / ops / QA | 目标；不收回已放行字节，不授予 writer/validator/archive 能力，v1 无立即撤销保证。 |
| [资产与密码学基线](p2p-mainnet-security-governance-readiness.prd.md#资产与密码学基线) | 客户端先签名并阻止缺失/partial bootstrap；API 与 shared runtime 必须验签及 nonce 约束。 | [授权与创世](#readiness-auth-genesis) | client launcher / runtime / QA | 产品适用性：N/A；理由：本行承接专业资产授权合同，未建立独立玩家经济或发行承诺；范围：signed transfer 边界。native/wasm 与 shared-layer 完整联测仍需补实际证据。 |
| [创世、ceremony 与 QA](p2p-mainnet-security-governance-readiness.prd.md#创世、ceremony-与-qa) | 缺 binding、ceremony、QA evidence 时不进入 mint；drill 不签发 release verdict。 | [授权与创世](#readiness-auth-genesis) | blockchain ops / QA / product / LiveOps | 产品适用性：N/A；理由：本行定义专业证据与安全阻断条件，不宣布玩家开放；范围：ceremony/readiness。真实 mint、production custody 与公开发布未覆盖。 |

### 11.1 验证映射表

以下是验证要求与已有入口的对应，不是本次执行结果。源码/测试定义只提供静态核对；实际验收须在同候选、适用 feature/config 与环境下运行，并保留命令、提交、输出、失败样例和未覆盖范围。文档检查通过不证明运行、部署或 release readiness。

| 上游 requirement / acceptance | 本设计条款 | 验证目标 / oracle | 验证方法 / 精确来源 | 证据产物 | 未覆盖边界 / residual risk |
| --- | --- | --- | --- | --- | --- |
| [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [registry 隔离](#readiness-registry-isolation) | world registry 优先；无合法 registry 的已有世界不能用 genesis 覆盖；测试通过也不等于生产 custody。 | 审读并执行 [testing manual](../../../testing-manual.md#s4b：governance-registry-drill（l4-前置-l4）) 对应 registry 回归文件 `crates/oasis7_chain_runtime/src/host/governance_registry_tests.rs` 中 world_finality_registry_overrides_node_pos_config、activation 后 effective registry 及 existing-world genesis 拒绝断言。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 仅 registry 优先级与 activation 子范围；服务暴露面、完整 BFT/rotation 证明未覆盖。 |
| [治理授权专业要求](p2p-mainnet-security-governance-readiness.prd.md#readiness-governance-authorization)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [治理主体认证](#readiness-governance-authorization) | 正确 controller 字符串仍需合法 threshold proof；不同入口的伪造、重放、错 parent/policy/world、batch 自授权都在 World 变更前拒绝。 | planned：按 [testing manual](../../../testing-manual.md) S4 required 扩展 NodeRuntime 提交与 World 治理事务测试，逐种动作核对签名与状态前后；同候选 test_tier_full 验证复制输入和历史 replay，不把现有状态机测试冒充 proof 测试。 | 实施 PR 绑定 source/config、原 envelope、policy/nonce 前后摘要、拒绝结果及一次有效动作对照；不含私钥。 | 本次未运行，端到端 proof 尚未实现；公开服务具体暴露面及 production custody 另验。 |
| [成员应用专业要求](p2p-mainnet-security-governance-readiness.prd.md#readiness-membership-application)；Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 | [成员应用](#readiness-membership-application) | h/h+1 旧集、h+2 新集；各组件实际 digest 一致；安装中断／冲突／滞后节点停止越界签名；重复恢复不产生第二转换。 | planned：按 [testing manual](../../../testing-manual.md) 的 required 与 test_tier_full 层级，在现有 `crates/oasis7_runtime/src/governance_membership_tests.rs` 基础上增补真实组件 ack、持久安装各边界 crash/restart 和 old/new signer 正反例；同候选验证 BFT 集合转换，本机 required 与同窗口多节点 test_tier_full 分开。 | 原治理 CommitRef、激活高度、各实例 applied 确认、安装日志与边界前后消息验证结果；不以 config dump 替代组件证据。 | 目标 install API 未实现；静态 projection 与旧 registry drill 不代签，BFT 整体验收仍归 P2P。 |
| [私有读取授权专业要求](p2p-mainnet-security-governance-readiness.prd.md#readiness-full-sync-read-authorization)；Trace refs: O7-REQ-cb84969d-dd7f-4782-ab30-167ffc59a0c8 | [读取授权与撤销](#readiness-full-sync-read-authorization) | revoke/replace 后旧 grant 逐请求／chunk 失效；分区 lease 到期停读；issuer/provider 共同回滚不恢复权限；完成判定覆盖每个实例/lease。 | planned：按 [testing manual](../../../testing-manual.md) 扩展现有 `crates/oasis7_chain_runtime/src/host/node_lifecycle/formal_full_sync_tests.rs`、`formal_full_sync_grant_reconcile_tests.rs` 的局部基线；增加未授权管理请求、错误请求者、受限委托、多 provider、延迟续租、时钟回退、重启、chunk 竞态及新旧能力拒绝；撤销前 issuer 备份+新／回滚 provider 取 fresh lease 须拒绝；同 provider 新实例 ACK、旧/复制实例持未到期 lease 时保持 pending，cutoff 回滚或 VM 恢复不能提前 effective。required 验状态机，test_tier_full 验分区／实际隔离及未撤销授权正向读取。 | 不可回退当前锚、issuer 完整签发/撤销记录、逐实例/lease cutoff或expiry证据、可信时间界、chunk 时间线、pending→effective 和正常读取对照。 | 当前只有 v1 lease；目标新鲜度/撤销接口未实现，测试未运行；已放行字节和恶意保留的既有副本不可收回。 |
| [资产与密码学基线](p2p-mainnet-security-governance-readiness.prd.md#资产与密码学基线) | [授权与创世](#readiness-auth-genesis) | 无效签名/账户不匹配在 consensus submit 前拒绝；不能以 API 局部测试代签所有层。 | 执行 [testing manual](../../../testing-manual.md) 对应 API 回归文件 `crates/oasis7_chain_runtime/src/host/transfer_submit_api_tests.rs` 的 missing/invalid signature、account mismatch 拒绝断言；nonce replay 另补 shared-layer 证据；该文件只覆盖 API 子范围，客户端与 shared-layer 需分别补证。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 产品适用性：N/A；理由：本行承接专业资产授权合同，未建立独立玩家经济或发行承诺；范围：signed transfer 边界。native/wasm 与 shared-layer 完整联测仍需补实际证据。 |
| [创世、ceremony 与 QA](p2p-mainnet-security-governance-readiness.prd.md#创世、ceremony-与-qa) | [授权与创世](#readiness-auth-genesis) | rotation/revocation/failover 的实际证据与 ceremony 条件分别判定；无 QA pass 不升级。 | 按 [testing manual](../../../testing-manual.md#s4b：governance-registry-drill（l4-前置-l4）) 核对适用 registry drill 的 pass + block 证据；另逐项审读 PRD ceremony 条件，缺任一项保持阻断。 | 对应候选的命令、退出码、断言输出与环境；本次未运行此技术验收。 | 产品适用性：N/A；理由：本行定义专业证据与安全阻断条件，不宣布玩家开放；范围：ceremony/readiness。真实 mint、production custody 与公开发布未覆盖。 |
