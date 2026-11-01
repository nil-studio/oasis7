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

<a id="iws-ledger-value-v1"></a>

`ledger-value.v1` 只冻结 `MaterialLedgerId` 值的无损原语，不是通用 entry wire、字段登记或 World 启用。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。工厂、配方、运输等聚合值内的账本 ID 仍须由后续 typed 值 codec 显式接入；键 codec 或任意 `Serialize` 不能替代值编码和准入。

值 bytes 固定为 ASCII `oasis7.iws.ledger-value.v1` 加零字节、kind u8（World=0、Agent=1、Site=2、Factory=3）、原 ID 的 u64 大端 UTF-8 字节长度、原 UTF-8 bytes。World 的长度必须为零；其他 kind 保留包括空文本在内的原值，业务合法性由原 owner 验证。编码长度必须 checked，未知 kind、错误 domain/总长、尾随 bytes 和非法 UTF-8 明确拒绝。不调用账本 ID 的格式化或 Serde 转换，不聚合另一个 ID 字符串，也不重写其业务身份。

借用解码的来源 bytes 由调用者持有并准入。树上流读先用预期 value-root ID 验证 descriptor，再核对固定头与完整逻辑长度，按显式读取类别和有限 I/O 预算读取；UTF-8 跨块只保留至多三个未完成字节，输出在整体校验和最终取消检查成功前均为暂态。逻辑长度额度是操作预算，不是格式上限；失败或取消不退回已消耗工作，也不授予字段归属、授权、head 接受或恢复后可服务资格。原始 side/history 中的浮点及其他 entry 值类型不在本原语中；完整字段/值/闭包和生命周期能力仍须按本节其余条件完成后才能启用新格式。

页面/块大小、fanout、事务工作集、并发/cache、history 段封闭、后台节奏和磁盘余量由实现候选的测量决定，记录实际选择与范围；它们不作为本文虚构的通用 SLO。逻辑布局参数与操作配额必须分开，前者变化需要显式格式规则，后者不能改变共识可见结果。

WASM/Runtime/外部效果 owner 共同确认 scheduler、allocator、授权/去重及 unknown 的完整语义；QA 在非空续跑和原效果不重派上独立审读。ops/P2P 提供实际独立域与恢复证据，缺失时只接受已验证的本地范围。本文提供新格式系统合同，不声明源码实施、原世界修复、生产重置或部署完成。

<a id="iws-due-source-order"></a>

到期模块查询必须先从当前事务的认证 scheduler index 选择 wake≤当前时刻的集合，再在显式准入的条目/原 ID 字节上限内恢复原实例 ID 的 String 字典序；时间/实例摘要枚举顺序不作为执行顺序。查询绑定 clock、当前 index-root 及每个目标的原 wake 标量，并在完整扫描后拒绝重复实例或 index/scalar 不一致，任何预算、缺页或取消失败使整笔事务不可 seal，不返回可采用的部分集合。返回集合与其 post-read prepared guard 一同保持准入，writer 在消费前按原 live fence 校验；集合不是模块调用、收费、工件或完整字段族资格。查询不扫描未来 wake 分支，也不从旧 World map 重建到期集合。输出额度属于操作预算，不改变逻辑顺序或派发身份；原 owner 仍须接通调用前快照、manifest/实例资格、真实 Pure/Reducer 执行、收费与 wake/clear。Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

### 12.3 通用字段键与语义投影边界

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；落实 §6.2 的完整字段归属与 §12.2 的编码冻结，不新增业务要求。

`GenesisFieldVisitor` 是借用来源接口，不是 wire。Agent、body/mailbox、module instance 等已定义的专用 typed 键和值继续由其唯一 codec 负责；模块选择、状态和工件等字段在专用 codec 接入时也遵循同一边界。通用字段键不能替代它们或写一套影子状态。其他字段使用显式 `world-field-key.v1` 键族，消费者须将解析结果与受支持的字段描述符及该 namespace 中的实际键核对。解析一个结构正确的键，不授予字段归属、授权、执行或接受资格。

键的字节布局固定为：ASCII domain `oasis7.iws.world-field-key.v1` 加单个零字节；owner u8；namespace u16 大端；shape u8；record-kind u8；name-byte-length u16 大端；name ASCII bytes；semantic-key digest 32 bytes。owner 固定 WorldState=1、Runtime=2、Cognition=3；namespace 沿用状态树的 1..6 标签；shape 固定 Value=1、Map=2、NestedMap=3、Set=4、MapSet=5、Ordered=6、ByteMap=7。History、Process、Children 不是状态字段 wire shape。name 为 1..128 个 ASCII 字母、数字、下划线及点，首字符为字母，各点分段非空。这是固定字段名上限，不是实体 ID 或大字段内容上限。

record-kind 固定 Metadata=1、Group=2、Entry=3、Bytes=4。Metadata 的 digest 必须全零；其他 kind 必须由调用者显式提供语义键摘要，全零摘要并不被擅自当成“没有成员”。Group 仅用于 NestedMap/MapSet 的外层存在标记，Bytes 仅用于 ByteMap；Entry 对应语义成员。不同 kind、owner、namespace、shape 或 name 均形成不同键，不用空 key 的特殊值模拟缺失。未知标签、错误 domain/长度、尾随 bytes 和不合法组合明确拒绝。

嵌套 map/map-set 对每个 outer key 先输出 `group`，包括零成员，再输出实际成员。group 的成员数是准入元数据，不代替未来字段值/索引的认证内容。Agent 等稀疏 typed 子字段由原 Identity、presence 和 count codec 表达 owner/集合存在，不要求所有 NestedMap 描述符都输出 group。任一 sink 错误后须丢弃整个暂存前缀。

本节只冻结通用字段键。语义 key 的规范摘要前像见 §12.4；对应 value/outer marker 的编码、完整字段登记、借用来源到专用 codec 的路由，以及恢复消费者仍须在其实现中冻结并验证；在这些义务和 §12.2 其他条件完成前，新格式世界保持不可启用。不能仅凭本键 codec 宣称完整 World 创世、闭包验证、恢复或 GC 已接通。


### 12.4 语义键的借用编码与摘要

Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。通用字段的实际 key 来源采用封闭的 `SemanticKeyV1` 类型集合，不用任意 Serde 对象作为身份前像。当前来源为 unit、UTF-8 文本、u64、ResourceKind、MaterialLedgerId、这些类型的借用以及二元组。新增来源类型须显式选择格式能力，不能依赖 Rust 类型名、Serde `into` 转换或不稳定 map 顺序自动扩展。

canonical bytes 为 ASCII `oasis7.iws.semantic-key.v1` 加零字节，再接一个自定界 token；摘要为完整 canonical bytes 的 BLAKE3。token 标签：unit=0；text=1 加 u64 大端字节长度和原 UTF-8 bytes；u64=2 加八字节大端值；resource=3 加固定 Electricity=1/Data=2；ledger=4 加 World=0/Agent=1/Site=2/Factory=3，非 World 再接 text token 形式的原 ID；pair=5 后顺序编码两个子 token。引用和 String/str 不另加标签，不复制原文；ledger 类型与同文字符串不互相冒充。允许空文本、原 Unicode 和 u64 极值；业务 ID 合法性仍由原字段消费者验证。

pair 嵌套最多16层，这是逻辑 profile 上限；字节操作额度不改变编码、摘要或树形。读取检查完整 domain、token、UTF-8、长度、已知枚举、深度与末尾边界，返回借用内容和已验证的 pair 子 token，不聚合原 key。哈希先取得同进程预算的固定 workspace，并在逐块处理前检查取消与有限字节额度；失败和 unwind 不发出成功摘要，已消耗的工作额度不回退。流式输出在整体成功前均为暂态，实际 sink 须自行持有分配/I/O reservation。

本节不把 schema-owned 原文键的解析升级为字段注册、授权或 head 接受资格。原语义键须随对应 value/索引内容保留以供恢复并重新核对摘要；完整 value wire、producer/reader 与 World 生命周期仍按 §12.3/§12.2 接通后才允许激活。


### 12.5 generic field member 原键与值封装

<a id="iws-field-member-v1"></a>

`field-member.v1` 为 §12.3 的 generic `Entry`/`Bytes` 成员保留 §12.4 的原始语义键字节与显式选定的值原语。专用 Agent、模块等 key family 仍由各自 codec 负责；本封装不重映射其键，不接管字段登记或 World 启用。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

固定布局为 ASCII `oasis7.iws.field-member.v1` 加单个零字节、field-key-byte-length u16 大端、payload-codec u8、semantic-key-byte-length u64 大端、payload-byte-length u64 大端、完整 world-field-key.v1 bytes、完整 semantic-key.v1 原 bytes、payload bytes。最后两段不加填充；总长度须 checked 相加且恰好等于值树 logical length。field key 必须是结构合法的 Entry/Bytes，原语义键的 BLAKE3 必须等于其中 semantic digest。Metadata/Group 不在本 profile 内，不用空成员冒充其独立存在性和计数合同。

payload-codec 固定 Unit=1（零字节）、U64=2（8字节大端）、I64=3（8字节大端二补码）、Bool=4（单字节0/1）、Text=5（原UTF-8字节）、Ledger=6（§12.2 ledger-value.v1 完整字节）、Bytes=7（原字节）、DeliveryCursor=8（§12.14 的固定16字节 era/event ID）、IdentityU64Triple=9（§12.21 的原身份 UTF-8 与三个 u64）。Bytes key 只配 Bytes payload；Entry 可配其余封闭类型。写者接收封闭借用值枚举，不能用任意 Serialize 代替本 profile，也不将这些原语声称为全部聚合值 codec。

写入前明确 key/payload/record 三项字节上限；语义 hash 使用有限工作额度、取消检查和已准入 workspace，再从同一封闭不可变源写入既有 ValueStreamWriter。输出 key 与 value 分段保留原内容，不构造整条编码缓冲。返回的 root 仍为 provisional，原 authority、事务、完整登记和发布合同独立适用。

raw reader 从期望值 root 与期望完整 field key 开始，以明确 pool/class 和有限节点/读写额度验证 descriptor、固定有界 header、field key、长度以及原语义键 digest；先确认声明总长与认证 root 自洽，再应用操作额度。key digest 匹配后才输出 payload。两段输出在整个读取成功前均为 provisional。该接口提供认证范围，不解析语义 token/UTF-8 或 payload，不返回已解码业务值；owner 后续必须按已登记 profile 验证键、值及业务身份，不能用 raw read 代签 typed 恢复。恢复 class 不得偷偷借用 Ordinary 配额，取消、错误和未完成读取均不得产出完成元数据。


