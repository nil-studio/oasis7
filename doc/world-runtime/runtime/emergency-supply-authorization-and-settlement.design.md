# 紧急保供授权、冻结分配与结算系统设计

- 设计 ID：`DES-WR-ES`
- 状态：`active`（长期目标设计；不是已实现或发布声明）
- Owner role：`runtime_engineer`
- 专业职责：`gameplay_designer` 提供危机、优先依据和经济合同；`blockchain_ops_engineer` 提供治理授权与分布式验证；`viewer_engineer` 提供结果投影；`qa_engineer` 验证组合边界。
- 上游产品：[常态市场与有界紧急保供](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001)
- 审读日期：2026-10-09
- 固定输入基线：canonical repository `eng-cc/oasis7`，source commit `6b7ed8347fa93290b7b124a693efe5695d405508`。本文的上游路径和 fragment 在该基线读取；实际实现和验证版本另由 PR 与实际 CI 记录，不以文档状态推导。

## 1. 问题、目标与非目标

产品已约束危机证据、最小授权包、公平分配、退出与单次世界效果，但需要专业技术合同把治理授权、资源竞争、物流、结算、恢复和玩家读面连接起来。本设计定义目标状态机、逻辑数据关系、提交边界及验证场景，使授权或分配决定不会冒充物资到达或补偿结算。

