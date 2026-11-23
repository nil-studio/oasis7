# P2P 权威配置、受控接管与预览世界初始化（分册）

- 主设计文档：[`design.md`](design.md)
- 本分册按 #6507 原样承接 `doc/p2p/design.md` 第 4 节中 `des-p2p-layer-authority` 至 `新预览世界初始化` 的正文，不改变既有权威边界、Trace 归属或验收义务。

### des-p2p-layer-authority

治理validator与permissionless服务、light/archive角色隔离；验证材料而非operator身份；prune重建+archive条件。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-target-service-gate

verified-serving→stale/catching-up→readonly→serving；proof冲突→isolated；非serving新intent receipt0、历史不改。

只读恢复提供 verified 历史与 truthful pending/unknown；服务闸门必须同时满足 identity/checkpoint/head/hash 材料、当前 append/finality、governing manifest compatibility 和单调 head。
不满足时新 intent 不产生 committed receipt；满足后，尚无固定原决定且已知无效果的 pending 按当前 permission/resources/version/conditions 重审，无 outage 期限/priority 继承。
已经固定的原决定按 [原 candidate 恢复](#des-p2p-original-candidate-recovery-v1) 先核对、续办，unknown 不推断为无效果；accepted 新 intent 至多一个原结果且无第二效果。
退回 blocked/isolated 不撤销或改写已 confirmed receipt；consumer 可读 semantic/blocker/next-step 与 linked attachment 由外部 owner 供给。

该外部交接的 pending/lineage 字段已由 [SR2 唯一字段](../world-runtime/module/module-lifecycle.md#sr2-pending-contract)定义；[持久准入与恢复](../world-runtime/module/module-lifecycle.md#sr2-pending-admission-recovery)区分 admission owner/stream 的原位置与 canonical decision，[原子 winner](../world-runtime/module/module-lifecycle.md#sr2-lineage-publication)约束已登记及迟到互斥成员。P2P 仍只提供原 profile 的 ordering、包含证明和持久条件，不按 admission 时间另排次序，也不把未验证本地 audit 变成最终拒绝。消费者按 [API 目标映射](../world-simulator/llm/external-agent-game-api.design.md#api-intent-results)读取具体状态和当前获权关系；这些是待实施的桥接，不能把当前临时受理升级为 durable pending 或宣称跨消费者已验收。

组合执行 lane 见 [Game World State Sync and Commit Closure](../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.prd.md)；evidence envelope 见 [topology/node truth](../testing/templates/state-sync-closure-evidence-packet-template.md#topology-and-node-truth)、[peer-head/gap sync](../testing/templates/state-sync-closure-evidence-packet-template.md#peer-heads-and-gap-sync) 与 [bundle/blob closure](../testing/templates/state-sync-closure-evidence-packet-template.md#state-sync-bundle-and-blob-closure)。DC-5 的 [target attachment/schema](../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-dc5-attachment)已有定义，仍需要真实 linked 材料和语义验证，逐项含状态转换、manifest/head 负例、每 intent receipt0/1、consumer blocker/next-step；[PL01–09](../testing/longrun/game-world-state-sync-commit-closure-2026-06-26.design.md#sr2-pending-lineage-combination)补齐本次 pending/lineage 组合方法，模板或方法本身不闭合实际证明。Ops同窗口事实、runtime根/receipt和消费者反馈由QA组合，缺任何适用字段不通过。

### des-p2p-replication-scope

writer/epoch从seq1及guard不污染；权威恢复错误与qualified storage-challenge fallback区分。

net 拥有 transport/peer/DHT；consensus 拥有 proposer/attestation/action-root；存储域由 cas/distfs 分担本地 CAS/challenge 与分布式 replication；node/chain-runtime拥有commit后执行/恢复接线；proto仅共享wire；wasm_abi是manifest/runtime ABI单一来源。公开服务只能供应hash/proof材料，不能投票或写canonical。

### des-p2p-authority-profiles

承接 [合法 profiles](prd.md#p2p-authority-profiles)、[单权威持久提交](prd.md#p2p-single-authority-durable-commit) 与 [H/H+1 交接](prd.md#p2p-authority-handoff)。同一世界/执行/receipt 协议按各高度合法 profile 验证，固定 world/chain/genesis 身份与可演进 manifest/profile/epoch 分开；受控单权威不宣称 distributed consensus，threshold prototype 不取得正式权威。

单权威追加先在最终存储原子检查 epoch、预期 parent/head、顺序；signer 在 fenced epoch 下核对完整 prepare 的本机与独立副本持久确认后生成提交决定/证明；最终证明和决定再按同一协议复制持久化。prepare 不发布世界效果；本机与独立故障域完整记录和正式提交决定/证明确认后才回 committed。单调 guard 与本机锁不能替代旧 writer fencing。未知结果保留原 identity 查询，独立副本不可用时暂停确认；人工恢复先隔离旧 signer/追加权，无法证明则停写。

#### des-p2p-local-authority-journal-prerequisite

本地持久日志仅承接 [单权威持久提交](prd.md#p2p-single-authority-durable-commit) 中的本机原子记录、请求去重及恢复查询前置。固定 trusted world/chain/genesis、signer 与 epoch；在本地独占进程锁内检查预期 parent/head 与原请求 payload digest，将签名 local decision、完整日志与派生的 head/request index 作为同一持久事务保存。同 identity 同 payload 查询原本地结果，异 payload 冲突；任何不确定写入沿原 identity 返回 unknown。

`oasis7_distfs::controlled_authority` 的 local decision proof 使用独立 schema/domain 与 `local_durable_prerequisite` 范围，offline verifier 只验证该范围的身份、签名和链接。它不构成已激活 profile 的正式 CommitRef，既有 runtime、manifest startup、WorldService 与旧 finality verifier 不消费它作为 committed。独占锁及固定 epoch 只防同一受信存储路径的本地竞争，不具备跨主机 fencing；没有独立故障域完整持久确认时不得发布正式效果。

恢复核对 trusted identity 与外部保留的最低 head anchor，重建日志/head/request index 并拒绝截断、签名或链接冲突；本地 snapshot 单独无法证明未被回滚，丢失外部 anchor 不自动降格为可信恢复。固定 epoch 不支持自行提升、signer 轮换或 H/H+1 激活。首次实现仅在支持文件和目录持久同步的 Unix 本地文件系统开放；Windows 或不支持目录同步的平台明确拒绝该新入口，既有 DistFS 路径不改变。回归入口为 `cargo test -p oasis7_distfs controlled_authority`，覆盖竞争写者、stale epoch/parent、相同请求与 payload 冲突、持久阶段故障与 proof 篡改；其运行结果由 [Issue #4363](https://github.com/eng-cc/oasis7/issues/4363) 对应候选维护。本切片不关闭跨故障域灾备、正式提交及消费者接线义务。

#### des-p2p-replicated-durability-prerequisite

`controlled_authority::replicated_protocol` 承接同一单权威持久提交义务中的两端记录协议，使用独立 `replicated_durability_prerequisite` 签名范围。固定 trust 绑定 world/chain/genesis、writer、epoch 与不同身份/密钥的 primary、replica；它不转换 local proof，也不接入正式 CommitRef、runtime、profile 激活或自动接管。

封闭记录保存输入、结果、snapshot、连续 journal、nonce/去重状态、外部效果 outbox、执行 manifest、WASM 与规则的完整字节对象，以及明确声明的引用闭包。接收端核对角色、长度、hash、重复和缺失对象；不访问外部 URL，不执行工件，也不证明调用方执行正确、玩家授权或任意 opaque 对象内部未声明引用。调用方仍负责可信执行记录与实际重放材料的语义完整性。

协议顺序为两端 prepare → primary 原子决定 → replica 保存同一决定 → 两端保存完整证据。primary 的同一持久快照原子维护 fixed epoch、预期 parent/head、决定与原请求索引；一旦决定持久化不可取消。prepare 不产生世界效果。同 identity 同 payload 查询/续办原决定，异 payload 冲突；任何可能已改变持久状态的错误保留 identity 为 Unknown，停止该实例追加并要求明确恢复。

prepare、primary decision 与 replica decision 分别使用签名域。接收端只有在完整记录和决定同步落盘后才签 durable receipt；离线 verifier 要求不同配置端点的两份同决定回执，单端签名无法构造合格证据。协调器将完整证据同步保存到两端后才返回 DurabilityQualified。该结果证明受信接收端声明满足所配置同步合同；签名和两个本地目录不能独立证明实际硬件持久性或故障域隔离，也不是产品 committed 状态。

恢复先验证 fixed trust、完整记录、签名/链接与外部最低 head anchor；anchor 区分不可撤回的决定与已有双端证据，不从 prepare 推导效果。primary decision/replica prepare 只能续办原决定；同位置分叉或材料冲突隔离停写。任一端丢失后，幸存端的完整合格证据可验证原记录；恢复写服务仍须重建缺失副本并重新满足双端条件。固定 epoch 不授权更换密钥/端点或人工接管，旧 signer 与实际最终存储访问控制仍需部署隔离合同。

本切片仅支持可同步文件与目录的 Unix 受控文件路径，防同路径进程竞争；不防同权限恶意进程替换目录/锁 inode。回归入口为 `cargo test -p oasis7_distfs --lib replicated_protocol`，覆盖持久阶段崩溃/丢回复、同请求重试、parent 竞争、prepare-only、单端丢失、伪造回执与错误 anchor。候选及结果由 [Issue #4363](https://github.com/eng-cc/oasis7/issues/4363) 维护；实际独立故障域、存储防绕过、正式消费者接线与 epoch 恢复尚未完成。

交接停写排空后，新集合同步 H−1；旧 authority 在 H 正式记录新集合/profile/epoch/H+1 边界及结果根。H 提交前可取消，提交后接续权不可撤回，入口/存储/signer/proof verifier 均拒绝 H+1 旧 profile；新集合启动失败保持停写。历史仍按各阶段合法 profile 验证。演练覆盖 H 已提交而 H+1 未提交时宕机/取消、重启拒绝旧 writer，以及无资产重发/断链/失效 pending 自动生效。该设计尚需协议、消费者与故障域实证，不是当前 readiness。

#### des-p2p-formal-bft-read-handoff-v1

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39、O7-REQ-792f38f5-82d4-47bc-8b1f-e4d4568e1b46。首个正式 BFT 消费切片承接只读历史从受控 H 接续到 BFT H+1；本节是实现合同，不表示该路径已经交付。它不授予新的 Submit、Lookup、Changes、私有完整同步或 validator 签写权限，也不取消这些消费者及真实 custody 的后续义务。

交接验证从既有外部可信受控起点和原 qualified 历史得到 H，不从任意 peer 提供的 H proof 自建信任根。H 的 `ProfileHandoffV1` 必须指定精确的新 profile/version、目标配置 digest 及 H+1 位置。目标配置使用独立版本和域，绑定同一 world/chain/genesis、BFT wire 版本、新 authority epoch、完整初始 validator set 与 proposer/epoch 映射政策；不以本机 selector、operator 身份或安装 ACK 代替该承诺。配置 digest 不包含 H 自身的 record ID 或 H 的输出根，避免与承载它的 H/input/持久结果形成循环；验证后的激活能力另从 H 的已验证记录绑定实际 record ID、位置、原始 Snapshot root 和 governing manifest。目标验证本身不宣称已经恢复或重执行 World。

H+1 的初始 schedule 由该激活能力导出，parent 必须是 H。H 的 governing runtime digest 来自已提交记录，并与恢复 World 的 cognition runtime binding 核对；它不是 BFT 使用的 World manifest hash。BFT parent manifest 必须从 H 的认证 Snapshot 恢复后计算，与 parent semantic root 一样由新 QC 的 execution preimage 绑定；客户端仅验 QC 不等于独立恢复 H。受控 H 的 `after_state_root` 认证原 Snapshot 字节；先核对该原始摘要和 Journal 闭包，再从恢复后的 World 计算 BFT 所需的语义 parent state root，不能把两个不同含义的 root 互换。执行 tick 与 canonical position 分开：tick 从 H 所认证的 Snapshot/Journal 闭包恢复并核对，不把 canonical position 直接当成 simulation tick，也不按 BFT genesis 的 height 1 重新创建世界。现有 engineering V2 的 `parent_tick+1==height` 及 epoch→height 映射保持原义；正式接续必须另有明确的 position/tick 与集合生效映射，缺少适用映射时拒绝，不能仅放松旧校验或静默重置 epoch。普通 engineering V2 genesis 配置不能构造这项激活能力。新 ledger 格式固定原受控信任起点、H 及目标配置；重启重验同一激活关系和完整后继 QC/执行前缀，不把结构性最高记录当作已验证 head。

本节首个版本只支持恢复并核对后确实满足 `World.time == H.position` 的切点；这是显式支持边界，不是从 position 合成 tick。异值切点返回不适用映射的 blocker，保留 H 后旧权威停写，不回退或另建 genesis。初始 schedule 为 H+1、parent 为 H、parent tick 为恢复值、parent semantic root 为恢复 World 的重算值，proposer priority 采用初始集合的规范初值。目标配置与激活关系同时绑定这项映射规则；其他 position/tick 关系需要新的显式映射版本与回归，不能由这个受限读取切片宣称已经支持。

目标配置 digest 为 BLAKE3(`u64be(domain.len)` || `oasis7.formal.bft.profile_target.v1` || canonical target CBOR)。激活 digest 则为含固定 `domain="oasis7.formal.bft.handoff_activation.v1"`、schema 1、target digest、H head/world、H 原始 Snapshot root、H governing runtime digest 和 H intrinsic finalizing-authority ID 的 canonical typed payload 摘要；不包含 reader 路径相关的 authority context 或证明 envelope。相同 H/target 不因合法的历史读取或恢复验证路径不同而得到不同激活身份。

Runtime 只通过显式交接入口接续：由已验证 H/目标配置的能力与精确 schedule 共同校验 H 的保留历史 parent、实际 tick、scope、根和 manifest，再安装 BFT 执行阶段；H 的 retained-history reader 保留用于冷历史读取。实际 H Snapshot/语义根/manifest 的相等检查限于接续与执行开始前；tick 推进后，治理授权继续使用捕获的 H parent policy，并核对交接标记、retained parent/descriptor 与 schedule 的稳定绑定，不能要求已变更的 World 仍等于 H，也不能使用同批次新策略自授权。普通 engineering BFT 的 active-qualified-parent 拒绝规则不放宽，也不把旧受控 H 伪装成新受控执行 parent。

独立 ledger 的 CAS pin 以完整 H/目标身份隔离分片，以显式节点数/累计字节预算记录 H 恢复与 H+1 重执行实际读取的索引节点，将校验后的节点与 Snapshot/Journal 一同保存并耐久 pin，再发布不可变提交记录；不能靠重启补 pin 掩盖已提交记录与 GC 间的丢失窗口。重开先在既有 ledger 锁下装载有界、内容寻址校验的依赖缓存，再以同一 H 重新恢复和执行；缓存不是信任根。

该入口仍要求专用 CAS；[本地 retention 协调](distfs/distfs-production-hardening.design.md#des-dh-local-retention-coordination)使遵守同一协议的 pin 发布与 GC 串行，成功发布的 pin 不被过期快照删除。启用并发维护前须升级所有 CAS 访问者；旧进程、直接文件系统删除和不支持可靠 OS 锁的文件系统仍须排除。尚未 pin 的候选可被先行回收，安装必须报错且不能发布提交记录；调用者保留同一 QC/操作身份重试。该局部协调不替代完整 GC 闭包与保留资格。未被该固定执行/投影路径访问的冷历史仍由外部 retained reader 保留，这不证明完整历史的独立重建或全量 GC 条件。

H+1 仍使用现有 V2 的签名域、严格权重阈值、独立重执行及 QC 验证；profile 交接不生成一个内部 h+2 validator transition，也不混用旧受控 P/R 回执和新 validator 的票。接续只读 head 前，driver 从 H 的已认证闭包恢复隔离 World，重执行 H+1 的原输入并比较全部已有执行 roots，再持久安装精确 QC 与结果。仅有 QC、Snapshot hash、服务端声明或 transport 成功均不足以发布该 head。

engineering V2 的现有 execution hash 不承诺正式读取的投影树，不能沿用它认证投影。正式 profile 使用独立版本及域的 execution commitment preimage，保留原有 scope、height、parent、epoch、集合、policy、manifest、action/event/state 与 transition 绑定，并增加实际执行结果生成的 visibility-aware projection root 及 result root；其 hash 写入原 V2 block 的 `execution_block_hash`，由原 block ID/QC 承诺。preimage 不包含自身 digest、QC、当前 block ID/CommitRef，或由这些值派生的 root；parent CommitRef 可以包含，输出树须在当前提交身份封装前按既有非循环投影合同计算。只读证明携带该有界 canonical preimage，客户端重算 hash 并核对所有重复字段与 QC block，再验证请求 audience 的投影 inclusion。既有 engineering execution hash 不隐式升级，未经实际 World 重执行生成的投影承诺不进入正式 publication；读取证明无需披露完整私有 Snapshot。

该 preimage 的固定字段集合为 `domain="oasis7.formal.bft.execution.v1"`、`schema_version=1`、`profile="bft"`、`profile_version=1`、`activation_digest`、`scope`、`height`、`parent_commit_ref`、`parent_tick`、`parent_semantic_state_root`、`authority_epoch`、`active_validator_set_hash`、`schedule_policy_hash`、`manifest_hash`（parent）、`post_manifest_hash`、`action_root`、`event_root`、`execution_state_root`、`state_root`、`projection_root`、`result_root`、`transition`、`transition_ref`。使用仓库 `to_canonical_cbor` 的 typed value/map 排序及 self-described CBOR 编码，再取 BLAKE3 摘要；源代码字段排列不是另一种 wire 顺序。解码拒绝未知字段、非规范摘要和超限，并要求重新编码与原字节完全一致。`activation_digest` 固定已验证 H/目标配置关系；H+1 的 parent tick 必须等于该受限切点的位置，parent semantic root 与 parent World manifest hash 是新 QC 所承诺的执行声明，driver 必须先从 H 的认证闭包实际重算核对后才能发布，不能将客户端验 QC 等同于客户端独立重执行。结果索引沿用已有业务结果树合同，不另造只供 BFT 使用的结果索引含义。field 缺失、同义别名或旧域不得默认补齐后通过验证。

正式读取消费者采用显式版本化的证明和协商边界，旧 V1 wire 不增加可被旧客户端静默忽略的新权威语义。消费者先验证受控前缀和 H 交接，再逐高度验证新 profile 的 QC、集合链和投影承诺；response 的 profile、CommitRef、state root、投影及请求锚必须来自同一已验证 cut。原 public/private audience、持钥证明、撤销和释放前闸门继续约束原数据范围；新证明不把公开读升级为私有完整读取。未协商新版本、工程 scope 混入、错 target/parent/root/manifest、缺闭包或旧 profile 在 H+1 继续写入时均拒绝，不降级到 prototype 或 engineering verifier。

首个读取入口为独立签名的 `/v2/world/read`，仅接续到调用者事先独立固定的精确 H+1 `(height, block_id)`，不把 minimum floor 当成精确目标，也不接受 provider 自选更高 head。请求另绑定外部已验证 H 锚、目标配置/激活 digest、world、scope 和观察 nonce；响应重复核对同一目标，携带有界 canonical QC、正式 execution preimage 和投影 inclusion。该接口不宣称 latest/current/fresh；自动向后继高度追随需要另外满足适用的独立 checkpoint 有效期/时间授权合同。历史投影 World 不能充当当前私有授权 World：私有 audience 另要求可用的当前授权状态，缺失时阻断私有读取，公开投影仍按原公开范围验证。

主机提供两个显式入口：`bft-read install` 从有界 canonical QC 文件安装已被独立配置固定的精确 H+1，先验证 H/闭包/目标及证书，再由实际 World driver 重执行并持久保存；输入 QC 不能自行选择信任起点或目标 block ID。`bft-read serve` 只重开已有独立 ledger，重验该固定 cut 后提供 loopback 读取；不创建缺失信任或重新初始化账本。install 不生成 validator 票、不调用 Submit，也不代表网络写入或正式 validator 生命周期已接线。

连续正式 BFT 前缀扩展保留上述 H+1 合同，另以不可反序列化构造的验证状态绑定已认证 H、目标配置和激活 digest。版本 1 的完整 canonical 前缀同时限制为最多 64 个高度和 16 MiB 编码字节。首高度的协议 schedule 由 H 能力与首条正式 preimage 派生，发布驱动仍须从 H 实际重放并逐项核对该 schedule；每个后继高度复用现有 QC token 验证器，且只能取前一个 token 的 next-height schedule。每条正式 execution preimage 必须与对应 block、激活 digest、精确 parent CommitRef/tick、parent semantic root 和 manifest 相符；缺高、重排、冲突或混入 engineering 执行均拒绝。最后一个普通 QC token 不能单独代替从 H 连续验证的能力。该能力认证协议前缀，不认证 World 已执行、当前可服务、持有签名权限或读取新鲜度。

受控 `restore_snapshot_with_attached_formal_history` 继续只恢复精确 H 的原 Snapshot。连续 BFT 的隔离候选从驱动实际重放所得的父 World 派生；在保留原 H reader 的同时，先核对父 Snapshot 原始摘要、semantic root、manifest 与已验证前缀的 tip/QC，拒绝未提交 action/effect 队列或已有执行阶段，并核对隔离前后的 Snapshot/Journal 字节相同。该路径不接受外部 Snapshot/Journal 替换参数，也不给 H 的旧证明增加对后继状态的认证范围。

驱动在恢复时从相同 H 的认证闭包逐高度重执行，核对每条 commitment、实际 Snapshot/Journal、投影和结果树，再公开相应 cut。完整重放入口先验证整个有界协议前缀及外部精确 `(height, block_id)`，再从已恢复的 H 顺序执行全部高度；每步结果树使用刚重算的前一步树及其 root，不合成受控 CanonicalRecord 充当 BFT parent。合法 QC 但伪造执行输出的前缀也必须拒绝。

该重放阶段只返回隔离执行结果，不追加旧 H+1 ledger，也不能生成带持久化含义的 publication cut。重放在独立的有界依赖 recorder 上用同一已验证 retained-parent 能力重建 H，保留原 release policy，并核对 H Snapshot/Journal 不变；结果树和 World 冷读都使用这项隔离 reader，缺页直达原 source，而不经旧 H+1 recorder，避免改变其未来提交的依赖集合和预算。每步匹配实际 commitment 后缓存新结果索引节点，执行结果持有累计依赖的 reader 生命周期。每个高度重新捕获刚重放出的父 World controller policy；H 的 reader 继续只承担受控冷历史读取，不能将 H 的 controller policy 沿用为所有后继高度的授权。

ledger 对每个高度追加不可变记录并保留累计 CAS 依赖，拒绝截短、覆盖、跳高和用最高结构记录直接生成可信 head；旧单高度 ledger 不静默解释为新格式。完整前缀须同时有累计高度与编码字节硬上限，达到上限返回明确容量不足，不能丢弃旧前缀后继续。新消费者必须协商显式新版本、携带完整有界前缀证据并到达外部精确固定的目标；旧 `/v2/world/read` 仍只支持 H+1。这些接续能力不替代正式 signer/provider 安装、网络共识生产、checkpoint 时间授权或部署 readiness。

多高度 ledger 从已认证 H 与固定目标配置直接初始化，不要求旧 H+1 账本先存在；否则正式 producer 无法在首个 H+1 QC 产生前建立其执行起点。新目录、schema 与 required marker 独立解析，同一记录目录不并存旧 H+1 与新 prefix writer mode，双方启动检测均须拒绝混用或降级。不同目录中的只读 QC 导入副本不产生共识签名，分别持有稳定、持久的本地 ledger 身份；其 CAS scope/shard 绑定该身份与 origin digest，不能仅以相同 H/target 共用 shard 而覆盖另一副本的保留集合。正式 producer 的 Node 身份、受保护 signer fencing 与实际安装仍是另行接线的前置条件，导入 receipt 不授予这些权限。

所有当前 BFT 格式在检查、初始化各自目录前共同持有记录根目录的稳定 `bft-format.lock`，再持有自身 ledger lock，避免不同格式各自加锁后并发通过空目录检查。新 prefix 还在旧 `formal-bft-read-required-v1` 路径发布明确不受旧解码器支持的降级 fence；这不是旧 H+1 账本，旧入口不得据此重建或把新格式解释为旧格式。启用新格式前须停止并升级不遵守共同锁/新 marker 合同的旧进程；该 fence 不证明与已经运行的任意旧二进制可无停机混用。锁载体丢失、被替换或出现额外链接时阻断，不能以新建锁文件恢复一份既有 origin。共同锁载体在首次取得 OS 锁时写入并 fsync 32 字节随机 token，当前各格式一致消费，旧空载体仅在持有 OS 锁时升级。prefix 的本机 origin 同时固定两把锁的 device/inode 与 token 身份，重开先核对原身份，发布前再核对持有 fd 与路径；只把新 fd 与新路径相互比较，或只记录可能复用的 inode，均不足以证明跨重开的连续性。该本机物理身份不属于可移植 QC 前缀；新副本必须独立初始化。

初始化先持久发布含随机本机 ledger ID、固定 H 材料 digest 与首高度的 `init-intent.cbor`，再通过同 inode 硬链接发布 origin，持久发布两个格式 marker 后才清理原 init-intent。完整初始化 staging 也固定同一原身份；重试不能另生成 ID 或换 H。已完成 origin 即使没有高度记录也要求两个 marker 完整；缺 origin 且仅余 marker 不能重新初始化。初始化未完成时不能已有高度记录；无法证明的状态保留并阻断。

每个实际高度在发布 CAS/pins 前先耐久保存绑定原 QC、commitment、parent-record digest 和全部引用的 `reservation`。材料及精确 per-height pin shard 全部核验后，将原 reservation 原子改名为 intent；再通过不覆盖的硬链接成为 record，同步目录后删除等价 intent，再同步目录。reservation 尚未发布 pin 时可重新执行同一输入以完成材料；原 shard 已存在时必须核验原 pins 与 CAS，不能重写掩盖损坏。intent 或 record 已存在后的缺失、损坏或不符 pin 均阻断。pin shard 找不到原 record/intent/reservation 身份时也阻断，不能从引用集合猜测或换用另一 QC。

恢复须在稳定 OS ledger 锁下识别原状态：仅 reservation 或 intent 为单链接 regular 文件；record 与 intent 同时存在时须为同 dev/ino、逐字节相同且恰有这两个链接；已完成 record 为单链接文件。已完整写入并 fsync、尚未 rename 的初始化或 reservation staging 固定同一原操作身份，重试先核对规范编码和原输入，不能盲删。只有实际重执行、精确 pin/CAS 核验及原字节相等后才提升或清理等价 staging；部分写入、额外链接、符号链接、冲突、未知文件或无法证明的中间态保持原件并阻断。不得按 PID 或超时猜测锁所有者，也不能直接重写部分记录。较早 cut 的隔离 reader 仅装载截至该高度的实际依赖，不能因账本已存更高记录而开放仅由未来高度保留的节点；整本账本仍须核验容量与既有材料完整性。

协议的 64 高度/16 MiB 上限不能代替执行材料容量。安装与重开还须核对 H 到目标的累计记录、CAS 依赖节点和字节硬上限，在 intent 发布前预检，并将已安装前缀计入同一容量核算；重试或增加后继高度不重置累计容量。固定 cut 的局部重开与 GC 用例只证明实际保留的重放闭包，不替代未访问冷历史的 archive 保留资格或完整同步预算。

现有 `BftBlockV2` 的 block ID 对包含 `actions_cbor` 的整个 block 编码取 hash，签名证明不能在不改变被认证身份的情况下删去原动作。原 WorldService 动作可能含私有输入，public projection 的 inclusion 并不隐藏 QC 中这些字节。当前 V2 单高度和 V3 连续前缀的 light-read publication 因此只允许每个被披露高度的 `actions_cbor` 精确为规范空 vector `[0x80]`；其他编码或非空动作在生成/编码任何响应证明前返回 not-ready，单 Agent 的私有投影授权也不能放行其他主体原动作。V3 verifier 同样拒绝这类证明，但客户端拒绝不能代替发布端保密。该限制不拒绝 private ledger 的原输入重执行或已授权完整同步；普通带动作链的公开读取需要另外实现由 header/root 认证且不携原输入的新 block/proof 版本，不能用服务端的“可公开”声明代替。这是原 V2 publication 的安全收紧和当前读取实现边界，不代表完整正式 BFT 读取已完成。

连续前缀的读取采用独立签名域 `/v3/world/read`、schema/contract/wire version 3；旧 `/v2/world/read` 继续只接受其原 H+1 合同。V3 请求独立固定 H read anchor、目标配置/activation digest、world、audience、observation nonce 和精确目标 `(height, block_id)`，高度限于 H 后的 64 个位置内。响应携带完整 canonical `FormalBftCertifiedPrefixV1` 的小写 hex、同一 tip 的 head 字段及投影 inclusion；不另带可与前缀错配的独立 tip QC/preimage。prefix CBOR 最多 16 MiB，JSON hex 最多 32 MiB，整个响应的实际传输编码限制为 40 MiB。结构校验按有界计数写入而非先分配任意大的重复编码；接收端在解码前限制响应字节。

V3 客户端必须先从本地已认证 H 验证整条前缀及所有集合/schedule 变化，再匹配本地精确目标与响应的 tip/root/tick，最后核对签名请求 audience 的 inclusion。服务端仅从本机 ledger 已持久回读和实际逐高度重放的 cut 建立 publication，重算 World 的语义执行根、manifest 和投影，并核对 cut 的实际结果树 root；不能以单个 tip QC 或服务声明代替该前缀。`bft-prefix serve` 只重开原配置中的既有账本，不初始化缺失状态。固定历史 cut 不能用作当前私有授权 World；缺少独立的当前授权源时私有请求保持阻断；提供当前源时，授权 guard 覆盖响应构造、签名及有绝对截止时间的 socket 写入，不能在返回编码字节后、实际释放前提前放开。公开请求仍受原公开范围与释放前闸门约束。新版本不产生 latest/current/fresh、新 validator 权限或 provider 安装资格。

主机通过独立 `bft-prefix install --config <prefix.json> --prefix <complete-prefix.cbor>` 和 `bft-prefix verify --config <prefix.json>` 接入上述实际安装与恢复。配置 schema 1 固定 `trust_config_path`、已由 H 承诺的 `target`、独立精确的 `exact_target.height/block_id`、`handoff_material_path`、`cas_directory` 与 `ledger_directory`；不接受 provider 选择目标。输入文件为 owned 0600，CAS 与 ledger 为既有、分离且 owned 0700 的真实目录。verify 不初始化缺失状态，可以在完整重放和 pin 核验后完成已保存的原 intent；它不接受替代 QC。实现为整条前缀设置 512 MiB 材料容量，包含编码/展开的 H、记录元数据和内容寻址 CAS 闭包；节点集合另受既有 4096 节点/64 MiB 索引预算约束。记录字节在解码前受累计读取上限约束，解码后的数量/引用、实际执行结果和材料容量在 intent 发布前核对；达到容量返回明确 blocker，不丢弃旧前缀继续。

两个命令的 `oasis7.formal_bft_prefix_install_receipt.v1` 是同一原安装 cut 的本机持久回读诊断，绑定实际高度、block ID、activation digest 与本机 durable record digest。外部消费者仍需独立验证完整 QC/正式 preimage 前缀；stdout JSON 不是新的外部权威证明。该入口不生成票、不启动服务端新 read wire、不宣称 latest/current，也不代签正式 signer/provider/Node 安装或全量状态可用性。

保留长期离线 checkpoint 的有效期、退出 key/stake 约束及独立 minimum 要求；完整重放不自动免除弱主观信任期限。缺少适用且独立认证的 checkpoint、时间或受保护 minimum 时，自动接续保持阻断，历史固定 cut 的离线验签与当前可服务/可投票资格分别报告。本机测试只证明实际使用的 provider、时间和存储接缝，不代签硬件、磁盘掉电或故障域验收。

回归必须包含真实 qualified 受控 H、实际 World driver 执行的 H+1，以及正式读取 producer 到客户端 verifier 的正向结果；同一场景拒绝 H+1 受控追加、错误目标配置/父引用/根/manifest、未认证闭包和冒充正式历史的 engineering V2。H 已提交而 H+1 未提交的停启保留同一交接和旧写入拒绝；H+1 安装后重开保留精确 cut，后继合法只读请求可继续。未执行的消费者、签写和真实环境义务保持未完成，不由该只读切片关闭 #4975。

#### des-p2p-controlled-history-reexecution-prerequisite

execution bridge 的历史 capture 为显式 opt-in（`--capture-schedule-recipe-history`，默认关闭），只支持每高度恰好一个带既有已验证 ingress origin 的 `ScheduleRecipe`。它在真实执行前保存原始 Snapshot、Journal、上下文及实际 security policy，将该引用绑定进首次 product-validation intent 原子发布；执行后封闭包保存九角色真实字节、typed module registry 所声明的全部 WASM 和前缀关联。启用时材料持久或校验失败阻止记录发布并回滚内存执行；旧记录的 None 不反填，普通/BFT 默认路径不改变。

这里的 outcome index 仅是所支持连续记录前缀派生的 execution request outcome 累积索引，绑定既有 player/key/nonce/session tuple、实际提交 CBOR hash、parent/input/result。它不是 GameplayNonceLedger，不证明 ingress reservation 已成为正式提交，也不覆盖其他操作或所有 nonce domains。origin 是现行 ingress 签名、nonce 与玩家身份校验所得的受信 producer 请求身份；包中 tuple 不替代原玩家签名证明、Agent 控制 grant 或 Hosted issuer 授权，离线验证不授予玩家控制权。

只读入口 `oasis7_chain_runtime verify-controlled-history --trusted-config <json> --evidence <json>` 要求操作员独立固定 world/chain/genesis、writer/不同端点 keys、epoch、最低 qualified head、初始执行 height/root/block 与 security policy。完整连续历史必须从声明的初始锚起，不能跳过缺 capture 的旧记录、prepare-only 或单回执。初始锚可选显式外部 published anchor：操作员在输入前保存原 V3 record、Snapshot CBOR、Journal CBOR 的路径和独立 BLAKE3 hash；trust JSON 仍限 64KiB，record 限 64KiB、Snapshot/Journal 各限 64MiB并保留原 typed budget/no-follow。其 world/height/block/root/CAS refs、journal 长度和原 driver diagnostic context 必须相符；初始 pre-state 可按已验证的 published/default 完整资源注解 pair 演进；若 World 保留原 bootstrap 缓存 pair，须在输入前独立保存原 typed Snapshot CBOR 并固定 BLAKE3 hash，通过可选 `published_initial_anchor.cached_resource_snapshot` 的 `snapshot_path`/`snapshot_hash` 提供；所有外部锚文件路径必须为绝对路径。该文件沿用 Snapshot 的 64MiB、typed budget 和 no-follow 限制，只认证其完整 manifest/delta pair；world 必须对应 published record，缓存时间/manifest height 不得晚于锚，delta 仍按原完整 manifest/context 校验。全部其余当前 state 和 Journal 原字节连续，真实 root 不替换。原缓存只属 diagnostic annotation，不授予 genesis/profile/Agent authority；从待验证 capture 后取出的 pair 不能作为预先固定的外部信任。原 exact raw pre-root 路径保持兼容。入口验证双端证据、请求和实际 payload、所有 typed 引用闭包与前缀，再以原始 Snapshot/Journal、原模块字节和相同 committed context 调用既有 Runtime 的真实 WASM step，比较完整结果 Snapshot/Journal、state/block root 与 effect 材料。内存恢复不配置持久目录或 dispatcher；WASM 使用既有空 Linker，无新 LLM 调用、网络 host function 或替代模块。

连续性保留完整 Snapshot/Journal 字节约束，仅两个已验证资源注解允许既有缓存、重开或按实际 manifest hash/journal 长度生成的默认完整 pair 演进；delta 必须绑定相同 manifest，不能混合 pair 或任意去字段归一化 root。真实 pre/post roots 始终绑定原始材料。当前 driver 的 diagnostic resource context 使用 chain_id=world_id、genesis_ref=None，这些注解不证明正式 genesis；外部 fixed trust 不补写历史身份。

文件输入有界、拒绝观察到的祖先/末端 symlink，支持范围为 Unix 操作员受控目录；不承诺防同权限恶意进程替换祖先。合格输出仅为本地持久协议与重执行前置验证，不转换正式 CommitRef、不激活 planned profile、不改变 BFT、开放全路径 nonce 共持久或创建新世界。Rules 角色保存实际前后 World Manifest 配置，原生配置绑定在 Snapshot，security policy 与操作员配置核对；原 Runtime 没有可历史加载的 native Rust 规则二进制工件，本入口仅证明同候选 Runtime 语义下重执行，不宣称任意旧 native 版本的可移植 replay。真实候选须显式打开 capture 生成新材料；既有未 capture 世界不能通过补造 sidecar 获得此证明。回归入口为 `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime controlled_history`；当次命令、候选与结果另由工程 evidence 维护。
#### des-p2p-initial-activation-verifier-prerequisite

`controlled_authority::activation` provides an offline initial-activation prerequisite. It does not change manifest v2 planned status, startup readiness, legacy finality, or CommitRef. The caller independently authenticates an issuer key and fixes world/chain/genesis, initial epoch, writer/P/R, initial state root, execution manifest root, activation height, and the external genesis head. Certificate declarations, self-signatures, and configuration hashes cannot establish this authority. Writer/P/R keys must be pairwise distinct.

A dedicated-domain strict schema v1 signature binds `controlled_single_authority` profile/version 1, position 1, genesis parent, height, before/after state roots, and all eight non-Input artifact roots. Input is the original canonical CBOR signed envelope, whose BLAKE3 must equal the record payload digest and Input root. The signed body excludes Input to avoid circular signature/hash binding. The envelope is bounded to 16 KiB; existing closed-record budgets remain unchanged. Verification requires the complete nine-role record, writer proposal, and both P/R DecisionDurable receipts. Prepare-only evidence, a missing receipt, a wrong external anchor, and later positions fail closed.

Only successful verification constructs opaque `VerifiedInitialActivation`. It is not yet connected to execution-semantic verification, formal CommitRef, durable epoch publication, handoff, or runtime activation. Real FileEndpoint/coordinator fixtures exercise the local synchronous filesystem contract; they do not prove retained-world issuer authorization, independent deployment fault domains, or a formal opening. Upgrades require an explicit old-profile transition action and evidence chain; this initial-only verifier rejects them. Regression: `cargo test -p oasis7_distfs --lib activation`.

#### des-p2p-certificate-bound-bootstrap-anchor-prerequisite

The read-only execution bridge `controlled_bootstrap_anchor` consumes an opaque `VerifiedInitialActivation` and its exact original ClosedRecord. It checks Input digest, all eight signed roots and state roots before decoding. Version 1 accepts a no-state-change bootstrap marker only: before/after roots equal the complete original runtime Snapshot digest. It does not prove or apply an execution transition. No caller can deserialize or construct the opaque validated bootstrap output directly.

A strict `ActivationArtifactManifestV1` at ExecutionManifest binds world/chain/genesis, the existing schema-v3 ExecutionBridgeRecord checkpoint height, the separate simulation tick, the predecessor execution-block hash, and seven artifact roots. It excludes its own digest and Input to avoid circular dependencies. Height comes from the certificate-bound ExecutionBridgeRecord, never from `WorldState.time`. The existing ExecutionHashPayload algorithm recomputes block/state/journal consistency using the manifest-bound predecessor; that predecessor is an issuer assertion, not an independently verified history chain.

Snapshot and Journal use existing Observer decoding budgets and World::from_snapshot. This narrower contract requires a complete journal with a nonempty canonical prefix commitment, exact restored bytes, and the concrete declared module bytes with no extra or missing artifacts. WASM uses the existing capture registry/byte-reference closure; Rules binds the actual Manifest, PolicySet and externally fixed ReleaseSecurityPolicy. EffectOutbox is the existing pending/inflight effect tuple. NonceIndex v1 is specifically the existing capability authorization nonce-record map; it does not claim universal gameplay request-index recovery. Result is the actual typed execution checkpoint record. Exact reconstructed objects/references reject aliases, opaque placeholder roles, missing/extra objects and undeclared dependencies.

Runtime Snapshot has no separate canonical world/chain/genesis authority field. Authority therefore comes from the independently authorized activation certificate and its pre-pinned manifest; ChainResourceManifest identity annotations are consistency checks, including regenerated manifest/delta equality with the restored state. Absent genesis annotations and existing diagnostic history using genesis_ref=None fail closed and are never rewritten. This is a certificate-bound restored checkpoint prerequisite, not independent canonical-genesis lineage, runtime activation, readiness, CommitRef, a new journal or automatic takeover. Original historical Snapshot/Journal remain unchanged. Regression: `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime controlled_bootstrap_anchor`.

#### des-p2p-offline-bootstrap-cli

`oasis7_chain_runtime verify-controlled-bootstrap --trusted-config <external-json> --evidence <original-proof-json>` is an offline consumer of the initial-certificate and typed bootstrap verifiers. Dispatch occurs before startup-option parsing, Node construction and services. It has no signer, key/world creation, provider, directory persistence, readiness transition or manifest-v2 planned-profile relaxation. It reads the original Input envelope from the supplied single DurabilityEvidence record, then validates the independently fixed policy, external minimum head and exact original typed closure. Files are never repaired or rewritten.

Trusted JSON schema version 1 contains `issuer_policy` (the existing fixed trust, issuer_public_key, initial_state_root, execution_manifest_root, activation_height), `minimum_head`, and `release_security_policy` with all four existing policy booleans explicitly supplied. Unknown fields at every configuration layer and unsupported versions fail closed. This wire declaration explicitly constructs InitialActivationPolicy; it does not make that opaque trust boundary deserializable or authenticate its own issuer/world authority. The operator must authenticate the policy independently before invoking the command. No real issuer/world identity is generated by this entry point.

Config is limited to 64 KiB and original proof JSON to 64 MiB; the existing closed-record, signed-envelope, Snapshot and Observer typed budgets still apply. Reused offline file reading rejects observed ancestor/final symlinks and nonregular/oversized files on Unix operator-controlled paths; it does not guarantee protection against hostile same-permission ancestor replacement. Trailing JSON data, changed signed material, wrong roots/profile/minimum, absent original genesis annotations and unsupported artifact semantics are rejected. Output explicitly states the certificate-bound no-state-change checkpoint prerequisite and caller-authenticated trust basis; it is not CommitRef, live activation/readiness, independent fault domains or independently established genesis lineage.

Ordinary process regression seals an isolated engineering fixture before invocation, exercises early dispatch in a child process for success and failures, and verifies unchanged input bytes and directory contents. The same regression also invokes the actual built chain binary when `OASIS7_BOOTSTRAP_CLI_TEST_BINARY` is explicitly provided during final verification. The internal ignored child helper is invoked by the always-running parent; it is not an omitted acceptance test. Run `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime,test_tier_required controlled_bootstrap_cli`.

#### des-p2p-devlocal-retained-commit-recovery

Trace refs: O7-REQ-cf672e18-ed16-42a3-9338-ab7b965ac607 ([AC-DWE-001](../product/world-infrastructure/deterministic-world-execution.prd.md#ac-dwe-001)).

An ordinary DevLocal local decision can outlive the Node acknowledgement after its execution record is durable. Before executing that same local committed decision again, the execution hook may recover its original timestamp from the retained schema-v3 record only after checking the complete decision identity against its authoritative external-effect material (world/node/proposer, height, block, action root, slot/epoch and action anchors). The recovered time must also be used for subsequent replication and Node commit metadata. A conflicting or incomplete retained identity fails closed; no replacement action, timestamp rewrite or second execution is permitted. Direct execution replay continues to reject timestamp changes.

This narrow ordinary local recovery seam does not authorize network history, checkpoint-install records, legacy records, or guarded/formal continuations. Those keep their existing explicit original-context and proof contracts. It repairs continuation of an already durable matching decision; it does not make execution and Node persistence a single atomic transaction or certify an online repair.

#### des-p2p-guarded-local-initial-execution-prerequisite

Deployment is deferred by the user. The explicit `dev guarded-world --config <external-json>` engineering command installs the existing NodeRuntime execution hook and existing signed WorldService HTTP protocol. It does not activate the planned manifest-v2 formal profile, claim independent hardware fault domains, create an issuer/world/genesis identity, or enable Viewer/production-launcher consumption. Ordinary startup and ordinary client assurance retain their existing fail-closed boundaries.

Strict configuration version 1 has scope `guarded_local_execution_prerequisite`, `node_id`, existing private `node_keypair_directory`, private `records_directory`, loopback `status_bind`, `node_tick_ms` (1–60000), `trusted_bootstrap_config`, original `activation_evidence`, P/R directory paths, existing writer/P/R key-file paths, and an independently supplied `minimum_runtime_head`. Relative artifact paths resolve against the external configuration directory; the provisioned node-key directory must resolve to an absolute protected path. Existing activation/bootstrap verification runs before ordinary startup mutation. On Unix, P/R and node-key directories require current-user ownership and mode 0700; existing signing key files require current-user ownership, regular non-symlink files and mode 0600. Unsupported platforms reject this branch; it never generates keys or changes permissions of operator inputs. This config is independently authenticated operator trust input, not issuer authorization proof. The branch rejects external transport/provider/consensus overrides, constructs no libp2p/gossip/discovery/replication network, uses the existing single-local-validator decision machinery, and admits only existing authenticated WorldService routes on loopback. Public CAS/checkpoint/observer and legacy submit handlers are absent from this branch.

The Node captures the complete original local decided context, timestamp, predecessor hashes, action reservation, slot/height counters and actual stake totals before invoking the hook. Durable private continuation precedes execution. `Deferred` retains that exact decision and blocks successors; it never substitutes a network decision or reruns proposal/attestation evaluation. Candidates restore detached World::from_snapshot state, preserving no published-world persistence binding. Original signed WorldService input, complete Snapshot/Journal, result ledger and effect material remain exclusively in private operator storage. No candidate read cursor, external dispatcher or checkpoint export is available before qualification. A crash after execution starts but before a complete candidate becomes durable leaves recovery blocked: it fails closed without automatic executor replay, replacement nonce or repair. Only a retained complete original candidate supports the tested Unknown-to-restart continuation; formal PRD §9.2 service recovery remains unfinished.

The versioned generic nine-role closure preserves the existing ScheduleRecipe capture contract. Input holds exact original Node actions including signed request bytes; Manifest binds the original continuation, predecessor checkpoint and separate before/after simulation ticks. Snapshot/Journal include exact predecessor objects and complete journal-prefix continuity. Rules/WASM use concrete existing policies, module registry and module bytes; nonce closure explicitly covers capability nonce records, authenticated gameplay nonces, WorldService results and agent intent ledger. EffectOutbox includes existing pending/inflight state and original execution-effect material. Result and ExecutionHashPayload bind actual chain checkpoint height independently of simulation tick. Existing closed-record, Observer and capture budgets remain unchanged.

P/R `Unknown`/`Pending` retain the same request, ClosedRecord and candidate without successor execution. Qualification independently verifies both receipts, exact package/result/root/module closure and predecessor ancestry before immutable publication. The durable published checkpoint is saved before `Applied`; original continuation remains after Node completion for crash recovery. Restart reconciles the exact retained request against real locked endpoint status/head and the complete immutable published prefix. Missing acknowledged tails, changed config, alien heads and ordinary replication rollback/checkpoint/synthetic-decision recovery fail closed. An in-process monotonic verified-head fence prevents current HTTP reads from regressing after tail deletion; explicit historical pins still verify the entire prefix and exact requested CommitRef.

Graceful stop uses one caller deadline for admission-gate acquisition and Node-worker waiting; it never renews the worker budget after waiting for admission. Stop intent atomically clears readiness and sets stopping before waiting, preventing a new submit from passing its ready check. An already admitted operation may finish under the gate; the Node stop worker is created only after that gate is acquired and released. Gate acquisition uses deadline-aware polling rather than an unbounded mutex wait. Expiry returns retained `Pending`, not successful stop, and does not start a late worker; the runtime, ingress owner and original inflight work remain owned for a later bounded retry. An existing stop worker is never replaced. An unstarted/already-stopped owner without a Node worker does not acquire the active-Node barrier, but still performs its real delivery/server cleanup; `stop(0)` remains idempotent when cleanup is complete. Poison and true worker errors remain errors. This bounds deliberate waits, not OS scheduling or stderr latency, and does not promise every workload completes within one interval. Verification uses the controlled real-owner admission/worker regression and checks completion only after the same owner is actually stopped.

All signed Describe, IntentResponse, ReadView and Changes responses bind optional `execution_evidence_scope=replicated_durability_prerequisite`. Transport CommitRef identifies this engineering cursor only; it does not establish formal finality/readiness. Absent scope preserves ordinary serialized bytes. Ordinary RemoteWorldServiceClient rejects any present scope; explicit `ReadEvidencePolicy::AllowControlledPrerequisite` accepts exactly this scope and rejects unknown or omitted scope. Read authority identity binds that policy. Existing Viewer remains ordinary and rejects this backend; the bounded consumer is the explicitly configured pure API client for Describe, Submit, Lookup, ReadView and Changes only, with existing projection/agent authorization unchanged. The owner-read helper refuses engineering policy before transport; ordinary signed owner reads remain supported, and no engineering owner/Viewer capability is enabled.

Focused fixtures use real local FileEndpoints, actual NodeRuntime decisions and signed WorldService requests, with endpoint-file unavailability and restart recovery; two local directories do not prove independent hardware. Verification command: `cargo test -p oasis7_chain_runtime --bin oasis7_chain_runtime --no-default-features --features node-libp2p,wasmtime,test_tier_required controlled_live`.

#### des-p2p-formal-rpc-frame-preparation

现有 formal TCP 客户端须在建立连接前完成原签名请求的 CBOR 帧编码及既有 64 MiB 大小检查，避免本地编码空窗消耗端点已经开始等待首字节的 socket read 期限。此顺序不改变请求、签名域、挑战、minimum、帧字节或响应验证。每次调用仍在原位置确定一次 effective absolute deadline；编码后先核对剩余时间，再连接、写入已编码帧和读取响应，所有步骤消费同一 deadline，不在编码或连接后重新给满预算。编码失败、超限或编码期间到期须在零连接、零发送下失败；已有调用方提供的跨 RPC absolute deadline 继续覆盖其准备工作，不能通过局部帧准备续期。同步编码本身不声称可抢占，返回后到期仍必须拒绝连接。

端点 socket read timeout、客户端配置期限与测试等待期限保持原值；不能把端点首读等待误称为完整帧的 absolute deadline。未知结果仍查询原请求，不自动重试或推进信任锚。验证须覆盖受控慢编码时连接数为零、编码后过期/失败/超限零连接、原帧往返及现有慢分片 absolute deadline；小 loopback 模型只证明该排序契约，不追认历史 CI 超时或 RPC EOF 的根因。

原生 monotonic Oracle 在同一 TCP 连接上保有 exclusive lease，并顺序交换长度前缀与 CBOR body 分写的请求和响应。客户端各连接入口复用同一配置，客户端与响应端均启用 `TCP_NODELAY`，避免持续小帧交互等待小包合并。此连接仍使用 Oracle 原有的 1 MiB 帧界、阻塞 I/O 和读写 timeout，保持原 canonical 字节、签名域、challenge、lease、断连失败和物理存储语义；配置失败按 I/O 错误传播，不增加重连、自动重试或续期。共享测试 Oracle 使用相同的及时发送配置，并继续执行真实签名、checkpoint、落盘和恢复场景；本机模拟仍不证明独立故障域或真实防回滚部署。

### des-p2p-original-candidate-recovery-v1

本节选择“同原请求、同原决定输入的确定性 candidate 重建”，作为正式受控权威的目标协议。
它不是当前 guarded-local 的自动重放开关：现有完整 candidate 落盘前崩溃仍按
[工程边界](#des-p2p-guarded-local-initial-execution-prerequisite) 停写。
只有本节闭包、正式激活与故障验证全部实现后，新入口才可声明支持此恢复能力。

#### 执行前持久闭包

每个世界最多有一个已固定但尚未完成提交的 execution slot；未收敛前不选择或执行后继 slot。
选定输入只是不可替换的执行决定，不等于世界已经 committed。
执行器第一次进入前，`PreExecutionContinuationV1` 及其可达对象必须在 primary 和独立
replica 完整校验并同步落盘；两端返回同一 continuation digest 才允许执行。
只完成单端保存时只能续存同一记录，不能开始执行或另选输入。

该版本的必需材料如下；引用均为内容 hash，所指完整字节也属于持久闭包。

| 材料 | 必须固定的内容与校验 |
| --- | --- |
| 身份与位置 | `world_id/chain_id/genesis_hash`、合法 activation ref、profile/version、authority epoch、全局 position/height、预期 parent CommitRef/state root、原 Snapshot/Journal 与已验证历史锚。 |
| 原请求与 admission | 原始已签名 envelope 字节、请求 identity 与 payload digest、所有适用 nonce domain、原 nonce reservation、授权检查依据及原有序 action 序列；不从重建结果反推身份。 |
| 原决定 context | Node 已固定的时间、slot/height、stake、前驱 hash、simulation tick 和计数器；原执行使用的随机输入或确定性随机流及其算法版本。恢复后主机时间或新随机数不能替代。 |
| 精确执行版本 | 原 native executable 字节及 digest、受支持 target/ABI/feature 与确定性配置，原 runtime manifest、全部 WASM、规则与 typed 引用闭包；版本字符串或当前二进制不足以恢复旧语义。 |
| 无外部效果边界 | 执行所需 provider 结果已作为原输入固定；candidate 只能访问上述输入和内存暂存。LLM、HTTP、当前权限查询、主机时钟/熵及外部 dispatcher 均不进入重建过程。 |
| 持久控制 | continuation digest、原请求索引、预留执行/存储预算及状态。原 nonce reservation 随记录保留，不释放给 replacement，也不第二次扣除。 |

continuation 使用独立的非最终性签名域；签名只承诺选定输入，不冒充正式提交证明。
执行前按既有域容量规则预留闭包与 candidate 空间，超限拒绝进入执行；未决闭包不参与 GC。
native 历史工件不可用、平台不能复现原语义或原执行需要未冻结的外部输入时，进入
`recovery_required`，不能用当前 Runtime、mock provider 或补造 sidecar 继续。

#### 恢复顺序与写入一次

重启先核对两端持久状态、原请求索引、已发布 checkpoint、正式证明和外部最低 head 锚。
单端 `NotRecorded`、超时或本地 marker 不能证明请求未提交。
先采用已决定/qualified/published 事实；只有排除冲突历史且需要原 candidate 时才调用执行器。

| 查明的原请求阶段 | 唯一允许的后继动作 |
| --- | --- |
| 尚无固定 continuation 的 pending | 保持原 signed intent；服务恢复后按当前权限、资源、期限和 governing manifest 重新 admission，尚不适用原决定 replay。 |
| continuation 已完整持久、execution 未开始 | 校验原闭包和预期 parent；以原决定执行一次。当前 snapshot 不能替代原 parent。 |
| execution 已开始、完整 candidate 未持久 | 在 detached World 上用精确原输入重建 candidate；无磁盘发布绑定或 dispatcher。与任何已保存 proposal/result hash 对账后续存同一 candidate。 |
| 完整 candidate 已持久、尚无持久提交决定 | 校验完整九角色记录与原 continuation，续办同一 prepare/decision；不再次执行。 |
| primary 已持久决定、尚未 qualified | 决定不可撤回；补齐该决定的副本、回执和完整证据，不能重选结果或判失败。 |
| 已 qualified、尚未发布 | 安装同一不可变 checkpoint/result/nonce/outbox generation，持久发布指针后才回应；不再次执行。 |
| 已发布、原回应丢失 | Lookup/同身份重试读取原 disposition、结果和 CommitRef；不产生第二 receipt、里程碑或外部派发。 |

重建阶段的 `candidate_put(original_identity, payload_digest, continuation_digest)` 是持久
put-if-absent：同 key 同完整结果返回既有对象，任一 digest/root 不同即隔离。
任何残存 prepared proposal 必须与重建的全部九角色 bytes/roots 相同；不能仅比较业务结果。
原请求结果、业务状态、nonce/去重、里程碑和 outbox 沿一个正式记录及完整 generation 发布，
只能写入一次。候选内计算可以重新运行；对外效果仍仅由 committed outbox 按既有幂等合同派发。
这与 [Runtime 的 canonical history replay](../world-runtime/design.md#runtime-controlled-candidate-recovery-receiver)
是不同阶段，不放宽历史 replay 的不执行外部效果约束。

材料缺失、不同端点决定冲突、未知更高 head、原版本不可执行或重建不一致均保留原身份并
进入 `recovery_required`；只允许补回可验证的原字节或续办原持久决定。
重试次数、经过时间、用户换 nonce 或恢复后权限变化都不能把已固定决定变成失败或新动作。
恢复任务按世界串行，有界占用既有预算；没有新材料或状态变化时不无限自动重放。

### des-p2p-formal-controlled-authority-v1

本节规定 `controlled_single_authority` profile version 1 的正式发行与消费协议；
已有 native formal 子范围按[持久节点手册](blockchain/persistent-world-node-lifecycle.runbook.md#正式合同层与准入边界)核对，完整部署与各消费面的实现状态分别取证。它沿用两端完整持久判定，规定正式激活链、proof 编码、不可绕过的 signer/storage
admission 和普通客户端验证，不能通过删掉 `execution_evidence_scope` 升级工程记录。
此 profile 仍信任运营权威的执行正确性，不声称独立验证者 BFT。

#### 初次激活与证明编码

初次 issuer 来自部署前独立认证并固定的创世授权；不能由待验文件自行提供可信 issuer。
`FormalAuthorityActivationV1` 固定世界/chain/genesis、初始状态与执行工件、profile/version、
initial epoch、writer/primary/replica 身份和互不相同的 key、允许的恢复 issuer/key 与恢复权限，
以及部署 inventory 中的独立持久域引用。已有保留世界必须沿其合法身份与历史接续，
没有合法历史证明时阻断，不能以该证书重创世或把工程历史追认为正式提交。

issuer 的证书只授权首次 activation record。其签名 body 绑定初始 height、genesis parent、
初始 state root、执行工件和除 Input 外的八角色 roots；Input 是完整证书的 canonical 字节，
随后由正式 commit core 绑定 Input root，避免证书递归包含自己的 hash。
primary/replica 分别验证证书、完整 typed bootstrap 闭包及 genesis 身份，保存同一无业务
状态变化的 activation record。两端正式证明完整持久、checkpoint 安装且只读复验通过后，
才可构造不可由配置直接生成的 `ActiveAuthorityV1`；离线 `VerifiedInitialActivation`
和 bootstrap anchor 仍只是 prerequisite，不能转换为该对象。

正式对象使用新域 `oasis7.controlled_authority.formal.<kind>.v1.<purpose>`。
`kind` 仅为 `core`、`proof`、`activation`、`continuation`、`commit`、`primary_prepare`、`replica_prepare`、
`primary_decision`、`replica_decision`、`recovery_plan`、`epoch_seal`、`epoch_activation`。
`purpose=object` 对完整 typed 对象（含已有签名封装）求内容 digest；`purpose=signature`
只对该类型的未签名 body 签名，body 不含自己的签名。`core/proof` 只允许 object purpose，
`request_commitment`、`recovery_requests` 与 `pending_handoff` 是另三个仅供 object digest
使用的 kind，分别承诺原请求、恢复 allowlist 和待交接 pending 集合，不能签成正式提交。
编码固定为无浮点、无重复/未知字段、整数最短表示的 deterministic CBOR；map key 以编码字节
长度及字节序排序，数组保持协议顺序，缺失必需值和显式 null 均拒绝。
domain UTF-8 长度以 u64 big-endian 前缀编码，再串接 domain 和 body，形成 framing bytes。
BLAKE3 对 framing bytes 计算内容 digest；Ed25519 直接签完整 framing bytes，验证使用
strict verification，不把 digest 再当作待签 message。key/hash 为 32 bytes、signature 为 64 bytes。
这要求专门 codec 和 golden fixtures，不能把现有 serde 编码函数直接标记成新正式编码。

| 对象 | 必需绑定与准入规则 |
| --- | --- |
| `FormalControlledCommitCoreV1` | record kind、固定世界/chain/genesis、全局 position/height、epoch 内 sequence、parent commit hash/state root、result state root、activation certificate digest、profile/version/epoch、runtime/native/WASM/rules 工件闭包的 commitments、九角色 roots；execution 另绑定 `request_commitment` 与 continuation digest。公开 core 不包含原主体/nonce tuple 或原 payload。 |
| `FormalControlledCommitProofV1` | core digest、writer decision、同一 core 的 primary/replica prepare 与 durable decision 回执、适用 activation certificate 及其历史证明引用；角色 key 匹配当时合法 activation，公开 proof 引用逐一验 hash。九角色私有字节闭包只由受控持久/恢复端核验。 |
| 存储 head / 请求索引 | 原子 compare-and-append 校验世界、epoch、sequence、预期 parent 和原请求 identity/digest；同位置只能有一个 core，持久决定不可替换。允许确认的最高 head 与完整 proof/record 一起保存。 |
| `CommitRef` | 映射到该 core 的世界、height、commit hash、parent 和 state root；仅凭 WorldService 签名或格式正确不能构造可认可的正式 CommitRef。 |

签名不循环：activation certificate digest 不包含承载它的 commit proof；core 不包含自身
proof。prepare receipt 只绑定 core digest 与接收端角色/身份，不等待 writer commit 签名；
writer decision 的签名 body 绑定 core digest 和两端 prepare receipt digest；decision receipt
再绑定该完整 writer decision digest，完整 proof 是这些对象的外层封装。
first activation 使用 position 1 和 genesis parent；以后 position 连续，epoch 切换不重置
全局 position，epoch 内 sequence 从 1 起。普通 execution 的高度/上下文保持原决定，
控制记录不伪造 gameplay tick 或资源效果。

`request_commitment` 是专用 `request_commitment` 域中对原请求 identity、payload digest
和 continuation digest 的承诺；该域仅计算 BLAKE3 内容 digest，不签发提交证明。
原 tuple、原签名和 payload 留在授权 Lookup 及私有 Input，普通读者只核对承诺与正式签名。
公开 proof 验证证明受信 authority 对该 root 的正式承诺，不证明读者已经重执行私有状态；
ReadView 的私有投影继续依赖受权服务对精确 CommitRef 的签名绑定。

执行采用“两端完整 prepare → 合法 writer 签 commit → primary 持久不可撤回决定 →
replica 持久同一决定 → 两端保存全部 proof/record 与可确认 head → 发布”的顺序。
signer 仅能通过受控服务使用 key，按 record kind 验证授权：execution 校验当前 active
epoch、parent、两端 prepare 和原 continuation；initial activation 校验独立固定的 issuer
证书、genesis 空 head 与两端 prepare；epoch activation 校验既有 recovery policy、全部
seal、唯一新绑定和两端 prepare。后两者不要求先存在待构造的新 ActiveAuthority，且只允许
对应的那一条控制记录；不能把控制记录授权用于 execution。
writer 不能直接读出 signer key、替换最终存储文件或绕过接收端追加检查。
任一端丢失后，幸存端保存的完整 qualified 前缀仍可验证已确认历史；缺副本时只读，
恢复写入必须先恢复完整双端条件。独立域和实际 fsync 能力另须部署事实证明，不能由签名
或两个目录推导。首次正式开放及单故障域演练仍由现有运行验收闸门控制。

#### des-p2p-controlled-fault-domain-acceptance-v1

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39、O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4。P/R 的双端回执证明受信端点作出持久声明；实际抗故障能力由部署事实及故障注入共同证明。inventory 记录 P/R 的主机、存储、恢复/回滚边界和依赖的 signer/provider、单调 floor、issuer witness；至少证明声明中的一个主机或存储域失效不能同时毁损两份完整 qualified 历史。共享电源、区域、管理凭据及备份体系的相关风险须披露；未声称覆盖的同时多域毁损不外推为已保障。

nonexportable key custody、签名决定/epoch floor 的防回滚和独立 issuer witness 必须在真实 provider 上验证。进程锁、两个目录、不同公钥、远程 capability 字符串和本机 mock 均不能替代。provider/witness 不可用或完整性存疑时停写，既有可验证历史可只读；恢复不得退回 exportable key、本机计数器或自授 issuer。

| 故障窗口/动作 | 必须观察的结果 | 判定材料 |
| --- | --- | --- |
| P 或 R 任一真实域损毁；断电/强制退出后冷启动 | 已向用户确认的每个 CommitRef、root、原请求结果及闭包均可由幸存域恢复；缺另一端时无新 committed 回应 | 同窗口域 inventory、确认结果清单、幸存历史/闭包、冷恢复 root 与 receipt 对账 |
| 冻结前、冻结完成、候选写入中、决定/proof 写入中、qualified 后但回应前分别中断 | 未固定 pending 才当前重审；固定 continuation/候选/决定按原身份和原输入续办，回应丢失不创建新请求，重复调用不产生第二结算、LLM 调用或 outbox effect | 各持久 seam、原 identity/nonce、冻结输入/工件、决定与 oracle 对照、效果计数 |
| 旧 writer/signer 仍存活，旧盘或凭据副本在接管后重连 | 最终 signer 与存储入口实际拒绝旧实例；新 epoch/head 不回退，未证明隔离时不进入新写入阶段 | 受保护 floor/lease、控制面隔离事实、旧签写实际拒绝和无新增历史 |
| plan/seal/activation 每次持久动作后故障或重试 | 原 recovery_id、原 tail 和唯一新绑定收敛；缺历史、原 pending、闭包或隔离证据保持 blocked | 前后 epoch/head、原计划/seal/activation、重试回执和拒绝原因 |
| provider/witness 暂停、回滚备份或复制实例 | 无新签名/提交，不降为本机弱实现；合法恢复后仍保留原签名决定与最低水位 | provider 实际响应、counter/witness 原记录、恢复前后同请求结果 |
| controlled 的 H 已提交、BFT 的 H+1 尚未提交时故障 | 只恢复合法历史并保持停写；恢复 issuer、取消计划或旧 signer 均不能恢复 H+1 controlled 写入 | H 证明、交接边界、旧追加拒绝与零双写 |

该矩阵扩充既有 CA-R1/R2/R3、CA-F1/F2、CA-B1，不建立第二套提交规则。Ops 负责真实故障边界与注入，Runtime/P2P 负责原候选、proof/root 和拒绝事实，QA 按同一 world/配置/工件/窗口判定；本机协议测试与真实域演练分别出证。安全与可用性分开：单域故障后已确认结果不丢失可以成立，同时新写入因双端条件不全而暂停。本文不预填 RTO、实际 provider 能力或运行通过结论。

<a id="formal-replica-staging-content-reuse"></a>
#### 健康 Replica 归档安装中的内容复用

健康集合安装仍先验证 archive transfer 的签名 checkpoint、seal 和新绑定，再遍历完整认证闭包。copy walk 内的共享索引路径及已验证 Result envelope 的 business delta node writes 可以重复引用相同内容；每次引用不必再次进行远端签名 chunk 传输。

一次 `copy_authenticated_closure(copy=true)` 内可使用最多 16 MiB、4096 个对象的有界内容缓存，所有接纳字节先核对原 CAS digest；仅回填当前已验证材料内的 inline writes 或本次源读取的正确字节。16 MiB 约束保留对象的 bytes 权重，key/map、解码及返回值等临时分配不在此限内，不代表总 RSS 上限。缓存只提供相同 digest 的 bytes，每条引用边的 schema、深度、head/root/上下文、proof 和整体闭包预算仍独立验证。容量不足时按原源读取路径继续，不放宽预算、不缓存 absence 或任何授权/资格判定。缓存每次 walk 新建，失败、重启及跨 manifest 不复用；预复制的 raw seed 不成为缓存接纳依据或权威捷径。

provider 的 `copy=false` 独立本地 descriptor 读回不使用该内容缓存；目的端原文件校验、fsync、绑定与持久安装顺序保持。实际读取或接纳的源 bytes 与 digest 不符、缺边、错误 manifest/authority 及目的端已有对象损坏继续拒绝；已认证 inline Result 中完整的相同 digest bytes 可以用于重建目的逻辑对象，但不能据此声称原源副本完整或可用。分别记录 transfer 验证、closure copy、binding/persist 的单调耗时及源对象读取次数/字节、缓存命中/未命中；源对象读取数不冒充 chunk RPC 帧数，不记录 payload 或凭据。回归使用真实闭包与计数源证明重复读取减少，并验证容量退化、坏 hash、每条边校验、跨 operation 隔离和独立 provider 篡改拒绝。

#### 普通 WorldService / client 消费

目标正式服务固定使用 `WorldService contract_version=2`；v1 原 wire/signature 和工程策略保持
现状。v2 签名响应新增 `formal_authority_ref`；凡声明 committed view/result 的响应另必需
`commit_proof_ref`，二者均为内容 digest 引用，响应请求关联与原有字段也在 v2 签名内。
其中 `formal_authority_ref` 指向已持久 activation record 的完整 proof digest，而非单独的
issuer 证书；`commit_proof_ref` 指向本次结果记录的完整 proof digest。普通 execution core
中的 certificate digest 必须与前者证明的合法 activation 对应。
v2 不接受 `execution_evidence_scope`。签名域必须包含 contract version；不允许 v2 转 v1
或丢字段后重新签名来兼容正式消费者。JSON 中 u64 沿现有十进制无损约定，正式 proof 字节
由上述 codec 独立验证。未来 active tier 只引用已验证 activation，见
[manifest 接收边界](blockchain/formal-network-tiers-testnet-mechanism.design.md#formal-controlled-activation-receiver)。

| 路由 | 正式消费与失败后行为 |
| --- | --- |
| Describe | 返回经过验证的 activation/profile/epoch、proof scheme、当前 service grade 和 head；消费者核对独立固定的 genesis/issuer 与完整历史链。配置声明或服务签名不足以把 planned 变为 active。 |
| SubmitIntent | admission 检查固定世界、当前 active epoch 和当前授权；accepted/pending 不产生 committed 声称。进入原决定后固定原身份与 continuation，最终结果必须带可验证 proof。 |
| LookupIntent | 先验证查询主体与原请求关联，再查原持久索引；原结果不能因旧 Submit 的 freshness 或现行 execution 版本被重算。未决/未知/无效果拒绝沿既有 API 合同呈现，只有合格证明支持 committed。 |
| ReadView | 独立验证精确 CommitRef 的 profile/proof 与 root，再验证服务签名对该 root 和授权投影的绑定；公开证明不扩大 owner/private 投影权限。 |
| Changes | 每个 cursor 推进绑定连续的 CommitRef/proof；跨 epoch/profile 验证 activation 链。缺页、缺 proof、错误 genesis 或回退保留旧 cursor 并阻断推进，不混拼缓存视图。 |

#### Proof 字节交付与公开边界

上述引用必须可解析为完整公开证明。目标另设固定只读入口 `POST /v2/world/proofs`，
复用配置中已认证的 WorldService origin 与 v2 读请求/响应签名，不接受响应内任意 URL、
重定向 origin、文件路径或由 proof 声明的新可信服务 key。它只供应证明材料，不新增世界写入。
`ProofReadRequestV1` 绑定 world、request correlation，并且恰选一种查询：
`objects` 请求至多 16 个去重的内容 digest；`history` 请求从已验证 `after CommitRef`
到固定 `through_commit_hash` 的连续公开记录，单页至多 16 条。
响应回显原 world/query，完整返回对应对象或显式 `missing`；history 必须从 after 的直接
后继开始，返回最后 record 的 cursor 和 `complete`，不能跳页或由服务改写目标 head。

公开对象种类只含 core、正式签名与两端回执、activation certificate、epoch seal、公开
recovery authorization 和历史 header/proof。单对象 canonical bytes 上限 64 KiB，单次响应
编码后上限 2 MiB；未知 kind、超限、重复/额外对象、错 hash 或非 canonical 编码拒绝。
历史引用单调向既有 parent/epoch，循环或不连续拒绝；客户端按既有请求预算逐页验证，
只保存已经验过的连续锚，目标 proof 未闭合前不认可目标结果/推进其业务 cursor。
缺页、未取得证明、取消或超时只使本次核验保持不可用，并保留原请求与最后已验锚；
不能退回“只验服务签名”，也不能把同请求改成新动作。

接口使用显式公开类型白名单，不从 ClosedRecord 通用序列化输出字段。九角色的 Input、
Snapshot、Journal、nonce 全账本、outbox、目标正文、账户私有状态及完整执行闭包不在
公开 proof DTO，也不能通过其 root digest 从此入口读取；返回这些字段属于协议错误。
public recovery authorization 只含冻结请求集合的 commitment，不含原主体或请求列表；
运维隔离证据也只公开引用 digest，实际凭据/实例控制信息仍受控。
授权 Lookup 才把调用者原 tuple/payload 与 `request_commitment` 对账；proof 可读不授予
owner 读取权、原始账本读取权或重建 private projection 的能力。

普通客户端、Agent 与 Viewer 共用正式 verifier；服务 grade 与提交 proof 分别校验。
当前 authority 不可用但历史完整时可以提供明确的 verified read-only 历史；不能恢复写入。
客户端持久保存已接受的最低 head/activation 锚，重连拒绝低于该锚的历史；无锚的新客户端
仍须使用独立认证的 genesis/issuer，不能信任 endpoint 自报的新世界。
原请求 Unknown、rejected 及重试规则由 [外部 API 恢复协议](../world-simulator/llm/external-agent-game-api.design.md#api-operation-recovery)
继续主责，本节只增加证明消费，不引入一套替代结果状态。

| wire / evidence | 既有 v1 客户端 | 目标正式 v2 客户端 |
| --- | --- | --- |
| v1 且无工程 scope | 保持现有兼容行为 | 只能识别为兼容服务，不能打开正式世界或确认正式提交。 |
| v1 且为已知 prerequisite scope | Ordinary 拒绝；显式工程 policy 按原有界入口消费 | 拒绝正式使用；不通过删 scope、换签名或重新包裹提升旧记录。 |
| v2 且完整已知正式 proof/activation | 版本不支持，拒绝 | 全部身份、历史、root、签名和路由授权验证通过才接受。 |
| v2 缺该响应必需的 proof/activation、混工程 scope、未知 profile/版本/签名域，或未知 wire version | 拒绝 | 拒绝且保留原请求/cursor，不回退旧 verifier；真实 pending 无 committed 声称时不要求结果 proof。 |

### des-p2p-controlled-epoch-takeover-v1

目标 v1 采用人工停写与硬隔离，不使用 timeout/lease 推选新 authority。
创世授权只允许恢复 issuer 为同世界 controlled epoch 指定受限恢复器及唯一 E+1 绑定，
不授予它任意业务写入、改资产/授权、另建 genesis 或撤销 BFT handoff 的权限。
旧机器从硬隔离完成起即不能新增任何签名或存储写入；旧 key 在排空原请求期间仅由受限
恢复器使用，封 epoch 后才彻底退役，避免原决定已冻结却无人能完成其正式签名的死锁。

#### 权限与持久状态

`RecoveryPlanV1` 绑定固定世界身份、唯一 `recovery_id`、E/E+1、先前 activation、两端
可确认 head/外部最低锚、冻结原请求集合的 commitment、受限恢复器公钥、
原 writer/primary/replica 绑定、预定新绑定，以及实际隔离证明引用；由既有 recovery issuer
签名。每个 E 的计划用 compare-and-set 持久保存；同 ID 内容不符或第二份竞争计划拒绝。
完整冻结集合保存原 identity、continuation digest/position 和必要 payload，属于两端与
signer 的私有恢复闭包；按原 identity 的 canonical 编码排序，在 `recovery_requests` 域
计算的内容 digest 必须等于计划 commitment，不能从公开 proof 枚举。
issuer 签的就是上述含 commitment 的公开 plan body；公开 verifier 收到完整原签名 body，
不是从已签私有 allowlist 对象中删字段。受控端另持有完整私有集合并核验 commitment。
单世界单 execution slot 使允许继续执行的原决定集合至多一个；尚未选定的 pending
保留原身份，但不进入旧 epoch 恢复签名 allowlist。

实际隔离必须同时撤销旧进程对 signer 和接收端的凭据、终止并阻止旧实例重启/重挂盘、
撤销直写卷/对象存储路径及备用凭据。存储和 signer 服务由不同受控权限保护，旧 writer
不能拥有其管理员权限或原始密钥。基础设施控制面确认与旧凭据实际签写拒绝结果共同作为
隔离证据；“节点失联”“磁盘暂时不可达”或操作员声明本身都不是证据。
恢复所需 endpoint/writer key 必须可从受保护、独立于失效域的备份恢复到服务，不能因
key 丢失生成替代 key 来假冒旧身份；无法安全恢复则继续停写。

| 阶段 | 执行主体、持久动作与允许的后继 |
| --- | --- |
| `fenced` | Ops 完成真实隔离；恢复控制器读取全部可用持久前缀、外部锚与未决闭包。任何已确认 tail 无法证明完整时保持 `recovery_required`，不能选择较短 head。 |
| `plan_persisted` | 两端接收器和 signer 保存同一 RecoveryPlan 后进入仅恢复模式。只允许计划内原 identity/continuation/position，拒绝新 Submit、替换 payload、新 nonce 或其他旧 epoch core。 |
| `draining` | 隔离恢复器获得 signer 服务的计划限定访问，原 writer key 仍只签原决定；接收器同时校验 allowlist、parent、原 continuation 和两端 prepare。先补已持久决定/qualified/publication，再按恢复 v1 重建原 candidate。 |
| `drained` | 所有不可撤回原决定、result/nonce/outbox 已持久收敛并发布；固定唯一 cutover head C 与根。原未决定 pending 及其 nonce/去重关联完整冻结，形成私有交接集合和 `pending_handoff_root`。两端及 signer 校验无其余未决 continuation，持久绑定计划、C、该 root 与新绑定 digest。 |
| `sealing` | 两端及 signer 分别持久 `EpochSealV1(recovery_id,E,E+1,C,pending_handoff_root,new_bindings_digest)`；每个组件从本地 seal 成功起永久拒绝 E 签写，同时禁止普通 E+1 execution。首个 seal 后不能退回 draining 或改 C、pending 集合、新绑定。 |
| `epoch_activation` | 预授权 recovery issuer 签新 epoch activation，绑定先前 activation、C、`pending_handoff_root`、全部 seal 回执与新角色 keys。新绑定指定且满足独立持久域要求的 primary/replica 先复制并核对 C 的完整历史、九角色恢复闭包、外部最低锚、同一计划/全部 seal 及私有冻结集合；新两端完成本地持久接收后才可签该 activation 的 prepare。新 writer signer 先持久同一计划与 seals，只允许承载该证书的唯一控制记录：全局下一 position、E+1 sequence 1、相同业务 state root。其正式 proof/record 在新两端持久后才激活 E+1；旧实例不得因轮换 key 恢复签写权限。 |
| `active_next` | 新 ActiveAuthority 及当前 head/工件/双端条件全部通过才开放写入；原未决定 pending 按当前权限和 manifest 重审。旧 E 永久只可查历史。 |

受限恢复器不拥有原 key 的任意签名接口。signer 与两个存储接收器均从已持久计划生成
allowlist，只接受原 continuation 推导的 core 或已经保存的决定；新结果仍须按原闭包
确定性核验。恢复 issuer 的签名不能替代上述检查，也不能自己为旧 epoch 创建新业务请求。

待交接 pending 集合保留原签名请求、原 identity/digest、状态及 nonce/去重关联，按原
identity 的 canonical 编码排序，在 `pending_handoff` 域计算 `pending_handoff_root`；
空集合也有唯一显式 root，缺集合不能解释为空。该 root 同时进入 seal 与新 activation 的
签名 body，覆盖尚未进入 C 的 canonical execution 的已接受请求。新两端必须按 root
逐项验证并持久保存完整私有集合；缺失、冲突或较短 tail 均阻断 activation prepare。
公开 proof 只带该 digest，不能通过 proof 入口取得私有 pending；接管不重签请求或重占 nonce。

控制面只限制 key 的调用主体，未提前废除旧 epoch 的合法签名，所以 `draining` 能完成
原决定；seal 后的新 epoch activation 由创世预授权证书闭合，不再要求已经退役的旧 key
签新记录。新 activation 沿 C 接续同一历史，不执行游戏动作或制造新资产。

#### Writer custody 服务的实际消费边界

正式 Runtime 的 `FormalLiveConfigV1` schema 2 仅加载独立客户端凭据：
`writer_custody` 指定 loopback endpoint、`credential_file` 与外部固定的
`minimum_revision`；writer 公钥和 authority 身份来自已验证的授权链，不从服务响应
反向发现信任。该 schema 禁止 `writer_key_file`，包括显式 `null`。schema 1 的原本地
key 配置保持兼容，但它不能证明旧进程在服务端失去最终签名资格。

实际 writer 服务通过 `oasis7_durability_endpoint --writer-custody-config <file>`
独立启动。其配置持有 writer key、固定的普通与恢复凭据公钥、原授权链、存储模式及
独立 revision 下界。`initialize` 只接受全新空目录；`open_existing` 必须读取同一
目录绑定、authority、凭据和恢复授权的原签名账。缺账、缺稳定锁、损坏、绑定改变或
低于外部下界均在发布 listener 前拒绝，不能自动生成空 epoch。

该入口使用 `ControlledFileWriterProvider`：原文件租约覆盖校验、持久迁移与最终签名，
写入完成须同步文件并原子替换、同步父目录。普通凭据和计划限定的恢复凭据分别认证；
Runtime 只调用 Frozen、原 candidate 和封闭 endpoint 请求等既有签名用途，没有任意
bytes 签名能力。请求绑定 authority、challenge 与 revision，下游 endpoint 的认证签名
与该次 RPC 共用原绝对期限。回包丢失保持 unknown，只能查询或重试原计划／原 seal，
不因此创建新的恢复身份。

此 provider 是 `CONTROLLED_ONLY` 的本地文件与进程崩溃实现。它提供实际服务端撤权、
持久账与冷重开的软件消费链；普通文件 IO 不证明非导出密钥、管理员反回滚、独立备份
或物理故障域。这些部署事实仍须按上文的真实隔离与保护要求独立取证。当前远程能力
未实现的健康 durable-set 变更在 arm 前拒绝，不从恢复能力推导另一类变更权限。

#### 单域丢失、半完成 epoch 与旧盘

单域失效时先硬隔离失效角色的旧实例，再从幸存端完整 qualified 前缀、可确认 head、
未决 continuation/decision 与外部最低锚恢复；不能只取 snapshot。
若排空需要缺失 endpoint 的回执，先以原 endpoint ID/key 恢复其受控服务并复制完整闭包，
两端核对后才进入原 E 的受限 draining。角色换 key/端点的正式生效只能发生在 E+1；
同身份恢复旧服务也必须证明旧实例无法再签写，不能并行运行两个持有原 key 的实例。

原数据目录完全丢失时，普通初始化与 acquire 仍拒绝用新目录指纹替换原 instance。
显式 `recovery_repair_bootstrap` 消费原 issuer 签名计划、原固定请求、writer 确认、
私有 pending 交接文件、幸存端 checkpoint 与 archive，以及独立最低 revision 和显式
复制字节预算。恢复授权由原 protected provider 在原排他租约内签发，固定同一计划、
恢复凭据、新目录指纹、原角色和原签名 native checkpoint；逻辑 endpoint ID/key、
authority 与原单调账不变。授权一旦生效，原普通凭据不能继续签写；服务冷重开只能
继续同一授权，不能把缺账解释为允许初始化。

幸存端只提供按原 hash 查找的不可变材料，不为丢失角色生成 checkpoint 或最低锚。
恢复先重建原 pending/index 的确切 bytes，再沿原 protected native root 复制并验证
完整闭包，随后仅从本地目录独立重验相同 inventory；移除幸存端读取后备并持久安装
原计划后才发布 listener。未齐全、损坏、预算不足或无法重建原 bytes 时保持拒绝。
若只丢失已持久化的原角色收据，provider 仅在原计划允许的 Frozen/Prepared 签名用途
内计算签名；原收据前后 bytes 以至多 64 KiB 分块传输，连同签名总计不超过 64 MiB，
沿用原 1 MiB frame 限制。同一 lease 的所有分块共用一个绝对期限，完整原 pending
hash 核对成功前不得向客户端释放签名。缺块、乱序、换授权或失回包均不产生新回执
资格，也不允许通过续期或自动改写原请求重试。

计划或 seal 只写到一端后崩溃，不把它视为取消：重启合并可验证的单调状态，只能完成
同一 recovery_id/新绑定；C 一旦在 drained 阶段固定也不可改变。尚未 sealed 的端点不得
独自放行 E 或 E+1；任何一份已持久
seal 都要求重建另一端后续封，不能回滚本地 max epoch。所有端点在正常服务前先核对
现行计划/seal/activation 与独立最低锚，旧盘的较小 epoch/head 只可作历史材料，不能成为
writer、signer 或最高 head 的来源。
若现存材料无法判断是否曾封 epoch、是否存在更高可确认 tail 或旧写权限是否已隔离，
保持 verified read-only / `recovery_required`；不以超时推断失败或重开旧 epoch。

恢复发现已经正式提交的 BFT handoff H 时，拒绝创建或继续 controlled E+1 activation。
即使 H+1 尚无 BFT 提交，也只能由合法新 profile 恢复；本节不撤销 H 或恢复旧单权威。
未开始隔离且未持久计划前可放弃一次接管准备；进入持久恢复流程后按表单调推进，
不把恢复失败当作旧机器重新上线的授权。

<a id="des-p2p-fresh-preview-bootstrap"></a>
### 新预览世界初始化

`initialize-preview-world` 是普通新建 DevLocal 技术预览的显式初始化入口。
它消费操作员固定的 world/node ID、已有受保护 node key directory、全新绝对 root、
loopback endpoint 与 genesis 初始化 nonce；nonce 不选择模拟世界 seed，也不是授权凭据。
已有世界的身份、历史或部分目录不能被该命令覆盖、修复或重新创世。

创世摘要使用版本化域分离编码，绑定真实保留的初始状态/日志字节、初始化参数和节点派生
服务公钥。初始化使用现有 canonical Runtime preset、实际 step 与原 NodeRuntime/execution
driver，生成可验证 CAS、提交记录和 world proof；不写入测试能力、虚构资金、占位提交或
人为 finality。cognition 的 pending binding 和本地提交只证明该预览的初始化材料。

Trace refs: O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39。显式
`devlocal-provider-v1` 预览 profile 在同一个全新 staging 世界内，先安装操作员指定的
真实 local-test-provider WASM 与构建材料，再绑定 Runtime，走现有 DevLocal
proof-bearing authority admission，形成有限 grant、allowance 和 provider invocation。
仅该显式 fresh profile 在创建时选择已有 DevLocal release security policy；默认 public
preset 的 hardened policy、现有根与 Runtime 的 production 拒绝条件保持不变。
随后普通 `ClaimStarterOc` 动作建立指定 Agent 的 canonical owner 和有界 starter 资金；
owner 通过正常签名与 Runtime 验证安装 executor delegation。该 profile 不叠加 W3 的
pristine-ledger funding，不使用测试 fixture 或直接编辑 snapshot/context。

profile、WASM/build 摘要、Agent/owner、allowance、显式本地 finality marker 及生成的
owner/executor 公钥全部进入创世与签名 receipt；本地 marker 不证明正式 BFT finality。
首次真实 NodeRuntime commit 后才发布根目录。owner/executor 私钥、authority bundle 和
Agent-scope consumer 环境只写入 0600 私有文件。默认入口仍只生成 public-read 材料。
已存在根只能按原 profile/输入只读重验，不能补装 provider、自动续 grant 或重新签 receipt；
initializer source commitment 变化仍按原合同拒绝旧版本重跑，不能伪造旧 hash。

输出在同文件系统的 0700 sibling staging 构建，文件和目录同步后通过 OS 原子 no-replace
发布。receipt 固定初始 publication manifest 的摘要；manifest 绑定原始 execution record、
节点签名 envelope 及 snapshot、journal、world-head proof 的确切 CAS 字节，使初始证明在
历史 CAS 回收后仍可验证。目标已存在时核对原 receipt、创世原字节、identity、实际 signer、
归档初始证明与当前 live CAS/record/proof；缺失、冲突、篡改或非受保护路径立即拒绝。旧根若
没有该认证归档，只能从已验证备份恢复，初始化器不重建、不重签、不改写旧证明；幂等查询也
不因后续真实提交推进而把原初始锚改写成当前 head。

DevLocal 历史读取缺失热窗口物料时，不把当前 root 当成旧提交，也不将
`HistoryUnavailable` 降为可重放的 `Unknown`。显式离线诊断可在独立空 scratch 中，先
验证独立 service pin 固定的原 receipt、创世与 initial-publication 完整归档，再消费该
归档认证的节点 signer 所签的逐高度原 consensus envelope。必须有完整有序动作字节、
原模块依赖与 external-effect/proof 闭包；使用真实 execution driver 重执行，并逐高度核验
动作 root、前驱执行链、execution hash、state root 与原 proof/materialization。仅校验
payload hash 或当前状态中的结果索引不构成历史 World。归档校验、初始 World 恢复和最终
读取使用同一个受信 Host RestoreContext；public profile 保持 hardened policy，显式 provider
profile 复用其已签名声明的 DevLocal policy。所需 participant authority 必须由调用者独立
固定 operator pin 后显式传入，不能从 snapshot/receipt 自授。缺物料、越界或 deadline 到期即
拒绝，不补造旧签名、不改原 records/CAS/current World，不产生新模型调用或写入权限。
隔离 replay 不在 WorldService query 内运行。显式 `rehydrate-preview-history` 仅接受原
DevLocal preview 身份与独立 service pin，先取得既有 World writer 排他锁证明原 writer 已停，
再在独立 scratch 完成全部检查。操作员必须明确给出 replay 高度/累计字节/deadline，及
目标 CAS 总物理字节、新增字节、对象数和历史 cut 数预算；无默认无界 cap，不冒用正式
protected ledger 的 4 GiB 预算，也不删除旧对象腾配额。所有预检成功后，持同一 CAS
retention guard 发布缺失的 hash-identical snapshot/journal 原字节及独立 pin shard；原
current head、compact record、World 状态、nonce、模型 ledger、密钥和原创建 receipt 不变。
失败可以留下尚未引用的已验证 CAS 字节，但没有完整 exact pin/闭包不得报告可读。

同一 Trace `O7-REQ-0bb14512-622e-48fc-879f-bd9f00127b39` 的显式操作员命令
`verify-preview-history` 复用唯一的认证 replay 路径及相同 writer 排他约束，只在调用者给定的
独立 scratch 验证原 world、node、service pin 与精确高度；不发布原 source 的 CAS 或 pin，
不启动服务、网络请求或模型调用。必须显式指定 replay 高度、累计字节和 deadline 预算。
源业务 World、records、CAS、pins 和模型 ledger 保持原字节；writer 排他锁的瞬态文件不视为
业务变化。可选原始错误只写调用者指定、source 外、当前用户拥有的 0700 canonical 父目录
下全新 0600 私有文件，拒绝 symlink、覆盖和超界；公开输出仅固定错误类别与数字高度。
诊断来自同一次 replay 的原失败点，不二次恢复或绕过原预算。

历史 reader 只从原 record 绑定的原 world-head proof 推导缺失 refs，核对独立 scope 的
exact pins、CAS hash/boundary，再复用严格 World restore；补 refs 仅在内存 record clone，
不回写原 record，不在 query 中重放或修 pin。缺闭包仍是 `HistoryUnavailable`，冲突/损坏
立即拒绝。独立 pin scope 同时进入既有 GC/record-retention protected height 集合，避免后续
pruning 丢失原 proof 入口；它是可验证读取材料的保留缓存，不授当前 head、服务/签名或正式
BFT authority。该恢复只证明相同 DevLocal 原高度的执行结果，不授跨故障域耐久资格。

私钥只保留在受保护配置中。公开初始化 receipt 给出 identity、材料承诺和 derived
feedback-submit service public key，不提供 node seed 或消费者签名私钥。操作员通过独立
认证的部署通道核对该 pin 后，将 endpoint/world/genesis/service key 和独立 public-read
配置交给 Viewer；读取签名不授予 Agent、operator、validator 或治理权限。

这是新建隔离预览的 provenance 和初始化合同，不替代上文正式创世授权、profile 激活、
历史恢复链或独立故障域耐久性。跨平台输出需核对相关 schema/hash/persistence 等价并在
目标机器实际打开和验证 pin。使用与负例验证见 [初始化手册](../testing/manual/fresh-world-preview-bootstrap.manual.md)。