### 12.6 generic field 与 outer group 存在标记

<a id="iws-field-presence-v1"></a>

`field-presence.v1` 独立表示 §12.3 的 Metadata/Group 存在性，避免空集合、空 outer group 在成员列表中消失；不以伪造的 Entry 或空字符串键替代。专用 Agent/模块 codec 仍保持原合同。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

固定布局为 ASCII `oasis7.iws.field-presence.v1` 加单个零字节、field-key-byte-length u16 大端、semantic-key-byte-length u64 大端、完整 world-field-key.v1 bytes、原 semantic-key.v1 bytes。没有 payload 或尾随填充；checked 总长必须恰好等于认证值树 logical length。Metadata 的 field key 必须没有 semantic digest，semantic length 必须为零；Group 只接受 NestedMap/MapSet 的合法 Group field key，semantic length 至少为一，完整原键的 BLAKE3 必须匹配其中 digest。Entry/Bytes 不能使用本 profile。

标记不保存成员数。§12.3 visitor 提供的 entries/count 仍仅供准入，不能因一个存在标记就推断集合为空、全部成员完整或 count 已获认证。Group 对所有实际存在的 outer key 写入，包括空 group；普通点更新保留其原身份，仅在 owner 实际删除 outer group 时删除标记。Metadata 表示已登记字段的存在，不授予字段登记；实际 field profile、完整集合枚举/成员关系及业务资格仍由 owner 和认证根/索引验证。

写者分别提供 Metadata 与借用原 Group key 的封闭入口，显式限制 semantic/record bytes，使用有限 key hash 工作额度、固定准入 workspace 和 ValueStreamWriter，不构造完整原键编码缓冲。读取从期望 root 和完整期望 field key 开始，在应用操作额度前核对 framing 与认证总长，显式使用 pool/class 和有限 I/O；Group 原键流式输出并核对 digest，Metadata 不调用原键输出。每次底层输出尝试后返回外层取消检查，hash 仅计成功接受前缀。输出、暂存页和返回 root 在原事务发布前均非权威；读取整体完成及最终取消检查前输出保持暂态。

raw reader 不解析语义 token 或业务身份，不用结构合法或摘要匹配代签 typed 恢复、完整 World 创世、激活、复制闭包或 GC。缺标记与认证缺失的判定须经实际字段恢复流程，不能把预算不足、取消或读失败当作空集合。


### 12.7 资源、材料与材料账本的创世生产者

<a id="iws-inventory-genesis-v1"></a>

本生产者直接借用 `WorldState.resources`、`materials`、`material_ledgers` 的原类型，只负责这三个已登记字段的暂存值和点写入，不遍历其他 World 字段或历史，不调用任意 Serialize，不 clone World。字段描述符复用 §12.3 的唯一 WorldState 登记，并核对 owner、Business namespace 与 Map/Map/NestedMap 形状。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

三个字段各有一个 §12.6 Metadata 标记。`resources` 使用原 ResourceKind 语义键、`materials` 使用原 UTF-8 材料名；成员均为 §12.5 Entry/I64。`material_ledgers` 对每个原 MaterialLedgerId 写 Group，含空账本，再以原 `(ledger, material-name)` pair 为 Entry/I64 键值。数量保留全部 i64 位，不规范化、拼接或格式化原 ID，也不把同名的不同 ledger variant 合并。金额和业务身份是否合法仍由原 owner 验证。

每条输出先消耗有限 record 额度，再进行有界 key hash/值写入；全部记录共享调用者提供的节点/读写及 key-work 额度、同进程内存池、取消回调和 semantic/record 字节上限。新的 owned StateWrite 键缓冲在分配前取得额度，并将 reservation 随记录交给 sink；sink 保留记录时须保留其 charge，并为自己的批次/索引元数据另行准入。每条写入均为该注册字段的原完整 field key、before=None 和暂存 value root。源对象及已有容器的驻留额度由调用者持有，不用“借用”掩盖源状态内存。

输出顺序用于确定性创世遍历，不代替 StateDelta 的唯一、有序写集合同；实际 sink 在已准入的有界批次中整理写集并构建暂存树。错误、取消或额度耗尽不返回完成摘要，调用者丢弃整个未完成创世前缀，不能发布部分字段；已有暂存页也不因此取得 authority。成功摘要仅报告这三个字段的完整生产，不表示全部 World family 已覆盖、原始 genesis guard 已验证或新格式已可激活。普通提交必须由实际变更入口产生增量 write-set，不通过重扫这三个 map 模拟点更新。


### 12.8 资源三字段的类型化点读

<a id="iws-inventory-point-read-v1"></a>

点读只适用于 §12.7 的三个已登记字段，输入为调用方资格化的 Business StateRoot 与实际页 reader，不扫描其他字段或历史。先认证该字段的 Metadata，再从原 ResourceKind、UTF-8 材料名或 `(MaterialLedgerId, material-name)` 生成规范预期键。原 key 的摘要与长度必须匹配，成员必须明确为 I64 且 payload 恰为 8 字节；未完成前不返回业务值。member profile 检查在认证总长自洽之后、操作字节额度及 payload 输出之前进行，错误类型不能因其大 payload 被掩盖为有效 I64。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

资源/材料返回认证的成员缺失或原 i64；账本查询分别表示 Group 缺失、Group 存在但材料缺失、数量存在。查询涉及的 member 若存在而 Group 缺失则拒绝；不能因此声称已检查同一字段的全部其他成员。缺 Metadata 是字段格式/覆盖错误，不是空集合。不存在与显式零值保持区别。

状态页和 value reader 必须使用同一进程预算及同一 Ordinary/Recovery class。状态 lookup 前取得固定 query workspace，逐节点检查取消；语义键哈希、节点/读写和记录字节均有有限额度。状态 reader 的旧 String 错误边界由调用方提供原 typed-error carrier 恢复 Memory/ResourceLimited/Cancelled/I/O；没有原 typed error 的验证错误明确为 Invalid，不通过匹配错误文本猜测类型，任何失败都不返回合法缺失。输出按原 key digest 绑定到调用方的规范 typed key，不物化巨大原键。

这提供指定根下的类型化点内容，不授予根 authority，也不证明完整 family 闭包、完整 World 恢复或可激活。原 root/物理代保护、完整 owner 登记、源身份和业务授权仍由外层负责。


### 12.9 库存数量的有界点变更准备

<a id="iws-inventory-point-change-v1"></a>

本入口针对 §12.7 三字段准备单个资源/材料数量的最终点写，或显式允许创建的账本及其中一个数量，不重扫 map，不 clone World，不执行费用、外部效果或 head 发布。调用方提供原 typed key、精确 expected-before 和最终 after（None 表示删除该成员，Some(0) 保留零值），业务计算和授权仍由原 owner 承担。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

先按 §12.8 在给定不可变 Business 根下认证 Metadata、Group、原键和 I64；expected-before 不符返回独立 precondition 错误，不能在失败后写值或输出 StateWrite。资源/材料的缺失与零值、账本/材料各自的缺失分别比较。缺失账本只有在显式 create-ledger 许可下才写 Group；删除最后一个已知成员也保留 Group，不凭一次点读推断其他成员不存在。本入口不支持删除 Group，后者需要完整的成员关系依据。

每次最多输出两条已计费 StateWrite，记录原 value root 为 before、新暂存 root 或 tombstone 为 after；不改无关 Metadata/Group/成员。before 与 after 相同时不重写成员。读写使用同一进程内存池；普通点准备明确使用 Ordinary，拒绝把 Recovery reader 的专用额度转给隐式 Ordinary 写者。读、写各自有限工作/I/O 额度及取消回调仍适用，调用方组织整个操作的总准入。owned 写键在分配前取得 reservation 并由 sink 保留；任何错误或取消不返回完成结果，调用方丢弃整段暂存前缀。

完成结果携带源 StateRoot 的完整规范 bytes，外层必须绑定同一 prepared guard、合并业务已决定的最终写集、校验全部读依赖并在有界批次中构建 StateDelta；不能把同一旧根上的多次独立计算误当作可顺序重放的业务命令。暂存页、点写完成和业务提交/接受保持不同状态。本片不提供完整事务、账本删除、完整 World facade、authority 发布或恢复续跑的验收。


### AgentMoved 的共享事务贡献

<a id="iws-agent-move-v1"></a>

Runtime 的原 AgentMoved 事件与 BodyAttributesUpdated 复用同一 AgentEventContext 和 ExecutionTransaction。位置处理从当前认证 overlay 读取 world clock、body/last_active 与三个具名 i64 坐标，按原 reducer 的 to 写变化坐标并更新 last_active；from 仅保留在原事件中，不作为过期 World 缓存的额外前置条件。不得 clone Agent/World、重建旧 Tick 共识或扫描完整历史来计算点变化。原 allocator 的 event ID/era、邮箱 frontier/ordinal、原事件 canonical bytes 及其原 flat ID 在同次贡献中处理；原件写入回调只持久保存暂态对象，不发布业务接受。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

输入事件、原件编码临时空间、页和输出均须按适用范围预先准入；读写使用同一预算池的 Ordinary 额度以及有限 I/O/记录上限和取消。任一原类型、clock/allocator/mailbox 前置、读写、原件回调或取消失败使整笔共享事务不能 seal，不能采用先前成功写入。此贡献入口不执行移动权限、费用结算、M1-v2 era 接受检查或 authority/head 发布；适用原 owner 与外层 driver 仍须在接受链路接通，不将暂态 contributor 当作新格式激活。

物理分量验证保存原事件后关闭并重开实际 Raw/Zstd store，以认证根继续 body/move，独立核对原事件 bytes 与缓存相等目标的坐标前后值。此 fixture 仍保留 World 的旧上下文和预期 transition 元数据，不声称完整 World 重建、C<T 非空后缀恢复、到期模块动作或正式独立持久保障。
### 12.10 库存点准备对共享事务的贡献

<a id="iws-inventory-transaction-v1"></a>

§12.9 的准备结果同时保留固定数量的完整点读依赖：字段 Metadata、适用的 Group 及目标成员，各含完整结构键与原 value root/absent。它们使用固定大小存储，不复制原业务大键。导入共享 ExecutionTransaction 前校验同一进程预算、精确 prepared guard/live fence 和同一 Business parent root，再逐项将依赖登记到该事务的 read-set；overlay 中的当前 root 必须与原依赖一致，不能把先前贡献已修改的内容当作旧根结果继续覆盖。即使最终没有写入，读依赖仍须登记。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

