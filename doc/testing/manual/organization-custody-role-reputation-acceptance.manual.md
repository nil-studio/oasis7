# 组织托管返还与接替岗位情境咨询验收手册

- 生命周期：`active`；Owner role：`qa_engineer`；协作：`gameplay_designer`、`runtime_engineer`、`agent_engineer`、`viewer_engineer`；Last reviewed：2026-11-04。
- 上游：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)、[AC-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#ac-wr-cr-003)；政策唯一来源：[organization-custody-role-policy-v1](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#organization-custody-role-policy-v1)。
- 专业合同：[组织托管与岗位咨询设计](../../world-runtime/runtime/organization-custody-role-reputation.design.md)。本手册定义方法与独立判据，不表示新 profile、正式入口或真实玩家验证已经通过。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。验收仅覆盖已登记组织的具体托管事项、原生效接替任期及派生咨询校正；一般组织签约、开放岗位和其他 CR-003 义务继续按原范围判断。

## 1. 执行前置与来源资格

每次执行固定 source HEAD、tested tree、实际 build、world/branch 与 authority profile、源政策/adapter、原登记/案件/托管项、控制历史、权威读点和当前读者授权。结果写入实现 PR 与原始测试产物，逐项标明 `pass/fail/blocked/not_run`；本文不维护运行通过台账。

静态走查、受控 adapter/状态夹具、真实源生成与正式消费者分别取证。真实源须沿原组织规则生成原件，不把直接写测试状态或构造 JSON 当成合法登记、交接、返还。业务前置沿[组织连续性验收](organization-continuity-system-acceptance.manual.md#oc-verify-001)至其恢复场景；执行权限与有界环境沿相应实现任务，不在本手册新增发布、模型或生产访问授权。

在固定审读基线 `803e81524e29a988b40f8f642776a2c3c03ab085`，已核到以下具体入口与限制：

| 源或既有测试 | 已能定位的内容 | 本咨询仍需证明的前置 |
| --- | --- | --- |
| [typed commands](../../../crates/oasis7_agent_api/src/world_action/world_rule_organization.rs)、[托管入账](../../../crates/oasis7_world_state/src/world_rules/organization.rs) | `Enable`、`DepositOrganizationEstate`、`SubmitSuccessor`、`CommitHandoff` 与 `SettleEstateAsset` 的真实输入 | 同候选正式入口、原登记/托管/身份与提交证明闭合；命令定义本身不证明已执行 |
| [交接](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_handoff_commands.rs)、[返还](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_remedy_commands.rs) | 有限任期与直接返还分支；完整托管项返还至原 contributor 账本 | 原执行切点的授权、资格、阶段、scope、治理、账本和唯一 effect；当前 `appointed` 不能恢复历史任期 |
| 同一返还文件的 `AdvanceDisposition` | `ExistingContracts` 阶段返回 `organization_contract_inventory_adapter_unavailable` | 直接返还须先有真实可用的原合同清单/阶段 adapter；缺失时该真实路径为 `blocked` |
| [transfer tests](../../../crates/oasis7_runtime/src/world_rules/tests/organization_continuity_transfer_tests.rs) | `active_handoff_executes_only_scoped_custody_return_and_rejects_old_actor_and_expiry`、`appointment_authority_can_return_custody_only_at_the_custody_stage` | 两者含直接构造的状态；后者手设 `Disposition/CustodyReturn`，只可作窄分支测试定位，不能代签正式流程可达 |
| [组织读面](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_types.rs)与[原请求读面](../../../crates/oasis7_world_state/src/world_rules.rs) | 前者按 Agent ID/名单过滤并先 clone；后者仅保留 reader 是 actor 的 requests | 新 profile 的当前身份/读权、历史证明、有界读取和完整覆盖须单验；贡献者看不到执行人原请求，不等于没有返还 |

实现候选须提供真实咨询、翻页、原件查看、客观校正及原请求查询入口，并固定各自实际命令或调用方式。尚无这些入口时先完成可执行的静态检查，相关运行项保持未执行，不发明一个 CLI 后声称正式链路通过。

以下只读定位命令可在仓库根执行；返回定义和测试名只证明源码存在：

```bash
rg -n 'SubmitSuccessor|CommitHandoff|controller_root_id|handoff_expires_at_clock' crates/oasis7_world_state/src/world_rules/organization/organization_continuity_handoff_commands.rs
rg -n 'organization_contract_inventory_adapter_unavailable|scope_includes_asset|ReturnCustody|consumed_effects' crates/oasis7_world_state/src/world_rules/organization/organization_continuity_remedy_commands.rs
rg -n '^fn (active_handoff_executes_only_scoped_custody_return_and_rejects_old_actor_and_expiry|appointment_authority_can_return_custody_only_at_the_custody_stage)' crates/oasis7_runtime/src/world_rules/tests/organization_continuity_transfer_tests.rs
rg -n 'pub fn for_reader|let mut value = self.clone|controller_player_id.clear' crates/oasis7_world_state/src/world_rules/organization/organization_continuity_types.rs
```

## 2. 共同样例与独立对照

使用隔离世界 W 的已登记组织 O、原案件 K 和托管清单。自然主体 P 控制贡献者 Agent `c`，A/B 分别控制接替 Agent `a/b`，Q 控制原任命 authority `q`；另设无关读者 U 和另一托管权利人 R。独立身份与控制来源由原 authority 证明，不能从名字不同推断。下列数量仅为测试数据，按各材料最小单位计算。

| 样例 | 原合法输入与结果 | 咨询的独立预期 |
| --- | --- | --- |
| X | `c` 托管 10 单位 alloy_plate；H1 为 `a` 已生效任期，scope 含 X；在原有效窗口内完整返还 | 组织 O/K 与 H1 均可引用同一结果；`c` 原账本增加 10，只有一份 effect |
| Y | `c` 托管 6 单位 iron_ingot，未在任何 handoff scope；`q` 在合法 `Disposition/CustodyReturn` 阶段完整返还 | 组织 O/K 有直接路径结果；H1/H2 均不取得该岗位履历；前置不可达时仅做 fixture 分支测试 |
| Z | 独立合法源序列中，H1 scope 含 X/Z；X 与 4 单位 alloy_plate 的 Z 均已实际返还；原有权 founder 的 `Reclaim` 经合法 `ResolveRemedy` 仅支持 Z | 组织保留 X/Z 两个实际结果；H1.X 仍当前、H1.Z 转历史，原时点与期限不变 |
| H2 | 在原允许的时点、资格与排序下产生后续新任期，保留 H1 原交接原件 | H1 的 X 结果保留原归属；H2 无 X 的返还业绩 |
| R 的托管项 | 同组织但 U、P/A 等读者没有该项读权 | 不因能查 O/K 或 H1 而取得 R 的材料、数量、控制关联或证据 |

H2 与部分收缩可用不同合法源序列分别验证，不要求从已进入处置的同一快照倒退阶段来凑样例。每个真实样例保存实际命令顺序、输入版本及前置 proof；原资格/清单/治理无法生成时标明具体阻塞，不能在当前 `case` 中改字段充当源事实。

读写前后建立独立对照：原资产/材料账、`organization_estate` 与 continuity asset 的 `disposed/effect_receipt`、`consumed_effects`、原请求/世界 receipt、case revision/manifest、原任期/控制绑定、保护和救济时钟/stay，以及原资格、资源费用、旧 score、治理/claim/工业与机会消费者。只读和拒绝要求这些业务量不变；客观校正仅允许设计规定的派生投影、请求结果和服务诊断变化。

<a id="orgrep01-source"></a>
## ORGREP01：真实来源与两条返还路径

承接 [AC-ORGREP-01](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-01)；依据 [orgrep-source](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-source)。

1. 从真实 `Enable` 与入账原件定位 O/K/X；由合法候选接受与 `CommitHandoff` 取得 H1，再由实际返还提交取得 X 的结果。分别核对世界/登记/案件、托管原量与材料、原 contributor、执行 actor、原时点/任期、治理与完整提交证明。输出每个字段须能回到这些原件或原执行切点，不能从当前余额反推返还。
2. 用已提交账本差分独立复算 X：原量 10、返还 10、原 contributor 账本增加 10、原资产处置一次、`effect_ref`/原请求键/两个资产视图/来源去重一致。组织与岗位同时查看、重建索引、转载和相同请求重试均不增加数量或独立结果数。
3. 在可达的真实原流程生成 Y，核对任命 authority 当时绑定、规定阶段、治理及资产 gate；只归组织直接路径。若 asset 同时在 handoff scope，即使 actor 也属于 appointment authority，也须满足 delegated 路径；不能回退到 direct 规避到期、失权或 stay。
4. 在受控负例中分别替换原登记、world/branch、case、asset、材料、数量、接收者、effect key、执行切点、治理或 commit proof；再提供仅 ACK、`disposed=true`、候选申请、LLM 总结、普通 transfer、其他处分或旧 score。均不得形成已验证返还，拒绝或不完整状态不改变共同对照。
5. 删除一项必需历史原件再读，恢复原件后重读；前者明确来源不完整，后者只恢复原结果。无数据不能推断未返、失败或不可信，恢复不能重新执行返还。

保留实际源生成步骤、原件与输出映射、金额/材料差分、拒绝原因和无效果断言。完整源链只在真实源、证明与正式 adapter 同候选接通后成立；现有 fixture 通过只报告其分支范围。

<a id="orgrep02-subject"></a>
## ORGREP02：组织、原任期与控制主体

承接 [AC-ORGREP-02](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-02)；依据 [orgrep-subject](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-subject)。

1. 在 H1 当前、结束、H2 生效三个读点查询同一 X。组织同案保留 X；岗位记录始终绑定 H1 原生效交接与当时 controller root/player/source/generation，不绑定现在的 `handoff_candidate_id`。仅提交申请、接受或 `appointed` 标记，不能代替 H1 原件。
2. 经合法 authority 将同一 Agent `a` 的控制来源改为 D，再查询 D/当前 `a`、A 的受权历史以及 H1/H2。D 不继承 A/H1 的业绩、个人 credential、一般责任或私有证据；A 的合法历史读权不授予对 `a` 的当前控制或 D 的资料访问。删除历史控制证明时返回归属不完整，不能默认归当前 owner。
3. 用原 authority 认可、保持同一身份与控制来源的密钥轮换作对照；不得仅因公钥字符串改变拆出新主体，或仅因 Agent ID 相同合并两个实际控制来源。
4. 让组织改名、合法变更负责人或形成新登记/新案件，再查旧 X。历史原登记与 case 不变；新名字不清洗旧事实，新登记/案件不自动继承为同一当前履历。Y 的实际直接执行人不因此获得接替岗位成绩。

测试用独立维护的源身份时间线作预期，不调用被测投影自身计算“正确 owner”。字段与时序夹具可验证映射，但真实控制变更、身份恢复和当前授权仍需原入口证据。

<a id="orgrep03-context"></a>
## ORGREP03：当前相关性、部分范围与去重

承接 [AC-ORGREP-03](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-03)；依据 [orgrep-context](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-context)。

| 刺激 | 必须成立的预期 |
| --- | --- |
| X 实际返还后 `disposed=true`、原量已返完 | X 仍是同组织事项和 H1 的已证结果；不能因只查未处置资产而消失 |
| H1 scope 为 X/Z 且两项均已实际返还；原 `kind=Reclaim` 请求经 `ResolveRemedy` 仅支持 Z | 收缩前 H1.X/Z 均当前；收缩后 H1.X 仍当前、H1.Z 为历史，组织 X/Z 仍可查；两项原时点、原任期与期限不变，账本不再次返还 |
| 原任期截止前、等值、后一个源时钟单位 | 按原连续性 C 与权限判断当前/历史；等值不允许原任期继续执行，新咨询不续期或重新计时 |
| H2 生效或 K 被合法关闭/替换 | 原结果保留可读历史及决定来源；组织事实与岗位当前性分别更新，H2 不继承 X |
| 任期关系、源时钟或当前覆盖不可取 | 无法确认当前性；已证历史可按权限另读，不能推定到期、仍有权或未返还 |
| 拖延建索引、清缓存、重连、复制 receipt、校正展示 | 保持原结果时点、原任期和权利期限；不套个人合同 600 tick 窗口或从索引时间起刷新 |
| 改查 case/manifest/托管项/任期或另一 world/branch | 按精确范围查询或拒绝，不把别案结果拼入；无地点证明时不生成“本地经验” |

部分收缩场景须保存 X/Z 两笔真实返还原件、收缩前后源读点、原 `Reclaim` 与仅含 Z 的有效决定。该独立源序列固定原登记的当前 founder 为申请人，不从 P 的 contributor 身份推定 reclaim 资格；裁决者须有当前绑定、原制度的独立性及治理授权，不能是申请人或 H1 接替者。普通 `Appeal` 支持可能只增加 quarantine，不能冒充撤回岗位范围。若查询忽略部分收缩而继续把 H1.Z 显示当前，或将 H1.X 一并移入历史，均失败；仅观察 X 保留不足以判通过。直接构造上述状态只证明映射分支，真实来源证据仍沿第 1 节前置分别取得。

重复 X 通过两个视图、两个读取请求、同源不同转述进入索引，只留一份原来源结果；X 与 Y 材料不同，不将 10 与 6 相加为“16 单位履约”。伪造分拆、同 effect 换 request 或相同 asset 再次返还不得生成新的独立成功。纯 schema/去重负例与真实源的重复拒绝分别报告。

<a id="orgrep04-access"></a>
## ORGREP04：当前读权、分页与覆盖

承接 [AC-ORGREP-04](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-04)；依据 [orgrep-access](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-access)。

以公开读者 U、P 的合法当前身份、A 的原任期受权身份、原制度的现行有权角色、同 Agent ID 的新控制人分别查询同一 case。逐条对照原权限来源与可见字段：公开摘要不扩大披露；权利人只取得自身事项；历史任期读权不恢复执行权；成员、候选或旧名单不自动获得所有案件读权。

| 场景 | 独立判据 |
| --- | --- |
| 同一授权快照完整查验且没有匹配返还 | 只表达该受众/范围/快照下的完整空集，不给正负信用结论 |
| 无权限、源未启用/不支持、历史缺件、覆盖未知、还有下一页 | 分别给出实际限制；不能全部显示为零返还或零失败 |
| 已知隐藏对象与不存在对象由无权读者请求 | 返回同等级的安全拒绝/限制，不通过数量、字段、关联或存在性差异泄露隐藏对象 |
| 页间换读者、撤权、同 Agent 换控制人或 cursor 篡改 world/root/filter/term | 重新核当前授权并拒绝不匹配 continuation；不拼接两个读点或沿旧受众返回正文 |
| 同 root 下只取部分页，或某来源到预算边界停止 | 展示已核页及其覆盖限制；尚无完整集合证明时不能显示完整合计或确认空集 |
| 缓存命中、原件打开、下载/正文读取与校正预览 | 仍按当前读者和实际范围复核，缓存键与主体/授权版本一致；不能复用前任私有 payload |
| P 有 X 的权利，但原返还请求 actor 是 `a/q`，旧 requests 过滤结果为空 | 由合法 source adapter 内部核完原件后，按 P 当前权利提供最小证明与 X 的结果；不能要求 P 冒充 actor，也不能把空过滤结果当完整无返还 |

原件或字段因权限不可见时，源 adapter 可以在合法 authority 下给最小证明与明确覆盖；客户端不能索取完整内部状态再自行遮挡。原 `for_reader` 的返回形状不是以上行为已正确的证据。

<a id="orgrep05-correction"></a>
## ORGREP05：一次客观校正与原救济分流

承接 [AC-ORGREP-05](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-05)；依据 [orgrep-correction](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-correction)。

保持原 World、返还与任期原件正确，只通过实现提供的受控投影故障点，分别制造遗漏 X、重复 X、把 Y 归 H1、把 X 归 H2、陈旧来源版本和错误当前/历史标记。先证明源未变，再以有权主体提交一个原 origin 的完整证据和 expected projection version。唯一可成立的差异原子更新派生投影，并可查原校正结果；源世界 receipt、资产、任期、权利、责任、时钟与费用不变。

| 输入或竞态 | 预期结果 |
| --- | --- |
| 完整输入与投影一致；相同错误/来源/证据重试 | 返回无变化或原已知结果，不另写 revision；换 nonce/编码/引用顺序不制造新工作 |
| 缺原件、仅有声明、错误来源/受众、无法唯一判断 | 一次有界返回限制/无法成立，无部分更新；不登记等待补证的新程序 |
| 原件后来有真实修订或获得新的完整证据 | 在当前授权与预算内重新核对真实新差异，不重开原任期、返还或救济期限 |
| 校正准备后原来源/授权/投影版本变化 | 在发布前重新核验并返回陈旧/权限变化；两个同差异请求至多一个改变投影，不能覆盖新值 |
| 改原托管权、返还金额、任命合法性、欺诈责任或补偿 | 转介可读的原组织对象与实际救济入口，不在咨询层裁决、建 stay 或重新给原程序计时 |

在原救济未登记、受理、期限已过、已结案等情况下打开转介，核对原状态不变、没有新请求或暂停。有权原程序后来实际形成结果时，只消费该结果；点击链接和提交咨询校正都不能显示“申诉已受理”。

校正首次完整输入与响应丢失场景不附加个人合同的 60 tick 窗口。未知结果按 ORGREP06 查回原请求，不用“已超时”否认实际已发布的投影，也不先补写世界 receipt 来追平缓存。

<a id="orgrep06-recovery"></a>
## ORGREP06：原请求恢复、预算与故障切点

承接 [AC-ORGREP-06](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-06)；依据 [orgrep-recovery](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-recovery)与[唯一预算来源](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-budget)。

在同一候选的可控测试环境，分别于 source read 后、当前权限重验前后、projection CAS 前、投影/请求结果决定后但响应前、服务重启恢复时注入故障。每次先固定原请求、来源读点、受众与预期差异，再以原请求查询；测试既覆盖正常 handler，也覆盖实际使用的持久结果与恢复接口。

| 已证情况 | 必须观察的恢复行为 |
| --- | --- |
| 明确未接纳或未发布 | 无投影变动与世界效果；可按实际原因补齐前置，不冒充已受理 |
| 已决定并发布，响应丢失 | 查回原结果与原投影关联；相同请求重发不再校正，更不重新返还 |
| 派生提交决定未能核清 | 关闭受影响派生分区的新校正，保留原身份并查询/恢复；不换 ID 猜测失败、不先发布第二份投影，也不暂停原 World 正常业务 |
| 只读缓存/索引丢失 | 按同源、同政策和可证明授权重建；构建中如实标注覆盖，原时点/任期/历史不变化 |
| 结果已知但读者随后失权 | 恢复接口只返回当前权限允许的状态，不把原成功 payload 泄露给新 owner；失权不撤销历史结果 |
| World generation 为 `StatusUnknown` | 遵循 [G1](../../world-runtime/runtime/runtime-storage-footprint-governance.design.md#storage-generation-g1-design) fence 整个旧 writer；最后已验证的受限历史读不称为当前确认，咨询不能解除 fence 或继续依赖未决源发布 |
| 只在旧服务存储 cut 查不到原请求 | 不据此确认未提交；先 fence 旧 worker，再核清同一已恢复存储 generation 的原结果与全部在途提交，条件成立后才允许原 key 重试；原 World 结果不被改写 |

从 Runtime 唯一预算表取得实际阈值，逐项在 `limit-1 / limit / limit+1` 及组合边界验证：请求字节/引用/说明、单 origin、依赖对象/总解码/单对象、页条数/响应体、每主体与实例并发、原请求恢复保留容量。记录实际计数与停止点，拒绝不截断输入后提交、不释放别人的任务、不以收费或资产 hold 补偿服务资源。

在普通池满额时查询已接纳原请求，验证恢复使用其保留池并受原恢复配额限制；同一原请求的重复查询不生成多个有效 worker。分别注入新请求无法预留存储，以及已接纳后剩余可用空间耗尽：前者不得受理，后者保留原预留空间并能恢复或结束，不能将它借给新请求后再让原结果丢失。活动槽释放不删除原结果或去重关联。

特别覆盖：一页多个完整 origin 合计超过依赖/字节预算；一个 origin 的必需闭包本身超限；同对象反复读取或环形依赖；同一次工作重开 cursor 绕过上限，或把未完整的单 origin 拆页后伪作已验证；授权变化后旧 continuation；新入口禁用但旧结果仍待恢复。合法后继页各自有页面预算，不要求整份查询共用一次上限；一次校正仍不能跨页补证。页上限不是每次必须返回的数量，预算未允许完整核验时保持 partial/受限，不借完整页标签隐瞒遗漏。原请求保留/清理与关闭行为按该设计验证，不能靠丢去重记录恢复容量。

以受控负控制分别让恢复时改用 current owner、把索引时间写成原结果时间、让 projection 发布与可恢复结果脱节；本手册的归属、时序和至多一次断言应能检出错误。合成故障只证明注入范围；实际进程重启、存储失败或跨节点证据按所执行的层级独立报告。

<a id="orgrep07-reading"></a>
## ORGREP07：三入口同义、原消费者隔离与玩家理解

承接 [AC-ORGREP-07](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-07)；依据 [orgrep-consumers](../../world-runtime/runtime/organization-custody-role-reputation.design.md#orgrep-consumers)。

在同一实际候选、读者与读点，对 X、Y、H1→H2、部分 scope 收缩、完整空、受限、来源未知与校正结果未知，分别通过正式 API、Agent 和 Viewer 查询。核对组织/原登记、case/asset、结果/材料/数量、原 actor/任期与路径、当前/历史关系、来源覆盖/限制和实际下一步；文字可以不同，含义和可执行动作一致。API 通过不代签 Viewer，模型复述也不代签源事实；Agent 实现验收按[正式自测手册](agent-runtime-self-playtest.manual.md)取得实际操作证据。

连续查看、翻页、打开原件、预览校正、等待与关闭界面后，独立核对共同业务对照向量。它们均不提交任命、返还、世界校正、费用或 stay。实际完成客观校正后，除允许的派生服务状态外，世界向量仍不变。

在实现候选沿真实调用链列出新投影消费者，以同一源分别提供有效、历史、纠正后与 unknown/partial 视图，核对原候选资格/排序、治理、claim、工业许可、个人 credential、旧 score 和机会消费者不变。原源的新执行动作仍重验实时权限与 gate，不能因为看过正面履历或预览而省略。

**真人方法。** 在目标表面可操作后，首轮选择 4 名代表性参与者：2 名初次接触此类组织委托，2 名有协作/托管经验；覆盖键盘操作与至少一种需要放大或辅助阅读的使用方式。样本用于发现理解阻塞，不估计总体信任或合作成功率；语言、设备及未覆盖能力如实记录。采用隔离的虚构身份和合法样例，不展示他人私有证据。

主持人不先解释正确答案，让参与者完成以下任务：

| 给出的任务 | 观察什么 | 必须避免的引导 |
| --- | --- | --- |
| “核对你这份托管物返到了哪里，再决定下一步。” | 能定位原账本/材料/数量与来源，不推论所有债务已清 | 不先告诉其“返还成功”或代开正确原件 |
| “这位新接替者做过哪些返还？”同时提供 X/H1、H2 与 Y | 能区分组织结果、旧任期与直接执行，不把 X/Y 记给 H2 | 不提示“后任不能继承”替代界面 |
| 比较完整无匹配、权限不足和来源暂缺三份咨询 | 能说清各自能判断什么，并选择真实可行的查看/等待路径 | 不将状态码口头翻译为答案 |
| 对重复 X 申请校正，再对托管权归属提出异议 | 能分别找到客观展示校正与原救济，理解未自动发起世界处分 | 不代玩家选择程序或宣称转介已受理 |
| 校正响应丢失后决定如何继续 | 保留原请求，优先查询其结果；不将超时当失败重复提交 | 不透露后台实际决定来帮助作答 |
| 阅读 H1 历史后解释现在能否执行返还 | 能说明历史事实与当前操作资格分别核验，咨询没有授新权 | 不用主持口头免责声明补产品表达缺失 |

记录实际动作、打开的来源、用时/错误重试/放弃、原话与主持提示，评审推断另列。首次无提示不能完成时记录阻塞；允许一次不含答案的“请查看来源或限制入口”提示，并分别记录有无恢复。仍把组织事实归给后任、把受限/未知当零、把咨询当授权或把超时当未生效，说明该样例需修订后复测，不能用其他参与者成功抵消。相关身份、术语、权限、覆盖、恢复反馈或入口变化后重测受影响任务；真人理解不替代业务、权限和持久结果验收。

## 3. 结果与交接

每项提交固定版本、实际输入与前置、真实执行层级、预期/实际差异、失败处理和证据位置。敏感原件只保留受控引用，公开记录不得暴露其他主体资料。设计/静态方法成立、旧源局部实现、完整新 profile、正式三入口和真人理解分别报告；缺 adapter、历史证明、当前权限或正式入口时保留具体 `blocked/not_run`，交回对应 source/runtime/consumer owner。

本轮文档完成仅交付上述有限方法。CR-003 全项和 GD-13 仍按父项及产品主责跟踪，不以本手册、静态检查或旧分支测试改为整体验收完成。
