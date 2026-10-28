# 增量世界状态与完整历史存储

- 状态：target；本文定义新格式，尚未形成该格式的实现、容量或恢复验收。
- Owner role：`runtime_engineer`；共同审读：Runtime、DistFS/P2P、WASM、`blockchain_ops_engineer`、`qa_engineer`。
- 适用范围：显式选择新状态承诺与存储格式的新建世界，从其创世被接受后保留完整历史。
- 固定源码基线：`112a627e889cb0601ed368b2c0be204e39e7554a`，2026-10-28；源码事实与目标分列于 §3。
- 上游：[完整历史产品要求](../../product/world-infrastructure/lossless-world-history-storage.prd.md#lws-requirements)；相关设计：[既有物理存储优化](formal-world-storage-efficiency.design.md#des-lws-002)、[Runtime 提交与恢复](../design.md#runtime-authority-commit-recovery-target)。
- 问题来源：[#5196](https://github.com/nil-studio/oasis7/issues/5196) 的全量 checkpoint 累积，以及 [#5197](https://github.com/nil-studio/oasis7/issues/5197) 的历史物化、事务深拷贝与全量编码。本设计不等于这些运行世界已修复。

<a id="iws-scope"></a>
## 1. 问题、目标与非目标

每隔若干高度重新序列化、hash、压缩和保存整个 snapshot，会把少量状态变化扩大为全状态读写；若 snapshot 还携带不断增长的历史，累计成本进一步放大。只在输出端分块可以减少重复落盘，但仍保留全量构建和跨 store 物化。本设计直接改变新世界的状态表示及提交接口，使正常提交处理本次变更、原始历史增量和受影响索引路径。

目标结构为：有界页面的持久状态树、追加的不可变历史段、独立执行工件、完整运行控制状态，以及引用这些对象的结构化 checkpoint。已接受历史及其解释、恢复、证明和继续工作的依赖一直可获取；热窗口只影响缓存和定位成本。成功同时要求确定性、事务持久性、完整历史、非空工作续跑和同范围容量/性能证据。

这是显式的新逻辑格式和状态承诺版本。范围不要求导入旧 snapshot/CAS、保持旧 state hash、兼容旧二进制、保留旧索引或进行新旧双写。新格式一旦接受对象，其原 canonical bytes、身份、签名及历史引用仍受完整历史要求约束，后续物理重排不能改写。既有世界继续按其原格式与维护合同处理；本设计不授权删除、解 pin、重置或替换运行世界，也不凭新格式交付关闭 #5196 的原世界义务。

本设计不改变提交 authority、共识故障模型、收费或授权规则，也不新增玩家流程。它不承诺固定总容量、任意规模一次读完或未经测量的节省比例。新格式未覆盖的消费者不得获得新世界的读写服务资格。

## 2. 上游约束与相关角色

九项 REQ-LWS 继续使用原 Trace ID。这里的“原内容”指新格式世界自创世接受后的内容；旧数据导入不属于本设计的接受范围。产品完整历史义务保持，技术承接与既有物理优化设计按世界格式分开。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | 新格式创世到已接受 head 的状态、原历史、证明及执行依赖具有完整 typed 对象闭包 | [对象闭包](incremental-world-state-storage.design.md#iws-object-model) | Runtime、WASM、authority 原证明及历史执行环境 | 不导入或补造旧世界材料 |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | 任一已接受高度能从原状态/控制根及所需依赖恢复，缺失不得由重算外部结果掩盖 | [读取恢复](incremental-world-state-storage.design.md#iws-read-recovery) | Runtime、WASM、authority 原证明及历史执行环境 | 不导入或补造旧世界材料 |
| Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c；[REQ-LWS-002](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-002) | 新格式接受后，物理共享、压缩、重排保持该对象原 bytes/hash 与证明身份 | [格式和身份](incremental-world-state-storage.design.md#iws-object-model) | DistFS、全部新格式读者、canonical 编码合同 | 新状态承诺不要求等于旧 snapshot hash |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | 越过热窗或调整 checkpoint 密度后，任一已接受高度及原历史仍可定位和恢复 | [历史读取与 checkpoint](incremental-world-state-storage.design.md#iws-read-recovery) | 历史索引、archive source、受权远端取回 | 不以热缓存数量决定逻辑历史寿命 |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | 终态控制历史下冷后，原前驱、授权/去重结果和原处置仍可定位并支持后续工作 | [控制历史](incremental-world-state-storage.design.md#iws-runtime-state) | 历史索引、archive source、受权远端取回 | 不以热缓存数量决定逻辑历史寿命 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | 完整 owner/history、reader、未决与回滚闭包之外的冗余表示才可回收 | [保留与 GC](incremental-world-state-storage.design.md#iws-retention-gc) | 各 store owner、reader lease、提交和维护 fence | 不按年龄、磁盘压力或仅“未在 pins.json”删除 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | 候选/新增引用先受根保护，提交发布与退休使用同一 fence，避免 mark 后新增根被删 | [发布 fence](incremental-world-state-storage.design.md#iws-commit-publication) | 各 store owner、reader lease、提交和维护 fence | 不按年龄、磁盘压力或仅“未在 pins.json”删除 |
| Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005) | 新根选择仍满足原 authority、完整根集和必要独立持久域；旧写者不能推进 | [提交与耐久发布](incremental-world-state-storage.design.md#iws-commit-publication) | P2P/authority、部署 profile、ops 真实故障域 | 同机共享或本地双目录不证明多域耐久性 |
| Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005) | 物理共享和回收不合并必要独立域，也不将尚未合格的远端副本视为替代保障 | [分域回收](incremental-world-state-storage.design.md#iws-retention-gc) | P2P/authority、部署 profile、ops 真实故障域 | 同机共享或本地双目录不证明多域耐久性 |
| Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0；[REQ-LWS-006](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-006) | 新格式启用后，重启、物理维护和后续格式演进保留全部已接受历史及原未决请求 | [启用和恢复边界](incremental-world-state-storage.design.md#iws-activation-recovery) | Runtime、发行能力声明、原提交状态查询 | 不要求回退旧格式 binary，不提供旧世界导入 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | allocator、scheduler、授权/去重与 effects 完整，unknown 不重发补数据 | [运行控制根](incremental-world-state-storage.design.md#iws-runtime-state) | 执行事务、模块调度、外部效果账本与 provider 幂等条件 | state root 相等不代签续跑，unknown 不重发补数据 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | 恢复后继续接受并完成合法新工作，不重做已确认效果；root 相等不能代签续跑 | [读取恢复](incremental-world-state-storage.design.md#iws-read-recovery) | 执行事务、模块调度、外部效果账本与 provider 幂等条件 | state root 相等不代签续跑，unknown 不重发补数据 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | 业务、完整控制状态及原历史由同一事务接受和发布，崩溃不产生混合版本 | [原子发布](incremental-world-state-storage.design.md#iws-commit-publication) | 执行事务、模块调度、外部效果账本与 provider 幂等条件 | state root 相等不代签续跑，unknown 不重发补数据 |
| Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008) | 按页面、分片、事务和操作约束资源；超预算保留历史与原操作并明确退化 | [资源预算](incremental-world-state-storage.design.md#iws-resource-budgets) | 全部 writer/reader/transfer/GC、ops 容量 | 不放大整对象常量冒充无界恢复 |
| Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008) | 增长队列、分配集合及 effects/去重按键分页，受限不误判为空或触发另一效果 | [控制状态的有界访问](incremental-world-state-storage.design.md#iws-runtime-state) | 全部 writer/reader/transfer/GC、ops 容量 | 不放大整对象常量冒充无界恢复 |
| Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916；[REQ-LWS-009](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-009) | 等价业务与保留范围下，独立核验实际容量、增长、写读放大、RSS 和恢复成本 | [验证及证据](incremental-world-state-storage.design.md#iws-validation) | QA、ops、固定 workload/源码/设备 | 文档检查、算法估算和合成规模不代签生产收益 |

### 2.2 专业责任与输入

Runtime 拥有确定性执行和完整 write-set；WASM/模块 owner 定义控制状态及历史工件；DistFS 拥有对象读取、物理共享与保留闭包；P2P/authority 拥有合法 head、finality 和持久域条件；ops 提供真实容量、故障域与恢复约束；QA 分别验证内容覆盖、业务续跑、性能和故障。存储层不从位置、压缩成功或内容 hash 推导权限。

复用 [Runtime 提交恢复边界](../design.md#runtime-authority-commit-recovery-target)、[原生执行环境闭包](formal-world-storage-efficiency.design.md#des-lws-007)及[物理编码合同](formal-world-storage-efficiency.design.md#des-lws-002)。已有格式的生命周期与验证规则继续在原文维护，本设计不把既有实现状态扩大到新格式。

## 3. 当前状态、目标状态与差距

下表是固定源码 `112a627e889cb0601ed368b2c0be204e39e7554a` 的只读核查；线上容量取样仍以 #5196 记录的 `32a9bc7368be4af8bfd4fe05b7db44ab2274c183` 为准。两者都不是本设计的运行验证。

| 对象/能力 | 当前状态（基线） | 目标状态 | 差距/假设 | 证据或 owner |
| --- | --- | --- | --- | --- |
| replication checkpoint | bundle 保存完整逻辑 payload，再独立写 CAS 和 pin | 发布 root descriptor 及新增对象闭包 | 需改 checkpoint producer、consumer 和同步入口 | [bundle 路径](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_node/src/replication_checkpoint.rs) |
| CAS 压缩与 range | 全对象 zstd；大于 32 MiB 的 range 每次重新解压前缀 | 页面/分片独立压缩，按索引读取交叠区间 | 新逻辑接口不能绕回完整对象 Vec | [CAS range](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_cas/src/store.rs#L81-L117)、[缓存阈值](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_cas/src/compressed_range_cache.rs#L9-L10) |
| formal 物理共享 | 已有 min 4 KiB、mask 14、max 64 KiB 的确定性 CDC、frame/recipe；整对象/批次上限 64 MiB | 复用纯 codec，构建有界页/块接口与增量对象树 | 不能直接复用整包 API 或提高 LIMIT 解除新格式约束 | [chunker](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_distfs/src/controlled_authority/formal/physical_chunker.rs)、[physical](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_distfs/src/controlled_authority/formal/physical.rs#L17-L26) |
| roots 与回收 | replication orphan sweep 保护平面的 file index、effective pins、legacy cold refs | typed root 图保护树页、历史、工件、控制状态及活动操作 | 现有 sweep 不能直接覆盖新对象图 | [cas_retention](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_node/src/replication/cas_retention.rs) |
| 新状态承诺与事务 | 本设计的新根集、write-set 和 commit catalog 尚未实现 | 新创世明确版本，提交与恢复使用相同根集 | 全部新格式行为、兼容拒绝和规模证据待实现 | Runtime / P2P / QA |

## 4. 边界与结构

| 组件 | 拥有的内容 | 允许的职责 | 权限边界 |
| --- | --- | --- | --- |
| ExecutionTransaction / StateStore | 固定 parent roots、read-set、write-set、控制变更 | 一次提交内形成确定性 COW 页面和结果 | 不绕过 authority 选择 head，不以本地缓存补缺失前态 |
| HistoryStore / CommitCatalog | 原输入/事件/结果、提交 body/证书、连续高度与 checkpoint 索引 | 追加内容、认证定位与历史恢复 | 索引和存储服务不能自授 finality |
| ArtifactStore | WASM/native 原件、规则/schema/环境、外部结果大对象 | 同域按内容共享、分片读取与完整性验证 | 原引用、访问范围与执行环境匹配仍由消费者核定 |
| PhysicalStore / RootRegistry | 页面/块/pack 位置、owner roots、read leases、维护代 | 合并同域重复表示、压缩、读取和安全回收 | 不更改逻辑树形、业务 hash 或独立故障域要求 |
| Authority adapter | world/genesis、parent、候选、原提交证明和持久确认 | 调用原协议接受候选、原子选择获准根集 | codec、checkpoint、归档可读均不能代替该判定 |

状态树、history、artifacts 可以使用同一持久域的物理池，各自保留可审计 owner roots；execution 和 replication 从共享对象引用读取，不解压整包后再次压缩一份。同域共享不能使一个 owner 的热策略解除另一个 owner 的保留义务。必要独立域分别保存完整合格闭包，不能合并到唯一共享磁盘或 bucket。

纯分块/压缩/recipe 代码应下沉到 `oasis7_cas` 的基础模块或共享基础 crate。当前 `oasis7_distfs` 已依赖 `oasis7_cas`，不得反向依赖形成环；纯层不带 formal authority、提交锁或 `FormalError`。既有 formal adapter 保持自己的格式与预算，新 adapter 维护本设计的逻辑对象、资源及发布合同。

## 5. 关键运行流程

### 5.1 正常提交

1. 按现行 authority 固定 world、genesis、parent commit/root、版本及原请求身份。事务读取固定根，记录实际 read-set 和预期版本。
2. 原始外部输入先按其原权限和幂等身份持久化。执行产生业务与控制 write-set、事件及结果；拒绝动作若依规则仍产生事件或费用，也纳入同一 write-set。
3. 先生成独立 Input/Result/Event 原内容对象；将 write-set 按冻结规则合并并排序，COW 更新受影响树页，effects/去重等状态引用这些原件；执行工件写为独立对象。只 hash 新页、新块和变更路径，原内容与新根的历史绑定由下一步完成。
4. 形成新根集，将完整 put/delete 与控制变更封闭为 TransitionRecord，再产生 history prefix、结构化 checkpoint 和不含本次证书的 commit body。增量验证父子连续性及新增依赖，复用仍有效的父闭包证据；按 [提交发布](#iws-commit-publication)持久化、接受和选择 head。
5. 只有原协议已确认后才向消费者报告相应持久结果。外部派发由受保护 outbox 驱动，索引和后台维护不得重新触发原业务。

并发 parent/read-set 冲突在接受前拒绝或按原事务策略重试；重试保留原请求幂等身份，并重新读取获准前态。超时不证明未提交：先查询原请求及候选，不以新请求号重做。

一次合法提交只有一个 ExecutionTransaction。driver、runtime step、bootstrap、service intent 以及属于同一提交的 simulator 变更共享该事务的固定父视图、read-set 和 overlay；内层 prepare 不另建完整 World 副本或独立发布。父 head 保持不可变，失败丢弃未接受的世界差量，外部效果义务按 §6.2 独立处理。

<a id="iws-read-recovery"></a>
### 5.2 历史读取、checkpoint 与恢复

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；承接 [REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)、[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003)、[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007)。

每个已接受 commit 关联完整根集及本次原历史范围。checkpoint 是这些根和恢复边界的有界描述符，不导出全量 world snapshot，也不把所有历史引用展开成一个数组。checkpoint cadence 只控制检查、定位和预热成本；每高度的 commit/root 与 history 索引仍可查询。`hot_commit_height_window` 与 `hot_checkpoint_count` 是两个独立策略概念，不是已有配置名称或历史删除期限。

纳入 CommitBody 的可选 CheckpointDescriptor 按 §7 在接受前形成并持久化；缓存、预热和物理打包可以在接受后进行。加速任务失败本身不撤销业务提交，也不一律阻断后续提交；只有实际必需的恢复依赖、资源预算或原服务条件无法满足时，才按相应合同限制操作。

读者先取得当前授权、可信 head/目标历史位置和 read lease，使用 head 的已认证 catalog prefix 验证旧高度的 inclusion/range/absence proof；当前 head 项由其原提交证书验证。之后验证所需根集及执行版本，再按需访问状态树和历史段。分页游标绑定 world、目标 commit/root、schema 和查询范围；跨重启续读重新核验绑定。历史索引必须能证明其受支持前缀中的缺口或缺失，不能用未完成扫描的“未找到”返回业务不存在，也不能每查一个旧高度都重走它到 head 的全部 parent 链。

恢复可以直接装载目标高度的状态/控制根，或从较早 checkpoint 按已保存的原 TransitionRecord 重建非空后缀。后者逐高度验证 before roots、完整有序 put/delete、原输入/结果绑定及 after roots，不把业务事件列表当成完整 delta。两条路径均使用原模型结果、原模块/执行工件和原授权/去重材料；需要内容却缺失时停止相关恢复。新格式不要求通过一份展开 snapshot 才能恢复，但读取过的页面和范围必须逐项验证，完整恢复声明还需覆盖所需全部闭包。

两条恢复路径分别验收。checkpoint 后缀组固定 C<T，实际读取并应用连续 `(C,T]` TransitionRecord；不得直接挂载 T 的当前状态树、读取 T 的完整 snapshot 或由它回填缺字段来替代后缀应用。C=T 往返仅证明 checkpoint 读取，不能代签增量恢复。

保存/GC 闭包、一次提交验证闭包和一次读取/恢复闭包分别计算。常规启动或单高度恢复按可信锚/原协议要求核验目标 lineage、目标状态/控制页及实际需要的历史/工件，不因为 HistorySegment 的 parent 或 RootSet 引用递归展开每个旧高度的全部状态树。只有全历史审计与 GC mark 才遍历完整保留图，且按独立预算执行；缺少可验证的 lineage 快捷证明时仍按原协议分批认证所需链，不宣称启动成本与历史无关。字段的“内容依赖、历史定位/证明、保留边”类别由 typed schema 固定，不能由通用字符串 hash 扫描推断。

页面、历史段和大工件通过有界读取接口访问。大 value/artifact 使用带有序长度索引的 chunk 树，逻辑 offset 定位到交叠块，不从起点重解前缀。受信任对象根及路径证明可以认证新格式范围；只有传输 checksum 的分片仍须在接收端完成其逻辑对象/根验证，二者不能混称。缺页、损坏、预算不足、取消和权限失效分别可判定。

恢复后必须与不中断执行的对照分支接受相同合法新输入，核对新请求身份、分配序列、费用/结算、模块唤醒与实际工作结果。当前 root 相同、历史可读或所有新请求都被拒绝，均不足以证明恢复可继续工作。

## 6. 接口与数据合同

下列名称为目标概念，不是现有 Rust API、协议字段或 CLI。实现须冻结 canonical 编码、对象 tag、schema/layout version 和确定性规则，并提供同版本测试向量；任何 new root 都不得冒用旧 snapshot 或旧状态根的身份。

<a id="iws-object-model"></a>
### 6.1 对象、身份与引用图

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c；承接 [REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001)、[REQ-LWS-002](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-002)。

逻辑对象标识由冻结的 domain、对象类型、schema/layout version、长度与 canonical bytes 计算；world/genesis 和 authority 绑定字段属于相应根/提交对象的 canonical 内容。物理位置、压缩选择、pack 边界与缓存状态不参与逻辑身份。未知版本明确拒绝；不同类型不得仅因裸 bytes 相同就互相解释。

创世身份先于状态根形成：原激活权威先确定不依赖本次 commit hash 的 world_id 和创世配置；GenesisConfigId 对不含本次 RootSet、CommitBody/证书或自身 ID 的冻结配置求 digest。RootSet、history 和 descriptor 的 genesis 绑定指 GenesisConfigId。之后从初始内容构造根和 genesis CommitBody，得到不同的 GenesisCommitId，并按原 authority 接受为可信起点；它作为后续 head/证明的锚，不回填自身 body 的 hash 输入。初始 RootSet 不包含 GenesisCommitId，创世 parent 使用明确的空前缀语义，避免创世阶段另一条自引用环。

| 对象 | canonical 内容与引用 | 增长和校验边界 |
| --- | --- | --- |
| GenesisConfig | 预先确定的 world_id、初始内容/规则/格式与原激活配置，不含本次状态根和证书 | 先形成 GenesisConfigId，再构造创世根/commit；大配置内容以独立有界对象引用 |
| StatePage | namespace、规范键/值或 Merkle 子节点引用；大 value 指向块树 | 逻辑节点与叶值有界；物理页可打包多个节点，但不改变它们的身份 |
| Input / Result / Event | 原请求、原外部响应、执行结果或事件的独立内容；大内容指向块树 | 先于本次 RootSet 构造，不引用本次 root、history wrapper 或 commit ID |
| RootSet | world/genesis、height、format version，以及业务、allocator、scheduler、授权/去重、effects、artifact catalog 根 | 固定种类根集；模块扩展经版本化目录，不把全部实体/历史铺平 |
| TransitionRecord | parent commit、before/after RootSet、原输入/结果/事件 refs、按 namespace/key 排序的 put/delete 及顺序/前置条件 | 覆盖业务与全部控制变更；写入原历史闭包，不含本次 history root 或 commit ID；可分片但事务完整接受 |
| HistorySegment / HistoryPrefix | TransitionRecord 及原内容 refs、有序边界、parent commit、原请求身份、必要状态/工件 refs | 不可变段与增量有序索引；单大记录可分块，不能按 checkpoint 重写全部前缀 |
| Artifact / ValueChunkTree | 原字节或有序分块描述、逻辑长度、版本/环境绑定和块 hash | 每块及索引页有界；数据量大不要求整对象 Vec |
| CheckpointDescriptor | domain/version、world/genesis、height、parent commit、RootSet、history prefix、执行格式/环境 refs | 只引用结构，不嵌入展开 snapshot、历史全集或本次 commit 证书 |
| CommitBody | 原提交身份与 parent、前一已接受 catalog prefix 的 root/count/range、RootSet、TransitionRecord、history prefix、可选 checkpoint descriptor、原输入/结果范围 | hash 后按原 authority 签发/验证证书；不引用包含自身的 catalog root |
| CommitCatalog | height/commit ID 到 body/checkpoint 的确定性前缀索引，以及证书/receipt 的独立附属账本 | 前缀 root 被下一 CommitBody 绑定；当前项由当前原证书认证；附属原件保留但不引入自引用 |

状态承诺按 namespace/key 分区，复用现有[可验证 Merkle 索引及 v2 最终新可达节点策略](formal-world-storage-efficiency.design.md#66-merkle-v1-与-v2)的构造/验证基础，具体实现入口见[business_index_v2](https://github.com/nil-studio/oasis7/blob/112a627e889cb0601ed368b2c0be204e39e7554a/crates/oasis7_formal_protocol/src/controlled_authority/formal/business_index_v2.rs)。不另造 hash 算法或重复实现一套稀疏树。现有业务索引的 append-only 叶语义不自动支持任意状态更新；新适配必须显式定义 namespace、规范键编码、put/delete 与缺键规则，保持父根认证和独立 transition 验证，不能放宽原索引 verifier 冒充复用。

一次事务合并同键写入后按稳定 key 顺序批量更新，只输出最终新根仍可达、且经认证父树不存在的节点；不输出同批被覆盖的临时路径。是否已存在以父树证明判断，不由任意本地 CAS 命中决定。相同前态与有序输入必须产生相同新根；线程完成顺序、缓存、物理页面 split/merge、pack 或压缩策略均不得改变逻辑 root。物理页分裂和 compaction 只改 locator；若确需改变逻辑构造规则，须显式逻辑版本及合法转换。

逻辑 chunk 的切分规则、inline 阈值、空值/末块表示、块顺序和索引构造由同一格式 profile 冻结，具体参数按 §12.2 确定。相同 canonical 内容在该 profile 下必须形成相同块树，不受 I/O 缓冲大小、线程完成顺序、运行配额或物理压缩影响。逻辑块不能因预算不足而临时改边界，descriptor 以有界树承诺顺序、范围、长度和块身份。

可变大 value 的身份由该 typed 块树承诺，微更新只重算实际改变的块和路径，不要求每次再计算整值 flat hash。原工件/外部响应的完整内容摘要在首次导入时流式计算，原件不变则复用；实际改变全值或改变编码布局时，工作量按真实变更计费。

chain height、simulation tick、Tick record sequence、事件 era/序号和物理 generation 分别定义并校验。行政提交可以推进 height 而不产生 Tick；业务时间按原输入/运行规则推进，TransitionRecord 保留必要的前后边界。不能从 `Vec.len()`、归档数量或 simulation tick 推断提交高度，也不能把物理换代当成业务提交。

history 的逻辑段一经接受即不可变；小段可物理合包，已有段不因包追加而改变身份。当前尾部以已持久的独立记录/块及封闭边界追加；不能每次提交重写一个持续变大的段、offset 数组或索引 manifest。段索引、height/checkpoint 索引及 root catalog 都使用有界页面和增量路径更新；后台校验允许全覆盖，正常提交不扫描全部旧高度。

未接受 candidate 的准备、传播与合法裁定仍遵守原 authority 的替换规则。已接受原记录不就地覆盖；后续合法修订或补充证明追加并引用原记录，由指定 head 的有效视图区分原事实与后续处置。不可变段不取消原有合法裁定能力。

引用图必须避免 self-hash 循环。先生成不带本次 root/commit 引用的独立 Input/Result/Event 原内容对象；RootSet 的业务、effects、去重及工件目录只引用这些原件或已接受前缀，不引用本次 TransitionRecord、history wrapper、CommitBody、checkpoint 或证书。再构建绑定 before/after 根的 TransitionRecord 及其 history prefix；随后产生 checkpoint descriptor 和 CommitBody，最后得到本次 commit ID/证书。需要携带本次 commit ID/证书的 receipt 与证明放在独立 CommitCatalog 的受保护附属账本，不能回填同一个已 hash 的 RootSet/history/descriptor。

目录的认证边固定为：CommitBody_h 绑定已接受到 h−1 的 catalog prefix root、count 和连续高度范围；原 authority 的证书因此认证这个旧前缀。签发/验证本次证书后，再把确定性的 h 项追加到目录，形成下一次可引用的前缀。接受 h 时以认证 parent 的目录和增量 extension proof 核对前缀延续、world/GenesisConfigId、范围/计数与末项 parent commit；创世使用冻结的空目录根。读取旧项直接验证 head body 所绑定前缀的 membership/range/absence proof，读取最新项验证 head 证书；本地选择器记录的追加后 catalog root 还须与旧前缀加精确当前项一致，不能单凭它自授权威。

catalog 的逻辑条目只承诺确定性的高度、commit/body 身份和 checkpoint 定位，不把可能存在多个合法编码/签名集合的证书原件纳入该前缀 root。证书和已产生 receipt 的原 bytes/hash 在同组件的 proof/receipt 账本保留，按 commit ID 定位并按原协议验证；其 typed 保留边进入 root registry。这样不同节点持有不同但合法的证书表示，不会改变下次候选的逻辑前缀 root。后到的原结果/证明使用后续追加或 pending 账本，不修改已接受目录项。

TransitionRecord 的每项写入包含规范 namespace/key、put 或 delete、原值 ref/absent 前置条件及新值 ref；RootSet 的 before/after 覆盖业务和所有控制根。记录保留同事务原事件/结果顺序与拒绝/分配/费用相关语义，验证者从认证 before 根应用完整写集并核对 after 根，缺项、重复、冲突或顺序错误拒绝。原执行合法性仍由对应版本和 authority 验证，不由一份能重建 root 的差分自授。TransitionRecord 的分片、值、原输入与结果全部进入历史保留闭包，C<T 恢复不得依赖当时进程缓存。

原 operation/request 及候选执行的幂等身份用于绑定 parent、输入和恢复处置，不能与 hash CommitBody 后才得到的 commit ID 混为一谈。如果原协议使用 body digest 定位候选，该 digest 只在 body 形成后使用，不要求先生成的内容对象嵌入它，也不把 digest 写入自己的 hash 输入。返回玩家的完整 receipt 可以在提交后组合原 Result 与 catalog 中的证明，其原组件均受保留闭包保护。

新格式接收后，原逻辑对象 bytes/hash 不再因物理压缩或重排变化。复用 [frame/recipe 基础合同](formal-world-storage-efficiency.design.md#des-lws-002)的确定性分块、独立 raw/zstd、checksum 与有界解码；新对象树在其上组合页/块，不使用现有整对象/批次 64 MiB API 作为世界规模边界。若新增 locator/recipe 能力，使用显式版本，保留单块约束与旧 formal adapter 的原验收边界，不隐式改变其 v1。

<a id="iws-runtime-state"></a>
### 6.2 完整运行控制根

Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 [REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003)、[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007)、[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008)。

RootSet 的状态不能只覆盖玩家可见实体。实现必须逐字段盘点当前 Runtime/Snapshot 的语义及所有写入入口，为每项给出所属 namespace、原更新顺序、历史来源与恢复 oracle；存在未分类字段时不允许新格式创世启用。至少覆盖：

| 根/状态类 | 必须保留的内容与更新边界 |
| --- | --- |
| 业务状态 | 实体、资产/余额、设施、库存、任务/目标与控制关系、cognition、模块 instance 状态及版本；终态原文可在 history，仍有效的定位/控制依赖不可丢 |
| allocator | 实体/事件/请求的 next 值、era rollover、预分配 provenance、已预留 ID 及其消耗顺序、tick/时间推进；拒绝及回滚按原规则推进或不推进，不能从最大 ID 推断 |
| scheduler | 稳定 module instance 的绝对 wake tick 或明确 clear、module_tick_schedule、scheduler_cursor、有序队列、重试及等待条件；Pure 调用或无状态改变也保留调度更新，增长集合按键保存 |
| 授权与去重 | capability/委托、撤销/失效、nonce 与预算、原请求处置、receipt links、治理/回滚幂等 outcome 及必要 epoch/版本；历史控制前驱由相同闭包可读取 |
| effects | 原 operation/dispatch 身份、输入与原响应 refs、费用/结算、outbox 和已确认/未决/unknown 处置；未接受外部响应的持久 pending 账本也是独立保护根 |
| 工件与执行环境 | 原 WASM/native bytes、schema/规则、target/features、必要环境与模块绑定；版本名或摘要不替代原件 |

业务页、以上控制页和 history 由同一事务发布。对结果会产生影响的控制状态不能留在不受根约束的进程内变量或“可以重算”的缓存中。派生缓存可以丢弃，但必须从受保护内容重建并在恢复后验证；原模型回答和已发生外部效果不可重算。

reducer、授权、调度和模块输出通过同一事务的细粒度读写接口访问稳定 entity/field/key 或 chunk。每项持久字段只有一个主责 namespace；世界规则、manifest、版本 binding、治理/finality/rollback 等控制字段也须逐项归属。禁止先构造完整 owned World/WorldState 再 diff，或以 `Arc<World>`/`Arc<Vec<History>>` 的首次写时全复制代替增量；大 cognition/队列不能成为每次微改都重写的单叶。必要过程事件仍按原序保存，最终同键 mutation 合并不消除原收费、拒绝和调用事实。

终态控制记录下冷后，其原前驱、授权/去重结果和原处置仍由历史索引可定位，缩小热点不得使下一合法工作失去前置条件。队列、已分配集合及 effects/去重记录按键分页访问，恢复和查询采用 §8.2 的工作/并发预算；受限时保留原 operation 和控制根并明确进度，不能把暂未读完解释为“没有派发/没有授权”从而产生另一效果。

外部动作优先提交 outbox 后再派发。执行中必须取得外部结果的操作，在调用前耐久保存原 input bytes、operation/request/attempt、幂等身份、parent、授权/预算、模块/规则版本、已分配 ID 和可恢复 continuation；完成读回后才进入派发。原 response/receipt 按相同身份先持久化，再供事务消费。恢复 continuation 不重做已完成的 action prologue、折旧、费用结算或 validator 调用。

同步外调与 outbox dispatcher 共用派发前写前记录协议：首次物理发送前，独立 pending ledger 必须耐久写入并读回同一原 effect identity 的 `DispatchedOrUnknown` 标记，绑定 intent、attempt、authority 和单调 sequence；未完成时不得发送。标记已耐久但尚未发送、已发送但后置派发事实或 receipt 未落盘，恢复均按可能已发送核对。缺少后置记录不证明未发生，不能生成新 nonce 或重新调用来补历史。

pending ledger 至少区分 intent durable、DispatchedOrUnknown、result durable、已被获准提交消费及明确拒绝/取消处置，并独立登记保护根。世界事务未接受不抹除真实外部效果；超时、断连或响应丢失只按原身份查询或使用原结果，provider 无法核对时保留 blocked/unknown。没有外部幂等条件时不承诺物理恰好一次。较早形成的 RootSet 只绑定原 operation/attempt 和内容 refs，最终 commit ID/证明关联遵守 §6.1 的独立账本规则。

提交后的派发、未知或 receipt 事实先追加到该独立幂等 ledger，再作为原 effect identity 的输入由后继事务纳入 effects 根和原结果历史；不原地修改已接受对象，也不自行取得提交权。查询区分已提交 effects 与待确认投递事实。未消费或未解决义务保持根保护，世界 overlay 丢弃、读者超时和缓存淘汰均不解除它们。

### 6.3 跨组件接口

| 接口/条款 | producer → consumer | identity/version | ordering/idempotency | success/error | compatibility |
| --- | --- | --- | --- | --- | --- |
| Begin/ApplyWriteSet | Runtime → StateStore | world、parent commit/roots、request、schema | 固定 read-set/版本；有序批量 COW；同请求原结果 | 新候选、冲突、拒绝、预算不足 | 仅新格式，不调用旧全量 snapshot writer |
| PutPage/PutChunk | 各逻辑 store → PhysicalStore | typed logical ID、长度、codec 能力 | 不可变发布；同 ID 必须精确相同 | 已验证持久对象或明确错误 | 复用纯 codec，不借用 formal authority |
| ReadPage/ReadRange | 查询/恢复/同步 → 统一 reader | 原 root、对象/范围、read lease、版本 | 断点固定同一逻辑切面；只读交叠页/块 | 内容与验证范围，缺失/损坏/取消/受限分开 | 新格式消费者能力检查；没有旧 raw 猜测 |
| AppendHistory/Index | 事务 → HistoryStore/CommitCatalog | 原 position/request、连续范围、parent | 追加不可变记录；索引只更新受影响路径 | 完整历史边界或未完成；不得部分接受一笔事务 | 本次证书在 body hash 后独立追加 |
| PublishRootSet | authority adapter → root owner | candidate、parent、原证书、root set/catalog | 原提交判定、持久闭包、expected head；原身份查询 | NotCommitted、Committed、Unknown | 不引入新的共识胜者或自动回到旧世界 |
| Lease/Mark/Reclaim | reader/GC → RootRegistry | owner、operation、logical roots、physical generation | 先保护后读；发布/退休共用 fence；可恢复续作 | protected、reclaimable、blocked/unknown | unknown schema、缺边或预算耗尽停止对应 sweep |

接口错误不得把受限/缺页当业务不存在，也不得把局部对象写入成功升级为世界提交。超时和取消保留原操作身份；如果原提交结果已经不确定，消费者进入查询原处置路径。

<a id="iws-commit-publication"></a>
## 7. 状态、事务与持久化

Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c、O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；承接 [REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004)、[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005)、[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007)。

事务的本地阶段是读取固定前态、构建候选、持久化依赖、按原 authority 判定、选择获准 head、完成派生维护。阶段记录可有 pending/unknown，它们不替代原协议的 accepted/applied/persisted/qualified 含义。COW 只提供旧视图不被原地改写的条件，本身不证明提交原子性。

prepared guard 固定 world/genesis、精确 parent commit/roots、原 authority token/epoch 和事务自身单调 revision；同事务子操作绑定该 revision。所有影响确定性或持久结果的变更必须经事务并使相应 guard 失效，旁路 mutable 访问属于实现缺陷。校验直接比较这些身份与版本，不重新序列化/hash 全状态或历史，不只比较 tick，也不以物理 generation 代替逻辑 parent。

接受前完成可失败的候选校验和内存安装资源预留；耐久发布后的内存安装仅交换准备好的根/视图。若此间进程崩溃，从 durable head 继续。只有能证明 exact candidate 未被原 authority 接受时才返回 NotCommitted；丢弃 overlay 不构成该证明，已接受结果不能因缓存/通知失败变成可重做的新工作。

### 7.1 发布顺序与原子边界

1. 登记候选/pending roots；写入全部新增状态/控制页、TransitionRecord/history 段、工件、checkpoint 和 CommitBody。校验新增对象的逻辑 hash、长度与全部新增引用边；未变化子树仅复用与精确 parent/对象根、owner、持久域及有效物理代绑定的已验证耐久闭包证据，且这些依赖仍受相应 roots 保护。完成适用文件及目录同步。
2. 按原提交协议完成必要域的持久确认和证书判定。证书未明确前，不发布世界成功；多域不通过跨目录 rename 代替协议。每域都能独立读取其合格闭包。
3. 持久化原证书及 CommitCatalog 增量，验证 body 所绑定旧前缀与当前项的追加证明；在同一受保护选择边界核对 expected parent、候选和证书，原子选择包含 commit、RootSet、history 和追加后 catalog 前缀的 head descriptor。读者看到完整旧 head 或完整新 head，不看到业务状态新、控制/history 旧的组合。
4. 只有当前选择及其依赖完成耐久确认和独立读回，才报告相应本地发布成功。索引缓存、通知和物理维护失败独立记录，不撤销已经接受的业务结果。

原 authority 已接受但本域 selector 尚未完成时，重启查询并继续同一候选的本地发布；不能另造候选或自动服务旧 head。selector rename/fsync/读回不确定时保留候选、旧/新表示和原证明，报告 Unknown。Unknown 未收敛期间，在原提交域内隔离受影响 writer，包括重启后的写入准入；不得基于旧 head 推进可能与该候选矛盾的后继提交。仅在原身份的接受结果与本域 head 按原处置收敛后恢复相应写入。可信 head 和完整目录前缀以原证书/lineage 验证，单个文件路径、较大高度或新 mtime 不构成恢复选择依据。

正常提交的闭包验证是“新增节点/边验证 + 仍有效的父闭包证据”，不是每次重读全部已接受历史。证据必须来自实际成功的持久化/验证，并由 root registry、物理代发布和退休 fence 维持其依赖；任意 CAS 命中、目录 mtime 或一个未绑定的 verified 布尔均不够。根/owner/域/代改变、丢失/损坏或证据状态未知时，受影响路径重新验证或受阻；当前 authority、授权和服务条件仍独立检查。首次恢复及后台全覆盖审计按固定根分页执行，分别计量，不把它们的成本隐藏在每次增量提交或声称一次局部验证等于全历史审计。

### 7.2 故障与并发

| 位置/刺激 | 保留与处理 | 可观察结果 |
| --- | --- | --- |
| 接受前候选写失败、ENOSPC、read-set 冲突 | 旧 head 不变；保留仍承担原请求/效果义务的 pending 内容；可证明孤儿之后回收 | 明确未接受或原请求待核对，按原身份重试 |
| 证书/持久回执丢失 | 查询精确 world/parent/candidate；保留全部可能承载原结果的材料 | Unknown；不由超时推断回滚 |
| 证书有效而 head 未选中 | 重新验证同一获准闭包并完成选择；不足时限制服务 | 接受结果保持，本域发布/恢复受阻 |
| 创世已接受而本地激活未知 | 按原 world_id、GenesisConfigId、原创世 operation/body 查询并继续选择；保留原材料 | 不创建另一 world/创世、不把空 selector 当从未激活 |
| head 选择后崩溃 | 验证新 head 及闭包，恢复 pending/lease/维护作业 | 不重复外部效果，不回退已接受历史 |
| 同时 writer、GC 或物理换代 | 使用 owner 的固定发布/退休 fence；长构建/mark 在边界外，最终重核差集 | 先成功的引用受保护，已退休位置拒绝新引用并重试 |
| 读者持有旧根 | 旧逻辑根及所选物理代由 lease 保护 | 新提交可继续，GC 延后该表示回收 |

锁与事务 owner 在实现时按实际 crate 边界固定：纯 codec 不自行取得上层提交锁；现有非重入 CAS retention guard 不能嵌套调用再次加锁的 put/pin。跨 store 操作用 source lease 与 target 独立发布，不靠同时持有多个全 store 锁完成网络复制。任何等价 epoch 方案须证明同样的发布/退休顺序。

## 8. 部署、安全与运行约束

新格式使用原部署 profile 的 authority、故障域和服务闸门。维护 worker 只有绑定存储根的权限，不持业务 signer、不扩大历史读取范围。共享按原权限和域隔离，命中相同 hash 不自动授权读取；路径、symlink、文件类型、owner、边界及异常内容检查沿用相应存储合同。旧格式 peer/tool 不参加新格式世界的必要读写/恢复集合。

<a id="iws-retention-gc"></a>
### 8.1 保留、GC 与后台维护

Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c、O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；承接 [REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004)、[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005)。

root registry 至少保护新创世、所有已接受 commit/history 的可达根、合法分支/恢复证明、必要 artifact、未决 candidate/dispatch/unknown、当前与历史 checkpoint、活动 reader/transfer、维护 source/target 和受支持回滚根。可以从经验证的增量历史目录遍历全部祖先，不要求把所有高度展平到一个 pins 文件。

保留与替代证明按实际持久域核定。域内 owner 共享不能解除部署 profile 要求的另一独立域闭包；远端“能下载”不等于替代副本已满足原持久资格。回收前核验必要域及原 authority 的适用条件，缺失或未知时保留相关材料并停止对应优化，不由 GC 改变 serving 或写者资格。

typed mark 沿状态/控制页、历史段和索引、commit/certificate/receipt、工件/环境、块树及物理 recipe/frame/dictionary 遍历，记录 owner 和保留理由。逻辑历史在被接受后持续受保护；回收对象主要是未被任何根引用的未接受页、可替代物理表示和已完成作业的冗余材料。保留每高度的持久树根会保留该高度实际变化的页，这属于完整版本成本；不能用 latest-only GC 删除旧路径后仍宣称所有高度可读。

mark、校验、sweep 和作业清理按有界页推进。最终重核 roots/leases 与发布处于同一 fence；待回收表示先进入可恢复的 retire 状态，拒绝新引用，已有 reader 结束后再删除。未知类型、缺依赖、根变化、预算耗尽或 reader 状态不明只阻止相关回收并暴露原因。若 owner 根类型或引用边未知，且无法证明引用范围，则暂停整个可能共享的池/代的回收，不能把未能 mark 解释为无引用或据此缩小保护范围。refcount 可加速，必须可从原根重建与复核；时间经过、连接断开或 PID 消失不能单独证明可删。

checkpoint、height 索引增量更新，冷热按独立目标调度；后台压缩/打包/GC 只处理新变化或未完成作业，有吞吐、并发与空间预算，业务负载下退让。无变化时不反复重写同一历史；持续 blocked 记录具体 owner/root 与解除条件。逻辑小页可以物理合包以控制 inode 数量；一个 pack 含受保护成员时，先完整迁出并验证其替代表示再退休，实际释放以最后链接/持有者解除为准。

<a id="iws-resource-budgets"></a>
### 8.2 页面、事务与操作预算

Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 [REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008)。

新格式不以整世界或整 snapshot 的 64 MiB Vec 为接口。内存上界由页面、块、事务工作集与同时操作数共同决定，大对象和长历史通过有界树、流和游标处理；完整扫描、hash、校验和导出同样逐页消费，不能在末端重新聚合成覆盖全对象或全历史的 Vec。opaque 大 value、模块/执行工件进入有界 chunk 树，checkpoint 只保存其根，不列出全量块清单。首个实现启用前，Runtime/DistFS/QA 按冻结 workload 明确下表数值和计费方式；ops 给出设备容量余量与恢复目标。文档不虚构通用页大小、并发数或硬时限。

进程级预算统一发放可追踪 reservation，commit、读者、编码/解码、checkpoint、复制/恢复及 GC 的在途额度相加受同一上限约束，不能只给每个线程独立上限。分配、解码或扩大并发前先取得额度；reservation 随 buffer/Arc 所有权转移，缓存淘汰后仍被读者持有的内存继续计费。热尾、索引节点/descriptor、prepared guard、游标、proof、root inventory、mark 工作页与 visited 集均在范围内。

| 预算 | 必须覆盖 | 耗尽后的行为 |
| --- | --- | --- |
| 页面/块解码 bytes、项数与索引深度 | writer、读者、proof、cache、块树解析 | 接受前拒绝超限对象或按冻结规则分片；既有内容保留，读取报告受限 |
| 事务 dirty pages、read/write-set、输出与 history bytes | 所有业务/模块入口及拒绝路径 | 执行前可判定则拒绝；在途失败保留原身份，不能部分提交或漏记控制状态 |
| 查询/同步/恢复的总工作、并发、缓存/FD、取消/时间 | 本地/远端全部消费者 | 有界游标、背压与续传；未完成不报告全闭包 verified |
| GC/compaction 的读写量、mark 状态与目录元数据 | source/target、索引、临时文件和失败残留 | 暂停本页/作业并保护两侧；无新业务时仍可恢复未完工作 |
| 磁盘预留与暂态峰值 | 新 COW 页、history、双代/pack 重写、pending 与 reader 持有 | 提前停维护或按原服务合同限制新确认，不清历史换空间 |

页/块预算是接受格式的一部分或显式能力版本；操作配额可以按设备配置，但不能改变确定性树形、canonical hash 或业务结果。大逻辑对象超过单页大小时走分片路径，不能把现有 LIMIT 改大后称为流式。物理 raw/zstd 不值得压缩时可保留 raw；所有 header/index/recipe/pack 开销均计入预算和容量。

资源申请采用固定顺序，或在阶段开始一次预留该阶段所需上界，避免 worker 持有 decode 额度再互等 publication 额度。为提交结果核对、义务恢复、释放和进度持久化保留专用额度，后台维护不能耗尽它。未知大小输入按有界块申请并检查累计上限；无法继续时保存原操作及已验证范围，不先全量分配后检查。

写入在私有 staging 中流式编码、核验逻辑长度/hash 并完成耐久发布，成功后才产生可供提交引用的对象。读取按页/块/proof 完成验证后才向 authoritative reducer 提供内容；仅传输校验未完成逻辑认证的 bytes 不可提前使用。需要完整摘要后才能认证的大对象可以落到受预算控制的临时文件，再按页消费，不能退回完整 Vec。解压累计输出、临时磁盘和 encoder scratch 同样计费。

取消在有界工作边界释放未发布 buffer/普通 read lease，保留原 operation、固定根和已验证进度；下次重新核验授权及根后续做，不切换成 latest。派发或发布已开始的 unknown、未解决 obligation 和维护 source/target 保护根不因取消消失，可转成小型持久引用。错误说明限制维度、完成范围与当前提交判定，不能将受限解释成历史不存在。

应用计费不等于 RSS。allocator retained/fragmentation、线程栈、WASM/模块缓存和映射分别归因，尚未受该预算覆盖的部分给出独立上界或隔离策略。status 报告 hot/total history、index/cache/reader-held bytes、各类 reservation 与峰值、changed nodes/bytes、cold read/decode、backlog 和 unknown 数量；指标采集自身有界，不为打印统计遍历全历史。

## 9. 质量与容量

令 S 为当前有效业务/控制状态，H 为累计已接受历史，Δ 为本次真实变更及原结果。目标更新量随受影响叶页及其树路径、新历史和新工件变化，不固定按 S 或 H 扫描。若一次交易确实改全状态，或页面分裂/新工件很大，成本仍可相应增长；合法去重/控制前驱查询也可能需要冷点读。该模型不是严格 O(1)、已测增长斜率或压缩率。

| 场景 | 环境、规模与资源 | 预期响应 | 指标/阈值来源 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- | --- |
| 固定 S、Δ 扩大 H | 合成 13,792、55,168、220,672 Tick；固定当前状态、模块、热窗、每步负载和并发，历史超过热窗 | load/idle/commit/save/checkpoint/range 不展开或深拷贝累计历史 | history deep-clone=0；热驻留/临时峰值符合冻结预算；记录新页、扫描/解码字节和 p50/p95/p99 | [验证设计](#iws-validation)的容量组 | 未实现验证 |
| 固定 Δ 扩大 S | 相同少量 entity/key 更新，增加无关活跃实体与大 cognition 子树 | 不构造/clone 整个 World 再 diff；只改相关对象和路径 | changed nodes/bytes 与真实 Δ 对账；必要业务扫描独立归因，计量峰值/保留工作集 | [验证设计](#iws-validation)的容量组 | 未实现验证 |
| 实体/历史/工件增长 | 多档数据量，包含逻辑对象超过旧 32/64 MiB 边界 | 分页/分块可读；实际新增历史正常增长 | 页/块/并发预算、峰值 RSS、metadata/inode、冷/热恢复成本 | [验证设计](#iws-validation)的预算与恢复组 | 未实现验证 |
| 相邻版本与范围读取 | 固定范围查询、随机/顺序读取 | 读取/解压与交叠块及证明路径相关，不随 offset 重解前缀 | logical/encoded read、decoded bytes、cache、CPU、响应分位数 | [验证设计](#iws-validation)的对象组 | 未实现验证 |
| 满盘、中断与并发维护 | 固定文件系统、真实进程、reader/pending 同时存在 | 原结果不丢、未知可查询；冗余材料恢复后收敛 | source/target/临时/reader 残余全路径 allocation 与恢复时间 | [验证设计](#iws-validation)的事务/GC 组 | 未实现验证 |

容量口径至少区分：活跃逻辑状态、完整历史与原工件、唯一逻辑页/块、编码文件长度、文件系统 allocated bytes、device/inode 去重、owner/pin 保护量、pack 未释放量和维护峰值。跨故障域副本分别计入；应用写量、设备 I/O、页缓存读取、解压扫描、CPU 与 RSS 分别报告，不互相代替。

按 load、idle、commit、save/checkpoint、range 和 restore 阶段记录 live heap、分配总量/次数/可用调用栈、峰值 RSS/PSS、allocator retained、CPU、逻辑读写与实际设备 I/O；连续多轮记录峰谷，区分临时放大、仍被持有的对象和未归还 allocator 的内存。固定 source/tested tree、seed、原外部结果、机器/文件系统、进程/线程/allocator、warm/cold 条件和采样窗；S、Δ 的真实增长另组呈现。历史窄读允许必要页与证明路径，不把所有负载 cold decode=0 当验收。

比较分两种：同一新格式历史的物理优化比较必须逐对象同 bytes/hash；新格式与旧全量表示的架构比较使用冻结的等价业务输入/原外部结果、状态与控制语义 oracle、同保留范围及持久保障，明确两种格式身份不同。后者不是旧运行世界的无损迁移证据，也不能把删除旧世界材料计入收益。容量、正确性、恢复代价和故障域保障分别判定。

<a id="iws-activation-recovery"></a>
## 10. 兼容、迁移与回滚

Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0；承接 [REQ-LWS-006](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-006)。

首个实现只用于明确选择新格式的新建世界。创世/激活描述绑定新状态承诺 domain/schema、树布局、历史格式和必要消费者能力；独立目录/命名空间避免被旧程序误认为旧 world。新格式 reader/writer、同步、查询、恢复、校验与运维工具须完成能力矩阵后才允许该新世界启用。遇未知版本应明确停止，不猜旧 raw、不双写旧索引、不自动导入或覆盖已有世界。

Open 先只读识别有界 header/selector：旧格式、未知 family/version，或无标记但已有世界/record/identity 材料，返回 UnsupportedFormat；识别出的新格式损坏返回 InvalidFormat/IntegrityError。拒绝必须发生在 bootstrap、清理、恢复回写之前，源目录 bytes/身份不变。完全空目录的 Open 返回 Uninitialized，不隐式 Create；新建须显式请求并核对原激活身份，目录为空不授予重新创世或替换既有世界的权限。

首个创世被接受后，本世界身份和全部后续接受历史持续有效。创世接受回执丢失或本地激活 Unknown 时，按原 world_id、GenesisConfigId 和原创世操作查询，继续同一获准 genesis commit；不得更换配置/身份或以空目录判断从未接受。重启按原 authority、受保护 head 和所需验证闭包恢复；存储维护换代继续使用可验证 source/target、expected generation、耐久选择、reader pins 和 Unknown 保留原则。后续软件升级若引入新逻辑格式，需要显式的本世界版本转换和相应消费者合同；不把新软件启动当成完成转换。

不提供到旧 snapshot/CAS 二进制的回退承诺。程序回退只允许到能验证已接受新格式历史的已知实现；不兼容时限制受影响服务并修复，保留当前历史。物理 codec/pack 回退可以重建该新格式的原对象 bytes，须包含切换后的新业务并完成完整闭包验证；不能回到较早 head、取消已接受效果或以新创世替代。

建议实施顺序是：冻结新逻辑编码与运行状态盘点；实现 page/tree/history/artifact 基础和事务发布；贯通 checkpoint、查询/同步/恢复与非空续跑；补齐全部 root/lease/GC 和预算；执行固定规模、故障及必要域验收后，在实际授权范围启用。旧世界导入或处置是另一项明确范围的工作，不由这些步骤默认取得授权。

<a id="iws-validation"></a>
## 11. 验证设计与可追溯性

Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916；承接 [REQ-LWS-009](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-009)。

复用既有 V-LWS 验证类别，按下表增加新格式场景；这些是待实现方法，不是已存在或已通过的测试名称。实现 PR 固定 source/tested tree、format/layout/codec、输入 seed/原外部结果、profile、设备/文件系统、规模、并发、热窗和测量窗口，再选择所属 crate 的合同/故障测试及 [V-LWS 手册](formal-world-storage-efficiency.manual.md#v-lws-capacity)中适用的实际环境方法。缺少工具或环境如实记录，不以文档检查代签。表中手册链接提供既有验证类别与方法来源；各行均为新格式新增场景，待实施，既有命令不自动覆盖本设计。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | [对象图](incremental-world-state-storage.design.md#iws-object-model) | 新创世至 head 的所有接受内容及 typed 依赖完整 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)（新格式新增场景，待实施）：全 commit/height/root/对象覆盖与分支/版本检查；逐类移除 page/history/artifact/证书验证拒绝；required 对象 fixture，full 原件与引用图审计 | 完整覆盖清单、原 bytes/hash、typed 引用与缺口分类 | 抽样业务恢复不证明全对象覆盖；不覆盖旧世界导入 |
| Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1；[REQ-LWS-001](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-001) | [恢复](incremental-world-state-storage.design.md#iws-read-recovery) | 目标高度可由原状态/控制根及实际依赖恢复，不以外部重算补缺 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)（新格式新增场景，待实施）：固定目标高度执行跨进程及归档恢复，核对原内容、执行工件和依赖；断开必要来源验证阻断；full 冷恢复环境 | 目标位置、所需依赖覆盖、原内容与恢复结果对照 | 抽样业务恢复不证明全对象覆盖；不覆盖旧世界导入 |
| Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c；[REQ-LWS-002](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-002) | [格式身份](incremental-world-state-storage.design.md#iws-object-model) | 新对象 canonical/domain 身份及物理重建保持，引用图无自环 | [V-LWS-CODEC](formal-world-storage-efficiency.manual.md#v-lws-codec)（新格式新增场景，待实施）：相同 parent/有序输入重复生成根；物理页 split/merge、不同缓存/pack 后 root 不变；put/delete、最终新可达节点集合、type/domain/version、同次证书循环负例；冻结 profile 的 inline/块边界、空值/末块及不同 I/O 缓冲的同根向量；raw/frame/pack 往返、跨块 range/proof、损坏/截断；required corpus，full 实际文件系统 | 冻结编码向量、根和 bytes 差分、拒绝结果、范围读取/解压计数 | 新旧 state hash 不要求相等；局部 codec 不证明世界恢复 |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | [checkpoint 与历史](incremental-world-state-storage.design.md#iws-read-recovery) | 独立热窗/cadence 不减少可定位历史，索引无遗漏或全链扫描 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)（新格式新增场景，待实施）：运行跨两类热窗，改变 cadence/缓存后逐高度查询；旧 checkpoint 与完整 TransitionRecord 的 C<T 非空后缀恢复，禁用 T 当前状态回填捷径；固定旧高度/范围而增长 head，证明节点与读取量随目录树路径变化；缺页/远端断开不报业务不存在；full 冷恢复 | 连续高度/对象覆盖、cold 恢复、索引证明节点与读取/扫描计数 | 近期 checkpoint 或小历史 fixture 不证明全部规模 |
| Trace refs: O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；[REQ-LWS-003](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-003) | [控制历史](incremental-world-state-storage.design.md#iws-runtime-state) | 控制历史下冷不丢原处置或后续工作的前置条件 | [V-LWS-HISTORY](formal-world-storage-efficiency.manual.md#v-lws-history)（新格式新增场景，待实施）：控制记录跨热窗后定位原授权、去重和 effect 前驱；从冷根恢复并执行依赖这些前态的新工作；required 控制 fixture，full 模块/业务路径 | 原控制记录与授权/去重处置、冷读结果、后续工作结果 | 近期 checkpoint 或小历史 fixture 不证明全部规模 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | [GC](incremental-world-state-storage.design.md#iws-retention-gc) | 所有 owner/history/pending/reader/rollback 根保护，冗余最终收敛 | [V-LWS-GC](formal-world-storage-efficiency.manual.md#v-lws-gc)（新格式新增场景，待实施）：共享块多 owner、旧高度唯一路径、未决效果与 lease；未知 owner/边时扩大共享回收保护；中断退休/重启；required 竞态 fixture，full 进程故障 | root/保留理由差分、无缺边、最终释放与残余 inode | 单线程 sweep 或目录下降不能代签并发/完整历史 |
| Trace refs: O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c；[REQ-LWS-004](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-004) | [发布 fence](incremental-world-state-storage.design.md#iws-commit-publication) | 发布新增根与并发退休共同保护完整候选和 reader | [V-LWS-FAULT](formal-world-storage-efficiency.manual.md#v-lws-fault)（新格式新增场景，待实施）：final mark 后并发发布引用/读者，覆盖候选持久化、selector 与退休故障点；required 发布/退休竞态 fixture，full 进程中断 | fence 顺序、候选/reader 保留证据、原历史读回和无已引用对象删除 | 单线程 sweep 或目录下降不能代签并发/完整历史 |
| Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005) | [提交发布](incremental-world-state-storage.design.md#iws-commit-publication) | 根集和证书原子绑定，原 authority 和必要域不被本地发布替代 | [V-LWS-DURABILITY](formal-world-storage-efficiency.manual.md#v-lws-durability)（新格式新增场景，待实施）：各写/fsync/证书/selector 点中断，父冲突、错证书和旧 writer 负例；required 原协议适配测试，full 真实必要独立域丢失后恢复 | 原候选/证书/head、域 inventory、已确认与拒绝结果、Unknown 查询 | 本机双目录不证明独立域；不改变共识协议安全结论 |
| Trace refs: O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；[REQ-LWS-005](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-005) | [分域回收](incremental-world-state-storage.design.md#iws-retention-gc) | 域内去重与回收不削弱必要独立域闭包和替代副本资格 | [V-LWS-DURABILITY](formal-world-storage-efficiency.manual.md#v-lws-durability)（新格式新增场景，待实施）：同域多 owner 共享与跨域独立复制；移除或降级必要域、仅远端可下载但未合格的负例；full 原部署 profile 的真实独立域 | 分域闭包、替代资格、阻断回收原因与故障域恢复结果 | 本机双目录不证明独立域；不改变共识协议安全结论 |
| Trace refs: O7-REQ-87c0cd77-0b20-4eba-90ff-5a492e85b3a0；[REQ-LWS-006](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-006) | [启用恢复](incremental-world-state-storage.design.md#iws-activation-recovery) | 显式新格式准入，重启/后续维护保留原身份与全部已接受历史 | [V-LWS-MIGRATE](formal-world-storage-efficiency.manual.md#v-lws-migrate)（新格式新增场景，待实施）：新创世能力不完整拒启；创世已接受而本地激活 Unknown 时查询并续同一身份；旧/未知格式写前拒绝且源 bytes 不变，空目录 Open 不 Create；物理换代中断/新写后回退到支持该格式的实现；full 固定文件系统和进程 | 能力矩阵、NotCommitted/Committed/Unknown、切换后业务与历史对照 | 不验证旧 snapshot 导入或旧 binary 兼容 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | [控制根](incremental-world-state-storage.design.md#iws-runtime-state) | 完整控制根和 pending/outbox 保留；已确认效果不重派 | [V-LWS-CONTINUE](formal-world-storage-efficiency.manual.md#v-lws-continue)（新格式新增场景，待实施）：字段覆盖与单事务入口盘点；独立 J1 allocator/预分配/rollover、J2 stable instance/Pure/wake/clear，拒绝收费、授权撤销、去重、pending/unknown 与 continuation；required 独立控制 oracle，full 实际模块/业务路径 | 字段覆盖、新 ID/顺序、费用/模块状态、原处置及模型/外部调用计数 | root 相等不证明续跑；provider 无幂等条件不证明恰好一次 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | [续跑](incremental-world-state-storage.design.md#iws-read-recovery) | 恢复后至少一条合法新工作完成，结果与不中断对照相同 | [V-LWS-CONTINUE](formal-world-storage-efficiency.manual.md#v-lws-continue)（新格式新增场景，待实施）：checkpoint 加 C<T 非空 TransitionRecord 后缀恢复后，向中断/不中断分支输入相同合法新工作；以独立 J1/J2 oracle 逐提交核对费用、分配、模块唤醒与外部结果复用；full 实际消费者 | 新工作完成、状态/事件/费用/模块结果、原请求及外部调用对照 | root 相等不证明续跑；provider 无幂等条件不证明恰好一次 |
| Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；[REQ-LWS-007](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-007) | [原子发布](incremental-world-state-storage.design.md#iws-commit-publication) | 业务、全部控制和原历史原子发布，失败不拆分处置 | [V-LWS-FAULT](formal-world-storage-efficiency.manual.md#v-lws-fault)（新格式新增场景，待实施）：在新增页、原历史、证书和 selector 各点及 §11.2 派发前标记/结果窗口中断；覆盖 stale guard，再恢复并继续新输入；required 根集原子性 oracle，full 跨进程提交与派发 | 各阶段完整旧/新根集、pending 原身份、无混合状态及续跑结果 | root 相等不证明续跑；provider 无幂等条件不证明恰好一次 |
| Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008) | [预算](incremental-world-state-storage.design.md#iws-resource-budgets) | 每个消费者按页/事务/操作有界，受限保留原结果与进度 | [V-LWS-BUDGET](formal-world-storage-efficiency.manual.md#v-lws-budget)（新格式新增场景，待实施）：每维 limit±1、逻辑数据跨旧 32/64 MiB、全局并发/Arc 持有/取消/重启、恢复预留及 ENOSPC；记录实际分配峰值、计费转移和访问范围；required 极值对象，full 端到端同步恢复 | 明确错误/续传游标、RSS/FD/CPU/I/O、源和 pending 完整性 | 只有错误码或只调大常量不证明有界行为 |
| Trace refs: O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；[REQ-LWS-008](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-008) | [有界控制访问](incremental-world-state-storage.design.md#iws-runtime-state) | 控制集合的大小不迫使完整物化，未读完不视为不存在 | [V-LWS-BUDGET](formal-world-storage-efficiency.manual.md#v-lws-budget)（新格式新增场景，待实施）：扩大 scheduler、已分配集合、去重/effects 并注入分页取消、预算不足和恢复；required 极值控制 fixture，full 端到端新工作 | 按键访问范围与内存峰值、原 operation/进度、无误判为空或额外派发 | 只有错误码或只调大常量不证明有界行为 |
| Trace refs: O7-REQ-cb320711-af0a-44f3-98d4-bc7e8c1d0916；[REQ-LWS-009](../../product/world-infrastructure/lossless-world-history-storage.prd.md#req-lws-009) | [质量与验证](incremental-world-state-storage.design.md#iws-validation) | 同保留/保障范围比较容量与增量工作，区分格式间语义对照 | [V-LWS-CAPACITY](formal-world-storage-efficiency.manual.md#v-lws-capacity)（新格式新增场景，待实施）：按 §9 两种比较冻结基线；固定 S/Δ 的 13,792/55,168/220,672 Tick 及固定 Δ 增长 S；报告各阶段 allocation/live/retained/RSS、CPU/I/O、全 store/域 inode 与 allocated、hash/编码量、分位数及恢复成本；full 实际文件系统 | 原始计量、版本/配置/窗口、完整性配套结果、未覆盖项 | 模型估算不代签收益，不能计入删除旧世界或独立副本 |

### 11.2 完整性与性能接受规则

先观察全量 checkpoint 基线在少量变更下的重复增长或扫描，再验证候选的增量工作；不得只断言代码用了树或 chunk。新格式正确性使用独立的业务/控制参考模型及原输入/结果，避免测试仅重复生产 write-set 实现。错误注入同时核对原请求、世界根、history、控制与后续真实动作，不能只检查函数返回值。

同格式独立参考分支逐提交核对业务/控制语义、原请求效果及承诺根；C<T 恢复分支再与不中断分支执行相同合法新输入。J1 allocator oracle 独立检查 next/era、预分配 provenance、消耗顺序及 rollover；J2 scheduler oracle 独立检查稳定 instance 的绝对 wake/clear、游标、Pure/无 state 变更调用和多个模块到期动作。两者不从被测 TransitionRecord 自报结果生成期望值；根相等、全部拒绝或零外部调用都不能替代应完成的新工作。

同步外调与 outbox 均在以下边界注入中断：intent 未耐久、intent 已耐久而派发标记未完成、DispatchedOrUnknown 已耐久而未发送、已发送但后置事实/receipt 未落盘、result 已耐久但未消费、head 已发布但响应丢失。检查原身份核对、continuation、NotCommitted/Committed/Unknown、独立 pending roots 和后续工作；提交结果 Unknown 未收敛时，在原进程和重启后尝试基于旧 head 的矛盾后继提交均须被拒，收敛后再验证合法新工作完成。外部调用/费用由独立 fake 或受控 provider oracle 统计。prepared 后同 tick 的状态/控制更新也须使旧 guard 失效，且不能触发全状态/历史编码。

容量组执行 §9 的三个 H 档及固定 Δ 增长 S 组，分别给出阶段内分配、峰值/保留工作集、CPU/I/O 和扫描量曲线；资源通过与正确性通过分列。并发 commit、慢读者、checkpoint、恢复和 GC 时耗尽每维预算，验证 Arc 持有仍计费、恢复预留可用、取消保留原义务且可继续；不能靠降低历史保留范围满足预算。

全范围对象/引用覆盖与代表性业务续跑独立通过，才能声明该固定候选/规模的完整恢复。必要域、性能、资源和消费者验证继续分别成立；未执行环境或用例列为未验证。软件、布局/codec、执行版本、对象类型、消费者或平台变化时复核受影响项，不用一次测试结果覆盖未来格式。

## 12. 决策、长期风险与未决问题

### 12.1 架构取舍

| 选择 | 理由及代价 | 复核条件与 owner |
| --- | --- | --- |
| 新世界直接采用显式新状态承诺 | 去掉旧整 snapshot 和旧 hash 兼容包袱，允许事务级增量；需要全部消费者贯通 | Runtime/P2P：任何启用或新逻辑版本；旧世界需求另定范围 |
| COW 状态树与追加历史分开 | 最新工作集与完整原历史分别读取，局部修改共享旧页；页路径和版本保留仍有真实成本 | Runtime/QA：dirty-key 分布、页放大或控制状态遗漏 |
| 树/块引用 checkpoint | 避免每代展开、序列化和压缩全世界；恢复依赖完整对象图与索引 | Runtime/DistFS：冷恢复成本、依赖规模或查询模式变化 |
| 证书独立 commit catalog | 消除当前证书/receipt 的自哈希引用，以上一 catalog prefix 的承诺支持范围证明，保留原 authority 认证 | P2P/Runtime：新的 proof/receipt 字段或目录范围证明 |
| 复用 pure codec，同域共享物理池 | 统一分块/压缩/范围读，减少重复复制；要求 owner roots 和删除入口一致 | DistFS：新 owner、codec、pack 或跨域部署变化 |
| 页级预算与有界后台维护 | 大历史不要求全量内存，维护可以续作；吞吐和恢复时间需要实测 | ops/QA：设备、规模、并发或 SLO 变化 |

### 12.2 实现前需要固定的参数与边界

Runtime/DistFS 在首个实现接受新创世前冻结 canonical 编码向量、domain/type/version、Merkle 键编码及 put/delete/排序规则、完整字段到 namespace 映射，以及 commit catalog 的前缀追加、范围证明和 GenesisConfigId/GenesisCommitId 边界。未完成时新格式保持不可启用，不能由暂定序列化器输出产生正式历史。

页面/块大小、fanout、事务工作集、并发/cache、history 段封闭、后台节奏和磁盘余量由实现候选的测量决定，记录实际选择与范围；它们不作为本文虚构的通用 SLO。逻辑布局参数与操作配额必须分开，前者变化需要显式格式规则，后者不能改变共识可见结果。

WASM/Runtime/外部效果 owner 共同确认 scheduler、allocator、授权/去重及 unknown 的完整语义；QA 在非空续跑和原效果不重派上独立审读。ops/P2P 提供实际独立域与恢复证据，缺失时只接受已验证的本地范围。本文提供新格式系统合同，不声明源码实施、原世界修复、生产重置或部署完成。

### 12.3 v1 状态页与局部树根编码

Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.1 的逻辑身份与 §8.2 的页边界。此处只定义状态页和局部状态树根；字段映射、其他对象和全部消费者完成前，§10 的新创世准入限制继续适用。

基础路径算法复用业务索引的 256 层稀疏 Merkle 构造，hash 为 BLAKE3。旧业务索引继续使用原 codec 和 append-only verifier；状态适配使用独立 domain，不将旧页解释为新状态。所有整数使用大端编码，digest 使用原始 32 bytes；对外文本 digest 只接受 64 位小写十六进制。

| 编码 | 冻结规则 |
| --- | --- |
| namespace | `u16`：business=1、allocator=2、scheduler=3、authorization/去重=4、effects=5、artifacts=6；未知值拒绝。完整 RootSet 另绑定 world、GenesisConfigId 和提交位置。 |
| key | 原始 bytes，允许空键，最长 1024 bytes；路径 digest 是 BLAKE3(`oasis7.iws.state-key.v1\0` + namespace + `u16` key 长度 + key)。事务先合并同键写入，再按原始 key bytes 排序，重复键拒绝。 |
| StatePage | `oasis7.iws.state-page.v1\0` + `u32` 后续 payload 长度 + payload；超 1200 bytes 拒绝。逻辑页与文件、pack 或 I/O buffer 无关。 |
| branch payload | tag=1（`u8`）+ depth（`u16`，0..255）+ left digest + right digest。 |
| leaf payload | tag=0 + namespace + `u16` key 长度 + key + value digest。v1 不内联 value；value digest 指向独立 typed value/块树，目标的类型、原件与闭包由 owner 验证。 |
| 空子树 | 深度 256 的 sentinel 为上述 StatePage header 和单字节 payload tag=2 的 hash；递归 hash 两个相同子 sentinel 得到上层空根。tag=2 不作为可读取 leaf。删除最后一个叶子回到空根，不落盘空路径。 |
| 局部树根 | `oasis7.iws.state-root.v1\0` + `u32(42)` + namespace + leaf count（`u64`）+ node root。空 count 与空根必须一致；该 digest 是 RootSet 的局部完整性承诺，不单独授予世界权限。 |

put/delete 均携带原 value digest 或明确 absent 的前置条件。构造与独立重建验证只读取实际 key 路径；输出按 digest 排序的最终新可达节点，排除认证父树已有节点和同批临时路径。物理池命中不改变输出。事务 pending bytes 是操作额度，超限报告受限，不改变逻辑树形或产生不同的成功根。presence/absence proof 最多包含 257 个有序节点；缺页、损坏或未完成证明不返回业务不存在。

冻结向量由 `oasis7_formal_protocol::incremental_state::tests::state_profile_vectors` 承载；独立全量 map/逐层重建 oracle 核对 put、替换、删除及新旧节点集合差。原业务索引的递归兼容 oracle 保留，用于核对共享路径算法调整不改变旧格式字节。

### 12.4 v1 大值树与有界范围读取

Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.1 的 ValueChunkTree 与 §8.2 的工作量/内存边界。逻辑值按固定 64 KiB 切块，只有末块可不足；空值有一个长度为零的叶块。索引 fanout 固定为 128，逐层从左至右组成等高子树；除末子树外，每个子树覆盖其深度的完整容量。最小高度的根不保留冗余单子节点层。物理 CDC、压缩或 pack 不参与该构造，不能由网络 buffer 或额度改变逻辑切块。

所有整数为大端，所有对象 ID 为原始 BLAKE3 32 bytes。值节点编码为 `oasis7.iws.value-tree.v1\0` + `u32` payload 长度 + payload。叶 payload 是 tag=0（`u8`）+ 原 bytes；索引 payload 是 tag=1 + depth（`u8`）+ 逻辑总长度（`u64`）+ child count（`u16`）+ 按原顺序排列的 child（`u64` 长度 + digest）。child count 为 1..128，depth 为 1..7，child 深度必须恰少一层；总长度、非末子树完整长度和末子树范围均核对。节点编码超过 65,600 bytes、未知格式、截断、尾随内容、错误长度或 digest 均拒绝。

ValueRoot 编码为 `oasis7.iws.value-root.v1\0` + `u32(41)` + 顶层节点 ID + 逻辑长度（`u64`）+ depth（`u8`），其 digest 是状态叶的 value 引用。总逻辑长度使用 `u64`，depth 最大 7；这不是一次操作可物化的长度。root 与各索引页通过 typed 边进入保留闭包。根校验、树节点校验、范围完成与完整闭包完成分别判定，窄读成功不代签整值完整或持久资格。

构造只保留固定块和每层至多 127 个待封闭引用；读取先按已认证索引选择交叠子树，再访问交叠块，不读取/解码全部前缀。输入长度明确绑定本次对象，构造不消耗其后的另一对象。范围越界和算术溢出拒绝；读取输出在操作成功前为临时结果，取消、缺页、IO/内存受限不能返回业务不存在或完整复制成功。

共享 reservation 基础位于 `oasis7_proto::memory_budget`，CAS 保留同一类型的重导出；写入、读取与缓存使用同一个进程池。值树写入先申请 512 KiB 工作空间，读取申请 128 KiB；编码输出在分配前另申请额度，sink/缓存/reader 接管其 charge，最后一个持有者释放后才归还。读取拒绝另一预算池或低于实际 Vec capacity 的 carrier。适配器的输入、输出、元数据和物理解码缓冲另按所属阶段计费；这些固定逻辑工作空间不替代 allocator retained、RSS 或设备 IO 测量。节点数、输入/encoded read bytes、encoded write/输出 bytes 作为操作额度提前扣除，恢复使用总额内专用预留；原 authority 与资格独立检查。

编码向量和独立逐层重建模型见 `oasis7_formal_protocol::incremental_value::tests`；实际文件系统的超 64 MiB 流与窄读方法见同模块 `streaming_tests`。这些组件证据不代替 §11 的新世界接线、完整历史、跨进程恢复、全进程计费和必要故障域验收。

### 12.5 v1 RootSet、执行事务与 TransitionRecord

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1 的单事务、§6.1 的完整差量及 §6.2 的控制根。这里定义共享事务与表示合同；Runtime 各字段和入口仍需按 §6.2 接入，不能以通用 overlay 已存在推定全部业务、allocator 或 scheduler 已覆盖。

RootSet 编码为 `oasis7.iws.root-set.v1\0` + `u32` payload 长度 + payload。payload 为 `u16` world ID UTF-8 byte 长度、原 world ID bytes（1..256 bytes）、GenesisConfigId（32 bytes）、height（`u64`）、simulation tick（`u64`），随后按 §12.3 namespace 顺序保存六个原编码局部树根；不得省略、重排或用 history/物理代替代其中一项。完整 RootSet 最多 1024 bytes，所有整数均为大端。RootSet 不包含本次 commit ID 或证书。

ExecutionTransaction 固定完整 parent commit/RootSet、原 operation 和 authority token/epoch；父 RootSet digest 在 begin 计算一次。所有嵌套入口借用同一非 Clone overlay。首次读按固定父根读取，并按 namespace/key 保存原 digest/absent；后续读看到同一 overlay 的写入。最终同键写合并，恢复原值的写可不进入最终差量，原事实仍按序保存。任何成功 write 或追加事实均推进进程内 revision，即使最终写回原值也使旧 guard 失效；失败的额度申请不产生语义变更。发生读页、格式、额度或内存错误后，该事务不能继续读写、派生有效 guard 或 seal；不得捕获错误后封闭已完成的部分写入。外部 pending 义务仍由独立账本持有。

prepared guard 绑定该事务独占且有独立 charge 的进程内身份、revision 和精确 parent/authority fence。校验只做固定长度比较，不再次 hash 全状态/历史，不把物理 generation 当作业务 revision。相同请求/父根的新事务不能复用旧 guard；guard 存活时其身份不能被释放后重新分配而造成误匹配。最终 seal 重新核对 guard 与现 authority adapter 给出的 fence，不因这些字段本身相等就跳过原资格/提交协议。

TransitionRecord 编码为 `oasis7.iws.transition.v1\0` + `u32` payload 长度。payload 顺序为 parent commit（32 bytes）、原 operation（32 bytes）、authority token（32 bytes）/epoch（`u64`）、before/after RootSet（各有 `u16` 编码长度），随后是 read、write、原事实三个序列，各以 `u16` count 开头。read 为 namespace（`u16`）、`u16` key 长度、key、原值；write 在同样字段后再带新值；值编码为 tag=0 的 absent 或 tag=1 后接原始 32-byte digest。事实编码为 `u8` kind（input=1、result=2、event=3）和原对象 ID，保留原执行顺序。

v1 单次记录最多 4096 个 read、1024 个最终 write、4096 个事实和 8 MiB canonical bytes；key 边界沿用 §12.3。read/write 按 namespace/key 排序且唯一，write 必须有相同原值的 parent read，最终 no-op 不进入 write。before/after 绑定同一 world/GenesisConfigId，height 恰增一，tick 不倒退但可以不变。未知 tag、过量 count、截断、尾随、错误排序/前置条件或重建后根不等均拒绝；原事实的业务正确性及原对象类型/闭包继续由执行与 authority adapter 校验，不由差量自授。

事务先申请固定 64 KiB 元数据/读页工作空间，再在每个 read entry、原事实和 guard 身份分配前获取共享额度；seal/验证在构建树路径与编码前按实际 dirty/read/fact 数申请保守上界，结果只接管其可见 Vec/String 容量及结构元数据 charge，临时额度随后归还。解码在序列分配前核对 count 与剩余 bytes，并另申请 owned entry 槽位；计费合并不短暂释放容量，跨池失败保留双方额度。该上界计费不替代 §9 的真实堆/allocator retained/RSS 测量；操作额度可低于格式上限，受限不得输出部分提交。

协议组件方法见 `oasis7_formal_protocol::incremental_execution::tests`：手工编码与冻结向量、控制根漏写/前置条件篡改拒绝、同 tick stale guard、额度边界，以及从 C 的页集应用非空 transition 后缀到 T 后再产生一致新差量。此类 fixture 不代签真实 Runtime 的 J1/J2、外部派发、提交资格、耐久发布或完整故障域恢复。

### 12.6 v1 历史对象、提交目录与查询证明

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-46fb6d36-e9c9-41d5-98df-200b93d68670；承接 §6.1 的无环身份、连续目录与 §5.2 的认证定位。以下对象使用各自的 `oasis7.iws.<type>.v1\0` domain、`u32` payload 长度和 BLAKE3 身份；整数为大端，object ID 为原始 32 bytes。Scope 为 `u16` UTF-8 world ID 长度、原 bytes（1..256）和 GenesisConfigId。optional digest 为 tag=0 或 tag=1 + digest；checkpoint pointer 为 tag=1 + `u64` height + descriptor ID，或 tag=0。普通 header 最大 4096 bytes。

| type / 编码顺序 | 绑定与边界 |
| --- | --- |
| `genesis-config`：world ID、format profile、初始内容、原激活配置、执行环境 IDs | 不含 RootSet、CommitBody、证书或自身 ID。配置先取得身份，创世 body 另取得不同身份；profile/执行材料和原激活资格由实际消费者核定。 |
| `fact-set`：scope、operation、`u16` count、原序 kind/ID | kind 为 input=1、result=2、event=3；最多 4096 项、256 KiB 编码 bytes。记录原事实顺序，不包含本次根、history 或 commit ID。 |
| `history-segment`：scope、height、optional parent、operation、optional before RootSet ID、after RootSet ID、optional transition ID、fact-set ID | 每提交封闭一个不可变逻辑段；创世 parent/before/transition 均 absent，后续均 present。正文大对象另走值树，不能重写累计尾段。 |
| `history-prefix`：scope、count、optional previous prefix ID、segment ID | count 为 height+1；首段无 previous，后续引用原前缀，不展开旧段清单。旧高度定位通过目录，不沿 prefix 逐代回走。 |
| `checkpoint`：scope、height、optional parent、RootSet ID、history prefix ID、format profile、执行环境 IDs | 形成于 body 之前，不包含当前证书或当前 commit ID；引用结构，不嵌入历史全集。 |
| `catalog-root`：scope、count、node root、optional last commit、optional last checkpoint pointer | 前缀精确覆盖 0..count−1；空根/count/last commit 一致，checkpoint height 小于 count。该描述符本身不证明 authority 或目录完整性。 |
| `commit-body`：scope、height、operation、optional parent、authority token/epoch、`u16` 长度和原编码 catalog-before、RootSet ID、optional transition、history prefix、fact-set、optional checkpoint、执行环境 IDs | height 等于旧目录 count，parent 等于旧目录 last commit；本次 body 不含追加后的目录根、当前证书或 receipt。证书/receipt 原件由独立受保护账本保留。 |

目录复用 §12.3 的共享 256 层 sparse Merkle 算法，使用独立 `catalog-node` codec。branch payload 为 tag=1 + `u16` depth + left/right digest；空叶 sentinel 是单字节 tag=2 的同 domain 对象。叶 payload 为 tag=0 + kind；kind=1 记录 scope、height、commit/body ID、optional parent、optional checkpoint 与 latest checkpoint；kind=2 记录 scope、commit ID、height。query key 为 `catalog-key` domain 下的 scope + kind（height=1、commit=2）+ `u64` height 或 commit ID 的 digest。节点最大 1024 bytes。一次扩展只插入本次 height 和 commit 两叶，禁止覆写，输出最终新可达且认证父树不存在的节点；CAS 命中不决定写集。

latest checkpoint 为本次 checkpoint 或精确沿用父目录 pointer；它使旧高度可直接得到当时最近 checkpoint，独立于热高度窗口和 checkpoint 缓存数量。已认证 count 外的高度可以按前缀边界判定 absent；count 内缺叶、断路径、损坏或 locator/height 不一致均为错误，不能返回合法不存在。认证前缀的来源仍是 §6.1 的原证书及 extension 验证，不得直接信任本地未完成索引。

`catalog-proof` 编码为 root digest、query（tag=1 + height 或 tag=2 + commit）、`u16` node count、按实际访问顺序排列的 digest + `u16` 原节点 byte 长度 + bytes。最多 514 个节点、每节点最多 1024 bytes、packet 最大 600 KiB；解码先匹配受权 root/query，再分配节点序列。height 点查最多 257 个节点，commit 点查先验证 locator 再验证 height，最多 514 个节点；证明不能缺页、带多余节点、重排或跨根/范围复用。构造/解码先申请 2 MiB 操作工作空间，验证申请 64 KiB，保留的 packet/条目接管实际容量 charge；IO/时间/取消由读者适配器在这些路径边界继续计费。

HeaderPreparation 校验 config、parent body、parent prefix、RootSet 和原 TransitionRecord 的 scope/高度/operation/authority/前后根绑定，再按原件 → segment/prefix → checkpoint → body 的依赖方向形成内容。Catalog extension 可以准备候选 bytes，但必须在原证书获准后才能成为已接受目录并由 selector 选择；任何本地 hash、对象写入或证明测试均不自授接受资格。固定编码向量、缺页/篡改负例及增长目录下固定旧位置的读取计数见 `oasis7_formal_protocol::incremental_history::tests`；它们不代签真实 authority、持久化、全量闭包、复制或故障域验收。

### 12.7 有界物理页发布

Trace refs: O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.3 的独立 frame 与 §8.2 的有界发布。`IncrementalPageStore` 是 DistFS 的单页物理适配器，直接承接 `ValueNodeReader`/`ValueNodeWriter`，不提供世界 head、接受证书或删除接口。大型原工件的完整对象接口、类型闭包、受保护根及维护仍须继续接入，不能以此替代。

每页保存原 canonical bytes 的 BLAKE3 身份，输入非空且不超过 65,600 bytes（64 KiB value data 加冻结节点 envelope 上界）。物理切分复用既有 gear v1：最小 4096、mask 14 bits、最大 65,536 bytes；frame 复用 level-3 zstd 或原 raw，压缩不能节省时保留 raw。两者规则抽到 `oasis7_cas::frame_codec`，既有 formal v1 recipe/selector 不改格式；新页不经过旧整包累计 API。decoder 限制 window log 为 16，按声明长度分配，并以有界最后一字节探测拒绝超长输出。

新页使用独立物理 recipe：`oasis7.iws.page-recipe.v1\0`、原 page ID（32 bytes）、原长度（`u32`）、frame count（`u8`），随后每 frame 的 stored/original digest、原长度（`u32`）和 raw=0/zstd=1 tag；尾部为此前 recipe bytes 的 BLAKE3。整数大端。recipe 最大 2048 bytes，最多 17 frames，总覆盖长度恰等于页长；未知 domain/tag、跨页 ID、过量 count、截断或额外 bytes 均拒绝。这是可替代的物理表示，recipe/压缩身份不代替逻辑页身份。

Create 只创建不存在的专用目录，Open 不创建或修复缺失目录、格式标记和固定 lease。受控路径要求原 owner、非组/其他用户可写的普通目录/文件，拒绝 symlink；Unix 打开最终文件使用 `O_NOFOLLOW` 并核对 inode。live handle 持有固定空文件的共享内核 lease，操作重新核对其身份；独占维护占用时 Open 受限返回，不按 PID/TTL 推断过期，不无限等待。相同 OS 用户绕过 lease 主动修改目录不在合作协议保障内。

每 frame 先在私有 staging 写入并 fsync，再以不可覆写 hard link 发布，sync 最终文件及所属目录后才清理本次 staging。全部 frame 耐久后才发布 recipe，随后重读重建并验证原 page hash；同 hash 的并发 raw/zstd 发布允许选择任一有效原表示，不能覆盖已选 bytes。中断残留只作为未引用材料保留，不由 Open 猜测补交或删掉。frame-only 残留不是可读页；错误/取消后的调用者须查询或重试相同原页，不能据错误推断磁盘未产生任何材料。这里不宣称整个业务提交的原子性或跨故障域耐久性。

路径元数据、单次 raw/压缩写入和物理解码分别先从同一共享池申请保守 workspace（512 KiB / 16 MiB / 2 MiB）；codec scratch、页/recipe/frame 缓冲包含在对应阶段，输出 Vec/Arc 保留独立 charge 至最后持有者释放。物理操作额度分别限制文件读写次数和 bytes，包括 recipe、frame、重读及最终 overrun 探测；取消在页/frame 边界检查，恢复可以使用总额内的专用 reserve。此保守准入不代替 native allocator、RSS、实际设备 IO、临时磁盘累计或维护峰值测量。

组件验收命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib incremental_page_store::tests -- --test-threads=1`。真实临时文件系统覆盖重开、跨逻辑块窄读、重复/并发表示发布、frame-only 中断重试、六个 frame/recipe 发布边界的真实进程 kill 与重开、缺失/损坏、危险路径、内存/IO/取消及 live Arc 计费。进程 kill 不等于断电；它们不代签断电、ENOSPC、整世界恢复、受保护根/GC 或 §11 的规模验收。

重复发布必须同步实际选中的完整 frame 闭包、recipe 和所属目录，再返回耐久成功。先前进程可能在 hard link 可见后、fsync 前退出，或另一个 writer 尚在完成其 barrier；“页已可读”不能证明这些步骤完成。此规则适用于同 ID 重试和并发 raw/zstd winner，不以缓存命中替代。六窗口 kill 回归同时核对重试确实经过选中 frame/recipe 的 sync barrier；无 barrier 的旧重试路径在 recipe 已可见窗口判红。

### 12.8 原不可变对象的流式物理表示

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.1 的原内容 digest 与 §8.2 的有界工件接口。`IncrementalObjectStore` 借用同一页池、预算与 live lease，保存历史 canonical 记录和原工件，不重新 hash 全部历史。首次导入按明确原长度读取并流式计算原 BLAKE3，后续对象 bytes 不被提前消费；期望原摘要不符时不发布 locator，已写页仅为未接受材料。

该能力显式 Create/Open：Create 在页池中创建 `objects` 目录和 `objects-format` 标记（原 bytes 为 `oasis7.iws.object-aliases.v1\n`），Open 不补建缺失能力或猜测中断创建已完成。页池原格式和 frame/recipe 编码保持；这是独立的物理对象定位能力，不激活新世界。locator 使用 `oasis7.iws.object-locator.v1\0`、原对象 ID（32 bytes）、原长度（`u64` 大端）、ValueRoot descriptor ID（32 bytes）及此前 bytes 的 BLAKE3；固定长度且不超过 256 bytes。locator 仍用 §12.7 的 staging、不可覆写发布和耐久 barrier。先形成全部值树页、descriptor 及原摘要，再发布 locator 并独立重读核对完整原内容。

locator 校验和与 ValueRoot 在这里只认证物理重建结构，不能直接授权原 flat digest 下的局部范围。原对象读取须验证 locator 绑定、ValueRoot 长度、全部所需原 bytes 及其完整原摘要；没有该证据时不开放 flat-ID range。已由合法逻辑根直接承诺的 ValueRoot 范围仍走 §12.4 的路径接口。`read_object_into` 的输出在最终成功前为临时结果，不能边解码边修改 reducer；调用者须使用已计费的私有文件/临时 sink。`read_object_owned` 在分配前申请完整记录上界及输出 charge，只在原摘要验证后返回，适用于有界 canonical 记录；大工件使用流式临时 sink，额度不足不能回退到未计费 Vec。

对象 metadata workspace 为 64 KiB，句柄保留 4 KiB 元数据准入，页/codec/输出沿用共享池和阶段计费。逻辑树与物理文件分别消耗显式操作额度，取消和恢复读取沿用原总额内 reserve；完整原摘要只能在第一次导入或实际读取完整原件时计算，不用 locator 代替原摘要认证。`persist_history_material` 逐件承接 `HeaderPreparation` 产物的事实集合、TransitionRecord、segment/prefix、可选 checkpoint 与 body，并核对其相互内容 ID；不构造全历史包。它返回内容 ID，不返回接受/完整闭包回执；RootSet、状态页、事实内容、执行工件与原证书仍由所属消费者提供并认证。

组件验收命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib incremental_page_store:: -- --test-threads=1`。覆盖实际 header producer、超过单页的完整 FactSet 原 bytes/ID、locator 被替换但校验和有效的拒绝、空值/精确长度、共享预算及持有者计费、三窗口原对象发布进程 kill 和原身份续做。超 64 MiB 重复工件组在 8 MiB 总 reservation、3 MiB 恢复预留下完成真实页存储导入与流式读回，整对象 owned 读取被内存准入拒绝；它只证明该重复 fixture 的有界路径，不代签随机数据容量、RSS、设备 IO、满盘、断电、世界恢复或真实故障域。

### 12.9 状态事务与目录的物理页接线

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1 的固定父视图、§5.2 的非空后缀与 §8.2 的共享计费。`StatePageSession` 把 `ExecutionTransaction`、`TransitionVerifier` 和 `CommitCatalog` 接到同一 `IncrementalPageStore`，返回仍持有原 reservation 的 `ChargedMerklePage`。共享 `MerkleNodeReader` 使用 `AsRef<[u8]>` 载体，遍历在借用 bytes 时完成长度/hash/深度检查和解码；不为了适配源而拆掉 charge 或复制完整节点，pending 编码页也直接借用解码。旧 business adapter 保留 Vec 接口与原 codec，全部 canonical 身份不变。

producer 在分配前核对具体 source 的预算池身份，拒绝用另一个池准备事务、验证 transition、扩展目录或构造目录证明；旧 source 仍按其原 caller-owned workspace 合同处理。会话先申请 4 KiB 元数据额度，物理页和 decoded carrier 仍按 §12.7 申请同池额度；读者保留载体时 charge 不能提前归还。状态和目录的最终新节点集合逐页转交，每次仅申请一个有界 transfer buffer，不复制整个 write-set；原 producer 的集合 charge 一直由其持有。普通与恢复会话显式选择同一总池中的 class，恢复页安装及所选表示的重读/sync 也使用该 class，不借普通额度挤占恢复预留。该 class 只决定资源准入，不授予恢复或 writer 权限。

共享 Merkle 层原有 String 错误合同保持；会话另保留首次原 `PageStoreError`，消费者在失败路径直接取得其 typed 原因，不从报错文本推断 Missing、损坏、额度或取消。缺少实际引用页返回错误；业务 absent 只来自经过验证的空路径。执行 read 错误仍按 §12.5 poison 原事务，错误后解除资源压力或取消标记不能使其 seal。会话不选择 head，不以页已发布或差量根相等替代原 authority、owner/域/代资格及完整闭包。

组件验收命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib incremental_page_store::state::tests -- --test-threads=1`。覆盖真实页上的 put/delete、重开后读取、从落盘原 TransitionRecord 重建 C<T 非空后缀（独立重放池只装 C 的状态页，不读取 T 的状态页），然后以完全相同读写产生一致记录和最终节点；另验证目录落盘重开后的 height/commit 证明、缺页/资源/取消后不输出可 seal 事务、载体保留计费，以及普通额度耗尽时的恢复原页安装。这是通用状态/目录组件的实际文件系统路径；fixture 的合成 parent 身份、原件与状态引用不构成原提交资格，不代签 Runtime 全字段、J1/J2、外部效果、完整工件恢复、世界发布、复制、GC 或规模性能验收。

### 12.10 Runtime 字段所有权与借用准备入口

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.2 的逐字段覆盖。字段所有权清单位于 `oasis7_world_state::incremental_projection` 与 `oasis7_runtime::world::incremental_projection`；它们直接穷尽匹配当前 WorldState、World 和 Snapshot，不使用 `..`，新增或移除字段必须同步分类才能编译。Snapshot 仅提供字段到真实 Runtime 源的关系，代码不为完成清单而构建 Snapshot，也不把其派生 last ID、历史计数或 journal commitment 当作 allocator/新格式身份。

`World::visit_iws_genesis_fields` 借用实际字段：map/set 按项提供，ledger/nonce 的嵌套 map 与权限集合按复合语义键拆项，module state/工件以原二进制切片提供；不会 clone World、完整状态、字节工件或历史。它只服务创世准备及字段覆盖核对，不能放入普通 commit/save。普通提交仍须由实际 mutation 入口产生 read/write-set，不能遍历全部字段再 diff、序列化或 hash 整个状态。本入口没有启用新格式、导入旧世界、选择 head 或发布接受回执的能力。

| 字段组 | 新格式语义所有权与准备边界 |
| --- | --- |
| WorldState 的实体、余额、产业/市场和模块 instance/state | 业务；nonce、原处置/settlement 去重与治理授权分别归授权根，next/era/order provenance 归 allocator，绝对任务时间和 delivery cursor 归 scheduler。模块市场所有权是业务，不因名称含 artifact 就授予工件或执行权限。逐字段明细以编译约束的清单为准。 |
| Runtime 的原 next event/action/intent/proposal 值与 era、治理 penalty allocator | 保留真实 next 值及顺序，不能用 Snapshot 的 last_event_id 或集合最大值重建。 |
| pending actions/effects、inflight、module schedule/cursor、capability 与 rollback 控制 | 有序队列按借用顺序提供，原 nonce/outcome/receipt link 保留；枚举位置只用于准备，不冻结成会在 pop 后整体重编号的运行态 wire。 |
| registry records/active、原模块 bytes、manifest/环境与 release 配置 | 工件/执行材料按项借用；借用视图不替代原 bytes、profile、scope 和完整工件闭包核验。 |
| 物流 SLA 与 threat heatmap | 现有消费者用它们参与后续 release/governance 决策，不能因旧 Snapshot 未携带就当作可丢缓存。新读者须保留或经精确根绑定重建，本准备清单归入确定性业务字段。 |
| module routing metrics、运行容量与 backpressure | 路由只取确定性计数；queue 容量和会影响后续 gate 的队列处置计数保留。墙钟 duration、热 journal 上限/淘汰量及局部 dispatch-blocked 观测属于进程/缓存策略，不进入确定性字段。各投影亦穷尽匹配子结构，不能静默漏掉新增成员。 |
| cognition | 拆分 scheduler、continuation/effects、idempotency、journal frontier；历史事件/原 response 走历史 owner。commit/receipt/root-view 和绑定原件走独立 proof owner，含 commit_record_id 的 retention store 走独立保留 owner，避免把本次证明回填 RootSet。未知顶层或 journal 成员、错误集合形状和 schema 明确拒绝。 |
| signer、编译/模块/订阅缓存、reader、当前 driver cursor 与进程凭据 | 只提供不含值的 process descriptor；secret 不交给 Serialize。需要的原执行环境和独立 authority/cursor 仍由相应 owner 认证，不能凭“不序列化进程字段”省略恢复前置。 |

visitor 的目标是语义分类与借用交接，不是已经冻结的字段 wire。sink 拥有其分配、磁盘和编码准入；历史/证明 callback 收到借用，不自动扫描或序列化历史，也不能被通用状态 sink 塞入 RootSet。cognition 原 DTO 的 finality/status/receipt 关联、当前 RootSet/commit 引用及其他复合控制字段，仍须完成适用版本的 typed context/codec 转换、mutation 接线和保留 owner 接线，才能允许新格式创世；清单覆盖或 callback 成功不代签这些条件。

局部验收：`env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --no-default-features --lib world::incremental_projection::tests -- --test-threads=1`。核对实际 World 字段归属、Snapshot 源关系、binary pointer 相同、逐项嵌套键、serde 省略空字段仍被覆盖、历史/证明分离、未知字段拒绝、sink 错误不修改源，以及改变墙钟/热历史策略不改变确定性字段。该命令未启用 wasmtime，不验证普通事务接线、J1/J2、真实模块执行、旧世界导入或完整世界恢复。


### 12.11 typed body 子字段与嵌套贡献失败边界

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1 的单事务与 §6.2 的逐字段读写。`World::contribute_iws_body_attributes_update` 是现有 body reducer 的新格式贡献入口：复用 `validate_body_kernel_view` 与 `PreparedBodyAttributesUpdate`，借用调用者的同一个 `ExecutionTransaction`，仅暂存目标 Agent 的 body/last_active 子字段。它不遍历其他 Agent、mailbox、intent、Snapshot 或历史，不安装缓存、不追加原事件、不推进 event allocator、不发布接受结果；这些仍是尚未完成的外层 driver 责任。旧入口继续原行为，新格式未激活。

body 叶按 WorldState / agents / body 拆分，属于 Business namespace，不能把整个 AgentCell（含 mailbox、receipt context）塞入该叶。key 为 `oasis7.iws.agent-body-key.v1\0`、`u16` UTF-8 Agent ID byte 长度和原 bytes；ID 非空，完整 key 最多 1024 bytes。value 为 `oasis7.iws.agent-body-value.v1\0`，随后依次为 mass_kg、radius_cm、thrust_limit、cross_section_cm2、last_active 五个大端 `u64`。body 结构用穷尽匹配约束新增字段；decode 严格拒绝错误 domain、长度与尾随。该叶不声明整个 agents 字段的其余成员已完成 lowering。

WorldState.time 独立于 simulation tick/commit height，归 Allocator namespace；key 为 `oasis7.iws.world-time-key.v1\0`，value 为 `oasis7.iws.world-time-value.v1\0` 加原 time 的大端 `u64`。贡献入口在共享 read-set 中核对该时钟与当前 body 的原 ValueRoot digest，才允许写新 body 叶；缓存过时、父叶缺失或同键已暂存其他值时拒绝，不凭进程缓存覆盖认证视图。前值验证以丢弃 sink 计算固定小值身份，仍消费实际逻辑工作额度；仅新值页交给实际 provisional sink。新世界 genesis 及恢复还须完成这些键与其他字段的一致 lowering，不能混用 §12.10 的语义 visitor 原 AgentCell 序列化结果。

`ExecutionTransaction::contribute` 包住一个嵌套 reducer/codec/storage 操作，任一错误或 unwind 永久 poison 整笔事务。内层 read/write 或内层 contribute 已失败，即使调用者吞掉错误并返回成功，外层仍失败，之前暂存的 debit 不能 seal。临时页可以成为待回收孤儿，但不能因此取得提交资格。入口先核对共享预算池，准备的 Agent ID 分配在申请额度之后进行。

`ValueStreamWriter` 为 serializer 提供 push `Write` 入口，以相同固定 chunk/fanout 构造原值树；调用 Write/flush 的边界不改变身份，flush 不提前封闭短 chunk。完整字段 bytes 不驻留，固定 frontier/buffer 先申请共享 workspace，再逐节点申请 charge；最大逻辑值长度、节点/输入/encoded bytes 额度和取消显式处理。write/flush 的失败和捕获的 sink/cancel unwind 均禁止 finish 输出 descriptor。上层 serializer 自身的失败也必须返回外层 contribute，不能捕获后以部分编码 finish。此接口目前按普通额度构建，不代签恢复导入或大可变值局部更新。

局部验收：`env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --no-default-features --lib world::incremental_body::tests -- --test-threads=1`；验证实际 World/prepared reducer、与外层 allocator 写共用一笔事务、原 clock/body 读取、新值 digest/transition 重建、无源修改，以及过时 body/time、sink、额度、取消、业务拒绝、跨池和缺失 Agent 错误后整笔不可 seal。协议组件另对 push/pull 的逐页身份、不同分片/flush、超 64 MiB push 在 1 MiB 共享池中的流式文件写入/窄读、错误/unwind 后不可 finish 和嵌套事务 poison 做回归。本段仍非完整事件提交、World driver、原 authority、J1/J2、持久化发布或规模验收。


### 12.12 body 原事件、allocator 与 mailbox 的同事务贡献

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1 的原执行路径与单事务。`World::contribute_iws_body_event` 接收原 `WorldEvent::Domain(BodyAttributesUpdated)` envelope，复用现有 body guard、prepared reducer 与 rolling event ID 预览规则，在调用者的同一事务内暂存 body、event allocator、mailbox append/frontier 并追加原序 event fact。其他事件形状明确拒绝。当前只完成这条 typed 事件路径；旧 publication、tick consensus、原 authority 与接受发布尚未切换，新世界仍未激活。

| 字段 | 固定编码与语义 |
| --- | --- |
| next_event_id 与 era，Allocator | key 为 `oasis7.iws.event-sequence-key.v1\0`；value 为 `oasis7.iws.event-sequence.v1\0` 加 next/era 两个大端 `u64`。从事务当前值派生 allocated/next/era，沿用现有 next 至少为 1、MAX 回到 1 且 era 饱和递增的原规则，不从 journal 最大 ID 推断。event.id 必须等于 allocated，event.time 必须等于经 §12.11 认证的 reducer clock。 |
| Agent mailbox frontier，Scheduler | key 为 `oasis7.iws.mailbox-key.v1\0`、kind=0、`u16` UTF-8 ID byte 长度和原 Agent ID；value 为 `oasis7.iws.mailbox-frontier.v1\0` 加 head/tail 两个大端 `u64`，要求 head≤tail。 |
| Agent mailbox 项，Scheduler | key 使用相同 framing、kind=1，并在 ID 后加稳定 ordinal 的大端 `u64`；完整 key 最多 1024 bytes。追加位置为旧 tail，要求认证 absent，再将 tail checked 加一；不得覆盖原项。消费移动 head，不能对剩余项整体重编号；消费入口仍须接线。 |
| mailbox 内容引用 | value 为 `oasis7.iws.mailbox-event.v1\0` 加原 WorldEvent 的 32-byte flat digest，再经通用值树取得状态叶引用。实际投递读取原 envelope 的 Domain payload；原件必须由 history owner 完整保留并认证，不能凭此引用或物理 locator 直接接受。 |

只读取 event sequence、mailbox frontier 和目标 body 三个定长值树，外加时钟原引用与追加位置的认证 absent。body 从事务当前引用读取，而不是从未安装的 World cache 读取；同一父事务连续两次更新同一 Agent 时，第二次看见前一次 body、allocator 和 mailbox 写，最终同键变化合并但两个原 event fact 都按序保留。队列 head/tail 无论数值多大都不扫描旧 mailbox；准备过程中不复制热 journal，不重新 hash 全状态或当前 tick 历史。

原事件使用现有 ABI canonical CBOR 编码，原 flat BLAKE3 digest 同时绑定 fact 与 mailbox 内容引用。该 typed envelope 只有固定 scalar 与 Agent ID、reason、可选 effect cause 三个字符串，先按其长度为原 canonical value model、排序临时空间和 growable output 申请共享额度；结构与变体穷尽匹配使新增动态成员必须重新审查计费。caller 的借用原输入另由其 owner 计费。操作最大事件 bytes 明确准入，实际 bytes 超限则失败；本路径不宣称通用巨大事件也已流式编码。原件 callback 在同池 charged bytes 仍存活时接收原 bytes/expected ID；callback 的职责是按期望原身份持久化，失败必须上抛。其成功不代替完整闭包或接受回执。原件发布或 fact 额度在所有状态写已暂存后失败，整笔事务仍 poison，不能 seal 部分状态。

局部命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --no-default-features --lib world::incremental_ -- --test-threads=1`。覆盖与旧 publication 产生相同的原 envelope/canonical bytes、四叶同事务变化、固定值读取次数、非零大 ordinal、event era 回卷、连续贡献、晚期原件/fact 失败、错误 ID/time/大小拒绝，以及 codec 边界。实际文件系统 fixture 将相同 Runtime 贡献接到 `IncrementalPageStore`/`IncrementalObjectStore`，重开后核对原事件 bytes/解码、transition 重建与下一次实际 body 贡献记录和新页完全相同。fixture 的初始字段/parent/authority 为测试构造，不是完整 World 创世或原提交资格；它没有证明全字段闭包、跨进程世界恢复、J1/J2、tick consensus、复制、GC 或规定规模性能。


### 12.13 Runtime driver 候选准备与材料交接

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；承接 §5.1 的单事务及 §7 的提交发布边界。`World::prepare_iws_commit` 在分配前申请同池工作空间，核对父 GenesisConfig、RootSet、CommitBody、HistoryPrefix 与 catalog tail 的 world/genesis、height/count、digest/parent/checkpoint 关系，再创建唯一 `ExecutionTransaction`。所有实际 Runtime apply/嵌套入口借用这笔事务，不复制 World，不读取热 journal，不进入旧 Snapshot publication。父材料必须由原 authority/root owner 提供认证；这里的结构验证本身不产生资格。

执行前还从父 body 认证的 catalog-before 重建该 body 的确定性扩展，与提供的追加后 catalog 精确比对。只剩最新目录项路径、但缺少其认证旧前缀依赖时也必须拒绝，不能以 tail 可读代替连续前缀认证。

事务的 parent fence 在 begin 固定并只通过固定字段取回；driver 在 apply 前、seal 时及历史/目录候选材料形成后调用原 adapter 的 live fence 读取并比较。任一父 head、完整父 RootSet digest、authority token/epoch 改变或 adapter 读取失败，均丢弃候选；apply 的外部错误/吞掉的内层事务错误/unwind 沿 §12.5/§12.11 失败边界处理，先前暂存状态不可部分提交。已持久的外部效果义务或 provisional 对象仍由所属 owner 保留，不由此处删除。

同一封闭 transition 的 after RootSet 交给 §12.6 的 HeaderPreparation 形成原 FactSet、TransitionRecord bytes、segment/prefix、可选 checkpoint 与 CommitBody，再从精确父 catalog 生成该 body 的增量候选目录。driver 提供的 next_tick 独立于 height；相同 tick 的行政候选仍按原事务规则使 height 恰增一。RootSet 与追加后 catalog descriptor 的原 canonical bytes 在编码前取得共享额度，随输出持有。`PreparedRuntimeCommit` 的 transition/history/catalog、原 RootSet bytes、catalog bytes 与 fence 均不可变；各组件保留原 charge，外围 carrier 只另收自身元数据，最后持有者释放后归还。

消费者可逐项把 charged transition/catalog 页交给真实页 store，把原 RootSet bytes、历史材料和 catalog descriptor 交给原对象 store；body ID、RootSet ID 与 catalog extension 保持同一候选绑定。这只是候选材料交接，`PreparedRuntimeCommit` 及其 commit_id 不代表已获准提交、完整闭包、耐久域证明、selected head 或 cache-install 权限。它提供固定字段的 `validate_live_fence`，后续协议与原子发布边界仍须再次核对，不能以准备阶段最后一次检查替代实际 CAS/fencing。原证书、独立持久域确认、合法目录选择、失败裁定与恢复仍待真实 adapter 接入。

既有 controlled formal `CanonicalRecordV1`/`ClosedRecord` 的 Snapshot 槽位要求原 Snapshot/schema 展开及原 root 验证，不能把这些 RootSet bytes 塞进去冒充已适配；旧 verifier 和 profile 未因此放宽。新世界仍未激活。候选准备链也没有完成其余 Runtime 字段/事件、tick consensus、复制、保留/GC 或全量验收。

局部命令：`env -u RUSTC_WRAPPER cargo test -p oasis7_runtime --no-default-features --lib world::incremental_body_event::tests::commit -- --test-threads=1`，覆盖真实 body event 经 driver 形成相互绑定的四类材料与原 fact、三个 live fence 边界冲突、坏父材料在执行前拒绝、源不变与输出/失败释放计费。`world::incremental_body_event::tests::physical::prepared_driver_material` 将相同 driver 输出接到实际状态页、目录页与原对象 store，重开读取 CommitBody/RootSet/catalog，核对旧父引用、目录新项与 transition 重建。fixture 的创世/authority 和字段集合为测试构造，未证明原资格、完整世界闭包、接受发布、跨进程世界恢复、J1/J2 或真实故障域。


### 12.14 原 controlled authority 的 IWS 格式绑定

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；承接 §7 的原协议、资格与独立持久边界。`controlled_authority::formal::incremental` 将 IWS 材料显式绑定到现有 controlled-single-authority 协议，不另设签名算法、端点角色、决定/complete 回执或降低独立域数量。原签名 domain、原 record_id、原 frozen/reservation 身份和 proof-complete 流程保留；schema 和 format profile 明确区分新旧材料。这里是协议元数据与内容引用绑定；实际端点的新格式物料闭包、落盘、恢复及服务资格仍未接通。

| 合同 | IWS 版本与绑定 |
| --- | --- |
| authority profile | 原 `controlled_single_authority` 的 profile_version=3；仍使用原 writer、两个独立实例/角色/域与原 issuer/fencer/witness 策略。只用于新建 IWS 世界；旧 profile 1/2 不能提交 IWS record，不能把旧世界直接升版导入。 |
| 初始授权 | InitialActivationBody schema=3；六个精确根角色为 IncrementalGenesisConfig、IncrementalRootSet、ExecutionManifest、Wasm、Rules、NativeExecutionEnvironment。初始 RootSet 与 world.genesis/config、manifest 精确绑定。不包含 IWS CommitBody 或 HistoryPrefix，避免授权证书与带当前 authority context 的 body 形成循环。 |
| 原 canonical record | schema=7 为初始/普通执行，schema=8 为原 durable-set transition 的执行边界；九个精确角色为上述六项加 Input、IncrementalCommitBody、IncrementalHistoryPrefix。RootSet 绑定 after_state_root，GenesisConfig 绑定 world.genesis_digest，Input 绑定原 payload_digest；不使用 Snapshot/Journal/NonceIndex/EffectOutbox/Result 槽位冒充新根。控制状态由完整 RootSet 持有。 |
| 原 frozen | FormalFrozenBody schema=4；签名、原父记录、request/payload、原 epoch/activation 及双端 frozen 回执规则不变。IWS parent 只能使用该 schema；旧 Snapshot frozen schema 不能用于 IWS，也不能反向混用。新格式 frozen_material 的有界完整验证仍须由新消费者接入。 |
| 身份映射 | 原协议 position=IWS height+1；原 canonical record_id 是协议身份，IWS CommitBody digest 是存储内容身份，两者不相等也不互相覆盖。原 record 的 typed artifact roots 绑定 IWS body/history/RootSet；IWS body.parent_commit 绑定父原 record 的 IncrementalCommitBody 根，原 record.parent_record_id 仍绑定父原协议 head。 |

`authority_binding` 从原 VerifiedAuthority 的完整 authorization-window context_digest 和 epoch 派生 ExecutionTransaction fence，而不是只取可重复的 authority definition ID。其 metadata hash 前申请共享预算；`verify_commit_binding` 同样先准入，再核对实际 GenesisConfig、六根 RootSet、CommitBody、HistoryPrefix 与原 record/authority/父 qualified record 的 scope、height/position、operation、epoch/context、native 环境与前后根引用。预算不足保留 typed Memory 错误，不变成材料损坏。该函数只返回绑定结果，不颁发新的 Verified commit 或已接受能力。

原 initial/execution candidate、frozen、双端 decision 和 proof-complete verifier 复用同一实现，并拒绝 authority/record、parent/child 或 frozen 的格式混用。新 schema 不改变现有 1–6 record 或 1/2 profile 的 bytes/身份；旧 Snapshot 完整包验证入口对 IWS record/frozen 明确拒绝，不能在 `schema>=3` 分支尝试展开 RootSet。尚未覆盖 IWS 的旧读取、端点和恢复消费者不能因元数据 verifier 已支持 schema 就取得服务资格。

初始 RootSet 形成于初始授权证书之前；其状态不能回填本次 activation/context、CommitBody 或证明。授权签发后才能形成带原授权 context 的 IWS body，随后形成原协议 record 和其独立 proof ledger 原件。原证书/回执作为原内容保留，不回填已 hash 的 RootSet/body。内容绑定还不等于完整物料验证：原 input/result/event bytes、FactSet/segment 与业务 roots、transition 重建、catalog 扩展、projection、WASM/native 历史依赖、frozen 写前材料、独立耐久域与原子 head 发布均须由实际 endpoint/Runtime 消费者完成后才可服务。

局部方法：`env -u RUSTC_WRAPPER cargo test -p oasis7_formal_protocol --lib controlled_authority::formal::incremental::tests -- --test-threads=1`。用实际 IWS HeaderPreparation 与空状态 transaction，经过原 initial 授权签名、frozen 双端回执、candidate/decision/proof-complete 验证并核对 typed 内容引用；覆盖角色缺失/混用、格式降级、缺一端 complete、错误角色回执、旧 Snapshot 入口拒绝、body 承诺替换和预算准入。测试使用独立测试 signer，不是实际端点落盘、真实故障域、完整世界激活或完整恢复证据；安全/兼容变更仍须独立协议与持久化评审后才可合入。


### 12.15 候选结构原件的有界读回验证

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-1d257860-b834-4e59-a785-5696ee5aa0f5；承接 §5.1、§6.1 及 §7 的实际内容验证。`incremental_material::verify_commit_core` 要求原协议已经验证的 candidate context，以及适用的父 qualified record；它从 `OriginalObjectReader` 读取由原 record 根指定的 GenesisConfig、RootSet、CommitBody、HistoryPrefix，再读取 FactSet、segment、可选 checkpoint/transition。每次先扣 object/原 bytes 额度，reader 在分配前使用受限 max，返回同池且保留 reservation 的原 bytes；校验长度、capacity/charge 与原 flat digest 后才解码。成功仅退还未使用的预留 bytes allowance，失败保留已准入工作量；物理读取、解压、重复验证等仍消费 source 自己的实际 IO 额度。

原对象接口由实际 `IncrementalObjectStore::original_reader` 实现，不从 locator 或 CAS 命中推定原内容。该 reader 借用原 ObjectIo 的物理/逻辑额度，保留 typed PageStoreError。当前为普通候选准备路径，要求同池 ordinary source；Merkle reader 显式提供 allocation class，实际 StatePageSession 返回其已选 class。恢复 class、未知 class 或跨池不能被普通候选借来绕过预留。完整恢复路径尚须接通所有 producer/decoder 的 recovery phase，不能把这个 ordinary 检查冒充恢复验收。

读回后复用 §12.14 的原协议引用绑定，再以同一个 HeaderPreparation 重建当前 body/facts/segment/prefix/checkpoint 并逐项比对；无需展开完整历史。原 record 的 Input/payload 原身份必须在该提交 FactSet 的 Input 项中，不能只保留签名元数据却使历史输入范围失去原件定位。检查执行父 body/prefix/RootSet 的原身份，并从父 body 绑定的旧目录重建父 catalog extension，核对当前 body 的 catalog-before。非空 TransitionRecord 从认证父 RootSet 重新应用 read/write-set，校验实际新状态页与重建页 bytes 完全一致；再生成当前 catalog delta，核对所有实际新目录页及其原 descriptor。缺页、替换原件、相互一致但与原 producer/输入承诺不一致的候选均失败；受限和取消不会返回部分结果。

Merkle source read 另有独立计数，每次父目录重建、transition 验证、候选目录构建与新页读回均在 source read 前检查取消和额度。adapter 以独立 stop 标记返回 typed Cancelled/ResourceLimited，不解析底层 String 错误；计数直接更新 caller 的 Cell 视图，捕获 source unwind 也不能退还已发生的读取工作。固定 metadata workspace 在任何解析/扩张前取得；FactSet、TransitionRecord、重建阶段及 catalog delta 保留各自 charge。输出 `CheckedCommitCore` 只持有仍计费的结构材料，没有 Clone/可变访问或接受/回执构造能力。它证明当前结构检查成功，不证明完整物料：原事实 bytes/类型语义、value 树与创世状态闭包、projection、执行环境及历史工件、业务执行合法性、实际 fsync/独立域、保护根租约、frozen/恢复写前材料和原子选择仍是后续完整 verifier/endpoint 的义务。旧 Snapshot 入口继续拒绝新材料；不得仅凭此 core 结果签发最终回执或取得服务资格。

局部方法：`env -u RUSTC_WRAPPER cargo test -p oasis7_formal_protocol --lib incremental_material::tests -- --test-threads=1`，覆盖原字节/计费持有、非空差量、实际新状态页缺失、签名有效但 checkpoint 错配/原输入漏记、原件替换、原件/新页缺失、精确额度与少一额度、取消/跨池/class。实际来源方法为 `env -u RUSTC_WRAPPER cargo test -p oasis7_distfs --lib incremental_page_store::objects::tests::material -- --test-threads=1`：用同一原协议测试 signer 和实际 header producer 将结构原件/目录页写入真实存储，重开后通过原对象 adapter 验证，并核对 carrier charge 及 ordinary class 边界。测试没有签发端点回执，也未证明完整世界/原事实与工件闭包、完整恢复或真实故障域。


### 12.16 当前事实原件的流式身份验证

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；承接 §5.1 与 §12.15。`verify_fact_originals` 消费结构检查得到的不可变 `CheckedCommitCore`，按 FactSet 原序读取每次 Input、Result、Event 原件。重复 digest 仍逐次读取、计数和扣额度；不排序、去重或展开旧历史。独立 hash sink 核对实际输出 bytes 的完整 flat digest，以及 reader 返回的长度；来源 locator、局部 chunk 或返回成功都不能代替原身份检查。空原件也按一次事实检查。

`OriginalObjectStreamReader` 继承同池原对象接口，实际来源是 `IncrementalObjectStore::original_reader`，继续借用物理/逻辑 IO 额度并限定 ordinary class。每个事实先扣 occurrence 和至多 max_fact_bytes 的剩余总原 bytes 额度；成功才退还未消费部分，缺失、晚失败、取消或捕获来源 unwind 都不退还本次准入。独立 sink 最多每次 hash 64 KiB，持续检查取消与长度上限；即使来源忽略 Write 错误并返回成功，stop 状态仍导致 typed Cancelled/ResourceLimited，不能返回部分检查结果。固定工作空间在 hash 前取得共享 ordinary reservation，不分配整个原件。

返回的 `CheckedFactOriginals` 只绑定该 body、FactSet、三种 occurrence 数和原 bytes 总数，不提供 Clone、反序列化或接受/回执构造能力。它证明当前事实原件的字节身份，不验证事实类型、业务语义、所有状态 value、历史工件、独立耐久域或服务资格。完整 endpoint/frozen/恢复验证及发布仍须接通；新世界仍未激活。

局部方法：协议库 `incremental_material::tests::fact_originals` 覆盖精确额度、原序/重复/空原件、缺失/替换、输出后失败、谎报长度、忽略 sink 错误、取消及 unwind 不退额度。实际来源 `incremental_page_store::objects::tests::material::large_fact_streams_after_reopen_with_less_memory_than_original` 使用 8 MiB 共享池（保留 3 MiB recovery），写入并重开 64 MiB+17 bytes 原件；整件拥有方式无法准入，而按同一 FactSet 的流式校验可在普通预算内完成。这是原字节读回与 reservation 的组件证据，不是进程 RSS、固定 H/S 规模、完整世界恢复、业务/provider oracle 或真实故障域验收。


### 12.17 差量新写 value 的完整内容检查

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709；承接 §5.1 和 §12.15 的状态叶目标检查。`ValueReadContext::verify_closure` 从预期 value descriptor digest 开始，读取 descriptor 及每个 typed index/data 后代，验证实际原 bytes、同池 carrier/capacity charge、内容 hash、深度与子边界、总逻辑长度。空 value 也必须有真实空 data 叶；零长度 range 的正常无 IO 行为不能代替该闭包检查。与窄 range 共享逐页内容验证实现，原 range 的读取范围和输出预算语义不变。

完整遍历先取得固定共享 workspace，再用最多 depth×(fanout−1)+1 个待访问引用按原序深度优先读取；不建立与 value 大小增长的 visited set。重复子引用按 occurrence 读取和计数，所有读取在节点/原编码 bytes allowance 与取消检查下执行，descriptor 计入节点数。逻辑长度上限在 descriptor 解码后、读取后代前核对；无输出 bytes，不消费输出 allowance。来源失败或 unwind 已消费的逻辑/物理读取额度不退还，失败不返回部分 summary，末页后仍检查取消。

`verify_written_values` 消费不可变结构 core 的已认证 TransitionRecord，按最终 write-set 检查每个非删除 after value 引用。每次先扣 value occurrence 和 max_value_bytes 限定的逻辑 bytes 准入，完成整棵检查才退还未使用部分；同一 value 出现在多个写入中仍逐次验证。不从仅可读根、已存叶或缓存命中推断 value 可读。当前为 ordinary 候选差量路径，要求 core 与 context 同池且 context class 为 ordinary。没有 transition 的创世提交被明确拒绝，不能返回空标记冒充创世状态闭包。删除不产生新 value 义务。

输出 `CheckedWrittenValues` 只绑定当前 body、value occurrence、逻辑 bytes 和实际页 occurrence 总数，没有 Clone、反序列化或接受能力。它不证明父/创世全部状态闭包、字段类型与业务规则、历史工件、持久域、保护根或最终回执；旧材料与新世界仍未激活。创世/恢复的全部状态枚举，以及完整 endpoint 物料接线仍需后续完成；普通提交不得通过全状态枚举补此差量检查。

局部方法：协议库 `incremental_value::tests::closure_tests` 与 `incremental_material::tests::written_value_tests` 覆盖重复页 occurrence、空值缺叶、窄读未经过的叶缺失、精确节点/读取额度与少一额度、最大逻辑长度、取消、来源 unwind、实际 transition after 引用及创世/class 拒绝。真实来源方法为 `incremental_page_store::tests::complete_value_closure_reopens_above_sixty_four_mib_under_small_pool`：在 8 MiB 共享池（3 MiB recovery 预留）中写入、重开并遍历 64 MiB+17 bytes value 的 1036 个页 occurrence，不申请整件值、不消费输出额度；另核对途中取消的已消费读取和 workspace 释放。该证据只覆盖 value 闭包与真实页适配器，不能代替固定 H/S、进程 RSS、完整世界恢复、provider oracle 或独立故障域验收。


### 12.18 创世及恢复的全部状态/value 枚举

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1、§8 与 §12.17。`StateScanContext::visit_entries` 用于显式创世、bootstrap 或恢复的固定根扫描，不由普通提交调用。它复用已有 sparse Merkle 内容与深度校验，按 key hash 路径顺序枚举；叶的 namespace 和 key digest 必须与该根和实际 256-bit 路径一致，扫描完成后核对声明的 leaf_count。仅有合法 hash 的错位叶、错误深度、错误 namespace 或错误 count 均不能返回完整扫描结果。

扫描先取得固定 128 KiB 共享 workspace，待访问引用最多 257 项；不保存随状态大小增长的节点集合，不复制完整状态。实际来源必须显式声明同一预算池和所选 ordinary/recovery class。每次读前扣节点和受剩余 bytes 限制的准入，成功仅退还未用部分；缺失、底层失败及来源 unwind 不退已准入工作。叶访问有独立条目额度，源读取和 visitor 前后均检查取消；空根无需页但仍检查取消。错误和缺失不是合法空树。visitor 借用单个已验证路径的叶，其副作用在全部扫描成功前仍属临时结果，不能安装活 World 或发布 head。

`verify_state_values` 消费已检查结构 core，对 RootSet 的六类根逐一完整扫描，并复用 §12.17 同一个 value 引用验证/额度入口检查每个叶的完整 descriptor 和数据后代。它适用于创世与显式恢复，可以选择 recovery class；同一 value 被多个业务或控制叶引用时按 occurrence 检查和计数。普通提交继续使用 `verify_written_values`，不会因此全扫 S 或 H。输出 `CheckedStateValues` 只绑定该 body、六类叶数、逻辑 bytes 和 value 页 occurrence；无 Clone、反序列化或最终接受能力。失败不返回部分标记，也不证明事实语义、字段解码、模块/执行环境工件、历史全闭包、租约、独立持久域或可服务资格。

局部方法：`incremental_state::tests::scan_tests` 用独立整树构造与直接 profile key 编码核对根和顺序，覆盖精确/少一额度、错路径/深度/namespace/count、缺页、visitor 失败、空根、取消、跨池及 unwind 已消费额度。`incremental_material::tests::written_value_tests::full_state_checks_all_six_namespaces_and_each_repeated_value` 从原协议候选结构 core 枚举七个真实引用（业务两项、五类控制各一项），分别验证 ordinary/recovery class、每次原值及缺叶拒绝。实际来源 `incremental_page_store::state::tests::full_six_namespace_state_and_value_scan_after_real_store_reopen` 将同一事务六根及 value 写入真实页 store，重开后用 recovery class 完整枚举 1542 个状态页和十二个 value 页 occurrence，核对精确节点/叶额度、无输出及计费释放。测试中的创世/authority/字段语义为隔离 fixture，不代表完整世界恢复、后续业务或 J1/J2/provider oracle 已通过；实际端点和全部 Runtime 字段仍待接线。


### 12.19 原 native 执行环境的计费完整读回

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.2 的执行环境与 §12.15 的实际来源。结构 core 现在保留原候选的 ExecutionManifest、Wasm、Rules、NativeExecutionEnvironment 四个精确角色引用；后续 verifier 不接受任意另一候选提供的替代角色。角色引用本身仍不是内容或闭包验证。

`verify_native_environment` 使用其中 native 原 ID，并再次核对 body.execution_environment。它从同池 ordinary 原对象 reader 读取描述符和每个原 chunk，复用已有 `NativeExecutionEnvironmentV1` 格式、校验与整件摘要算法，不重新定义原 native 表示。描述符最多 1 MiB、chunk 最多 1 MiB、单环境最多 4 GiB 及既有组件/边界约束不变；每个 chunk 同时核对原 flat digest、声明长度和按原序累计的 component digest。已有 chunk 全部各自有效但整件摘要不符时仍拒绝。重复引用逐 occurrence 读取，不用同域缓存命中代替整件验证。

固定 16 MiB ordinary metadata workspace 在任何读取、描述符解码、字符串/集合扩张与 canonical 再编码前取得；原字节输出、物理解码及逻辑页仍保持各自 reservation 与实际 IO 额度。CoreBudget 的 object/原 bytes 额度在读取前扣除，成功仅退还未用准入；来源错误或 unwind 保留已发生的准入。native 共同校验器新增保留借用 carrier 的 `verify_with`，避免从计费 Arc 拆出或复制整个 chunk；旧 `verify` 接口转调同一算法，原格式、结果与错误合同不变。完整校验只需描述符和一个当前 chunk，不物化全部可执行文件。取消在原对象读取边界及成功返回前检查；解码/校验仍是明确受尺寸限制的阶段，不宣称进程 RSS 或全局时间治理已完成。

`CheckedNativeEnvironment` 只保留 256 bytes 计费元数据，绑定当前 body、native ID、实际 executable digest、component bytes 和 chunk occurrence。它没有 Clone、反序列化或接受回执能力；不证明当前机器与原环境相同、文件可执行、manifest/WASM/规则语义、字段依赖、frozen 续做或端点服务资格。旧 Snapshot 执行 manifest 不能直接作为 IWS manifest 使用；完整新格式工件与实际端点接线仍待完成，新世界仍未激活。

局部方法：`incremental_material::tests::native_tests` 覆盖原角色绑定、精确和少一 object/bytes 额度、缺失/替换、chunk 各自正确但 component digest 错误、描述符恶意 sequence 声明、取消、跨池、metadata 预算不足及来源 unwind 后准入/内存释放。实际来源 `incremental_page_store::objects::tests::material::native::native_larger_than_pool_is_verified_from_original_chunks_after_reopen` 在 64 MiB 共享池中写入原 chunk 和原描述符，重开后经原协议结构 core 验证，完整读回 64 MiB+17 bytes 可执行内容及 build metadata，逐 occurrence 扣除 descriptor 加 66 个 chunk 的精确额度，并核对仅结果 charge 留存。fixture 原件不执行，不代表真实可运行环境、完整恢复续跑、独立持久域或 provider oracle 证据。


### 12.20 IWS 执行 manifest 与原工件目录

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6.1/§6.2 的独立执行工件。IWS 不复用引用 Snapshot 的旧 execution manifest。新 manifest 是独立有界对象，不引用本次 RootSet、CommitBody、HistoryPrefix 或证书；原 record 的四个执行角色引用它和对应目录/native 原件。创世 manifest 不含本次 activation input，避免初始证书与 manifest 互相引用。执行 manifest 另外绑定精确 parent commit/RootSet、operation、原 input 和可恢复 continuation 原件；恢复解释与业务合法性仍由对应 owner 核定。

Wasm 与 Rules 角色各指向一个原工件内容目录，按原 content digest 查找长度与 kind。目录复用同一 256 层 Merkle 实现及最终新可达节点策略，每次新增仅改受影响路径，不重写不断增长的平面模块列表；版本/名称/治理绑定仍在 Runtime 的对应状态字段，不由内容目录授予权限。目录当前采用追加原件身份；旧高度自己的根一直保留，不以活跃版本筛掉历史原件。已存在 ID 仅在 kind/长度完全一致时幂等复用。

目录 root 编码为 `oasis7.iws.artifact-root.v1\0` + `u32(41)` + kind（Wasm=1、Rules=2，`u8`）+ count（`u64`）+ node root（32 bytes）。页编码为 `oasis7.iws.artifact-page.v1\0` + `u32` payload 长度；branch payload 为 tag=1 + depth（`u16`）+ left/right digest，leaf 为 tag=0 + kind + 原 digest + 原长度（`u64`），叶路径为原 digest 的 256 bits。空 sentinel 为同页 header 加 tag=2，空根/count 必须一致。页最多 160 bytes，root 最多 128 bytes；未知 kind、错路径/深度/count、截断和尾随内容均拒绝。

manifest 编码为 `oasis7.iws.execution-manifest.v1\0` + `u32` payload 长度，最多 1024 bytes。payload 按序为 world scope（沿用 §12.6）、format profile、native ID、executable 原 digest、Wasm/Rules 目录 ID、execution-policy 原 digest/长度，以及 mode。mode=0 为创世且无后续字段；mode=1 后续为 parent CommitBody ID、parent RootSet ID、operation、原 input ID、continuation 原 digest/长度。整数为大端，所有 digest 为原始 32 bytes。policy 必须与原 record 的 execution_policy_digest 相同，且是 Rules 目录的精确成员；continuation 单独保留原 bytes。此 wire 只定义完整性和依赖位置，不能把原件可读等同于 WASM 可执行、规则/schema 正确或 continuation 可恢复。

完整冷校验按目录原序读取每个原件，逐 occurrence 消费工作与原 bytes 额度，原件经流式 flat digest/长度检查，不物化整个模块包。原目录页与 descriptor 由同池读者和独立额度读取；少一项、缺页、替换或取消均不产生完整标记。正常提交对未变化工件仍须使用 §7 的精确父闭包证据，而不是每次调用完整冷扫描。新 manifest、目录、Runtime 字段/版本绑定及端点消费者全部接通前，新世界继续不可启用。


`ArtifactCatalogV1::append` 在分配前取得与有界条目数对应的共享 phase，仅输出精确父目录中不存在的最终新可达页；`ArtifactCatalogDeltaV1::verify` 重建并逐项比对，包括提交节点集合。typed 页边显式区分目录孩子与原对象 digest/长度，不能把原 bytes ID 当成 value-page ID。目录/manifest 解析均先限制原 bytes；冷检查固定 metadata workspace 为 128 KiB，结果只保留 256 bytes charge。其页读取、原字节读取分别使用独立额度和实际 source 的物理/逻辑 IO 预算；来源 unwind 不退已准入工作。Input 事实与工件原件复用同一个有界流式 hash sink，忽略 Write 错误也不能绕过额度或取消。

空原件读取另补齐 §12.17 的空 data 叶检查：完整原对象读回不能用零长度 range 代替 typed 闭包。真实存储删除该叶的回归已先判红，修复后明确失败；正常空原件导入与窄 range 的语义保持不变。

局部方法：`incremental_artifacts::tests` 使用独立全量逐层树和直接二进制 profile 向量，覆盖追加/幂等、父 delta 重建、节点集合替换、kind/长度/顺序/count/缺页/跨池及未知格式。`incremental_material::tests::artifact_tests` 用原协议签名的实际结构 core，覆盖创世与非空执行的 parent/operation/input/continuation、原 policy 摘要/目录 membership、native executable 绑定、精确与少一额度、替换/缺失、取消/class/跨池及 unwind 后已消费额度。实际来源 `incremental_page_store::objects::tests::material::artifacts::iws_manifest_and_large_wasm_catalog_reopen_with_exact_streaming_dependencies` 在 32 MiB 共享池中写入并重开含大 custom section 的超 64 MiB WASM 原件、原 policy/native 和两类目录，核对原协议角色、771 次页读取及全部原件额度精确耗尽、输出计费释放。fixture 不执行模块或 policy，不证明 ABI/规则 schema、Runtime 字段/版本对应、frozen 恢复、有效父耐久租约或最终发布；这些接线与 §11 规模/续跑验收仍待完成。


### 12.21 AgentCell 独立字段与流式创世准备

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1 与 §12.10 的借用字段清单。agents 不再将整个 AgentCell 作为一个 Map value 交给 genesis visitor；新的穷尽解构逐一访问身份、坐标、机体规格、资源、槽位、货物、activity、intent，并沿用已有 body/mailbox typed 编码。新增字段会使穷尽解构编译失败，避免默默遗漏。此变化尚未接通完整世界格式启用。

字段键编码为 `oasis7.iws.agent-field-key.v1\0` 加 agent digest（32 bytes）、字段 tag（u16）、ordinal（u64）。agent digest 为 BLAKE3(`oasis7.iws.agent-id.v1\0` + UTF-8 长度 u64 + 原 bytes)；整数大端。tag 1 为身份，2–4 为坐标，5–6 为机体规格，7 为资源，8–10 为 frame/capacity/expansion，11–16 为槽位 count/next/id/type/installed/locked，17–22 为货物 count/next/id/kind/quantity/unit-size，23–30 为 activity，31–53 为 intent。精确顺序与类型由 `AgentFieldV1` 固定；未知 tag、错误类型及枚举值拒绝。1–22 属 Business，23–30 属 Scheduler，31–53 属 Authorization；这里仅确定内容位置，字段内容本身不授予权限。

非索引字段 ordinal 必须为零。资源 ordinal 0/1 分别为 Electricity/Data，保留显式零、负值以及不存在的差别，业务合法性仍由业务 owner 判断。槽位和货物采用原顺序的独立 ordinal，即使语义 ID 重复也不合并。创世 count 和 next 等于原条目数，后续删除可以留下空洞，不能通过重新枚举活跃条目改变旧 ordinal。activity/intent 独立 present 字段区别 None 与存在对象；可选子字段保留 None、空字符串及零的区别。

标量编码为 `oasis7.iws.agent-scalar.v1\0` + 类型 tag（u8）+ payload。类型 1–9 按序为 u8/u16/u32/u64/i64/bool/text/optional-text/optional-u64；整数大端，bool 为 0/1，text 为 UTF-8 长度 u64 加原 bytes，可选值为 0 或 1 加原 payload。解码借用输入并拒绝截断、尾随、非法 UTF-8、类型不符和非法判别值。semantic visitor 的 serde 视图不属于冻结 wire。

Runtime 的 `write_agent_scalar_value` 先准入固定 metadata 和最大逻辑长度，直接借用标量向既有 ValueStreamWriter 写入；不生成整件编码 Vec，修改 status 不读取或编码 summary。`World::write_iws_agent_genesis_fields` 逐个发出 typed key/value 引用，body、mailbox frontier 和原事件引用复用既有格式。mailbox 每个 occurrence 必须由调用者解析到实际持久化的原 WorldEvent；缺失 provenance 失败，不能从 DomainEvent 合成原 envelope。输出回调负责同池计费和临时状态写入，全部成功前不能安装活 World 或发布 head。取消、IO、来源、限额或回调失败不返回成功 count。

本节只提供 AgentCell 子字段和单 agent 创世准备。其余 Runtime 字段 codec、普通业务 reducer、全部世界根构造、原件语义对应、恢复重建、端点与 durable head 尚未完成；这些组件不能签发完整世界、可写恢复或服务资格。局部验证核对直接 wire 向量、原顺序及重复条目、借用地址、非法 key/type、来源状态不变、mailbox 原 occurrence、失败路径，以及大于 64 MiB summary 的真实存储写入重开和独立小 status 写入；该证据不能代替 §11 的 H/S、RSS、J1/J2/provider oracle 与独立故障域验收。


### 12.22 移动原事件的同事务字段更新

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §12.12、§12.21。原 body-event 入口扩为 `World::contribute_iws_agent_event`，当前明确支持 BodyAttributesUpdated 和 AgentMoved，其余事件拒绝。原上下文改名 AgentEventContext，既有调用者同步更新，不保留同义入口。

移动复用原事件的 canonical bytes、flat digest、allocator/era、稳定 mailbox ordinal、原件持久化与事实顺序。同一 ExecutionTransaction 读取精确父根或此前 overlay 中的坐标、body/last_active 和世界时钟；坐标按 §12.21 的固定 i64 标量解码，只为实际变化的坐标及 last_active 写新值。不读取完整 AgentCell、intent summary 或历史，不从物化 cache 推导坐标前值。既有 reducer 使用 `to` 和当前 reducer 时钟更新已存在 agent，原 `from` 保留在原事件中，不新增与旧 reducer 不一致的 from-cache 前置条件。事件时间仍需与当前 reducer 时钟和认证时钟一致；本入口不替代动作策略、授权、receipt 语义或完整 driver 的正式接线。

连续移动读取先前 overlay，last_active 已相同或坐标未变时不产生额外状态写入，原事件及 mailbox occurrence 仍完整保留。缺字段/页、标量类型错误、错误时钟、取消、预算耗尽或原件持久化失败均使整个外层事务不能 seal，包含此前已暂存的无关写入。临时页不代表提交，World cache 不安装。

局部验证包括连续移动的精确原事件、直接坐标预期值、旧 cache 与认证位置不同时仍使用 overlay、i64 极值、完整 TransitionRecord 重建，以及缺字段/时钟/取消/额度/晚原件失败。真实存储方法 `physical_move_reopens_and_continues_from_authenticated_positions` 持久化首次移动页与原事件，重开后核对 transition 和原 bytes，再从同一认证父根继续第二次移动并与不中断分支比较。这只证明该 reducer 及既有页/原对象适配器，尚非完整 World 恢复、正式 head/authority 接受、scheduler/provider 续跑或独立故障域验收。


### 12.23 注册原事件与初始字段写集

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §12.21–§12.22。`contribute_iws_agent_event` 继续接入 AgentRegistered。它沿用既有 registration contract 判别和工业身份限制、AgentState 初始 body/resource/slot 值，以及注册时 idle activity；不从新副本中的任意默认 JSON 推断字段，不创建或安装第二个 World。

注册先核对认证世界时钟，并在分配初始模型前取得固定 64 KiB 加身份字符串余量的共享预算。临时单 cell 复用与创世相同的借用字段写入器；每个待写 typed key 必须在当前父/overlay 中不存在，随后进入同一事务写集。body、坐标、初始槽位、resource、activity、intent present 和空 mailbox frontier 均写入对应 namespace。只检查这些待写键，不通过扫描整个状态查找可疑孤立键；完整父状态的合法字段集合仍由正式恢复/语义接线证明，不能用本入口代替。注册重用旧身份、覆盖已存在字段或同事务重复注册均不能 seal。

空 frontier 暂存后，同一原事件管线分配本次 event ID/era，追加注册原事件的 mailbox 引用并前移 tail，持久保留原 canonical envelope 与事实顺序。随后移动可直接读取刚注册 cell 的 overlay 字段，无需在 World cache 中先插入 agent。初始模型、输出页均属临时材料；缺额、取消、错误版本、工业身份拒绝、局部字段冲突及晚期原件失败使全部外层写入失效，不安装部分 agent。

局部验证用原 PreparedCorePolicyEvent 的注册和移动结果作为预期 cell，与增量两次事件的全部对应 typed 字段、两项 mailbox occurrence 和原件 digest 逐项比较，并核对唯一额外写入为 event allocator。反例覆盖既有 agent、部分字段已存在、未知版本、工业身份限制、写集额度、取消和原件失败。实际文件方法 `physical_registration_reopens_new_fields_and_moves_without_materialized_agent` 写入全部注册字段页和原事件，重开后验证首次 transition/原 bytes，再继续对该 agent 移动并与不中断分支逐 record/新页比较。这里不将单 cell 的恢复依赖读回扩大为整个 World 恢复、注册动作授权、正式 head 发布、模块续跑或 §11 规模验收结论；其余 Runtime 字段、语义闭包与正式端点仍待接线。


### 12.24 小 Merkle 页的有界 pack 发布

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §6 的原逻辑身份与 §9 的预算。StatePageSession 的 transition/catalog/artifact 页持久化改为最多 256 个小页一批，原 producer 仍持有同池 charge；批次仅借用输入，先准入 64 KiB 引用元数据，再准入编码和读回 workspace。单页上限沿用 STATE_MAX_PAGE_BYTES（1200 bytes）；value 流的单节点写入暂不改为延迟 batch。

采用既有 business pack 的不可变整包、逻辑对象硬链接与有界索引定位方式，IWS 使用独立物理 profile，不调用旧 endpoint 的 authority/GC 接口。每批分别形成 frame pack 和 recipe pack，保留原 frame stored digest、recipe 中的原 page ID、规范页 bytes 与上层全部逻辑根。目录仍按原 digest 寻址，多个路径共享一个 inode；不能按路径长度累加推算唯一物理占用。整包先 fsync，并按预期 header、各段原 bytes、各 index row 和 EOF 完整读回，再发布硬链接；frame 目录耐久屏障完成后才发布 recipe 目录。每次返回前完成目标目录屏障；不会逐个新逻辑页重复同步同一新 inode。

pack 编码为 `oasis7.iws.page-pack.v1\0`，后接 version u32=1、kind u8（frames=1、recipes=2）、count u16、index offset u64、total bytes u64；整数大端。header 后依序保存按 ID 排序的非空成员 bytes，尾部每条索引为原 ID（32 bytes）、offset u64、length u32。count 上限 256，pack 上限 4 MiB；索引严格位于数据之后，total 必须与实际文件长度及完整 index 长度一致。正常点读只读取有界 header、二分 index 路径与单个成员，不加载整个 pack；该索引只定位，返回内容继续由既有 recipe/frame/page hash 校验，不授予接受或服务能力。未知 version/kind、缺成员、越界 span、损坏与截断均不能充当合法缺失。

raw frame 允许任意原 bytes，可能恰好以 pack 标记开头。此歧义反例已先判红；仅在有界 frame 的 pack 解析出现内容错误时，才用完整原文件摘要与其既有 frame ID 核对原表示。完整摘要不符仍拒绝，额度/取消错误不触发重试或放宽；普通有效 pack 点读不走整文件路径。recipe 与其他控制文件不允许该 raw frame 例外。

编码 workspace 为 128 KiB 加每个条目的 1200+2048+512 bytes 余量，再加既有 raw/zstd codec workspace；不固定申请一个超出恢复预留的大缓冲。原 ordinary 满额、4 MiB recovery 预留的恢复页写入继续使用原预算测试。实际每段读、索引探测和最终 probe 均先扣读取额度；pack 总写 bytes 在创建前准入，每次硬链接另消费工作额度。取消、预算或进程终止后的部分名字/私有文件只代表临时物料，不能发布 head；重试核对实际赢家的原内容并补齐其耐久屏障，不覆盖另一种合法 raw/zstd 表示，不自动删除不明 staging。

局部方法：`incremental_page_store::pack::tests` 核对独立 wire 向量、raw/zstd 的 256 页精确读回与共享 inode、点读精确及少一额度、坏 header/index/span/成员、raw 标记歧义、普通额度耗尽与 recovery 预留、途中耗尽重试、并发单页与 pack 选择，以及六个真实 SIGKILL 边界后的重开/重试。既有 page/object/value/六根状态闭包及原件验证继续使用同一 reader。注册的真实物理测试用于前后核对原字段、原事件和重开后的移动；单项耗时观察不替代固定 H/S、RSS、独立持久域或全负载测量。大值写入 batching、可恢复 pack 维护/回收、全部 typed roots 和全局物理指标仍未完成，当前整个 store 的共享保留租约与无删除边界保持有效。


### 12.25 原对象的大值页批量写入

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §12.24。pack 后端扩展到 VALUE_MAX_NODE_BYTES 范围的页，每批最多 256 页且原页总 bytes 不超过 1 MiB。物理 cuts 继续调用原 frozen gear codec，不把整大页强行作为一个 frame；唯一 frames 超过 256 时分成多个有界 frame pack，并在全部 frame 屏障完成后发布对应 recipe pack。规范 value 页、frame 原内容与原 page/对象 ID 不变。

编码前先按总原页 bytes、每页 recipe 上限和由最小 4 KiB cut 推导的 frame 数取得同池 workspace，加上既有 codec workspace；不按整原对象大小申请。小 Merkle 页仍经过原 StatePageSession 的 1200-byte 校验。value 普通单页 session 保持其同步语义；本节将批量写入接入原对象 write_object 内部，未把所有 Runtime value writer 改为隐式延迟。

原对象私有 writer 先取得 64 KiB 元数据余量，持有 producer 的 ReservedValue 页而不复制整原件。待写队列最多 256 项、256 KiB；下一页超限时先 flush 已有队列，最终 flush 成功后才发布原对象 locator，之后仍完整读回核对原平面 digest/长度。队列内相同 page ID 仅在原 bytes 完全相等时复用；原输入流及有序 value 引用、事实 occurrence 的逻辑额度仍完整消费。locator、root 和成功对象 ID 不会由一个尚未 flush 的队列产生。错误、取消和来源 unwind 释放队列 charge；此前已耐久的临时 pack 保留原身份，不能清成空世界或被解释为接受结果。

批量目录中重复 frame 的索引开销曾使既有大事实流读取额度不足，原反例先判红后修复。单次 read_page 现在只在该页范围复用已校验的 decoded frame，key 包含完整 stored/original digest、length 和 compressed 判别；不按 stored ID 单独猜测内容。条目最多 17、唯一 decoded 总 bytes 不超过该页长度，由已有 decode workspace 准入，返回或失败即释放，不形成全局缓存。所有 occurrence 仍按原顺序复制并核对最终 page digest。format 读取仍单独消费额度；原预算反例按唯一 descriptor 的实际物理读数修正，少掉 format 那一读仍须失败，未增大原事实流的 IO allowance。

局部方法：新增 general_batch 测试独立构造 4 KiB cut 边界，验证 510 个唯一 frames 跨两个 pack、恢复原页及 1 MiB 批次上限。original batch 测试覆盖显式 flush/队列和 charge 释放、最终 flush 无额度、import 摘要不符、取消/unwind、raw/zstd 重导入身份不变，以及三个中间批次 SIGKILL 窗口下无 locator、原身份重试。实际大值在 8 MiB 共享池（3 MiB recovery 预留）中流式写入并重开 64 MiB+17 bytes，逐个 64 KiB 区间有独立位置内容，使用独立分块平面摘要 oracle 全量核对并检查 recipe 硬链接共享；不以重复单页模拟唯一大值工作集。重复 frame 回归还使用 checksum 正确但 original digest 被篡改的 descriptor，确认局部复用不会绕过内容校验。本节仍不代表 Runtime 全字段写入、维护/GC、正式发布、独立持久域或固定 H/S/RSS 验收完成。


### 12.26 模块唤醒字段与同事务消费

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §5.1 的 module_tick_schedule、稳定 instance 与绝对 wake/clear 控制状态。原字段在创世投影中使用固定 bytes，仍归 Runtime/Scheduler，不再将此字段的 u64 值交给通用 serde 入口。

键为 `oasis7.iws.module-wake-key.v1\0` + UTF-8 byte 长度 u16 + 原 instance ID；长度大端、整体不超过状态 key 的 1024-byte 上限，空 ID、截断、尾随和非法 UTF-8 拒绝。ID 不 trim、不改写为 module ID 或版本；生命周期 owner 负责选择既有语义下的稳定 instance。值为 `oasis7.iws.module-wake.v1\0` + 绝对 simulation time u64（大端），0 与 u64 最大值均合法；不存在由状态叶 absent 表达，不能用 0 代替 clear。World 的借用创世 writer 逐项写值，并有独立条目额度、取消检查和临时输出边界，不 clone 整张 map。

`contribute_iws_module_wake` 供已完成 owner 校验的激活、更新或 clear 使用，读取认证 parent/overlay 的世界时钟及当前值，只写实际变化的键。`contribute_iws_due_module_tick` 对调用者提供的一个 instance 点查：不存在或未来 wake 不执行；到期则先在同一 ExecutionTransaction 清除旧键，再调用受信执行准备回调，最后按回调 directive 重排。回调、此前外层业务及调度写集共用同一个事务；取消、预算、字段内容错误、旧时钟、跨池或回调失败使整次事务不能 seal，World cache 不安装。

唤醒时间计算与现有 tick 路由共用同一个函数：WakeAfterTicks 的相对间隔至少为 1，以当前绝对时钟饱和相加；Suspend/无 directive 不额外重排，保留既有回调中其他合法控制写入的语义。稳定 instance 的另一个版本或同 module 的其他 instance 不被顺带清除。这些点贡献不枚举 due set；正式 driver 仍须从精确根提供有界、完整且每轮去重的 due 集合，特别是最大时钟饱和后不能在同一轮重复消费新写的唤醒。

本节尚未接入认证 due 索引、完整 module invocation/工件绑定、Pure 的 state 限制、费用/原结果事实及确定性 routing metrics；不能用这些函数代签完整路由或 J2。局部方法覆盖独立 wire 向量/UTF-8/key 上限、实际 map 的固定字节创世、条目额度、overlay 与旧 cache 不一致、执行前 clear、零延迟/溢出、挂起、缺失/未来键、坏值、写入不足及晚失败。既有模块路由测试核对共享计算的消费者。实际文件方法 `module_wake_reopens_and_continues_from_the_same_absolute_schedule` 持久化首次 wake 与业务写集，重开后验证 transition，再由隔离 driver 显式暂存新时钟并执行到期 clear，与不中断分支逐 record/新页比较；回调为 fixture，不是实际 WASM/费用或完整世界恢复验收。


### 12.27 认证键范围遍历基础

Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §12.26 未完成的 due 集合。共享 sparse Merkle 遍历增加闭区间键读取，完整扫描复用全键域区间。读取子节点前按当前认证路径前缀计算子树的最小/最大键；与请求区间不相交的子树不读取，不以读取失败或猜测摘要代替范围判断。范围内节点继续核对原 digest、编码、深度和叶 key/path；无效反向区间在任何 source 读取前拒绝。

遍历仍使用最多 257 项 DFS 栈，不保存随总叶数增长的 visited map，也不收集完整结果。该内部原语不自行声明 IO/内存准入：调用者沿用已准入 workspace、带额度 source 和有界 visitor，任何错误均只保留临时输出；完整扫描调用方原有取消、额度和计数核对保持有效。局部测试使用独立逐层分区构树与排序区间 oracle，覆盖每个 bit/byte 边界、闭区间端点、单点、空区间结果、空树、反向边界、范围内缺页、范围外未读取的缺页、坏摘要/深度/叶路径及 visitor 立即失败。单点在多分支树中恰读 257 个路径节点，未遍历其余叶子。

本节只提供认证读取基础。普通状态键仍是摘要键，不能直接把其区间当作绝对 wake 顺序；按时间排序的索引、索引与标量字段的同事务双向约束、完整且每轮去重的 due 集合，以及实际 module driver/J2 仍须继续实现。范围结果不授予接受、持久发布、恢复完成或服务能力。


### 12.28 按绝对时间排序的认证 wake 内容索引

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；承接 §12.26–12.27。新增 ScheduleIndexV1 内容结构，外层 sparse Merkle key 为绝对 wake 的大端 u64 加 24 个零 bytes；每个时间叶绑定该 wake、非零成员数与 instance 集合根。内层 key 使用完整 BLAKE3 摘要，输入为 `oasis7.iws.schedule-instance.v1\0`、原 UTF-8 byte 长度 u16 和原 instance；叶保留原 instance bytes。两层均使用完整 256 层树，不截断 instance 摘要来拼接时间。内层允许相同内容集合跨历史时刻复用，其外层叶仍绑定精确 wake。

页域为 `oasis7.iws.schedule-index-page.v1\0`，后接 body 长度 u32、kind u8（Times=1、Instances=2）与 tag u8。branch tag=1，字段为 depth u16、left/right 各 32 bytes；leaf tag=0，Times 字段为 wake u64、count u64、bucket root 32 bytes，Instances 字段为 byte 长度 u16 与原 UTF-8。空叶 marker 使用 kind 与 tag=2，空根由固定深度规范 branch 递推，不保存空对象。整数均大端；页不超过 1200 bytes，非空 instance 不超过 1024 bytes。未知域/kind/tag、尾随、截断、非法 UTF-8、零 count 或空 bucket root 拒绝。两种树使用独立 sentinel 表，不能因泛型 static 共享而混用身份。

根域为 `oasis7.iws.schedule-index-root.v1\0`，后接 body 长度 u32、buckets u64、entries u64 与 time node root 32 bytes，总长不超过 128 bytes。空 entries、空 buckets 与空 time 根必须一致，buckets 不超过 entries。单项 update 接收实际旧 wake 与目标 wake，验证旧 pair 后删除/清空原 bucket，再写目标 pair；相同 pair 幂等。所有路径采用同一个受限 time updater 和同一个受限 instance updater，分别跟踪读取的父闭包，最终只输出目标仍可达且不属于已读父闭包的新页，按 digest 排序。单例从旧时间移动到新时间时可复用原 instance 树；临时删除/重插的中间页不进入 delta。任意未读 CAS 命中不能改变规范新页集合，delta verify 从精确父根重建并逐项比较。生产阶段先取得同池 Ordinary 的 2 MiB metadata/workspace，单树 pending 上限 256 KiB，结果保留实际容量 charge；跨池或错误 class 拒绝。

visit_due 对 time key 的闭区间 [0, now] 做认证范围读取，再逐个读取到期 bucket，顺序为 wake、完整 instance 摘要。读取结果仍是临时内容，不能把这个顺序直接当作实际模块执行顺序；原 routing 的优先级、稳定 instance 与轮次规则仍由 owner 接入。两层 DFS 共用 256 KiB workspace，同池 Ordinary/Recovery 与 node/byte/bucket/instance 额度；逐个 source 调用前消费读取额度，错误或 unwind 不退还已准入工作。取消、visitor 失败、源错误及计数矛盾不返回部分成功；每个到期 bucket 核对完整成员数，完整最大时间读取还核对全根 buckets/entries。未来 bucket 不读取，其缺页不能影响请求区间；已请求缺页不能被当作合法空 schedule。

Runtime 必须将根描述符绑定在 Scheduler 状态、与标量 wake 在同一事务维护，并在创世/恢复核定其双向一致性。此索引的独立 update 要求已合格父索引且 old_wake 来自相同事务的实际标量字段；它不能靠一个 pair 点读证明该 instance 在所有其他时间均不存在。本节未完成该 owner 接线、正式 due 快照/每轮去重、实际 invocation 或 J2。typed page edges 和 StatePageSession 的同池 pack 持久化入口只是保留闭包的基础，尚未接入六根恢复的索引闭包 owner、GC 或正式 head 发布。

局部方法用独立 wire/逐层递归树和逻辑 map 核对创建、同桶新增、跨桶迁移、清除、空树、幂等、精确最终新页及旧根保留；还覆盖 0/MAX、完整 UTF-8 上限、未知格式、错误 kind/摘要/叶路径/count、范围内缺页、未来缺页剪枝、未来桶增长时已请求读取量不变、精确/少一各维额度、晚取消、visitor 失败、源 unwind 保持额度消费、跨池与 recovery 预留。实际 raw/zstd 页测试持久化根描述符为 value，重开后核对 due 集合并继续更新，和不中断 delta 逐项一致；每个显式操作阶段有独立有限物理 allowance，阶段内不重置。raw 使用 16 MiB 池、zstd 使用 32 MiB 池（均含 4 MiB recovery 预留），后者受既有 16 MiB codec workspace 约束；16 MiB 配置的 zstd 写入先明确拒绝，不以取消 charge 放宽。本方法不证明全局 IO/RSS、固定 H/S、原模块行为、完整世界续跑或独立故障域验收。