准备结果的根、条数及写身份保持私有并仅提供只读访问。prepare 内部在输出到 sink 前，为最多两条写分别计算域分隔摘要，覆盖完整键的长度/bytes、before/after 的存在标签和全部 root bytes；导入按原输出顺序比较每条摘要，不能用另一条同键同 before 的任意 after 替换。调用方交付的暂存 StateWrite 必须仍持有同池 reservation，且 reservation bytes 至少覆盖既定记录开销及 key 的实际 capacity；条数与准备结果一致、键不重复、仅对应目标成员或显式新建 Group，before 与认证依赖一致；导入不接受 Metadata 写。全部操作在一次 contribute 内进行；错误、取消、依赖冲突、写集/内存/读取额度耗尽或 panic 都使整个事务不可 seal，不能捕获错误后发布已导入前缀。新增 read-set 条目或执行 write 时事务 revision 变化，旧 guard 随之失效；调用方在贡献成功后取得当前 guard 再执行后续阶段，不假定重复命中已有依赖的纯 no-op 一定改变 revision。

此入口接受原 owner 已决定的最终 after roots，不赋予业务授权，不证明这些 roots 的耐久闭包或完整字段家族，也不安装活 World 或发布 head。基于同一 parent 的多次点计算若触及已改变的依赖须拒绝并重做，不能把此批量贡献冒充 overlay 上的顺序业务计算。


### 12.11 共享事务中的类型化库存读取

<a id="iws-inventory-overlay-read-v1"></a>

库存 facade 的事务读取使用同一个 ExecutionTransaction.read，读取当前 overlay 的 Metadata、Group 和成员 root，并复用 §12.8 的规范 typed key、presence 和 I64 内容验证；不绕过 read-set 去读旧 parent tree，也不将尚未 seal 的 overlay 当成已接受 head。前一笔贡献暂存的新值页必须在该事务的 value reader 中可读，后续资源/材料/账本查询才能看到新值或 tombstone，并保持 LedgerAbsent/MaterialAbsent/Value 与显式零的区别。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

事务与值读取必须属于同一进程池，类型化普通执行读取使用 Ordinary，不借 Recovery 额度。语义键、记录字节、value I/O 与事务 read-set 额度保持有限；状态 walk 在分配前取得固定工作区，实际状态 reader 继续执行其 I/O/取消预算。查询在每次 lookup 前后及流式值校验中检查取消。原状态 reader 的 typed error carrier 在 lookup 后读取，即使返回 Ok 也不能忽略 outstanding failure；错误不通过字符串推断，不返回合法缺失。

每次查询包在一次 contribute 中；缺 Metadata、已知 orphan member、错误原键/codec、缺暂存页、损坏、取消或额度不足均使整个事务不可 seal，外层不能捕获查询错误后发布先前部分写。查询成功只提供当前事务的点内容，不授予授权，不完成持久闭包、完整家族枚举、World 激活或 C<T 恢复。当前 owner 仍须在同一事务内依据这些结果计算和提交其业务效果。


### 12.12 当前库存 overlay 的连续点写

<a id="iws-inventory-overlay-change-v1"></a>

普通库存点写直接借用 §12.11 的同一 ExecutionTransaction，在精确当前 guard/live fence 校验后，认证当前 overlay 的 Metadata、适用 Group 和成员，再比较 expected-before；它不调用固定 parent root 的准备入口重读旧值。前一笔写9→10后，下一笔必须以10为当前值，并可继续写13；最终事务记录只保留原9的 before 和最终13的 after，业务输入/事件仍由原 owner 按原顺序追加。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

点写复用 §12.9 的封闭记录编码、expected-before 与显式 Group 创建规则，以及 §12.10 的实际输出绑定、带 reservation 的最多两条写和完整依赖检查；不另设键/值 wire。中间暂存批使用固定容量，不复制 World 或重扫 map。读、写、事务和暂存写属于同一 Ordinary 进程池，键工作与 I/O 额度由调用方持续持有，不在每次方法调用中隐式重置。

认证读取、前置条件、暂存编码和贡献位于一次 contribute 内；任何错误、取消、guard/fence 失配、有限额度耗尽或 panic 都使整笔事务不可 seal，不能在失败后发布较早成功点写。新值页在同一事务后续读取前必须可见；页可见和点写成功都不表示业务提交或 head 发布。成功后调用方取得当前 guard 继续后续阶段，原 parent 与已接受 head 保持不变，直到原提交链路实际发布。

该入口写原 owner 已决定的最终数量，不执行费用、余额许可或外部效果；不能用存储 setter 代替业务授权。它不提供完整 World、Group 删除、持久闭包资格、复制或恢复激活，仍须接入完整 owner 与原提交合同。


### 12.13 世界时钟与固定 u64 字段

<a id="iws-world-u64-fields-v1"></a>

类型化标量入口覆盖 WorldState 的 time、next_industry_settlement_order、next_module_release_request_id、next_module_instance_id、next_module_market_order_id、next_module_market_sale_id 和 agent_claim_last_processed_epoch。沿用唯一字段登记：前六项属于 Allocator，最后一项属于 Scheduler，均为 Value；不把 WorldTime 当成 RootSet.simulation_tick，不将任何业务字段擅自迁入 Business。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

每个字段写一条 Metadata 和一条以规范 Unit 语义键索引的 Entry/U64 记录，payload 为8字节大端。零、u64::MAX 和字段原值完整保留；这七项均是必需标量，缺 Metadata 或 Entry 是错误，不默认零，也不提供删除入口。创世仅借用这七个原字段，最多输出14条带 namespace 和 reservation 的暂存写，不遍历其他状态或历史；失败不返回完整结果，调用方丢弃全部暂存前缀。

七字段创世生产、固定根类型化读取及当前事务 overlay 点读/点写已提供；完整 World 接线仍待完成。固定根读取使用同池同 class 的 Ordinary/Recovery reader，按已登记结构键的 namespace 在状态 I/O 前拒绝错根，缺失 Metadata/Entry 均报错；事务内读写须复用共享字段上下文，按结构键的已登记 namespace 查询同一 ExecutionTransaction。共用键/presence/固定8字节认证，U64 codec 必须显式匹配，不能把 I64 同位模式或错误字段的值当成合法 u64。事务写在同一 contribute 内检查当前 guard/live fence、同池 Ordinary、当前值 expected-before 和实际 writer 输出键，再直接贡献该字段最终根；no-op 保留读依赖，任一失败/取消使整个事务不可 seal。读、写及状态查找持续使用调用方的有限额度，owned 写键在分配前取得 reservation。

标量 setter 接受原 owner 已决定的值，不分配 ID、不替代 era/防重放规则、时钟推进或调度业务授权；包含 rollover 的变化仍须由同一完整事务更新相关控制字段。共享字段读取上下文只提供各 typed adapter 共用的有限读取与错误传播，不因通用化而授予字段/根 authority。此片不代表完整 World、C<T 恢复、复制/GC 或规模验收，结构化可增长字段仍须单独实现，禁止以整块 Serde 代替。

### 12.14 产品验证交付游标

<a id="iws-delivery-cursor-v1"></a>

product_validation_delivery_cursor 沿唯一字段登记保持 WorldState/Value/Scheduler，不能用 simulation_tick 或最高事件 ID 单独替代。其必需 Metadata 与规范 Unit Entry 共同认证，Entry 使用 field-member.v1 的封闭 DeliveryCursor codec（tag 8），payload 固定16字节：event_id_era u64大端，随后 routed_through_event_id u64大端；零和最大值完整保留，不用 Serde 或整段历史恢复该游标。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

创世生产借用原游标，仅输出两条带 reservation 的 Scheduler 写；缺失 Metadata/Entry、错 namespace、错 codec、固定长度错误均报错，不默认游标零值。固定根读取沿用同池/class、有限键/I/O/内存和逐节点取消合同，Ordinary/Recovery 均可使用。生产入口只保存原 owner 决定的 era-aware 游标，不调用 advance_to 或授权重送；完整事件身份、原结果、交付账本和共享事务仍须原提交链路共同认证。事务内入口在同一 contribute 中认证当前 Metadata 与 Unit Entry，比较完整 era/event ID expected-before；守住当前 guard/live fence、同池 Ordinary、组合读写取消及封闭 writer 输出键。no-op 保留两条读取依赖，任一失败使整笔事务不可 seal。它直接保存原 owner 决定的最终游标，不自行调用 advance_to；跨 era 的后续合法交付仍由原业务合同决定。该单字段接线与持久成功不代替完整 World 或 C<T 续跑资格。

### 12.15 结算与回收 ID 集合

<a id="iws-world-id-sets-v1"></a>

completed_logistics_route_ids、retired_factory_ids 保留原 String ID；settled_logistics_transit_ids、settled_factory_build_ids、settled_recipe_job_ids 保留原 ActionId/u64。五项沿唯一登记保持 WorldState/Set/Authorization，各自必需 Metadata，成员以原完整语义键索引 Entry/Unit（零 payload）。文本与数值键保持不同规范 token；类型不符的请求必须拒绝，不能把字符串“1”当 ActionId 1。零、极值、空字符串及 Unicode 原键不能裁剪或改写。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-0ca67e20-ffdf-4741-aa8f-b7d827a8f11c、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

创世按源集合借用逐项输出，不 clone World 或先收集全部 ID；记录数、键工作、I/O、取消及进程池额度持续有限。空集合仍保留 Metadata；缺必需 Metadata 是错误，只有在认证有效字段根后查不到成员才是 false。固定根点查使用同池同 class 的 Ordinary/Recovery reader，认证原键、Unit codec 和零 payload。全部暂存输出保持 provisional，失败须整体丢弃。集合点变更将由同一业务事务贡献，不能将集合落盘视为结算授权、完整 receipt、实际费用或交付已完成；相应 owner 仍须原子提交原结果及必需控制字段。集合事务点查及增删在同一 contribute 中认证当前 Metadata/成员，核对当前 guard/live fence、同池 Ordinary、完整类型化 key 与 expected-before；no-op 保留依赖，失败或取消使整个事务不可 seal。删除只移除成员，永不删除必需 Metadata。创世、点读及事务增删仍不构成完整 World 资格或上层结算链路已接通。

### 12.16 固定数值映射

<a id="iws-world-number-maps-v1"></a>

