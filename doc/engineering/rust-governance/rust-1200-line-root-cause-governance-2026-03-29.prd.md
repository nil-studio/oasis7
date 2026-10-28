# Rust 1200 行与结构切片治理

## 权威边界

本文定义长期规则与完成态；`scripts/check-rust-file-size.sh` 是当前扫描结果
的单一运行时真值。历史峰值、迁移批次、逐文件行数和一次性验证记录不属于
当前规则，改从 GitHub task、Git history 或 git history 追溯。

## 当前契约

- 扫描 tracked 的首方 Rust 源文件，覆盖 `crates/**/*.rs` 与
  `tools/**/*.rs`，排除 `third_party/`、vendored、生成和构建产物目录。
- 任一生产或测试 Rust 文件超过 1200 行即失败，不维护 frozen baseline、
  allowlist 或“未触碰可保留”例外。
- `split_part*`、`impl_part*`、`part1`、`part2` 和 `include!` 不能作为结构
  治理完成态；结构切片扫描或 include target 非零即失败。
- `doc/.governance/rust-oversized-file-baseline.tsv` 与
  `doc/.governance/rust-structural-slicing-baseline.tsv` 已退役，不得恢复为
  放行机制。
- `scripts/ci-tests.sh required` 必须执行 `scripts/check-rust-file-size.sh`；
  本地可直接运行同一脚本取得快速、可复现的门禁结果。该扫描必须可在 Linux
  与 macOS 执行，required tier 中的增量耗时目标不超过 15 秒。

## 可接受的拆分完成态

1. 按业务或技术职责抽出有语义的模块，父模块只保留编排、稳定导出或共同
   不变量。
2. 拆分后的每个文件均不超过 1200 行，且没有复制类型、helper 或测试来把
   债务转移到平行文件。
3. 测试文件按行为域拆分；生产代码按职责边界拆分。文件名表达职责，不表达
   临时序号。
4. 变更绑定受影响 crate 的定向 check/test；涉及 runtime、network、viewer
   或 launcher 的稳定入口时，按该模块现行验证规范补充集成或 UI/Web 验证。

## 变更流程

1. 运行 `./scripts/check-rust-file-size.sh`，记录失败路径和计数。
2. 在当前工作说明或按需 Issue 中声明目标职责、写范围和回归命令；不要在本文
   维护并行任务 ledger。
3. 完成语义拆分并执行定向回归。
4. 再次运行 `./scripts/check-rust-file-size.sh`；只有 oversized code、test、
   structural slice 与 include target 四项均为零才满足本治理门禁。

紧急修复也不得静默绕过门禁。若修复暴露更大的领域边界问题，由当前负责人按实际问题组织对应 runtime、viewer、WASM、agent、blockchain ops 或其他专业能力的复核；复杂协调按需由 TPM 协助。领域复核负责拆分后的语义边界，QA 关注验证充分性与适用放行风险；这些是专业职责，不要求额外人员、固定角色组合或独立审批流程。必要验证、按风险独立评审和合入条件遵循[开发流程规范](../workflow/source-of-truth.md#3-验证与评审)。repository health 只判断仓库结构与治理契约，不替代领域正确性和必要验证。

## 维护规则

- 阈值、扫描范围或 required-gate 接线变化时，同一任务同步修改脚本、本文和
  对应测试。
- 当前扫描数字只出现在命令证据中，不硬编码进本文。
- 已完成任务的逐文件结果留在任务证据与 git history；只有仍然有效的规则、
  风险和操作入口留在 live 文档树。
