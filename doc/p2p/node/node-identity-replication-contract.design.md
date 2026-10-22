# Node identity and replication contract design

## 1. 问题、目标与非目标

把local identity、conditional signer map、signed replication的验证/持久/观察顺序固定，避免假进展和corrupt authoritative recovery默认化。

Owner role：runtime_engineer；canonical repository：eng-cc/oasis7；固定审读基线 f9d5a552d9af04c1b1398262808198a58e560230，2026-09-26。本设计消费下表有限关系；旧章节保留作兼容详细条款，新增DES细化原义务而不改算法/schema/默认值/产品承诺。任务实际source/integration/tested tree/config/environment/window/结果由GitHub Issue/Project evidence维护，本文不伪造候选/通过结果。

非目标：不以文档active/合并、local fixture、历史MIG或CLI可编译宣称运行、BFT、部署、SLA或release。剩余215-object required set及root SC1..10/full组合不变。

## 2. 上游约束与相关角色

runtime/P2P拥有机制；ops拥有同窗口部署事实；QA拥有组合证据判定；producer拥有产品含义；消费者和WASM拥有各自schema/manifest/行为。professional_acceptance与product_requirement是关系类型，本文不新增机器trace schema或activation。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [professional_acceptance: nir-bootstrap](node-identity-replication-contract.prd.md#nir-bootstrap)；Trace refs: O7-REQ-b39ac7bb-609b-4784-9a94-7ef48ad4280e | disabled 不建 key；合法 missing-key ensure／显式 init 至多发布同一身份；崩溃重试不替换已绑定身份，异常配置明确失败。 | [des-nir-bootstrap](#des-nir-bootstrap) | Runtime / Node / filesystem platform / QA | 产品适用性：N/A；reason=本机初始化的专业持久性要求；scope=密钥、锁、初始化绑定及恢复，不包含 world 恢复／custody／admission。 |
| [professional_acceptance: nir-signer-binding](node-identity-replication-contract.prd.md#nir-signer-binding) | 配置map完整normalized32-byte，无unknown，signature先于binding，缺失/mismatch拒绝 | [des-nir-signer](#des-nir-signer) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；局部 Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 |
| [professional_acceptance: nir-ingest-order](node-identity-replication-contract.prd.md#nir-ingest-order) | network injection只路由；world/topic隔离不混状态 | [des-nir-isolation](#des-nir-isolation) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: nir-ingest-order](node-identity-replication-contract.prd.md#nir-ingest-order) | apply/persist成功才peer/committed observation；failure不假推进 | [des-nir-ingest](#des-nir-ingest) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: nir-ingest-order](node-identity-replication-contract.prd.md#nir-ingest-order) | single-writer/order/duplicate/stale持久guard；hash/write失败不污染 | [des-nir-guard](#des-nir-guard) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: nir-recovery](node-identity-replication-contract.prd.md#nir-recovery) | corrupt/unreadable PoS authoritative state blocks start；保留诊断纠根因 | [des-nir-recovery](#des-nir-recovery) | runtime/node/net/distfs与QA；相关外部authority | local test source≠执行通过；全拓扑/恢复/finality/release未证明；局部 Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 |
| [product_requirement: REQ-DCS-003; O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | 启用 binding 时有效签名仍不能冒充受治理 validator | [des-nir-signer](#des-nir-signer) | Runtime / P2P / Ops / QA | 不证明 map 未启用部署、registry/轮换、网络暴露、BFT 或全部角色隔离 |
| [product_requirement: REQ-DCS-002; O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | corrupt/unreadable PoS authoritative state 阻断启动且不默认重置 | [des-nir-recovery](#des-nir-recovery) | Runtime / P2P / Ops / QA | 不证明全恢复链、signed-V2、正向新请求/模块行动/结算续跑或可服务 |

## 3. 当前状态、目标状态与差距

本轮静态复核固定 `a0b6ab12ad14c6c881e48cecdb4adbdb322dd30e`（2026-10-15）；原冻结审读是历史合同来源。本轮仅核源码/测试定义，未执行运行、网络或恢复验证。

重要差距：[node_engine_replication.rs](../../../crates/oasis7_node/src/node_engine_replication.rs) 先验证远端记录并调用 observe_network_replication_commit，之后才调用 apply_remote_message；[node_engine_network.rs](../../../crates/oasis7_node/src/node_engine_network.rs) 的该观察更新 network_committed_height 和 peer_heads。因此远端 advertised/network 观察与本地 persisted/committed cursor 不能合称“全部 apply 后观察”。§4/§5/§7 及保留的 Flow 是原规范目标；是否应改变字段语义或目标顺序须 Runtime owner 决定，本轮不选择。

[Node replication](../../../crates/oasis7_node/src/replication.rs) 的 apply_remote_message 在 DistFS apply 后先更新内存 remote_guards，再持久化 commit 与 guard。hash/write 局部失败断言不足以证明所有 persist/crash 边界原子性。

| 对象 | 当前专业合同（冻结source） | 目标 / 差距 | 实现 / 执行证据 |
| --- | --- | --- | --- |
| 本专题现行规则 | 配对PRD与保留旧章节的有界接口/失败合同 | 可选map未启用部署不能被说全节点binding；persist-failure/crash atomicity尚需node oracle；ops/root全链恢复外部。 | 本次source阅读，未运行behavior tests；具体测试定义存在不等于覆盖或通过 |
| 跨节点组合能力 | local documented contract，历史完成是provenance | 依赖root/runtime/ops/consumer与full同candidate验证 | 运行/部署/发行未验证；不能由文档推导 |

身份初始化的历史差距基线为 `b42a9e3fb8c8cd60679936c5f46c2fde6f62cf1e`：旧 `ConfigFileLock` 使用 create-new 标记和 Drop 删除，helper 直接写目标文件。当前 [node_keypair_config](../../../crates/oasis7_chain_runtime/src/host/node_keypair_config.rs) 已接入 [身份事务](../../../crates/oasis7_chain_runtime/src/host/node_keypair_config/transaction.rs)：Unix OS 锁、无私钥意图、私有候选、文件及目录同步、原子发布和同身份重试；[node lifecycle model](../../../crates/oasis7_chain_runtime/src/host/node_lifecycle/model.rs) 继续负责外层世界／目录绑定。正式 v2 的运行租约再将身份绑定到实际 world/chain/genesis 与 writer 目录，并由真实执行／效果签名后台所有者持有。此为 #4975 的本机密钥与正式 v2 运行子范围，整体初始化合同仍为部分实现：不推导新角色准入、绑定跨文件断电保证、生产 custody 或其他七个范围已完成。进程级故障回归与实际执行结果分别见测试映射和实施 PR；真实断电／目标文件系统故障验证仍待执行。

## 4. 边界与结构

config bootstrap只本地dev；consensus/governance/custody各自authority。NodeRuntime持有network handle和guard/state，network仅运输，ops读取runtime status不重定义truth。FileReplicationRecord必须world/topic/source/signature/ordering适用。

### des-nir-bootstrap

Trace refs: O7-REQ-b39ac7bb-609b-4784-9a94-7ef48ad4280e；唯一专业声明：[nir-bootstrap](node-identity-replication-contract.prd.md#nir-bootstrap)。

本条负责 local/dev 启用前可选 ensure 和显式 `node init` 的本机身份事务；disabled 不创建，非法已有身份不得覆盖。formal authority／validator／governance signer 的受信配置不参与自动补 key。Product N/A；reason=本机初始化不构成玩家世界恢复、生产 custody 或 validator admission；scope=当前私有配置、锁、密钥发布、初始化绑定与重试。

**互斥与输入。**在经过 owner/mode/no-follow 检查的固定配置目录中取得 OS 生命周期锁，持有至身份验证／创建和绑定完成。锁文件是固定锁载体而不是“存在即占用”的永久标记；不能 unlink/recreate 一个仍可能被持有的锁 inode，也不能根据超时、PID 文本或本次进程看不到旧 PID 就夺锁。进程退出由 OS 释放锁；平台／文件系统不能提供所需锁、原子发布或同步语义时明确拒绝该模式。超时只报告当前 busy／blocked，不生成新密钥。现有 malformed/conflict/unwritable 与 symlink 拒绝保持。

**准备与发布。**先持久保存原 init identity、目标路径和配置摘要；意图不包含私钥。不存在已发布合法密钥时，在同目录以私有权限、排他创建的临时文件写完整 keypair/config，核验公私钥匹配与原配置字段，再同步文件。临时材料及公钥指纹关联原 init 意图；完整的同意图材料必须复用。只有原子发布成功并同步父目录后才把密钥视为已发布，然后持久保存 node binding 与 complete 状态，再返回公钥／PeerId 和成功。

专用 key 文件使用“不覆盖既有目标”的原子发布；local/dev 在已有合法 config 中补缺失 key 时，持锁核对原文件身份／摘要后原子替换完整新 config，保留其他配置，不用原地 truncate/write。已经存在的有效 key 不生成替代值；已有非法 key 明确失败。普通 ensure 原有的合法 private-only 补公钥行为若保留，也必须从同一 private key 派生，不能生成新的 private key。

**恢复判定。**重试先取得同一锁、读取原意图、检查已发布 key 和绑定，再决定下一步；不根据日志中“完成”字样决定身份。各持久文件采用完整写入、文件同步、原子发布和目录同步，不能要求多个文件天然原子。发布后目录同步失败属于结果未知／待恢复：保留原意图与材料并核验目标，不能删除目标重来。

| 观察到的状态 | 恢复动作 | 禁止行为 |
| --- | --- | --- |
| 原意图存在，目标缺失，未形成完整临时 key | 验证临时路径归属，隔离不完整材料；按原意图继续首次准备，尚无已发布身份 | 不把不完整文件当可用 key，不输出可用于授权的公钥 |
| 原意图和完整临时 key 一致，目标缺失 | 重验后发布同一 key；新建意图不能接管该材料 | 不另生成第二份 key 或读取未知所有者的临时文件 |
| 完整 key 已发布，binding／complete 未写完 | 与原意图核对，重建同一公钥／PeerId 的缺失绑定并持久完成 | 不因元数据落后而换 key；不在身份冲突时补写绑定 |
| key、binding 与完成记录一致 | 重验后幂等返回同一身份 | 不以重跑作为 rotation |
| key 损坏、多个候选冲突、binding 不匹配或原意图不可确认 | 保留现场并返回 recovery-required／blocked 及不含 secret 的诊断 | 不静默修复／覆盖，不删除未知文件，不宣称成功 |

以上恢复名词是设计语义，不声明已有 CLI 新枚举。初始化恢复只完成本机身份，不自动加入治理集合、重签 grant、转移 signer 或启动网络。旧 lock 格式迁移必须在能证明没有并发旧版写者后进行；新旧锁协议混跑不具有本设计保证。

#### 正式 v2 运行期身份租约

Trace refs: O7-REQ-b39ac7bb-609b-4784-9a94-7ef48ad4280e。初始化互斥和运行互斥是不同生命周期：正式 v2 在任何启动历史读取、网络请求、writer 打开或服务线程启动前，重新验证已配置的完整身份，依固定顺序取得同一 key 路径及 writer 数据目录的运行期 OS 排他锁。初始化 helper 返回后释放的锁不能代替此租约。锁载体永久保留；busy 时立即阻断，不能因 PID、超时或更换 writer 目录绕过。

非秘密运行绑定持久保存规范 key 路径、writer 数据目录、node ID、root public key、派生 PeerId、实际受信 world/chain/genesis 三元组及 profile。网络域来自已验证的正式锚，不来自远端自述或自行补出的 network ID。角色不属于身份绑定；取得租约不授予 writer、读取、治理或投票权限。首次启动可收录已有合法身份，后续只接受完全一致的绑定；损坏、错域、换目录、换 node ID 或原密钥丢失须保留原件并阻断，不自动覆盖绑定或换 key。已有完整身份与尚未建立运行绑定的升级仅允许在所有旧运行者停止后执行；新旧版本混跑没有此保证。

运行锁分别为 `<key>.runtime-identity-v1.lock` 和 writer 目录的 `.node-runtime-identity-v1.lock`；记录分别为 `<key>.runtime-identity-v1.json` 和 `.node-runtime-identity-v1.json`，严格版本 1、拒绝未知字段，最大 16 KiB，锁与记录均要求私有 owner/mode。同一绑定分别写入 key 邻接记录与 writer 目录记录，防止一个 key 换域或另一 key 接管既有 writer 目录；两处不假装原子，崩溃后只续写完全一致的缺失记录。绑定使用同目录私有临时文件、文件同步、原子发布及目录同步。写入／同步结果未知时保持原材料，下一次持锁核对同一绑定；不输出成功。租约必须随实际 execution hook、效果签名 worker 与后台运行所有者存活，不能只放在外层启动函数中：停止超时或外层对象退出，而后台执行仍可推进时，锁仍须保留。全部运行所有者退出后由 OS 释放；重启核对同一身份和绑定后才恢复。

本切片只接通正式 profile-v2 的实际启动与后台执行路径。legacy standalone／DevLocal 生命周期、显式目录迁移与合法轮换的运行绑定重写、独立复制 bare key 的跨目录／跨机排重、防整机回滚、真实断电验证仍不在已实现范围。复制完整绑定到其他目录因规范路径不一致而拒绝；相邻锁不是全局 key 注册表或跨机 fencing。已有迁移入口不得静默改写此绑定；需要迁移的新正式目录应先完成受支持的离线重绑定合同。

#### 当前本机密钥格式与升级边界

- 新锁载体为 `<config>.identity-v2.lock`，权限 0600；OS 释放锁后仍保留文件，禁止在线 unlink/recreate。普通 local/dev 配置父目录须由当前用户拥有且不可被 group/others 写入；祖先须由当前用户或 root 拥有，并拒绝可被他人无 sticky 保护替换子目录的路径；专用目录仍要求 0700、完整 key 文件 0600。新建父目录逐级持久化。非 Unix 当前仅允许读取已存在完整身份，不提供自动生成／补公钥。
- `<config>.identity-v2.json` 保存版本、规范目标路径、原配置摘要、随机候选 ID、准备摘要、公钥及 complete；不含私钥。候选 `<config>.identity-<id>.toml` 排他创建、0600，保留非 key 字段；完整候选重试复用。新文件用 hard-link 不覆盖发布；既有 private-only config 用同目录发布链接及 rename 原子替换。完成记录持久后删除候选；同步失败保留可重试状态，不删除目标。
- 已存在完整 key 原样收录公钥连续性记录。complete 后允许修改非 key 配置；目标丢失、key 改变或损坏须恢复原件。路径绑定变化不能直接复制旧意图接管；正式迁移仍由原迁移入口负责。无摘要且为空的未发布候选隔离为 `.incomplete` 后按原意图继续；非空损坏或冲突候选保留并阻断，须由操作员核对原件恢复。
- 升级必须先停旧写者，不支持 v1/v2 并发混跑。发现旧 `<config>.lock`（包括未知标记）立即阻断并提示离线迁移；确认无旧写者后由操作员处理旧标记。不能按 PID／超时抢锁，也不能回退旧版继续写 v2 目录。备份应覆盖原配置、v2 意图和未完成候选；新协议不宣称抵御整机旧备份回滚。
- 正式数据目录迁移在既有 stopped/source fence 下，校验源完整身份后，仅在私有 stage 中重绑定 complete 意图的目标路径，并随原目录原子发布；未完成发布或未清理候选必须先在源完成恢复。legacy 迁移同时覆盖独立 config 与嵌入 runtime 组件的 config：不可遗漏相邻 complete 意图。其不可变 Entry 可选 `publication_metadata` 固定该公有 JSON 的目标字节；源校验仍用原摘要，目标校验／容量用变换后字节，原 key／world／业务文件原样复制。缺此字段的旧 plan 序列化不变；旧解释器不得接管带新字段的 plan，仍受原 runtime digest 约束。普通复制不提供重绑定权限；source 与 migration receipt 保留原身份连续性。
- 所有成功／阻断只涉及本机 key；不得由此签发 admission、私有读取 grant 或 validator/controller 权限。进程 SIGKILL 与操作边界注入不等于真实断电验证。


### des-nir-signer

配置map完整normalized32-byte，无unknown，signature先于binding，缺失/mismatch拒绝。

Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3。局部承接 [REQ-DCS-003](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003)：启用对应配置/签名模式后，即使签名有效，missing/mismatch binding 也不能让任意 signer 冒充已受治理 validator。未配置 map 的部署不能据此声称完整绑定。治理 registry、轮换、activation、所有网络角色与被攻破服务组合不由本条证明。

### des-nir-isolation

network injection只路由；world/topic隔离不混状态。

本条只负责 network handle 的 world/topic 路由和适用 source 拒绝；不把注入视为部署。Product N/A；reason=局部隔离 fixture 不是完整同世界恢复或公网可达验收；scope=注入、空 topic 拒绝与 world/topic/source 边界。

### des-nir-ingest

apply/persist成功才peer/committed observation；failure不假推进。

本条保留原 apply/persist 后才推进 peer/committed observation 的目标，不把该目标写成已实现事实。当前先观察远端 advertised/network head，再执行 apply；字段范围与原目标的差距见 §3。不得借规范化放宽原 PRD。Product N/A；reason=局部 ingest 顺序与 cursor 不足以承接完整 DCS 恢复链；scope=验证、apply、persist、失败不假推进及观察字段的待核范围。

### des-nir-guard

single-writer/order/duplicate/stale持久guard；hash/write失败不污染。

本条只规定 local/remote single-writer、顺序和持久 guard，保留 duplicate/stale 拒绝与 hash/write 失败不污染。DistFS 局部 guard 成功不替代 Node apply/persist crash atomicity。Product N/A；reason=局部 guard 算法不等于完整 checkpoint/snapshot/replay 恢复；scope=记录顺序、去重、持久失败与崩溃边界。

### des-nir-recovery

corrupt/unreadable PoS authoritative state blocks start；保留诊断纠根因。

Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4。局部承接 [REQ-DCS-002](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002)：已定义的 PoS authoritative state corrupt/unreadable 必须阻断启动，不 default reset 或伪恢复，保留诊断并纠正根因。仅此负向边界；不证明合法激活链、checkpoint/snapshot/replay/root 全链、非空日志后缀、新请求/模块行动与业务结算正向续跑或当前可服务。


## 5. 关键运行流程

以下保留原目标流程，并非当前源码全字段的顺序保证；§3 明列 advertised/network observation 的实际差异。

enabled节点读当前config→缺key可创建→严格map验证→normal signature→binding→source/world/order→apply→persist guards→publish observation。任何拒绝停止该记录不记成功；startup required recovery load失败blocked。disabled不创建身份，重复/stale不应用。

拒绝保留可定位原因；timeout/retry受原策略有界限制。并发冲突遵循原guard/precondition/共识authority，不能因接线或重连获得新权限。取消/替代只有外部专业合同明确支持时成立，本设计不新建取消或补偿schema。

## 6. 接口与数据合同

config path/keypair和optional validator_signer_public_keys由本地config提供；每validator normalized32-byte ed25519无遗漏/额外。proposal/attestation/commit同验签-before-binding。NodeReplicationNetworkHandle可注入world/topic route，FileReplicationRecord包含原版本ordering identity，不能增新schema或恢复旧aw topic/UDP authority。

| producer → consumer | 输入身份/版本 | 顺序与幂等 | success / error | 兼容与资源边界 |
| --- | --- | --- | --- | --- |
| config/network/DistFS → NodeRuntime | 原PRD列明identity/hash/version，不复制schema | 上述flow及DES验证顺序；local幂等不推导global exactly-once | 完成相应validation/apply/persist/publish后才声明该结果；失败不伪进展 | 原policy/defaults及旧data兼容；不加未知deadline/阈值 |
| local init／ensure → identity loader | 原 init identity、固定配置摘要、已发布公钥指纹；私钥仅在私有文件 | 同一 OS 锁下持久准备→原子发布→绑定；重试复用已发布身份 | 完整读回及父目录同步后成功；冲突／未知结果 blocked 或待恢复 | 目标；已有合法 key 保留，不自动轮换；不兼容的锁／文件系统拒绝 |

## 7. 状态、事务与持久化

以下为目标状态约束，当前观察顺序及持久化差距见 §3。identity missing/malformed/conflicting/unwritable可区分；非法已有key不replace。record validated不等于applied；applied不等于persisted；persist成功才observation。local/remote writer guard持久保护stale/duplicate。crash durability及node apply+persist原子边界须实际测试，不能借DistFS guard单测代签。

accepted、applied、persisted、published必须分别具备各自证据；workflow done/文档active不属于运行状态。外部receipt/journal/state root生命周期和recovery由runtime authority；本地观察不能提供更强保证。

身份事务的持久提交点是完整 key 的原子发布和父目录同步；可对外报告初始化完成还须 binding／complete 持久一致。多个文件的部分完成由[恢复表](#des-nir-bootstrap)解释，不依赖跨文件原子替换。OS 锁只防并发写，不代替 key 与目录 fsync；私有临时文件不是已发布身份，完整临时材料也不能在所属意图之外复用。

## 8. 部署、安全与运行约束

secret不进入docs/inventory/monitor/artifacts；node/session/consensus/governance identity分开。可用local key不提供KMS/HSM/rotation/revoke/admission。注入不证明libp2p/NAT/公网或当前triad health；role从current runtime status/evidence窗口读取。

当前 transport／consensus 的用途派生由 [signer_derivation.rs](../../../crates/oasis7_chain_runtime/src/host/signer_derivation.rs) 定义；不同域避免跨用途复用，但共用本机 root 仍有共同泄露风险，不能称为独立 custody。治理主体认证、实际成员采用及读取 grant 撤销分别由[安全治理设计](../blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-governance-authorization)、[成员应用](../blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-membership-application)和[读取授权](../blockchain/p2p-mainnet-security-governance-readiness.design.md#readiness-full-sync-read-authorization)拥有；node key／PeerId 成功生成不能替代任何一种授权。

设计只定义受控输入和失败条件；操作留在现行runbook。当前governed live网络raw-copy禁令和signed-V2边界只适用于该runbook环境，不否定历史/offline local事实。任何真实fleet/health/restore claim需固定inventory/manifest/package/config与同窗口证据，不从triad标签或本地成功推导。

## 9. 质量与容量

| 环境/刺激 | 预期响应 | 判定指标/阈值来源 | 验证入口 | 当前证明范围 |
| --- | --- | --- | --- | --- |
| 同冻结候选的local unit/fixture，逐DES负例与旧数据兼容 | 精确拒绝/no false mutation；正确路径满足原顺序 | 配对PRD常量/default/policy；未定义容量/latency不新增数字 | §11.1逐场景的source/手册 | source定义/计划，SN1未执行 |
| 多节点/恢复/服务角色组合，固定world/version/config/window | 同history/root或明确隔离/只读，不产生无证世界效果 | root与testing-manual S9A；root历史lag≤50、DistFS ratio≤0.1、mismatch0及insufficient_data阻断口径保留 | [testing-manual](../../../testing-manual.md) S9A/S9/S10；按原场景选择tier | 无真实规模、窗口或容量/SLA执行证明 |

## 10. 兼容、迁移与回滚

吸收MIG088/092/095/099历史只保留provenance。旧transport/topic不复活；未来接线更改必须runtime/network证据。已有guard/recovery数据按原合同读，错误blocked不能静默default；无自动deploy/restore/rollback，restart只诊断或暂时恢复且须根因修复。

本文保留原path/旧章节anchors与历史ID，不替换task truth；添加的接受/DES只细化旧合同。新输入、消费者或语义变更需重新绑定/专业审读。package/config回退不能复原已删chain state或撤回committed效果；破坏性操作依原runbook与runtime恢复合同。

已有合法 keypair 与其公钥／PeerId 必须原样保留；采用新初始化事务时只补可核对的意图／绑定，不重生成密钥。遗留半写、未知 lock 持有者或冲突 binding 不自动升级，先给出明确诊断。回退旧写者之前必须停止并发初始化；旧 create-new-lock/direct-write 实现不具备新 crash-safe 目标，不能以格式仍能读取宣称回退后保证不变。

## 11. 验证设计与可追溯性

下表是可复用计划：existing test source、planned场景与executed evidence分开。每条只消费准确upstream/DES；执行时须同可比较candidate/config/入口/环境，local fixture不升级为真实网络。未定位完整测试写planned，owner为runtime/QA；缺证阻断该组合结论，不能从required局部降低root full。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [professional_acceptance: nir-bootstrap](node-identity-replication-contract.prd.md#nir-bootstrap)；Trace refs: O7-REQ-b39ac7bb-609b-4784-9a94-7ef48ad4280e | [des-nir-bootstrap](#des-nir-bootstrap) | disabled 不建 key；合法 ensure/init 唯一发布；异常配置与冲突不覆盖身份；持锁进程退出不留下永久占用，存活锁不可被超时夺取；临时写入／发布／目录同步／绑定中断后复用原身份或明确 blocked。 | [testing-manual](../../../testing-manual.md) S4 required：现有 concurrent_first_writers_share_one_generated_keypair、secure_config_dir_creates_a_mode_restricted_complete_keypair、secure_config_dir_rejects_a_symlinked_key_file 仅是局部基线；planned 补同候选 enabled/disabled/invalid/path/read-only 入口矩阵，分别核对普通 private-only 补公钥与 secure 完整性；增加进程级 SIGKILL、并发、短写/fsync/rename 故障注入及目标平台实际文件系统重开。断电持久性另用可控文件系统故障场景，普通模拟不代签。 | 实施 PR 记录候选/config、命令和诊断；每个故障边界的原 init identity、公钥／PeerId、文件状态前后、锁结果、重试结果及一次正常完成对照；不输出私钥或 key bytes。 | 本机 key 的 Unix OS 锁、意图／候选／发布／完成恢复已有实现；正式 v2 新增双运行租约与不可覆盖域绑定。进程级场景见 node_keypair_config::transaction::tests、node_keypair_config::runtime_lease::tests 及 protected_runtime_identity_lease_excludes_duplicates_and_survives_outer_drop，实际候选与结果记实施 PR。完整初始化／绑定故障矩阵及真实断电／文件系统故障仍未验收；产品 N/A，限本机身份，不证明网络 admission、custody 或 world／fleet 恢复。 |
| [professional_acceptance: nir-signer-binding](node-identity-replication-contract.prd.md#nir-signer-binding) | [des-nir-signer](#des-nir-signer) | 配置map完整normalized32-byte，无unknown，signature先于binding，缺失/mismatch拒绝 | [testing-manual S4/S9A](../../../testing-manual.md); testing-manual S4/S9A (`../../../testing-manual.md`)；exact local test source `crates/oasis7_node/src/tests_hardening.rs`；existing config_rejects_duplicate_validator_signer_bindings、pos_engine_rejects_signed_mode_without_complete_validator_signer_bindings、pos_engine_rejects_signed_proposal_when_signer_binding_mismatches_validator；per-message完整矩阵planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；局部 Trace refs: O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3 |
| [professional_acceptance: nir-ingest-order](node-identity-replication-contract.prd.md#nir-ingest-order) | [des-nir-isolation](#des-nir-isolation) | network injection只路由；world/topic隔离不混状态 | [testing-manual S4/S9A](../../../testing-manual.md); testing-manual S4/S9A (`../../../testing-manual.md`)；exact local test source `crates/oasis7_node/src/tests/non_sequencer_followers.rs`；existing runtime_network_replication_respects_topic_isolation、replication_network_handle_rejects_empty_topic；local fixture，多world/source鉴权完整负例planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: nir-ingest-order](node-identity-replication-contract.prd.md#nir-ingest-order) | [des-nir-ingest](#des-nir-ingest) | apply/persist成功才peer/committed observation；failure不假推进 | [testing-manual S4/S9A](../../../testing-manual.md); testing-manual S4/S9A (`../../../testing-manual.md`)；exact local test source `crates/oasis7_node/src/tests_network_gap_sync_execution_failures.rs`；existing successor_probe_does_not_advance_replication_cursor_when_execution_fails、gap_sync_does_not_advance_replication_cursor_when_execution_fails、out_of_order_replication_ingest_does_not_advance_contiguous_persisted_cursor；这些测试断言本地 committed/persisted cursor，不完整断言全部 peer observation；persist/crash oracle 仍 planned。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: nir-ingest-order](node-identity-replication-contract.prd.md#nir-ingest-order) | [des-nir-guard](#des-nir-guard) | single-writer/order/duplicate/stale持久guard；hash/write失败不污染 | [testing-manual S4/S9A](../../../testing-manual.md); testing-manual S4/S9A (`../../../testing-manual.md`)；exact local test source `crates/oasis7_distfs/src/replication.rs`；existing apply_replication_record_hash_mismatch_does_not_mutate_guard、apply_replication_record_write_failure_does_not_mutate_guard、replay_replication_records_restores_files_in_order；DistFS局部不能证明node atomicity。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明 |
| [professional_acceptance: nir-recovery](node-identity-replication-contract.prd.md#nir-recovery) | [des-nir-recovery](#des-nir-recovery) | corrupt/unreadable PoS authoritative state blocks start；保留诊断纠根因 | [testing-manual S4/S9A](../../../testing-manual.md); testing-manual S4/S9A (`../../../testing-manual.md`)；exact local test source `crates/oasis7_node/src/tests_hardening.rs`；existing runtime_start_fails_when_pos_state_snapshot_is_corrupted；同config/state source重启拒绝，unreadable/persist故障完整矩阵planned；不是restore/full恢复。 | 当次GitHub task Issue evidence、实际命令/exit code/log/artifact；实际source/integration/tested tree/config/environment/window在执行时绑定 | local test source≠执行通过；全拓扑/恢复/finality/release未证明；局部 Trace refs: O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4 |
| [product_requirement: REQ-DCS-003; O7-REQ-103d7ad7-33f2-41b8-a741-a9c4b383b4c3](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-003) | [des-nir-signer](#des-nir-signer) | 启用 binding 时有效签名仍不能冒充受治理 validator | [testing-manual S4/S9A](../../../testing-manual.md); Node hardening source (`../../../crates/oasis7_node/src/tests_hardening.rs`)；tests_hardening.rs 中 duplicate map / signed proposal mismatch / incomplete map / local signer mismatch 定义；proposal、attestation、commit 各签名有效后 missing/mismatch 及有效 binding 顺序矩阵仍 planned；同候选/config 的隔离 S4/S9A 定向入口，产品完整 AC 仍 test_tier_full | 实际命令/结果/产物由执行 task 绑定；本轮静态定义核对、未执行 | 不证明 map 未启用部署、registry/轮换、网络暴露、BFT 或全部角色隔离 |
| [product_requirement: REQ-DCS-002; O7-REQ-3f7d612d-5a6a-4d6d-bb0e-722c44d986f4](../../product/world-infrastructure/distributed-consensus-and-state-availability.prd.md#req-dcs-002) | [des-nir-recovery](#des-nir-recovery) | corrupt/unreadable PoS authoritative state 阻断启动且不默认重置 | [testing-manual S4/S9A](../../../testing-manual.md); Node hardening source (`../../../crates/oasis7_node/src/tests_hardening.rs`)；tests_hardening.rs::runtime_start_fails_when_pos_state_snapshot_is_corrupted 写入 not-json 后断言 start Err 且 running false；unreadable 权限及完整持久故障仍 planned；同候选/config 的隔离 S4/S9A 定向入口，产品完整 AC 仍 test_tier_full | 实际命令/结果/产物由执行 task 绑定；本轮静态定义核对、未执行 | 不证明全恢复链、signed-V2、正向新请求/模块行动/结算续跑或可服务 |

## 12. 决策、长期风险与未决问题

选择保留现有专业合同并精确分配，拒绝以导航/历史标签免除真实设计，也拒绝将local证明升级为full。可选map未启用部署不能被说全节点binding；persist-failure/crash atomicity尚需node oracle；ops/root全链恢复外部。

runtime/P2P负责缺失技术/实现；ops负责环境事实；QA负责计划覆盖和verdict；producer审读产品含义。新算法/schema、权限/默认值变化、额外消费者或claim扩展触发重新评审；未解除项保留在GitHub正式evidence及剩余required set，不能靠未来Issue占位关闭。下列原章节的里程碑/审计轮次是保留provenance，不维护可变进度。



- PRD: `doc/p2p/node/node-identity-replication-contract.prd.md`
- Project record: GitHub Issue / GitHub Project

## Design position

The design joins only the node-side contracts that must agree: local identity
bootstrap, optional validator signer binding, signed replication, persistence,
and fail-closed recovery. It consumes runtime-published status rather than
creating a second consensus, reachability, or topology truth.

## Flow

This retained flow states the target contract. The pinned source observes remote network heads before apply; see §3 for the unresolved observation-field scope. It is not a claim of current apply/persist ordering for every status field.

```text
config validation/bootstrap -> node identity + optional validator binding
  -> signature/source validation -> apply + persist guard state
  -> publish observed replication progress/status
```

Each arrow is ordered. In particular, unsuccessful validation or persistence
does not advance observed peer or committed progress. Startup load failure is a
blocked start, not a default-state recovery.

## Components and constraints

| Component | Current responsibility | Constraint |
| --- | --- | --- |
| Config bootstrap | Load a usable local node keypair on the verified current path | Explicit error for malformed/unwritable config; no silent replacement of invalid identity. |
| Signer binding | Validate normalized validator-to-ed25519 public-key mapping when configured | Missing/mismatched signing key rejects the message; governance/admission remains external. |
| Replication handle | Inject the node replication network and isolate its world/topic route | Injection is not a deployed libp2p or public-reachability claim. |
| Replication ingest | Verify source/signature, apply record, persist ordering guards, then observe progress | Invalid, stale, duplicate, or failed records cannot update status as success. |
| Recovery | Load required node/PoS and replication state | Corruption is surfaced and blocks the applicable start; restarting does not repair root cause. |
| Observability | Expose failure/progress to the runtime status and operations evidence path | The triad monitor only samples/projections; it does not redefine runtime truth. |

## Security and operating boundary

Node identity, transport/session identity, consensus signer, and governance
signer remain distinct. Local config generation supplies none of the custody,
rotation, revocation, registry, or ceremony guarantees required for production.
No private signer material is emitted to documentation or operational artifacts.

Historical UDP and `aw.*` transport wording is deliberately excluded. Likewise,
this design adds no automatic deploy/restart/rollback/restore/state-sync path;
such operations require separately authorized environment evidence.

## Evolution

Future transport or topology work may update this contract only with current
runtime evidence and the network authority. It must preserve apply-before-
observe ordering, explicit recovery failure, and the separation between a local
bootstrap key and production signer/governance truth.
