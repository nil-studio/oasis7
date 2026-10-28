# 成熟世界小玩家成长合同

- 上层产品映射：本合同承接 `doc/product/world-rules-core-gameplay/mature-world-progression.prd.md` 的成熟世界产品承诺与 `doc/game/prd.md` 的 `PRD-GAME-015`。
- 产品叶子入口：[`REQ-WR-MW-001`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-001)；认可/机会专业叶子另承接 [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004)。本文保留成熟世界 lane、checkpoint、专业化、认可/机会玩家动作、生命周期、失败恢复与专业验收语义。
- 主题 authority：本文件拥有 generic recognition/opportunity 的 mature-world 适用性、事实/审核、作用域/用途/期限、预览/待决/receipt、重验、失效/复核/恢复与 anti-abuse 详细玩法语义，同时保留成熟世界 lane、checkpoint、专业化与恢复选择；不覆盖产品承诺、runtime schema、数值或 Viewer 布局。
- 可变执行状态：对应 GitHub Project task 与 issue evidence comments；当前实现完成度不得由本合同单独宣称。

## 2.9 成熟世界中的小玩家成长线

在 `PostOnboarding` 与首个持续能力之后，产品还必须回答：当世界已经存在更强组织、更深政治和更长历史时，小玩家/新玩家为什么还值得继续玩。

当前答案不应是“立刻加入大组织”，也不应只是“世界本来就很热闹”。正式路线应至少提供 1 条不依赖立即站队的 `small-player lane`，让玩家能在成熟世界里继续形成独立 leverage。

当前冻结的长期推荐轴是：

1. `local operator`：先建立并守住 1 条可恢复的小规模工业或服务能力，完成 1 次对世界有可见后果的阶段成果。
2. `regional specialist`：把这条能力转成短周期、区域性有用的专业化服务，而不是马上跳到全局治理或大型宏系统。
3. `limited-scope regional influence`：通过持续贡献获得有限且可审计的局部优先级、机会、可见度或协调位置，但不直接等价为 global governance 权力。

这些轴允许玩家按当前世界状态改道、重排或回退，不是必须逐级完成的职业树。组织、协议或治理等文明尺度项目只能作为自愿的共同扩展：它们可以形成更大范围的协作后果，但不能取代独立成长、成为唯一有效路线或构成全体玩家的胜利条件。

### 2.9.1 Disruption recovery comparison

当 disruption 阻断 active goal 时，gameplay 必须以同一个目标比较 `repair / rebuild / pivot`，并给出每条路线的时间/阶段成本、资源成本、保留/失去价值、主要风险和推荐理由。推荐应解释它如何最快或最可靠地恢复当前目标的玩家价值，而不是只按最低即时成本排序。

- `repair` 保持现有能力并修补关键缺口；`rebuild` 放弃或暂停旧位置/能力后重建同类能力；`pivot` 将已有投入转换为另一项能服务当前目标的区域用途。三者必须有可感知的恢复节奏或价值保留差异。
- 每项比较必须说明独立 `small-player lane` 是否仍可行。仅当独立路径当前确实不可行，才可推荐外部赞助或 major-power 依赖，并说明阻断约束、该依赖的用途及下次重评时机。
- 代表性验收至少覆盖一次局部停机、资源短缺、据点受压或路线失效：玩家能在同一 active goal 下作出有依据的选择，恢复后仍能回到本地立足、区域专业化或有限区域影响，而不是被静默降级为旁观者或强制站队。
- 本节只定义比较与可玩性验收；不新增 runtime 状态、数值、路径可用性算法或 Viewer 布局。

从 `local operator` 切到 `regional specialist` 之前，系统必须展示 `specialization_entry_quote` / `first_delivery_preview`：玩家要知道候选专业化的第一单交付会满足哪个本地需求、预计产出什么、需要哪些输入、多久形成价值、解锁哪种 `leverage_class`，以及交付后的回访 hook。否则专业化只是抽象标签，不能证明 mature-world 小玩家仍有可判断的经营取舍。

- 首单之后的持续区域交付也必须是可选择、可恢复的循环：玩家比较 `fulfill_next_order`、`reserve_capacity_for_local_need`、`reroute_or_pause_service` 与 `exit_specialization`，并看到当前需求、输入/容量窗口、预期区域价值、承诺期限/退出代价和下一次回访动作。缺料、路线、权限、需求失效或接收方不可用时，不得自动续约、吞没未交付库存或把推荐当成交付；玩家可 repair/rebuild/pivot、持有或重新报价。preview 只读且不授予长期市场资格/排他权；正向只产生一次可追溯交付与需求变化，负向/重连/replay 保持未交付状态。该补充不冻结服务费、质量、容量、合约公式或当前 runtime/Viewer 实现声明。
每个 small-player lane checkpoint 还必须展示 `leverage_checkpoint_summary`：`checkpoint_id`、`previous_leverage_class`、`new_leverage_class`、`new_option_unlocked`、`regional_usefulness_delta`、`recovery_resilience_delta`、`negotiation_position_delta`、`same_loop_repeat_count`、`grind_risk_reason`、`recommended_next_branch`、`leverage_checkpoint_class`。该 summary 需要将结果归类为 `new_option_unlocked / resilience_improved / negotiation_position_improved / regional_usefulness_increased / grind_only`；如果只展示 throughput、库存或同一产线重复执行，而没有新选择、恢复弹性、议价位或区域用途，不能判定为 small-player lane progression。

