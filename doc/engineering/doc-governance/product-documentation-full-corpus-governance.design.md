# 产品文档全量治理设计

- 上游内容规范：[`product-documentation-standard.design.md`](product-documentation-standard.design.md)
- 产品树入口：[`doc/product/README.md`](../../product/README.md)
- Owner role：`producer_system_designer`
- 治理协作：`repository_health_engineer`
- 状态：`active`
- Last reviewed：2026-10-09

本文定义如何将已采纳的产品文档内容规范应用于全部存量产品文档，并在不破坏专业 authority 的前提下收口外部产品语义。它补充全量治理机制，不改变四模块产品树、专业职责或 Git、PR 与实际 CI 记录。

## 1. 目标与完成边界

全量治理完成必须同时满足：四个模块根 PRD 保持唯一入口和汇总职责；每个 active 专题达到现行内容合同；复杂专题在适用时拥有配对 design；外部产品语义已分类并迁移或明确保留为专业 authority；全量检查与 changed-range 检查均通过。

`superseded` 与 `retired` 文档不要求恢复为 active 内容模板，但必须具备可判定的接收 authority、剩余语义、稳定引用与删除条件。历史文档不能继续承担 active 路线图、产品承诺或实现输入。

完成不表示实现、试玩、发行或公开能力已经成立。产品、专业实现、体验证据与公开 claim 继续由各自 authority 判定。

## 2. 治理单元与顺序

治理以单个产品专题或单个外部来源文件为最小单元，按以下顺序执行：

1. 建立全量、可重复的机械审计入口，列出每个文件的具体缺口。
2. 按四模块分别治理 active 根 PRD、专题 PRD 和配对 design。
3. 分类 `doc/product/` 外的产品语义，迁移产品承诺并保留专业合同。
4. 复核 retired/superseded 文档的迁移闭合与删除条件。
5. 冻结最终 HEAD，执行全量验证、涉及角色审查与 PR 流程。

每个治理单元必须在同一修改中完成内容、导航、回链、authority、REQ/AC 和必要 design；不得留下只能靠后续读者猜测的半迁移状态。

### 2.1 范围与内容去向

治理开始时列出来源文件、待接收产品条款、专业 authority、内容与检查器范围、排除项和验收方式。规模较大时可用协调 Issue 跟踪，但不作为开发准入。

每个治理单元在 PR 中说明条款的实际去向、引用修复、验证版本与结果、剩余缺口。整体完成依据全部范围的内容完整性与实际验证判断，局部完成不能替代尚未完成的治理义务。

## 3. Active 专题内容合同

每个 active 专题 PRD 必须明确包含或等价表达：

- 所属模块、上位 PRD、生命周期、owner、专业 authority 与复核日期；
- 不依赖实现术语的玩家或使用者情境；
- 范围、Non-Goals、正常路径、核心选择、成本与风险；
- 主要失败、拒绝、失效、恢复和安全停止边界；
- 目标要求、已验证事实、设计假设与公开承诺的区分；
- 可独立失败且可观察的稳定 `REQ-*`；
- 以给定/当/则或等价方式表达的稳定 `AC-*`；
- `REQ -> AC -> product/design/professional authority -> evidence tier` 追踪；
- 证据能够证明和不能证明的范围；
- 未决问题的影响条款、决策 role、所需信息、触发条件和临时排除范围。

根 PRD 只保留模块承诺、组合关系、稳定 SC 与六列追踪。细节达到独立生命周期、状态机、资源取舍、恢复或多角色协作规模时，必须下沉到专题。

## 4. 配对 design 判定

专题存在下列任一条件时，应提供同名 `*.design.md`：非平凡状态转换；跨入口或跨角色体验；授权、成本或不可逆提交；失败恢复交互；信息分层；多个真实策略选项。

配对 design 说明信息阶段、选择理由、成本反馈、状态优先级、异常恢复、可访问性和失败信号。它通过带 fragment 的链接承接 PRD 的 REQ/AC，不复制需求，也不引入 API、schema、组件或算法 authority。

