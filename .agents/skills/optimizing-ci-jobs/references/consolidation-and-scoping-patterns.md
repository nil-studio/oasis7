# Oasis7 CI Job 缩减实证模式与踩坑参考

本参考汇总仓库第 1–7 轮 CI 缩减（`#6575`、`#6597`、`#6626`、`#6659`、`#6685`、`#6701`、`#6746`、`#6818`）验证有效的具体操作模式、文件接缝与高频陷阱。权威门禁规则始终以 [PR 快速门禁与定期全量设计](../../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 和 [CI 分层执行设计](../../../../doc/testing/ci/ci-tiered-execution.design.md) 为准。

## 1. 关键文件与职责接缝

| 文件 | 在 Job 优化中的职责 | 对应本地单测 |
| --- | --- | --- |
| `scripts/ci-required-scope.json` | 声明路径规则（`rules`）、受控控制路径（`scoped_control_paths`）、合并宿主映射（`group_jobs`）及各项 `*_version` 能力开关 | `scripts/plan-rust-required-scope.test.py` |
| `scripts/plan-rust-required-scope.py` | 计算受影响包/组、校验配置 schema、判定候选源码是否支持对应版本能力（`tested_source_supports_capability`），不满足安全条件时回退 `scope=full` | `scripts/plan-rust-required-scope.test.py` |
| `scripts/ci-contract-jobs.py` | 生成并校验 `.github/workflows/rust.yml` 中的托管合约 job 区块及版本跳过条件（`--write` / `--check`） | `scripts/ci-contract-jobs.test.py` |
| `.github/workflows/rust.yml` | 定义 `select`、合并宿主 job、条件跳过（`if:`）、产物传递与 `required-gate` / `explicit-full-gate` 依赖图 | `scripts/ci-workflow.test.py`、`scripts/resolve-product-doc-ci-range.test.py` |
| `scripts/ci-required-result.py` | 在 `required-gate` 中根据 `plan` 与 `*_version` 将逻辑检查组映射到实际必须成功的宿主 job 或直接分片 | `scripts/ci-required-result.test.py` |
| `scripts/ci-full-result.py` | 校验全量门禁收据与工作流定义摘要（`definition_digest`），升级任何 `*_version` 时必须同步允许集合 | `scripts/ci-full-result.test.py` |
| `scripts/ci-custody-platform.py` | Custody 拆片打包、不可变测试清单导出（`export-inventory`）与零检出完整性验证（`validate --context-file`） | `scripts/ci-custody-platform.test.py` |

## 2. 四大验证有效的缩减模式

### 模式 A：短合约分批并入同环境常驻 Job（`contract_job_consolidation_version`）
- **问题**：多个 10s–50s 的 Python/Shell/文档/打包契约组各占 1 个独立 job，冷启动与容器拉取时间远超检查本身。
- **做法**：
  - 将纯脚本/文档/轻量编译度量合约（如 `codex_agent_config_validation`、`macos_package_contract`、`doc_checker_contracts`、`compile_metrics`、`packaging_contracts`）并入固定 `ci-rust-build` 容器镜像的 `baseline`；
  - 将治理与运维脚本合约（如 `cargo_tooling_contracts`、`pr_governance_preview`、`operational_contracts`）并入 `workflow-governance`；
  - 将 Windows 专属短合约（`windows_rollout`）并入本来就要启动 Windows runner 的 `fleet-health (windows-2022)`；
  - 将普通非门禁控制脚本自测并入 `rust-baseline`（`ordinary_controls_in_rust_baseline_version`）；
  - 将需要 Chromium/Playwright 的静态站点检查（`site_quality`）并入已安装同款浏览器的 `viewer-js-required`，并在仅命中 `site_quality`（未命中 `viewer_js_required` 且非 `full`）时，用一致的 `if:` 跳过 Viewer 的 npm 依赖、Rust 工具链、缓存、`wasm-bindgen` 与原生库安装。
- **实测效果**：合入 `#6818` 后，`baseline` 吸收 3 组原 `doc-checker-contracts` 合约后的总耗时约 `195s–224s`（低于原先两 job 之和 `95s + 246s = 341s`），关键路径零增加，`scope=full` 直接减少多个独立 job。

### 模式 B：零检出门禁直验替代独立汇总 Job（`workspace_support_direct_gate_version`）
- **问题**：`workspace-support` 独立启动容器、检出完整仓库，仅为了下载 `custody-*` 分片结果并运行 `ci-custody-platform.py validate`，额外增加一跳串行排队和 1 个 job。
- **做法**：
  1. 在 `select`（本来已检出受测提交）阶段运行 `ci-custody-platform.py export-inventory` 将受测提交的不可变测试文件清单写入 `ci-authority` 产物，并一并拷贝受信脚本；
  2. 将 `custody-native-contracts`、`custody-native`、`custody-wasm-production`、`custody-node` 加入 `required-gate` 的 `needs:`；
  3. `required-gate` 下载 `ci-authority` 与分片结果后，直接通过 `--context-file` 零检出完成完整分片与清单校验（实测仅增加约 `4s`），在 `pr-fast` / `main-fast` 下完全跳过 `workspace-support`。
- **注意**：保持 `explicit-full`（`periodic-full` / `candidate-full`）仍走原汇总路径，避免无必要地改动全量收据 schema。

### 模式 C：同栈串行构建产物合一与主干队列合并
- **同栈生产者合并**：如将 `custody-wasm-approval` 合入 `custody-wasm-production`，一次准备 Rust/WASM 工具链与依赖缓存并同时产出 production 与 approval 两个隔离 bundle 产物，消除重复编译和 runner 占用。
- **主干 `main-fast` 准入与合并**：已通过等效 PR 门禁的提交合入 `main` 时，由 `main-fast-admission` 单 job（1 个 job）快速放行；连续合入通过受控 concurrency 合并排队（`main_fast_coalesce_concurrency_version`），并用历史退化路径桥接保障宽范围 diff 规划性能。

### 模式 D：精准依赖与受控配置切片（减少 `scope=full` 误触发）
- **Workspace 成员与条件目标依赖精细化**（`workspace_member_cargo_manifest_scope_version` / `scoped_production_cfg_version`）：解析成员 `Cargo.toml` 的 `[target.'cfg(...)'.dependencies]` 及生产源码受控 `#[cfg(...)]`，只唤醒实际目标平台消费方，而非一律升级为全仓构建。
- **受控 `ci-required-scope.json` 规则增删切片**（`ordinary_scope_config_slice_version`）：当 PR 仅编辑 `ci-required-scope.json` 的受控字段或 `rules` 时，只要满足：
  1. 所有保留规则保持严格相对顺序不变（不改变 first-match 遮蔽语义）；
  2. 被移除规则在 `before` 与 `after` 两个受追踪 Git tree 中均匹配 **0** 个文件；
  3. 新增规则满足 `not rule.get('full', False)` 且 `groups` 非空合法（命中 `workspace_support` 时同步传播 `support_rule`）；
  即可仅运行 `workflow_governance` + 受影响分组，将规则注册 PR 从 25+ 个 job 降至约 5 个 job；任一条件不满足立即 fail-closed 回退 `scope=full`。

## 3. 高频陷阱核对清单（提交前必查）

1. **宿主容器环境与 UID 一致性**：把原运行在 `ci-rust-build` 容器（`--user 1001:1001`）的组迁入其他 job 时，必须核对目标 job 是否具备同版本 Python 3.12、`markdown-it-py` venv、`jq`、`ruby`、`rustc`/`cargo`/`clippy` 以及 `git config --global --add safe.directory`。
2. **`baseline` 产品文档范围环境变量不变量**：`scripts/resolve-product-doc-ci-range.test.py` 明确断言 `baseline` job 内任何步骤不得出现 `OASIS7_PRODUCT_DOC_BASE:` 或 `OASIS7_PRODUCT_DOC_HEAD:` 步级 `env` 覆盖（该范围由前序步骤统一写入 `$GITHUB_ENV`）。向 `baseline` 迁入步骤时须删去多余的步级同名环境变量。
3. **Rust 缓存配对 `if:` 不变量**：`scripts/ci-workflow.test.py` 强制校验同一 job 中 `Stabilize Rust cache toolchains` 与 `Swatinem/rust-cache` 的 `if:` 表达式完全一致；按条件跳过重型构建时必须成对同步。
4. **`ci-workflow.test.py` 字符串替换范围**：为宿主 job 编写负向突变单测时，注意同一表达式可能在 job 级与多个 step 级重复出现，避免误用 `.replace(..., 1)` 只改到第一处而漏测后续分支。
5. **能力版本上限单测同步**：每次递增 `*_version` 或新增能力开关，同步更新 `scripts/plan-rust-required-scope.test.py` 中的非法版本号负向测试、迁移回退测试（需要时一并 `pop` 新增能力字段模拟旧基线）以及 `scripts/ci-workflow.test.py` 的 `validate_candidate_graph`。
