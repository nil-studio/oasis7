# Provider、学习认证与情报连续性

## 文档身份

- 结构裁剪：理由：准入、学习和情报的取舍直接写在玩家边界；范围：长期provider学习与情报产品语义；风险：长期目标不得冒充当前能力，专业状态字段尚由owner裁定；映射：[玩家体验与决策](#2-范围与玩家边界)、[风险与未决边界](#3-权威与冲突处理)

- 所属产品模块：智能体、世界模拟与交互
- 上位产品 PRD：[`prd.md`](prd.md)
- 生命周期：`active`
- Owner role：`producer_system_designer`
- 专业域权威：[`doc/world-simulator/prd.md`](../../world-simulator/prd.md)、[`doc/world-runtime/prd.md`](../../world-runtime/prd.md)、[`doc/testing/prd.md`](../../testing/prd.md)
- Last reviewed：2026-09-13

本文是长期产品分册，定义 provider 选择、准入与固定权威节奏，及 Agent 学习/认证和情报连续性的产品边界。它不定义模型、profile 字段、评测方法或阈值、训练算法、模块 ABI、情报分类技术、action slot 实现、当前 provider 支持矩阵或 readiness verdict。

## 设计适用性与生命周期闭合

- 设计判定：`simple-topic-exemption`（`PRD-only-sufficient`）。
- 历史设计判定 Issue：#3680。
- 设计适用性理由：本 PRD 只定义准入、节奏、暂停、训练和能力范围的产品边界；具体学习交互由专业 authority 决定，独立 design 会复制产品或实现 authority。
- 历史设计判定记录：本次分类见 [Issue #3680 C4 设计判定](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652452993)，本次闭合要求见 [Issue #3680 accepted repair](https://github.com/eng-cc/oasis7/issues/3680#issuecomment-5652870280)。
## 1. 产品目标

玩家可以在获认证且适用的 provider profile 间作出可读选择，但 provider 质量或商业条件不能改写权威世界的节奏、规则或行动容量。Agent 通过可审计的经验、训练、认证和受治理模块形成有来源的能力历史；需要重训时承担真实成本并保留历史。情报在合理私有、必要安全披露和最终公共基线之间平衡，且始终说明新鲜度和不确定性。

这些是长期产品方向，不表示当前 Local Provider parity、社交事实处理或任何 provider 已覆盖认证、试点、训练、情报或公开基线能力。当前支持、默认、范围与结论继续由专业 authority 和根 `README.md` 的 claim envelope 决定。

## 2. 范围与玩家边界

### 2.1 认证 provider 与固定权威 cadence

- 玩家只能从适用于其场景和权限的认证 provider profile 中选择；选择影响 Agent 如何提出意图和可读体验，不改变世界规则、行动权限、资源成本、结算、治理权或因果责任。
- 认证遵循证据优先路径：技术可用性、场景 parity 与安全证据先成立，才可进入受限试点；试点以明确 tier、范围和可审计结果运行，并可在风险、失效或证据变化时暂停或撤销。一次演示、模型声明或局部成功不能直接成为广泛准入。
- 所有 provider 都在同一固定权威 cadence 与 action slots 内提出意图。更快模型、更多调用、付费档位或后台代理不得换取额外世界回合、优先排序、绕过冲突窗口或更高的权威行动频率。

#### 准入生命周期与动作边界

provider profile 的产品基础状态依次为 `证据不足 -> 评估中 -> 受限试点 -> 已准入范围`；`受限试点` 或 `已准入范围` 可因风险、失效或证据变化转入 `已暂停` 或 `已撤销`。只有 `受限试点` 与 `已准入范围` 可以在其已记录的 tier、场景、权限和有效范围内被玩家选择；其余状态不能形成新的权威意图来源。状态名称只表达玩家可见的准入结果，不定义 profile 字段、评估阈值或专业状态机。

`已暂停` 是可逆的风险隔离：它停止该 profile 提交新的意图；尚未产生 committed 世界效果的请求必须按当时有效的范围和前置条件重新评估，只能明确地继续、被拒绝、过期、取消，或经有权主体重新确认后以新请求继续。重试、去重、receipt 查询和是否继续的判定，均由 [`doc/world-runtime/prd.md`](../../world-runtime/prd.md) 的 request identity、anti-replay 与 canonical journal/receipt 合同决定；本文不定义其实现。历史 receipt 只是既有 committed 结果的证据，不是新意图或重新执行的输入。任何重新确认后的新请求仍按正常权威路径进入，不能获得额外 slot 或制造重复世界效果。`已撤销` 不得通过重连、切换 profile、复制请求或历史 receipt 自动恢复或重放。暂停或撤销不会追溯抹去已 committed 的世界结果、责任或 receipt。

恢复必须由新的、适用范围明确的证据结论进入 `受限试点` 或 `已准入范围`；不得自动还原暂停前更宽的范围或权限。每次状态转换应让受影响玩家读到原因、有效范围、当前待决请求的真实结果和可用下一步；该可读性不把本地排队、Agent 记忆或界面提示表达为权威成功。

### 2.2 可审计学习、认证和重训

- Agent 的经验、训练、认证和经治理许可的模块必须保留可审计来源、适用范围、有效状态和后续变化；它们不是不可解释的黑箱 power，也不会因模型切换或组织偏好抹去历史。
- 训练/认证只在适用世界资源、授权、证据与治理边界内形成能力；受治理模块不因上传、购买或外部声称自动取得权威使用资格。
- 重训、替换或降级可用于纠错与适应，但应承担与其影响相称的世界内成本、时间、资格或机会成本，并保留此前训练、认证、撤销与重训的审计历史。重训不能成为洗去失败、责任、风险记录或受限模块来源的快捷方式。

#### 能力有效期、重训窗口与降级边界

训练/认证的历史连续性不等于当前能力仍然有效。对玩家可见的产品结果至少要能区分：当前证据支持的有效范围、正在复核或重训的范围、已降级到较窄范围，以及已失效或被撤销的范围；这些是可读的能力结果，不冻结 profile 字段或专业状态机。

- 复核或重训进行中时，只有专业 authority 明确仍有效的既有范围可以继续被使用；不得借未完成的训练、模型切换、模块上传或历史认证扩大范围、取得额外 action slot，或把原本受限的能力表达为已恢复。若既有范围也无法证明仍有效，相关新行动必须进入 blocked、Wait 或明确的低风险替代路径；低风险替代只能同时满足当前 profile 的 guardrails、合法 catalog 与当前仍有效且预授权的 capability scope，任一条件无法证明时只能 Wait 或拒绝，不得临时生成或扩大替代。
- 认证到期、被撤销、证据失败或重训未通过时，依赖该能力且尚未产生 committed 世界效果的请求必须按当前范围重新评估、拒绝、过期、暂停或要求重新确认；不得静默沿用旧能力、自动迁移到新 profile 或让重连/重试恢复旧权限。即使后续训练/认证成功，原先暂停或待决的 capability-dependent request 也不得静默自动恢复；必须由玩家/owner 在当前有效范围内明确重新确认，或以新请求重新提交并保留与原请求的可读关联。已 committed 的结果、责任与训练/认证 provenance 保持可查询，不因能力失效被追溯抹除。
- 新的训练/认证结果只有在其证据明确支持的范围内恢复或扩大能力；“训练完成”、本地文件存在、provider 可连接或界面显示成功都不能单独证明世界能力已恢复。恢复前后，玩家必须能读到当前范围、证据新鲜度/状态、受影响的行动类别、主要原因与重新评估、等待、降级或重新授权中的适用下一步。

### 2.3 情报的私有期、安全披露与公共基线

- 新获得的情报可在有界私有期内服务发现、竞争或研究；私有期不构成永久信息主权，也不允许持有人隐瞒足以造成即时系统性伤害的必要安全事实。
- 涉及可验证的公共安全、重大风险或受影响者基本恢复能力的最低事实必须按适用授权披露；在私有期、保护期或争议处理结束后，能够公开的知识应形成可访问的公共 baseline。受保护个人资料、敏感漏洞细节、他人私有合同或安全绕过方法不因“公共”而被无界公开。
- Agent 和玩家使用的情报必须表达来源、适用范围、新鲜度和不确定性；过期、冲突或不完整情报应触发刷新、降级、纠正或重排，而不能静默驱动长期、高风险或不可逆行动。

## 3. 权威与冲突处理

| 产品层拥有 | 专业与执行权威 |
| --- | --- |
| provider 准入结果语义、固定 cadence 公平、学习/重训历史和情报私有/安全/公共基线的组合边界 | `doc/world-simulator/prd.md` 拥有 provider、Agent、Local Provider parity、社交事实与玩家 surface 的专业合同；`doc/world-runtime/prd.md` 拥有权威时序、资格、receipt 与执行；`doc/testing/prd.md` 拥有测试与当前 verdict |

本分册不扩大 Local Provider parity、社交事实 adjudication 或产业专题的当前专业真值，也不定义认证算法、训练内容、情报保密级别、时长、披露字段、模块实现、准入状态字段、去重/重放规则或执行节拍。缺少同一候选的专业证据时，以较窄的 blocked、limited 或未承诺边界为准。

## 4. 路线图

1. 可选且有界：让认证 provider 的选择服从证据、试点/分层和暂停/撤销，而非默认授权。
2. 同一世界节奏：使所有 provider 在固定权威 cadence 和 action slots 下竞争与协作。
3. 可追溯学习：把经验、训练、认证、模块和重训接入同一可审计历史。
4. 有效情报：在有限私有、最小安全披露、最终公共 baseline 与 freshness 之间维持可解释平衡。

## 4.1 叶级产品要求与验收

<a id="req-agent-learning-001"></a>
### REQ-AGENT-LEARNING-001：provider 准入必须受范围与固定节奏约束

Trace ID: O7-REQ-86ed9522-e22d-4706-9b1e-3e1452a9fb3c

- 功能点：[REQ-AGENT-LEARNING-001](#req-agent-learning-001)；适用范围沿本条原条件与 [AC-AGENT-LEARNING-001](#ac-agent-learning-001)，不扩大入口或阶段。
- 系统设计状态：未核实
- 系统设计依据：本轮已读本条、对应验收及所列专业 authority；尚未核清“准入生命周期、权限/tier与固定cadence公平”的逐项专业设计承接。产品体验 design 或模块入口本身不作技术覆盖证明；不据此断言无设计。
- 实现状态：未核实
- 实现与验证依据：[bootstrap完整输入与原子安装](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/provider_backed_bootstrap.rs#L19-L80)接受治理finality、identity、grant、invocation与cognition allowance，[catalog消费者](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/capability_catalog.rs#L17-L100)读取当前identity及verified/有效grant；[live撤销](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/capability_authorization.rs#L327-L340)拒绝revoked/superseded。闭合的是治理授权链，尚未核到产品认证证据/tier/试点scope的producer如何生成或限制该输入。[认知调度](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/cognition_scheduler.rs#L635-L676)限制每Agent每tick wake，并不证明认证恢复后的权威action slot公平；[DevLocal生命周期](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/local_test_provider_grant_lifecycle.rs#L18-L36)明确startup-only。本轮未执行验收。
- 核对日期与基线：2026-10-17；需求与静态实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用源码逐文件Git blob与当前完整tree核验；#4865未合并，不使用其候选类型。本轮只读源码/本地产物，未执行模型、Runtime、产品验收或部署；测试源码和历史QA报告不改称本候选PASS。
- 剩余范围：待定位认证profile状态authority及正式选择入口、暂停/撤销向new-intent准入和未committed请求重评估的消费者、恢复新scope的显式重确认及同eligible actor/action slot排序。cognition wake配额、普通grant状态或本地启动续期均不作为该产品认证准入的直接交付。

- 要求：provider profile 只有在适用场景、权限、tier 和证据范围内才可被选择或形成新的权威意图来源；所有 provider 共享同一权威 cadence 与 action slots，不因性能、调用量或付费便利获得额外世界机会。
- 验收：AC-AGENT-LEARNING-001

<a id="ac-agent-learning-001"></a>
### AC-AGENT-LEARNING-001：暂停、撤销与恢复不绕过权威节奏

Trace refs: O7-REQ-86ed9522-e22d-4706-9b1e-3e1452a9fb3c；承接[REQ-AGENT-LEARNING-001](#req-agent-learning-001)。

- 覆盖要求：REQ-AGENT-LEARNING-001
- 场景与结果：证据不足、超出范围、暂停或撤销的 profile 不能提交新的权威意图；恢复只进入新的已证实范围，重新确认后的请求仍按正常 slot 进入，不增加排序优先级或额外世界效果。
- 证据边界：准入状态、slot、公平排序和请求再评估由 Agent/runtime/QA authority 验证；产品层不定义状态字段或阈值。

<a id="req-agent-learning-002"></a>
### REQ-AGENT-LEARNING-002：训练与认证变化必须保留能力范围和历史

Trace ID: O7-REQ-dc313559-e262-478c-8a8d-ee2f9817e5e7

- 功能点：[REQ-AGENT-LEARNING-002](#req-agent-learning-002)；适用范围沿本条原条件与 [AC-AGENT-LEARNING-002](#ac-agent-learning-002)，不扩大入口或阶段。
- 系统设计状态：未核实
- 系统设计依据：本轮已读本条、对应验收及所列专业 authority；尚未核清“训练/认证变化、当前能力与历史及待决重确认”的逐项专业设计承接。产品体验 design 或模块入口本身不作技术覆盖证明；不据此断言无设计。
- 实现状态：未核实
- 实现与验证依据：[授权持久状态字段](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/capability_authorization.rs#L166-L196)与[授权root](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/capability_authorization_state.rs#L71-L115)保存grant/revocation/context/nonce/receipt/effect link；[模块command成本证据](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/cognition_command.rs#L19-L69)绑定host quote及module/version，不是训练成本或认证provenance。[普通命令幂等执行](https://github.com/nil-studio/oasis7/blob/8a9d6b78c6fc94f1676cb29537c7af97665c3d0d/crates/oasis7/src/runtime/world/cognition_command.rs#L74-L145)与live grant消费者已经存在，但训练完成/失败/到期/重训事件→当前scope→旧待决重新确认/关联新请求→玩家历史读取仍未核到。本轮静态审读，不将QA复签或grant verified当作产品认证。
- 核对日期与基线：2026-10-17；需求与静态实现固定main `8a9d6b78c6fc94f1676cb29537c7af97665c3d0d`，引用源码逐文件Git blob与当前完整tree核验；#4865未合并，不使用其候选类型。本轮只读源码/本地产物，未执行模型、Runtime、产品验收或部署；测试源码和历史QA报告不改称本候选PASS。
- 剩余范围：具体未知为训练/认证provenance、成本、模块来源和复核/重训/撤销状态的生产writer与持久权威；相应有效scope及受影响行动reader、成功后不自动恢复旧待决的确认入口、责任/已结算结果保留。普通nonce/receipt历史和治理finality不能代签。

- 要求：训练、认证、模块和重训只能在新证据明确支持的范围内改变能力；复核、失败、到期或撤销期间必须显示当前有效范围与受影响行动，并保留既有 provenance、责任和已结算结果。
- 验收：AC-AGENT-LEARNING-002

<a id="ac-agent-learning-002"></a>
### AC-AGENT-LEARNING-002：重训成功不会自动恢复旧待决请求

Trace refs: O7-REQ-dc313559-e262-478c-8a8d-ee2f9817e5e7；承接[REQ-AGENT-LEARNING-002](#req-agent-learning-002)。

- 覆盖要求：REQ-AGENT-LEARNING-002
- 场景与结果：能力复核/重训/撤销期间，超出当前范围的请求进入 Wait、拒绝、过期或明确低风险替代；后续训练成功也不自动恢复旧待决请求，必须在当前 scope 重新确认或以关联新请求提交，且不产生重复世界效果。
- 证据边界：训练/认证 provenance、scope、guardrail、重训成本和待决请求处理由 Agent/runtime/QA authority 定义；产品层不声称当前能力已实现。

## 5. Done：成功标准与验收

- PL-1：provider 样例证明认证以技术/parity/安全证据为前置，并经历有范围的试点/分层、暂停或撤销；未经证据的模型、一次成功或付费档位不会自动获得世界权威。样例至少覆盖：证据不足或超出试点范围时拒绝选择，暂停后新请求不进入权威节奏且未 committed 请求被重新评估，撤销后旧请求或历史 receipt 不能重放，以及恢复只在新的已证实范围内生效。
- PL-2：不同 provider profile 的样例仅在相同 eligible actor、场景和 slot 输入下比较其权威行动机会；provider 性能、调用量或付费便利不增加世界权力。暂停、撤销、重新确认或恢复也不得借机取得额外 slot、优先排序或跨越冲突窗口；重新确认后的新请求只能按正常 slot 进入。
- PL-3：训练、认证、受治理模块、撤销和重训样例可追溯来源、适用范围、成本和历史；重训不会洗去责任、失败或 provenance。
- PL-4：情报样例区分有界私有期、必要安全披露、可公开的最终 baseline 与不应公开的敏感内容，并在决策时显示 freshness/uncertainty 与刷新或纠正路径。
- PL-5：所有说明将长期目标与当前 Local Provider parity、社交事实或产业专业真值分开；没有新鲜专业证据时不宣称 provider、学习或情报能力已实现或已就绪。
<a id="pl-6"></a>
- PL-6：能力复核、重训、到期、撤销或证据失败期间，Agent 只能在当前仍有效且有证据支持的范围内继续；否则必须进入 Wait、拒绝、过期、暂停或明确合规的低风险替代。待决请求按当前范围重新评估，已结算结果、责任和 provenance 保持可查询；后续训练成功不得自动恢复旧待决请求，必须在当前 scope 重新确认或以关联新请求提交，且不产生重复世界效果、额外 action slot 或隐藏权限。该成功标准是未来产品验收目标，不表示当前能力、provider 支持或 readiness 已成立。

以下为**未来需要补齐的** `test_tier_full` 证据场景，而不是本分支已存在或已通过的测试。每个场景必须记录初始准入状态/范围、操作、权威提交结果、slot 数量与排序，以及 committed 结果：

- `provider_admission_scope_rejection`：证据不足或超出 `受限试点` / `已准入范围` 时，选择或提交被拒绝且无世界效果。
- `provider_pause_rechecks_pending_request`：暂停阻止新提交；既有未 committed 请求按当时范围重新评估，并留下继续、拒绝、过期、取消或重新确认的真实结果。
- `provider_revocation_rejects_old_request_or_receipt`：撤销后旧请求和历史 receipt 不能作为新提交或重新执行输入，既有 committed 结果仍可查询。
- `provider_transition_preserves_slot_fairness`：对相同 eligible actor、场景和 slot 输入，暂停、撤销、恢复或重新确认不改变 slot 数量或排序；新请求仅从正常权威路径进入。
- `capability_retraining_scope_transition`：能力在复核/重训、到期/撤销或失败期间只能按当前有效范围继续，或进入明确 blocked/Wait/低风险替代；guardrail 缺失、catalog 非法、fallback 过期或超出当前有效范围时必须 Wait/拒绝，不产生世界效果或额外 slot，并读出原因与当前有效范围；后续训练成功不得自动恢复旧待决请求，必须当前 scope 明确重新确认或新 linked request，且不产生重复/第二次世界效果；未 committed 请求按当前范围重新评估，已 committed 结果与 provenance 保留；新的训练/认证结果只恢复其证据明确支持的范围。

### 5.1 验收追踪

| 成功标准 | 专业 owner | 专业域 PRD-ID | 权威文档 | 验证证据 | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| PL-1 / PL-2 | producer_system_designer / agent_engineer / runtime_engineer / qa_engineer | PRD-WORLD_SIMULATOR-016 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 未来 full-tier 场景 `provider_admission_scope_rejection`、`provider_pause_rechecks_pending_request`、`provider_revocation_rejects_old_request_or_receipt`、`provider_transition_preserves_slot_fairness`；每例记录初始状态/范围、操作、权威提交结果、slot 数量/排序与 committed 结果 | test_tier_full |
| PL-3 | producer_system_designer / agent_engineer / wasm_platform_engineer / runtime_engineer / qa_engineer | PRD-WORLD_SIMULATOR-016 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 训练/认证/模块/重训的 provenance、成本、撤销和历史连续性证据 | test_tier_full |
| PL-4 | producer_system_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-WORLD_SIMULATOR-016 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | `doc/world-simulator/prd.md`; `doc/world-runtime/prd.md`; `doc/testing/prd.md` | 私有/安全披露/公共 baseline、freshness/uncertainty 和刷新/纠正组合证据 | test_tier_required |
| PL-5 | producer_system_designer / agent_engineer / qa_engineer / liveops_community | PRD-WORLD_SIMULATOR-016 / PRD-TESTING-003 | `README.md`; `doc/world-simulator/prd.md`; `doc/testing/prd.md` | Local Provider/social-fact/industry 现状与长期专题 claim 分离审计 | test_tier_required |
| [PL-6](#pl-6) | producer_system_designer / agent_engineer / runtime_engineer / viewer_engineer / qa_engineer | PRD-WORLD_SIMULATOR-016 / PRD-WORLD_RUNTIME-001 / PRD-TESTING-003 | [`world-simulator PRD`](../../world-simulator/prd.md); [`world-runtime PRD`](../../world-runtime/prd.md); [`testing PRD`](../../testing/prd.md) | 见 [REQ-AGENT-LEARNING-001](#req-agent-learning-001) / [AC-AGENT-LEARNING-001](#ac-agent-learning-001) 与 [REQ-AGENT-LEARNING-002](#req-agent-learning-002) / [AC-AGENT-LEARNING-002](#ac-agent-learning-002) 的未来 full-tier 组合证据：当前有效 scope、guardrail/catalog/fallback 边界、待决再评估、provenance 连续性、slot 公平、重新确认和无重复世界效果；不以局部训练成功或界面显示代签当前 readiness。 | `test_tier_full` |

### 5.2 叶级 owner、authority、evidence 与 test tier 追踪

本表把每个新叶级产品关系导航到专业 owner、权威文档和未来验证证据；它不定义 profile 字段、评测阈值、训练算法、action slot 实现或当前 provider 支持。

| REQ / AC 关系 | 专业 owner 与真实边界 | 专业域 PRD-ID | 专业权威（可导航） | 验证证据（应提供，不预示当前结果） | 测试层级 |
| --- | --- | --- | --- | --- | --- |
| [REQ-AGENT-LEARNING-001](#req-agent-learning-001) / [AC-AGENT-LEARNING-001](#ac-agent-learning-001) | `producer_system_designer`：准入范围、固定 cadence 和公平语义；`agent_engineer`：profile/Agent 意图与状态反馈；`runtime_engineer`：资格、slot、请求再评估与权威提交；`qa_engineer`：准入、暂停、撤销和恢复证据；任何角色都不把局部演示或性能优势提升为额外世界权力。 | `PRD-WORLD_SIMULATOR-016` / `PRD-WORLD_RUNTIME-001/031/033` / `PRD-TESTING-003` | [`world-simulator PRD`](../../world-simulator/prd.md#目标); [`world-runtime PRD`](../../world-runtime/prd.md); [`testing PRD`](../../testing/prd.md) | 未来 full-tier 对相同 eligible actor、场景和 slot 输入覆盖证据不足/超范围拒绝、暂停/撤销阻断、恢复范围和重新确认；对账 slot 数量/排序、无额外世界效果与玩家可读原因。 | `test_tier_full` |
| [REQ-AGENT-LEARNING-002](#req-agent-learning-002) / [AC-AGENT-LEARNING-002](#ac-agent-learning-002) | `producer_system_designer`：能力范围、历史与成本边界；`agent_engineer`：训练/认证/能力 provenance 与 Agent 反馈；`wasm_platform_engineer`：受治理模块的工件/能力边界；`runtime_engineer`：有效 scope、待决请求和 committed receipt；`qa_engineer`：重训、撤销、失败与恢复组合证据；产品层不裁定实现字段或 readiness。 | `PRD-WORLD_SIMULATOR-016` / `PRD-WORLD_RUNTIME-001/031/033` / `PRD-TESTING-003` | [`world-simulator PRD`](../../world-simulator/prd.md#目标); [`world-runtime PRD`](../../world-runtime/prd.md); [`testing PRD`](../../testing/prd.md) | 未来 full-tier 覆盖复核/重训、到期/撤销/失败期间的当前有效 scope、guardrail/catalog/fallback 检查、待决请求重新评估、已结算 provenance 保留、重新确认或关联新请求，以及无重复世界效果/额外 slot。 | `test_tier_full` |

## 6. Non-Goals

- 不选择或实现 provider、profile、模型、评测、训练、认证、模块、情报分类、准入状态字段、去重/重放、cadence/action slots 或玩家 UI。
- 不为 provider、付费服务、训练、模块或情报授予额外世界权力、治理权或行动频率。
- 不把当前 Local Provider parity、社交事实或产业专业合同扩写为本专题能力已实现。
- 不声明任何当前 preview、发布、provider 可用性或 readiness。