如果专业角色判断专题足够简单，无需 design，必须在 PRD 中保留简短设计说明，并在当前 PR 与实际验证记录中写明不适用理由。产品树不保存长期例外台账。

### 4.1 设计适用性与生命周期闭合字段

每个 active 专题 PRD 都必须在产品正文中明确一个 `设计判定`：

- `paired-design`：PRD 的“配对产品设计”链接指向同名 `*.design.md`；配对 design 只承接产品语义，不复制需求或取得专业实现 authority。
- `simple-topic-exemption`：PRD 保留简短的设计说明、`设计适用性理由` 和当前 PR 与实际验证记录 链接，说明为什么独立 design 不增加新的信息分层、交互状态编排或策略取舍。该字段是本次判定的稳定结果，不是产品树中的长期例外台账。

每个 paired design 必须同时具备可解析的 PRD backlink，以及至少一组带 fragment 的 PRD `REQ-*` 和 `AC-*` 链接；完整映射应覆盖该 design 承接的每个产品义务。裸 token、只写文件路径、反引号中的 ID 或无 fragment 的泛化链接都不能满足这一字段。

`superseded` 与 `retired` 专题不参与 active design 判定，但必须在“生命周期闭合”段落中逐项提供以下字段：

| 字段 | `superseded` | `retired` |
| --- | --- | --- |
| `接收 authority` | 指向当前接收产品或专业 authority 的真实路径与 fragment | 指向保留历史语义的当前 authority 或专业 authority |
| `剩余语义` | 列出仍未被接收的产品语义、验收或引用 | 说明仍需保留的历史含义与不可再作 active 输入的边界；无剩余语义时明确写无 |
| `稳定引用` | 保留接收路径和本页仍被需要的稳定引用 | 保留历史/专业引用与当前可达入口 |
| `删除条件` | 语义完整接收、authority 可达、活跃引用修复且无未决阻塞 | 历史/专业语义有可达接收 authority、活跃引用修复且保留理由不再成立 |

上述字段只描述 authority 与迁移闭合，不复制任务状态、review ledger 或发布证据；任务证据仍由 Git、PR 与实际 CI 记录维护。

## 5. 外部语义迁移

每个 `doc/product/` 外候选文件按段落分成四类：产品承诺、专业合同、历史过程、未迁移语义。

