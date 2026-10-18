# oasis7 主链 Token 创世分配审计清单设计

- 对应需求文档: `doc/testing/governance/token-genesis-allocation-audit-checklist.prd.md`
- 可变任务状态与历史: PR、实际 CI 与评审记录（Issue 按需）

## 1. 设计定位
提供一份 `qa_engineer` 侧的创世配置门禁设计，让 Token mint 前至少经过一轮结构化 QA 审计，而不是只靠 producer/runtime 自审。

<a id="audit-method"></a>
## 2. 设计结构

产品适用性：N/A；reason=本节只设计 QA 清单、证据格式及对既有 producer 参数的一致性检查，不决定或实现 Token 分配政策；scope=审计输入、逐项核对、verdict 和证据记录机制。此说明不将经济政策本身标为不适用：[Token 创世与贡献奖励权威](../../p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md#2-user-experience-functionality)仍拥有分配、控制主体、流通和奖励口径；本专题原有数值、custody 语义、禁语与独立 QA 硬阻断完整适用。真实冻结前审计不能被模板或文档检查代签，本文不授予 mint、充值、发放或发布权限。
- 输入层：读取 `TIGR-1` 创世参数表与 producer 口径。
- 审计层：按比例、上限、流通、语义四个维度逐项判定。
- 输出层：统一输出 `pass / block / conditional_draft_only`。
- 回流层：将实际审计结论保留在对应 PR/既有 Task 与原始证据中，`testing` 与 `p2p token` 规范只接收可复用的审计规则变化；不强制另建 Project。

## 3. 关键接口 / 入口
- `doc/p2p/token/mainchain-token-genesis-and-contributor-reward.prd.md`
- `doc/testing/evidence/token-genesis-allocation-audit-template.md`
- `crates/oasis7_runtime/src/main_token.rs`

## 4. 约束与边界
- checklist 只做 QA 门禁，不替代 producer 的经济决策。
- `conditional_draft_only` 不是放行结果；模板存在或比例草案正确都不能替代真实证据、mint 决策或发布结论。
- 若控制主体未从逻辑名映射到真实多签/治理主体，不得给最终 `pass`。

## 5. 设计演进计划
- 先建模板和阻断规则。
- 再在真实创世冻结前跑第一次正式审计。
- 后续若 early contributor distribution fully on-chain，再扩展新审计项。

### 2.1 需求承接与分配表

下表承接本专题既有专业验收，保持方法与实际运行分离；旧任务编号仅用于来源定位。

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [QA 清单与 verdict 专业验收](token-genesis-allocation-audit-checklist.prd.md#2-user-experience-functionality) | 真实冻结前参数完整、比例/控制主体/流通/custody 语义与 verdict 一致 | [审计/采样结构](#audit-method) | QA；专题 PRD §4/6 指定的原始输入和领域 owner | 不决定供应量、mint 或经济参数，不把模板通过当作真实创世审计或发布许可 |

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [QA 清单与 verdict 专业验收](token-genesis-allocation-audit-checklist.prd.md#2-user-experience-functionality) | [审计/采样结构](#audit-method) | 真实冻结前参数完整、比例/控制主体/流通/custody 语义与 verdict 一致 | 专题 PRD §6 验证入口；原始结果按[主手册证据规范](../../../testing-manual.md#证据规范)保存。按专题 PRD §6 的 test_tier_required，使用现有审计模板逐项回读 TIGR-1 参数、实际主体映射与 producer 口径；核对总和10000、个人上限1500、genesis_liquid=0、首年外部释放500 bps和禁语，缺字段或权威主体映射不得 pass | 实际审计参数版本、每项实际值/目标值、阻断依据和 verdict；PR 引用原证据 | 不决定供应量、mint 或经济参数，不把模板通过当作真实创世审计或发布许可；本次文档审读没有执行该审计/量测 |

## 范围、结构边界与风险

本设计范围是创世冻结前的独立 QA 一致性审计机制，输入、逐项核对和 verdict 依下文结构执行。主要风险是参数缺失、逻辑主体没有实际治理映射或 custody/treasury 混淆；遇到这些缺口保留阻断，不凭模板完整或 producer 草案替代真实审计。