- 中期新增产能必须先给出 `expand_capacity_preview`：玩家比较 `add_parallel_line`、`upgrade_existing_line`、`stabilize_before_expand` 与 `defer`，并看到当前需求、输入/电力/物流可达性、预期新选择或区域用途、承诺/暴露的机会成本和回退动作。成功收益是打开不同的产能或服务路线；容量、权限、输入或电力不足时不得静默扩张、透支现有产线或伪造吞吐，玩家可修复、降载、等待或改道。preview 只读、不生成免费产出/重复里程碑；正向只形成一次可归因能力变化，负向/重连/replay 保持原产能。详细的已建工厂能力升级、重配置与退役选择见 [工厂能力生命周期合同](./gameplay-industrial-factory-capability-lifecycle-contract.prd.md)，仅在 profile 支持时表达且不改变本行扩容语义；该补充不冻结容量/产率/物流公式、状态机或当前实现声明。
这里所谓 `protected first industrial win`，保护的不是“不会被碰”，而是：

- 早期 footprint 小，不应一开始就与 major-power 主战略面重叠。
- 失败后存在 repair / rebuild / pivot 路径，不会立刻把玩家打回“只能投靠别人或只能退坑”。
- 玩家必须能明确回答“我做了什么、世界因此变了什么、下一步为什么仍值得继续”，而不是只看到世界自己在运转。
- 这条线不能只靠“再多做一点同样的工业”维持；每一阶段都必须新增一个 leverage class，例如更稳的恢复权、更短的交付周期、更有议价能力的局部服务位，或新的区域性选择权。
- 如果继续玩只能得到更高产量、库存、吞吐或重复次数，而没有新的局部用途、恢复弹性、协调位置或选择空间，这条线应判定为 grind-only，而不是 mature-world lane 成立。
- 如果专业化推荐缺少第一单交付预览，应标记 `specialization_delivery_preview_missing`，不得只用 `recovery_operator` / `conversion_specialist` / `regional_service_runner` 标签替代玩家侧收益说明。

