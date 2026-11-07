# 地点 tenure 登记、规划与实际处分系统设计

- Owner role：`runtime_engineer`；gameplay 拥有[原 tenure 政策](../../game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md#tenure-policy)，区域、资产、授权、QA 共同审读。
- 状态：`target`；本文是完整目标技术合同，不声明现有登记/处分实现、实际迁移、补偿到账或玩家验收。
- 固定审读输入：`eng-cc/oasis7@cca8cda97cf121ebc1e25e96d6b0204a421323ea`，2026-11-05。
- 产品：[GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；冲突临时占有与条件转让只沿 [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004) 的有限交接。

## 1. 问题、目标与非目标

现有 site/owner 和建厂权限没有地点长期建设权的期限、续期、转让、通知与处分闭包。区域 charter 退化设计会请求 tenure 处置，但不能替未定义的登记服务执行收回。本文使每次权利变化、维护、实际腾空和补偿都有准确原对象、当前 authority、一次结果及恢复路径。

成功条件是同一地点可规划、有界使用且公共通行不被取消；到期/闲置/违约/公共必要性只能按原政策改变用途，独立资产、身份、合同和历史不被悄悄删除。权利停止、实际迁移、费用和案件可分别处于未结状态。

不授予主权，不创建一般土地市场或新的 levy，不改变区域成立/双资格/退化规则，不把工厂 builder 当地主；不选择新的 native institution pilot。时间/补偿政策只在原 gameplay 文件维护，本设计不另定第二组权利规则。

## 2. 上游约束与相关角色

区域 authority 提供合法空间、用途与当前授权；gameplay 提供已发布、已接受的参数和处分政策；tenure L2 instance 拥有登记及程序；原资产/运输/资源 writer 拥有真实位置、condition、custody、数量、费用；Kernel 只做通用权限、守恒、空间/资源约束和原子提交。Viewer/API/Agent 消费同一受权投影。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 准确地点/权利身份、用途、规划horizon、维护/实际使用与公共通行 | [登记模型](#tenure-identity) | 原区域空间、当前政策与资产authority | 不授予资源库存、主权或保证未来收益 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 取得、续期、规则化转让当前重验，完整重叠集合CAS且不重复生效 | [权利变化](#tenure-acquire-renew-transfer) | 当前主体/代表、第三方义务与真实资源owner | 转让不自动出售资产或接替旧债 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 四触发、完整通知、独立review、final guard和最小保全/硬尾部 | [通知与复核](#tenure-notice-review) | 原治理、tenure名册/随机、可信时钟/受理索引 | 他域panel或管理员无本案处分权；不证明真人时长合理 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 迁移/回收/保护custody逐项真实结算，停止权利不代表腾空 | [实际处分](#tenure-disposition) | 原工厂/物品/运输/空间writer | 不瞬移、不免费回滚、不抹历史损失 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 公共必要性先足额保护，净差或明确残余权益交换互斥单次结算 | [补偿与原路释放](#tenure-compensation) | 原报价/资金payer与当前补偿/资产authority | 不预扣未到账回收，不造无预算赔偿或重复价值 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | 原条件接受、真实连续持有/到期后一次正式转移；临时用途独立到期 | [冲突权利交接](#tenure-temporary-use) | 冲突instance和原tenure当前权利/第三方保护 | 比分/图标不代签长期转移，不延原期限 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 并发/原请求/G1三态/同代恢复保持唯一权利、真实处分和支付 | [原事务与恢复](#tenure-persistence) | 原ExecutionTransaction、G1与当前history reader | S0或旧site测试不证明新组合/跨节点通过 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | 当前权限/完整范围读取、256对象/4MiB/两invoke及复核关闭资金全部计入 | [有界资源与费用](#tenure-budget) | 原metering schedule、真实payer、受权source/index | 不全扫后截断；cap/算式不是性能实测 |

## 3. 当前状态、目标状态与差距

| 当前来源 | 本基线真实含义 | 目标差距 |
| --- | --- | --- |
| [Factory lifecycle](../../../crates/oasis7_world_state/src/state/industry_transition/factory_lifecycle.rs) | 建造使用 site/consume ledger/authority revision；维护改 durability，回收保留 receipt/retired identity | 不是地点权利登记、期限、收回通知或第三方资产迁移 |
| [现行 material transfer](../../../crates/oasis7_world_state/src/state/industry_transition/material_transfer.rs) | 原材料账本转移及 receipt；其合法入口/位置仍按原规则 | 不代表远程移动、tenure 交接、无限仓储或补偿资金 |
| [区域 charter 设计](regional-charter-authority-and-recovery.design.md#charter-degradation) | 已定义阶段及资产/tenure 各自处分责任，tenure 委托给本域 | 需本设计的登记/通知/处分/恢复适配器；不能用 charter dissolved 标记替代 |
| [Runtime 分类](../prd.md#native-surface-分类与冻结规则) | institution 语义不任意增加到 Kernel 专属 schema | 新 tenure 使用受治理 L2 instance 和通用物理/资源 effects，尚未实现或激活 |

## 4. 边界与结构

一份 tenure 的逻辑主键为 `(world_id, module_id, instance_id, tenure_id)`；升级不换身份，artifact/schema/activation 绑定另外记录。每份登记、维护/使用摘要、原通知/案件和逐资产处置目录归一个 instance。当前主记录上限见 §9；历史正文和已结记录保留原 canonical journal/可审计原件，不以 client 提供的 hash 冒充已验证来源。

目标 `TenureReader` 只按 exact key、有限范围及同一权威 cut 读取。地点占用索引由原登记/物理 writer 的 canonical 变化建立，记录原 key/commit、水位和完整性证明；它是可重建索引，不另造权利 authority。原子提交更新声明空间的索引与 tenure 状态；不可证明完整或落后于所用 head 时拒绝新登记，不能先全库扫描再截断，也不能以“索引查不到”推断空闲。

一笔制度迁移最多调用两个事前声明、当前有效的 L2 instance：本 tenure 与需要原子交接的冲突或原义务 instance。空间/资产/资源事实由原通用 gate、原 domain writer 和有界可信输入消费；必要的第三次制度调用必须先用独立已提交步骤完成可冻结前置，原子提交时重验该前置仍有效，否则拒绝当前操作，不偷偷免费调用或分两次发布应原子交接的权利。

<a id="tenure-identity"></a>
### 4.1 地点、权利、资产与时钟

`TenureRecord` 包含原 world/instance、地点锚点与已批准的用途 scope atoms、持有人自然/组织主体及当前代表来源、policy revision/digest、起止、`Q/H/M`、最大累计规划上界、维护/实际使用证据摘要、完整资产/义务目录 root/count、当前 revision、通知/案件、临时用途、各费用 hold 与原 receipt。scope atoms 来自区域 authority 批准的具体 site/空间单位和用途；不得由申请者圈任意空域。原空间矩形/单位的整数 cm、邻接及物理重叠仍用原空间来源；重叠使用需显式共用政策，不能以不同 ID 掩盖同一实体位置。

租用的地点、在该地点的厂房、厂内库存、Agent 站位、临时访问许可是不同对象。`TenureAssetEntry` 对每项记原 domain/id/revision、真实 owner/custodian、位置/数量/condition、已承诺义务/第三方、允许的处分及其 authority。登记只能引用已存在合法对象或明确尚空的使用区，不能从名称创造物体。

`effective_world_time` 来自原已提交世界时钟；程序需要的服务可用性由原可信 availability 记录形成累计暂停量，引用完整区间和 cut。玩家离线不暂停；系统无法提供合法通知/提交/复核入口时只暂停受影响程序窗口，不追加参与者收益或重置原 world-time/history。既有已提交截止及暂停前缀持久保留；未知区间不得猜过期。正常使用期限与维护周期按原世界时钟，程序暂停不自动增加 tenure 长期经营权。

## 5. 关键运行流程

<a id="tenure-acquire-renew-transfer"></a>
### 5.1 取得、维护、续期与转让

1. `InspectTenure` 返回当前权利、空间/用途、有效政策、规划/维护/退出和完整性。quote 仅绑定当前 cut，不产生 hold 或顺位。
2. `AcquireTenure` 校验当前申请人/代表、区域授权、精确用途/期限、完整重叠集合、实际可用空间、所有已列义务、必要资产同意、review authority 和足额程序/关闭预算。双方同版接受与所有门槛齐全，才原子产生一个 tenure revision、有限费用/预算占用和 receipt。
3. `RecordTenureUse/Maintenance` 消费原 domain 的真实结果，绑定 `(tenure, original_effect, quantity_slice, purpose)`；同一不可分效果不靠改 purpose 重计。用途匹配、原维护量及期限可核才更新摘要；不调用世界动作来“证明已经发生”。
4. `RenewTenure` 仅原结束前 `4Q` 合法窗口，在同一当前版本核使用/维护、资格、累计 horizon、现有通知和新费用。成功追加原条款允许的期限；未支付/未知/拒绝保持旧期限和既有义务，不预留未来资格。
5. `TransferTenure` 绑定原 holder/revision、受让主体、剩余时界/费用、明确资产与第三方义务安排、双方当前同版接受。原授予代表不等于新 holder 授权。未决收回/权利争议拒绝普通转让；并发两 transfer、renew/transfer、temporary-use/transfer 均对同一 revision CAS，后者重读。转让不刷新 clock，不改独立资产/原债、不重发归属历史。

对重叠地点同时取得，事务还 CAS 对应空间索引版本和完整查询承诺；仅检查各自新 tenure ID 不足。索引与 domain 状态任一准备失败不留下半个权利或扣款。

<a id="tenure-notice-review"></a>
### 5.2 通知、限定理由与独立复核

`ProposeTenureDisposition` 只能使用政策的四种 reason：`expiry`、`qualified_inactivity`、`proven_breach`、`predeclared_public_necessity`。输入含原因事实完整区间、受影响用途、最早执行时点、资产/义务清单、已可行的安全方案、资金和当前处分 authority。区域退化、竞争得分、缺席或未知来源不能单独通过。

`TenureNotice` 在同一提交记录原政策/证据与各直接主体的持久 inbox 条目、发送/可取回状态和 `N`。发送外部通知只走原 durable outbox；邮箱 ACK 不是本通知生效或客户端已读证明。缺送达/入口不可用按 §4.1 暂停程序，不把重发变成新案。自然到期关闭新长期使用按原时界；缺提醒时仍需完成资产处分的独立通知流程。

每 `(tenure, notice_lineage)` 一案。按[原复核政策](../../game/gameplay/gameplay-regional-charter-tenure-funding-contract.prd.md#tenure-review-policy)保存 `N+2Q/3Q/4Q/5Q/6Q` 各时界、每主体/阶段证据份额、scope/evidence root、资格与完整冲突排除。名册固定后才消费其授权随机源的首个可用不可预选结果，保存证明与排列；初审和复核不同三人、两份相同 typed disposition 才有效。无合格人、预算、随机或当前授权时不临时改为管理员决定。

保全只限制直接争点的未执行不可逆处置，不冻全部地点生产、无关资产、通行或其他合同。final 与尾部使用同一 `case_resolution_key`；尾部先查全原决定/执行，不因少读一页推断无裁决。有效 final 继续；无有效决定时不作提前收回，原自然到期/已验证安全处置保持。来源 unknown 进入恢复，不把案件自动“驳回”或退款。

各阶段采用原政策半开区间，canonical受理时间必须严格小于截止。初审在申诉截止N+4Q之前不可执行不可逆处分；截止后同一事务核完整case/受理索引及原未知请求，证明无已受理待决复核，或读取已经完成的有效复核，才从preliminary转executable_final。有已受理申诉则等复核或原tail规则，不能先执行再补案；当前来源不完整就保持pending。到N+4Q恰好提交的新申诉超窗，之前已受理的不会因尚未投票丢失。

<a id="tenure-disposition"></a>
### 5.3 权利停止与真实资产处分

到合法处置边界，先在一个事务中改变后续用途权限并冻结完整处置目录；每个 `DispositionItem` 是精确原资产/quantity slice 与一个已选择的处理方式，具有稳定 effect key、原版本、费用、前置、阶段、实际 receipt 和剩余。

| 方式 | 原始条件与实际执行 | 完成/失败边界 |
| --- | --- | --- |
| `safe_complete` | 原有尚可合法完成的安全/既有义务动作，限原范围与截止/预算 | 实际原 receipt 才减少剩余；不接受新长期承诺或给新资格 |
| `relocate` | 原 owner 授权、可达路径、真实载具/容量/能源、目标空间/接收权限及容量 | 装载/途中/到达/接收分别记；只有实际入库/定位 receipt 才腾空原物理占用；失败保留实际位置与成本 |
| `recoverable_dismantle` | 原工厂/物品可回收规则、原 owner 接受、合法目的账本和工作资源 | 使用真实 condition 的原回收量；回收 receipt 与 retired identity 不重复，不能拿“曾建造成本”当全额退款 |
| `protected_custody` | 精确资产、仍合法保管位置/容量、原 title、可取回主体及期限/费用；仅在原政策允许时 | 保管不改变 owner、不提供免费无限容量；无新使用权，仍占空间明确阻断重叠建设，缺前置保留 pending |

各项只可依原领域允许的状态转换，不把普通 direct material transfer 当远程迁移。已有 job/WIP/transit/受托债权按原 writer 形成单次安全处置；不能由 tenure module 伪造 FactoryRecycled、清理别人库存或取消已生效付款。前置缺失不使用“管理员强制完成”，返回具体 source/authority/capacity/funding 缺口。

迁移截止后关闭该 tenure 的新用途；处理项依然分别为 completed、custody、blocked 或 outcome_unknown。已结项不重复执行，未结项仍可原请求恢复和合法安全动作。`vacated` 只有完整目录证明全部实际占用已按规则腾空且原空间索引更新后才能写；不是由 `expired` 推出。后继申请不能覆盖仍有真实占用的 scope。

<a id="tenure-compensation"></a>
### 5.4 补偿、费用和原路释放

公共必要性必须先有 `CompensationPackage`：原提案 payer、逐资产原领域报价/版本、每种资源单位和 cap、预计回收量、搬迁/关闭成本、原接受或有效复核、受保护资金及受款主体。报价采用原实际 condition 和重建/修复规则；未来利润、原维护消耗及真实已发生损失不伪造为可退资产。没有原领域合法估值/材料对应来源则补偿包不完整，不能开始不可逆收回。

不可逆处分前保护`accepted_replacement`的完整逐资源最大量，加原搬迁/安全费用；尚未实际到账的回收按零抵扣，不允许只保护预计净差。原清单中所有已知受影响源/数量都在预算闭包内，新增资产或提高cap须先取得新资金/接受，不能先处分后筹钱。

净差模式须先由原writer封存该item的最终回收结果，并证明全部应返还量实际到账或已按原合法规则真实损失；随后逐资源计算 `payable_r = max(0, accepted_replacement_r - actually_returned_recoverable_r)`。保护custody、途中或原结果unknown未结时不先按零回收付满再继续返还。另一种仅在原owner同版明确接受的模式是足额替代补偿换取原精确片段的残余回收权益：CAS原残余权利清单/受益者改为payer与实际付款同根，后续原片段回收只进入该payer合法账本；没有这份明确授权不能改残余归属。两个模式共享一个`item_compensation_resolution_key`和source数量消费索引，互斥、不得相加。

赔付原effect key同时CAS包remaining、付款hold、真实payer/recipient、实际回收/残余权益索引与receipt，原子实际支付；随后释放未用保护余额。不同资源不净额互抵，已返还与已支付的同一补偿价值不重复。原包或资金不足保持payment_pending。新报价更贵不直接扣第三方或把差额变成无预算债务；需当前新授权与受影响人接受，原已确定责任保留。

普通到期/闲置/本方违约按原政策无自动经济赔偿，但原回收/取回资产不因无赔偿被没收。fees、bond/compensation、物理工作投入和 closure reserve 分账；实际消费不会释放，未用余额在原结果明确且本用途义务结清后一次归原 payer。支付未知不释放同一钱再重付；不得用预期拆解所得为本次拆解之前已不足的费用融资。

<a id="tenure-temporary-use"></a>
### 5.5 冲突条件转让、临时用途与赛季

暴露前的 `ConditionalTransferConsent` 绑定原 tenure/revision、holder 的当时有效签署、精确用途/可转范围、原结束上界、受让资格、冲突/连续持有/到期判据、资产/第三方安排和原义务；当前登记为已生效的有限条件承诺。承诺进入公示后，不能借换代表、普通转让/续期或退出单方抹掉；后续有权撤销/保护决定仍适用并可阻断未结处置，不通过过期的操作者 grant 执行。

冲突 instance 在到期冻结完整 contested occupation 和截至冲突结束仍连续满足 H 的证明；`SettleConflictTenureTransfer` 同时读取此证明、原条件同意、当前 tenure/受让接受与资格、仍合法的 settlement authority、未决保护/第三方义务和空间状态。一个两 instance 事务 CAS 原 holder/revision、消耗唯一冲突权利槽、保存原/新holder链及剩余期限/用途/维护、建立实际资产交接清单并产生两侧同根 receipt。只改变已暴露的正式 tenure 范围，不能延长原结束时间；分割用途须原已定义可分原子scope，不能切开不可分权利或制造双holder。原期限已结束、连续证明有洞、资格或独立保护不满足则不转让；缺读回先查原结果，不能制造另一条权利。

临时使用是另一个明确 outcome：在原条件允许时写 `TemporaryUseGrant`，原 holder 不变，grant 不含资产处分权，截止为原 tenure/冲突许可较早者。临时控制不能被这个分支自动升级成正式条件转让。两类均保留未列独立用途和资产；原holder签署的条件承诺不等于把其私人身份交给受让者。

每次新临时使用按当前有效时界/撤权检查，即使后台到期清理尚未执行也拒绝旧 grant。关闭只撤后续用途，原在途/物品仍由本设计逐项处置。赛季进入新窗口不重发 grant、不刷新原期限、不取消普通长期 tenure；原长期权利不得被当作赛季奖励清零。

## 6. 接口与数据合同

| 目标命令/读面 | 必填绑定 | 输出与主要拒绝 |
| --- | --- | --- |
| `Inspect/Acquire/Renew/TransferTenure` | world/instance、原 key/revision、current authority、policy/digest、空间完整性、主体接受、预算和稳定 operation key | revision/receipt 或 `scope_conflict / source_incomplete / unauthorized / stale / invalid_horizon / obligations_unresolved / budget_exceeded` |
| `RecordUse/Maintenance` | 原 source effect/quantity、用途、时间、完整未认领证明 | 一次维护/使用摘要；`duplicate_same` 重读，`source_conflict` 拒绝 |
| `Notify/RegisterEvidence/Draw/Vote/Appeal/Finalize` | 原 notice/case/阶段、scope/evidence root、资格/随机证明、每方保留份额、原固定时界 | 有界案件/决定；`wrong_jurisdiction / window_closed / conflict_of_interest / roster_unavailable / outcome_unknown` |
| `ApplyDisposition/ConfirmArrival/PayCompensation` | 原 item、域 owner、position/quantity/condition、原 request/receipt、当前费用/资金 | 实际部分/全部结果；`destination_unavailable / protected_claim / unsupported_asset_effect / insufficient_reserved_funds / original_result_unknown` |
| `InspectOriginalResult` | 原真实主体、operation key、当前读权和 exact lineage | 原 disposition/receipt 或具体 unknown；不换 ID 重试，不额外创建义务 |

所有 ref 为完整 `(world, domain/instance, schema, object_id, revision, commit/digest)`；数量是带 domain/kind/单位的非负整数，时界/计数用 checked arithmetic。空字段、未知 schema、非 canonical 编码、同 ID 不同正文拒绝；名字/裸 hash 不能替身份。原通知和长期历史正文单独受权取回，公开页不泄露他人私有资产/合同细节。

## 7. 状态、事务与持久化

<a id="tenure-persistence"></a>
登记状态、程序状态、物理处置和支付分开：`active/expired/use_suspended/terminated` 不推出 `vacated`；`notice/review/final` 不推出执行完成；每个 item 有真实剩余、receipt 和原请求。当前相关资产/义务清单由所有适用 writer 的完整有界索引证明，不能使用一个静态 list 继续允许后来添入第三方受保护资产。新增义务需检查现有用途/处分限制并更新同根索引。

每个 mutation 使用原 ExecutionTransaction：准备当前 authority、状态版本、空间/义务索引、资源/费用、item 与 receipt 的完整写集，再一次发布。准备失败且确认未提交时无业务效果；同 operation/同 payload 返回原结果，同 key 不同 payload 拒绝。不同 key 仍须按 tenure/item/source/付款 effect identity 防重，避免换 request ID 重复转让/返还。

snapshot/journal 保存同一 instance binding、时界/暂停前缀、维护/使用消费键、原 notice/case、每项实际结果/剩余、资金 holds、原请求和索引 generation。replay 只应用原 canonical 已提交效果，不能重新报价、抽签、评估闲置、搬迁、赔付或发送外部通知。升级先迁移同一语义和 identities，旧 receipt 不能用当前 artifact 重算。

持久化沿 [G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design)：`NotCommitted` 才可维持旧世界可写；`Committed` 安装/恢复原候选，维护失败不撤销；`StatusUnknown` fence 整个不确定旧 World 的 mutator/save/依赖派发，查明完整同代决定并对齐内存/索引后恢复。不是只冻结一个 tenure 然后让其他写覆盖可能已提交的权利。客户端结果未知但 durable world 已明确时只限相应原操作/付款核查，不扩成无依据的全世界停机。

## 8. 部署、安全与运行约束

模块、权限、名册/随机、资产适配器、可信 availability、完整空间/义务 reader 与真实 payer/fee schedule 必须在激活前可验证；缺项保留不可用。运行者或 module publisher 不能自授收回、赔付或公开私有材料的权力。升级/撤权停止新高风险效果，原安全关闭在当前明确受权版本执行，不给已撤销代码无限继续权。

读面按当前 subject/audience 重验，原权限失效不能借历史索引或缓存取他人数据。保留可证明原结果的最小受权入口；分页 token 绑定 reader/scope/cut/schema/到期，后续页不扩大权限。公开范围只含政策、必要程序阶段和有权公开的结果；原材料按直接当事人与有效 panel 任期读取。hash 或 ACL 不宣称密码学保密。

## 9. 质量、容量与可行性

<a id="tenure-budget"></a>
以下是工程 admission 上限，政策可收紧但不能把超限清单截断为“完整”；不等于经过规模实测。

| 维度 | 上限及计入规则 | 超限处理 |
| --- | --- | --- |
| 单 tenure/current state | 64 scope atoms、64 资产/义务项、16 直接权利主体；current state ≤128 KiB | 已接受义务不丢弃；新增承诺前拒绝或要求独立无重叠地点，不拆案规避同权利保护 |
| 单 notice/case | 每主体初审/复核各8原件引用；最多256项，原件正文另存原 journal；6个panel位与固定排列 | 各方/阶段独立槽位；超限不挤掉他人或复核份额，补证替换须保留版本且不变 scope |
| 原读闭包 | 最多256真实对象/证明节点、累计原始+解码≤4 MiB，32次有界读取/1024索引访问；单页32项，deadline≤2秒 | 全部嵌套身份、否定/范围证明、current head重验、fee与asset来源均计数；超限/超时返回incomplete，无World clone/全扫 |
| 模块与输出 | 每迁移≤2次invoke；每次完整输入≤512 KiB、输出≤256 KiB，manifest若更小用更小值；state≤128 KiB | 当前完整源不能塞进预算就拒绝，不免费预热第三次调用 |
| 工作/并发 | 每请求8 MiB工作槽；4并发合计≤32 MiB；包括迟到I/O、输入证明、两次invoke、输出与scratch | 满槽拒绝新比较/变更；原结果查询保留独立2个各1 MiB小槽，只读精确键不全扫 |

代表性上界核算：64资产项×384 bytes=24 KiB；64空间项×128=8 KiB；16主体×256=4 KiB；256证据摘要×96=24 KiB；6投票×512=3 KiB；policy/时界/原请求与索引摘要预留32 KiB，合计95 KiB，小于128 KiB；这些大小是编码准入约束，真实编码超过即拒绝新对象，不能压掉必要字段。原读4 MiB + 两次输入1 MiB + 两次输出0.5 MiB + 请求64 KiB + 1 MiB scratch = 6.5625 MiB，余量覆盖当前state/发送缓冲，仍须实测执行器原固定开销。

费用按当前已治理 metering schedule 对完整声明调用/证明闭包报价，真实 payer 为申请者或收回提案者，不向 panel/被收回者暗扣。每 notice 预留至多 `1通知 + 256证据 + 1冻结抽签 + 6投票 + 16申诉登记 + 1final + 64项各3次处分/确认 + 64付款 + 1关闭 = 538` 个有费用的成功迁移额度，并另保护64次原资产安全核对/关闭额度；每次额度按实际最多2次invoke和原域费用上界计，原物理运输/建造成本另按计划保护，不能以602次覆盖免费世界工作。未使用余额原路释放；费用上调超过已签cap时停止新承诺，保留原已保护安全预算和重新授权路径。无变化/拒绝/同原请求重读不能再扣世界业务费，host读负载沿服务限流计，不借收费清空权利。

## 10. 兼容、迁移与回滚

旧 site、FactoryBuilt、Alliance charter 字符串和旧战争 receipt 不自动转换为 tenure。迁移须逐对象原身份、用途、holder、资产/义务清单、原同意及当前 authority；无法证明就保持原兼容能力，禁止签发新的长期排他权。当前用户资产不会因未迁移被删除或没收。

新 artifact 回滚只关闭其后续准入或按明确迁移恢复当前兼容版本；不能回滚已经发生的物理迁移、付款、合法转让或世界历史。原版本不可继续执行时，安全退出须由当前独立受权适配完成并保留原 keys/receipts；没有安全兼容闭包时阻断新承诺并报告未结，不执行任意旧代码。

## 11. 验证设计与可追溯性

<a id="tenure-verification"></a>
方法见[tenure 独立场景](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)。S0 仅审正文/链接/算式；required/full 须在同一固定实现候选、原世界/激活/参数和真实来源下执行，当前未运行。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [登记模型](#tenure-identity) | 准确地点/权利身份、用途、规划horizon、维护/实际使用与公共通行 | [TN-01](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)；required；固定同一实现候选/原世界、当前授权与真实来源 | 原权利/空间/资产与通行读回 | 不授予资源库存、主权或保证未来收益 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [权利变化](#tenure-acquire-renew-transfer) | 取得、续期、规则化转让当前重验，完整重叠集合CAS且不重复生效 | [TN-02](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 双受让竞态、原/新holder、期限/费用/receipt | 转让不自动出售资产或接替旧债 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [通知与复核](#tenure-notice-review) | 四触发、完整通知、独立review、final guard和最小保全/硬尾部 | [TN-03/TN-04](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 原因/notice/evidence/投票/原final/实际权限 | 他域panel或管理员无本案处分权；不证明真人时长合理 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [实际处分](#tenure-disposition) | 迁移/回收/保护custody逐项真实结算，停止权利不代表腾空 | [TN-05](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 实际位置/量/condition、占地、逐项receipt与剩余 | 不瞬移、不免费回滚、不抹历史损失 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [补偿与原路释放](#tenure-compensation) | 公共必要性先足额保护，净差或明确残余权益交换互斥单次结算 | [TN-06](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#tenure-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 独立逐资源账、回收/补偿消费索引与付款receipt | 不预扣未到账回收，不造无预算赔偿或重复价值 |
| [CC-004](../../product/world-rules-core-gameplay/chartered-conflict-soft-seasons-and-recovery.prd.md#req-wr-cc-004)；Trace refs: O7-REQ-a2b47612-d2c2-4331-811e-58a61366b64d | [冲突权利交接](#tenure-temporary-use) | 原条件接受、真实连续持有/到期后一次正式转移；临时用途独立到期 | [CF-04/TN-07](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#occupation-extraction-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 两instance同根、原/新holder、剩余期限及物理处分 | 比分/图标不代签长期转移，不延原期限 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [原事务与恢复](#tenure-persistence) | 并发/原请求/G1三态/同代恢复保持唯一权利、真实处分和支付 | [CF-09](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#recovery-capacity-cases)；full；固定同一实现候选/原世界、当前授权与真实来源 | 完整写集、root/receipt、费用、恢复与原未知结果 | S0或旧site测试不证明新组合/跨节点通过 |
| [GR-007](../../product/world-rules-core-gameplay/governed-regional-capabilities-and-extensions.prd.md#req-wr-gr-007)；Trace refs: O7-REQ-14aa9ab5-d30d-4be2-8c24-610bdc1d1186 | [有界资源与费用](#tenure-budget) | 当前权限/完整范围读取、256对象/4MiB/两invoke及复核关闭资金全部计入 | [CF-10](../../testing/manual/chartered-conflict-recovery-and-soft-season-acceptance.manual.md#recovery-capacity-cases)；required/full；固定同一实现候选/原世界、当前授权与真实来源 | 边界/超限/迟到I-O/并发与独立预算复算 | 不全扫后截断；cap/算式不是性能实测 |

## 12. 决策、风险与实施结束条件

本设计选择稳定 tenure identity、当前有界空间/义务索引、原政策的有限通知/复核、逐资产真实处分，以及付款与权利/物理结果分账。没有把“到期”简化为删除对象，也没有以持续pending延长地方权力。物理资产无法搬出时实际占地可能长期阻挡相同位置的新建设，这是守恒与独立资产保护的后果，应显示独立替代位置，不能伪称已经腾空。

完整实现仍须提供当前范围授权、L2/原writer接线、完整索引/availability/随机名册、资金hold/付款、安全运输和恢复、三入口同义读面及手册证据。合法政策参数的生产选择和平衡/成本证据由原专业owner按已定规则发布；缺这些实际材料不能成为旁路准入。未知外部资产或没有安全处置adapter的对象不得新登记为可处分范围；复核触发是新资产类型、空间规则、费用/时钟、制度升级或原保护改变。
