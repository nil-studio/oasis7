# 分册：减少 CI Job 数量与收敛全量触发

本分册隶属 [optimizing-ci](../SKILL.md)，汇总仓库第 1–7 轮 CI 缩减（`#6575`、`#6597`、`#6626`、`#6659`、`#6685`、`#6701`、`#6746`、`#6818`）验证有效的具体手段、关键脚本接缝、混合版本升级规则与高频防坑清单。权威门禁规则以 [PR 快速门禁与定期全量设计](../../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 和 [CI 分层执行设计](../../../../doc/testing/ci/ci-tiered-execution.design.md) 为准。

## 1. 诊断信号：什么时候该减 Job

先拉取近期 `pr-fast`、`main-fast` 与 `scope=full` 的真实运行记录，重点寻找三类高收益目标：

1. **高冷启动短耗时 Job**：执行主体仅 10s–60s，却独占容器拉取、`actions/checkout`、Python/Node/Rust 安装或产物上传下载。
2. **纯搬运 / 纯汇总中转 Job**：自身不编译也不执行业务测试，仅检出仓库并下载上游分片产物做 `validate` 或二次打包，白白增加 1 个 job 与 1 跳排队延迟。
3. **不必要的 `scope=full` 或无关矩阵唤醒**：局部控制脚本改动、特定目标平台依赖变动或普通 `ci-required-scope.json` 规则登记直接回退 `scope=full`（25+ 个 job）。

**硬红线**：任何向既有宿主 job 的归并，合并后宿主单 job 墙钟都必须显著低于整条流水线的关键路径（如 `oasis7-required` 分片、`custody-node`），且不得跨权限等级（候选只读 vs 受信写缓存）混跑。

## 2. 关键文件与职责接缝

| 文件 | 在 Job 缩减中的职责 | 对应本地单测 |
| --- | --- | --- |
| `scripts/ci-required-scope.json` | 声明路径规则（`rules`）、受控控制路径（`scoped_control_paths`）、合并宿主映射（`group_jobs`）及各项 `*_version` 能力开关 | `scripts/plan-rust-required-scope.test.py` |
| `scripts/plan-rust-required-scope.py` | 计算受影响包/组、校验配置 schema、判定候选源码是否支持对应版本能力（`tested_source_supports_capability`），不满足安全条件时 fail-closed 回退 `scope=full` | `scripts/plan-rust-required-scope.test.py` |
| `scripts/ci-contract-jobs.py` | 生成并校验 `.github/workflows/rust.yml` 中的托管合约 job 区块及版本跳过条件（`--write` / `--check`） | `scripts/ci-contract-jobs.test.py` |
| `.github/workflows/rust.yml` | 定义 `select`、合并宿主 job、条件跳过（`if:`）、产物传递与 `required-gate` / `explicit-full-gate` 依赖图 | `scripts/ci-workflow.test.py`、`scripts/resolve-product-doc-ci-range.test.py` |
| `scripts/ci-required-result.py` | 在 `required-gate` 中根据 `plan` 与 `*_version` 将逻辑检查组映射到实际必须成功的宿主 job 或直接分片 | `scripts/ci-required-result.test.py` |
| `scripts/ci-full-result.py` | 校验全量门禁收据与工作流定义摘要（`definition_digest`），升级任何 `*_version` 时必须同步允许集合 | `scripts/ci-full-result.test.py` |
| `scripts/ci-custody-platform.py` | Custody 拆片打包、不可变测试清单导出（`export-inventory`）与零检出完整性验证（`validate --context-file`） | `scripts/ci-custody-platform.test.py` |
| `scripts/ci-python-control-contracts.py` | 在 `workflow-governance` 中总控 `ci-authority` 受信测试：默认用 `policy_sha` 测试跑候选代码，仅 `migration == ['scripts/ci-required-scope.json']` 委托可迁移套件，但 `FIXED_TESTS` 始终在受信侧强制执行且禁删改签名 | `scripts/ci-python-control-contracts.test.py` |

## 3. 四大验证有效的缩减模式

### 模式 A：短合约分批并入同栈常驻宿主 Job（`contract_job_consolidation_version`）
- **做法**：
  - 将纯脚本、文档、打包与轻量编译度量合约（如 `codex_agent_config_validation`、`macos_package_contract`、`doc_checker_contracts`、`compile_metrics`、`packaging_contracts`）并入固定 `ci-rust-build` 镜像的 `baseline`；
  - 将治理与运维脚本合约（如 `cargo_tooling_contracts`、`pr_governance_preview`、`operational_contracts`）并入 `workflow-governance`；
  - 将 Windows 专属短合约（`windows_rollout`）并入本来就要启动 Windows runner 的 `fleet-health (windows-2022)`；
  - 将普通非门禁控制脚本自测并入 `rust-baseline`（`ordinary_controls_in_rust_baseline_version`）；
  - 将需要 Chromium/Playwright 的静态站点检查（`site_quality`）并入已安装同款浏览器的 `viewer-js-required`；当 PR 仅命中 `site_quality`（未命中 `viewer_js_required` 且非 `full`）时，通过条件 `if:` 跳过 Viewer 的 npm 依赖、Rust 工具链、缓存、`wasm-bindgen` 与原生库安装，直接复用已有 `AGENT_BROWSER_EXECUTABLE_PATH` 与 Playwright。
- **效果**：省去多个独立容器冷启动后，合并后的 `baseline` 总耗时（约 `195s–224s`）甚至低于合并前两 job 墙钟之和（`95s + 246s = 341s`），关键路径零增加。

### 模式 B：零检出门禁直验替代独立汇总 Job（`workspace_support_direct_gate_version`）
- **做法**：
  1. 在本来已检出受测提交的 `select` 阶段运行 `ci-custody-platform.py export-inventory`，把受测提交的不可变测试清单连同受信校验脚本一起写入 `ci-authority` 产物；
  2. 将 `custody-native-contracts`、`custody-native`、`custody-wasm-production`、`custody-node` 加入 `required-gate` 的 `needs:`；
  3. `required-gate` 下载 `ci-authority` 与各分片结果后，直接通过 `--context-file` 零检出完成完整分片与清单核验（实测仅增加约 `4s`），在 `pr-fast` / `main-fast` 下完全省去独立 `workspace-support` job。
- **边界**：让 `explicit-full`（`periodic-full` / `candidate-full`）保持既有汇总路径，避免无必要地改动全量收据 schema。

### 模式 C：同栈串行构建产物合一与主干队列合并
- **同栈生产者合并**：如将 `custody-wasm-approval` 合入 `custody-wasm-production`，一次准备 Rust/WASM 工具链与依赖缓存并同时产出 production 与 approval 两个隔离 bundle，消除重复编译和额外 job。
- **主干 `main-fast` 准入跳过与并发合并**：已通过等效 PR 门禁的提交合入 `main` 时，由 `main-fast-admission` 单 job 快速放行；连续合入通过受控 concurrency 合并排队（`main_fast_coalesce_concurrency_version`），并为历史退役路径保留桥接规则以保障宽范围 diff 规划性能。

### 模式 D：精准依赖与受控配置切片（收敛 `scope=full`）
- **条件目标依赖切片**（`workspace_member_cargo_manifest_scope_version` / `scoped_production_cfg_version`）：解析成员 `Cargo.toml` 的 `[target.'cfg(...)'.dependencies]` 及生产源码受控 `#[cfg(...)]`，只唤醒对应目标平台的真实消费方。
- **受控 `ci-required-scope.json` 规则增删切片**（`ordinary_scope_config_slice_version`）：当仅修改受控字段或 `rules` 时，同时满足以下全部不变量才切片为 `workflow_governance` + 受影响组，任一不满足立即 fail-closed 回退 `scope=full`：
  1. 所有保留旧规则在修改前后保持严格相对顺序不变（不改变 first-match 遮蔽语义）；
  2. 被删除旧规则在 `before` 与 `after` 两个受追踪 Git tree 中均匹配 **0** 个文件；
  3. 新增规则满足 `not rule.get('full', False)` 且 `groups` 非空合法（若含 `workspace_support` 则同步传播 `support_rule`）。

## 4. 混合版本切换与 `required-gate` 铁律

1. **旧 Job 用版本守卫退役而非直接硬删**：新增或递增 `*_version` 时，旧独立 job 必须保留并以 `!= <new_ver>` 守护跳过；`ci-required-result.py` 与 `ci-full-result.py` 继续兼容旧版本号映射。
2. **`required-gate` `needs:` 兼容规则**：本优化 PR 合入前在远端首次运行 CI 时，`policy_sha` 仍是旧版 `origin/main`（执行 `scope=full` 旧语义）。旧版 `ci-required-result.py` 要求 `required-gate` `needs:` 中的任何额外 job 在 `scope=full` 下必须为 `success`（不能为 `skipped`）。因此向 `required-gate` 新增 `needs:` 时，只能加入在旧版 `scope=full` 下也会实际执行并成功的 job（如 `custody-*`），绝不能加入在 `scope=full` 下会被 `skipped` 的 legacy fallback job（如 `workspace-support-legacy`）。
3. **无版本分支静态断言采用两步式切换**：若修改 `rust.yml` 触及 `policy_sha` `ci-workflow.test.py` 中未按 `*_version` 门控的硬编码结构断言，按 [按需选测与快速失败分册 §4](on-demand-scope-and-preflight.md#4-两步式可信测试前置切片pr-a---pr-b与跨版本本地仿真) 执行 `PR-A`（可信测试双态兼容前置）-> `PR-B`（工作流切换与收紧）。

## 5. 标准本地闭环命令与防坑核对表

推送前按顺序执行：

```bash
# 1. 同步并检查托管合约 job 区块
python3 scripts/ci-contract-jobs.py --write
python3 scripts/ci-contract-jobs.py --check
python3 scripts/ci-contract-jobs.test.py

# 2. 校验门禁结果映射、custody 零检出上下文与产品文档范围不变量
python3 scripts/ci-required-result.test.py
python3 scripts/ci-full-result.test.py
python3 scripts/ci-custody-platform.test.py
python3 scripts/resolve-product-doc-ci-range.test.py RangeTests.test_workflow_wiring_is_baseline_only_and_cannot_override_exports

# 3. 完整工作流图、范围规划回归与跨版本 policy_sha 校验（详见 on-demand-scope-and-preflight.md §4）
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py
TMPDIR=/dev/shm RUSTUP_TOOLCHAIN=1.96.0 python3 scripts/plan-rust-required-scope.test.py
python3 scripts/ci-python-control-contracts.py local-governance

# 4. 同步修改 doc/testing/ci/*.md 设计文档时执行文档治理与追溯检查（需 markdown-it-py==3.0.0）
./scripts/doc-governance-check.sh
python3 scripts/system-design-traceability-check.test.py

# 5. PR 切片边界与本地快速基线
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates none
python3 scripts/check-local.py --profile quick --base-ref origin/main
```

提交前逐项核对 9 个高频陷阱：
1. **宿主容器环境与 UID**：迁入 `ci-rust-build` 容器（`--user 1001:1001`）或其他宿主 job 时，核对 Python 3.12、`markdown-it-py` venv、`jq`、`ruby`、`rustc`/`cargo`/`clippy` 与 `safe.directory` 是否齐全。
2. **`baseline` 步级环境变量禁令**：`scripts/resolve-product-doc-ci-range.test.py` 禁止 `baseline` 内任何步骤声明 `OASIS7_PRODUCT_DOC_BASE:` 或 `OASIS7_PRODUCT_DOC_HEAD:` 覆盖前序步骤写入的 `$GITHUB_ENV`。
3. **Rust 缓存配对 `if:` 不变量**：`scripts/ci-workflow.test.py` 强制要求同一 job 内 `Stabilize Rust cache toolchains` 与 `Swatinem/rust-cache` 的 `if:` 表达式完全相同。
4. **旁路工作流静态解析测试**：修改 `.github/workflows/codeql.yml` 等旁路工作流前，先检索对应测试（如 `scripts/security/codeql-rust-coverage.test.py` 对 `paths-ignore:` 的切分假设）。
5. **负向突变单测替换范围**：在 `ci-workflow.test.py` 构造突变用例时，注意同一表达式可能在 job 级与多个 step 级同时出现，避免 `.replace(..., 1)` 只改第一处而漏测。
6. **版本上限与旧基线模拟测试同步**：递增 `*_version` 后，同步更新 `plan-rust-required-scope.test.py` 的非法版本负向断言、旧基线回退测试（一并 `pop` 新增能力键）及 `ci-workflow.test.py` 的 `validate_candidate_graph`。
7. **`rust.yml` 双投影字段列表同步**：向 `selection.json` 新增工作流消费字段时，必须同时更新 `select` 的 `workflow-plan` 与门禁共用的 `&validate-complete-selection` 两处 `fields` 投影及多键同现校验，并在 `ci-workflow.test.py` 的 `test_large_workflow_projection_preserves_fields_and_binds_complete_file_and_identity` 登记该 `fromJSON(needs.select.outputs.plan).<key>`。
8. **单行 `if:` 改多行 `if: >-` 破坏静态断言**：给既有步骤追加条件前，先检索 `ci-workflow.test.py` 中对该步骤的 `assertIn("if: runner.os == ...")`，提前放宽为兼容 `if:\s*(?:>-\s*)?` 的正则（单引号 `'` 后勿加 `\b`）。
9. **`plan-rust-required-scope.test.py` 临时 Cargo 夹具约束**：`cargo_graph` 要求夹具根 `Cargo.toml` 显式列出具体 `members` 路径（禁 `"crates/*"` 通配）并包含 `[workspace.dependencies]`（推荐基于 `self.ROOT_TOML` 派生），且所有夹具文件路径须命中已注册规则以免触发 `UnknownInputsError`。