industry_settlement_orders 保留 ActionId/u64 键与 u64 原序号；agent_claim_acquisition_sequences 保留 String 键与 u64 原序号，两者属于 Allocator。contract_pair_last_success_settled_at、reputation_reward_window_started_at 保留 String/u64；reputation_reward_window_accumulated、reputation_scores 保留 String/i64，四项属于 Business。main_token_treasury_balances 保留 String/u64，属于 Business；main_token_claim_nonces、main_token_transfer_nonces、node_redeem_nonces、governance_authorization_nonces 保留 String/u64，属于 Authorization。十一项沿 WorldState/Map 的唯一字段登记，必需 Metadata 和原键 Entry/U64 或 Entry/I64；codec 不允许有符号/无符号位模式互换，文本键与数值键不得互换。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

创世借用对应 map 逐条输出，空 map 仍保存 Metadata，不先收集或 clone 全部键值。固定根点读只有认证有效字段且成员缺失时返回 None；Some(0)、i64::MIN/MAX 和 u64::MAX 保留原值，缺 Metadata、错误键类型、错误 codec 或 namespace 均拒绝。所有读写沿用有限键工作、I/O、记录数、同池/class、预准入与取消规则。事务入口在同一 contribute 中认证当前 Metadata/成员、检查当前 guard/live fence 与同池 Ordinary，并比较完整 Option<有符号/无符号数值> expected-before；None 与 Some(0) 不合并。新增和替换使用原封闭 codec，删除只移除成员，必需 Metadata 保留。键/数值类型不符、前置冲突、取消或额度失败使整笔事务不可 seal；no-op 保留读取依赖。原 owner 的完整业务结果仍需接通，不把数值 setter 当成结算、声望或 ID 分配授权，也不以部分字段 root 取得完整 World 资格。

### 12.17 认证采集 nonce 映射

<a id="iws-authenticated-nonces-v1"></a>

authenticated_collect_data_last_nonces 沿唯一登记保持 WorldState/NestedMap/Authorization，原 outer player_id 与 inner public_key 均以完整 String 编码，nonce 保留原 u64。必需 Metadata 之外，每个玩家（包括空 map）有独立 Group，成员使用规范二元语义键 (player_id, public_key) 和 Entry/U64；不能连接字符串代替二元键，不能规范化或重写公钥文本。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

创世借用原 nested map 逐组逐项流写，不 clone 玩家 map 或汇总全部 nonce；空玩家 Group 仍输出，记录数、键/I/O、内存与取消使用持续有限额度。固定根点查分别返回 PlayerAbsent、KeyAbsent、Value(u64)，不将三者或 Value(0) 合并。Metadata 缺失、当前查询成员的 orphan、原键/codec/namespace 错误均拒绝，目标 Group 与成员必须共同认证。PlayerAbsent 只说明该查询路径的玩家 Group 标记缺失，点读不扫描兄弟公钥，不能据此证明整个玩家子图没有 orphan；接入业务授权前必须依赖 §12.2 的完整字段族资格，当前点查入口本身不产生该资格。读者支持同池/class 的 Ordinary/Recovery，所有暂存写在完整调用成功前 provisional。事务点读/更新在一次 contribute 中认证当前 Metadata/Group/成员，精确比较三态 expected-before，并检查当前 guard/live fence、同池 Ordinary、组合取消及 writer 实际返回键。新增玩家 Group 必须显式 create_player；该标志可单独创建空 Group，删除只移除目标成员，不推定无兄弟键或删除 Group。任何失败使整笔事务不可 seal，no-op 保留三条读取依赖。原验签、nonce 单调性、费用与原业务结果仍须由 owner 在共享事务内接通，存储记录本身不授予采集或重放权限，不代表完整 World/C<T 续跑资格。


### 12.18 授权 MapSet 原键成员

<a id="iws-authorization-mapsets-v1"></a>

data_access_permissions 与 module_release_role_bindings 沿 WorldState/MapSet/Authorization 唯一登记。必需 Metadata，每个原主体 String（包括空集合）有 Group；成员以规范二元语义键 (主体, 原成员 String) 写 Entry/Unit，不能拼接、规范化或重命名任一部分。创世借用原 map/set 逐条输出，不 clone 或收集全部成员。固定根点读分别返回 GroupAbsent、MemberAbsent、Present，只有认证 Metadata、目标 Group 与成员之后才返回；缺 Metadata、目标 orphan、错误 codec/namespace 均拒绝。GroupAbsent 只描述当前 Group 标记，不能证明所有兄弟成员无 orphan；业务授权仍要求 §12.2 的完整字段族资格及原 owner 校验。原角色或权限含义不由该存储入口授予。沿用有限记录数、语义键、I/O、同池/class 预算及取消；共享事务入口在同一次 contribute 中认证当前 Metadata/Group/成员并比较完整三态 expected，核对当前 guard/live fence、同池 Ordinary 及读写组合取消。新增组必须显式 create_group，允许单独建立空组；删除只移除目标成员，不能从点查推定空组并删除 Group。no-op 保留三条读取依赖，任何冲突、预算或取消失败使整笔不可 seal。原 owner 仍决定权限/角色变化，存储 setter 不提供业务授权或完整 World 激活。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。
### Runtime 提交候选组合

<a id="iws-runtime-commit-v1"></a>

Runtime 组合入口将同一原 operation、parent、authority fence 和所有嵌套 owner 的变更约束在一个 ExecutionTransaction 内，再以 seal 后的完整 TransitionRecord 构建 HistoryPrefix、CommitBody 和 CommitCatalog 扩展。父材料必须同时绑定原 GenesisConfig、RootSet、CommitBody、HistoryPrefix 与 catalog 的 scope、height/count、尾项和 checkpoint 指针；还须从父 body 认证的 catalog-before 重现其精确扩展，不能只凭可读的最新目录项认可另一份 catalog-after。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

入口在执行前、seal 时及候选返回前核对原 owner 提供的 live fence；任一 owner、前置条件或 fence 失败都不返回完成候选。候选保留 transition、history、catalog 与规范根字节各自的同池 reservation，外层只读 carrier 也持续计费，不能凭持有候选获得 authority、接受回执、head 选择或缓存安装权限。RootSet digest 只用于本格式材料绑定，不替换原业务请求、结果、上下文或操作身份。

候选的原协议绑定使用现行 CanonicalRecord、VerifiedAuthority 与适用 VerifiedFormalCommit parent；结构合法和物理持久化都不授予原协议接受。完整对象闭包、独立持久回执、Unknown writer 隔离、接受后的 head/运行态安装和维护恢复仍须由原 adapter 按§5、§7执行。回调式 prepare 不是普通 World step 已经切换的证明，不能以未接通的 owner 回调代替全字段写集或用完整 snapshot 补增量字段。

局部分量测试分别核对原 parent 材料冲突、fence 各边界、同池预算、完整候选内容及 Raw/Zstd 原件/页持久后关闭重开与后继候选一致。该分量仍保留 fixture World 上下文，不能声称完整新世界激活、从认证根重建所有字段、C<T 非空 TransitionRecord 后缀恢复、J1/J2/provider oracle 或规模验收通过；这些整体交付要求继续适用。

<a id="iws-runtime-rolling-allocators-v1"></a>
### Runtime 滚动分配器

Runtime 的 event、action、intent、proposal 四组 next/era 使用同一 allocator 根中的四个固定键，创世从原字段借用读取，不从已有最大 ID 推断。event 沿用原 EventSequenceV1 键与规范 bytes，不再写第二份通用字段；其余三组使用 `oasis7.iws.runtime-sequence-key.v1\0` 加 action=1、intent=2、proposal=3 单字节，值为 `oasis7.iws.runtime-sequence.v1\0`、同一单字节与 next/era 两个大端 u64。未知标签、跨组值、长度或 domain 错误拒绝。

分配贡献从同一 ExecutionTransaction 的认证 overlay 读完整 next/era，与调用者的精确期望值核对，然后复用原滚动规则：零归一为 1、最大 ID 后 next=1，era 饱和加一。它返回原分配 ID 和新 next/era；不修改活 World，不单独授予事件或业务接受资格。事件 owner 已贡献分配时不得再次调用分配入口。重复分配读取本次 overlay，必须使用更新后的期望值。固定读与写必须同池 Ordinary、有限 I/O，错误或最后取消检查失败 poison 包括先前成功贡献的整笔事务。恢复只读这些原值，不另行分配。四组字段不代表预分配 provenance、其他计数器或完整 Runtime 控制覆盖。Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

<a id="iws-world-clock-owner"></a>
### 世界时钟的唯一字段表示

WorldState.time 只使用字段登记的 WorldState / Value / Allocator profile：Metadata 存在标记和以 Unit 原键封装的 U64 Entry。Runtime 的 body、事件移动和 scheduler 检查同一 Metadata 与 Entry 的精确期望值；已知固定值可按该规范重新计算值 ID 后核对认证路径，不必读取或物化其他字段。完整创世对 time 只选择一个 owner 生产者，拒绝重复输出键；不得同时调用 WorldState 标量家族生产者和 Runtime 的分量时钟 helper。字段 visitor 只是借用分类接口，不作为任意 Serialize 落盘入口。创世生产与控制字段 setter 使用同一 profile，不再生成独立 world-time-key/value 副本，也不保留双写或旧键回退。该统一针对尚未正式启用的新格式组件，不承担旧世界导入。RootSet.simulation_tick 与 commit height 仍是独立语义。

时钟不符、缺 Metadata/Entry、同池检查或最后取消失败均沿共享事务贡献传播，不能 seal 先前成功前缀。固定两项编码、比较、输出遵守 Ordinary 预先准入及调用方连续 I/O 预算；不为校验扫描 World、历史或所有字段。控制字段变化后的 Runtime 后继读取必须看同一 overlay，不能继续采用旧专用时钟。Trace refs: O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。


### 12.19 原文本映射

<a id="iws-world-text-maps-v1"></a>

module_artifact_owners 属于 Business；node_identity_bindings、node_main_token_account_bindings 属于 Authorization，三项均按 WorldState/Map 的唯一登记保留原 String 键和原 String 值。Metadata 必需，成员为原键 Entry/Text，不做裁剪、大小写转换、Unicode 归一化、别名替换或业务身份授权。创世借用原 map 流写，不 clone/收集整张 map。固定根点查先认证 Metadata 与目标原键，只在已认证字段中返回缺成员；空字符串返回 Some(0)，与 None 区分。Text payload 必须完整通过 UTF-8 验证，跨块仅保留最多三个未完成字节，并沿已有单次 write/accepted-prefix 取消规则输出；错误 codec、损坏/截断或非法文本不能作为业务值采用。输出在完整调用及最终取消检查成功前 provisional，调用方为保留输出预先准入。记录、语义键、payload/总长、I/O、同池/class 与取消均有限；不把该存储入口当公钥绑定、账户权限或完整字段族/World 资格。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

