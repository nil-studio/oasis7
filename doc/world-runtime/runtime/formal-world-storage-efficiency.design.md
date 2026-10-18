# 正式世界历史的无损存储优化

- 状态：active；表示本 PR 候选的目标合同，不表示相关优化已经实现、启用或验收。
- Owner：runtime_engineer；共同审读：blockchain_ops_engineer、wasm_platform_engineer、qa_engineer。
- 适用范围：正式持久世界的端点、执行 attempt、历史依赖闭包、存储表示、迁移与恢复；覆盖 Primary/Replica 各独立故障域。
- 文档基线：[4220622fde069346a828d42572eea61c0073d95e](https://github.com/nil-studio/oasis7/commit/4220622fde069346a828d42572eea61c0073d95e)；静态源码审读基线为 `2676c835cc50f12d7fd16a762eaa21f026ae79d4`，期间变更只涉及文档，相关存储源码未变。下文源码定位统一固定到文档基线；该初始设计审读没有实际世界存储样本、运行验收或节省比例，后续实现与证据范围见 §10.2 及实施 PR。
- 产品来源：[无损世界历史存储 PRD](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)。
- 专业配套：[既有 storage 治理](runtime-storage-footprint-governance.design.md#storage-history-dependency)、[同世界恢复](../design.md#runtime-recovery-design)、[事件后缀控制状态恢复](../design.md#runtime-journal-suffix-recovery-design)。
- 验证操作：[无损存储验收手册](formal-world-storage-efficiency.manual.md#lws-validation-use)；步骤是目标合同，实际入口、候选和结果须由实现交付记录。

## 1. 问题、目标与非目标

当前正式提交对相同候选材料进行多阶段整体封装，快照内的模块和状态跨高度重复，执行 attempt 还有独立输出副本。冷热归档已减少部分热集合，却没有消除这些物理重复。目标是在同一世界、同一完整历史与相同恢复能力下，降低实际持久占用、增长斜率和写入量，并量化读取与恢复成本。

本设计按两个层次实施。首先改变磁盘表示，读取后仍得到原始 canonical bytes、hash、签名和状态。随后只有在完整恢复证据通过时，才演进 checkpoint/delta 与索引的逻辑 schema。两层有独立版本、启用条件和回滚边界。

成功必须同时满足：原历史及依赖全部可解析；恢复后能处理新的合法工作；保留原外部结果和幂等边界；每个必要故障域具备合格持久材料；物理用量测量可复核。压缩比、latest 恢复或新格式写入成功都不能单独满足成功条件。

非目标：不缩短产品历史承诺，不以 TTL、最近 N 高度或磁盘压力删除正式历史，不减少独立故障域副本，不重置世界、余额、身份或 allocator，不重发历史外部效果，不改变现有权威、收费、共识或世界状态根定义。诊断日志的单独保留政策不能授权删除恢复唯一依赖。

## 2. 上游约束与相关角色

产品 PRD 拥有完整性、可用性和用户可感知结果。Runtime 负责状态与续跑语义；formal/DistFS 负责逻辑对象、闭包、字节读写与持久事务；P2P/ops 负责正式权威、独立故障域、成员切换与服务资格；WASM owner 负责历史模块解释与调度恢复；QA 独立核对同一候选和环境中的容量、失败与恢复证据。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | 所有正式历史切面及原依赖内容可恢复，旧权威链按原身份验证 | [完整闭包](formal-world-storage-efficiency.design.md#des-lws-001) | formal 历史读取、Runtime、WASM；原权威与历史格式 | 不用摘要、当前配置或重新推理补缺失原文 |
| Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c；[REQ-LWS-002](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-002) | 物理编码、引用和重排还原精确原字节，逻辑身份不变 | [物理合同](formal-world-storage-efficiency.design.md#des-lws-002) | DistFS、所有读者；旧 bytes/hash/schema | 不把逻辑 schema 改写算成透明压缩 |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | 本地冷热与 checkpoint 密度只调整读取成本，完整历史仍可获取 | [冷热及快照](formal-world-storage-efficiency.design.md#des-lws-003) | archive sources、历史 module/manifest、恢复读者 | 不要求每个端都热驻全部 payload；不承诺已支持无限规模同步 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | 只回收已经验证可替代且不被未决/读者/回滚引用的表示与临时材料 | [GC 与 attempt](formal-world-storage-efficiency.design.md#des-lws-004) | authoritative roots、reader pins、endpoint guard | unknown、Frozen、未验证闭包不按时间删除 |
| Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005) | 各必要故障域独立持久、授权与 fencing 保持；归档可读不产生服务资格 | [部署与权威](formal-world-storage-efficiency.design.md#des-lws-005) | P2P/ops durable-set、受保护 signer/guard | 不跨 P/R 合并必要副本或凭物理路径授予写权限 |
| Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0；[REQ-LWS-006](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-006) | 迁移、失败和软件回滚保留当前已接受历史及原请求连续性 | [迁移回滚](formal-world-storage-efficiency.design.md#des-lws-006) | Runtime、ops；reader capability、原提交判定 | 不因回退二进制回到更旧世界或擦除新格式写入 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | 回收后恢复业务、J1/J2、队列与原外部结果，继续相同的新工作 | [完整增量及续跑](formal-world-storage-efficiency.design.md#des-lws-007) | allocator/tick producer、WASM、effect ledger、QA | root 相等、snapshot 往返不能代签非空后缀续跑 |
| Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008) | 写、读、迁移和恢复预算不足有可判定结果，保留原请求/源数据 | [预算与退化](formal-world-storage-efficiency.design.md#des-lws-008) | 所有消费者、ops 配额与容量 | 不扩大现有预算掩盖全量物化，也不靠清历史解压 |
| Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916；[REQ-LWS-009](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-009) | 容量、写入、内存、恢复查询成本和完整性使用同窗口可复核证据 | [质量容量](formal-world-storage-efficiency.design.md#des-lws-009) | QA、ops、metrics；实际文件系统和负载 | 没有样本不报告节省百分比；文件长度不冒充 allocated bytes |

### 2.2 适用范围与文档关系

本专题直接承接产品要求，不使用产品 N/A。既有 storage 的 profile、热窗、generation 原子性和本地诊断仍适用；其历史 retained-window 语句不得缩小本专题正式世界的完整历史要求。正式历史 GC 只有通过本文闭包规则才能启用。既有恢复、权威和 J1/J2 的失败边界继续适用，本文不把这些依赖的目标实现状态改成已验收。

<a id="storage-efficiency-current-gap"></a>
## 3. 当前状态、目标状态与差距

本节“当前”表示固定源码中可核实的实现；“目标”表示待实施合同。实现存在不代表已在部署环境验收。现有代码、旧设计和本次目标分别记录，不能把新 schema 名称写成现行接口。

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| 正式阶段记录 | Frozen/Prepared/Decided/Ready/Qualified 内嵌完整 material/candidate；每次 store.put 编码整份 entry | 阶段外壳与共享 candidate/artifact 物理分离，读出原 entry 字节 | 不同阶段整对象 hash 去重不了共有大 payload | [状态类型](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/endpoint_types.rs#L5)、[写入](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/endpoint.rs#L400) |
| 独立 artifacts | candidate 与 frozen 中相同 hash 去重后另存，业务节点另存 pack | 同域所有表示可共享逻辑 bytes；原阶段回执保持 | 独立 CAS 已有，但阶段外壳仍有内嵌副本 | [落盘路径](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/endpoint_business.rs#L86) |
| 压缩 | 通用 LocalCasStore 有 zstd level 3；正式 Store::put 是 raw CBOR，business pack 保存原字节 | 正式逻辑对象也支持有版本的压缩/分块 recipe | 不能把通用 CAS 的压缩能力视为已覆盖 formal | [通用 codec](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/lib.rs#L218)、[formal Store](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/endpoint_store.rs#L278) |
| 快照与模块 | Snapshot 内嵌 module_artifact_bytes；正式包还单独放 WASM object；每次新增完整 after snapshot/journal | 先共享精确字节分块，再以独立逻辑 gate 评估完整 delta | 微小状态变化仍生成不同整快照，内含历史与静态数据 | [Snapshot](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7/src/runtime/snapshot.rs#L63)、[candidate](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_package.rs#L580) |
| attempt | 每次正式执行随机新目录和独立 execution_store；执行函数读结果后返回，未定义成功 attempt 回收合同 | 已验证终态回收重复输出；未决材料受保护 | 源码审读不等于证明现场目录已无限增长 | [执行目录](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_executor.rs#L107)、[输出 CAS](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/driver.rs#L754) |
| 认证冷热历史 | 已归档 Journal、WorldService result、关闭 nonce/receipt/context、terminal intent、Tick consensus；GameGoalPublish/GameExecutorControl 尚保热 | 保留同一完整逻辑历史，扩大物理共享和可验证回收 | 热集合预算不表示总物理存储受限；控制链冷索引未完成 | [计划及预算](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7/src/runtime/world/formal_retention.rs#L31)、[控制链保护](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7/src/runtime/world/formal_retention_staging.rs#L227) |
| Merkle 节点 | sparse tree 深度 256；逐 update 生成路径并输出 pending 全集；v1 验证重建后全结构相等 | v1 物理压缩；v2 可只输出最终新可达节点 | 不能静默过滤 v1 writes，否则历史验证失败 | [build/verify](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/business_index.rs#L345)、[生成路径](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/business_index.rs#L608) |
| pack 与度量 | business pack 为多个逻辑 hash 建 hardlink；每索引项 44 B | 同 inode 只计一次实际占用，分开报告逻辑 bytes | 按路径累加 stat.len 会高估，同盘 P/R 也不证明独立域 | [pack](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_distfs/src/controlled_authority/formal/business_pack.rs#L1) |
| 大历史读取 | 已有分段/有界历史处理、按对象读取与范围读取；仍有整份 material/离线 envelope 预算 | 跨 pack、归档、迁移的完整闭包增量读取可续传 | 局部分页不证明全链无界规模；全量流式闭环待实测 | [历史读取](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/formal_live_history_replay.rs#L247)、[离线 envelope](https://github.com/nil-studio/oasis7/blob/4220622fde069346a828d42572eea61c0073d95e/crates/oasis7_chain_runtime/src/bin/oasis7_chain_runtime/execution_bridge/formal_writer_published.rs#L11) |

当前正式热预算为：WorldService results、已提交 capability nonce 闭包、终态 AgentIntent 各 32 条/256 KiB；Journal 256 条/512 KiB。16 MiB 是单次归档计划估算预算；formal Store 的 64 MiB 是单对象上限。它们均不是世界历史总量上限。普通正式 commit 也不等于每次刷新完整 JSON world 镜像，不能用旧镜像路径推导本轮主要收益。

## 4. 边界与结构

### 4.1 组件职责

| 组件 | 产生或拥有的事实 | 允许的动作 | 拒绝或隔离边界 |
| --- | --- | --- | --- |
| Formal authority / endpoint guard | qualified head、Frozen、decision、seal、durable-set、单调状态 | 接受原协议转换与恢复判定 | 物理索引、压缩 worker、archive source 不能授予权威 |
| Logical object reader/writer | 原 schema 的精确 bytes、逻辑长度和 hash | 原 typed decode/verify；通过统一读取层取回内容 | 未知 codec、缺依赖、hash 不符不回退到其他世界/较新缓存 |
| Physical object store | pack/frame/recipe、物理位置与 generation | 同域共享、压缩、重排、受控增量读取 | 位置或物理 checksum 不代替逻辑 hash 与签名 |
| Root inventory / GC | 已验证根集合及依赖遍历、reader/rollback/attempt pins | 生成保留证明和可回收表示清单 | 未知对象类型或缺边时停止 sweep |
| Compaction / migration worker | 新表示、迁移进度和切换记录 | 在授权存储根下构建新 generation | 不提交业务、不修改旧 signer/head、不绕过 fencing |
| Attempt lifecycle manager | 原执行身份、候选/终态对应与持有关系 | 条件满足后回收可替代本地材料 | 超时、目录 mtime、仅本地 success 均不足以删除 |
| Archive/transfer adapter | 原逻辑对象的可获取性与传输进度 | 由已绑定根选择对象、校验及续传 | 可下载不等于 canonical 或 qualified |
| Metrics/QA | 同窗口容量、成本、完整性和失败证据 | 只读度量、声明实际覆盖 | 统计失败不得返回 0 并宣称节省或回收成功 |

<a id="des-lws-001"></a>
### 4.2 完整历史及依赖闭包

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；承接 [REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)。

完整性以正式世界身份、历史位置及原 schema 为边界。保留每个已接受切面的原状态内容或可精确重建内容，输入、事件、结果、证明以及解释它们所需的版本。hash、摘要、首尾高度或索引存在只能证明定位或一致性约束，不能替代 payload。生成模型的新回答、当前规则、当前 WASM 缓存也不能替代历史原材料。

根集合至少覆盖：初始激活；当前及历史 qualified 链的可达性根；原 authority/epoch/durable-set/seal 与合法恢复、分支证据；尚未判定终态的 Frozen/Prepared/Decided/Ready；受保护 checkpoint、rollback-safe generation、迁移源/目标；活动 reader/传输/恢复 pin；仍需继续或核对的 attempt。可以从经过验证的历史索引根遍历所有祖先，不要求机械地把每个高度复制为一份根表。

遍历必须识别实际 typed 对象的引用边，包括 ClosedRecord 引用、snapshot 中嵌入或声明的历史模块、原 native 执行工件及环境依赖、业务与保留历史树、proof/certificate、recipe/base/dictionary/frame、外部结果、nonce 与 effect 关联。不能只沿普通 artifact.references 或当前 latest snapshot 扫描。未知 schema、缺失引用、非闭合 lineage、原始 hash 不符都意味着闭包未验证，禁止相应回收。

### 4.3 数据分类与生命周期清单

| 数据类 | 权威来源与必要内容 | 热/冷目标 | 允许释放的内容 |
| --- | --- | --- | --- |
| 正式提交与原输入 | CanonicalRecord、Frozen 输入/上下文、原结果和历史关联 | 小索引热；原 payload 冷可读 | 仅相同逻辑 bytes 的多余物理表示 |
| 决策与历史控制链 | 各阶段回执、qualified proof、activation、seal、epoch/durable-set 转换 | 活跃/未决热；终态证明完整归档 | 被明确证明无任何根需要的旧外壳表示 |
| 当前执行状态 | 资产、资源、控制权、权限、nonce、队列、模块状态和版本 | 活跃状态热，原切面冷 | 已有精确重建路径的快照物理重复；不能删唯一字段 |
| 终态业务历史 | Journal、Tick、result、intent、工业/结算/治理等历史原文 | 已支持类别继续认证归档；其他类别逐项扩展 | 活跃引用解除后热副本；冷原文与检索完整性保持 |
| 模块、原生工件与规则 | 原 WASM bytes；历史 native executable 原字节/digest、target/platform/features；manifest、schema、policy、ABI 及必要版本/运行依赖 | 同域按精确内容共享，明确可解释/可执行环境 | 未被任何历史、未决、回滚根引用的对象；版本名或 digest 不替代工件内容 |
| 外部效果 | 原请求身份、原响应、dispatch/receipt、幂等及 unknown 状态 | 活跃 outbox 热，终态账本冷可读 | 能由保留账本重建的重复副本；不得以重发补数据 |
| 存储索引与 codec | 原逻辑定位、frame/recipe/base/dictionary、旧 reader 所需信息 | 工作集热，旧 generation 由 pin 保护 | 所有受保护读者解除后、替代表示验证完毕的旧索引/pack |
| attempt/缓存/诊断 | 执行身份、原候选、提交判定与恢复依赖 | 活跃受保护；终态保留必要摘要 | 满足 §7 的重复输出及可重建缓存，唯一证据不得删除 |

## 5. 关键运行流程

### 5.1 正式提交接入物理共享

1. 按原 world/authority/Frozen/position/candidate 身份执行与验证，保留原签名输入和状态机判定。
2. 逻辑层产生原 canonical bytes 与逻辑 hash；存储层按内容查找已验证表示，不复用未绑定的较新世界缓存。
3. 新内容写入当前 generation 的不可变对象或 overlay，校验并持久化；阶段记录引用共享内容的物理 recipe。
4. 重建完整原阶段 entry，逐字节及逻辑 hash 核对后，才能进入原端点持久化/回执边界。
5. P/R 各自完成必要内容及原协议回执。一次共享命中不等于另一故障域已保存。
6. 成功后只登记可审计的存储结果；旧表示回收异步进行。回收失败不反转已接受的世界提交。
7. 超时或回执丢失按原请求查询同一候选；不另造 nonce、position 或世界。决定不明时返回 unknown 并保持全部相关 pin。

### 5.2 历史读取与恢复

读者先获得确切 qualified/历史切面及所需服务资格，固定逻辑根和物理 generation pin，再有界遍历依赖、读取或续传原逻辑内容。每层校验版本、长度、hash、历史边界和预算。原凭据过期或被 fence 时停止授权操作；已经拿到的物理 bytes 不自动延长权限。

完整 snapshot 恢复、事件后缀恢复和 committed-input 重执行是三个不同验证路径。选择哪条路径必须由保存材料和恢复合同决定，不因缓存缺失自动改为另一条。外部响应从原记录注入；历史回放不能调用外部 dispatcher、重新请求 LLM 或再次结算费用。

读取失败保留可续传的逻辑根、已验证进度和原请求身份。只有完整候选通过同世界、版本、finality/append/head 及续跑门槛，才可由原 authority 开放服务。没有完整历史材料时显式隔离或保留受限读取，不能声称从 hash 元数据恢复成功。

### 5.3 维护与业务并发

压缩 worker 在固定 generation/根快照上构建新表示，业务仍向当前 overlay 追加。切换时进入现有单 writer 受控边界，纳入快照之后的新根、新写入和未决 pin；无法闭合本次差集就推迟切换。取消维护只删除已经确认未发布、无人引用的 staging，保留原业务状态和操作身份。

## 6. 接口与数据合同

以下名称和 schema 是供实现审读的目标概念，不是现有公开 API、CLI 参数或已发布 DTO。字段必须经实现 PR 固定编码与兼容测试；任何新逻辑 schema 都不得借物理布局版本隐式启用。

<a id="des-lws-002"></a>
### 6.1 原逻辑身份与物理表示

Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c；承接 [REQ-LWS-002](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-002)。

~~~text
LogicalObject:
  original_schema, logical_hash, logical_length, original_bytes

PhysicalRepresentationV1:
  representation_version, logical_hash, logical_length
  codec, codec_version, stored_length, stored_checksum
  frames_or_recipe_ref, optional_dictionary_ref, resource_limits_revision

ByteRecipeV1:
  logical_hash, logical_length, ordered_parts
  part = literal_bytes | exact_slice(logical_object_hash, offset, length)
  optional_exact_embedding_adapter = explicitly_versioned_legacy_encoder

StorageGenerationV1:
  generation_id, parent_generation, object_index_ref
  supported_representation_versions, root_inventory_ref, activation_record_ref
~~~

逻辑 hash 始终按原 canonical bytes 计算；若原协议使用带 domain 的 typed digest，继续按原协议计算，不与物理对象 BLAKE3 标识互换。物理 checksum 保护存储/传输表示，不能取代原逻辑验证。重建 bytes 必须通过原 typed decode、hash 与签名校验。

第一实现优先用精确字节切片共享整个 candidate 及重复包片段。若要把嵌在 CBOR 整数数组中的 artifact 还原为引用，必须有冻结版本的 exact embedding adapter，证明所有原字节完整重建；不能靠新版 serde 重序列化、丢弃未知字段、转换整数数组为 byte string 或重新排序字段。不能证明时继续保存原字节分块。

recipe 依赖必须无环、限制深度/分片数/累计输出量，所有依赖都进入闭包。物理索引仅是 locator，不能据其自报 logical_hash 判定可信。没有对应 schema/codec 能力时应明确拒绝，不把解码失败当作“这是旧 raw 文件”继续猜测。

### 6.1.1 新世界的首个物理表示版本

无运行世界的全新创建直接启用物理表示 v1，不等待现场迁移。原逻辑对象名及 canonical bytes 保持；新表示用 `.representation-v1/<logical_hash>` 的显式选择器定位，不额外创建重复的根目录 digest 链接，绝不只凭 payload magic 猜 codec。若兼容对象同时具有选择器及根目录链接，两者必须指向同一 inode，否则拒绝歧义表示。旧 raw/pack 没有选择器时仍由旧读者路径验证。

物理 recipe 固定有界内容分块：最小 4 KiB、平均约 16 KiB、最大 64 KiB，使用固定字节散列滚动规则；每个 frame 独立选择 raw 或 zstd level 3，仅实际更小时压缩。frame 按存储内容 checksum 命名，同时声明解码长度及原 frame hash。共享 recipe index 为一批逻辑对象保存精确原字节切片，不重序列化嵌入的 CBOR。单对象保持 64 MiB、整批逻辑 bytes 保持 64 MiB、描述文件上限 16 MiB；完整对象返回前重新核对原逻辑 hash。range 只读取交叠 frame，保持既有未认证传输分片合同，接收端仍须完整校验对象。

文件及目录 fsync 后才能发布引用；新 selector 出现前，新物理文件不构成正式对象。物理 frame、recipe 依赖计入完整闭包和保留理由；不能把 selector 或 frame 当新的业务/服务权威。没有部署节点时，本地故障测试不证明 S6 独立故障域验收。

frame/recipe 先写私有暂存文件并 fsync，再原子发布且不覆盖既有对象；遇相同名称时比较原 bytes。recipe 外壳及内部 CBOR 均校验规范编码和 checksum，索引采用 32-byte digest 的紧凑 tuple，避免把二进制展开为整数数组。读取检查受控目录、最终文件的类型、owner 和写权限并使用 `O_NOFOLLOW`；这不声称抵御已获同一用户权限的并发恶意修改。 解析缓存最多保留 32 项、16 MiB 计费内存；每次仍重新读取当前完整 recipe 并计算内容 hash，只有同一完整内容才能复用已通过规范编码及覆盖检查的解析结果。缓存不信任路径、inode 或 mtime，不缓存 frame 原字节，也不替代本轮耐久同步或原逻辑 hash 校验。

稳定 `.physical-guard-v1` 负责退休及代选择：reader 和不可变追加持 shared，GC/selector 切换持 exclusive。另一个稳定 `.physical-publication-guard-v1` 串行 CAS、selector 批次补全、raw/pack fallback 及 retained ledger 追加；固定取锁顺序为 root→publication，反序释放，reader 不取 publication。跨页闭包/native 读回持 root shared，不能阻挡后续合法追加。已启用目录缺任一 guard 时拒绝重建，不能通过替换锁 inode 绕过存活 reader。

### 6.2 阶段外壳、正式 pack 与范围读取

目标阶段存储记录只持有原阶段元数据片段、共享 candidate/frozen 的逻辑对象引用及 exact recipe。Frozen、prepare/decision/proof 的原回执内容与每次状态转换均保留；重建后的整个 FormalRequestState 仍与原 v1 bytes 相同。目标不是把现有 FormalRequestState 字段直接改成引用。

正式 raw object、business pack、closed artifact 和嵌套 recipe 都通过同一逻辑读取能力访问。迁移清单必须覆盖 Store::get、raw_object、BusinessIndexNodeReader、object_range/chunk、archive source/import/export、native restore、durable-set copy、writer 离线/published reader、checkpoint/repair 和缓存校验；任何仍把物理文件当原 bytes 读取的消费者都阻止 codec 启用。

压缩以独立 frame 支持小范围读取，起始实现可评估 zstd level 3、64 KiB 逻辑 frame；只有算入 header、frame 和 index 后实际更小时才选择压缩。最大 pack、逻辑对象和解码窗口受 §8 全链预算约束，不通过扩大现有上限解除不兼容。dictionary 必须不可变、版本化、可随闭包恢复；缺 dictionary 视为缺数据。

range 的 offset、length、total_length、逻辑 hash 和 EOF 均指解码后的原对象；不得暴露压缩偏移或把部分 frame checksum 当成全对象认证。原协议若返回未认证传输分片，继续由完整接收者校验原对象 hash；若读者要求 verified 结果，应完成全对象验证，或使用绑定同一逻辑 hash 与不可变物理代的已验证缓存。

中间 range 可以跨 frame/pack，错误应区分越界、依赖缺失、解码失败、hash mismatch、预算不足与取消。校验缓存不能跨 logical_hash、codec/recipe、物理 generation 或权限边界借用。raw payload 可能碰巧以 codec magic 开头；格式选择必须来自有界、明确版本的表示描述及迁移选择器，不能仅凭前缀误判。

### 6.3 跨组件接口

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| PutLogical / GetLogical | formal/Runtime → physical store | 原 schema、logical hash/length、表示版本 | 同 hash 写入必须原字节一致；不覆盖不同内容 | 已持久表示或具体失败；命中需校验 | raw、旧 pack、新 codec 混合读；旧逻辑不变 |
| ReadLogicalRange | archive/restore → logical reader | 原对象 hash、逻辑 offset/limit、generation pin | 断点绑定原根/对象；同范围重试 | 原分片语义或分层错误 | 所有消费者共用解码后坐标 |
| Prepare/ActivateGeneration | compactor → storage owner | old/new generation、根清单、overlay 截止点 | expected-generation 比较；取消/重试可识别 | NotCommitted/Committed/Unknown | 不发新的世界 commit 或 serving 资格 |
| Acquire/ReleaseReadPin | reader/transfer → pin registry | 世界/逻辑根、generation、operation 身份 | 与根/引用发布及 retire fence 共用 guard 或等价 epoch；先 pin 后读 | 有效 pin、fenced 重试、取消、失效或未知 | 不按文件 mtime 推导无读者，不给待删表示新增成功 pin |
| VerifyClosure / PlanReclaim | GC → authority readers/physical store | 原 authority/qualified 根、typed schema、对象集合 | 根发生变化重核差集；删除前再验 pin | 完整可替代、blocked 或 unknown | 未知旧类型不跳过，旧 raw 可继续保留 |
| ResumeTransfer | restore/migration → archive | 原切面、已验证对象/范围、消费者能力 | 校验连续性后续传原操作 | progress、complete、retryable、isolated | 有界处理不要求整历史装入一个 envelope |

<a id="des-lws-003"></a>
### 6.4 冷热、快照与模块共享

Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；承接 [REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003)。

热窗仅控制本地工作集。超窗的原数据及按原身份访问的路径仍保留在合格持久层。旧证明、旧回执、旧 schema、原 authority/epoch 的冷读取应继续可验证；历史只读访问不得恢复旧 writer 的当前写权限。一个 archive URL 或 hash 列表不构成归档完成，需从目的地读回并验证实际内容与闭包。

冷查询还必须满足后续新请求的前驱判断：目标绑定、控制权、授权、去重与原请求关联不能因前驱下冷而误判不存在或重复执行。迁移某类热集合前，须验证既有消费者在冷命中、暂不可读和超预算时分别继续、受阻或拒绝的准确语义。

GameGoalPublish 与 GameExecutorControl 按原任务、版本链与关联身份建立认证冷索引及前驱查询。原仅保存两条创建候选的索引维持原语义，新增独立的 `game-task-control-chain-v1` 命名空间承接完整控制链；不通过扩大或覆写旧索引改变已签原结果。跨窗口恢复后须继续核对原目标和执行者前驱，不能把查询失败解释为不存在。

任务控制冷索引按原 owner/history scope 保存认证 head/count，每个不可变 link 绑定原 result reference 和前驱 link。创建、目标发布、执行控制三类原回执均可沿完整链查询。一次查询最多遍历 4,096 links；超过预算返回 Unavailable，缺 link、环、count 或原 scope 不符返回 Invalid，保留历史并阻止误判前驱不存在。更早的原 result 仍可按身份直接读取；查询预算不授予删除历史的权限。

近期对完整 snapshot 和 module bytes 做 byte-preserving chunk recipe，共享相同内容，重建原 snapshot 再计算原 state root。可比较固定块与内容定义分块；实现前冻结 chunker 参数与限制，按真实变更样本选择，不能假定前缀插入后固定块仍有高命中。小对象保留直接表示，避免分块元数据反而放大。

随后按占用贡献检查未纳入现行冷热归档的历史集合，例如工业/物流/结算回执、验证尝试、治理及发行记录。对每类先列出读者、幂等和资产依赖，再增加认证冷查询与完整性验证，最后移除热副本。迁走历史内容不能删除当前余额、仍有效的权限、nonce 判断或未决业务。

<a id="des-lws-007"></a>
### 6.5 未来 checkpoint 与完整 delta 的独立 gate

Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；承接 [REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007)。

逻辑增量是后续阶段。普通 WorldJournal、输入日志或事件数量不能直接作为完整状态 delta。启用前必须逐字段证明 checkpoint C 加真实非空增量到 T 能重建相同业务状态、控制状态、原字节/根及下一次执行。既有 [J1/J2 合同](../design.md#runtime-journal-suffix-recovery-design)、[兼容矩阵](../design.md#runtime-journal-suffix-compatibility)及[独立验证设计](../design.md#runtime-journal-suffix-validation)是前置依赖。

| 必需材料 | 完整 delta 的最低义务 | 拒绝的替代推导 |
| --- | --- | --- |
| 业务状态 | 所有受支持状态集合的创建、更新、删除、余额/资源/授权与提交边界 | 仅保存“主要业务事件”或只对比展示投影 |
| J1 allocator | action/event/intent/proposal 等实际需要的序列、era、分配/预分配 provenance、队列边界和消费关系 | 从最大事件 ID、事件数或 serde 默认值猜 cursor |
| J2 模块调度 | 稳定 instance 的最终绝对 wake tick 或明确 clear，Pure/无新 module state 也记录 | 从 module state 是否变化猜是否唤醒/挂起 |
| Pending/control | 入队、消费、取消、inflight、重试/幂等身份，原边界的前后关联 | 恢复时清空队列、重新排期、赋新 nonce |
| 模块/规则版本 | 原 WASM/native executable bytes 与 digest、target/platform/features、manifest、policy、schema/ABI、binding 及必要原版本环境依赖 | 使用部署机当前缓存、新二进制或单独版本名解释旧结果 |
| 非确定性与外部结果 | 原随机/时间等实际依赖、已物化原结果、dispatch/receipt/unknown 账本 | 重新请求 LLM、重发效果或重算收费 |
| 证明与控制链 | 原 CommitRef、Frozen、history link、authority/epoch/seal 及合法恢复语义 | root 相等就补授服务或重签历史 |

增量 schema 必须明确 producer、每条记录版本、顺序、缺失判定、字段覆盖和上限。C 必须具备完整控制基线；legacy 区间缺记录时保留足够历史 snapshot，或使用指明原材料的显式 adapter。材料不足就阻止该区间 snapshot 释放，不以“以后可以重放”代替证据。

先验证精确恢复，再用同一新输入进行对照续跑：新配方/结算不复用旧 ID，rollover 后 era 一致，只有到期模块在预期 tick 执行，原外部效果不重复派发，下一合法效果仍可执行。J1 与 J2 分别出证据，一项通过不覆盖另一项。限制增量链深度和恢复预算；checkpoint 密度可改变，但每个历史 T 均须有已验证完整路径。

恢复入口区分三种材料：已有持久候选时继续精确原候选，不重跑 execution；只有原 Frozen 时，必须先具备原输入、已物化非确定性结果、执行版本及原 authority 许可，才能沿现有恢复合同继续同一预留；外部响应丢失时，按原 effect/幂等身份查询交付事实。无法取回原结果或外部系统不支持可判定的原请求核对时保持 blocked/unknown，不通过再次调用补造历史，也不宣称外部物理恰好一次。

### 6.5.1 新世界的完整快照增量编码

完整增量的第一个版本按 Snapshot 顶层字段保存原始 CBOR value 切片，携带完整且有序的字段清单、精确 base/target 原字节 hash；不从业务事件、最大 ID 或默认值推断任何字段。allocator、era、预分配、pending/inflight、模块 wake/clear、模块状态和原外部结果都作为原字段的一部分进入完整覆盖。字段集合变化、缺失、重复、顺序错误及 hash 不符均拒绝应用。

Snapshot artifact 使用明确版本的 `oasis7.runtime.snapshot-delta.v1\0` envelope，绑定 base artifact reference 和声明的链深度。普通新 record schema 3、durable transition schema 4 将 Snapshot artifact 的 content hash 与展开后的原 `after_state_root` 分开绑定；Frozen schema 2 允许同样的引用，但展开 bytes 必须等于原 prestate。旧 record/Frozen verifier 保持原合同，不能对旧签名材料重新解释。创世激活仍保存完整原 snapshot。

链深度上限 32，字段数上限 255，字段名上限 256 bytes；单对象上限 64 MiB，恢复链累计编码预算 256 MiB。达到深度、字段覆盖变化或增量不比完整 snapshot 更小时写新的完整 checkpoint。ClosedRecord 的对象数、payload 和执行层原有更小预算同时约束实际写入，不能凭本节较大上限扩大消费者预算。每个 envelope 都包含其所需的有界 snapshot 闭包；恢复、历史、full sync、Frozen 验证及独立耐久读回共用精确字节 resolver。

### 6.5.2 原生执行环境的原字节闭包

新增 `NativeExecutionEnvironment` artifact role 指向 ClosedRecord 内的小型、规范编码 descriptor；其外部 chunk 引用使用独立 typed walker，不能伪装成已包含在 ClosedRecord 内的普通 references。每个组件绑定原文件总长、完整内容 hash 和有序 chunk；descriptor 保存真实 target/OS/arch，组件包括原 loaded executable、实际已加载的文件依赖及原 features/toolchain/Cargo.lock 构建材料。每块上限 1 MiB，descriptor 上限 1 MiB，单个执行环境上限 4 GiB，不扩大原 ClosedRecord、对象、RPC 上限。

新 activation schema 2、新 record schema 5/6、Frozen schema 3 明确要求此 role；旧 schema 的身份及 verifier 保持原合同。新世界初始化配置 schema 2 在外部批准前捕获完整 descriptor，激活时须再次匹配原 descriptor；签 Frozen 前逐批向 P/R 各自 stage 并由各自本地耐久读回。stage 每批最多 8 块/8 MiB，verify 每步最多 8 块，跨步持同一内核读 pin，重启重新验证。完成进度在本次 freeze/prepare 消费后释放，不作为永久“已归档”缓存；stage 和 verifier 都不授世界写入或服务资格。

Linux 捕获固定 loaded-image 和 mapped backing-file FD，前后核对文件身份及映像清单；inode/device 只用于本次核对，不进入逻辑 descriptor 身份。原生环境不匹配、映像已被替换/删除、依赖不可读或平台无法提供完整原文件时返回 Unavailable。当前 macOS dyld shared-cache 原闭包不支持此捕获入口，不能仅保留主 executable 冒充完成。匿名 JIT 是由已保留原 WASM 与原 engine 派生的运行态；新受控 engine 禁用 memory-init CoW 文件映像，不能按任意 memfd 名称忽略可能的原依赖。该选择写入执行策略 profile 3 的原 CBOR（`memory_init_cow:false`），不改变正式世界的协议 profile version 2。

原环境合同覆盖绑定时的 loaded-file 闭包；恢复及候选封装再次核对当前映像，新增动态依赖或原环境不同会拒绝继续。它不归档内核内存或声称支持未声明的未来 native plugin。旧 native 环境的原 bytes 可经认证 archive 和 durable-set copy 取回；不能自动改用部署机当前二进制、库或摘要解释原结果。当前协议没有隐式 native 环境升级，连续执行保持原环境 root。

### 6.6 Merkle v1 与 v2

现行 v1 的 verify 会调用 build，再以 rebuilt == 原 delta 比较全部字段和 node_writes。因此旧 delta 内暂时中间根所用节点也是原编码的一部分；不能在物理整理中修改它们或直接过滤后覆写旧 envelope。v1 首先只做精确字节压缩、共享默认路径表示和 pack 重排。

新的 delta v2 可以采用批量更新、最终 root 新可达节点集合与明确顺序，减少同批被覆盖的临时路径。必须单独声明 schema、确定性构造/验证、大小边界与父根语义，保留 v1 verifier。跨版本边界由实际 producer/consumer 能力及原升级 authority 接受，不能只修改 schema 常量或让新 build 重解释旧 v1。

v2 构造评估至少明确：沿原逻辑更新顺序计算最终树，从最终根区分父树已存在节点与本批新节点，仅收录仍被最终根需要的新节点；对输出进行确定排序与 hash/引用校验，拒绝重复冲突、缺节点和额外节点。验证器独立重建同一最小集合再比较全部 v2 字段，不能仅验证 new_root。默认先保留现有树与 root 算法；若另改树结构或根定义，必须作为进一步的逻辑协议变更评审和验收。

新世界 producer 显式生成 schema_version 2；节点编码与根算法不变。构造先按原 key 顺序应用 updates，仅沿最终 root 收集本批仍可达且父树不存在的节点，按 digest 排序。父树存在性由受认证的更新路径判定，不以任意本地 CAS 命中改变结果；验证按版本独立重建完整最小集合并比较，额外、缺少、重复或顺序错误的写入均拒绝。构造前限制累计 leaf bytes 为 64 MiB，避免仅靠单叶与数量上限造成过量中间分配。v1 build 和 verifier 继续保存原字节与中间 node_writes。

256 层索引每个 branch 原编码为 104 B，一个新键路径的 branch 约 26 KiB；pack 的 44 B/entry 索引还有额外开销。这是结构分析，不是实测总量或节省承诺。v2 的精确构造、编码与预算须在专门实现 gate 固定；物理 gate 不依赖这项决策。

## 7. 状态、事务与持久化

### 7.1 表示切换事务

维护状态为 Planned → Building → Verified → Activated → Reclaimable → Complete。失败可进入 Blocked 或 Unknown；这些是本地维护状态，不是新的世界状态、正式提交阶段或 workflow 门禁。原业务 accepted/applied/persisted/qualified 仍按各自回执判定。

1. 在受控存储根内读取当前 generation、权威根与 pin，登记维护源/目标保护；创建独立 staging。
2. 写新 pack/frame/recipe/index；校验大小与所有原逻辑 hash，遍历完整闭包，并验证活跃读者兼容。
3. 对新文件 fsync，发布不可变 generation 路径并同步父目录；持久化绑定 old/new、内容索引、验证覆盖及 overlay 截止点的维护记录。
4. 进入单 writer 切换边界，比较 expected generation，合并期间新增根/对象/pin；对新增依赖和最终索引完成同样的验证、文件及目录同步，再固定切换点。无法在该边界闭合差集就保持旧选择器并退出。
5. 原子更新选择器并同步文件及目录；全部持久性步骤成功且读回准确新值，才能判为 Committed。明确未切换才是 NotCommitted；同步失败或 I/O 不确定时为 Unknown，缓存中读到新值不能代替成功的耐久证明。
6. Activated 后新读者使用新表示，旧读者继续持有旧 generation；待 pins 解除且替代闭包仍合格，才回收旧表示。
7. 回收失败保留多余文件并报告残余占用，不能撤销已经提交的世界结果，也不能伪报已释放。

### 7.2 中断、冲突与恢复

| 中断/冲突点 | 必须保留 | 恢复判定与动作 |
| --- | --- | --- |
| staging 写入或校验失败 | 当前 generation、全部源闭包与活跃 pin | 明确未激活；允许重建 staging，不影响原世界 |
| 已 fsync 新数据但未切换 | old/new 闭包和维护记录 | 核对选择器及记录；没有提交证明不删旧表示 |
| selector rename/fsync/读回失败 | old/new、期间 overlay、未决业务材料 | Unknown；fence 受影响维护/服务，先核定原操作，不以错误推断世界回滚 |
| 激活后进程崩溃 | 新选择器闭包、尚未回收旧代 | 校验已提交 generation；恢复 pin/维护记录，安全继续原清理 |
| 激活后新包局部损坏 | 所有尚可用表示及诊断材料 | 查证同逻辑 hash 合格副本；无法闭合时隔离，禁止扫掉备用闭包 |
| 同时有 writer/成员切换 | 正式未决、seal/durable-set 和所有相关根 | 原 fencing 先决定权限；worker 无权完成旧 writer 的提交 |
| reader/transfer 尚未结束 | 其固定根、generation/recipe/字典 | 延后回收或显式取消后解除；不让活跃范围读混代 |

<a id="des-lws-004"></a>
### 7.3 GC、reader pin 与 attempt 生命周期

Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；承接 [REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004)。

区分逻辑对象回收与物理表示回收。正式历史可达的逻辑内容必须始终存在；同一逻辑内容可以在新表示激活、原字节验证、必要故障域与所有 pins 条件满足后释放旧表示。无法辨明某文件是唯一证据还是缓存时保留并报告 blocked，不用路径名猜测。

GC 在受控边界固定根集合，完成 typed mark，记录未解析项与每个对象的保留理由，再核对发布期间的新根及 pins。只有根一致、完整遍历成功且删除对象不属任何保护闭包时才 sweep。缺对象、未知类型、预算耗尽或读者状态不明均禁止该范围 sweep；“扫描没找到引用”不是完整性证明。

最终重核到 unlink 之间必须消除竞态：新根/引用发布、AcquireReadPin 及 Reclaimable→Reclaimed 共用现有受控存储 guard，或实现有同等可证明性质的 epoch 协议。长时间 mark 在锁外完成；进入最终边界后，核对根/pin 差集并为待删表示/generation 安装可恢复的 retire fence，禁止新成功引用或 reader pin 指向它。已有持有者继续读取，待其全部解除后再次在相同边界复验，才删除并登记 Reclaimed；崩溃后 fence/删除状态不明则保留材料并核对，不能猜已释放。

业务复用或 reader 获取遇到 retire fence，必须重试到已经持久且验证等价的新表示；没有替代表示就延期，不能先发布对唯一待删位置的引用。新根或 pin 若先成功，GC 必须把它纳入保护并放弃本次删除；fence 若先成功，新引用/pin 必须重试。仅在 mark 末尾读一次 pin 数量，随后无保护地 unlink，不满足本合同。

一个 pack 若同时含保留和可回收成员，先把全部保留成员写入并验证新 pack，再通过 generation 事务更新所有 locator，最后解除旧 pack 的所有硬链接与读者持有。删除某个逻辑 hash 路径并不代表共享 inode 已释放；最后链接或仍打开的文件持有未解除时，容量证据仍记录残余占用。

reader 必须先原子获得固定 generation/根的 pin 再读，结束、取消或已核实进程失效后明确释放。远程范围读取、恢复与迁移也持有 operation pin。时间经过本身不证明读者消失，不以 TTL 释放逻辑历史；未知生命周期宁可多保留一代。反向引用表可加速，但必须能够从原根重建并交叉验证。

pin 注册表及其恢复信息必须跨重启可核对；进程号重复使用或连接断开不能独自证明旧读者失效。取消或回收失效读者的 pin 时，先撤销其继续获取旧 generation 的能力并确认无人持有依赖；未能完成核对则保留。资源允许且根/pin 已稳定满足条件后，维护调度应重试并最终收敛可回收表示，持续 blocked 必须暴露具体保护理由。

attempt 目标生命周期为 Active → AwaitingDecision → QualifiedVerified → PublishedClosureVerified → Reclaimable → Reclaimed；任一步可能转 Unknown/RecoveryPinned。其幂等身份绑定原 world、authority、Frozen reservation、position、candidate hash 和执行 attempt ID。QualifiedVerified 仅证明原协议的 qualified 边界，不能代替 writer published envelope 的持久化、读回或后续 acknowledgement。

进入 PublishedClosureVerified 前，必须确认精确原 published envelope 已持久，并通过不依赖 attempt 路径的目标 reader 完整 typed 遍历其原 bytes、全部引用与必要故障域材料；不能从仍存在的 attempt 目录回退取数来证明目的层完整。pending ack、outbox 交付和恢复义务分别核定，并把未结束义务的原件/身份/恢复依赖登记为独立保护根。只有待删清单不承载这些义务、目的闭包可独立恢复且对应 reader/rollback/迁移 pins 已解除，才解开该清单的 attempt pin。

无需等待所有业务 ack 才回收真重复输出：未结束义务的原件与继续路径已独立保留、且不依赖待删表示时，可以在上述事务边界回收重复 snapshot/journal/module/output。若某未结束义务仍唯一依赖 attempt 中的内容，则只保留所需闭包并阻止相关删除，不以“candidate 已 qualified”删除整个目录。

Frozen、Prepared、Decided、回执丢失和本地成功但远端 unknown 均不满足回收条件。终态摘要保留原身份、结果定位、闭包验证、回收清单和原因；摘要不替代正式 payload。失败 attempt 可在证明其不再承载任何恢复唯一依赖后回收；“运行失败”本身不是删除凭据。

## 8. 部署、安全与运行约束

<a id="des-lws-005"></a>
### 8.1 独立故障域、授权与旧 writer

Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；承接 [REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005)。

去重在各授权持久域内执行。P/R 必要副本不得因内容相同合并成同一磁盘、同一故障点或唯一共享 bucket；一个 hardlink 不能证明第二份耐久副本。若使用远端冷层，应按当前 durable-set 合同证明每个必要域的独立持久性、访问权、内容完整性及故障恢复，不用两个路径/URL 代替独立性。

压缩 worker 仅有其存储根的维护权限，不持有或代用业务 signer，不改变当前 authority、seal、registry、finality 或 serving 状态。原 guard 与单 writer 限制覆盖维护切换；旧 writer 在成员/版本切换后仍被 fence。历史归档读取须由当前合法读者按原证明验证，不能重新启用旧 epoch 写权限。

维护只处理 owner 已绑定的普通文件/目录，拒绝不安全 symlink、路径越界和未预期权限；文件与目录同步、原子 rename 语义须在目标文件系统验证。读取、指标和错误不得导出私有 payload、密钥或凭据。对象 hash 被猜中不代表有权读取对象。

<a id="des-lws-008"></a>
### 8.2 全链预算与可判定退化

Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 [REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008)。

每个实现 gate 必须固定并记录下表预算。预算是操作资源边界，不是历史保留期限。不能只把写端上限改大而让 proof、offline reader、恢复或全量同步继续无法读取；也不能因压缩后很小就绕过原逻辑大小限制。

| 预算维度 | 必须约束的消费者/阶段 | 耗尽后的合同 |
| --- | --- | --- |
| 单逻辑对象与原 envelope | writer、endpoint get、原 typed decode、offline/published/restore | 写前能判定则拒绝该表示/候选；原协议已持久的 unknown 先查询，不重造请求 |
| pack/frame/stored bytes | compactor、archive copy、校验 | 停止本批构建；源闭包保留，可按原计划续传 |
| 解码后 bytes/窗口/recipe 深度/依赖数 | 所有 Get/Range、cache、Merkle reader | 明确 BudgetExceeded/UnsupportedRepresentation；不把缺页当不存在 |
| 总闭包遍历内存、并发与 I/O | GC、full sync、迁移、验证 | 有界批次和进度检查点；未完成不授予闭包完成或 GC 资格 |
| 时间、重试和缓存 | range、传输、历史查询、双端状态核定 | 可取消/可续传；不延长原业务权限或重新执行历史效果 |
| 磁盘峰值与保留余量 | 双代共存、overlay、rollback、reader pin | 提前暂停维护/按原合同限流新工作；不删历史来腾空间 |

全量恢复目标采用固定逻辑根的清单/对象/范围增量遍历、校验与断点续传；不能把整世界装入单一 ClosedRecord、Vec 或 RPC envelope 后称为流式。进度必须绑定原根和已验证范围，消费者重启后重新校验来源、版本与 authority；原数据在目的地闭包完成前保持 pin。

当前源码的局部分页和范围读不证明上述端到端能力已经成立。大历史或超出现有操作预算时，应明确返回“材料保留但本次读取/恢复未完成”、可恢复进度与阻塞原因；无法安全服务时按原服务闸门退化。只有该流式闭环通过相应规模验收，才允许减少那些目前仍被整包消费者依赖的源表示。

<a id="formal-open-prewarm-reuse"></a>
### 8.3 单次 open 内的成功预热复用

在一次正式 runtime open 内，第一遍从外部固定 C0 到目标 head 的完整回放、所有材料验证及缓存安装均成功后，可生成不可由外部构造的成功凭据。凭据绑定同一 resolver 实例及其不可变 origin/trusted/recovery/activation 上下文、同一 reader 实例，以及回放产生的完整 checkpoint。只完成部分前缀、超时、缓存安装失败或验证失败时不得生成可复用凭据。

writer 的正式双端恢复与当前 head/权限核定仍独立执行；恢复后得到的完整 writer anchor 仅在与成功凭据所有绑定精确一致时免除同一次 open 中第二遍相同预热。受保护 head hint 只用于定位，不能授予该凭据或替代外部 C0 信任。不同 head、同高度不同记录或 authority、不同 reader/resolver/context、恢复推进及新一次 open 一律重新验证，不跨进程持久化成功凭据。

此复用只避免重复装载已验证的只读历史内容，不改变持久格式、资格判定、动态权限、原请求身份或错误/期限语义。验证覆盖 exact 命中、不完整和失败不出票、各绑定变化失效，以及最终独立恢复保留；同时记录预热尝试、命中与耗时，不能把跳过一次预热推断为整个恢复已通过。

## 9. 质量与容量

<a id="des-lws-009"></a>
### 9.1 度量定义与收益判定

Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916；承接 [REQ-LWS-009](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-009)。

每次证据固定 source/tested tree、世界/起止高度、负载种子、schema/codec、故障域、机器/文件系统/配额、冷热配置、并发、缓存条件和测量窗口。前后两组必须有相同正式历史、资产与恢复承诺；以减少历史或副本得到的下降不计收益。

分别报告：原逻辑内容 bytes、逻辑唯一内容、实际 allocated bytes、apparent bytes、文件/inode 数、重复表示、活跃/旧代/attempt/回滚 pins、冷归档占用和未完成回收量。文件系统使用实际 block 分配与 device/inode 去重；共享 pack 的多个 hardlink 只在该故障域计一次。各 P/R 域单独计数后相加，不跨域去掉必要副本。

支持 reflink、文件系统透明压缩、稀疏文件或远端对象存储时，说明 inode block 计量与真正介质/服务计费占用的差异；有共享 extent 时使用适用的原生分配统计补充，不能把按 inode 求和当作精确全盘物理用量。缺少任何纳入范围的目录、故障域或归档计数时报告 incomplete，不能将缺值补零形成整体收益。

写放大与持久增长分别报告。累计写入包括新对象、阶段记录、pack 重写、迁移和清理元数据；能够测到实际设备 I/O 时标明范围，只有应用 write bytes 时明确其局限。持久增长使用稳定边界后的 allocated 差值/提交数或实际业务操作数。压缩完成但 old generation 尚受 pin 保护时不能提前计入释放。

同时记录读取/range p50/p95/p99、冷启动与热缓存恢复耗时、目标高度查询、解码 CPU、峰值 RSS、临时磁盘峰值、归档 I/O 和续跑延迟。读性能阈值沿用已接受的同环境服务预算；没有既有数值时由 Runtime/ops/QA 在启用 gate 前固定，不能只写“可接受”或事后选择更宽阈值。

### 9.2 质量与容量场景

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 空 Tick 与稳定模块 | 固定世界 1k/10k 提交；同 WASM，已记录磁盘与内存配额 | 重复 payload 共享，历史身份完全一致 | 稳定增长斜率、阶段重复量；不预设节省百分比 | [V-LWS-CAPACITY](formal-world-storage-efficiency.manual.md#v-lws-capacity) | 未运行目标 |
| 活跃业务历史 | 固定种子业务、工业回执、result/nonce/intent/Journal 跨热窗 | 冷内容完整可查，余额和下一业务一致 | 唯一内容增长与额外表示增长分列 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)、[V-LWS-CONTINUE](formal-world-storage-efficiency.manual.md#v-lws-continue) | 未运行目标 |
| 小修改的大 snapshot | 固定模块集与大状态，每提交改小子集 | 原 bytes/root 可重建，比较块命中及元数据成本 | allocated、write bytes、重建开销 | [V-LWS-CODEC](formal-world-storage-efficiency.manual.md#v-lws-codec) | 未运行目标 |
| 索引与 pack | v1 多 update/跨 pack，小事件和大叶子均覆盖 | 原 node/delta 不变；v2 独立验证 | node/pack/index bytes、inode、读延迟 | [V-LWS-MERKLE](formal-world-storage-efficiency.manual.md#v-lws-merkle) | 未运行目标 |
| 迁移和读者并发 | 双独立域、长 range/full sync、持续业务 overlay | reader 不混代，峰值不越已冻结预算 | pins、峰值、冲突/重试；§8 预算 | [V-LWS-MIGRATE](formal-world-storage-efficiency.manual.md#v-lws-migrate)、[V-LWS-FAULT](formal-world-storage-efficiency.manual.md#v-lws-fault) | 未运行目标 |
| 单域故障与冷恢复 | 实际独立故障域、原 authority/历史版本、冷缓存 | 另一合格副本恢复；无旧 writer 重新服务 | 完整性、恢复成本及服务 gate | [V-LWS-DURABILITY](formal-world-storage-efficiency.manual.md#v-lws-durability) | 未运行目标 |
| 超预算与截断 | 大对象/解压炸弹/损坏/取消/磁盘不足 | 明确退化，保留原请求与源闭包，不静默删历史 | 错误分类、原数据/权限不变、可续传 | [V-LWS-BUDGET](formal-world-storage-efficiency.manual.md#v-lws-budget) | 未运行目标 |

## 10. 兼容、迁移与回滚

<a id="des-lws-006"></a>
### 10.1 物理迁移序列与软件回退

Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0；承接 [REQ-LWS-006](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-006)。

先部署能读旧 raw/pack 和新表示的读者，默认仍旧写；盘点所有直读文件、range、offline、迁移/归档和 recovery 消费者，完成能力矩阵。之后 shadow 构建新表示并重建原 bytes/hash，仍保留旧选择器和源材料。新写入启用前，当前 endpoint、必要故障域、运维/修复工具和回滚读者均需具备明确能力。

再按 §7 激活同世界物理 generation，记录切换前后逻辑根和存储索引。在同一活跃业务历史下验证冷读取、双端恢复和续跑；旧代 reader/rollback pins 解除后才回收。迁移期间超时或 unknown 继续原维护事务，业务未知提交按原 Frozen/candidate 查询，不回滚到历史 snapshot、不创建替代世界。

“停止使用新 codec”与“启动旧 binary”是两件事。新格式已经写入且只保留新表示后，旧 binary 可能无法读取；必须拒绝不具备 reader capability 的启动。物理回退先把切换后全部已接受逻辑对象转成目标旧读者可理解的表示，验证完整当前历史及必要故障域，再切换选择器；回退 feature 不擦除期间业务。

若新逻辑 checkpoint/delta 或 Merkle v2 已产生历史，旧逻辑读者未必可恢复，不能通过物理解压解决。应保留兼容新旧逻辑的 binary，必要时暂停新写/保持受限读取并修复；不得重写已签历史、回退 epoch/head、补造旧格式结果或取消已经产生的外部效果。

<a id="storage-efficiency-rollout"></a>
<a id="current-storage-implementation"></a>
### 10.2 分阶段实施及退出门槛

现行前置入口：`oasis7_distfs::storage_inventory::inventory_storage` 及 `storage_inventory` example 提供只读分域 inode/allocated 基线，使用边界见[容量入口](formal-world-storage-efficiency.manual.md#v-lws-capacity)。这是 S0 的本地物理计量子范围，尚未覆盖运行时 typed root/pin 完整盘点或真实世界基准。

S1 由 `business_pack::read_object` 统一承接 raw、既有 pack、显式物理 recipe 及所选 generation；`Store::get` 在逻辑 bytes 校验后解码，`Store::put` 重复检查比较逻辑 bytes。物理 codec 不改变 canonical serializer。下表是必须持续维护的消费者边界；每次新增 codec 或 typed 外部引用都要覆盖这些入口。

| 消费者 | 现行入口与边界 |
| --- | --- |
| typed 状态、request/history/index | `endpoint_store.rs` 的 get/put → raw_object → business_pack::read_object；归档返回后仍校验 64 MiB 和 hash |
| raw 与 committed material | Store::raw_object、endpoint_archive_file.rs → business_pack::read_object；逻辑对象逐 hash 校验 |
| range / chunk / 本地 archive | endpoint_store_native.rs、endpoint_chunk.rs、endpoint_archive_file.rs → business_pack::read_range；保持原分片认证合同 |
| registered durable readback | endpoint_store_durable_set_copy.rs → read_object_durable_once；每对象校验、每 inode 同步 |
| RPC / transfer / remote archive | endpoint_archive_transfer.rs、transport/client.rs；消费逻辑 bytes/chunk，不把物理 pack 传为逻辑对象 |
| offline/published / native restore | 上述逻辑接口、共享 Snapshot resolver、原 native typed chunks；不能依赖 writer attempt 或其他故障域补齐本域缺失 |

S2 另有 `build_formal_storage_shadow` 和 `formal_storage_shadow` example：在持有原 endpoint 独占 lease 时，将全部 digest 对象重建到新目录的既有 v1 pack/raw 表示，并逐对象精确读回。固定 8 MiB payload 批次、65,536 对象、1 GiB 总逻辑输入及原单对象 64 MiB 上限；大于既有 pack node 上限或为空的对象保持 raw。目标必须在源外且不存在，对象写入前先同步 shadow-only 标志（初始化失败可只留下空目录）；未知条目、非空 staging、超预算或校验失败保留源并停止。它只提供对象命名空间的候选物理占用，未携带 checkpoint/authority/selector，不能作为 endpoint 或完整恢复 generation 激活；当前 Store open 在创建 checkpoint 前拒绝 shadow-only 标志。操作和失败边界见[shadow 入口](formal-world-storage-efficiency.manual.md#v-lws-codec)。

另有 `advance_formal_storage_shadow` 分批入口：绑定不变的源 checkpoint、registry、selector 与目录元数据，持久化私有 catalog 和经校验游标；copy、独立 verify 与临时统计清理分阶段推进，每步最多 1,024 records，采用原单轮逻辑预算，不把总历史物化进一个 Vec。中断后只续同一源和原操作；源变更明确拒绝，shadow-only 标志持续阻止作为 serving endpoint 使用。verify 页持久化唯一对象计数及逻辑字节数，中断重试不重复计数；收尾只扫描文件元数据，不再隐藏全量 payload 读取，临时统计材料分批清理后才计算最终占用。旧一次性 API 保留原预算语义。

正式 endpoint 的 `advance_physical_generation` 使用独立带 checksum 的 build/selection/retire 状态和分片 catalog；单次复制、退休最多 1,024 对象/8 MiB（单个较大有界对象除外）。切换前在稳定根 fence 内补查 overlay，新选中代损坏不得回落到旧 overlay。旧代及根 orphan frames 分页回收；仍被新 overlay 引用的共享帧保留。catalog v3 采用按内容 hash 认证的层级目录：manifest 只保存固定大小的根引用及计数，每个内部节点最多 16 个子引用，每个叶最多 4,096 个有序 IDs；叶满时按下一个 hash nibble 分裂，深度只受原 64 位十六进制对象 hash 的自然边界限制，不以固定分片总数限制历史。lookup、分页及验证沿原 hash 路径进行，不把完整目录装入 manifest 或内存。每轮提交新目录根后，以可恢复清单分批回收不再被当前 objects/completed 根引用的旧目录节点；仍被任一根引用的节点必须保留，不能用索引版本堆积换取表面 payload 节省。同步 typed orphan GC 另有独立 mark 预算，预算不足只保留，分批 generation 不继承其整轮 1 GiB/262,144 对象限制。

attempt 回收要求原 Published envelope 与 P/R 分别完成的独立 typed 闭包读回。每个闭包作业在活跃内核 pin 内分页，重启失去 pin 后重新读回；未完成或 unknown 保留 attempt。稀疏树遍历页保留最多 256 节点/8 MiB 的工作上限，并在完整校验一个节点后按 1 秒软工作时窗让出；软时窗不替代原 RPC 截止时间，也不允许部分校验节点进入 receipt。业务提交与 Node 空闲轮询仅推进有界维护步骤；源中独有的 world/外部结果/未决义务不能因已经 Qualified 而删除。完整实现是否达到退出门槛仍按下表和 §11 的实际 source/environment 证据判断。

同一共享 recipe 的多个小对象允许通过一次明确的有界批量读取验证：单批最多 1,024 个请求对象、8 MiB 返回原字节，超过该字节界的单对象沿原 64 MiB 上限独立读取。批内持有原物理根 pin，重新读取并校验每个涉及 recipe 的当前完整内容及每个涉及 frame；同一分组内的 frame 只需解码一次，各逻辑对象仍分别验证完整原 hash。按 recipe 分组后逐组释放元数据，仅保留一个最多 64 KiB 的解码 frame 暂存，返回字节仍受 8 MiB 批界限制。各 alias 都通过受控路径及 O_NOFOLLOW 新打开并校验 owner、类型和写权限；仅允许在本批仍持有 anchor FD 时，以 dev/ino 证明 alias 指向该同一物理文件，不能据此证明内容正确。每组仍须从 anchor FD 本次完整读取并校验 recipe，逐项确认请求对象的成员关系、长度及覆盖。最多同时持有 16 个 anchor FD，达到上限先验证并释放当前分组再继续，不无界持有文件。函数返回即丢弃批内复用状态；下一批即便遇到相同 inode 也重新完整读取和校验，不以 inode/mtime 复用内容，不跨调用保留 frame 原字节，也不复用其他端点的结果。全局解析缓存仍受原固定预算约束。耐久读取在同一批内对共享不可变表示同步一次；不得为已有对象先做一遍普通全读、再重复全读只为同步。批量错误仍阻止相应持久化成功或删除源副本，不将部分验证结果视为完整 receipt。

上述 16 个 FD 指保留到分组完成的 anchor，另允许一个临时 probe FD 校验新 alias 身份；probe 不计入保留组，不能累计。frame 解码复用只覆盖当前 recipe 分组；不同 recipe 或已释放后重新遇到的分组仍重新读验，不能为全调用去重引入无界 frame 缓存。

尚未启用物理 codec 的受控 CAS 根仍需保持原 pack 的耐久去重：同一有界批内可按上述当前 FD 身份对共享原 pack 分组，本批重新检查固定 pack header 和各请求的二分索引、span 与声明长度，逐对象读取并验证原 hash，分组结束对 anchor 文件同步一次。不能因新增 codec 批量分支移除原有按 inode 的批内同步去重，退化为共享 pack 的每个成员各自 fsync。该路径同样受 1,024 请求/8 MiB 输出、16 anchor 加一个 probe 的限制，不物化完整 pack，不跨调用缓存内容，也不通过更改根的 codec 启用状态或测试材料来声称优化收益。

同一次有界物理写入中，每个新 frame 仍先完成原文件 fsync、无覆盖发布及既有内容一致性检查；允许将 frame 目录和私有 staging 目录的重复 fsync 合并到该次 frame 批次结束。所有相关目录同步成功后，才可发布引用这些 frame 的 recipe/selector；其本身仍沿原发布顺序耐久同步，不跨写入调用延迟。目录同步失败或中途崩溃时，尚未发布的引用不得获得成功回执，源材料必须保留；仅留下可由原批次重试重建的未选中 frame/暂存文件，不能把文件内容同步等同于目录项已耐久。


同一次 Node 执行对同一 Frozen 的恢复和候选重建，允许复用一次完整验证得到的原 Snapshot 字节，避免重复展开相同 delta 链。该证明仅绑定本次借用的精确 Frozen、authority 和 parent；等值副本也不能替代绑定对象，不可序列化、克隆或跨执行、恢复重试、RPC 复用。构造时仍验证原材料、完整依赖闭包、body digest、Snapshot 链和原前态 hash，不允许用执行后 World 补齐原材料。最多额外持有一份 64 MiB 原 Snapshot；原深度 32 与链输入 256 MiB 上限保持不变，不缓存全部中间历史。每次恢复继续检查权限、证书、manifest、当前 native 环境、树与 transition；重建后的 body 和全部 material 必须与原 Frozen 精确相等，才可复用其内容验证证明。现有公共 verify/attach 接口保持原行为，返回同一验证 pass 已展开字节的内部接口不得改变旧 schema 的原验证或分配语义；恢复专用 attachment 路径必须接收不可自行制造的证明并立即核对完整重建结果，不提供通用跳过验证开关。

SnapshotDelta 的验证接受上限仍为深度 32、链输入 256 MiB；生产者不必耗尽该上限。新生成 Snapshot 默认最多连续 8 层 delta，下一次变更写完整 checkpoint，以限制逐层重组、hash 和闭包解码的重复成本；不重写已有签名历史，也不删除早期 checkpoint 或 delta。原 Snapshot 字节、状态根和业务效果保持一致，新 artifact 的身份随合法 producer 输出重新生成。完整 checkpoint 仍经过相同有界物理 CDC/压缩和完整性验证；不能预先宣称更密 checkpoint 的实际空间开销为零，须将完整长历史耗时和容量证据一起报告。验证器对已有 9–32 层合法链继续完整验证，超过上限仍拒绝，不能把 producer 策略变成历史读取上限。

单个端点 Prepare 的既有 request-local 候选闭包证明，允许保留同一完整验证 pass 得到的原 before Frozen 与 after record 的 Snapshot 字节，供原前态匹配、retention descriptor 和 Tick prefix 检查借用。证明仍绑定精确借用的 candidate、record、material、Frozen body/source；不跨端点、RPC 或 Decision 复用，不接收执行后 World 作为原前态替代。完整闭包、对象 hash、原前后状态根、证书、native 环境、两个 delta 及原 retention/prefix 校验全部保留；只消除同一不可变材料在一次校验中的重复展开。两个原字节缓冲各不超过 64 MiB、合计不超过 128 MiB，不克隆额外缓冲；已有两个 encoded 闭包各 8 MiB 上限保持。原 typed Tick tail 允许合法 legacy 长度，不能把其 Value 容器误称固定小型视图，需保留原解码语义并分别测量峰值。公开 record/Frozen verifier 的旧行为保持：旧 schema 本来不展开时仍不新增展开、校验或分配，只在原来就需要 Snapshot 的 Prepare 路径补一次原读取。Decision 是新的独立校验调用，不能沿用 Prepare 的证明。实现和验收须区分源码消除的重复次数与真实 Linux 进程耗时，不以次数变化代替完整场景通过。

Node writer 从同一不可变候选恢复时，也可将 record 完整验证同一 pass 返回的原 Snapshot 字节直接交给原 typed Snapshot decoder，避免验证后立即再次展开同一链。该原字节仅在本次恢复内使用、解码后释放；旧 schema 未返回字节时沿原读取路径。原 journal、状态根、qualified anchor、retained parent、World boundary、树及 release policy 检查不变，不能将这种字节复用视为免除其他恢复检查或跨候选缓存。

World 的共享正式历史 reader 完整转发底层 `BusinessIndexNodeReader` 的路径预取、已缓存节点读取、内容缓存与普通节点读取，避免透明适配器截断既有整路径双端预取，使冷 Journal/Tick 恢复退化为逐节点远程调用。原远端 reader 的签名、root/key、双端路径一致性和固定缓存预算不变；`BusinessIndexRoot` 仍从合格根逐节点校验 hash、深度与路径，缺失或损坏仍拒绝。预取与逻辑内容缓存只服务恢复读取，不代签端点独立持久闭包读回或删除授权；包含未合格 overlay 的其他 reader 不盲目套用远端根预取。

阶段是技术依赖顺序，可合并实施，不新增 workflow、审批次数或每阶段强制 PR。当前无运行世界时直接创建新表示；本地实现与验证不代替真实节点部署验收。

S0–S3 的物理优化与生命周期治理可按已验证范围独立交付、部署。S4 完整 delta 与 S5 Merkle v2 可以分别后置，彼此也不作为物理优化的前置条件。S6 对本次实际启用的子集执行全部适用部署验收；未启用能力维持原数据/格式与必要限制，明确标为未验证，不要求等待所有逻辑演进后才能部署已通过的物理优化。

| 阶段 | 具体改造及依赖 | 退出门槛与接受证据 | 未通过时行为 |
| --- | --- | --- | --- |
| S0 基线与闭包盘点 | 度量 actual allocation；列全对象/直读者/预算/根/pin；选固定负载 | 可复核目录归因、域独立性、引用类型和基准成本 | 保留原路径，只读观察；不报告预计节省为事实 |
| S1 统一逻辑读取 | raw/旧 pack/新 codec 混合 reader；完整 range/归档/repair 消费者能力；source fencing | V-LWS-CODEC/BUDGET：原字节/hash/签名与范围一致；所有读者可用 | 禁用新写；未知读者阻止启用 |
| S2 物理共享与压缩 | 阶段 recipe、candidate/artifact 共享、frame pack、snapshot/module 字节分块；shadow→事务切换 | 全逻辑对象重建及闭包完整；故障/双代/续传通过；稳定占用与成本同证据 | 原表示继续，最多多占用；不跳过失败恢复 |
| S3 生命周期与 GC | attempt qualified/published/未决义务分验、完整目的闭包；typed 根/pins 与 retire fence、同域回收 | V-LWS-GC/FAULT/DURABILITY：发布后无 attempt 依赖、对抗 pin/引用竞态安全、故障无丢失；实测净释放 | 停 sweep、保留材料、报告 blocked/残余占用 |
| S4 完整增量 | §6.5 全字段覆盖、producer 记录、历史 adapter、checkpoint 路径、非空后缀续跑 | J1/J2 独立对照、原结果与效果连续、全区间恢复路径和预算证明 | 不释放该区间唯一 snapshot/控制材料 |
| S5 Merkle v2 | 单独协议版本与 verifier；最终新可达节点策略；v1/v2 混合历史 | 原 v1 bytes/验证不变；v2 确定根/证明/边界及恢复通过 | 继续 v1；不过滤历史 node_writes |
| S6 部署收口 | 真实两域、长历史冷恢复、升级/物理回退演练、运维手册和监测 | 全适用 V-LWS 场景同 source/environment；明确规模与残余风险 | 限制启用范围，不将本地 fixture 扩成生产验收 |

## 11. 验证设计与可追溯性

本节 V-LWS-* 是目标场景 ID，不是已经存在或运行过的测试命令。实施 PR 必须把场景绑定到准确测试/手动步骤、source/tested tree、原始输出和环境，报告未执行项。文档检查只能证明文档结构与引用，不能证明节省空间、恢复正确或发布完成。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | [闭包](formal-world-storage-efficiency.design.md#des-lws-001) | 全历史位置、原依赖内容和控制链可获取/验证 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)：同 world/authority 的迁移源与目标逐对象原 bytes/hash、连续位置/引用闭合核对，跨热窗、旧 epoch/module/branch 抽取恢复；required 逻辑层 + full 真实两域 | 完整 root/object 覆盖清单、缺失项为零、恢复与未运行区间记录 | 抽样业务重放不代表所有路径均已语义验证 |
| Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c；[REQ-LWS-002](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-002) | [物理合同](formal-world-storage-efficiency.design.md#des-lws-002) | exact bytes、原签名、逻辑范围与混合旧格式 | [V-LWS-CODEC](formal-world-storage-efficiency.manual.md#v-lws-codec)：raw/pack/压缩/recipe 互转，跨 frame range、magic 冲突、截断/错 hash/字典/旧 CBOR 数组；fixed corpus required，真实文件系统 full | 原对象差分、签名与范围结果、消费者能力矩阵 | 不证明未覆盖 codec/平台或逻辑新 schema |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | [冷热](formal-world-storage-efficiency.design.md#des-lws-003) | 超热窗、稀疏 checkpoint 后完整原历史仍可读 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)：切换热窗/密度前后同高度逐对象核对，目的地断开/损坏时保持源 pin；含旧 nonce/receipt/module、GameGoalPublish/GameExecutorControl 冷前驱与新工作，完整 closure full 读取 | 冷源读回、原内容、前驱/续跑、进度与失败分类 | 不证明尚未实现的无限规模/无限时限 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | [GC/attempt](formal-world-storage-efficiency.design.md#des-lws-004) | published 独立闭包及未决义务可替代才回收，unknown/reader/rollback 保护 | [V-LWS-GC](formal-world-storage-efficiency.manual.md#v-lws-gc)：同候选跨 qualified/published/待 ack、无 attempt 路径读回目的闭包；final check 后对抗插入 pin/引用与 retire fence；required fixtures + full 进程终止/重启 | root/pin/义务差分、发布读回、竞态结果、精确释放量与恢复 | 单进程错误注入不代替独立故障域耐久性 |
| Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005) | [权威](formal-world-storage-efficiency.design.md#des-lws-005) | 双独立域、旧 writer fencing、历史读不授写权 | [V-LWS-DURABILITY](formal-world-storage-efficiency.manual.md#v-lws-durability)：真实两域固定版本，单域不可用/损坏、成员切换与旧 writer 请求；从合格闭包恢复并核 serving gate | 域拓扑/持久证据、原回执、拒绝与服务结果 | 两目录/同 inode 或同盘 fixture 不能证明域独立 |
| Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0；[REQ-LWS-006](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-006) | [迁移](formal-world-storage-efficiency.design.md#des-lws-006) | old/new 不确定切换和新写后回退保持同世界完整历史 | [V-LWS-MIGRATE](formal-world-storage-efficiency.manual.md#v-lws-migrate)：复用 FAULT 的文件/目录 fsync、selector 切换/读回、旧代回收中断；加入新业务后测试旧 binary 拒启及合法物理回退；full 真实文件系统 | NotCommitted/Committed/Unknown 原始结果、selector/根、当前历史与效果对照 | 不宣称旧逻辑 binary 可读已发布新逻辑历史 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | [完整增量](formal-world-storage-efficiency.design.md#des-lws-007) | J1/J2/队列/原外部结果恢复后续跑，C<T 非空 | [V-LWS-CONTINUE](formal-world-storage-efficiency.manual.md#v-lws-continue)：按 §6.5 的 J1/J2 验证扩展同候选未中断对照；多事件/拒绝/预分配/rollover、两 instance/Pure/wake/clear；持久候选/Frozen-only/外部回执未知分别核对，原已确认效果 0 次重派；required + full 原 formal 路径 | 独立 J1/J2 差分、下一合法结果/费用/调用、原幂等对账与缺记录拒绝 | 完整 T snapshot 往返或单独 state root 不证明非空后缀；不宣称无外部幂等条件的物理恰好一次 |
| Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008) | [预算](formal-world-storage-efficiency.design.md#des-lws-008) | 所有消费者超预算明确退化，原操作和源闭包保留 | [V-LWS-BUDGET](formal-world-storage-efficiency.manual.md#v-lws-budget)：逐 budget 维度上限/越界、并发、取消/超时/续传、磁盘不足；cold/full sync 覆盖大历史，输出内存/峰值/进度 | 错误分类、请求身份、pin/世界不变、续传完成 | 较小 fixture 不证明更大规模已支持 |
| Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916；[REQ-LWS-009](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-009) | [容量](formal-world-storage-efficiency.design.md#des-lws-009) | 同历史收益、增长/写放大/峰值/读取成本与完整性可复核 | [V-LWS-CAPACITY](formal-world-storage-efficiency.manual.md#v-lws-capacity)：§9 固定负载及适用 Merkle 组合前后比较，device/inode 去重，P/R 分域，cold/warm 分组，记录采样时点与未释放 pin；actual filesystem full | 原计数及归因、source/配置、p50/p95/p99、完整性配套结果 | 无数据不报节省比例；应用 write 不冒充设备 I/O |

### 11.2 独立检查与接受规则

完整性证明需要两个维度：所有迁移逻辑对象及 typed 依赖的精确字节/引用覆盖；代表性业务与控制路径的真实恢复续跑。对整包和 recipe 的全对象 hash 校验不能替代第二个维度；几个续跑样本也不能代替第一维度的完整对象遍历。

V-LWS-MERKLE 必须将 v1 原 fixture/delta 逐字节保持与未来 v2 新构造分别测试：同一批 updates 的确定性、最终 root/证明、被覆盖中间节点、错误 node_writes、混合历史、旧 reader 拒绝/兼容行为。不能仅更新 golden bytes 让旧历史测试通过。

V-LWS-FAULT 至少覆盖源有损坏、新包坏块、原状态不明、文件/目录 fsync、切换后再次新写、清理中断、远程回执丢失、reader 尚持有旧代及单故障域消失。接受标准为无静默数据损失、无越权服务、无历史外部效果重复；阻塞允许明确报告，不能伪装成功。

实际证据存入实现 PR/CI 与原始产物，记录 owner、命令/步骤、退出码、具体场景、源与实际测试版本、硬件/文件系统、计数基线和未覆盖项。需求追踪只关联条款，不另造任务身份或审批协议。

## 12. 决策、长期风险与未决问题

### 12.1 采用与排除的技术选择

| 决策 | 选择与理由 | 代价/风险 | 复核 owner 与触发 |
| --- | --- | --- | --- |
| 完整历史与减少容量 | 减少物理重复，保留原逻辑内容与必要副本 | 压缩/索引带来 CPU、读取和迁移峰值 | Runtime/ops/QA；负载或恢复预算变化 |
| 先物理、后逻辑 | recipe/frame 保持旧 bytes；checkpoint/delta、Merkle v2 单独 gate | 近期仍需重建完整逻辑对象，不能承诺立即消除所有内存成本 | Runtime/DistFS；新读写路径及测量结果 |
| stage payload 共享 | 保留状态/回执，用物理引用复用 candidate/artifact | exact embedding 有旧序列化兼容复杂性 | DistFS；旧 corpus 中出现不支持编码时保留原表示 |
| GC 根选择 | typed 完整历史根 + 未决/reader/rollback/迁移 pins | 扫描与 pin 维护成本；未知时会多保留 | Runtime/ops；增加任何对象类型或消费者 |
| old writer 隔离 | 原 guard/authority 为准，codec 不能改变资格 | 需要覆盖维护、恢复、直接文件读者的能力矩阵 | P2P/ops；成员/epoch/二进制切换 |
| 索引演进 | v1 保留 exact rebuild；v2 明示版本与最小最终节点 | 新旧 verifier 共存，逻辑升级不能透明回退 | DistFS/WASM/QA；S5 开始前形成相应 ADR |
| 不采用逻辑历史 TTL | 热窗/操作预算只影响资源与可用性，超限保留数据并退化 | 仍有不可避免的真实历史增长与容量扩展成本 | 产品/ops；产品历史承诺实质变更时另行决策 |

### 12.2 需要实施时定案的参数与责任

codec/frame/chunker、delta 深度及 native 分块的首版硬边界分别见 §6.1.1、§6.5.1、§6.5.2，分批维护使用 §10.2 的操作预算；这些源码边界不等于已达成部署性能指标。并发额度、磁盘预留、真实恢复时限及 1,000/10,000 高度的容量/成本仍须用同负载实测，ops 给出真实容量和独立域条件，QA 按适用启用范围冻结指标。预算不足时保留原历史及未决材料并报告受阻，不以占位指标接受生产。

增加对象类型、改变 canonical serializer、切换模块 ABI、扩充 archive source、升级文件系统或更换故障域，均触发受影响闭包/能力/预算重新验证。需要扩大超出现有边界的产品服务能力时回到产品承诺和专业 owner，不从这份设计推断已支持任意规模。

本文提供合同、具体实现边界、阶段依赖和验证方法；不代签源码测试、压缩收益、运行数据迁移、真实故障域恢复或正式发布。实现交付按实际代码和证据逐项更新状态，缺部署环境时保留未执行结论。
