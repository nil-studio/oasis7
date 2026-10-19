# Organization Continuity and Disposition System Design

生命周期：`active`；Owner：`runtime_engineer`；协作 owner：`gameplay_designer`、`blockchain_ops_engineer`、`viewer_engineer`、`qa_engineer`；Last reviewed：2026-10-09。

## 1. 问题、目标与非目标

承接[组织连续性产品 PRD](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md)：组织解散、不活跃保护、受限维护和恢复不能造成越权处分、双重授权、重复资产效果或历史删除。本文定义目标逻辑合同，不发布 wire schema、ABI、时长、资格评分、清算价格或新的治理权限。

固定审读基线：`eng-cc/oasis7` 提交 `ed95dcbe26beb67972f16694c23b5c4c12bc5b88`；该基线用于定位既有文档，具体实现、测试候选与环境由未来实现 PR 固定。本文不声明组织生命周期已实现或可上线。

## 2. 上游约束与相关角色

### 2.1 需求承接与分配表

| 上游 requirement / AC | 具体 obligation | 本设计条款 | 外部 owner / dependency | 排除范围 |
| --- | --- | --- | --- | --- |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | 冻结、合同、托管、债权责任、业务处置、剩余分配顺序及独立权利保护 | [DES-WR-OC-001](#des-wr-oc-001) | game 拥有经济/合同政策；各资产 authority 拥有实际转移 | 不确定价格、债权排序或拍卖机制 |
| [AC-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | 冻结、合同、托管、债权责任、业务处置、剩余分配顺序及独立权利保护 | [DES-WR-OC-001](#des-wr-oc-001) | game 拥有经济/合同政策；各资产 authority 拥有实际转移 | 不确定价格、债权排序或拍卖机制 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-002](#des-wr-oc-002) | game 确定有效活动、期限和资格；P2P 提供权威授权事实 | 不把离线或消息未读认作放弃 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-004](#des-wr-oc-004) | game 确定有效活动、期限和资格；P2P 提供权威授权事实 | 不把离线或消息未读认作放弃 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-002](#des-wr-oc-002) | game 确定有效活动、期限和资格；P2P 提供权威授权事实 | 不把离线或消息未读认作放弃 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | 通知、保护、受限持续与请求层分离；恢复不越级 | [DES-WR-OC-004](#des-wr-oc-004) | game 确定有效活动、期限和资格；P2P 提供权威授权事实 | 不把离线或消息未读认作放弃 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-003](#des-wr-oc-003) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-004](#des-wr-oc-004) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-003](#des-wr-oc-003) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | 重叠范围唯一授权、原子交接、不迁移旧请求 | [DES-WR-OC-004](#des-wr-oc-004) | runtime 拥有执行和 journal；P2P 拥有授权证明；Viewer 只读投影 | 不授予永久所有权或运营旁路 |

消费[确定性执行](../design.md#runtime-deterministic-design)、[待决恢复](../design.md#runtime-pending-design)、[恢复边界](../design.md#runtime-recovery-design)和[存储治理](runtime-storage-footprint-governance.prd.md)。它们分别约束执行、查询/非重放、恢复及 retention，不替代本专题资格与处置规则。

## 3. 当前状态、目标状态与差距

| 对象 | 当前基线 | 目标 | 差距 / owner |
| --- | --- | --- | --- |
| 产品边界 | 基线 PRD 已声明三组 REQ/AC 和 OC-1..7 | 每项有明确技术条款及验证计划 | 本文补齐映射，不是运行证据 |
| 组织执行、存储与 API | 未做完整实现审计，状态 unknown | 处置、授权、义务、资产与 receipt 可联合恢复 | runtime / game 必须冻结 schema 和每种 effect 的合同 |
| 跨节点及玩家表面 | 未验证 | 同候选权威结果与正式表面对账 | P2P / Viewer / QA 负责真实环境验收 |

## 4. 边界与结构

game 产生版本化 charter、有效活动、通知/保护期限、资格和处置政策；治理/P2P 提供有权主体批准、撤销与裁决的权威事实。runtime 在当前世界顺序中重验这些事实，拥有阶段、授权范围、未决义务、请求与 effect journal。资产、合同、托管子系统拥有其资源与最终结算事实，不能被组织状态直接覆盖。

组织成员、Agent 和客户端只提交 intent/证据；签名只能证明来源，不能单独证明处分权。Viewer/API 读取来源版本和 receipt，不通过缓存、管理员编辑或重连制造处置资格。不得把组织资产集合扩大到成员独立资产，或把 Agent 控制变更转换成身份/来源删除。

## 5. 关键运行流程

<a id="des-wr-oc-001"></a>
### DES-WR-OC-001：处置顺序与分步原子提交

1. 按组织身份、处置 case 身份、政策版本、触发事实与范围登记解散/重组 intent；验证治理授权、相关资产 owner、未决义务与保护底线。缺失、冲突或未知时阻断，不能视为无义务。
2. 确认风险冻结，只限制声明范围内的新风险与越权处分；合法合同履行、保护和查询路径仍按各自合同运行。未提交冻结不能宣称已保护。
3. 依次处理合同履行/终止/结清、可识别托管返还、债权/成本/责任、Agent/设施/业务处置，最后处理剩余。每步保存义务身份、处理证明、资产版本、效果量和 receipt；后续步骤必须核验前序全部适用事项已完成或依法终结。不能把仍不可核验的事项归类为不可识别剩余。
4. 长期物流、拍卖、合约结算不是一个跨时间事务。每一步条件提交独立 effect，与前序 receipt 关联；未完成外部义务保持可读待决，不能提前分配其占用资产。外部合同无法支持原子资源变更与 journal 对账时禁用该类处分。
5. 重试同 effect 返回原结果，不能重复返还、转移或剩余分配。处置终结保留稳定组织/Agent 身份、来源、权利与责任记录；未来纠错必须创建关联的新裁决/effect，不改写历史。

<a id="des-wr-oc-002"></a>
### DES-WR-OC-002：不活跃基础阶段与恢复请求

基础阶段是正常经营、可读通知、保护、受限持续、已解决；estate / 可撤销 delegation 是受限持续的容器类型，reclaim/申诉是独立请求层。

| 转移 | 提交前置与结果 | 失败 / 恢复 |
| --- | --- | --- |
| 正常 → 通知 | 当前版本政策确认不活跃事实；记录通知原因、对象、范围和权威时间 | 缺事实或政策拒绝；不改控制/资产 |
| 通知 → 保护 | 满足政策规定的通知与保护前提，仍存在触发事实且有可用主张入口 | 通知未读/不可达不能替代前提；事实纠正、活动恢复或通知失效则停止推进，记录确认后的恢复结果 |
| 保护 → 受限持续 | 保护条件已满足且仍有保全需要；确认有限范围、期限、可撤销授权及未决义务 | 无有效授权或存在阻断性待决裁决保持保护，不以超时自动通过 |
| 受限持续 → 保护 | 授权撤销、到期或前提失败；禁止新受限效果，保留已提交安全动作 | 已确认在途义务按其合同处理，不删除资产 |
| 保护 / 受限持续 → 已解决或适用阶段 | reclaim、申诉、恢复或处置裁决已确认，明确范围和保留义务 | 请求 accepted 不改基础阶段；拒绝/过期/撤回只终止请求 |

期限依据版本化政策和权威世界时间判断，不由客户端时钟或清理任务决定。活动恢复不自动撤销已提交合同或绕过裁决；已解决案件的新事实走新程序。申诉是否限制特定动作由有效政策/裁决明确，不能推导全局停摆，也不能越过产品要求的保护边界。

<a id="des-wr-oc-003"></a>
### DES-WR-OC-003：范围唯一与原子授权交接

授权范围由可比较的权威对象/权利/动作集合表示，不能用客户端自由文本判断重叠；范围解析或集合完整性未知时阻断。每个组织、对象及受限动作的重叠范围最多一个可执行授权；不重叠范围可继续原授权。

交接请求绑定旧授权、旧范围版本、新授权来源、新范围、理由、有效期和预期 case 版本。确认前新授权仅待决。提交时重验新旧授权、时间、资格、裁决、冻结范围及全部相关对象版本；在同一事务中终止/收缩旧范围、激活新范围并记录唯一交接 receipt。冲突或任一写失败时全部不变。部分交接保留未交接范围及同一历史关联，不能创建第二套可执行影子授权。

旧、新授权执行、交接、撤销、reclaim 与申诉均按权威提交序重验：先提交的合法效果保留，后到操作基于更新后的范围与义务判断。交接不授予越过 pending 裁决或资产占用的优先权。旧请求保留原授权绑定，失效范围内拒绝或保持不可执行待决；新主体必须显式重提新请求并关联原请求，不继承权限、排位或效果。

<a id="des-wr-oc-004"></a>
### DES-WR-OC-004：幂等、恢复与结果投影

请求身份在组织/case 与请求者上下文内唯一，内容摘要固定；同身份同内容查询/返回原结果，内容不同拒绝。跨请求 ID 的重复处分还由权威义务/资产来源身份、effect kind、已履行量及活跃占用约束；不能通过换 ID 或授权再次处分同一份价值。真正的新义务必须具有新权威来源，不能仅靠客户端声称。

响应超时意味着结果未知。先查询原请求的权威 journal；未提交可以在原身份且授权仍有效时重试，已提交只返回原 receipt。恢复同时读取 case、基础阶段、政策版本、授权范围/撤销、义务占用、资产状态、请求/effect 与 receipt；缺依赖、日志损坏或来源陈旧时保持 blocked，禁止猜测重放。重连仅查询，不自动重提或将旧请求移到新授权。

投影保留基础阶段与请求状态两个维度，并提供原因、范围、授权来源、保留价值/义务、来源世界版本、receipt 及 authority 支持的下一步。accepted 仅表示请求登记；applied 需要 effect receipt；persisted 需要恢复可回读；finality 与跨节点可用性引用 P2P 合同。未知/陈旧不能显示为恢复完成、资产返还或新 owner 已接管。

## 6. 接口与数据合同

以下是目标逻辑数据，不是已存在字段/API；实现前需冻结类型、大小、错误和版本。

| 接口 | producer → consumer | identity / version | ordering / idempotency | success / error / compatibility |
| --- | --- | --- | --- | --- |
| 政策与治理事实 | game / P2P → runtime | 政策、授权/裁决来源、组织、范围、期限和世界版本 | 当前权威序重验；不可由缓存替代 | 缺项/未知版本/越界拒绝；不默认兼容 |
| 处置/恢复请求 | 玩家 / Agent / API → runtime | case、请求身份/摘要、原授权、新旧关联、证据版本 | 同身份同内容；提交点重验 | 登记或拒绝；超时查询，不换 ID 重放 |
| 资产/义务效果 | runtime ↔ 资产/合同 owner | 来源义务、对象版本、effect、量和前序 receipt | 条件原子提交，累计与占用守恒 | 唯一 receipt 或零变化失败；无联合提交能力禁用 |
| 交接提交 | runtime → journal / 授权状态 | 旧新授权、范围版本、case 与 effect | 同事务收缩旧范围/激活新范围/写 receipt | stale/conflict 原子拒绝；旧请求不迁移 |
| 读面 | 权威状态 → Viewer / API | case、两维状态、来源版本及 receipt | 单调按确认来源更新；恢复只查询 | 未知/陈旧显式标记；旧消费者收窄能力 |

证据不可达、资格拒绝、授权失效、版本不支持、范围冲突、资产/义务阻塞、重复内容冲突与结果未知须可区分。超限在接纳前明确拒绝，不能截断资产/义务集合后继续执行。

## 7. 状态、事务与持久化

基础阶段、请求状态、授权有效性和处置进度分别持久化，不能用一个枚举合并。请求为待验证/已登记/待决/已提交或拒绝/过期/撤回；撤回必须与执行竞争同一权威序，已提交不能被撤回抹除。处置进度以各义务的确认结果判断，不靠游标前进假定结清。

每步 effect 将相关资源与义务变化、case 版本、授权范围变化（适用时）、幂等结果及 receipt 纳入同一提交边界。故障前无提交则全部不变；提交后响应丢失返回原结果。checkpoint 与 journal 一致，重放使用原政策和事实身份，不因新政策重算既有结果。

快照/GC 必须保留恢复依赖、未决请求、已履行账本和可重放/申诉窗口内的去重身份；具体期限由存储与治理政策确定。已解决或授权到期不等于可删除历史。裁决修正保留前序 receipt 与关联的新效果，不能逆写账本。

## 8. 部署、安全与运行约束

嵌入既有 runtime 与权威日志，不新增管理员直接处分入口。治理确认与执行资格分开校验，组织多数、运营工具或有效签名均不能免除资产 owner/合同校验。最低公开投影与敏感申诉证据分离；内部证据按适用权限读取，审计可读不等于无界公开。

依赖故障只阻断相关范围的新效果，查询保留未知语义；无关组织和已确认义务按既有合同继续。限制每请求证据大小、范围对象数量、未决请求数和执行预算；尚无数值时禁用对应新入口，不宣布无限容量或 SLA。操作和恢复步骤留在既有专业 runbook，不由本文授权部署。

## 9. 质量与容量

| 场景 / 环境与刺激 | 响应及指标 | 验证入口 | 当前范围 |
| --- | --- | --- | --- |
| 固定候选 runtime，乱序/重复/换 ID/重连 | 相同权威输入效果一致；重复资产效果为零 | [OC-VERIFY-001](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-001) / [003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003) | 计划，未执行 |
| 授权交接、撤销与恢复并发，提交故障 | 重叠可执行授权最多一个；失败零部分写 | [OC-VERIFY-003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003) | 计划，未执行 |
| checkpoint 恢复、响应丢失、依赖缺失 | committed 可查；未知无新处分；状态/资产/receipt 一致 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004) | 计划，未执行 |
| 最大允许集合及超限、证据超时 | 不截断后执行、不污染无关范围 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004) | 阈值由 runtime / QA 冻结后验证；无性能承诺 |

## 10. 兼容、迁移与回滚

本次不迁移运行态。未来启用前冻结 schema、政策、scope 表达与资源事务适配，核对旧快照、所有读写消费者和恢复测试。旧数据没有本专题事实只能表示未启用/未知，不能被默认转换为可处置资产；缺失授权和义务不是空集合。

仅在兼容消费者就绪后启用新请求。新 schema 写入后，旧二进制不能忽略字段继续执行；禁用入口仍保留历史查询、在途义务及适用救济。回退至能读取已写状态的验证基线；不能回滚已提交转移，只能走新的有权纠错程序。无可兼容基线时停止相关执行并保存状态。

## 11. 验证设计与可追溯性

以下是未来实现的可判断验收计划，文档检查不证明运行通过。每次运行固定 source/tested tree、政策/快照、环境、输入顺序及前后资产/义务/授权/receipt；输出进入实现 PR 和实际测试产物。

### 11.1 验证映射表

| 上游要求与验收 | 本设计条款 | 独立 obligation | 验证方法 / candidate / layer | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [REQ-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | [DES-WR-OC-001](#des-wr-oc-001) | 处置顺序、独立权利与历史 | [OC-VERIFY-001](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-001)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | fixture 前后账本、失败差分、receipt | 外部合同实现、经济平衡 |
| [AC-WR-OC-001](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-001) Trace refs: O7-REQ-366c9e94-4bd6-41f8-93dd-8d87f163f21a | [DES-WR-OC-001](#des-wr-oc-001) | 处置顺序、独立权利与历史 | [OC-VERIFY-001](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-001)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | fixture 前后账本、失败差分、receipt | 外部合同实现、经济平衡 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [DES-WR-OC-002](#des-wr-oc-002) | 阶段/请求层与恢复不越级 | [OC-VERIFY-002](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-002)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [REQ-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [004](#des-wr-oc-004) | 阶段/请求层与恢复不越级 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [DES-WR-OC-002](#des-wr-oc-002) | 阶段/请求层与恢复不越级 | [OC-VERIFY-002](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-002)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [AC-WR-OC-002](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-002) Trace refs: O7-REQ-6e6d890d-c94d-4b43-9947-bfccf2f066e3 | [004](#des-wr-oc-004) | 阶段/请求层与恢复不越级 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 场景断言、恢复对账、真实表面证据 | 通知渠道有效性及上线范围 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [DES-WR-OC-003](#des-wr-oc-003) | 唯一授权、原子交接和非重放 | [OC-VERIFY-003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |
| [REQ-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#req-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [004](#des-wr-oc-004) | 唯一授权、原子交接和非重放 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [DES-WR-OC-003](#des-wr-oc-003) | 唯一授权、原子交接和非重放 | [OC-VERIFY-003](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-003)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |
| [AC-WR-OC-003](../../product/world-rules-core-gameplay/organization-continuity-dissolution-and-dormancy-protection.prd.md#ac-wr-oc-003) Trace refs: O7-REQ-b59a4fa5-9b5f-4119-abf6-7c0f2b4f4268 | [004](#des-wr-oc-004) | 唯一授权、原子交接和非重放 | [OC-VERIFY-004](../../testing/manual/organization-continuity-system-acceptance.manual.md#oc-verify-004)；固定候选，required 状态/读面，full 持久化/跨节点；未执行。 | 顺序矩阵、故障注入、账本/receipt | 尚未实现专用执行与测试 |

## 12. 决策、长期风险与未决问题

| 决策 / 未决项 | owner 与理由 | 解除 / 复核触发 |
| --- | --- | --- |
| 基础阶段与请求层分开 | runtime / Viewer；避免申诉被误读为控制恢复 | schema/API 实现时验证二维投影 |
| 分步处置并保留占用，拒绝跨时间假原子事务 | runtime / game；保障合同与价值连续，可能长期待决 | 每类外部 effect 获联合提交/终止/结算合同后才启用 |
| 对象/权利/动作集合定义重叠 | runtime / P2P；自由文本无法证明唯一授权 | 冻结 scope schema 和部分交接规范，加入竞态测试 |
| 活动判据、期限、通知前提、申诉阻断及资格未定量 | game / P2P；技术设计不自行立法 | 版本化政策确认并由产品 owner 核对保护底线 |
| 保留/隐私、容量及迁移版本未冻结 | runtime / QA；不能以已解决触发 GC | 存储合同、容量预算、兼容矩阵确定并完成 OC-VERIFY-004 |

本文不改变当前权限、共识或持久化实现。未来改变这些边界时，按[开发流程规范](../../engineering/workflow/source-of-truth.md)安排对应能力的独立评审；逻辑设计和计划不能代替该评审、实际验证或发行判断。

## v1 初始政策（owner 授权）

有效活动为组织任一当前成员在权威 AgentCell.last_active 更新；连续 600 tick 无活动可通知，通知 120 tick 后进入保护，保护至少 600 tick。治理授权且仍有保全必要才可进入受限持续；授权最长 600 tick，可撤销。接替者须为现存非 blocked Agent，治理确认不赋予成员独立资产的处分权。reclaim/申诉是独立请求，pending 不激活授权；申诉 pending 阻断新的处分，拒绝不解除基础保护。

estate v1 仅接收组织显式委托的材料，每笔委托绑定贡献者和来源，不包含成员余额、Agent 身份和个人合同。解散依序冻结新风险、核清该 estate 的义务/托管/债权、处理持续业务、返还贡献者，剩余按冻结成员等额分配，整数余数按成员 ID 排序。不能识别或无法联合结算的外部义务保持阻塞，不假定不存在。各阶段要求前序 receipt，并使用独立 effect 来源避免换请求重复分配。

授权 scope 是 estate 对象 ID、权利和动作的有界集合（最多 128 项）；部分交接是明确集合差集，新旧授权在同提交内变更。旧请求保持旧授权版本，不自动迁移。已结清的价值使用原始委托/义务身份账本去重，不能因新授权或新请求重新处分。


v1 使用 `OrganizationCaseV1`、`OrganizationEstateDepositV1`、`OrganizationGrantV1` 和独立 `OrganizationRecoveryRequestV1`，同属 `WorldRulesStateV1`。既有 Alliance 的成员个人合同/身份不会被 case 删除。v1 estate 仅保存新显式委托的材料，不接纳设施、Agent、个人合同或无法联合结算的外部义务；对应未知类型无执行入口，不能把不存在的适配器当成已支持的处置。Business 阶段核清这一材料容器无持续业务，不能据此宣称其他组织形态的清算已实现。Protected 期间的有限保全可按治理 scope 返还已识别托管；剩余分配仍要求解散前序阶段。

Governance proposal 的 description 绑定完整 typed 命令（自身 proposal_key 除外），通过权重至少 3；普通多数或相邻政策提案不能复用。pending appeal 阻断新处分，交接保留旧 grant 及 revision，归还/分配逐 deposit 保存一次 effect receipt。到期不删历史；native/module tick 禁止过期效果并将失去全部有效授权的 Limited 基础阶段退回 Protected。


用户业务动作更新成员活动；系统产生的 case 通知/到期推进不产生用户活动。claim upkeep 的新事件使用 activity_contract_version=1，不刷新 last_active；缺版本的历史 v0 事件保持原重放结果。reclaim/申诉的请求登记 accepted 只更新请求层；经绑定治理裁决 approved 才可改变基础阶段。申诉获准后回到 Protected，不能在保护阶段继续解散；重新产生有限授权仍须满足保护期限与新治理前提。每成员每种请求最多一个 pending，历史结论保留而不占用永久请求配额。


Case 阶段推进与最终关闭有独立的 AdvanceCase / FinalizeCase 对象 scope。托管已全部返还后，申诉裁决仍可进入保护；保护期结束后可通过新的绑定治理提案赋予仅结案授权，不要求虚构未处置资产。显式 case scope 接管时原 founder 在同 scope 的默认执行资格暂停；旧 case revision 的待决操作不迁移到接替者。
