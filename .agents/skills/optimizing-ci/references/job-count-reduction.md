# 分册：减少 CI Job 数量与收敛全量触发

本分册隶属 [optimizing-ci](../SKILL.md)，汇总仓库 CI 缩减与切片演进（`#6575`、`#6597`、`#6626`、`#6659`、`#6685`、`#6701`、`#6746`、`#6818`、`#6987`、`#7015`）验证有效的具体手段、关键脚本接缝、混合版本升级规则与高频防坑清单。权威门禁规则以 [PR 快速门禁与定期全量设计](../../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 和 [CI 分层执行设计](../../../../doc/testing/ci/ci-tiered-execution.design.md) 为准。

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
| `scripts/ci-python-control-contracts.py` | 在 `workflow-governance` 中总控 `ci-authority` 受信测试：`EVOLVING_POLICY_TESTS` 在 `policy_sha` 侧仅跑 `FIXED_TESTS` 红线子集并在候选侧跑完整新套件；其余契约套件在无全量策略迁移（`boundaries` 含 `ci-required-scope.json` / `ci-python-control-contracts.py`）时跑 `policy_sha` 全集，`FIXED_TESTS` 始终在受信侧强制执行且禁删改签名 | `scripts/ci-python-control-contracts.test.py` |

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
- **受控 `ci-required-scope.json` 规则增删、超集扩充与同 PR 新路径自登记切片**（`ordinary_scope_config_slice_version=1/2`，`#6818`、`#7015`）：当仅修改受控选测/分片字段或顶层 `rules` 时，同时满足以下全部不变量即按 `workflow_governance` + 目标组快速切片，无需拆两步前置 PR（任一不满足立即 fail-closed 回退 `scope=full`）：
  1. 所有保留旧规则的相对顺序严格不变（不改变 first-match 遮蔽语义）；`v2` 同时支持在不改 `groups`/`exclude`/`full`/`reason` 及既有 `match` 顺序的前提下，向既有规则**保序追加 `match` 超集模式**；
  2. 被删除旧规则在 `before` 与 `after` 两个受追踪 Git tree 中均匹配 **0** 个文件（仅清理已退役死路径）；
  3. 新增规则或保序扩充模式满足 `not rule.get('full', False)` 且声明非空合法 `groups`（禁静默新增 `groups: []` 忽略规则；若含 `workspace_support` 则同步传播 `support_rule`）；
  4. **同 PR 新引入文件直连候选受控规则（`#7015`）**：同次比较区间内全新创建的文件（`before` 不存在、`after` 为 regular Git blob）可直接命中经全部比较对一致校验通过的本 PR 受控新增/扩充规则模式（同批若合改 `scripts/ci-tests.sh` 等共享总控入口仍回退 `scope=full`，但不再误报 `unknown CI inputs`；既有未登记文件的修改仍严禁自授权并继续报 `UnknownInputsError`）。

### 模式 E：暂停推进模块与 `workflow_call` 可复用工作流的零算力下线（`#6987`）
- **适用场景**：独立工具或子工程（如 `tools/oasis7-command-center/`）阶段性暂停推进、代码保留在仓内，但其专属工作流（尤其是 `macos-15` / `windows-2022` 高倍率 Job）仍在 PR、主线与定时全量中持续消耗算力。
- **为什么不能直接从 `CONFIG["groups"]` 或 `rust.yml` 硬删**：`policy_sha` 上的 `FIXED_TESTS`（如 `explicit-full-gate` 全组覆盖、`ci-full-result.test.py` 缺组负例、`ci-workflow.test.py` 规划失败前置守卫及 `FIXED_TESTS['scripts/ci-required-result.test.py'] = None` 全集）强约束配置组集合与 caller Job 存在性。
- **单 PR 平滑下线三步法**：
  1. **保路径归属、立独立离线版本键**：保留目录路径规则避免触发 `UnknownInputsError` 和保留 `.github/workflows/<callee>.yml` 只读定义；保持 `contract_job_consolidation_version` 在 `FIXED_TESTS` 已允许的版本集合内，新增独立 `"<module>_offline_version": 1` 并在 `group_jobs` 将该组映射至常驻 `"baseline"`；
  2. **按计划版本条件启用覆盖映射**：在 `rust.yml` 用 `fromJSON(needs.select.outputs.plan).<module>_offline_version != 1` 守护原 caller Job（启用后在 `pr-fast`、`main-fast`、`periodic-full`、`candidate-full` 下均 `0s` `skipped`），在 `baseline` 绑定 `run_<module>`；在 `ci-required-result.py` 的 `_consolidation_overrides` 中**仅当 `plan.get('<module>_offline_version') == 1` 时保留该覆盖映射**，无该字段的旧计划 `pop` 回退原独立 Job 成功校验——同时兼容本 PR 合入前 `old policy_sha` 的最后一次运行与 `ci-required-result.test.py` 全部历史固定用例；
  3. **解绑共享包与通用脚本连带映射**：同步从共享基础 crate 的 `package_groups` 及通用脚本（如 `scripts/cargo-dev.sh`、`scripts/find-python-with-module.sh`）移除对该暂停组的连带选择，在归零远端算力的同时解除本地 `check-local.py --profile affected` 命中 `UNSUPPORTED` 组的误阻塞。

## 4. 混合版本切换与 `required-gate` 铁律

1. **旧 Job 用版本守卫退役而非直接硬删**：新增或递增 `*_version` 时，旧独立 job 必须保留并以 `!= <new_ver>` 守护跳过；`ci-required-result.py` 与 `ci-full-result.py` 继续兼容旧版本号映射。
2. **`required-gate` `needs:` 兼容规则**：本优化 PR 合入前在远端首次运行 CI 时，`policy_sha` 仍是旧版 `origin/main`（执行 `scope=full` 旧语义）。旧版 `ci-required-result.py` 要求 `required-gate` `needs:` 中的任何额外 job 在 `scope=full` 下必须为 `success`（不能为 `skipped`）。因此向 `required-gate` 新增 `needs:` 时，只能加入在旧版 `scope=full` 下也会实际执行并成功的 job（如 `custody-*`），绝不能加入在 `scope=full` 下会被 `skipped` 的 legacy fallback job（如 `workspace-support-legacy`）。
3. **区分 `EVOLVING_POLICY_TESTS` 单 PR 演进与 `FIXED_TESTS` 两步式前置**：`ci-workflow.test.py` 等 `EVOLVING_POLICY_TESTS` 的常规拓扑/投影断言可直接随 `rust.yml` 单 PR 同步更新；仅触及 `policy_sha` `FIXED_TESTS` 红线或无全量 `migration` 的非演进契约套件时，按 [按需选测与快速失败分册 §4](on-demand-scope-and-preflight.md#trusted-control-split-and-simulation) 执行 `PR-A -> PR-B`。

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
RUST_TC="$(sed -n 's/^channel = "\(.*\)"/\1/p' rust-toolchain.toml)"
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py
TMPDIR=/dev/shm RUSTUP_TOOLCHAIN="$RUST_TC" python3 scripts/plan-rust-required-scope.test.py
python3 scripts/ci-python-control-contracts.py local-governance

# 4. 同步修改 doc/testing/ci/*.md 设计文档时执行文档治理与追溯检查（需 markdown-it-py==3.0.0）
./scripts/doc-governance-check.sh
python3 scripts/system-design-traceability-check.test.py

# 5. PR 切片边界与本地快速基线
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates none
python3 scripts/check-local.py --profile quick --base-ref origin/main
```

提交前逐项核对 11 个高频陷阱：
1. **宿主容器环境与 UID**：迁入 `ci-rust-build` 容器（`--user 1001:1001`）或其他宿主 job 时，核对 Python 3.12、`markdown-it-py` venv、`jq`、`ruby`、`rustc`/`cargo`/`clippy` 与 `safe.directory` 是否齐全。
2. **`baseline` 步级环境变量禁令**：`scripts/resolve-product-doc-ci-range.test.py` 禁止 `baseline` 内任何步骤声明 `OASIS7_PRODUCT_DOC_BASE:` 或 `OASIS7_PRODUCT_DOC_HEAD:` 覆盖前序步骤写入的 `$GITHUB_ENV`。
3. **Rust 缓存配对 `if:` 不变量**：`scripts/ci-workflow.test.py` 强制要求同一 job 内 `Stabilize Rust cache toolchains` 与 `Swatinem/rust-cache` 的 `if:` 表达式完全相同。
4. **旁路工作流静态解析测试**：修改 `.github/workflows/codeql.yml` 等旁路工作流前，先检索对应测试（如 `scripts/security/codeql-rust-coverage.test.py` 对 `paths-ignore:` 的切分假设）。
5. **负向突变单测替换范围**：在 `ci-workflow.test.py` 构造突变用例时，注意同一表达式可能在 job 级与多个 step 级同时出现，避免 `.replace(..., 1)` 只改第一处而漏测。
6. **版本上限与旧基线模拟测试同步**：递增 `*_version` 后，同步更新 `plan-rust-required-scope.test.py` 的非法版本负向断言、旧基线回退测试（一并 `pop` 新增能力键）及 `ci-workflow.test.py` 的 `validate_candidate_graph`。
7. **`rust.yml` 双投影字段列表同步**：向 `selection.json` 新增工作流消费字段时，必须同时更新 `select` 的 `workflow-plan` 与门禁共用的 `&validate-complete-selection` 两处 `fields` 投影及多键同现校验，并在 `ci-workflow.test.py` 的 `test_large_workflow_projection_preserves_fields_and_binds_complete_file_and_identity` 登记该 `fromJSON(needs.select.outputs.plan).<key>`。
8. **单行 `if:` 改多行 `if: >-` 破坏静态断言**：给既有步骤追加条件前，先检索 `ci-workflow.test.py` 中对该步骤的 `assertIn("if: runner.os == ...")`，提前放宽为兼容 `if:\s*(?:>-\s*)?` 的正则（单引号 `'` 后勿加 `\b`）。
9. **`plan-rust-required-scope.test.py` 临时 Cargo 夹具约束**：`cargo_graph` 要求夹具根 `Cargo.toml` 显式列出具体 `members` 路径（禁 `"crates/*"` 通配）并包含 `[workspace.dependencies]`（推荐基于 `self.ROOT_TOML` 派生），且所有夹具文件路径须命中已注册规则以免触发 `UnknownInputsError`。
10. **合并短合约后的 `scope=full` 宿主 `timeout-minutes` 复核（`#6925`、`#6966`）**：向常驻宿主（如 `viewer-js-required`、`workflow-governance`、`rust-baseline`）并入子合约时，必须按 `scope=full` 最坏叠加路径复核并同步放宽宿主 `timeout-minutes`，详见 [超时棘轮与余量校准分册](timeout-ratchet-and-headroom.md)。
11. **全仓检索 `tools/**` 与 `crates/**` 下的跨目录工作流自检脚本（`#6987` 案例）**：修改 `scripts/ci-required-scope.json`（如规则收窄、解绑共享脚本）或 `.github/workflows/rust.yml` 后，用 `rg` 排查静态断言时**切勿只搜 `scripts/`**——独立子工具（如 `tools/oasis7-command-center/scripts/test_package_context_cli.py`）与部分 crate 自检也会直接读取根仓 `ci-required-scope.json` / `rust.yml` 校验触发覆盖，漏改会导致过渡期运行在子工具测试步骤判红。
