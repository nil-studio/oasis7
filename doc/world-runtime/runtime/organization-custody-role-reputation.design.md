# 组织托管返还与接替岗位情境咨询系统设计

- 生命周期：`active`；Owner role：`runtime_engineer`；协作：`gameplay_designer`、`agent_engineer`、`viewer_engineer`、`qa_engineer`；Last reviewed：2026-11-04。
- 固定审读基线：`803e81524e29a988b40f8f642776a2c3c03ab085`。本文是目标合同，源码定位只证明所述局部实现；未执行业务、恢复、正式入口或玩家验证。
- 上游：[REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)、[组织/岗位咨询政策](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#organization-custody-role-policy-v1)；方法：[验收手册](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md)。

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。只承接组织具体托管事项与原有限接替任期的咨询，CR-003 整体仍为部分覆盖。

## 1. 问题、目标与非目标

组织登记、候选接受、交接与返还已有不同权威记录，但当前字段会随接替覆盖，旧读面也没有把历史主体、当前读权及完整来源组合成情境履历。目标是让有权玩家核对“这笔托管物实际返还了什么、由哪条合法路径完成、是否属于这个原任期”，并在原件充分时校正派生展示。

本 profile 为 `organization_custody_role_reputation_v1`，消费 Gameplay 的 `organization-custody-role-policy-v1`。登记/治理、原托管权利、任期和资产效果仍由[组织连续性](organization-continuity-and-disposition.design.md#des-wr-oc-004)拥有。咨询不写世界分数、资格或资源，不扩大到一般组织签约、开放岗位、任命排序、一般社会声明或未来合作保证。[个人合同 profile](contextual-reputation-evidence-and-review.design.md#reputation-subject)继续保持自己的主体和来源范围。

## 2. 上游约束与相关角色

Gameplay 唯一拥有来源选择、用途、时效与可读政策。Runtime/source adapter 核验原执行事实和历史身份；World Service 承载有界只读投影与缓存校正；Viewer/Agent/API 共同消费同义结果；QA 验证真实原件、失败恢复和玩家理解。

### 2.1 需求承接与分配表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | 组织与原岗位的具体托管返还咨询有真实来源和消费者 | [来源](#orgrep-source) | 原组织/资产 authority、Gameplay | 其他组织/岗位、一般声誉和机会分配 |
| [AC-ORGREP-01](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-01) | 验证组织两条返还路径，岗位只接原 delegated 结果 | [来源](#orgrep-source) | 原登记、命令与提交证明 | ack、fixture 注入及未接通生产者不证明真实返还 |
| [AC-ORGREP-02](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-02) | 稳定组织、原任期、当时 controller 与当前读者分别证明 | [主体](#orgrep-subject) | 身份 authority、历史交接原件 | Agent ID、当前 owner、头衔不代签身份 |
| [AC-ORGREP-03](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-03) | 按事项/原任期/具体 scope 判当前性，成功返还和未撤范围保留 | [情境](#orgrep-context) | 原 case、scope、C 与救济决定 | 无新年龄窗口，不延长原授权 |
| [AC-ORGREP-04](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-04) | 每次按当前受众限权；空集需要同快照完整覆盖 | [读权](#orgrep-access) | 原权利与受众规则、正式读面 | 过滤后缺字段不证明无事实 |
| [AC-ORGREP-05](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-05) | 完整证据只校正派生投影，原权利争议回原程序 | [校正](#orgrep-correction) | 原件读取、服务存储 | 不新增世界 Action、receipt、费用或争议窗口 |
| [AC-ORGREP-06](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-06) | 来源漂移与未知结果按原请求恢复 | [恢复](#orgrep-recovery) | 原世界服务闸门、派生缓存 owner | 本地缓存成功不代签 World commit/finality |
| [AC-ORGREP-06](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-06) | 读取、校正与恢复在完整来源预算内接纳 | [预算](#orgrep-budget) | Runtime/source adapter、QA | 静态预算不代签容量或真实来源可用 |
| [AC-ORGREP-07](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-07) | 三入口区分事实、当前可做动作与未连接的资格消费 | [消费者](#orgrep-consumers) | Viewer、Agent、API | 不改变原权限、价格、治理或工业资格 |

## 3. 当前状态、目标状态与差距

| 对象 | 基线可核事实 | 目标与尚缺部分 |
| --- | --- | --- |
| 登记及托管来源 | [Enable](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_registration_commands.rs) 固定 charter/controller/world binding；[DepositOrganizationEstate](../../../crates/oasis7_world_state/src/world_rules/organization.rs) 扣原 contributor 材料并登记托管项 | 可复用原记录；仍需精确历史读点、来源完整性及有界最小披露适配 |
| 接替任期 | [handoff commands](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_handoff_commands.rs) 冻结候选 controller，CommitHandoff 重验资格并覆盖 case 的现任字段 | 原生效命令及当时状态必须保留；current `appointed` 和现任 scope 不能反推旧结果 |
| 返还与前置 | [remedy commands](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_remedy_commands.rs) 有两种 ReturnCustody 分支，且只把完整 held-in-trust 项记入原 contributor 的 Agent 材料账 | `AdvanceDisposition` 在 `ExistingContracts` 返回 `organization_contract_inventory_adapter_unavailable`，直接返还的正式阶段链未贯通；不能靠改 fixture 的阶段宣称生产者已就绪 |
| 原请求及原子发布 | [WorldRuleReceiptV1 / PreparedWorldRule](../../../crates/oasis7_world_state/src/world_rules.rs) 保存 actor、原 command、提交 tick、result_id，安装组织状态和材料变更 | 本地 prepared/installed 与该环境的持久/正式提交等级仍分别证明；咨询不凭 receipt 字符串认定 finality |
| 旧读取 | [for_reader](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_types.rs) 先完整 clone 再按 Agent ID/名单过滤；外层 requests 仅给原 actor | 不具备本 profile 的历史主体鉴权、有界查询或覆盖证明；contributor 通常不是返还 actor，旧过滤后看不到命令不能解释成未返还 |
| 正式来源及客户端 | [VerifiedWorldView](../../../crates/oasis7_game_client/src/verified_world_view.rs) 区分 service assertion 与独立核验的正式读证明；[projection](../../../crates/oasis7_world_service/src/projection.rs) 有既有受众格式化入口 | 复用证明等级与鉴权边界；新 profile 的受限源读取、版本化投影、缓存校正及三入口组合尚未交付 |

直接/接替两路各自声明真实 source readiness。某路缺前置时保留该路未知或未启用，不以另一条成功代签，也不把这项义务删除。测试定义和合成 World 值可验证局部分支，不能作为真实原件登记进新 profile。

## 4. 边界与结构

| 组件 | 输入、输出和写入职责 | 必须保持的边界 |
| --- | --- | --- |
| 原 World / 组织与材料 authority | 原登记、候选、交接、救济、返还、账本和提交历史 | 本咨询不回写、重放或补造原命令，不更改保护/C/原权利 |
| 注册的 source adapter | 从获准的 exact-cut 内部读取验证原件；输出受版本约束的事实与覆盖 | 整个 source proof 内部可校验不等于整份内容可向读者披露；缺失不猜 |
| 既有 World Service 的派生索引 | 按组织/登记/case/origin/原任期索引；保留源 cut、算法版本和缺口 | 这是可重建缓存，不是第二份 canonical World、独立长期服务或新制度真值 |
| 缓存校正 | 有界确定性重算一个 origin；本地原子保存投影版本和原请求结果 | 无世界 Action、世界 receipt、资源费用或独立裁决程序 |
| 已鉴权 Viewer / API / Agent | 读取允许字段、来源等级、当前/历史与下一步 | 不以索引存在授予操作权；不使用 UI/LLM 重算事实或开放私密原件 |

source adapter 消费[同世界恢复与服务闸门](../design.md#runtime-recovery-design)，正式环境还消费[authority commit](../design.md#runtime-authority-commit-recovery-target)。只读缓存正确性与原世界 finality 是不同事实。派生服务只有其自身的存储提交；不得套用个人合同 profile 的世界纠正写入路径，或增加制度专属 native Action / WorldState 分数字段。

## 5. 关键运行流程

1. 请求精确指定 world/branch、profile/schema、组织登记、case、托管 scope、`organization` 或原 `role_term`，目的固定为核对本事项/任期。过滤条件只收窄；范围外类型直接返回不支持。
2. 当前身份 authority 验证读者与原权利关系，source adapter 捕获可读的不可变源 cut；读取能力、授权或完整性不足时先说明限制，不生成零记录结论。
3. 验证真实来源、原执行路径、历史主体和原件去重；按政策分别派生组织事项与原任期的当前/历史关系。原事实时间与本次索引时间分开。
4. 在工作预算内形成已完整核验的结果前缀，附同 cut 的 cursor/coverage；返回前重新校验当前读权。当前性或覆盖已经漂移时转为明确历史/陈旧或要求重读，不把旧 cut 自动改名为新 root。
5. 玩家可查获准原件、继续分页、等待来源、请求客观校正或进入原救济。每个动作有自己真实入口；打开咨询不提交任命、返还或申诉。
6. 校正先完整验证证据，再对同源投影执行本地 CAS；超时先查原请求。当前 World 变化和服务故障按 §7 处理，不把缓存结果作为世界提交证据。

## 6. 接口与数据合同

以下是目标逻辑合同，不表示当前 wire 已有这些接口或字段。沿现有鉴权读服务与版本协商接入，同一实现适配 Viewer/API/Agent；无 JSON patch 或管理员直接改事实入口。

<a id="orgrep-source"></a>
### 6.1 原件闭包与两种合法路径

Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905。

`CustodySourceV1` 绑定 world/branch、source schema、登记 receipt/charter hash、case 与原 manifest 关系、deposit origin/asset/material/原量、原 actor、完整 ReturnCustody command/result、发生 tick/C、前后验证 cut 与该环境的 commit 证明、原路径/任期、相关后继更正、当前核对 cut 及受众依据。使用原 canonical codec、对象 ID 与请求 key，不通过名称包含词或 trim/casefold 合并身份。

| 原件 | source adapter 必须核对的关系 |
| --- | --- |
| Enable 与原 charter/registration | world binding、组织身份、已生效治理、固定 controller 和政策；未启用旧联盟不可自动填入 |
| Deposit 与 held-in-trust 项 | 原扣款、contributor、材料/最小单位、正数量、asset/source receipt、原登记及 manifest 关系；现在的余额不代替原来源 |
| case 与原执行切点 | 原 case/asset revision、阶段、C/保护截止、治理、来源占用及原执行所需的 quarantine/remedy/其他 gate；不能把 filtered status 当完整验证输入 |
| ReturnCustody 的原命令与实际结果 | `amount == 原项 amount`、唯一 allocation 为原 contributor、effect key 与原请求一致、原 `consumed_effects`/资产 disposed/effect receipt/材料入账的同次关系；请求接受、字符串或单一布尔不足 |
| delegated 的额外原件 | 原 SubmitSuccessor、冻结 controller/接受范围、真实 CommitHandoff 及执行时的 scope/期限/资格；同 tick 采用原提交序 |

`appointment_direct` 需要执行时的原 appointment authority/controller、`Disposition/CustodyReturn` 及原治理/资产 gate 全部成立；它只进入组织事项视图。`delegated_term` 需要 actor 等于原生效接替者、被返还项位于原 handoff scope、当时 `C < expires_at_C`、候选与控制来源仍有效，并处于 `Protected` 或 `Disposition/CustodyReturn`。**只要原执行时的 handoff scope 含该项，就必须成立 delegated 路径**；过期或失权不会使原任命者自动得到 fallback。两路都不免除原治理。

最小完整返还事实的来源键为 `(world/branch, organization_id, registration_receipt, deposit_source_receipt)`，返还原件与后继校正作为该 origin 的结果链。同一原资产跨请求、case、组织视图及岗位视图不产生第二笔返还；不同材料逐项显示。这里证明记入了原记录的 Agent 材料账，不从 Agent ID 推定某自然人已获全部收益或组织其他债务已清。

只有登记的 typed adapter 可以接纳受支持 source schema。它必须读取原受信执行/提交及完整依赖，不能接受客户端自签 bundle、测试侧 state patch、自由 social fact、两次普通 transfer、旧 score 或模型总结。验证器已经核验并 pin 的 exact-cut 来源可以复用，身份/算法/cut 不匹配则重新核验；不能把“读过”作为任意新 root 的证明。

普通受控来源保留 service-assertion 的原等级；正式世界必须取得原正式读取/历史 anchor 与提交证明。扩展最小源投影时要同步原 allowlist、受众 commitment/版本验证和消费者，旧全文 projection 的证明不自动证明新字段。内部缓存哈希、签名服务回复或一个本地 receipt 都不能替代该证明。无可用正式适配器时，正式入口保持 `source_unavailable`，不能静默切到本地世界。

<a id="orgrep-subject"></a>
### 6.2 组织、原任期和控制来源

组织键保留稳定 organization ID、原登记 receipt、world/branch 与 case；新名称、重新登记或现负责人不能重写旧归属。岗位键额外绑定**原已提交 CommitHandoff 原件身份**，不是 candidate ID 或当前 appointed 布尔。原候选接受的 root_agent/player/source/generation、当时执行 actor 与 scope 都在内部证据中保留；披露受 §6.4 约束。

current case 会覆盖 handoff_candidate/scope/expiry，后次交接也会取消前次 appointed。因此岗位归属必须读取当时 committed 前后状态及原 handoff，不能从今天的 map 反推。缺历史闭包显示 `historical_binding_unavailable`，不匿名改归当前 owner。有效密钥轮换与主体变更由原身份 authority 判断；同 Agent ID、同名字、同组织成员均不足。

组织与岗位两种索引引用同一 origin。岗位只包含本原任期的 delegated 结果；直接路径不得添加给任何岗位。接替、续任或控制变更不继承旧个人 credential、私有证据、旧责任或原请求，也不把已发生历史删除。

<a id="orgrep-context"></a>
### 6.3 具体事项、当前性与部分撤回

`CustodyContextV1` 指定原登记/case、托管 manifest/source 范围、查询目的及可选原任期。首版不按地理邻近、组织标签或材料大类扩大匹配；缺真实地点证明不显示本地经验。组织范围与岗位范围分别比较：

- 组织视图按同一原事项及其当前合法关联决定当前性。已成功返还、`disposed=true`、源项无剩余都不让真实结果退出该事项。
- 岗位视图按原 handoff、查询资产 scope 和原权限/期限的当前关系决定当前性。整个原任期结束时变历史；部分 reclaim/revocation 只使被撤部分变历史，未受影响 scope 保留原任期和原截止，不重启期限。
- stay、quarantine 或当前其他 gate 可以阻止新动作，但不抹去已证明的返还。当前操作条件单独展示，不能由“本任期仍相关”推导可执行。

原 case 关闭/有权替换、任期结束或范围移出必须有确切源决定；读取缺失不能当作关闭。历史结果、当前相关性、当前操作条件、source coverage 为四个独立维度。缺 C/后继关系时当前性是 unknown，仍可在当前读权允许时显示“截至该 cut 已验证的历史”。

无新年龄 H 窗口；时间只解释原来源和原授权。晚索引、重建、查询、纠错、换 endpoint 不改变原 tick/C、原 scope、期限或救济。新后任 H2 不接收 H1 的事实；组织同事项仍可引用 H1 的原返还。

具体反例：H1 已合法返还 X、Z，后来原 `Reclaim` 仅获准撤回 Z；组织两笔事实都保留，H1.X 仍按未撤 scope 和原截止判当前，H1.Z 转历史。一般 `Appeal` 支持只增加 quarantine 时，不得误当成同样的任期撤回；实际 scope 变化按原决定核验。

<a id="orgrep-access"></a>
### 6.4 当前读权、完整性与分页

读者身份来自当前已鉴权的主体和受权代理。原 deposit 的受影响主体、原任期主体、现行原制度角色分别用其权利来源与合法身份连续性证明读权；只控制相同 Agent ID、位列旧 recipients/candidates 或持有旧缓存，不证明当前可读。原主体的历史读取也不授予当前 Agent 写权。每次读页、原件打开、校正、原结果恢复与输出前均重验当前读权。

原完整命令/治理/身份材料可以由可信内部 adapter 在原授权边界验证，但返回字段必须取原受众规则与当前请求权限的交集。公开面不新增资产量、contributor、controller、预算或申诉正文；当事人只取本事项允许字段。contributor 不是 ReturnCustody actor 时，提供原规则允许的最小结果证明/定位，不能直接开放整个 actor 命令，也不能依旧 actor-only API 缺 receipt 断言没有返还。没有合法最小披露适配器时返回覆盖受限。

缓存键和 cursor 绑定 exact source cut、profile/adapter/policy、组织/登记/case/scope、原任期、排序位置，以及受众/字段掩码与权限版本。cursor 是不可伪造的不透明服务值，不能承载可被新 owner 直接解码的私密正文；换主体、权限变化或跨 root 续页要求重新鉴权/重读。共享内部事实索引与受众结果缓存分开，服务不把任一读者的完整结果复用给另一人。

| 结果 | 可声明的范围 |
| --- | --- |
| `verified` / `verified_empty` | 本页来源完整；只有同受众/明确 scope 在同 root 的源目录与索引覆盖均闭合，且全部页已读完，才可声明完整空集 |
| `partial` / `page_budget_reached` | 已返回前缀有效，后续尚未完成；无总体数量、成功率或“无其他返还”结论 |
| `access_limited` / `access_denied` | 原规则不允许相应材料；错误不区分无权知道的对象存在性，不泄露隐藏数量 |
| `source_not_enabled` / `unsupported_source` | 指定来源生产者或版本尚未提供；不是空履历 |
| `source_unavailable` / `historical_binding_unavailable` / `coverage_unknown` | 原件、原身份、后继链或当前覆盖不够；不推断失败、无责任或可信 |
| `stale_at_source_cut` / `source_changed` | 只能证明旧 cut，或当前依赖已变化；需明确历史展示或重读 |

完整性来自 source adapter 对原受权范围的完整目录和提交水位核验，不能由缓存行数、hash、filtered clone 或某页结束推导。索引可以有界增量构建，但 gap/超预算条目必须保持可见的覆盖缺口。不能跳过不可读/未核完的记录后继续宣称完整；对无权知道存在性的项只返回不泄露细节的整体覆盖限制。

<a id="orgrep-correction"></a>
### 6.5 一次完整输入的客观校正

| 逻辑接口 | 输入、producer → consumer | 输出与重试边界 |
| --- | --- | --- |
| `read_organization_custody_history` | 当前有权读者 → 原 source/投影；context、view、source cut、cursor | §6.4 的结果、来源等级和下一步；不登记任何世界动作 |
| `preview_organization_custody_correction` | 有权记录主体/当事人 → 同一验证器；一个 origin、错误种类、expected projection、完整证据 refs | 可确定差异、原 cut 与限制；不注册校正或保留业务资源 |
| `correct_organization_custody_projection` | 同上；原 request ID/digest、全部预览输入 | `corrected_at_source_cut / no_change / not_established / source_procedure_required / source_changed / source_unavailable / budget_exceeded`；无等待补证的新案件 |
| `read_organization_custody_correction` | 原结果当前有权读者 → 同一服务的原请求结果 | 原决定及源/投影版本，或 processing/unknown；不重新校正、不扩大旧受众 |

输入只允许登记来源的精确对象引用，不接受任意 URL、文件路径或模型解释充当 proof。错误类别限遗漏/重复 origin、组织/原任期/路径误配、源版本陈旧及当前/历史标记错误；原物权、任命合法性、欺诈、责任和补偿争议返回 `source_procedure_required`，定位原程序与仍适用窗口。转介不登记新申诉/stay，不把已过期窗口复活。

完整输入在 §9 预算内唯一确定正确派生结果后才准备校正。与当前缓存相同返回 `no_change`；证据不支持所述错误为 `not_established`；缺 source、超预算或存在未解歧义时无投影改变，不先写再补证。新完整证据可以重新核对，相同证据换顺序/换 request ID 不构成新事实。

原请求键为 `(service/world/branch, profile, requester_subject, request_id)`；ID 为非空、最多 128 UTF-8 bytes。digest 覆盖 origin、错误、source cut、expected projection、规范化证据及动作；同 key 异内容拒绝。业务校正键固定同 origin/错误、相关源依赖版本、规范化证据摘要与算法版本，旧投影 commitment 用于 CAS；相同材料换 request ID 先查原结果，相同 diff 最多替换一次。原结果重投影给当前读者时仍按受众过滤，不泄漏其他请求者的私有元数据；World root 无关前进本身不产生新的源修订。

一次校正只改变服务的派生索引。`CorrectionResultV1` 保存请求/digest、来源 cut/依赖、旧新投影 commitment、算法、可核对差异、结果及本地提交身份；它是服务元数据，**不是 World receipt 或新世界事实**。无新补证/申诉窗口、费用、资产 hold 或主体处罚。

## 7. 状态、事务与持久化

<a id="orgrep-recovery"></a>
### 7.1 源快照与派生提交分开

原 World 的事实状态不变。派生服务的有限工作状态为 `evaluating → prepared → completed`，已确定拒绝直接结束；响应的 `unknown` 只表示无法确定本地提交，不解释为未生效或世界回滚。没有长期 awaiting-evidence 状态。

源事实证明绑定不可变 cut；本地 CAS 同时检查 expected projection/算法/业务去重键，并原子保存新索引版本、原请求结果和校正关联。源码/存储适配必须证明 durable 提交与重启回读后才返回 `corrected_at_source_cut`；没有该能力时禁用校正写入口，保留正确只读。不能用内存改值、日志字符串或异步写结果假装完成。

本地事务不能与 World 跨库原子保证“当前”。准备前固定原 source 及读权，提交前再核相关源依赖/授权；已发现漂移则无新校正。提交时仍绑定原 cut，不把它重标为后来 World head。响应释放前再次取当前鉴权/覆盖：若 source 已前进，只有相关依赖及覆盖被完整重验才输出当前结论；否则保留原决定、返回陈旧或历史结果。权限丧失时不释放旧私密 payload，即便本地校正已完成。只改缓存不成为延迟撤权或冻结 World 的理由。

每个原请求由单一有效 worker/服务存储 generation 持有，提交前检查其租约；服务故障切换先 fence 旧 worker，再恢复原结果。`not_found_at_cut` 不能排除另一 worker/在途提交；只有在已恢复的同一存储 generation 上核清原请求结果、所有在途提交和旧 worker 已被 fence，才可以声明确定未提交，并允许原 key 重试。若不能做到该查询，返回 unknown 并关闭新校正，不能换服务/nonce 逃避不确定性。

### 7.2 故障、恢复和保留

| 事件 | 必须保留的事实与动作 |
| --- | --- |
| 格式/权限/预算/容量拒绝且尚未提交 | 投影、原件与 World 均不变；明确未受理或无效果，修正原因后可重提 |
| 校正存储 commit 结果未知 | fence 受影响派生分区的新校正，保护候选/旧版本和原 key，查原存储决定；不暂停原世界正常业务 |
| 已提交而回复丢失 | 当前读权成立时返回原结果；不再次增加 projection revision，不再次执行返还 |
| 来源或受众变化 | 保留原切点的内部事实与校正身份，撤去旧当前声明/无权正文，按新源重读；不把同 key 改成新内容 |
| 原 World generation 为 `StatusUnknown` | 沿 [G1](runtime-storage-footprint-governance.design.md#storage-generation-g1-design) fence 整个旧 writer；本服务停止依赖该未决状态的新当前结论/校正，不能绕过正式读取闸门 |
| 旧来源或完整证明丢失 | `source_unavailable/coverage_unknown`；保留可验证历史，缺件不判空、不回填当前 owner |
| profile 停用或实现回退 | 停新校正；继续在权限允许时查询已受理原结果与获准历史，直到确认恢复/安全关闭 |

派生索引可从已验证原件和相同算法重建，不能重放任何业务命令、改变来源时点或虚构以前的校正。恢复闭包包含原请求 digest/结果、旧新投影、业务去重键、来源/算法/受众版本及必要 proof refs；删除缓存不允许删除未恢复请求或重用 key。最小结果和校正关联沿[原存储治理](runtime-storage-footprint-governance.prd.md#storage-profile-history-boundary)保留、可分层归档，活动槽释放不删除它们。原组织历史、权利与责任的 retention 仍由其 authority 决定。

保存或归档容量不足时只关闭新校正接纳并显示可读原因，既有请求按预留空间恢复/结束。已知有误或无法复核的缓存不得因“无法接纳校正”继续标为 verified；可依完整 source 重新只读派生，不能取得正确来源则显示限制。服务元数据损坏不能降级为“从未发生校正”。

## 8. 部署、安全与运行约束

<a id="orgrep-consumers"></a>
### 8.1 同一结果与实际下一步

本设计扩展既有服务与客户端，不增加常驻服务、模型调用或运营裁决者。source adapter 只使用原 world/身份/存储 authority，工作隔离在 §9 配额内；所有输入按 schema/version 严格解析。部署/开启仍由原产品与运维入口完成，设计文档不等于已开启。

三入口至少表达：组织/原登记与具体事项、实际量/材料及允许的接收账本、原时点、直接或 delegated 路径、原任期、当前/历史和覆盖、来源证明等级、原件及可行下一步。允许操作是查原件、缩小 scope、续页、等待来源、校正投影和进入原救济；每次依实际能力返回，不展示不可用的伪成功入口。

“返还已记入原账本”“这是上一任期的结果”“该任期仍有记录，但当前动作条件未核清”“权限或来源限制，不能判断是否存在其他返还”必须能分别呈现。正式客户端拒绝未知版本/受众/证据，旧客户端不能把未知当零；Agent 只消费自身当前授权允许的字段，不把完整内部 proof、前任私有内容发给模型。

新投影不进入 `reputation_scores`、候选资格/排序、治理 vote cap、Agent claim、制造/区域许可、价格或稀缺机会。即使来源都合法且当前，也只形成咨询；是否执行下一次返还、任命或合作仍由原 authority 当前重验。

## 9. 质量与容量

<a id="orgrep-budget"></a>
### 9.1 有界读取与最小路径

以下为工程初值，限制本服务一次工作及增量缓存，不改变原世界、原组织规模或玩法数值；不是当前实现指标。

| 资源 | 首版限制与计量 |
| --- | --- |
| 请求 | canonical 编码 ≤32 KiB；≤16 个输入 receipt/proof refs；说明 ≤2048 UTF-8 bytes；校正只涉及一个 origin |
| 解码与依赖 | 一次读页/校正/原结果恢复 ≤64 个实际依赖对象、累计 ≤4 MiB 解码；单对象 ≤128 KiB；编码声明不可信，流式限量后再解码 |
| 结果页 | ≤32 条完整记录，每条摘要 ≤4 KiB，整个编码响应 ≤256 KiB；原件正文另走当前鉴权的有限 exact read，引用不是绕开总工作量的免费对象 |
| 并发 | 每个服务实例总计64槽：56个普通读/校正槽，8个仅原请求查询/恢复槽；普通每主体≤4、恢复每主体≤2，受托Agent共享主体；同一原请求同时至多一个恢复工作，恢复不占新校正份额 |
| 校正保存 | 接纳前预留此次原请求结果/旧新投影摘要/来源引用/关联的实际编码上界及存储事务开销；业务元数据 ≤32 KiB/次，超限不接纳；未确认结果的保留空间不得借给新请求 |

对象数包括 proof 节点、身份记录、治理材料、覆盖页和其所需祖先；同物料多次读取仍计读/字节开销，不能把无限列表或历史包装为一个 bundle。读源已经安装并验证的 immutable World 视图属于原服务基础；本请求仍须按键有界取值，不能 clone 全 World、全 registration、完整历史或无界遍历其他 case。历史源未准备好时返回未就绪，由原受限同步/索引路径恢复，不在查询里隐式重放全世界。

页上限不是必须凑满32条。每个 origin 要完整核完才进入返回前缀；下一条所需工作超过本页剩余预算时返回 `page_budget_reached` 与同 root cursor。单条本身超过上限则 `source_budget_exceeded`，保留该缺口，不跳过它伪造后续完整性。玩家显式续页是另一份读请求，重新按页预算及当前权限接纳；同一请求不能内部重开 cursor、递归追页或换 key 重置计量。分页 continuation 不作为校正的分批补证；一次校正超限即无效果结束。source builder 同样按这些上限分段推进 watermark，未完成区间不能登记 complete。

**代表性 delegated 最小路径。** 选已真实启用的一个组织、一个 case、一个 held-in-trust 项、一个原接替任期、无额外后继争议，原读取/提交 anchor 已由该环境的真实验证器就绪。以下为可核输入预算，不允许用合成证明替代前置：

| 输入对象组 | 对象数 | 本样例每对象编码/解码预算 |
| --- | --- | --- |
| Enable、Deposit、OpenCase、SubmitSuccessor、CommitHandoff、ReturnCustody 六份原命令/receipt | 6 | ≤64 KiB，包含实际命令与结果绑定 |
| Enable/OpenCase/Handoff/Return 的四份原治理批准 | 4 | ≤64 KiB；额外证书/节点另计 |
| registration、charter、原deposit项、原执行前case、原候选、执行后asset、材料效果证明、consumed-effect映射 | 8 | ≤64 KiB；case/名单必须是能验证所查scope的有限读取，不能藏整表 |
| 原登记者、候选接受、返还actor、contributor的相应历史身份材料 | 4 | ≤64 KiB；缺历史原件不借当前身份 |
| 两份实际Delivered工业订单、候选的实际材料预算、资格/执行所用的原C与Certified availability有界时间读证明 | 4 | 各≤64 KiB；订单须包含原owner/quote.world_binding/delivered_at；时间证明条件见下文 |
| 当前case/所查scope、当前读权、source覆盖、所用索引分区 | 4 | ≤64 KiB |
| 精确历史/当前cut、提交/读取anchor与必要包含/范围证明 | 6 | ≤128 KiB；本样例的全部辅助证明须落在这6项内 |

资格核验复用[eligible_delivery_receipts / candidate_still_qualified](../../../crates/oasis7_world_state/src/world_rules/organization/organization_continuity_helpers.rs)的真实依赖，不能只读两条receipt引用。样例的时间对象必须由原source authority在exact cut核完完整C前缀、两次交付点的Certified区间及registration coverage，输出带原commit/受众绑定的有限读证明；包含证明计入表中最后6项。该适配器尚是实现前置，不存在时应逐项读取原区间/证书并计入实际对象和字节；超限保持来源未就绪，不能自填C或把无界历史包进该对象。增量source构建可按上限分段核原前缀，完成水位前不发布该有限证明；其总构建成本另记，不能当成本次32 KiB请求内免费完成。

该样例总计36对象，`30×64 + 6×128 = 2688 KiB < 4096 KiB`，请求32 KiB、响应256 KiB和一次保存32 KiB合计暂存上界为3008 KiB；落在64对象、4 MiB解码及页/保存限制内。原 actor/登记 identity 相同时可共享，但不以此降低验收输入；实际 source closure 比样例多时计入剩余对象/字节额度，无法闭合则明确不支持该组合。这是静态可行性预算，**不是当前已存在36个对应读取API或已测得成本**；实现须拿实际原件逐项填数，尤其证明树、名单范围读和历史身份不能用 bundle 隐藏。

服务同时满载的解码上界为 `64×4 MiB = 256 MiB`，每槽另预留请求32 KiB、响应256 KiB和业务元数据32 KiB，合计受计量工作缓冲≤276 MiB。编码器/索引/事务引擎开销仍另计实际内存，不能把该算式当进程RSS承诺；部署按实测另设内存与存储预算，超限提前背压。恢复保留池不被普通新工作占满；恢复超出主体/实例限额时即时背压，原请求、未知结果及其持久预留继续保留，不视为无效果。恢复本身也受相同输入/解码限额；不存在无限等待队列或无限自动重试。

### 9.2 验证目标

| 场景 / 环境与刺激 | 预期响应与判据 | 验证入口 | 当前范围 |
| --- | --- | --- | --- |
| 真实源与两路径、同tick/旧任期反例 | 原件/账本一致；直接路径无岗位行，缺正式阶段链不能被fixture代签 | [ORGREP01](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep01-source) | 设计目标，未运行 |
| 多受众、换控制人、部分reclaim、同源多视图 | 无越权披露/归属迁移/重复数量；未撤scope保留 | [ORGREP02](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep02-subject)至[04](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep04-access) | 设计目标，未运行 |
| limit与limit+1、完整页/半页、全并发和存储不足 | 超限无部分投影；缺口不判空；满普通池仍能恢复原请求 | [ORGREP06](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep06-recovery) | 静态预算，实际对象/峰值待测 |
| cache CAS失败/未知、World G1、当前授权变化 | 原结果唯一；缓存与世界故障边界正确；失权不发旧payload | [ORGREP05](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep05-correction)、[06](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep06-recovery) | 需真实存储故障和消费者证据 |
| 同候选Viewer/API/Agent及目标玩家 | 能区分原事实、当前条件、未连接资格，不将参考当保证 | [ORGREP07](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep07-reading) | 需实际入口/玩家证据 |

不新增未经依据的延迟/吞吐SLO。后续实现记录源对象数量/字节、访存、并发、实际存储与响应分布，按所测环境判断容量；一个fixture或算式不外推大世界吞吐。

## 10. 兼容、迁移与回滚

启用顺序是：真实source生产者与历史依赖 → 当前身份/最小披露及有界source读取 → 版本化派生验证/分页与覆盖 → 本地原请求恢复/CAS → 同版本三入口。每路独立核其生产者前置，缺项保留明确未启用，不从旧score、名称或空serde字段迁入。

旧个人合同profile、组织状态和World请求语义保持；本profile以新能力版本显式协商。旧客户端不懂组织/任期/coverage时关闭本咨询入口，不退回固定positive或actor-only status拼接。适配器改版需证明对同一原件产生同义结果或走显式缓存校正，不重写旧事实、任期或受众。

索引丢失可以从合法source重建，原请求结果/校正关联不能假装未发生。停用新请求仍保留必要原结果读取；回退到能读已保存元数据的版本，旧二进制不认识新记录时只读/隔离，不能删除后继续写。由于没有世界资产副作用，回滚不执行资产补偿；原有组织救济和世界恢复继续按其合同。

## 11. 验证设计与可追溯性

运行固定source/tested tree、profile/政策/adapter、world/branch、原件与受众版本。静态审读和文档检查不证明 source 可生产、API 已实现、正式提交、吞吐或玩家理解；当前业务/恢复/三入口场景未执行。

### 11.1 验证映射表

| 上游 requirement / product AC / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment 要求或选择规则 | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-CR-003](../../product/world-rules-core-gameplay/communication-contracts-reputation-and-rd-continuity.prd.md#req-wr-cr-003)；Trace refs: O7-REQ-5197cd23-8409-4cea-860a-96d2048c7905 | [来源](#orgrep-source) | 本事项/原岗位事实咨询可由真实来源消费 | [ORGREP01](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep01-source)；同候选正式源，fixture/真实入口分记 | 原件/账本/consumer对账 | CR-003其他情境与通用声誉 |
| [AC-ORGREP-01](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-01) | [来源](#orgrep-source) | 两路径来源齐全、直接不归岗位 | [ORGREP01](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep01-source)；实际adapter、错误world/路径/asset负例 | 原请求、提交证明、受限读面 | 直接路前置缺失不得记通过 |
| [AC-ORGREP-02](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-02) | [主体](#orgrep-subject) | 历史任期/控制绑定不可被后任继承 | [ORGREP02](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep02-subject)；换主/续任/旧字段覆盖 | 原handoff/身份/两读者差异 | 一般角色和组织签约 |
| [AC-ORGREP-03](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-03) | [情境](#orgrep-context) | disposed成功保留、部分撤范围分别判、原截止不重启 | [ORGREP03](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep03-context)；原C边界与部分reclaim | scope/时间/历史结果对照 | 玩法平衡、年龄评分 |
| [AC-ORGREP-04](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-04) | [读权](#orgrep-access) | 当前受众与同root完整性成立，错误不泄露隐藏项 | [ORGREP04](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep04-access)；public/当事人/后任、cursor与授权漂移 | 允许/拒绝字段与coverage | 旧filtered clone不是新证明 |
| [AC-ORGREP-05](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-05) | [校正](#orgrep-correction) | 唯一客观差异只改派生状态，实质争议转原程序 | [ORGREP05](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep05-correction)；完整/不足/重试/并发 | 原请求、本地前后投影、World零差分 | 原权利合法性不在本层裁决 |
| [AC-ORGREP-06](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-06) | [恢复](#orgrep-recovery) | 原结果/租约/持久化未知恢复，World G1不被绕开 | [ORGREP06](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep06-recovery)；真实存储故障和重启 | 提交决定、原key、恢复结果 | 本地恢复不代签World服务闸门 |
| [AC-ORGREP-06](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-06) | [预算](#orgrep-budget) | 完整source工作量、分页和恢复保留容量闭合 | [ORGREP06](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep06-recovery)；边界输入、满池、资格源与实际proof展开 | 实际对象/字节、限额拒绝及保留池 | 静态算式不证明RSS或生产容量 |
| [AC-ORGREP-07](../../game/gameplay/gameplay-organization-custody-role-reputation-contract.prd.md#ac-orgrep-07) | [消费者](#orgrep-consumers) | 三入口同义、当前可做动作独立、无资格写入 | [ORGREP07](../../testing/manual/organization-custody-role-reputation-acceptance.manual.md#orgrep07-reading)；实际客户端/Runtime与目标玩家 | 受众一致结果、操作/玩家理解记录 | mock/API不能代签UI和真人 |

## 12. 决策、长期风险与未决问题

| 选择与依据 | owner / 后果 / 复核触发 |
| --- | --- |
| 两条组织路径、岗位限原delegated任期 | Gameplay/Runtime；覆盖原合同的真实事项，不把组织结果变个人业绩；新增操作/岗位需先有相应来源与权限合同 |
| 事件/context决定当前性，无新增年龄窗口 | Gameplay；核对具体事项无需借个人信用寿命；原政策改变时只更新受影响解释，不追溯清洗历史 |
| source正确性与响应时当前权限分开 | Runtime/安全；避免跨存储假原子与旧数据泄漏；新的正式projection/历史读取必须独立核验 |
| 缓存校正不进入World、不设新案件 | Runtime；保持源权威和资源守恒，同时必须实现本地原请求持久恢复；存储不支持时关闭写入口，不能假成功 |
| 有限source闭包、字段与分页 | Runtime/QA；大名单/证明超限保持明确缺口；代表性真实原件、实际成本未验证，扩容需版本化及同消费者验证 |
| 直接路径正式阶段链未齐、历史身份/最小受众源仍需接通 | 原组织/身份/World Service owner；本设计保留目标与准入条件，实际adapter和ORGREP验收成立后才可记已实现；不以fixture或设计检查替代 |

本批交付设计与方法，不改变运行实现。实际治理激活、来源生产、读取适配、持久化故障、正式消费者和玩家理解仍须在实现候选上取得证据；保留这些缺口不表示把当前设计义务转交为无主TODO。
