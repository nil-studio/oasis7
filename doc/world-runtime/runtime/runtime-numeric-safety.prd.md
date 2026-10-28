# Runtime 数值安全与原子状态转移契约

- 对应设计文档：`doc/world-runtime/runtime/runtime-numeric-safety.design.md`
- 专业需求入口：[PRD-WORLD_RUNTIME-041](../prd.md#prd-world-runtime-041)；本文细化该需求的数值策略与验收边界。
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history；本文与 design 保留稳定数值契约及实现锚点。
- 专业权威：`runtime_engineer`
- 吸收范围：已完成的 numeric-correctness phase 1–15 与 infinite-sequence rollover 专题；另以目标合同补齐 action allocator 的 journal-suffix 恢复，不据此宣告实现完成。

## 1. 目标

Runtime、consensus 与 node 的权威状态不得因整数越界、隐式窄化或部分写入产生不可重放、不可恢复或节点间不一致的结果。确定性执行承诺由 `doc/product/world-infrastructure/deterministic-world-execution.prd.md` 承载，分布式连续性与恢复承诺由 `doc/product/world-infrastructure/distributed-consensus-and-state-availability.prd.md` 承载；本文只定义专业数值语义。

## 2. 范围

### 永久契约

1. 权威计数、余额、票权、高度、slot、epoch、sequence、term 与过期时间的推进必须采用以下一种显式策略：
   - 受检算术：越界返回领域错误；
   - 有协议定义的 rollover：代际与序列一起持久化并保持兼容；
   - 有明确调用约定的 clamp：边界值可观测、可测试，且不得被描述成“成功计算了精确值”。
2. 禁止依赖 release 模式回绕或无说明的饱和算术掩盖错误。
3. 可能失败的状态转移必须先读取、校验并预计算所有后继值，再一次性提交。失败不得留下部分余额、proposal、head、lease、schedule 或 durable record。
4. replay 可见的 action 成本溢出必须形成明确拒绝结果；不得中断主循环后留下半提交。
5. snapshot 恢复、复制补洞或 writer 位置若无法表示下一个高度、slot、epoch 或 sequence，必须拒绝启动或拒绝本轮处理，不得构造第二条权威历史。
6. 错误必须保留足以定位字段和现场值的上下文，并由调用链显式传播或映射为稳定的领域错误。

## 3. 接口 / 数据

### 已覆盖的权威路径

| 子系统 | 数值安全语义 |
| --- | --- |
| World 资源与事件 | `ResourceStock`、`ResourceDelta` 和资源余额使用受检累加；资源转移、power redeem、经济结算等高风险事件先预计算再提交；tick 成本失败记录 `ActionRejected`。 |
| Node points 与 PoS | epoch 结算的 award/cumulative/total/epoch 后继值在写入前全部校验；stake 与 slot 越界失败不得污染 proposal 或投票状态。 |
| Node 高度、恢复与复制 | height/slot 后继值、gap-fill 进度、proposal 摄取及 snapshot 恢复使用受检语义；不可表示的恢复状态拒绝启动。 |
| Replication writer | 本地 writer 的 epoch/sequence 位置计算可失败，local commit 构造必须透传错误；同 writer、writer 切换和无 guard 三种边界均须覆盖。 |
| Sequencer 与 lease | proposal slot/height、lease term/expiry 的推进使用受检语义；失败保留原 proposal/head/lease。 |
| Membership 与时钟转换 | in-memory 与 store-backed coordinator 在插入或持久化前计算 expiry；可失败接口使用受检转换，明确采用 clamp 的辅助函数必须稳定返回边界值并有测试。 |
| PoS 比率与 required stake | 超多数配置使用无乘法溢出的 `> 1/2` 判定，并在 proto、consensus、node 三层保持一致；required-stake 使用加宽计算和可失败窄化，无法表示时拒绝。 |
| Membership recovery/replay | retry attempt、backoff timestamp、调度间隔、policy/rollback cooldown 与计数/容量聚合采用受检算术；阈值和比率比较不得以饱和乘法改变数学判断，失败不更新 replay、pending、dead-letter 或 policy 状态。 |
| Governance archive/audit | recovery drill、audit retention、tiered offload、rollback streak 与 alert window 的时间和计数运算采用受检语义；时间回拨或边界异常在 archive、governance、alert 状态写入前失败。 |
| Membership reconciliation / sync / mempool | 调度与 revocation-dedup 时间差、reconciliation/sync report 计数、mempool batch/zone payload 字节数采用 checked 算术；溢出以带现场值的 `WorldError::DistributedValidationFailed` 返回，schedule、dedup、report 与 batch 状态不部分更新。 |
| Federated replay archive | 聚合扫描、复合游标 offset 与 archive 查询计数采用 checked helper；溢出不污染 cursor 或聚合结果。 |
| Runtime sequence rollover | event/action/intent/proposal 四类 `next_*_id` 在 `u64::MAX` 后以持久化的 `era + 1, sequence = 1` 滚动；snapshot 的 era 字段以 `serde(default)` 兼容旧快照（era=0），恢复与 replay 保持同一 era。现有边界分配与 snapshot 测试不证明非空 action journal suffix 的恢复完整性；该目标见下文。 |

<a id="numeric-action-allocator-replay"></a>

### Action allocator 的 journal-suffix 恢复

本节承接 [runtime 恢复合同](../prd.md#runtime-journal-suffix-recovery)，定义 action allocator 的目标行为与验收，不把目标记录描述成已有 DTO 或已完成实现。`next_action_id` 与 `action_id_era` 决定后续分配；恢复至目标切点 T 后二者必须与不中断执行至 T 的对照一致。只比较业务 `WorldState`、历史 receipt 或 state root 不足以证明该条件。

1. 权威来源是宿主真实完成的分配及其持久化记录。记录必须绑定宿主 allocation 身份、分配前 cursor、实际分配值、后继 `(era, sequence)`、所属事务与 action/queue 关联；具体语义由 [allocator 恢复设计](runtime-numeric-safety.design.md#numeric-action-allocator-replay) 承接。分配和业务执行是两个事实：持久分配不表示 action 已成功执行，业务拒绝也不使已经完成的分配变成未发生。
2. 一次分配只推进一次 cursor。一个 action 产生多个事件、重复出现的 `CausedBy::Action`、业务拒绝和 system event 都不能被按事件数量换算成分配次数。已预分配或显式指定的 ID 按宿主来源关联判断是否已计入 cursor，不能仅因 ID 出现在事件或队列中再次分配。数字最大值、现存 job 表及任意外部提交编号都不是分配进度的替代权威。
3. 分配来源记录、cursor 与相关 queue 变更在其所属的既有原子边界内一同发布和持久化。该边界失败时保持进入它之前的状态；此前已持久化的分配不得因后续 step 失败被回退。没有业务事件的分配只有被完整 checkpoint 或可验证的控制记录覆盖才可恢复，不能从“没有事件”推导出未分配、默认 cursor 或一个虚构的连续区间。
4. Canonical 历史恢复与 pending 恢复分开判断。checkpoint/suffix 已记录为消费或终结的历史 action 不得再次入队；仍有效的已持久 pending 保留原分配关联并按上位合同重审，不能分配新 ID 冒充原请求。未持久化的内存 pending 不获得新增恢复承诺；有持久 pending 时，cursor 与队列必须属于同一恢复切点，不能拼接旧 cursor 与较新队列或静默丢弃队列来制造一致。
5. rollover 必须从分配来源记录恢复原代际与后继顺序，不能按裸 `u64` ID 大小排序或取 max。旧 snapshot 缺 era 仍按 `serde(default)` 读取为 0；这不证明旧 tail 没有 rollover 或分配缺口。无法证明旧 tail 的 cursor/queue 完备性时，遵循 [共同兼容矩阵](../design.md#runtime-journal-suffix-compatibility)，不重执行 action 或通过遍历 job 补洞。
6. 同一规则覆盖 `World::from_snapshot`、普通 snapshot/load/replay，以及 `oasis7_world_repair_rebuild --source-world-dir`。repair 的保存和再次加载成功不能代替 cursor 与续跑验证；`--generated-world-dir` 是 bootstrap，不属于已有历史恢复证明。

<a id="numeric-clock-clamp"></a>

### 时钟转换的显式 clamp 边界

`oasis7_net` 的兼容时钟辅助函数 `unix_now_ms_i64` 将无法用 `i64` 表示的毫秒值夹到 `i64::MAX`，系统时间早于 `UNIX_EPOCH` 时返回 `0`。这是有损边界值，不是精确时刻或成功推进权威状态的证明。该约定不把其他可失败的时钟、lease 或 schedule 接口改成 clamp；它们继续遵循各自的受检失败合同。实现入口和现有边界测试见 [clock 设计条款](runtime-numeric-safety.design.md#numeric-clock-clamp)。

## 4. 验收

- 每条可失败推进路径至少有一个边界测试，验证错误类型与状态未变。
- 涉及 durable write 的路径必须验证失败前未写入或写入事务整体回滚。
- 涉及 replay/recovery 的路径必须验证拒绝结果可观测，且不会生成新的 committed history。
- rollover 必须验证边界分配、snapshot roundtrip 与缺失 era 字段的 legacy load；它只消除溢出失效，纯 `u64` ID 在不同 era 极端远期仍可能复用，不能表述为全链路复合 ID 唯一性。
- Action allocator 必须通过 [J1 计划验收](runtime-numeric-safety.design.md#numeric-action-allocator-validation)：从合法 finalized checkpoint C<T 加完整非空 suffix 恢复，在权限、资源、版本与输入相同的对照中，新 recipe 正常完成一次且不复用既有 settled job；另独立验证多事件/拒绝、预分配/pending、rollover、旧 tail 与各恢复入口。root 相同或“效果至多一次”不能替代该正向续跑结果。
- 文档或实现若改变受检失败、rollover、clamp 三者之一，必须同步更新本文、对应 design、相关测试、PR 与实际验证记录。

## 5. 里程碑

- M1：phase 1–12 的永久语义与当前代码/测试 evidence 完成核对。
- M2：稳定 PRD/design 文档建立并接管专业权威。
- M3：phase 1–12 的 36 个阶段源文件、活动索引和历史入口分两批完成原子迁移。
- M4：文档治理、陈旧引用、定向测试和 frozen-head review 通过。

## 6. 风险与边界

- 本文不要求全仓库立即采用 BigInt 或统一 newtype。
- 本文不要求将纯 `u64` 外部 ID 立即迁为复合 ID。
- 本文不把实现路径、测试任务状态或运维阈值迁入产品 PRD。
- 本文不宣称模块、集成、长稳或发布就绪；发布证据仍由对应测试与任务 evidence 承载。

## 7. Validation & Decision Record

| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| [PRD-WORLD_RUNTIME-041](../prd.md#prd-world-runtime-041) | runtime numeric safety | `test_tier_required` + 定向边界测试 | 文档治理、陈旧引用扫描、受检算术与原子性测试；clock clamp 的准确测试与覆盖边界见 [设计验证映射](runtime-numeric-safety.design.md#111-验证映射表) | world、consensus、node、replication、membership、net clock |
| [Action allocator 恢复](#numeric-action-allocator-replay) | [#4455](https://github.com/eng-cc/oasis7/issues/4455) J1 | 目标 `test_tier_required`；文档修订按 S0 | [设计验证映射](runtime-numeric-safety.design.md#numeric-action-allocator-validation) 的 J1-CONTINUE / ORDER / PENDING / ROLLOVER / ENTRYPOINTS 均为 planned，不能当作已存在或已运行测试 | World allocator、pending queue、event-suffix replay、普通恢复及 source-journal repair；不扩展外部 ID ABI |

| 决策 ID | 选定方案 | 否决方案 | 依据 |
| --- | --- | --- | --- |
| DEC-WR-NUMERIC-001 | 以稳定专业契约吸收已完成的 phase 1–6 | 长期保留六组三件套作为并列权威 | 阶段文档重复、设计模板化且项目状态已完成；稳定契约更利于防漂移。 |
| DEC-WR-NUMERIC-002 | 明确区分 checked failure、rollover 与 observable clamp | 统一写成“全部越界都失败” | 当前接口签名与兼容要求不同，clamp/rollover 是受约束的显式策略。 |
| DEC-WR-NUMERIC-003 | 以同一稳定专业契约吸收已完成的 phase 7–12 | 继续保留六组三件套与稳定契约并列 | phase 7–12 均是同一 numeric-safety 专业链中的完成记录；合并后仍以模块和边界测试保留精确证据。 |
