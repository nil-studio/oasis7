# Agent 工业制造系统验收手册

- Owner role：`qa_engineer`；协作：`runtime_engineer`、`agent_engineer`、`gameplay_designer`。
- 生命周期：`active` 验收计划；Last reviewed：2026-10-19；**全部场景尚未执行**，不是实现、测试通过、玩家理解或发布证据。
- 产品要求：[REQ-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#req-wr-aos-004)；产品验收：[AC-WR-AOS-004](../../product/world-rules-core-gameplay/agent-ownership-and-stewardship.prd.md#ac-wr-aos-004)。
- 配套合同：[Agent 工业制造系统设计](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#manufacturing-validation)；唯一政策输入：[agent-manufacturing-v1](../../game/gameplay/gameplay-agent-claim-economy-contract.prd.md#agent-manufacturing-v1)。本手册为既有七组验证计划提供稳定验收源，不另定配方、费用、权限或时间政策。

Trace refs: O7-REQ-e7deec0c-7c4b-4f08-a2df-db2839cda928

每次执行固定 source/tested tree、配置、政策 revision/hash、world/chain/genesis、资源/设施/主体来源版本与权威输入顺序。记录请求、quote/order/lot、前后物料/电力/liquid/claim 预留及已消费量、控制来源/identity generation、receipt、故障切点和未覆盖范围。缺制造 producer、共享资源守卫、控制来源 consumer、持久化驱动或正式读面时保留未验证，不把源码存在和历史测试名称记为通过。

`test_tier_required` 使用同一候选的规则与 runtime fixture 验证确定性、守恒、当前权限、完整提交和拒绝；`test_tier_full` 再覆盖正式持久世界、重启、适用跨节点顺序、正式 Service/host 及 Viewer/API/Agent。仅内存 replay 不证明正式持久恢复，仅 API accepted 不证明交付。以下数值来自首版政策或明确的对抗测试输入，实际运行按固定政策 artifact 读取，不维护第二套默认值。

<a id="mfg-v01"></a>
### MFG-V01：报价、费用与只读边界

关联 [DES-AGENT-MFG-001](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-001)。在 required 规则 fixture 中覆盖标准五种材料、48 电力、slot-2/3 的逐项向上取整及 liquid 预留报价；主 Agent 只有注册绑定且没有经济 claim 时，额外制造仍不能取得 slot-1 费用或 starter 支持。

分别修改相关材料/设施/配方/权限 revision、推进无关世界事件，并在 5 tick 报价截止前、恰到截止和之后确认。相关漂移或过期要求重新报价，无关事件不能静默改变固定报价。查询和重复查询前后资源、预留、owner、订单及 upkeep 均无效果；报价须分别包含投入、认领款、持续费用、原地交付、最长窗口、最大损失与不扩张选择。

证据为输入 policy/quote fixture、逐项计算结果、前后余额/预留与拒绝结果；不证明数值平衡或玩家理解。

<a id="mfg-v02"></a>
### MFG-V02：资源、产能与所有取得入口的共享守卫

关联 [DES-AGENT-MFG-002](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-002)。固定同世界一座 Assembler MK1 的两个普通槽，required runtime 分别构造材料、电力、liquid、claim cap、recipe slot 或成品保管位不足。每个拒绝点比较完整账本、占用、订单和序号，不能留下部分预留或扣费。

枚举制造与 Transfer、普通 ScheduleRecipe、普通 ClaimAgent、设施回收、紧急保供及组织资产处置的合法提交顺序；同一实际资源不能被两个业务承诺。采用设计 §9 的 100 次并发/重试刺激，合法成功集合始终满足资源守恒、一设施一未结单、普通工业保留容量及同来源唯一占用；此刺激不作为 HTTP 吞吐承诺。

固定反例：主 Agent 只注册，主体 cap=2，制造第二个并完成交付后，以普通 ClaimAgent 请求第三个。必须因“主绑定 ∪ 经济 claim targets ∪ 制造预留”的 prospective 集合超限而拒绝且不扣费；不能在制造预留释放后退回只统计经济 claim 的旧守卫。同集合内已有目标另建经济 claim 不重复计人数，但仍受原经济规则限制。

证据为事件顺序、完整余额/预留 oracle、工厂占用和容量集合，不能用单 handler 测试代签所有冲突 consumer 已接通。

<a id="mfg-v03"></a>
### MFG-V03：合法生产、取消、恢复与损失截止

关联 [DES-AGENT-MFG-003](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-003)。required runtime 从固定 quote/order 与权威时间启动，分别观察：开工前取消全部原路解冻；开工后材料/电力只消耗一次，普通取消不能退回投入；永久失败或报废只解冻尚未结算认领款，没有自动返料或转卖。

枚举同 tick 的取消/开工、恢复/过期顺序，并在启动、生产恢复、硬截止及完成后交付窗口的 `deadline-1 / deadline / deadline+1` 观察结果。正常生产须累计 12 个合法 tick；重复 Progress、停世界和重连不能增加工作量或延长窗口，未交付不发生新 Agent upkeep。

固定反例：t0 开工、t1 失权停产、t13 重新认证恢复。恢复不能立即把无权限期间算作 12 tick 完工；`completed_work_ticks` 只累计合法已提交进度，预计 `ready_at` 可以重算，原硬截止不刷新。新授权只续同受益人/工厂/配方/资金来源的原单，不复活旧 grant；硬截止前剩余时间不足完成生产时，按期报废。

证据为逐 tick 状态、工作量、定时器依据、原授权/新授权及物料/资金分类差分；不推断远程运输、分段退料或 salvage 能力。

<a id="mfg-v04"></a>
### MFG-V04：原子交付、真实控制与跨请求唯一性

关联 [DES-AGENT-MFG-004](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-004) 与 [DES-AGENT-MFG-005](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-005)。required runtime 固定 quote/order/lot，full 补正式 Service/host 鉴权与适用持久化观测。

| 对照/刺激 | 必须观察的结果 |
| --- | --- |
| 同 quote 或同 lot 换 request ID、跨设备重试、并发订单、伪 lot/目标 ID、跨主体/世界重放 | quote/order/lot/Agent 业务索引阻止第二次交付；旧请求不被当作新业务来源，拒绝不产生资源或控制效果 |
| 交付前操作人失权、受益资格失效或声誉降档 | 基于当前条件拒绝/等待，不改受益人、不越 cap、不因旧报价签名绕过 |
| 在 AgentCell、claim、control source、capability identity、费用和 receipt 各写点注入失败 | 整体无半交付；成功时仅一个 Agent、制造来源、有效 claim、控制来源与完整经济结果，必须属于同一个提交 |
| 新目标没有任何 starter 记录；原 owner 与其他主体分别访问和发直接意图 | 合法 owner 经正式 Service 和 host 可读/指令，其他主体拒绝；后续真实耗能动作先经普通合法资源转移满足条件，不能用制造增发初始资源 |
| owner 显式建立受限 grant，对照未建立 grant 和复制主 grant | 只显式新 grant 在既有范围/预算/期限内执行；制造不自动授予自治，主绑定、主目标和全部 starter 余额不变 |
| 以制造目标作 owner 根、递归或循环 root，或伪造多根 | 明确拒绝；合法根只能一次归一到现有非制造主主体来源 |

证据同时覆盖成功事实和失败零差分：Agent/claim/source/identity、余额、唯一索引和 receipt；仅 action ID、accepted 或历史 claim 回归不能证明此项。

<a id="mfg-v05"></a>
### MFG-V05：同切点恢复与旧数据兼容

关联 [DES-AGENT-MFG-006](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-006)。full 在同 source/config/world 下，分别于确认、开工、完成、交付的 prepare/persist/publish 边界中断、丢 ACK 和重开，使用 snapshot + 非空 suffix replay 与不中断参考执行逐项比较。

确认已持久化的 reserved 恢复原预留/截止；开工已提交不再次扣料或补离线工作；完成恢复同一 lot；交付恢复同一 Agent/claim/控制来源与 generation/receipt。未知结果先查原请求，不能假定已回滚。恢复 owner 读写仍消费当前根身份和存续 claim，不能用历史制造 receipt 复活旧来源。

篡改或缺失 parent、来源、quote/lot/identity 唯一索引、预留、owner source 或 policy 时拒绝发布该制造写路径。分别验证可信旧空 schema 的显式迁移、新未知 schema 的拒绝、旧 writer 禁止忽略已交付 provenance，以及历史压缩后用新 request 再交旧 lot仍被拒绝。

证据为正式持久世界版本、恢复前后状态 hash/逐项账本、receipt 与故障定位；内存 replay 不代签持久恢复或跨节点一致性。

<a id="mfg-v06"></a>
### MFG-V06：跨入口含义、容量与单主 Agent 路线

关联 [DES-AGENT-MFG-007](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-007)。full 使用同一候选与来源 commit，从正式 Viewer/API/Agent 读取全部订单阶段；对缺字段、旧缓存、主体切换、响应丢失和断线恢复分别核对。

各入口给出相同费用/投入/损失分类、窗口/进度、原工厂位置、当前 blocker、receipt 与合法下一步；未知不显示为交付，控制列表同步不出现第二次认领按钮。未支持或被裁剪入口明确 unavailable/incomplete，读面不产生确认、续期、解冻或新的权限。

按设计预算验证默认 20、最多 50 的分页及 16 KiB 请求边界；不把未读下一页当空订单，不把超限截断当成功。原主 Agent 同时继续已支持的采集/普通工业路径，额外制造保持可选择；无目标的旧入口仍定位原主 Agent，不因多个控制对象随意选取或制造歧义。

证据为各入口固定 projection、正式截图/API 结果、原主 Agent 行动回执和授权对照；静态文案不证明真实体验、时延或玩家理解。

<a id="mfg-v07"></a>
### MFG-V07：交付后的维护、权限变化与 quota 非供给

关联 [DES-AGENT-MFG-004](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-004) 与 [DES-AGENT-MFG-005](../../world-runtime/runtime/agent-industrial-manufacturing.design.md#des-agent-mfg-005)。required 核对生命周期/权限不变量；full 以同候选已交付 Agent 跨真实 upkeep epoch、释放/新认领、正式鉴权与重启验证。

1. 覆盖 upkeep 支付、grace、ReleaseRequested、正式 release/reclaim。沿既有生命周期施加限制，真正终态才同次使旧控制来源失效并推进 identity generation，原债务/制造身份/历史保留。
2. 正式终态后重启、旧 owner 缓存重试、旧 grant/signer/capability 和旧待决意图全部不能取得新效果。合法新 ClaimAgent 在其独立准入、容量及义务条件通过后创建后继 source/identity generation，不继承旧 grant、订单受益人或退款。
3. 根来源撤销、revision 改变或转给别的主体时，即使目标 identity 尚未 Refresh，所有实际执行 consumer 也须拒绝旧权限。原合法主体完成既有身份恢复/密钥轮换后，使用独立当前根证明显式 Refresh 同一存续 claim；suspended 目标可进入专用恢复准入，其他普通动作不能使用这一例外。刷新不迁移主绑定、不复活旧 grant/意图。
4. raw event、自定义 module receipt、普通 quota 与旧制造来源不能创建 Agent、补材料、免产能、重发 starter 支持或产生新控制权限。对照合法生产→一次交付，保留单主经营与明确扩张选择。

既有 [claim 回归源码](../../../crates/oasis7_runtime/src/tests/agent_claims.rs)仅作对应原生命周期的对照，需新增制造来源实例后另行执行。适用实玩按[Agent 自测手册](agent-runtime-self-playtest.manual.md)组织正式支持入口的受限主 Agent 路线与扩张样例；输出 CI、世界结果、来源 generations、拒绝记录与行动归因。Agent/自动化证据不代签真人理解、完整部署或 AOS-005 经济转让验收。

所有 verdict 只写入后续实际实现 PR 与运行证据；本手册及系统设计关系检查通过仍仅证明文档和验收计划可追溯。