这条线与 `PRD-GAME-012` 的 early-retention gate 保持分离；历史短期 focus order 见 [历史实施与验收记录](https://github.com/eng-cc/oasis7/issues/1192#issuecomment-6162409697)：
- `#165` 解决的是“首个持续能力之后如何继续有独立价值”，不是重新改写首个 10 分钟。
- 只有当成熟世界下的小玩家样本能持续给出 `player leverage != world activity only` 的证据时，这条线才算正式成立。

产品承诺见 [`doc/product/world-rules-core-gameplay/mature-world-progression.prd.md`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md)；本节拥有 lane、checkpoint、专业化与 anti-grind 的玩法合同。

Agent-facing 专业执行合同继续消费 `small_player_lane_id`、`leverage_class`、`same_loop_repeat_count`、`grind_only_flag`、`major_power_dependency_status`、`recovery_path_kind`、`requires_major_power_sponsorship` 以及 repair / rebuild / pivot 可用性，并保留 `selected_specialization_id`、`specialization_reason`、`preferred_next_action_class`、`dependency_boundary`、`recovery_escalation_reason` 等可解释摘要；若 guardrail 改写决策，还必须保留 `decision_rewrite` receipt。执行顺序默认先维持 `local_operator`，再按区域需求进入恢复、转换或区域服务专业化；只有玩家自愿升级，或 runtime 明确标记独立路线不可行时，才允许把 major-power dependency 作为有原因的升级路径。当 `same_loop_repeat_count >= 3` 且 leverage 仍是 `throughput_only` / `unclassified` 时，必须停止强化同一循环。

以下情况属于 blocker：独立路径可用时仍把 sponsor / alliance 写成必需；`grind_only` 后继续强化同一 throughput 循环；第一项专业化直接跳到全局治理、联盟领导或战争。没有 bounded canonical trigger 和复查时机时，`wait / wait_ticks` 不能替代 repair / rebuild / pivot。专业化预览缺少本地需求、第一项产出、交付时机或 leverage 解锁时，继续标记 `specialization_delivery_preview_missing`。

<a id="prd-game-015-recognition-opportunity"></a>
## 2.10 成熟世界认可与有限机会专业执行叶子

本叶子承接产品分册 [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004) / [`AC-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004) 与 `MW-11` 的 gameplay 专业执行语义。它只适用于玩家完成首次持续能力后的成熟世界；不把首局结果、starter grant、普通行动成功或历史贡献自动写成认可、资格、优先级、容量或可使用机会。

本合同拥有玩家可操作的认可/机会循环、结果边界、失败恢复和可玩性验收；产品分册仍拥有玩家承诺与适用条件，runtime、Agent、Viewer/API、blockchain 与 QA 保留各自的状态、实现和证据 authority。本叶子不新增 runtime/API/schema 字段，不冻结数值、队列算法或 UI 布局，也不宣称当前实现或发行已就绪。

### 2.10.1 玩家动作与生命周期边界

成熟世界认可/机会循环按以下顺序对玩家可读；当前 `permission/authorization` 是提交与结算之间独立且必须持续有效的条件：

`相称事实或审核决定 -> 认可查看/复核 -> 机会预览 -> 待决提交 -> 有界 hold/排队 -> 当前条件重验 -> 单次 receipt -> 有限使用 -> 失效/复核/恢复`

1. `review_recognition_evidence`：玩家能看到认可所依据的相称世界事实或可复核的审核/治理决定，以及来源、地点/作用域、用途、开始与到期边界和适用的复核/申诉入口。单次行动成功、推荐或历史记录本身不能跳过事实/审核边界。
2. `preview_opportunity`：资格、邀请、推荐和预览只说明可以考虑或尝试的下一步，不产生世界资格、容量、排队顺位、优先级或其他效果。预览必须让玩家知道用途、作用域、期限、主要限制、当前 blocker 和失败后的替代方向。
3. `submit_opportunity`：提交绑定当下可复核且当前有效的 `permission/authorization`、来源、作用域、用途、期限、资格、容量和反滥用条件；在权威结果前保持 `pending`。有界 hold/排队只能表达暂时待决，不等于已经分配、已经成功或获得排他权。
4. `settle_once`：结算前重新读取当前事实与当前 `permission/authorization`；只有一份当前有效 receipt 才能形成一次可使用的有限机会。若提交后权限/授权缺失、被撤销或已过期，该旧 `pending` 必须先失效并转为拒绝、释放或过期之一，不能继续保持可结算状态，也不能产生 receipt 或任何世界效果；结果要说明原因与独立下一步。其他权限仍有效、仅因容量或资格等条件待决的请求，可以按既有规则继续保持 `pending`。receipt 至少能追溯认可来源、地点/作用域、容量单位、期限和实际世界效果，但不冻结字段名或 schema。认可不自动变成资产、OC、治理权、区域控制、全局权力或下一次行动成功。
5. `recover_or_appeal`：容量、资格、期限、来源、当前 `permission/authorization` 或反滥用事实变化时，玩家看到重新报价/拒绝/释放/继续待决的原因和下一步；系统不得自动重提、续期、跨区域携带或用历史认可补签。提交后权限/授权缺失、被撤销或已过期时，旧 `pending` 只能拒绝、释放或过期，不产生 receipt/世界效果并保留历史；若权限后来恢复，必须重新校验当前权限及其他条件并由玩家显式新提交，绝不自动重提。玩家仍可回到独立行动、补证、替代路线、repair/rebuild/pivot、恢复或申诉。
6. `bounded_abuse_review`：由本叶子 generic recognition 派生的认可/机会不可出售、出租、转让、拆分、叠加或由代理/组织代持。该 generic 规则不覆盖 [`FI-002 pioneer priority`](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002) 在其专门范围、条件与 authority 下允许的可转让性，也不重定义 FI-002 的资格、期限或重验条件。伪造、刷取、重复申领、循环背书、付费换取或批量自动化只能按预声明且可复核的审核/处置规则处理；未经审核的怀疑不得直接惩罚。处置必须说明事实类别、当前效果、复核路径和再次取得资格的条件。

情境声誉、pioneer priority 与区域设施容量分别回链 CR-003、FI-002 与 GR-001；本叶子可以消费这些窄范围 authority 的结果，但不得重新定义、扩张或互相转译它们。

<a id="prd-game-015-recognition-opportunity-acceptance"></a>
### 2.10.2 来源到叶子映射与专业验收

| 上游来源 | 本叶子承接的 gameplay obligation | 专业验收入口 |
| --- | --- | --- |
| [`REQ-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#req-wr-mw-004) | 成熟世界适用条件、相称事实/审核、来源/作用域/用途/期限与不越权边界 | [AC-RO-01](#ac-ro-01)、[AC-RO-02](#ac-ro-02)、[AC-RO-07](#ac-ro-07) |
| [`AC-WR-MW-004`](../../product/world-rules-core-gameplay/mature-world-progression.prd.md#ac-wr-mw-004) | 预览/待决/hold、当前条件重验、receipt 单次效果、失效/拒绝/撤销与恢复 | [AC-RO-03](#ac-ro-03)、[AC-RO-04](#ac-ro-04)、[AC-RO-05](#ac-ro-05)、[AC-RO-06](#ac-ro-06) |
| `MW-11` 与 CR-003 / FI-002 / GR-001 窄范围 authority | 跨专业证据保持可追溯；不把情境声誉、pioneer priority 或区域容量扩张为 generic recognition | [AC-RO-07](#ac-ro-07)、[AC-RO-08](#ac-ro-08) |

<a id="ac-ro-01"></a>
- **AC-RO-01 阶段门槛**：样例必须发生在首次持续能力之后的成熟世界；首局、冷启动、starter grant、普通行动 receipt 或环境活动不能单独产生本叶子的认可/机会效果。

<a id="ac-ro-02"></a>
- **AC-RO-02 事实、权限与范围可读**：玩家能读到相称世界事实或可复核审核/治理决定，并能区分当前 `permission/authorization`、来源、地点/作用域、用途、开始时间、到期时间、复核/申诉边界；缺失、撤销或过期的权限/授权不得被推荐、预览或历史认可写成可结算认可。

<a id="ac-ro-03"></a>
- **AC-RO-03 预览与待决不生效**：预览、邀请、推荐和资格不改变资格、容量、排队、优先级或其他世界状态；提交保持待决，有界 hold/排队明确不等于已分配，并给出阻塞原因与下一步。

<a id="ac-ro-04"></a>
- **AC-RO-04 receipt 至多一次与竞争申领**：给定同一成熟世界作用域内一个只能分配一次的 scarce opportunity，以及两个 distinct 且当前 permission/authorization、资格与其他适用条件均有效的 eligible contenders；当两者提交、并发、重连或 retry 竞争该机会时，只有一个当前有效 receipt 可以产生一次可使用机会。获胜者之外的请求必须保持待决或原子拒绝/释放，说明冲突原因与独立下一步；不得产生第二次分配、隐藏优先级、隐藏 charge 或其他第二次世界效果。该规则也适用于 replay，但不改写 FI-002 专门 authority 下的 pioneer priority transfer 条件。

<a id="ac-ro-05"></a>
- **AC-RO-05 结算前重验**：来源、作用域、当前 `permission/authorization`、资格、容量、期限或反滥用事实变化时，系统必须重新校验；若提交后权限/授权缺失、撤销或过期，旧 `pending` 必须失效并拒绝、释放或过期，不得产生 receipt 或世界效果，也不得继续保持可结算状态，且要给出原因与独立下一步。权限后来恢复时，必须再次重验当前权限与其他条件并显式新提交，不得自动重提；其他权限仍有效、仅因容量或资格变化的合法 `pending` 可按既有规则继续待决。不得自动续期、跨区域携带或使用历史认可补签。

<a id="ac-ro-06"></a>
- **AC-RO-06 失效与恢复**：到期、拒绝、暂停、撤销或申诉结果保留历史，停止未来效果，并提供独立行动、补证、替代路线、repair/rebuild/pivot、恢复或复核下一步；失效认可不得封锁独立基线或静默重试。

<a id="ac-ro-07"></a>
- **AC-RO-07 非转让与反滥用**：由本叶子 generic recognition 派生的认可/机会不能出售、出租、转让、拆分、叠加或代理/组织代持；[`FI-002 pioneer priority`](../../product/world-rules-core-gameplay/frontier-expansion-and-world-information-boundaries.prd.md#req-wr-fi-002) 在其专门 authority 下的可转让性及其原有资格、期限、范围和重验条件不被本 generic 规则改写。处置只使用预声明、可复核规则，未经审核的怀疑不直接惩罚，并反馈事实类别、当前效果、复核路径和再获资格条件。CR-003、FI-002、GR-001 的窄范围边界保持不变。

<a id="ac-ro-08"></a>
- **AC-RO-08 证据闸门**：本叶子只定义可玩性和专业合同，不提供当前实现、fresh mature-world sample、跨角色对账或发行 readiness 证据。没有同一候选的 gameplay、runtime、Agent、Viewer、blockchain 与 QA 组合证据时，结论必须保持 `pending/unresolved`，不得公开声称认可/机会闭环已通过。

本叶子的细节验收不改变产品分册的非目标：不新增首局认可或经济旁路，不拍数值，不把 generic recognition 变成治理/资产 authority，也不替代 CR/FI/GR 专题或任何专业实现与验证结论。
