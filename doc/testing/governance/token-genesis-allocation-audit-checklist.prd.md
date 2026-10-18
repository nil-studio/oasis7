# oasis7 主链 Token 创世分配审计清单

- 对应设计文档: `doc/testing/governance/token-genesis-allocation-audit-checklist.design.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

状态: `active`；首次真实控制主体冻结前仍须执行独立的正式审计。

## 1. Executive Summary
- Problem Statement: oasis7 已经冻结了 Token 初始分配比例与创世参数草案，但如果没有 QA 审计清单，团队很容易把 `custody account`、`treasury bucket`、个人直持比例和首年流通边界混在一起，导致创世配置带病进入执行。
- Proposed Solution: 建立一份 `qa_engineer` owned 的创世分配审计清单，统一审计字段、阻断条件、证据格式与 verdict 口径，让 producer/runtime 在真正冻结创世前必须先过 QA 门禁。
- Success Criteria:
  - SC-1: checklist 明确覆盖 `sum=10000 bps`、bucket 完整性、recipient 非空、个人直持上限、`genesis_liquid=0` 与首年外部释放上限。
  - SC-2: checklist 明确区分 `custody account` 与 `treasury bucket`，避免语义误读。
  - SC-3: checklist 模板可直接填入真实创世参数并输出 `pass/block` verdict。
  - SC-4: 专题纳入 `testing` 模块与 `p2p token` 项目追踪，满足可追溯性。

<a name="2-user-experience-functionality"></a>

## 2. User Experience & Functionality
- User Personas:
  - `qa_engineer`：需要一份可重复执行的创世配置审计清单。
  - `producer_system_designer`：需要知道当前 Token 创世是否可冻结，还是必须继续挡回去。
  - `runtime_engineer`：需要把参数表映射到 runtime 真实现时，有一份外部 QA 门禁作为交叉检查。
- User Scenarios & Frequency:
  - 创世配置草案形成后执行一次 required-tier 审计。
  - 任何比例、recipient、vesting 或释放策略变化后重新执行一次。
  - 正式 mint 前执行最终审计并归档 verdict。
- User Stories:
  - PRD-TESTING-TGA-001: As a `qa_engineer`, I want a fixed audit checklist, so that token genesis review does not depend on memory or chat context.
  - PRD-TESTING-TGA-002: As a `producer_system_designer`, I want explicit block conditions, so that I know exactly when genesis must be held.
  - PRD-TESTING-TGA-003: As a `runtime_engineer`, I want custody-vs-treasury semantics spelled out, so that implementation does not drift from product claims.
- Critical User Flows:
  1. Flow-TGA-001: `读取 TIGR-1 参数表 -> 填入审计模板 -> 逐项检查比例/recipient/vesting -> 输出 verdict`
  2. Flow-TGA-002: `发现个人上限/流通边界/custody 语义错误 -> 标记 block -> 回流 producer/runtime 修正`
  3. Flow-TGA-003: `创世前最终复查 -> 归档 checklist + verdict + evidence path -> 决定是否允许执行 mint`
- Functional Specification Matrix:
| 功能点 | 字段定义 | 动作行为 | 状态转换 | 计算规则 | 权限逻辑 |
| --- | --- | --- | --- | --- | --- |
| 分配表完整性审计 | `bucket_id`、`ratio_bps`、`recipient`、`start_epoch`、`cliff_epochs`、`linear_unlock_epochs`、`genesis_liquid` | 逐行核对创世参数表是否完整 | `draft -> checked -> pass/block` | `sum(ratio_bps)=10000`；关键字段不可空 | QA 独立审计 |
| 控盘边界审计 | `project_control_bps`、`protocol_reserve_bps`、`founder_direct_bps` | 汇总比例并核对是否越界 | `unchecked -> pass/block` | 项目战略控制 `5000 bps`；协议长期储备 `3500 bps`；个人上限 `<=1500 bps` | QA 出具结论，producer 决定是否冻结 |
| 流通边界审计 | `genesis_liquid_bps`、`year1_external_release_bps` | 核对创世与首年释放边界 | `unchecked -> pass/block` | `genesis_liquid=0`；首年外部释放硬上限 `500 bps` | QA 阻断权 |
| 语义边界审计 | `recipient_type`、`custody_vs_treasury_note`、`reward_framing` | 检查是否把 custody 误写成 treasury，是否把贡献奖励误写成 P2E | `unchecked -> aligned/block` | 命中禁语或语义错位即 `block` | QA 独立判定 |
- Acceptance Criteria:
  - AC-1: checklist 至少包含 `参数表完整性 / 控盘边界 / 流通边界 / 语义边界 / verdict` 五个审计区块。
  - AC-2: checklist 明确以下硬阻断条件：
    - `sum(allocation_bps) != 10000`
    - 任一自然人直接受益 `> 1500 bps`
    - 任一 bucket `genesis_liquid != 0`
    - 首年外部释放计划 `> 500 bps`
    - 将 `custody account` 误写成已初始化 treasury bucket
    - 使用 `play-to-earn`、`login reward`、`time played = token` 叙事
  - AC-3: checklist 模板可以直接填入 `TIGR-1` 参数表并给出 `pass/block` verdict。
  - AC-4: 专题必须与 `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md` 互链。
- Non-Goals:
  - 本专题不决定最终总供应量绝对值。
  - 不取代法律/税务/证券意见。
  - 不直接执行 mint，也不负责发放 early contributor reward。

## 3. AI System Requirements (If Applicable)
- Tool Requirements: 不适用。
- Evaluation Strategy: 不适用。

## 4. Technical Specifications
- Architecture Overview: 该 checklist 作为 QA 层 required-tier 审计入口，读取 `p2p token` 侧的创世参数表草案，按固定字段输出 `pass/block` verdict，并沉淀到 testing 证据体系中。
- Integration Points:
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
  - `crates/oasis7_runtime/src/main_token.rs`
  - `crates/oasis7_runtime/src/world/event_processing.rs`
  - `doc/testing/evidence/token-genesis-allocation-audit-template.md`
- Edge Cases & Error Handling:
  - 若参数表暂时没有真实多签地址，只给逻辑账户名，允许 `conditional draft review`，但不得给最终 `pass`。
  - 若 bucket 比例正确但 custody/treasury 语义写错，仍必须 `block`。
  - 若团队总量正确但个人拆分未提供，仍必须 `block`。
  - 若首年释放计划只写“尽量低”，没有明确数值，视为配置不完整。
- Non-Functional Requirements:
  - NFR-TGA-1: checklist 字段完整率必须为 `100%`，缺字段不得出 `pass`。
  - NFR-TGA-2: verdict 只能使用 `pass` / `block` / `conditional_draft_only`。
  - NFR-TGA-3: 所有阻断结论都必须附带具体条目、实际值、目标值和修正建议。
- Security & Privacy: checklist 可以使用逻辑账户名或多签代号，不要求在模板中暴露真实私钥信息；若涉及真实地址，也只记录公开链上账户标识。

## 5. Risks & Roadmap
- Phased Rollout:
  - MVP: 建立 checklist 结构、阻断条件与执行模板。
  - v1.1: 在第一次真实创世冻结评审中实际填表并归档 verdict。
  - v2.0: 若后续加入 fully on-chain early contributor distribution，再扩展对应审计项。
- Technical Risks:
  - 风险-1: 如果 checklist 不区分 draft review 与 final pass，团队容易误以为“有模板就等于可 mint”。
  - 风险-2: 若 recipient 语义长期停留在逻辑账户名，最终地址切换时可能出现遗漏。

## 6. Validation & Decision Record
- Test Plan & Traceability:
| PRD-ID | 对应任务 | 测试层级 | 验证方法 | 回归影响范围 |
| --- | --- | --- | --- | --- |
| PRD-TESTING-TGA-001 | TGAC-1/TGAC-2 | `test_tier_required` | checklist 结构、字段完整性与阻断项检查 | 创世参数表审计一致性 |
| PRD-TESTING-TGA-002 | TGAC-1/TGAC-2/TGAC-3 | `test_tier_required` | verdict 规则、block 条件与互链检查 | producer 决策门禁 |
| PRD-TESTING-TGA-003 | TGAC-2/TGAC-3/TGAC-4 | `test_tier_required` | custody/treasury 语义检查、模板可填性与文档治理门禁 | runtime 实现与产品口径对齐 |
- Decision Log:
| 决策ID | 选定方案 | 备选方案（否决） | 依据 |
| --- | --- | --- | --- |
| DEC-TGA-001 | 用专门 QA checklist 约束创世配置 | 继续口头审查 | 创世配置错误一旦执行，修复成本极高。 |
| DEC-TGA-002 | 把 `custody account vs treasury bucket` 作为独立阻断项 | 只审比例和上限 | 当前实现最容易被误读的就是这条语义边界。 |
| DEC-TGA-003 | 允许 `conditional_draft_only` 作为草案阶段结论 | 只有 `pass/block` | 参数草案阶段需要明确“可讨论但不可执行”的中间态。 |

## 7. 创世分配审计清单（执行版）

### 7.1 Verdict 规则
- `pass`: 所有硬阻断项通过，且已填入真实控制主体/多签信息并归档该次审计证据；它只支持该次创世执行决策，不是一般发布结论。
- `block`: 任一硬阻断项失败。
- `conditional_draft_only`: 比例与结构正确，但真实控制主体、个人拆分或执行细节仍未补齐，不允许 mint。

### 7.2 审计项
| Audit ID | 检查项 | 通过条件 | 阻断条件 |
| --- | --- | --- | --- |
| TGA-01 | 分配比例总和 | `sum(allocation_bps)=10000` | 任意偏差 |
| TGA-02 | bucket 完整性 | 7 个 bucket 全部存在且字段非空 | bucket 缺失/空值 |
| TGA-03 | 项目战略控制 | 汇总为 `5000 bps` | 不等于 `5000 bps` |
| TGA-04 | 协议长期储备 | 汇总为 `3500 bps` | 不等于 `3500 bps` |
| TGA-05 | 个人直持上限 | 每个自然人 `<=1500 bps` | 任一超限 |
| TGA-06 | 创世液态流通 | 全部 `genesis_liquid=0` | 任一非零 |
| TGA-07 | 首年外部释放 | `<=500 bps`，目标 `100~200 bps` | 超过 `500 bps` 或未给出数值 |
| TGA-08 | custody / treasury 语义 | 文档明确创世先写 recipient `vested_balance` | 把 custody 误当 treasury |
| TGA-09 | 奖励叙事边界 | 无 `play-to-earn/login reward/time played = token` | 命中任意禁语 |
