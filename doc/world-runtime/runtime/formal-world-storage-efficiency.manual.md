# 正式世界无损存储：验收操作手册

- 状态：active；本文是目标验收步骤，不表示相应实现、测试入口或生产能力已交付。
- Owner：runtime_engineer；共同执行/审读：qa_engineer、blockchain_ops_engineer、wasm_platform_engineer。
- 设计权威：[正式无损存储设计](formal-world-storage-efficiency.design.md#des-lws-001)；产品结果见[完整世界历史与无损存储](../../product/world-infrastructure/lossless-world-history-storage.prd.md#lws-acceptance)。
- 文档输入：`nil-studio/oasis7@4220622fde069346a828d42572eea61c0073d95e` 上本 PR 候选。本次仅定义手册，未运行以下场景、迁移实际数据或测量容量。

<a id="lws-validation-use"></a>
## 1. 使用前提与结果范围

以下 V-LWS-* 是稳定场景 ID，不是假定已经存在的 CLI、测试函数或自动化 runner。执行者先确认实现 PR 提供相应可调用入口，将**实际命令或手动调用、完整参数、退出码和原始输出**记入证据；没有入口、能力、合法样本或环境时标 `not_run`/`blocked` 并说明原因，不用模拟成功或改造手册步骤代替实现。

只在固定的验证副本/独立验证部署中执行数据损坏、进程中断和删除负例，保留原始样本及未优化对照。世界 identity、输入、原结果和历史不能为使候选通过而重写。测试 owner 在开始前冻结适用规模、预算、阈值与消费者组合，失败后不放宽阈值重标成功。

| 必须固定的输入 | 记录内容 |
| --- | --- |
| 代码与能力 | source/integration/tested tree、二进制 digest、旧/新逻辑 schema、codec/reader 能力、实际测试入口 |
| 世界及范围 | world/chain/创世身份、合法 authority 与起止历史边界、原 qualified/未决集合、原版本工件与 hash |
| 负载 | 种子、原完整输入/事件/外部结果、业务/模块/控制状态、动作数量、同一新输入对照 |
| 部署 | P/R 故障域与存储依赖、文件系统/设备、配额、冷热策略、缓存状态、并发和网络条件 |
| 保留关系 | 全部 typed 根、对象和引用边；reader/rollback/attempt/迁移 pins；缺失、未知和未解析项 |
| 测量与效果 | 采样窗口、实际分配量和计数范围、应用/设备写入、内存、延迟、原外部请求及调用/收费账本 |

本手册 `passed` 只表示该固定场景的预期满足；`failed` 表示结果违反合同；`blocked` 表示条件尚不能判定；`not_run` 表示未执行。它们是验证记录用语，不替代原运行时的 committed/unknown、服务资格或业务结果。required 局部、full 真实集成、独立故障域及平台耐久性分别记录，不相互代签。

按本次实际启用子集选择场景：S0–S3 物理优化可独立验收/部署，S4 完整 delta 和 S5 Merkle v2 可分别后置。物理阶段仍须证明原材料恢复后的新工作连续；非空 delta 与 v2 未启用时独立标未验证，不用完整 T snapshot 或 v1 成功代签，也不将它们捆成物理优化前置。

<a id="v-lws-history"></a>
## 2. V-LWS-HISTORY：全历史、冷热与检查点覆盖

**输入**：固定正式世界，跨多个热窗口、checkpoint、authority/epoch 和模块版本；包括旧 nonce/receipt、GameGoalPublish/GameExecutorControl 前驱、原外部结果，以及一个早期 H 仅依赖旧 C 的区间。

1. 从合法初始锚到当前边界建立完整根/对象/依赖清单，标出每个 H 的原 snapshot 或 C<=H 加完整后缀路径；记录原 bytes/hash、版本及所有未解析项。
2. 在候选上共享/归档并变更热窗、checkpoint 密度，逐对象取回原内容与依赖，按原身份查询早期、中间、当前结果；对全部清单核对长度/hash/原字节及引用闭合。
3. 跨冷热读取目标/控制前驱并提交相同合法新工作，对照无优化分支；临时断开或损坏冷源，检查它返回受限/未知范围而非不存在，并保留源 pin。
4. 保持较晚 C'>H 存在，尝试删除唯一早期 C；必须拒绝。建立并验证所有受影响 H 的其他完整路径后，才允许释放多余展开表示。
5. 分别移除唯一 payload、原 native/WASM 工件、原 proof 或一条 typed 引用依赖；必须指出缺口、停止相应 GC/恢复服务，不得以 metadata/hash/较新工件补齐。

**通过条件**：所有迁移对象与适用 H 的覆盖清单完整，原历史身份/内容不变；冷热、数量与 checkpoint 变化不缩短承诺，正向新工作能够继续。完整对象核对与代表性业务恢复分别有证据。

**证据**：逐对象/高度覆盖及差分、原冷源读回、前驱/新工作结果、拒绝删除和缺证输出、源 pins。少数目标恢复成功不代表全部历史或任意更大规模已验收。

<a id="v-lws-codec"></a>
## 3. V-LWS-CODEC：原字节、共享与所有读者

### 离线 v1 pack shadow 入口（S2 子范围）

停机一个受控验证副本的 endpoint，在已有父目录下选择全新、位于源 endpoint 外的目标目录：

```bash
env -u RUSTC_WRAPPER cargo run --locked -p oasis7_distfs --example formal_storage_shadow -- \
  /absolute/source-endpoint /absolute/new-shadow-directory
```

入口取得 endpoint 同名独占 lease；运行中的 endpoint 会使其在创建目标前失败。它读取 `formal-objects-v1` 全部 digest 名对象，解析原 raw/v1 pack，再把不超过既有 pack node 上限的小对象按 8 MiB payload 批次写入现有 v1 pack；空对象及更大对象保持 raw。65,536 对象、1 GiB 总逻辑输入为本工具的固定上限；单对象沿用原 reader 的 64 MiB 逻辑/80 MiB 物理限制，读回同时持有两个逻辑对象，内存最多约 128 MiB 加有界名称清单及常数解码元数据。超限返回错误并保留源，不放宽预算继续。

必须将全部原对象逐个读回并比较精确 bytes；每次逻辑读取继续校验原 hash。源中未知文件、symlink、非空 staging 或不完整物理盘点都会拒绝，不能跳过 pending 材料来扩大容量收益。输出 JSON 分列原逻辑 bytes、对象数量和源/目标物理盘点；以 allocated_bytes 比较该**对象命名空间**的候选占用。目标 hardlink 的按路径 apparent_bytes 会重复计整个 pack，不代表逻辑内容增长；源/目标保留同一逻辑对象，不能从总磁盘增量推算已释放量。

源文件和 checkpoint 始终保留，目标既有内容绝不覆盖；任何失败都保留源及可能不完整的目标。写入任何对象前先创建并同步 `.shadow-only` 标志；初始化阶段失败可能仅留下空目录。目标只包含对象及打包元数据，不复制 checkpoint、authority 或 generation selector，**不能作为 endpoint 启动或视为可激活的完整恢复 generation**；endpoint open 在创建 checkpoint 前拒绝任何带此标志的目录。成功仅证明该命名空间的全对象精确读回；不证明 typed 全世界闭包、独立故障域或运行时恢复续跑。此一次性入口不激活、不回收源；失败重试使用另一个新目标。

### 压缩 shadow 与分批续传

新 codec 的离线对照使用 `--optimized`。大历史使用 `--resume`，在源保持停止且不变时重复**同一命令和目标**：

```bash
env -u RUSTC_WRAPPER cargo run --locked -p oasis7_distfs --example formal_storage_shadow -- \
  /absolute/source-endpoint /absolute/new-shadow-directory --optimized --resume
```

每次调用仅推进有界 copy 或 verify 步骤；退出码 0 且 `complete:false` 表示本页成功，**不表示整个 shadow 已完成**。保存 JSON 的 phase、processed_records、total_records，直到 `complete:true` 并取得最终 report；独立 verify 结束前不报全对象读回成功。原始 v1 pack 对照可用同一命令去掉 `--optimized`，两种表示使用不同目标。源 checkpoint、registry、generation、目录身份或 codec 参数变化会拒绝续原操作，不能删源或改游标绕过；私有 catalog 及 `.shadow-only` 在中断期间保留。

### 正式物理代与 attempt 维护

受控端点持有原实例/provider 后，维护调用 `FileFormalEndpoint::advance_physical_generation()`，每次只推进一批，直到 `building:false`。记录每页 selected、copied_objects、total_objects 和前后物理盘点；中断后使用同一正式端点及实例重开续原事务。读 pin、未完成独立闭包核对或未知 selector 会阻止相应退休，不能替换稳定 guard inode、强删 BUILD 或手工切 CURRENT。仅复制 shadow 到 serving 路径不构成这种事务。

`collect_physical_orphans()` 是另一个带 typed-mark 预算的受控入口；超预算只保留，不声称已经清空 orphan。正式 generation 保留全部已登记逻辑对象，分页移除旧代及不再被 overlay 引用的帧；不把历史阶段状态视作垃圾。已知 publication staging 需在完整预检及同一 retire fence 内清理，未知项停止整个相应 sweep。

attempt 回收由真实 Published envelope、P/R 分别读取的完整原闭包及精确替代清单共同决定。Node 空闲轮询和业务完成只推进有界维护页，pending/unknown 不改变业务已提交结果，也不能删除原输出。端点重启后丢弃失去内核 pin 的读回进度，重新验证；完成删除后才释放 P/R 维护 pin。容量报告须列出仍未释放的 attempt 和独有材料。

### 新世界的原 native 环境

新世界的生产 `world init prepare` 入口要求初始化配置 `schema_version:2`：准备批准时绑定原 descriptor，激活时再次匹配且向两个端点各自 stage/verify，之后的新 Frozen 延续同一环境 root。配置 schema 1 仅用于解读或继续此前已准备的原请求，不能用来新建缺 native 闭包的世界；旧 quote/activate 不改原请求身份。

当前捕获入口支持具备完整可读 loaded-file 闭包的 Linux；macOS dyld shared-cache 缺完整原文件时返回 Unavailable。不要替换成仅 executable digest、测试描述符或另一环境来取得批准。构建 target/features/toolchain/Cargo.lock 原 bytes、原 executable 与依赖 chunk 均纳入归档和 full sync；当前二进制或依赖不等于原环境时拒绝重演，按原环境工件恢复。原请求 stage/verify 中断可幂等重试，尚未完成不授予 Frozen 资格。


**输入**：包含 raw、旧 business pack、阶段外壳、candidate/frozen、大小对象、CBOR 整数数组 artifact、snapshot/module 的固定 corpus；另有不同授权范围的两个读者。

1. 保存原始 canonical bytes、对象 hash、typed digest、签名/证明结果和 range 基准。固定目标表示版本及 frame/chunker 参数，保留原 corpus。
2. 对所有实际支持转换分别构建共享/压缩/recipe 表示，逐对象重建原 bytes 并执行原校验；包括无收益对象继续 raw、跨阶段共享与原 CBOR 嵌入，不重写 serializer/golden bytes。
3. 通过消费者清单中的 Store/get/raw、Merkle、range/chunk、archive、native restore、durable-set copy、offline/published、checkpoint/repair/cache 路径重读；任一直接文件读者未覆盖即不允许启用新写。
4. 取空范围、首尾字节、跨 frame/pack 边界及非法范围；比较解码后 offset/length/total_length/EOF。部分分片只按原认证语义判定，不能以 frame checksum 冒充全对象认证。
5. 注入 raw 与 magic 前缀冲突、截断、错 hash、错长度、缺 dictionary、recipe 环/过深和不支持版本；检查明确拒绝且不猜测回退。使用未授权读者尝试已知 hash，确认共享不扩大访问权。

**通过条件**：覆盖组合精确保持原字节、逻辑身份、签名与范围语义；损坏/不支持/越权分别拒绝；格式选择不依赖歧义前缀。

**证据**：corpus 清单和字节差分、消费者能力矩阵、范围及错误结果、权限负例。兼容通过不证明节省用量；容量由 V-LWS-CAPACITY 独立判断。

<a id="v-lws-gc"></a>
## 4. V-LWS-GC：完整根、attempt 与回收收敛

**输入**：同一候选分别处于 Frozen/Prepared/Decided、dual qualified 但未 published、published 待 ack/outbox、回执 unknown 或失败 attempt；另含早期历史根、混合保留/可删成员的 pack、活动 reader/transfer、rollback 和迁移 pins。

1. 执行只生成计划的 typed mark，记录每个对象的保留根及所有未解析项；核对完整历史而非仅 latest。引入未知对象类型、缺依赖或扫描预算耗尽，必须停止相应 sweep。
2. 在 dual qualified 后但 published envelope 持久/读回前中断并触发维护，必须保留唯一材料；另测远端 unknown、交付未知和 reader 持有旧代。改变文件年龄/保留数量不得改变可删判定。
3. 用可控制调度暂停在 **最终 roots/pins check 之后、unlink 之前**，竞争发布新引用和 AcquireReadPin。分别让新引用/pin 先成功、retire fence 先成功：前者必须使 GC 放弃/重核并保护对象，后者必须使引用/pin 重试到合格新表示。不能出现成功 pin 后对象被删除；另在 fence 持久/持有者解除间重启，核对旧持有与原 operation，不凭 PID 复用或断连释放。
4. published 保存后屏蔽目标 reader 对 attempt 路径的访问，从持久目的层逐对象遍历精确 envelope 及完整 typed 依赖；故意删除一项目标依赖，必须失败，不能借 attempt 回退通过。读回完成前不得解除相应 attempt pin。
5. 对 published 已确认但 pending ack/outbox 未完成或 unknown 的样本，将原义务、原件及恢复路径保为独立根。待删清单承载唯一义务时必须保留；原件已独立闭合且清单只含重复输出时，应能回收真重复而继续原义务，不强迫全部业务 ack 结束。失败 attempt 另证不承载恢复唯一材料。
6. 对混合 pack 验证保留成员先迁入合格新 pack、locator 原子切换，最后才解除旧链接/持有；解除全部适用 pins 并复验后重试维护，检查逻辑路径删除与实际 inode 释放分别计量。

**通过条件**：保护条件存在时零误删；条件稳定且资源足够时可证明冗余最终收敛。清理中断只增加残余占用，不撤销提交、发布旧世界或丢原历史。

**证据**：mark/root/pin/fence 差分和调度顺序、qualified/published/ack/outbox 独立结果、无 attempt 依赖的目的读回及缺件拒绝、删除清单/实际释放与重启恢复。单进程 fixture 不证明全局 GC 或独立故障域耐久性。

<a id="v-lws-durability"></a>
## 5. V-LWS-DURABILITY：独立副本与权威隔离

**输入**：适用正式 profile 下实际独立的 P/R 故障域，持有优化后的相同历史及未决候选；记录设备、存储服务和恢复依赖的共同故障点。

1. 分域从持久层取回全部必要逻辑内容及证明，核对原 bytes/hash 和原协议资格；两个目录、同 inode 或唯一共享 bucket 不充当域独立证据。
2. 完整隔离一个故障域，从另一合格域恢复当前及选定深历史，核对全部已确认效果和原未决身份；所需正式持久条件不足时停止新确认。
3. 在合法成员/epoch 切换后发送旧 writer 请求，并由只具存储读取能力的组件尝试推进；验证 fencing/权限拒绝，历史读取不重新授予旧写权。
4. 对另一故障域独立重复步骤，检查压缩字典、recipe、历史版本工件没有隐含依赖失去的唯一域。

**通过条件**：支持的单域故障内已确认结果完整，原 unknown 按原身份核对；非法写者不能推进历史，受限恢复不会伪报 serving。

**证据**：域拓扑和实际隔离动作、逐域闭包、原回执/效果对照、权限拒绝及服务判定。进程退出不证明掉盘、掉电或独立物理域。

<a id="v-lws-migrate"></a>
## 6. V-LWS-MIGRATE：混合读取、切换与回退

**输入**：原世界旧 raw/pack、兼容新旧格式的候选、明确不支持新格式的旧 binary；迁移期间继续原业务，并保留活动范围读和未决候选。

1. 盘点并实际调用全部读者；先旧写、混合读，验证未知/旧直读消费者能阻止新表示启用。记录源 generation、根清单和 reader 能力。
2. shadow 构建新表示并逐对象重建原 bytes/hash；旧 selector 不变，源/目标均受保护。完成文件/目录同步及新业务 overlay 差集核对后才切换。
3. 按 V-LWS-FAULT 的切换窗口逐点中断并恢复原维护操作，核对 NotCommitted/Committed/Unknown 和 pins；期间原业务未知仍查询同一 Frozen/candidate。
4. 激活后再提交新业务，检查当前及深历史读取、双域恢复和续跑；旧读者仍固定旧代，解除 pins 后再验证清理。
5. 尝试启动不兼容旧 binary，必须拒绝。将**切换后全部已接受历史**转成目标读者可理解的表示并验证后，才可做物理回退；不得只切回旧 selector 丢弃新历史。

**通过条件**：所有切换结果可核对，同世界完整历史、原请求和效果保持；new logical delta/Merkle 已产生时不能假装物理解压能使旧逻辑 binary 兼容。

**证据**：能力矩阵、源/目标/overlay 清单、selector 与同步结果、切换后业务、回退拒绝/合法转换和残余占用。仅复制成功或新目录能读不算完整迁移验收。

<a id="v-lws-continue"></a>
## 7. V-LWS-CONTINUE：非空后缀、控制状态及新工作

**输入**：同一世界的不中断对照 A 与恢复分支 B；物理阶段沿原完整材料恢复入口，启用 S4 时另选较早完整 C 加真实非空后缀到 T。包含 J1 分配/预分配/era/队列、两 module instance 的 Pure/wake/clear、原 native/WASM 版本、已确认及未决外部效果。

1. 固定原恢复材料与执行路径；对 S4 子场景另外固定 C 的完整基线、C<T、原后缀及逐字段覆盖，记录 producer/schema/顺序/依赖，禁止用完整 T snapshot 代替非空后缀。缺控制记录的 legacy 区间保留快照或明确 adapter，不能默认补零。
2. 在 B 上完成适用优化/GC 后恢复 T，分别比较业务、原 bytes/root、J1 cursor/era/provenance/队列与 J2 最终绝对 wake tick/clear，不能只比较展示投影或 ID 最大值。
3. 向 A/B 提供相同合法新输入，至少完成一条新配方/结算一次；对照新请求身份、资源/费用、队列消费及模块只在预期 tick 行动。所有请求都拒绝不构成正向续跑。
4. 分别覆盖已有持久候选、仅原 Frozen、外部请求已发但回执未知：先复用原候选；Frozen-only 必须具备原执行依赖及已物化原结果；未知效果按原幂等身份核对。
5. 核对恢复/历史查询期间模型调用、已确认效果重派和重复收费均为零；新合法效果仍可执行。外部系统无法核定原请求时保持 blocked/unknown，不另发新身份，也不声称物理恰好一次。

**通过条件**：物理阶段证明原材料恢复后的正向新工作与控制状态连续；S4 还需 J1、J2 各自的非空后缀证据。缺材料负例拒绝，未启用子场景明确未验证；snapshot round-trip 或 root 匹配不能单独通过正向续跑或代签非空后缀。

**证据**：A/B 固定输入、C/T 与字段差分、cursor/era/wake 轨迹、新业务及效果/收费账本、三类恢复入口结果。按现有 J1/J2 合同分别记录，不合并为单一 root verdict。

<a id="v-lws-budget"></a>
## 8. V-LWS-BUDGET：资源耗尽与原操作连续

**输入**：实现前已冻结的各项预算及消费者矩阵，含在途提交、冷读取、完整同步、迁移和归档；实际支持的最大规模与超界刺激分开。

1. 对原逻辑对象/envelope、pack/stored bytes、解码输出/窗口、recipe 深度/依赖数、闭包内存/I/O/并发、时间/重试/缓存分别构造低于、达到、超过边界的输入。
2. 在 writer、endpoint、Merkle、range、offline/published、archive、repair/restore/full sync 逐项执行；压缩后很小的巨大逻辑输出也必须受原解码预算限制。
3. 注入磁盘余量不足、冷源慢/失联、取消/超时和回应丢失，记录原操作身份、已验证进度、pins 与源内容；检查读限制不伪装 not-found 或静默部分成功。
4. 恢复资源并续做同一操作，核对根/对象/版本/authority 仍绑定原输入，已确认结果不重做；目的地闭包完成前源始终保留。
5. 在声称支持的最大完整历史规模下观察全过程 RSS、临时磁盘和恢复进度，确认没有在某一 Vec/ClosedRecord/RPC 中重新全量物化。入口分页本身不能证明端到端流式。

**通过条件**：各预算有准确可区分结果与下一步，原世界/历史/效果不因压力改变；缺资源时限制能力而不删历史，恢复资源后可按原身份继续。

**证据**：冻结预算、各边界/消费者结果、峰值/进度、原请求/保留根及续传完成。只返回错误码、只跑小 fixture 或只检查写端不证明本场景。

<a id="v-lws-capacity"></a>
## 9. V-LWS-CAPACITY：相同完整历史的实际用量

### 本地物理基线入口（S0 子范围）

在冻结写入的本地验证副本上运行：

```bash
env -u RUSTC_WRAPPER cargo run --locked -p oasis7_distfs --example storage_inventory -- \
  primary=/absolute/primary-storage-root replica=/absolute/replica-storage-root
```

每个 `DOMAIN=PATH` 必须覆盖该端点全部纳入本次比较的本地数据（含 attempt、archive、staging 和旧表示）；根目录以外的存储需单独纳入清单并补充计量。相同域内使用一个共同根，不能把重叠子目录当独立域相加。独立域标签由操作者声明，工具不能证明设备或服务独立。

JSON 分域报告 file_paths、unique_files、directories、apparent_bytes、unique_apparent_bytes 和 allocated_bytes。Unix 使用 device/inode 去重，每个 inode 的 `st_blocks * 512` 计入 allocated（含目录），域间不去重。apparent 是所有文件名的长度和；unique_apparent 仍是文件长度，不代表分配量。目录项/权限/元数据错误、缺失根、symlink 和特殊文件产生 errors、`complete=false` 及退出码 1；参数错误返回 2。非 Unix 缺 inode/分配计量能力也返回 incomplete，不能用零代替。符号链接不跟随，需提供受控实际目录并显式记录覆盖范围。

遍历是只读的非原子采样，需要静止树；inode 集合与待遍历路径/错误列表随扫描规模占用内存，超大目录应预留资源，本入口不声称端到端有界历史遍历能力；不调用 GC，不判断根/pin 存活，也不对完整历史作认证。硬链接之外的共享 extent、透明压缩、远端计费和打开但已 unlink 的 inode 不由本工具精确归因，须按 §9 步骤补充原生统计。工具通过不等于完整 V-LWS-CAPACITY 通过；真实负载、写入/峰值/延迟、历史完整性与独立故障域仍须联合验证。


**输入**：等同正式历史、负载、原结果、存储角色及独立保障的基线/优化候选；覆盖空 Tick/稳定模块、小修改大 snapshot、活跃工业/结算历史和跨冷热窗口，固定采样点。

1. 在开始、稳定提交边界、归档后、迁移峰值、重启/失败恢复后及 pins 解除后采样；记录所有范围内目录、P/R、冷归档、attempt、staging、旧代与未完成回收。
2. 报告原逻辑内容/逻辑唯一 bytes、apparent bytes、实际 allocated、文件/inode 数；按 device/inode 在单故障域内去重 hardlink，再分域相加。不能跨 P/R 扣掉必要副本。
3. 文件系统有 reflink、稀疏、透明压缩或远端计费时，补充适用原生分配统计并说明精度；缺目录/域/归档计数标 incomplete，不补零。最后链接或打开持有未解除时记录残余分配。
4. 独立记录累计应用写入及可取得的设备 I/O、稳定 allocated 增长/提交或业务操作、峰值 RSS/临时磁盘、解码 CPU、冷/热查询及恢复 p50/p95/p99。清理尚未释放的容量不能提前计为收益。
5. 同时核对 V-LWS-HISTORY/CONTINUE 的完备性结果，按冻结阈值比较成本；任何历史/副本减少、计量缺失或完备性失败都不得宣称整体节省成功。

**通过条件**：计数可重算、归因清晰，物理增长、写放大、内存及恢复成本分开；结果仅适用实际测量窗口和数据形态。无收益对象可保留 raw，但不将兼容成功算作已节省容量。

**证据**：逐域原始计数/去重口径、负载/配置、采样时间、前后分布、残余 pins 和配套完整性结果。未测量时不报节省比例，旧 2500 heights 本地预算不外推正式总盘。

<a id="v-lws-merkle"></a>
## 10. V-LWS-MERKLE：v1 历史与 v2 新构造分验

**输入**：v1 原始 delta/canonical bytes/原 verifier，以及独立版本 gate 后才可能存在的 v2 producer/consumer；覆盖同批覆盖节点、多个 key、共享父节点、大/小叶子和跨 pack。

1. 先对 v1 进行仅物理优化，逐字节比较原 delta/node_writes；使用原 verifier 的 rebuild 全结构相等判断，不更新 golden bytes 掩盖差异。
2. 构造删除暂时中间节点的 v1 delta；即使最终 root 相同也应由原 verifier 拒绝，历史材料不得被该“优化”改写。
3. 仅在 v2 实现存在且 gate 明确后，固定更新顺序和父根，多次构造并比较确定编码；独立重建最终新可达最小集合，检查父节点复用、无多余/冲突/缺失 writes 及正确证明。
4. 验证 v1/v2 混合历史、旧 reader 的兼容或明确拒绝、原 authority 升级边界；改变树结构/root 算法时另列其独立验证，不由物理测试代签。
5. 记录节点、pack 索引、实际 allocated、写入及读成本，同一更新集比较；理论每路径字节分析不替代实测。

**通过条件**：v1 历史不变；v2 已实现组合确定且合法，负例拒绝。v2 未实现时将其子结果记 not_run，不能宣称整个索引演进通过。

**证据**：原 fixture、更新集、各版本 bytes/root/证明与负例结果、混合历史及成本；v1 与 v2 独立 verdict。

<a id="v-lws-fault"></a>
## 11. V-LWS-FAULT：维护崩溃、耐久与未知窗口

**输入**：单 writer 验证副本、可核定原提交的 witness/端点、源/目标 generations、活动 reader 和持续新业务；明确区分 failpoint、进程终止、文件系统错误与真实掉电。

| 中断点 | 核对重点与预期 |
| --- | --- |
| staging 写入/原字节校验中 | 旧选择器有效，原闭包与 pins 不变；未发布目标可重建，原业务不撤销。 |
| 新文件/目录同步后、selector 前 | old/new 与维护记录受保护，核对原选择器；没有激活证明不删源。 |
| overlay 差集/最终索引/selector 同步或读回失败 | 未完成耐久证明保持 Unknown；缓存读到新值不等于持久；保留两侧和期间新写。 |
| dual qualified 后、published 持久/读回前；published 后 ack/outbox 未结束 | 分别核对原义务；目的 reader 无 attempt 依赖完整读回前不解 pin，未结束义务唯一材料不删，已独立保留的真重复允许回收。 |
| 最终根/pin 重核后、unlink 前的新引用或 pin 竞争；retire fence 后重启 | 按 V-LWS-GC 的两种顺序验证共同 guard/epoch：GC 或新获取的一方延期/重试，不出现成功引用后丢对象；未决 fence/旧持有继续受保护。 |
| 激活后新业务写入、进程崩溃 | 恢复已提交 generation 与原历史，核对当前 head；不能加载旧代后发布为当前。 |
| 旧代清理/pack 链接解除中 | 已确认结果保持，允许多余文件；旧 reader/恢复 pins 不丢，重试收敛。 |
| 源/目标坏块、回执丢失、一个故障域消失 | 明确缺证或原 unknown，验证合格替代闭包；不重做已确认效果、不从损坏材料猜成功。 |

对每个适用窗口分别执行：保存原操作/根与计量，注入一次中断，重启后只读核对 selector/原协议结果及全部保护闭包，再沿原操作恢复，最后检查新业务/外部效果和实际残余占用。同步失败分类不得仅由函数返回 Err 或“文件看得到”推导。

**通过条件**：无静默历史丢失、无旧世界越权服务、无已确认外部效果重派；Unknown 可保持受阻但必须保住原恢复材料。只有原结果和所需合法边界核定后才能解除限制。

**证据**：每窗口刺激、实际同步/读回结果、selector/根/原回执、pins、重启与恢复轨迹。进程终止/failpoint 只证明其窗口，不冒充目标文件系统掉电耐久实验。

## 12. 接受记录与未覆盖项

实现 PR 按场景逐项附实际 source/tested tree、命令/手动步骤、输入、环境、结果、证据位置和未执行原因；关联设计中适用 REQ，分别保留物理/逻辑阶段、J1/J2、P/R 和平台范围。负例的正确拒绝不能代替正向恢复或新工作成功。

完整对象覆盖、历史/控制语义恢复、实际容量收益、读写性能、独立耐久和消费者权限分别判定。任一所需条件未验证时保留相应 gate，不将手册存在、文档检查通过或局部通过写成实现/发布完成。后续规模、对象类型、reader、codec、ABI、故障域或文件系统改变时重跑受影响场景。