文本映射事务点读与点更新在一次 contribute 中认证 Metadata 与当前原键成员，读取依赖进入同一 ExecutionTransaction。更新核对精确 guard/live fence、读写同池 Ordinary、原 expected 的 None/空字符串/完整 UTF-8 字节，随后保存 owner 决定的原 after 或 tombstone；不收集完整旧值，流式比较只借用 expected。no-op 仍保留两条读取依赖。旧值读取与新值写入分别受明确 payload/record 上限，读写取消组合检查直至 transaction.write 后；原键、namespace 或预期值不符、错误 UTF-8、预算/取消及任一后续错误均 poison 整笔事务，不能 seal 已有前缀。原 owner 仍负责账户/公钥/工件身份授权及跨字段业务规则，该 setter 不授予权限或完整字段族资格。


### 12.20 原已安装模块目标表

<a id="iws-installed-module-targets-v1"></a>

installed_module_targets 沿 WorldState/Map/Artifacts 唯一登记，保留原 String map key 及原 ModuleInstallTarget 变体。Metadata 必需；SelfAgent 为原键 Entry/Unit，LocationInfrastructure 为原键 Entry/Text，Text 保存原 location_id（包括空字符串），不裁剪、归一化或代换。复用现有封闭成员 codec，不将位置 ID 与 SelfAgent 混合；字段绑定和 codec 共同决定原类型，不重用文本值推断变体，不增加任意 Serialize 回退。

创世借用原 map 逐条流写，每条共享有限 records、key-work、I/O、semantic/payload/record bytes、同池 Ordinary 与取消；分配前准入的写记录 charge 交给 sink，源对象与批次索引由调用方准入，失败整段丢弃。固定根读取认证 Metadata 与当前原键，返回 Absent、SelfAgent 或 LocationInfrastructure 的原 UTF-8 字节长度，位置文本流式输出；只在全调用及最终取消成功后采用输出。错误 codec、损坏文本、错误字段/namespace、取消或预算不足不当作缺失。点读只核对此原键，不签发完整字段族资格；目标与实际 module instance/工件的一致性、合法安装和权限仍由原 owner 在共享事务内验证，本入口不提供安装授权或完整 World 恢复资格。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

目标表共享事务入口在同一次 contribute 中认证 Metadata 与目标成员，以完整原变体及原位置字节核对 expected；Absent、SelfAgent 与空位置均不合并。旧位置流式比较，不收集完整字符串；复用文本映射的借用字节比较器，变体判断保留在各 owner adapter 中。更新核对当前 guard/live fence、读写同池 Ordinary、有限原位置/record/key/I/O 额度及组合取消，再写原 after 或成员 tombstone；不移除 Metadata。no-op 保留两项读取依赖，并在可能较长的原字符串比较后再次检查取消。任何错误使整笔事务不可 seal，暂存页/输出不因先前成功贡献取得资格。目标与模块实例、工件及原权限的一致性仍由 owner 接通，本 setter 不签发安装或完整 World 资格。


### 12.21 原账户及节点余额记录

<a id="iws-world-account-balances-v1"></a>

main_token_balances 与 node_asset_balances 均沿 WorldState/Map/Business 唯一登记。各有必需 Metadata，以原 String map key 写 Entry；记录内部 account_id/node_id 独立保留，不能用 map key 回填、裁剪或归一化。主代币记录依次保存 liquid_balance、vested_balance、restricted_starter_claim_balance；节点记录依次保存 power_credit_balance、total_minted_credits、total_burned_credits，全部保留原 u64 位，零值不同于缺成员。三个量的业务含义及余额约束仍由各原 owner 验证，存储入口不授予转账、铸造或兑换权限。

复用 field-member.v1 framing，新增封闭 payload codec IdentityU64Triple/tag9：u64 BE 原身份 UTF-8 字节数、原身份字节、三个按上述字段顺序的 u64 BE；最小长度32，且声明长度必须等于32加原身份字节数，不用 JSON/任意 Serialize 回退。此物理 codec 只复用两种实际记录的共同布局，字段绑定仍决定含义；未知 codec 的旧 reader 必须明确拒绝，不能猜测为旧字段，正式新格式激活仍要求完整 reader/字段族资格。

创世借用原 map/记录逐条流写，不 clone 或收集整表；原记录用无省略的类型解构，新增成员必须明确处理。固定根点读先认证 Metadata 与原 map key，并校验 tag9/原键长度；原身份 UTF-8 流式输出，header 与三个数值仅保留固定缓冲，不构造完整旧身份。输出在完整记录、原身份长度/UTF-8、数值尾部与最终取消成功前保持暂态，成功返回原身份字节数和对应三个具名量。调用方准入源、查询键、保留输出和批次；records/key-work/I/O、原身份及总记录字节、同池/class 均有限，取消或预算错误不当作缺失。点读只证明该记录，不验证兄弟记录、完整字段族或 World/C<T 恢复资格。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

余额共享事务入口以具名借用 view 同时表示原身份和三个量，view 由原结构无省略解构得到，创世与点更新复用同一映射。field 与 view 类型不符立即拒绝，不能把节点余额的三个数按主代币余额解释。一次 contribute 认证 Metadata 和当前成员，流式比较 expected 原身份，并精确比较原具名数值与缺失状态；不收集旧身份，不以 map key 回填。检查当前 guard/live fence、读写同池 Ordinary、有限原身份/record/key/I/O 与组合取消后，保存 owner 的最终 after 或成员 tombstone；Metadata 不删除。no-op 保留两项读取依赖，并在可能较长的 view 比较后再查取消。类型/预期/原记录/额度/取消或后续写入失败均 poison 整笔事务。view/存储 setter 不校验供给、业务余额约束、转账/铸造权限或全字段资格，原 owner 仍须在同一共享事务内接通这些规则。

<a id="iws-factory-records-v1"></a>
### 12.28 工厂与待建造记录

factories 保持 WorldState/Map/Business，pending_factory_builds 保持 WorldState/Map/Scheduler 唯一登记。外层 Entry 以原 String/u64 map key 为身份，保留完整原 semantic preimage；不能用记录内 factory_id、job_id 或 spec.factory_id 替换，也不裁剪、归一化或合并真实 ID。每个字段保留必需 Metadata 和 Entry/Unit 格式标记，标记键为 (factory_record_closure_v1, unit)。此为尚未发布的新 profile，拒绝旧平面列/成员，不从旧 tuple 证据推定新 profile 已验证。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

