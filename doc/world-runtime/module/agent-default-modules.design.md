# Agent 默认模块设计

- 对应需求文档: `doc/world-runtime/module/agent-default-modules.prd.md`
- 当前任务状态与历史变更：GitHub task issue evidence 与 Git history。

<a id="1-设计定位"></a>
## 1. 设计定位与目标
定义 Agent 出厂默认模块的安装方式、模块角色划分、生命周期与治理挂接方式。

## 2. 设计结构
- 默认模块集合：定义最小默认启用模块与对应职责。
- 安装链路：power bootstrap 与 agent default package 分开通过治理事件完成 register / activate，并保证幂等。
- 生命周期：支持首次安装、重装激活、版本替换与停用恢复。

## 3. 关键接口 / 入口
- `install_m1_power_bootstrap_modules`、`install_m1_agent_default_modules` 与 power-first `install_m1_scenario_bootstrap_modules`
- module manifest / module changeset / governance apply
- 事件：模块启用、停用、状态变更

## 4. 约束与边界
- 默认模块必须保持最小可信集合，不直接固化业务扩展。
- 世界初始状态与模块能力边界必须通过治理链路显式落地。
- 重复安装必须幂等，不允许重复 register。
- power 安装保留低速 radiation emission、有限私有储能与移动前 denial；已注册停用模块只能 re-activate。
- 安装使用既有 `manifest.version.saturating_add(1)`，不将它外推为 checked numeric contract。

<a id="5-设计演进计划"></a>
## 5. 设计演进计划与风险
- 先完成设计补齐与互链回写。
- 再按对应 GitHub task 的任务拆解推进实现与验证。

本设计只承接已有安装入口与最小默认集合；身体槽位扩容、接口模块来源和完整能力目标仍由配对 PRD 管理，不能由安装入口存在推导这些目标已实现。runtime owner 在默认集合、治理入口或场景配置变化时复核，具体进度留在对应 PR。

### 2.1 需求承接与分配表

| 上游 requirement / professional acceptance（path#fragment） | 具体 obligation 与适用条件 | 本设计条款（path#anchor） | 外部 owner / dependency | 明确排除或未覆盖范围 |
| --- | --- | --- | --- | --- |
| [安装、治理与兼容边界](agent-default-modules.prd.md#agent-default-modules-install-governance-compatibility) | 三个安装入口经治理 register/activate，重复安装不重复 register，已注册停用项恢复 activate；场景入口保持 power-first 与默认包配置边界。 | [安装约束](#4-约束与边界) | runtime 拥有安装与治理；WASM 拥有模块 manifest 与能力语义。 | 产品适用性：N/A；reason=本行只规范既有 bootstrap 入口的幂等和安装组合，不新增玩家能力或完整默认模块产品验收；scope=三个 install_m1 入口，身体槽位、扩容来源与完整模块功能不由本行证明。 |

## 11. 验证设计与可追溯性

### 11.1 验证映射表

| 上游 requirement / professional acceptance（path#fragment） | 本设计条款（path#anchor） | 独立 obligation 与适用条件 | 准确验证方法、test/manual source 或 ID、scenario/layer、candidate/environment | evidence target | 未证明范围 |
| --- | --- | --- | --- | --- | --- |
| [安装、治理与兼容边界](agent-default-modules.prd.md#agent-default-modules-install-governance-compatibility) | [安装约束](#4-约束与边界) | 三个入口的治理安装、重复安装与停用恢复，以及场景 power-first/default-package toggle。 | 固定同一候选与 Wasmtime 配置，在 [power_bootstrap.rs](../../../crates/oasis7_runtime/src/tests/power_bootstrap.rs) 执行 L1 required 安装回归：`install_power_bootstrap_modules_registers_and_activates`、`install_agent_default_modules_registers_and_activates`、三个 `install_*_is_idempotent`、`install_power_bootstrap_modules_reactivates_registered_version` 及 `install_scenario_bootstrap_modules_supports_default_package_toggle`；比较 registry、active 集合与 journal 长度。默认包停用恢复与 power-first 事件顺序另需定向核对，不能由这些用例自动推出。文档编辑按 S0 核对合同与链接，本表不声明实际运行。 | 实施 PR 记录候选、配置、实际命令、registry/activation/事件对照与未运行项。 | 不证明完整七类模块功能、槽位扩容、跨节点 replay、生产发布或 saturating_add 的数值安全。 |
