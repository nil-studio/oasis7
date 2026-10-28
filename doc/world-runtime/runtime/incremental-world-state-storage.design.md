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