外层实际记录仅写 field-member.v1 Entry/RecordRoot(tag10)，固定165字节 payload 为[记录根引用](#iws-record-root-reference)。Factory schema 固定32字节 ASCII oasis7.factory.state.record.v1 加两个零字节，Build schema 为 oasis7.factory.build.record.v1 加两个零字节；必须同时绑定记录种类、原 outer semantic digest、namespace 和完整 child StateRoot。child 只有经此类型边才是该记录的可达内容；不能将未知散落全局列吸收为新记录。解析 RootRef 本身不授予闭包资格。

每条记录有独立 child StateRoot，含同字段描述符的 Metadata、相对 unit key 的 Entry/Unit 记录标记及全部真实叶子。列键固定为 ((unit, section), 原列名)。Factory 顶层 section=factory，Build 顶层=build，共享 spec、Factory 的 production、last_completed_canonical_snapshot，以及 Build 的 construction_power_obligation，均由封闭枚举选择。FactoryState、FactoryBuildJobState、FactoryModuleSpec、FactoryProductionState、FactoryProductionSnapshot、FactoryBuildPowerObligationV1、MaterialStack 无省略解构。u8/u16/u32 保存原 U64 并校验范围；生产状态 Idle/Running/Blocked/Paused 对应0/1/2/3，StartOnlySink 对应0，未知枚举拒绝。Option 叶子 None 为 Unit，Some 为具名类型；快照和电力义务另有 Bool 存在列。Text/ledger 保存原字节，不用任意 Serde 或整工厂 clone。

集合标记键为 (((unit, section), 原集合名), unit)。spec 只允许 tags/build_cost；快照只允许 consume/produce/byproducts/bottleneck_tags/logistics_route_ids/logistics_path_ids；电力义务只允许 material_balances_before/material_balances_after。Vec 标记保存 U64 原长度，包括0，optional 父结构缺失为 Unit；Text 项在集合前缀后配对原 u64 index，MaterialStack 再配对 kind/amount。保留次序、重复项、原材料名和 I64 位。两个可选 map 分别保留 Unit(None)、U64(0)(Some空map)、U64(n)，独立成员在集合前缀后配对原 String key，保存 I64。

普通点操作只认证外层 Metadata/格式/原 Entry RootRef 及目标 relative key 的必要祖先，不扫描未变 Spec、长 outer ID 的子树副本或其他集合。读支持同池/class Ordinary/Recovery，写要求同池 Ordinary、精确 guard/live 与 expected；更新 inner 路径后贡献唯一外层 RootRef。创建从 empty 构造完整新 child，不采用松散旧字段。显式整记录替换先认证 expected 的全部具名列/成员，并验证当前真实 child 关系完整，不能只相信声明 leaf_count 或 expected 提供的子集；删除仅在此完整关系成立后移除外层引用。固定 scratch 在解析/分配前准入，records/key/I/O/work/scan/pending-node 额度有限且连续，取消和失败 poison 整笔事务，no-op 比较及最后实际 put 后再次检查组合取消。

原始根、创世和恢复必须对所有 Factory 外层条目和 child 做有界 qualification：校验原 key preimage 与 digest、严格白名单、全部必需列、Option 父子关系、真实集合 cardinality/Vec index、类型和 namespace，拒绝旧 flat/orphan/缺页/未知字段。opaque 资格绑定完整 RootSet 和本 schema。通用事务 write 可绕过 producer helper，接收端必须在通用 TransitionVerifier 之外验证所有本族 outer 变化及 changed child closure，才可继承新 RootSet 的资格。一般 verify_state_values 只证明顶层 ValueTree，不证明这些 nested 状态页。

复制和 GC mark 必须实际沿已绑定 typed child edge 遍历子状态页及其 ValueTree；只保留外层 value pages 不完整。所有暂存输出与复制前缀在完整验证成功前 provisional，不发布部分资格。此组件不改变建设、生产、权限和电费规则，不把独立 receipt/recipe job 算入两个字段，也不单独授予完整 World、恢复/持久性或业务执行资格。

<a id="iws-record-root-reference"></a>
### 记录闭包的类型化子状态根引用

动态记录使用独立子状态树时，外层原 key 的认证 entry 承载封闭的 RecordRootRefV1：`oasis7.iws.record-root.v1\0`、大端 u32 正文长度、owner 定义的 32 字节 schema 身份、原语义 key 的 32 字节 digest，以及原 StateRootV1 的完整 canonical bytes。schema 身份必须区分记录种类；子根 namespace 必须与 owner 的固定预期一致。长度固定，拒绝尾随字节、未知版本、错误绑定或非法子根描述，不以任意 Bytes/Serde 包装后静默忽略子根。

该 codec 仅提供完整性及明确的子根类型边。外层值须先认证；owner 随后核对 schema/key/namespace，验证子树的字段白名单、必需列、Option 和集合关系，并使资格绑定当前根与 schema。通用 StateRoot/TransitionVerifier 校验不等于记录闭包资格；通用事务写入不能绕过 owner 的接收验证。复制和 GC 必须沿该类型边遍历子状态根，不能只遍历外层 value pages。创世、原始根准入、同 overlay 更新、资格继承及可达边维护由具体 owner 实现和验证；本 codec 本身不宣称这些链路已接通。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-90d4c44a-8042-4f63-94fb-3389d8474e2c。


<a id="iws-industry-job-records"></a>

### 12.30 工业任务与终态回执记录

本族覆盖原 Scheduler Map 字段 pending_recipe_jobs、pending_material_transits，以及原 Authorization Map 字段 recipe_completion_receipts、factory_production_failure_dispositions、logistics_settlement_receipts。立即 material transfer 的独立回执、行业进度、世界规则工业图、Factory 记录及原 settled-ID/order 字段保持各自 owner，不以本族资格代签。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c；沿用 §12.28 的计费、取消和闭包合同；本节不授予业务结算权限。

每个原 ActionId outer key 对应一个 RecordRoot(tag10)，绑定原 u64 semantic digest、原 field descriptor、namespace 与本族五个独立 schema-id 之一；不得以 record.job_id/action_id 替换外层键。五个 schema-id 分别为 ASCII oasis7.industry.recipe.v1、oasis7.industry.transit.v1、oasis7.industry.completion.v1、oasis7.industry.failure.v1、oasis7.industry.settlement.v1，右侧补零至32字节。外层各有同 descriptor Metadata 与 (industry_record_closure_v1, unit) Entry/Unit 格式标记，拒绝旧 flat profile。child 保留同 descriptor Metadata、相对 unit 的记录 marker。固定列使用 ((unit, section), 原字段名)；section 白名单为 record、industrial_root_ref、committed_recipe_origin、submission。列表 marker 为 (((unit, record), 原字段名), unit)，按原 u64 index 分成员；MaterialStack 每项分 kind/amount 两列。owner_payouts 按原字符串 key 分成员。空集合必须有计数零 marker，不能与缺 marker 混同。工业引用六项、提交身份七项、CommittedRecipeOrigin 的六项均逐字段保存；Option 原 None/Some 和可达字段关系必须认证，不使用任意 Serde 或整记录 JSON。WorldTime 保留原 U64；accepted_batches/duration_ticks/reroute_count 保存 U64 并拒绝超过原 u32 范围；Priority 的 Standard=0、Urgent=1，其他值拒绝。两个可选嵌套对象以原字段 Bool 存在列标记；None 时其每个固定子列为 Unit，Some 时除 hosted_registration_nonce 外的必需子列不得为 Unit。

借用创世完整枚举五族全部真实字段，遍历与分配先获有限 quota/Ordinary admission；标量、账本、长文本、集合成员复用既有封闭编码。正常 point 只沿 outer RootRef 和目标相对路径，不扫描未变 payload/成员。新建只能依据绑定同一父 RootSet 的 opaque 闭包资格，不能凭缺 marker 接纳孤儿。原始根/恢复以有限预算认证完整顶层及 typed child 值/状态页、白名单、键 preimage、Option 关系和真实集合计数；通用 transition verifier 之外必须从 qualified parent 与完整 changed-write-set 增量验证本族闭包。通用顶层 values marker 不代签 typed child 闭包。

公开 oracle 应独立列出五族原键/原值，覆盖原 outer/embedded ID 不同、None/空/非空、重复列表值、原 Unicode 与整数极值；冷读固定小池，普通 Δ 扩大未变 sibling 时计数不得跟随其大小。错误/取消发生于准备、no-op、真实 put 和最终 buffered flush 时不得接受 token/mark/root；typed transfer 必须先成功 finish 目标 sink 再接受闭包回执，根恢复必须在新 pool 重开验证。

<a id="iws-logistics-route-records"></a>

### 12.32 物流路线与已完成路径

本族仅覆盖原 WorldState/Map/Business 的 logistics_routes 和 WorldState/Map/Authorization 的 completed_logistics_paths。原 String outer map key 与记录内 route_id/path_id 分别保留原字节，不裁剪、归一化、排序或用其中一个回填另一个。原 LogisticsRouteV1 的11字段、LogisticsPathAuthorityV1 的7字段均须无省略借用解构；此两 DTO 无 Option 字段，不新增可选语义。账本沿原封闭 Ledger codec；MaterialTransitPriority 的 Standard=0、Urgent=1，拒绝其他值；I64 包括负数/极值原位保存，存储层不自行执行路线可用性、容量、收费或 recipe 数量业务规则。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

每族外层有同 descriptor Metadata 和 (logistics_record_closure_v1, unit) Entry/Unit 格式标记。每条原 String key 的 Entry/RecordRoot(tag10) 绑定原 semantic digest、namespace 和分别为 ASCII oasis7.logistics.route.v1 / oasis7.logistics.path.v1 右补零至32字节的 schema。子树有 Metadata、相对 unit 记录 marker 及全部真实字段；列键为 ((unit, record), 原字段名)。Path route_ids 独立 Vec marker 为 (((unit, record), route_ids), unit)，保存包括0的原长度；每个 Text 成员以原 u64 index 为尾键，保留次序与重复 ID。不得以整体 JSON/Serde blob 代替字段和成员。

借用 GEN、正常创建、显式完整读取和原始根/恢复 qualification 承担各自明确的完整范围，使用调用方连续有限 work/records/key/state/value/pending 预算及预先内存准入。创建需同父 RootSet 的 opaque family 资格；raw qualifier 认证原键 preimage/hash、outer RootRef 绑定、全部必需列、真实 Vec count/dense index、所有状态/值页闭包并拒绝旧 flat、缺 marker、未知字段/类型和缺页。普通标量或单 Vec 成员更新只沿 outer reference 与目标相对路径，保留原 guard/live、expected、同池 Ordinary、取消/no-op 后及最后实际 put 后检查，错误 poison 整笔事务。

接收端在通用 transition replay 之外，从 qualified 父 child 和全部 changed paths 推导后继闭包；固定列/marker 不得增删，Vec 数量净变化及首尾 slot 必须与计数一致，不能每次扫未变长文本或整 Vec。typed mark/transfer 显式沿两族 RootRef 到子状态及 ValueTree，复制先准入；所有回执在成功 finish 最后缓冲发布前 provisional。根恢复必须丢弃原 pool/store 后在有限小池重开验证；通用顶层 ValueTree 资格不代签本族子根或完整 World GC/激活资格。

### 12.33 Material profile 与直接转移原始回执

`material_profiles`（Business，原 String 外键）与 `direct_material_transfer_receipts`（Authorization，原 ActionId 外键）各使用隔离的 record StateRoot；外键与原记录内 kind/transfer_id 独立保留，不能因不一致而归一化。每个 record 包含唯一 metadata、Unit 记录标记和封闭原字段，未知字段或缺失字段拒绝完整资格。原始大 Text 与 Ledger 身份沿既有流式 codec 处理，不将完整记录串行化为单个 Vec。

MaterialProfile 保留 kind、tier、category、stack_limit、transport_loss_class、decay_bps_per_tick、default_priority。tier 以 U64 精确承载 u8 范围；loss class 的封闭值为 Low=0、Medium=1、High=2；default priority 为 Urgent=0、Standard=1。直接转移回执保留 transfer_id、requester_agent_id、from_ledger、to_ledger、kind、amount、distance_km、priority、route_id；Ledger 用原完整 enum 身份，priority 为原 Urgent=0、Standard=1，route_id 以独立 Bool presence 和可选 Text 区分 None 与 Some(empty)。

GEN、完整 family 资格、本地列更新与接收端验证使用同一封闭 schema；本地变更必须绑定已验证原 record root、精确 expected 值与同一事务 guard。页传输在完整 typed 闭包与缓冲 sink finish 都成功后才可作为完整交付；这些能力不授予 authority、全 World 激活或全局 GC 资格。实现与物理验证仍须在当前 PR 提供，不由本节文字代签。

原始家族恢复通过全部 top-level roots 的连续预算扫描，认证两个命名空间内的 Metadata、格式标记、原始外键类型及每个完整 child；返回资格仅绑定这两个家族和原 RootSet digest。它不授予其他 owner 的完整性。RecordRoot 点读中的原 Text/U64 键在同一次认证 member 读取中流式校验，固定前缀和 UTF-8 状态不保留完整原身份。

普通事务的 Material 单列 setter 只认证指定原 expected 值并编辑该列；RouteId 同次编辑其 Bool presence，不能单独修改 flag 或删除必需列。创建绑定完整父家族资格和当前不存在的外键，原嵌入身份不替代外键；删除绑定当前完整 record 资格和相同事务 guard，不为移除外层引用重读大 sibling payload。接收端必须先完成通用重放，再从已资格父根按认证变化路径验证封闭列、必需 marker、Option 关系和家族计数，跳过相等子树。此片仍不授予完整 World 资格。

Material 的页标记与传输复用有界逐页追踪，在完整 typed 家族资格结束后返回原页 hash 集合；其他 owner 仍须独立贡献闭包。传输输出在所有缓冲 sink finish 成功前保持 provisional，最后物理批次发布后观察到取消也不得报告完成。恢复可另开操作从原认证根重新验证已经耐久的页。物理压缩沿用既有 codec 工作区准入，额度不足必须明确失败；不能通过省略压缩工作区计费获得成功。

Material 的普通事务 getter 与 setter 共用经过 guard、同池 Ordinary、完整当前 record 资格绑定的 resolver，读取同一 ExecutionTransaction 的当前视图。resolver 在全部列与 Option 关系校验后检查取消；getter 据此返回暂态值。setter 在精确 expected 和借用 after 比较后还须独立检查读/写取消通道，NoOp 也不能跳过。任何查询或更新错误都使当前贡献不可 seal；getter 的独立读取边界可作为 NoOp 最后检查回归的基线。

<a id="iws-agent-authority-records"></a>

### 12.35 Agent 意图账本与位置身份原记录

本节只覆盖 WorldState/Map/Authorization 的 agent_intent_ledger，以及 WorldState/Map/Business 的 agent_location_authorities、location_anchors。12.34 在独立外交交付分支固定，本节不复制或重排其内容。AgentCell.intent 的已有借用投影不是独立耐久 ledger 的生产或闭包证明；agents children、claims、starter OC、FactorySite/Power 与 Runtime/Cognition owner 不在本族范围。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

三族原 String outer key 和 DTO 内 intent_id/agent_id/location_id 分别保留，不回填、折叠、裁剪或归一化。AgentIntentV2 的22字段全部借用解构：schema_version 为原 u32 界内 unsigned；logical_time/event_seq/updated_at 与 Option intent_tick/reorg_epoch 为原 u64；其余字符串与八个 Option<Text> 保留原字节。status/kind/source 不转换为新 enum，schema_version 不替换为常量2。AgentLocationAuthorityV1 的5字段与 LocationAnchorV1 的4字段均为原 Text/Bool/U64；legacy false/0/default 原样保存。不存在动态 Vec/Map。Option 以固定必需列的 Unit 表示 None，以原 Text/U64 codec 表示 Some，包括 Some(empty)/Some(0)，不得省略为缺列。

每族必须有 Metadata 与 (agent_authority_record_closure_v1, unit) Entry/Unit 格式标记。每条原 outer key 对应一个 tag10 RootRef，绑定原 semantic digest、namespace 和各自 ASCII oasis7.agent.intent-ledger.v1 / oasis7.agent.location.v1 / oasis7.location.anchor.v1 右补零至32字节 schema。子根有 Metadata、unit 记录 marker 和 ((unit, record), 原字段名) 固定列；每个 kind 仅接受自身封闭列集合，拒绝未知、重复、缺列、错误类型/范围、旧 flat 与缺页。借用 GEN 与初始/原始恢复 qualification 对明确完整范围负责，连续使用调用方有限预算和先准入后分配，不能靠整体 JSON/Serde blob 或 AgentCell/完整 DTO clone。

正常创建要求同父 RootSet 的 opaque family 资格，显式完整删除绑定 exact qualified child；普通单列及 Option 更新必须有精确 expected、guard/live、同池 Ordinary 和组合取消，完整替换不是小变更唯一路径。后继接收端先通用 replay，再从 qualified parent 和全部 authenticated changed paths 推导闭包，不扫描未变 summary/reason 等长文本；必需列与 marker 不能删除。no-op、长文本相等返回前、实际 put 后及最终 buffered finish 的错误/取消均不可接受成功前缀。typed mark/transfer 显式追踪本族 child StateRoot 与 ValueTree，复制先准入，完成资格需等待 sink finish；原 pool/store 销毁后的有限小池根恢复须可验证。

本族资格只证明存储完整性，不授玩家、意图替换或站点准入权限；Agent存在、active anchor、revision/effective_at、事件序号、receipt与principal绑定仍由原业务消费者裁定。共用 intent 借用 helper 必须有 AgentCell 与 ledger 两个真实消费者，保持旧 Agent 字段键、ordinal、值 codec 和输出顺序；公开 oracle 应覆盖31原字段、十个 Option 的 None/empty/0、outer/embedded 不等、原 Unicode/极值、同 TX 操作、固定 Δ 放大未变文本、来源/预算/取消/真实发布故障与冷恢复。不得把本族 token 代签完整 World 激活或 GC。

### 12.36 固定经济全局值的封闭记录

<a id="iws-fixed-economy-records-v1"></a>

main_token_supply、reward_asset_config、protocol_power_reserve、reward_signature_governance_policy 沿用各自唯一 WorldState/Value descriptor；前三者为 Business，签名治理 policy 为 Authorization。每字段原 Metadata 与 Entry/Unit 语义键的 RootRef165 必须存在；每个全局值有同 namespace 的独立 child StateRoot，引用绑定字段专属 schema、原 Unit digest 与 namespace。child 包含原 Metadata、Unit 记录标记与原名 primitive 列；不得以旧 generic Value 叶、其他 Entry 键或任意 JSON 替代。本格式资格不自动承认旧投影，迁移和完整 World 激活由上层独立完成。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

MainTokenSupplyState 穷尽 total_supply/circulating_supply/total_issued/total_burned 四 U64；RewardAssetConfig 穷尽 points_per_credit/credits_per_power_unit 两 U64 与 max_redeem_power_per_epoch/min_redeem_power_unit 两 I64；ProtocolPowerReserve 穷尽 epoch_index U64 与 available_power_units/redeemed_power_units 两 I64；RewardSignatureGovernancePolicy 穷尽 require_mintsig_v2/allow_mintsig_v1_fallback/require_redeem_signature/require_redeem_signer_match_node_id 四 Bool。所有十五列强制存在，无可选全局对象、任意字段删除或整个对象删除。编码使用已有 Unit/U64/I64/Bool/RootRef，不新增 Member codec、不 clone 整 DTO；借用式 GEN 显式解构穷尽原值，保留零、U64 最大值、I64 极值及 false/true，不新增供给、兑换、储备、签名或授权业务约束，不触 reducer。

完整 root-only family 资格只凭认证 RootSet、schema 与有限 reader 检查四个外部绑定、六根错位、全部必需 child 列、primitive 完整 framing 与未知/孤儿拒绝；不依赖原 World snapshot。普通点读和单列精确 expected 更新只访问目标及必需身份关系，支持同事务连续后继；单列值类型必须匹配字段，guard/live、同池 Ordinary、独立 key/read/write/node/heap 计费、NoOp 与成功返回前组合取消、失败后全事务 poison 均不得省略。不得从局部读成功签发完整 family 资格。

正常 receiver 先独立验证 generic 转移，再按真实四字段外写及 child changed Merkle 路径验证原 schema、身份、primitive 和完整固定列关系；仅继承已 qualified 父的未变闭包，禁止移除或引入非本格式外叶。page mark/transfer 复用完整本组资格，追随所有 child state/value 页，有限集合在分配前计费；包括 buffered finish 在内的全部输出成功前保持暂态。此组不授予经济业务权限、协议接受或全 World GC；仍须组合其他 owner、历史/lease、未决和回滚保护。独立字面原值、Raw/Zstd 冷根/合法后继、generic-valid 但 typed-invalid、真实 put 故障、精确预算及原子失败分别验证；末次取消的独立边界回归须有独立前序基线，不能用被测函数自身总回调自校准替代。

### 12.38 系统订单池 epoch 预算与有序奖励铸造原记录

<a id="iws-reward-budget-records-v1"></a>

本族仅覆盖 WorldState/Map/Business 的 system_order_pool_budgets 与 WorldState/Ordered/Authorization 的 reward_mint_records。Pool 原 u64 epoch 外键与 DTO 内 epoch_index 独立；Mint 原 Vec ordinal 是外键，DTO epoch_index/node_id/签名等仍按原值保存，不排序、去重或归一化。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

每族要求 Metadata 与 (reward_budget_record_closure_v1, unit) Entry/Unit 格式标记。每条原 U64 外键/ordinal 以 Entry/RootRef165 绑定原 semantic digest、各字段 schema 与 namespace；记录 child 有 Metadata、Unit marker、原名固定列。Pool 的 epoch_index/total_credit_budget/remaining_credit_budget 为 U64；node_credit_caps/node_credit_allocated 各有独立 Unit collection marker 与原 String→U64 成员，保留空 String、0、u64::MAX 和两个 map 的独立缺失/存在关系。Mint 的 epoch_index/source_awarded_points/minted_power_credits 为 U64，node_id/settlement_hash/signer_node_id/signature 为原 Text，均无 Option，不引入字段省略或任意 Serde blob。

Mint 另有 (reward_budget_record_closure_v1, length) Entry/U64 全局长度，原 ordinal 必须精确形成 0..length-1，重复内容仍是不同 ordinal。借用 GEN 与 root-only 恢复资格完整校验原格式、required 列、动态成员与 dense ordinal；连续使用有限 state/value/key/cell/heap/pending 预算，不需要原 World snapshot。普通 append 只创建新 ordinal 的完整 child 并更新 length；receiver 从 qualified 父与实际 changed Merkle 路径验证净计数/索引/长度，不扫描未变大签名或重写整个 Vec。既有记录单列、Pool 成员点写须精确 expected、同事务 guard/live、同池 Ordinary、最终组合取消与错误 poison；完整对象删除只可在隔离关系与父资格允许时显式提供。

页闭包由本 owner 的完整资格派生，显式跟随 child state/value，有限集合分配前计费；含 buffered finish 的全部输出完成前保持 provisional。本组不授奖励、mint、预算分配或签名业务权限，不改变原 reducer；仍不代签其他 owner、完整 World 激活或 GC。原始字面 oracle、非 Optional 必需列边界、Raw/Zstd 冷根、合法 append/同事务后继、generic-valid 但 typed-invalid、预算/取消/真实 put 故障及小增量成本分别验收。

<a id="iws-factory-authority-records"></a>

### 12.37 场地权限与建造供电配置

`factory_site_authorities` 和 `factory_construction_power_profiles` 均沿原 WorldState/Map/Business descriptor 保存。原 String 外键与 site_id/factory_id 分别保留，不作归一化或互相回填；字段级存储不授予场地准入、模块治理或供电业务权限。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

每族外层含同 descriptor Metadata 与 `(factory_authority_closure_v1, unit)` Entry/Unit 标记。原 String 键的 Entry/RecordRoot 使用标准 tag10，schema 为 BLAKE3(`oasis7.iws.factory-authority.v1\0` 连接原字段名 UTF-8)，绑定原 semantic digest 与 Business namespace。子树含 Metadata、unit 记录 marker 和各原字段，相对列键为 `(unit, 原字段名)`。授权列表以独立 authorized_agent_count/U64 和 `((unit, authorized_agent_ids), 原u64索引)` Text 成员保存，保留原顺序、空值和重复值；业务更新的规范化不由存储重做。

Site 原八字段全部借用解构；Power 原七字段全部借用解构。source_module_id 使用独立 Bool presence 与可选 Text 区分 None 与 Some(empty)；electricity_amount 保存完整 I64，StartOnlySink 的封闭 Mode/U64 为0。GEN 连续计费所有 outer rows、metadata、marker、固定列和列表成员，所有输出在完整调用成功前 provisional。普通修改、原始闭包资格、receiver 和页传输仍须按本设计各自完成，不能以 GEN 或字段投影测试代签完整 World、GC 或接通状态。

显式 child bootstrap/恢复资格只保留固定工作区：先认证全部必需列及 Option 关系，再按声明数量逐点认证列表的每个原索引，最后完整扫描认证实际键路径和叶数。必需键全部存在且真实叶数恰等其数量，才能排除额外未知列或列表洞；不保留随列表增长的键表。资格仅绑定该 kind、原 owner semantic digest 和精确 child root。外层 inspect 仍只证明 metadata/格式及一个原 String 键的 RootRef；点读须核对该外层引用与 child 资格相符，不能把单个 child 证明当作全家族或完整 World 资格。

普通列更新与 overlay getter 共用相同 guard、同池 Ordinary、metadata/格式和当前精确 child 资格解析。标量和已存在授权成员只改目标路径；source_module_id 同事务编辑 presence，禁止直接改独立 flag、Unit marker 或授权数量。数量及成员集合的结构更新由专门操作保持密集索引。getter 在认证值和 Option 关系后检查取消；setter 在原 expected 的逐字节/原整数比较后，还须检查组合读写取消，NoOp 也不豁免。任何失败使整笔贡献不可 seal，先前成功前缀不得漏出为已接受结果。

完整 family bootstrap 对同一 RootSet 的六个顶层根使用连续有限扫描/值/key/cell 预算，检查两个族的必需 metadata/格式、原 Text 外键与每个完整 child，返回仅覆盖这两个族的计数和根绑定。创建要求该精确父家族资格，并在当前同事务视图检查外键不存在，完整借用生产后才发布一个外层引用；删除要求当前精确 child 资格，不重扫被删除记录的大 sibling。结果仍需完整 transition 验证和合法提交，family token 不授予业务权限、全 World 激活或 GC。

授权名单结构操作保留原 Vec 语义：追加借用调用者尾段，截断只删除 `[new_len, old_len)` 的认证 slot，均核对当前资格与精确原数量，并将数量及所有受影响 slot 合成一个最终外层引用；不重写未变前缀，不读取被删成员的长 payload。数量读取复用公开 overlay getter 的 resolver，结构 NoOp 在相同 getter 边界后仍有独立组合取消检查。名单为空、空成员和重复成员均属于可保存原内容，不能在存储层当作集合规范化。

接收端在通用 transition 重放之外，继承精确父 family 资格，只验证真实变化的 record 路径。必需字段/metadata/marker 不得增删；Option 的 flag 与可选值根存在性必须一致。名单变化按原、新数量、真实增删 ordinal 的范围及净计数证明尾部密集性，不以总叶数相同代替索引关系，也不扫描未变 Vec。新记录承担完整 child 资格；删除或未变旧 payload 可继承已资格父证明，只认证需要的原 semantic key/header，不能借此跳过新值的完整闭包与类型验证。原相对键解析使用固定128字节工作区，拒绝未知列、错误 tuple/token 和尾部字节。

本族 typed mark/transfer 沿全部六个顶层状态根及这两个族的完整 child/value 闭包收集原页 hash，复用同池、同 class 的逐页准入与有限页数跟踪；其他 owner 仍须贡献自己的闭包。输出在完整遍历及全部 buffered sink finish 成功前保持 provisional，最终物理发布后观察到取消也不能返回完成交付。内存不足须按实际阶段报告，不能把读取/跟踪额度拒绝改称编码失败；冷恢复另以新 pool、原认证根和原 hash 重新验证，不能由该族资格代签完整 World、全局 GC 或物理故障域保障。

<a id="iws-governance-records"></a>

### 12.39 治理身份、提案与投票原始记录

本族仅覆盖 WorldState/Map/Authorization 的 governance_identity_profiles、governance_proposals、governance_votes。沿各原 String 外键保存，不能以内嵌 agent_id/proposal_key 替换、回填或规范化；snapshot 与 ballot 的原 String 成员键也独立于值内身份。存储不新增 staking、warmup、身份处罚、quorum、提案结算、投票资格或权重业务规则。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

每族保留 Metadata 和 (governance_record_closure_v1, unit) Entry/Unit 格式标记。每条外键的 tag10 RootRef165 绑定原 Text semantic digest、原 namespace 和 BLAKE3(ASCII oasis7.iws.governance-record.v1\0 连接原字段名 UTF8) schema。child 保留 Metadata、unit record marker；固定列为 ((unit, record), 原字段名)。Identity 六字段、Proposal 十七字段、Vote 五字段全部穷尽借用解构。原 IdentityStatus Active/Frozen/Expelled 和 ProposalStatus Open/Passed/Rejected 分别编码 0/1/2，拒绝其余值；pass_threshold_bps/u16、slash_count 与 ballot weight/snapshot vote_weight_cap 的 u32 范围保持，原 U64/I64 包括全部极值不加业务限制。finalized_at 与 winning_option 固定必需列用 Unit 表示 None，原 U64/Text 表示 Some，包括 0/空串。

options 保存独立 count marker 和原 u64 index Text 成员，保留原顺序、重复和空值。vote_weight_snapshot、votes_by_agent、tallies 各有独立 count marker，成员按原 Text 键保存；前两者分别逐列保存 Snapshot 五字段、Ballot 三字段，后者保存原 U64。集合 count 键为 (((unit, record), 原集合名), unit)，Vec/标量 Map 成员尾为原 index/Text，结构 Map 成员再接原字段名。空集合不能省略 marker，不用整 DTO、整集合 JSON/Serde blob 或旧通用投影代替 typed 闭包。

GEN 对明确的全部记录/成员范围使用连续有限 cell/record/key/state/value/pending 额度并先准入再分配；输出在完整成功前 provisional。原始根/恢复资格需认证外键 preimage/hash、每条 RootRef、完整 required/Option、Map 各列成员集合一致、Vec count/dense index、所有状态/值页和拒绝未知/缺页/旧格式。普通单列或单成员更新必须有同池 Ordinary、guard/live、精确 expected、同事务当前视图和组合取消；不得扫描未变 description/options/snapshot/ballots。后继接收端先 generic replay，再从 qualified parent 与认证 changed paths 推导 schema、必需列和净 count/成员关系；GEN 或局部读不代签完整资格。

取消/no-op/相等比较后、最后真实 put 后和 buffered sink finish 失败均不可接受成功前缀。typed page mark/transfer 必须显式追踪本族 child StateRoot 与 ValueTree，最终 finish 成功后才可完成；原 pool/store 销毁后的有限小池 root-only 恢复须独立验证。本节定义目标合同，GEN checkpoint 不代签未实现的 qualification、普通事务、receiver、页传输、完整 World 激活/GC 或整体 H/S 性能验收。

### 12.40 主代币配置与发行治理记录的封闭存储

<a id="iws-main-token-policy-records-v1"></a>

本组覆盖原 main_token_config（Value/Business）、main_token_genesis_buckets（Map/Business）、main_token_epoch_issuance_records 与 restricted_starter_claim_grants（Map/Authorization）、main_token_scheduled_policy_updates（Map/Scheduler）。配置以原 Unit 外键、bucket/grant 以原 String、issuance/scheduled 以原 U64 外键分别绑定独立 RootRef165；外键与 bucket_id、beneficiary_account_id、epoch_index、proposal_id/effective_epoch 各自原值独立保存，禁止裁剪、归一化、从内部 ID 重造外键。Trace refs: O7-REQ-c33f4bee-41db-4558-9c49-80c4e0c016a1、O7-REQ-e835ca47-69bb-4bef-8791-83d2ae78f709、O7-REQ-2d77214a-6c57-4794-bfeb-bb460187085c。

每族必需 Metadata 与 (main_token_policy_record_closure_v1, unit) Entry/Unit 格式标记；每个 child 有同 namespace 的 Metadata、Unit record marker 与穷尽原名列。Config 的 symbol/decimals/initial_supply/max_supply 以及 inflation_policy 六列、issuance_split 四列、burn_policy 三列按原嵌套相对 tuple 路径保存；scheduled 的 proposal_id/effective_epoch 为独立列，next_config 下复用完全相同的借用配置 visitor 和列定义。Config 全局对象始终存在且不得删除；其他四族允许从完整 qualified 父创建或显式删除闭合独立 child，不暴露不受资格约束的 empty-create。

bucket 八字段、issuance 七字段、grant 十一字段均为原类型的独立列，不聚合 Serde/JSON。decimals 严格保留 u8 全域，其余原 u32 列仅校验表示范围，不施加 bps/经济业务上限。grant status 穷尽 issued/expired/revoked 标签；max_supply、status_updated_at_epoch、status_reason 必需列以 Unit 表示 None、原 U64/Text 表示 Some，保留 Some(0)/Some(empty)，不得省略为缺列。所有其他原 U64/Text 保留零、极值、空值和 Unicode；不得 clone 整 DTO，也不改 mint、发行、治理或授权 reducer。

借用 GEN 完整投影原状态，root-only family 恢复仅凭六根、原 schema/semantic binding 与有限 state/value/key/cell/heap/pending 预算验证全部已保存关系，不以原 World snapshot 为必需 oracle。未知/缺列、孤儿、错误 codec/范围/标签、原键错配、child leaf_count/path/页缺失均拒绝。普通单列点读写要求精确 expected、同事务 guard/live、同池 Ordinary、组合取消与失败 poison；Option 作为一个完整目标列更新，不扫描无关长 symbol、reason、recipient。NoOp 最终取消须使用独立 getter 基线检验额外返回边界。

后继接收端先独立 generic replay，再根据 actual outer writes 与 changed Merkle 子树验证本 owner 闭包，继承 qualified 父的未变关系，不重复扫描未变 Text。页 mark/transfer 复用完整本组资格并显式追踪 child state/value，集合分配前计费，含 buffered finish 的全部输出成功前均为暂态。本组资格只证明存储完整性，不授代币发行、grant 花费或政策生效权限，不代签完整 World 激活/GC。独立字面字段/键 oracle、Raw/Zstd 冷根恢复及合法后继、generic-valid/typed-invalid、真实发布故障与取消、预算原子性和固定增量成本分别验证，未完成者不得由 GEN/check 或局部 visitor 冒称已闭合。