Trace refs: O7-REQ-276983b3-3187-405e-8453-e2785f01c5e6；本节承接[全局要求追踪](cross-layer-requirements-traceability.design.md#global-requirement-trace)。新增或实质修改的跨层要求以主责原文的稳定 Trace ID 关联设计和任务；canonical repository `eng-cc/oasis7`、repository-relative path 与实际条款的 stable fragment 继续负责准确定位，固定证据另保留提交版本。路径或锚点迁移不改变同一要求的 Trace ID；裸 token、本地摘要或无 fragment 的泛化路径不能替代真实条款链接。不要求重写未触达存量或历史证据，也不新增 typed refs、任务身份或传递回执。

- 产品承诺迁入对应模块根 PRD 或稳定专题，并建立旧条款到新 REQ/SC 的映射。
- 专业合同留在原专业域；产品文档只说明它支持或限制的产品结果并链接准确 fragment。
- 历史过程从 active 阅读链移除，通过 Git history、PR 与实际验证记录追溯。
- 未迁移语义必须记录接收 owner、阻塞原因、当前 authority 和可验证删除条件。

只有产品语义完整接收、专业 authority 仍可达、所有活跃引用已修复后，才删除旧来源。混合文件不得为了目录整齐整体搬迁或整体删除。

## 6. 全量检查模型

现有 changed-range 检查继续约束新增和实质修改的产品文档。另提供显式 full-corpus 模式，对当前工作树中的全部根 PRD、专题 PRD 和 design 执行同一机械内容合同。

full-corpus 模式必须：

- 输出确定的文件顺序、检查数量和稳定诊断；
- 检查身份字段、生命周期、authority、复核日期、最低内容、REQ/AC、anchor、跨文件 fragment、回链和追踪关系；
- 对 active、superseded、retired 应用各自生命周期合同；
- 对 active 专题检查 `设计判定` 的 paired-design 或 simple-topic-exemption 闭合；对 superseded/retired 检查生命周期闭合四字段，并禁止它们冒充 active authority；
- 对每个 paired design 检查 PRD REQ/AC fragment 映射可解析且至少覆盖一组 REQ 与 AC；
- 不使用 allowlist、作者标签或任务评论绕过内容要求；
- 明确说明通过只证明机械合同，不证明产品质量、实现或发行状态。

canonical invocation 为：

```bash
python3 scripts/product-doc-content-check.py --repo-root <canonical-worktree> --full-corpus
```

`--full-corpus` 与 `--base`、`--head`、`--worktree` 互斥；它读取所选 canonical worktree 的全部 `doc/product/**/*.prd.md` 与 `doc/product/**/*.design.md`，按 repository-relative path 排序。成功输出稳定的 mode、checked 数量和零 error 结果并返回 0；内容缺口逐行输出稳定 error code、路径与 detail 并返回 1；参数或身份错误返回 2。checker 回归必须覆盖选择互斥、文件排序、生命周期合同、空树、失败码和诊断稳定性。

上述直接调用 `product-doc-content-check.py --full-corpus` 保留产品全量严格判定：内容缺口返回非零。聚合入口 `doc-governance-check.sh --full-corpus` 则先执行变更范围严格检查，再以独立 `--warn-only` 历史调用报告产品及专业内容缺口；聚合退出 0 不等于历史零缺口或全量治理完成。读取、安全、参数和执行错误仍失败。若需证明产品全量机械内容合同零缺口，仍须查看上述 standalone 严格结果及诊断，并独立核对适用语义与残余风险。

现有 changed-range 路径及其 trusted base/head、merge-base 和 worktree overlay 语义保持不变。全量治理使用 `doc-governance-check.sh --full-corpus`；治理合入后，仓库的持续验证必须保留全量回归，不能退回只验证一次性基线或用 full-corpus 替换作者变更范围身份。

整体完成需要范围清单内的条款均完整接收或有明确保留理由、authority 与引用可达、全量和变更范围检查通过、阻断评审问题解决。检查器通过或单个模块成功不能单独证明全部治理完成；剩余缺口直接列在 PR 或按需协调 Issue 中。

## 7. 角色与集成

- `producer_system_designer` 裁定产品承诺、模块归属、组合结果与最终内容完整性。
- `gameplay_designer` 裁定玩法循环、选择、成本与规则引用。
- `game_visual_interaction_designer` 裁定信息、反馈、异常交互和可访问性。
- 相关 runtime、WASM、Agent、Viewer、blockchain ops 角色确认专业 authority 与实现边界。
- `repository_health_engineer` 负责结构、重复、链接、checker 与迁移完整性。
- `qa_engineer` 复核 AC 可判定性、证据层级和全量验证结果。
- `liveops_community` 复核公开承诺、发行范围和玩家沟通边界。
- 负责人集成结果；需要多人协调时由 `tpm` 协助管理依赖和写入范围。

专业协作按具体问题选择，明确写入范围并避免重叠。根入口、checker、治理规范和跨模块索引由负责人集成，不要求固定角色逐一签收。

## 8. 验证与失败处理

每个模块批次先运行其全量内容检查和链接检查；跨模块集成后运行完整文档治理、checker 回归、README 链接与 `git diff --check`。评审按实际复杂度与风险安排，记录审阅版本与结论。

检查失败按稳定诊断定位到文件、条款和缺失关系。语义归属冲突返回产品 owner 与专业 owner 共同裁定；无法证明专业 authority 的内容不得写成当前事实。无法形成安全恢复或可判定 AC 时，专题保持未完成，不通过治理验收。

阻断反馈记录具体条款、事实与处置；修复或以可复核事实说明不适用。非阻断建议按价值处理，不自动扩大治理范围。

## 9. 非目标

- 不借文档治理改变玩法数值、runtime、WASM、Agent、Viewer 或链实现。
- 不把模板标题数量当作内容质量。
- 不为每个小主题机械创建 design。
- 不把目标态产品要求表述为当前实现或公开发行能力。
- 不在产品目录创建任务、迁移或 review 台账。