不新增危机阈值、必需品清单、优先权重、价格或配给公式；不实现新服务、协议或数据库 schema；不改变产品交互设计豁免。没有专业政策与实现证据时，保供能力保持未支持，常态市场继续运行。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | 仅完整、有效且权威确认的危机授权可进入紧急执行；缺项不产生部分效果。 | [DES-WR-ES-001](#des-wr-es-001) | gameplay_designer 的危机政策；blockchain_ops_engineer 的授权合同 | 不裁定危机阈值与签名格式。 |
| [AC-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | 仅完整、有效且权威确认的危机授权可进入紧急执行；缺项不产生部分效果。 | [DES-WR-ES-001](#des-wr-es-001) | gameplay_designer 的危机政策；blockchain_ops_engineer 的授权合同 | 不裁定危机阈值与签名格式。 |
| [REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | 冻结资格、需求、范围、资源、政策；等价重排序与重复投递保持结果；漂移原子拒绝。 | [DES-WR-ES-002](#des-wr-es-002) | gameplay_designer 的分配政策；runtime_engineer 的提交与资源合同 | 不选择权重、配额与 lottery 算法。 |
| [AC-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | 冻结资格、需求、范围、资源、政策；等价重排序与重复投递保持结果；漂移原子拒绝。 | [DES-WR-ES-002](#des-wr-es-002) | gameplay_designer 的分配政策；runtime_engineer 的提交与资源合同 | 不选择权重、配额与 lottery 算法。 |
| [REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | 到期、撤销与续期不迁移旧请求；保留历史且同一需求不重复获益。 | [DES-WR-ES-003](#des-wr-es-003) | runtime_engineer 的时间与恢复；gameplay_designer 的在途处置合同 | 不以新授权修改既有交付或所有权。 |
| [AC-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | 到期、撤销与续期不迁移旧请求；保留历史且同一需求不重复获益。 | [DES-WR-ES-003](#des-wr-es-003) | runtime_engineer 的时间与恢复；gameplay_designer 的在途处置合同 | 不以新授权修改既有交付或所有权。 |
| [REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | 介入限于声明范围，资格不可交易；交付、存储、结算与 receipt 不可旁路，结果有真实下一步。 | [DES-WR-ES-004](#des-wr-es-004) | runtime_engineer 的物流/结算；viewer_engineer 的投影 | 不代签正式 surface、跨节点与发布验证。 |
| [AC-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | 介入限于声明范围，资格不可交易；交付、存储、结算与 receipt 不可旁路，结果有真实下一步。 | [DES-WR-ES-004](#des-wr-es-004) | runtime_engineer 的物流/结算；viewer_engineer 的投影 | 不代签正式 surface、跨节点与发布验证。 |

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标状态 | 差距 / owner |
| --- | --- | --- | --- |
| 产品要求 | 上游已有四组稳定 REQ/AC 与 ES-1..8 组合验收。 | 每组要求有系统条款和可执行验收场景。 | 本文补齐设计承接，不新增产品真值。 |
| 治理保护 | [治理入口](../../../crates/oasis7_runtime/src/world/governance.rs) 已有 emergency brake/veto。 | 保供授权有独立身份、范围、期限和政策输入。 | brake/veto 不构成保供资格；runtime_engineer / blockchain_ops_engineer。 |
| 保供状态与结算 | 未验证；本次未完成所有实现路径与部署审计。 | 授权、冻结批次、请求、资源及 receipt 可联合恢复。 | 本文逻辑合同尚无对应 wire/schema 与专用集成测试；runtime_engineer。 |
| 玩家与真实网络 | 未验证。 | 自己的结果、依据、未完成量与下一步在正式读面可核对。 | viewer_engineer / qa_engineer；不得由文档检查推导已交付。 |

## 4. 边界与结构

治理侧只生产权威确认的授权或撤销事实，不能直接写库存、余额或请求结果。玩法政策拥有危机判据、经济约束与优先规则；runtime 验证其适用版本并拥有世界时序、资源预留、分配与结算提交。物流和合约子系统生产到达、存储、支付或最终结算事实；P2P 承载既有权威日志/状态同步，不能由落后节点替代当前世界状态。

玩家、Agent、LLM 和 Viewer 提交的证据与请求是 intent/advisory；签名有效不等于危机真实或拥有执行资格。Viewer/API 只读取权威投影，客户端缓存、提交顺序、管理员队列和重连均不能创建优先权或临时资格。拒绝保供请求不得冻结范围外常态交易。

## 5. 关键运行流程

<a id="des-wr-es-001"></a>
### DES-WR-ES-001：授权验证与有效期

1. 接收带身份与政策版本的危机证据及授权提案，验证必需品、品类/地点/主体范围、证据快照、授权来源、开始/到期时间、介入方式、补偿/公开分配规则及复核/申诉入口。
2. 玩法 authority 必须确认匹配预声明系统性危机；治理 authority 必须确认授权。缺项、未知版本、越界或证据不可核验时拒绝，库存、资金和资格零变化。
3. runtime 在权威事件顺序上确认授权，并在每次产生新紧急效果的提交点复核。时间使用权威世界时间，采用开始含、到期不含的区间；客户端时间与后台定时器不赋予资格。
4. 待审或尚未开始只有可读状态；有效授权只允许范围内受支持的介入。常态价格波动、局部故障或治理多数支持均不能直接启用保供。

<a id="des-wr-es-002"></a>
### DES-WR-ES-002：冻结批次与确定性分配

1. 批次收集窗口及关闭规则必须由政策预先声明。登记身份不代表分配资格；关闭后冻结去重后的请求集合、资格、需求、范围、可用资源、政策版本与权威世界版本。晚到请求只能显式进入其他适用批次，不能修改冻结集合。
2. 依据声明政策计算排序与分配；相同输入必须产生相同结果。输入遍历使用规范化身份排序，但排序本身不能成为未声明的优先权。tie resolution 必须明确；若 lottery，冻结前确认权威 seed 及其来源证明，缺失时阻断，不回退为客户端随机数。
3. 冻结是只读评估快照，不保证资源可永久使用。提交前复核所有相关版本与授权有效性，并检查资源及累计需求余额；无关世界变化不必使批次失效。相关漂移时整个批次原子拒绝，不能部分扣减或静默改算。
4. 分配结果、资源预留、请求履行记录与 receipt 在同一权威提交中确认。预留是受既有资源合同约束的占用，不是复制库存或完成交付。竞争批次先后按权威提交序决定可用资源；失败批次可在当前快照下重新报价，但不能自动继承旧排位或加入新授权。
5. 已提交批次不可追溯重排。重复投递先查询已提交身份；同一身份且同一内容返回原结果，内容不同拒绝。政策更新或续期只能通过新的显式请求与批次评估。

<a id="des-wr-es-003"></a>
### DES-WR-ES-003：退出、续期与跨授权去重

到期是有效性谓词的直接结果，即使清理任务尚未执行也不能产生新紧急效果。撤销与申诉裁决按权威事件顺序生效；并发提交以同一序列中的有效授权状态判定，不能先执行再检查。

授权状态逻辑为 `待审 -> 已确认未开始 -> 有效 -> 到期/撤销`，拒绝为独立终态。续期创建新授权身份、证据、范围与期限，不复活旧授权。旧请求只能读原记录；未确认部分停止在待决/到期/拒绝结果，不自动在新授权继续。等待复核只提供恢复入口，不能绕过期限。

对同一需求，幂等约束跨授权生效：runtime 保留需求来源身份、请求 lineage、已确认量与未完成量；新请求必须显式关联旧请求，重新验证当前需求及资格，仅可评估未履行且未被其他活跃承诺占用的部分。可重新评估量为当前权威需求量减已履行量，再减尚未终止的预留、分配或在途承诺量；同一承诺在阶段转换中只计一次，剩余量不得小于零或超过可核验需求。旧承诺只有在资源释放/合同终止及需求占用释放原子提交后才可供新请求使用；旧物流最终结算必须原子把占用量转为已履行量，不能先释放再结算。旧承诺取消与最终结算竞争时只能有一个合法结果，无法证明终止或占用量时阻断新分配。客户端生成新的 request ID、隐藏旧关联或更换授权不能清空已履行量。无可验证需求身份或无法证明剩余量时阻断，不能假定是新需求。真正的新需求必须有新的权威来源证明。

部分完成分别保留已提交分配、预留、交付与结算量。到期时禁止使用剩余临时资格；释放尚未使用的预留需通过资源合同原子更新与唯一 receipt。对于已进入物理物流但未最终结算的货物，按进入时绑定的物流/合约义务保留、退回或终止，不自动删除货物、不再依据过期授权授予新补偿。专业在途处置合同缺失时保持可审计待决并阻断后续紧急执行，由 owner 明确处置；不能伪造交付、没收或债务。

<a id="des-wr-es-004"></a>
### DES-WR-ES-004：物理结算与可读结果

分配提交只证明资源分配/预留，物流到达 receipt 只证明实际到达；最终需求履行必须满足适用的目的地存储、合约结算和世界资格合同。采购、补偿、分配与 rationing 各自必须绑定受支持的 effect kind 与证明条件，不能复用一种 receipt 冒充所有结果。

资格绑定受益主体、授权与范围，禁止转让、兑换、叠加或转换成治理权；非法操作在资源变更前拒绝。分配不会生成通用货币；补偿金额和资金来源须来自合法专业合同且守恒，不能无偿没收或凭空补足。

结果投影同时表达分配类别与履行阶段：`allocated`/`partial` 表示权威分配量，不能单独表示交付或结算；`denied`/`expired` 保留原因；登记、在途、待结算与完成独立表达。每条结果关联授权、批次、自己的实际量与未完成量、依据、世界版本、receipt 和可执行下一步。无 receipt 或陈旧来源显示待核验；不给出未支持的重试/申诉按钮承诺。退出后仍可读历史，但历史不能变成当前资格。

## 6. 接口与数据合同

以下为逻辑合同，尚未冻结字段名称、wire schema、ABI 或 API。实现前必须在专业合同确定类型、大小上限、错误枚举与兼容版本；未知版本拒绝，不做宽松解析。

| 接口 | producer → consumer | 身份 / 版本 | 顺序 / 幂等 | 成功 / 错误 | 兼容 |
| --- | --- | --- | --- | --- | --- |
| 授权事实 | 治理 → runtime | 授权身份、政策版本、证据来源、范围、时间、复核入口 | 权威事件序；同身份内容不可更换 | 验证可用或缺项/越界/失效 | 未支持介入类别拒绝。 |
| 需求请求 | 玩家/API/Agent → runtime | 请求身份、需求来源、lineage、关联旧请求、授权、内容摘要 | 相同身份同内容返回原结果；不同内容拒绝 | 登记/拒绝，不宣称 world effect | 旧客户端缺少必要身份时不可执行。 |
| 冻结分配输入 | runtime/玩法政策 → 评估器 | 批次、输入集合、相关版本、政策与 seed 证明 | 规范化集合及确定函数；快照不可改写 | 报价/阻断，不产生资源变化 | 不兼容政策不可混算。 |
| 分配与履行提交 | runtime/物流/合约 → 权威日志 | 批次、effect kind、需求来源、资源版本、已履行量 | 联合条件提交；唯一 effect 身份 | receipt 或原子失败 | 不把旧 receipt 改写为新语义。 |
| 结果投影 | 权威状态 → Viewer/API | 请求、授权、来源世界版本、分配类别与履行阶段 | 重连只读取，不补发命令 | 当前结果或未知/陈旧 | 不支持字段需收窄 claim。 |

错误应区分授权缺失/失效、资格拒绝、快照漂移、资源阻塞、版本不支持、重复内容冲突与状态不可核验。网络超时表示结果未知：查询同身份的权威状态，不能换 ID 自动重提。队列和批次超过声明容量时在登记前拒绝并保留查询路径，不隐式截断影响公平性。

## 7. 状态、事务与持久化

请求逻辑阶段为登记、冻结评估、分配已提交、履行中、最终已确认，另有拒绝/到期终态；部分履行保留数量而非复制整笔请求。批次阶段为收集、冻结、已提交或拒绝；拒绝批次不可继续提交旧评估。所有转换由权威事实驱动。

同一提交点必须原子写入资源预留/扣减、资金变化、需求累计履行、幂等结果与 receipt；日志发布失败不得留下经济变化。分配与后续交付可以是多个独立事务，每个 effect 使用独立身份并引用前序结果，不能把跨时间物流伪装成一个数据库事务。重复执行任何一步均最多产生一次该步效果。

恢复需共同读取授权、撤销/时间事实、冻结输入、政策版本/seed、资源占用、需求账本与 receipt；缺少任一依赖不得执行 pending work。checkpoint 与日志必须同一一致性边界；已提交但响应丢失返回原 receipt，未提交准备数据可丢弃。只有事实充分时才能恢复原授权下仍有效的请求，恢复不能给旧请求赋新授权。

accepted 仅表示登记；applied 需要权威 effect receipt；persisted 需要恢复后可回读的提交；跨节点可读与 finality 由既有 P2P 合同证明。去重记录和需求累计量不得在仍可重放或申诉的窗口前 GC；具体 retention 由[存储治理](runtime-storage-footprint-governance.prd.md)与协议重放窗口共同确定，不能用授权到期直接清除。

## 8. 部署、安全与运行约束

本设计嵌入现有世界 runtime 与权威事件链，不新增旁路管理员服务。治理签名只证明来源，危机适用性与范围须另行校验；Viewer、LLM、缓存和运维工具不能直接改分配账本。客户端 wall clock 不参与授权到期判定。

政策、证据、seed 与冻结输入必须可验证并有资源上限；审计输出仅公开解释所需依据，不披露他人敏感资格材料。证据不可达、落后节点、日志损坏、配额超限与评估超时均阻断相关保供，不阻断无关常态市场。分配不得以线程完成次序或超时先后改变优先依据。

真实网络、重启、跨节点恢复和正式 surface 的操作步骤沿用相应专业 manual/runbook；本文没有生产部署或实际网络验证证据。

## 9. 质量与容量

| 场景 | 环境 / 刺激 | 预期响应 / 判定指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- | --- |
| 公平与回放 | 同一冻结集合，改变投递顺序、重复次数与节点回放顺序 | 每主体分配量、类别、资源总变化与 receipt 因果一致；隐藏顺序影响为零 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002) | 计划，未实现专用测试。 |
| 原子与守恒 | 资源冲突、提交失败、响应丢失 | 失败无部分经济变更；成功每 effect 只一次；总分配不超预留供给 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002) | 未验证。 |
| 自动退出 | tick 到达边界、撤销与提交并发、清理任务延迟 | 失效后新增紧急效果为零；历史可读且常态继续 | [ES-VERIFY-003](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-003) | 未验证。 |
| 容量与隔离 | 批次/证据超限，计算或依赖超时 | 明确拒绝/阻断，无静默截断、乱序优先或无关交易冻结 | [ES-VERIFY-004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-004) | 上限待 owner 确定；不得声明吞吐/SLA。 |

## 10. 兼容、迁移与回滚

本次只补文档，不迁移运行态。未来实现不得将旧 emergency brake/veto 或普通市场队列转换成保供授权。缺少新 schema 的旧快照只可被识别为未启用保供；一旦写入新状态，旧二进制不得忽略字段后继续执行。

启用顺序为确定专业政策/接口与容量、实现状态与提交、加入兼容/恢复测试、验证消费者及真实网络、最后按既有发布入口启用。所有消费者未兼容时禁用新请求。升级必须保留授权身份、需求账本、receipt 和原政策版本；不进行双写到两套可独立执行的保供账本。

回退只能到经过兼容验证、能够读取已写入状态的基线。禁用入口不能抹去已提交分配、在途货物或结算；不可逆效果仅通过新的纠错/补偿合同处置。无法兼容时停止保供执行并保留状态供修复，不向旧版本丢弃状态强行回滚。

## 11. 验证设计与可追溯性

### 11.1 验证映射表

表中场景是未来实现验收计划；本次文档检查不构成运行证据。

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | [DES-WR-ES-001](#des-wr-es-001) | 非危机/缺项/越界输入无紧急效果。 | [ES-VERIFY-001](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-001)：固定候选的 runtime 状态与资源对账。 | 未来 PR 的测试输出、前后状态与 receipt。 | 尚无专用运行测试，不证明真实危机判据。 |
| [AC-WR-ES-001](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-001) Trace refs: O7-REQ-918d9050-5f73-40ad-b9d4-aba87233bdd1 | [DES-WR-ES-001](#des-wr-es-001) | 非危机/缺项/越界输入无紧急效果。 | [ES-VERIFY-001](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-001)：固定候选的 runtime 状态与资源对账。 | 未来 PR 的测试输出、前后状态与 receipt。 | 尚无专用运行测试，不证明真实危机判据。 |
| [REQ-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | [DES-WR-ES-002](#des-wr-es-002) | 重排序、重复、漂移与资源竞争不改变公平或复制效果。 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002)：确定性 fixture、故障注入与 replay。 | 未来 PR/CI 的分配向量、资源差分与回放结果。 | 不证明经济平衡、规模性能或生产随机性。 |
| [AC-WR-ES-002](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-002) Trace refs: O7-REQ-18fe18ca-4920-4fe0-aad3-69b5fc7b80a7 | [DES-WR-ES-002](#des-wr-es-002) | 重排序、重复、漂移与资源竞争不改变公平或复制效果。 | [ES-VERIFY-002](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-002)：确定性 fixture、故障注入与 replay。 | 未来 PR/CI 的分配向量、资源差分与回放结果。 | 不证明经济平衡、规模性能或生产随机性。 |
| [REQ-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | [DES-WR-ES-003](#des-wr-es-003) | 旧请求不跨授权，新请求仅评估可核验剩余需求。 | [ES-VERIFY-003](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-003)：边界 tick、竞态、部分履行、checkpoint 恢复。 | 未来 PR/CI 的旧新 lineage 与累计履行对账。 | 不证明实际物流退回、跨节点恢复。 |
| [AC-WR-ES-003](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-003) Trace refs: O7-REQ-51922b18-cc1e-4894-9c12-66c0a6def6d1 | [DES-WR-ES-003](#des-wr-es-003) | 旧请求不跨授权，新请求仅评估可核验剩余需求。 | [ES-VERIFY-003](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-003)：边界 tick、竞态、部分履行、checkpoint 恢复。 | 未来 PR/CI 的旧新 lineage 与累计履行对账。 | 不证明实际物流退回、跨节点恢复。 |
| [REQ-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#req-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | [DES-WR-ES-004](#des-wr-es-004) | 物理/结算不可旁路，投影诚实且范围外常态保持。 | [ES-VERIFY-004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-004)：同候选 API/Agent/正式 surface 与资源合同组合验收。 | 未来 full-tier 输出、真实浏览器与网络证据。 | 本次未提供实现、玩家或发布证据。 |
| [AC-WR-ES-004](../../product/world-rules-core-gameplay/market-normal-state-and-emergency-supply.prd.md#ac-wr-es-004) Trace refs: O7-REQ-c17942de-f8ae-44fe-8525-88560dccf0a3 | [DES-WR-ES-004](#des-wr-es-004) | 物理/结算不可旁路，投影诚实且范围外常态保持。 | [ES-VERIFY-004](../../testing/manual/emergency-supply-system-acceptance.manual.md#es-verify-004)：同候选 API/Agent/正式 surface 与资源合同组合验收。 | 未来 full-tier 输出、真实浏览器与网络证据。 | 本次未提供实现、玩家或发布证据。 |

## 12. 决策、长期风险与未决问题

| 决策 / 未决项 | 选择、后果与 owner | 失效或解除触发 |
| --- | --- | --- |
| 整批条件提交，拒绝逐项静默改算 | 保证冻结公平和资源原子性；高争用可能频繁重新报价。runtime_engineer。 | 争用指标表明无法满足容量目标时，提出保持公平的新方案并重验。 |
| 同一需求跨授权累计，拒绝只按 request ID 去重 | 防续期/换 ID 套利；增加账本与来源验证成本。runtime_engineer / gameplay_designer。 | 专业域确定需求来源身份与部分履行量的 schema；缺失前不得启用。 |
| 保留旧在途合同，拒绝退出即删除物流 | 保持财产和历史因果；无法处理的在途请求可能长期待决。gameplay_designer / runtime_engineer。 | 每种 effect 明确到期时预留释放、物流退回及结算义务并完成组合测试。 |
| 危机政策、优先规则与 seed 来源未冻结 | 本文只约束输入完整和可回放；不批准具体经济与随机算法。gameplay_designer / blockchain_ops_engineer。 | 专业合同确定版本、批次窗口与抗操纵 seed 证明后复核 DES-WR-ES-001/002。 |
| 容量、retention 与 wire 兼容尚未定量 | 无吞吐、延迟或上线承诺。runtime_engineer / qa_engineer。 | 实现前确定规模/预算、重放窗口、字段边界和平台矩阵，并验证 §§6–10。 |

本次设计补齐不改变治理权限、共识协议或持久化实现。未来实际改变上述边界时，按[开发流程规范](../../engineering/workflow/source-of-truth.md)安排对应能力的独立评审；设计存在不替代该评审与实际验证。
