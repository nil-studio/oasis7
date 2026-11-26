# oasis7 Simulator：LMSO29 可用性与稳定性收敛（设计文档）设计

- 对应需求文档: `doc/world-simulator/llm/llm-lmso29-stability.prd.md`
- 专题入口与权威边界: `doc/world-simulator/llm/README.md`

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义 LMSO29 可用性与稳定性收敛设计，聚焦模型可达性、错误恢复与质量波动控制。

## 2. 设计结构
- 可用性层：保证模型请求链路、配额与超时处理稳定。
- 稳定性层：控制响应格式、质量与延迟波动。
- 降级恢复层：在不可用时提供可预期降级与重试。
- 验证基线层：沉淀 LMSO29 专用稳定性回归基线。

## 3. 关键接口 / 入口
- LMSO29 请求入口
- 稳定性/错误指标
- 降级重试策略
- 稳定性回归基线

## 4. 约束与边界
- 稳定性优先于局部体验增强。
- 降级行为需与用户可见反馈一致。
- 不在本专题扩展跨模型编排。

## 5. 设计演进计划
- 先收敛可用性问题。
- 再控制波动与降级策略。
- 最后固定回归基线。

## 6. 预算调优范围与当前状态

专业 owner：`agent_engineer`；验证 owner：`qa_engineer`。基线为 `4e94163a3dff9ba69f6959b406ca3fcedf8aa097` 的文档审读；配对 PRD 的 2026-02-11 已实施/回归观察及 LMSO29.1 是历史窗口，不改写为当前 PASS 或计划。

<a id="lmso-budget-compatibility"></a>
### 6.1 预算与协议兼容

预算调整只作用于实际 profile 的有效输入空间、peak soft/hard reserve 与软段压缩阈值；输出预留、tool-only 协议及世界动作语义保持。旧 module history、memory digest 可压缩，但当前必要 observation、目标和最新查询结果服从 builtin 输入合同，最终请求不足时停止，不能靠静默提高预算或裁掉必要输入达标。profile、assembler 和最终请求捕获分别负责预算计算、压缩与最终容量确认，历史预算单测不能代签输入完整性。

<a id="lmso-sampling-boundary"></a>
### 6.2 稳定性抽样与失败解释

保持同场景、候选、模型配置、tick 数与指标口径，对照 report.json 和逐 tick run.log；先报告 llm_errors、parse_errors、repair_rounds_total，再比较 action_success、llm_input_chars_max 和 module_call 波动。30 tick 单次样本与多次样本分别标识，不由一次零错误推出长期稳定。历史报告路径可取回性未核实；缺基线或原始日志时标记比较受阻，不生成今日通过结论。失败由既有 parser/repair 与有界行为循环处理，不在此增加跨模型降级或无限重试。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [预算接口与压缩范围](llm-lmso29-stability.prd.md#4-technical-specifications) | 调整 prompt_budget、peak_targets_tokens、apply_peak_budget 与素材压缩时维持协议兼容并保护当前必需输入。 | [设计条款](#lmso-budget-compatibility) | agent_engineer / qa_engineer；builtin输入保护由配对专业合同负责。 | 产品适用性：N/A；reason=本关系仅约束既有llm_bootstrap预算调优的工程兼容与抽样方法，不新增玩家能力或provider支持声明；scope=LMSO29预算阈值、压缩和同口径回归，不覆盖完整玩家旅程或发布。 |
| [验证与证据边界](llm-lmso29-stability.prd.md#6-validation-decision-record) | 历史样本、确定性单测与当前在线抽样各自保留证明边界。 | [设计条款](#lmso-sampling-boundary) | agent_engineer / qa_engineer；builtin输入保护由配对专业合同负责。 | 产品适用性：N/A；reason=本关系仅约束既有llm_bootstrap预算调优的工程兼容与抽样方法，不新增玩家能力或provider支持声明；scope=LMSO29预算阈值、压缩和同口径回归，不覆盖完整玩家旅程或发布。 |

## 11. 验证设计

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [预算接口与压缩范围](llm-lmso29-stability.prd.md#4-technical-specifications) | [设计条款](#lmso-budget-compatibility) | 预算阈值与协议兼容；当前必要输入另按[builtin验收](decision-provider-contract.prd.md#ac-builtin-input-integrity)。 | 按[测试手册](../../../testing-manual.md)选择required；精确入口 `crates/oasis7/src/simulator/llm_agent/tests_conversation_flow.rs` 的 `llm_prompt_profile_uses_relaxed_token_budget_for_stability`，命令 `env -u RUSTC_WRAPPER cargo test -p oasis7 llm_prompt_profile_uses_relaxed_token_budget_for_stability -- --nocapture`，只断言Compact/Balanced的context、输出预留、margin及有效预算。另运行 `crates/oasis7_agent_harness/src/llm_prompt_assembly.rs` 中的 `prompt_budget_peak_targets_use_relaxed_reserve_values`（命令 `env -u RUSTC_WRAPPER cargo test -p oasis7_agent_harness prompt_budget_peak_targets_use_relaxed_reserve_values -- --nocapture`），只断言3328输入预算下两阶段soft/hard四值，不证明最终payload输入完整性。按 builtin AC-BUILTIN-INPUT-INTEGRITY 验收另捕获最终请求与不足配置无调用反例，不能由预算阈值测试代签。 | 候选、真实命令、配置、退出码、原始断言/日志与未运行项。 | 本次只定义方法未运行行为或付费模型；历史命令不证明当前通过。 |
| [验证与证据边界](llm-lmso29-stability.prd.md#6-validation-decision-record) | [设计条款](#lmso-sampling-boundary) | 可比抽样和缺证据阻断 | 按[测试手册](../../../testing-manual.md)选择 full：`env -u RUSTC_WRAPPER cargo run -p oasis7_provider_tools --bin oasis7_llm_agent_demo -- llm_bootstrap --ticks 30 --report-json .tmp/lmso29_candidate_30/report.json`，stdout/stderr另存同目录run.log；执行前固定模型/配置/候选及可读取基线，保存每次 report.json/run.log，按§6.2逐项对账。缺旧报告时仅报告当前样本不声称相对改善；多次运行数量与种子在实施PR预先记录。 | 候选、真实命令、配置、退出码、原始断言/日志与未运行项。 | 本次只定义方法未运行行为或付费模型；历史命令不证明当前通过。 |

## 12. 决策与风险

选择有限预算放宽而非协议或模型替换，避免把单次质量改善当作整体稳定性。输入体积回升、随机样本波动和parser边缘分支仍需独立回归；agent/QA owner在profile、工具或压缩规则变化时复核。长期成本、跨provider和发布资格不在本专题证明范围。
