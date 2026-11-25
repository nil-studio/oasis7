# 分册：收紧按需选测边界、多平台切片与前置快速失败

本分册隶属 [optimizing-ci](../SKILL.md)，汇总 `#6925` 与 `#6930`（`#6934`、`#6938`、`#6939`、`#6940`、`#6941`）验证有效的四类按需跑测与快速失败手段、两步式可信测试前置切换方法及跨版本仿真防坑清单。权威门禁规则以 [PR 快速门禁与定期全量设计](../../../../doc/testing/ci/ci-pr-fast-and-periodic-full-design.md) 和 [CI 分层执行设计](../../../../doc/testing/ci/ci-tiered-execution.design.md) 为准。

## 1. 诊断信号：为什么 `scope=targeted` 仍然慢或昂贵

即使普通 PR 已进入 `scope=targeted`（未退化 `scope=full`），仍常存在四类高影响算力浪费：

1. **陈旧 `package_groups` 反向依赖映射与残留构建准备**：模块解耦后 `ci-required-scope.json` 仍保留历史包映射，使无依赖边的纯后端改动每次陪跑 4–5 分钟的前端 WASM 构建（如 `launcher-web`），或 Job 内残留早已无消费方的 `npm ci` 与 `resources` 声明。
2. **粗粒度连带触发与高倍率跨平台矩阵陪跑**：规划器硬编码连带选择（如改普通运维脚本无条件追加 `windows_rollout` + `fleet_health`），或多平台宿主将正交子契约绑死在三端全矩阵冷编译。按 GitHub Actions 计费倍率（Linux `1×`、Windows `2×`、macOS `10×`），单次 `3.4m` `macos-15` + `4.0m` `windows-2022` 即产生 **41.7 折算计费分钟**。
3. **前置门禁漏挂重编译宿主导致低级错误空跑**：极速前置门禁（如 `fast-preflight` 浅克隆 `cargo fmt`）若未完整覆盖所有重编译宿主（含承载合并组的宿主），一旦格式失败，漏挂宿主仍会空跑数分钟并在修复重推时被大面积取消。
4. **上下游同源检查重复执行**：前置门禁已对同一 `tested_sha`、同一容器镜像和同一工具链验证过的检查（如 `cargo fmt --all -- --check`），在下游重型基线 Job 中再次无条件重跑。

## 2. 关键文件与职责接缝

| 文件 | 在按需选测与前置门控中的职责 | 对应本地单测 |
| --- | --- | --- |
| `scripts/ci-required-scope.json` | 声明 `package_groups`、`resources`、路径 `rules`、`scoped_control_paths` 及子契约能力版本（如 `fleet_health_subcontracts_version`） | `scripts/plan-rust-required-scope.test.py` |
| `scripts/plan-rust-required-scope.py` | 基于真实 `cargo metadata` 依赖闭包选组，计算多平台子契约集合（`*_oses`、`*_checks`），不满足安全前提时保守回退全矩阵或 `scope=full` | `scripts/plan-rust-required-scope.test.py` |
| `.github/workflows/rust.yml` | 定义 `fast-preflight`、双处计划投影（`workflow-plan` 与 `&validate-complete-selection`）、动态 `matrix.os` 及步骤级 `if:` 守卫 | `scripts/ci-workflow.test.py` |
| `scripts/ci-tests.sh` | 执行组内具体检查命令，承接显式受控环境变量（如 `OASIS7_SKIP_RUSTFMT`）去重同源前置检查 | `scripts/ci-formal-process.test.py`、`scripts/ci-workflow.test.py` |
| `scripts/ci-python-control-contracts.py` | 在 `workflow-governance` 中总控 `ci-authority` 受信测试：`EVOLVING_POLICY_TESTS` 在 `policy_sha` 侧仅跑 `FIXED_TESTS` 红线子集并在候选侧跑完整新套件；其余契约套件在无全量策略迁移（`boundaries` 含 `ci-required-scope.json` / `ci-python-control-contracts.py`）时跑 `policy_sha` 全集，`FIXED_TESTS` 始终在受信侧强制执行且禁删改签名 | `scripts/ci-python-control-contracts.test.py` |
| `scripts/ci-required-result.py` | 在 `required-gate` 中强制所有被选中的逻辑检查组对应宿主结果为 `'success'`（因前置失败变为 `'skipped'` 时确定性判败） | `scripts/ci-required-result.test.py` |

## 3. 四大验证有效的按需与快速失败模式

### 模式 A：基于 `cargo metadata` 真实依赖闭包清理陈旧映射与冗余准备（`#6934`）
- **做法**：
  1. 用 `cargo metadata --no-deps` / `cargo tree --workspace -i <leaf_crate>` 核验终端产物 crate（如 `oasis7_client_launcher`）的真实传递依赖闭包，从 `ci-required-scope.json` `package_groups` 移除已解耦的非依赖包映射；
  2. 逐条核查对应 Job 步骤与 `resources` 声明，清理不再被构建脚本消费的安装步骤（如移除 `launcher-web` 内冗余的 `oasis7_viewer` `npm ci` 与残留 `"node"` 资源标签）；
  3. 在 `plan-rust-required-scope.test.py` 同时锁定**负例**（修改已解耦后端包不再选中该组）与**正例**（修改终端 crate 及其真实上游依赖包仍必选该组）。
- **实测效果**：纯后端 Rust PR 100% 跳过 4–5 分钟的前端 Trunk WASM 构建，轻量后端切片 PR 端到端墙钟收敛至约 `8` 分钟。

### 模式 B：解除粗粒度连带触发 + 多平台宿主二维子契约切片（`#6938`、`#6939`）
- **做法**：
  1. **显式规则替代硬编码捆绑**：先在 `ci-required-scope.json` 为此前依赖隐式捆绑才能覆盖的路径补齐独立显式规则（如将 `fleet_health_collector`、`windows_package_rollout_contract`、`local_signer_runbook` 从泛化运维目录拆出精确归属），再删除 `plan-rust-required-scope.py` 中 `if 'operational_contracts' in chosen: chosen.update(...)` 类无条件连带触发；
  2. **规划器输出二维子契约**：新增 `fleet_health_subcontracts_version: 1`，按实际命中风险计算最小必需的 `fleet_health_oses` 与 `fleet_health_checks`（例如 `host_cleanup` 仅需 `ubuntu-24.04` + `macos-15` 且无需 Rust 编译；`macos_signer` 仅需 `macos-15`；并入宿主的 `windows_rollout` 单独命中时仅唤醒 `windows-2022`）；
  3. **工作流矩阵与步骤双层守卫 + 保守回退**：在 `rust.yml` 同时用子契约收敛 `strategy.matrix.os` 与各子步骤 `if:`（纯脚本子契约跳过 Rust 工具链安装与冷编译）；当 `scope == 'full'`、`'explicit-full'` 或旧 `policy_sha` 缺少该版本能力时，完整回退三端全步骤执行。
- **实测效果**：普通运维脚本改动折算计费从 `50.2` 分钟降至约 `8.5` 分钟（`-83%`，macOS/Windows 计费归零）。

### 模式 C：`fast-preflight` 极速前置门控全覆盖重编译宿主（`#6925`、`#6940`、`#6941`）
- **做法**：
  - 对所有重编译宿主 Job（`rust-baseline`、`oasis7-test-build-candidate`、`oasis7-required`、`workspace-support-other`、`custody-native-contracts`、`custody-native`、`custody-wasm-production`、`distfs`、`node`［含合并 `consensus`］、`net`、`launcher-web`、`scenario-regression`［含合并 `pixel_world_bridge`、`cargo_tooling_contracts`］）统一接入（其中 `oasis7-required` 额外保留上游产物依赖 `oasis7-test-build-candidate, oasis7-test-build-trusted`）：
    ```yaml
    needs: [select, fast-preflight]
    if: >-
      always() && !cancelled() && needs.select.result == 'success' &&
      (needs.fast-preflight.result == 'success' || needs.fast-preflight.result == 'skipped') &&
      (<原有选测与合并组触发条件>)
    ```
- **关键边界**：
  - 非 Rust PR 与 `push` / `schedule` / `workflow_dispatch` 事件下 `fast-preflight` 为 `skipped`（0 秒等待），下游命中 `== 'skipped'` 正常执行，绝不误阻断非 Rust 改动；
  - **`inline` 旁路拓扑必须直连 consumer（`#6921` → `#6925`）**：当存在“构建 producer → 消费 consumer，且 `inline` 模式下 consumer 允许 producer 为 `skipped` 直接自建自测”的结构（如 `oasis7-test-build-candidate` → `oasis7-required`）时，仅给 producer 挂 `fast-preflight` 会产生反向漏洞——`fast-preflight` 失败把 producer 置为 `skipped`，反而满足了 `inline` 模式下 consumer 的 `(candidate=skipped, trusted=skipped)` 触发条件导致空跑。所有支持 `inline` 旁路的 consumer 必须直接把 `fast-preflight` 写入自身 `needs:` 与 `if:`，并在单测覆盖 `ordinary_execution == 'inline'` + `candidate == 'skipped'` + `fast-preflight == 'failure'` 负例；
  - Rust PR 格式失败时 `fast-preflight == 'failure'`，下游重编译宿主全部跳过；**切勿将 `fast-preflight` 直接加入 `required-gate` 的 `needs:`**——`ci-required-result.py` 检测到被选中的 `rust_baseline` 等组结果为 `'skipped'` 即确定性判败阻断。
- **实测效果**：格式失败 PR 端到端反馈从 `8.3` 分钟缩短至 `2.0–2.8` 分钟，无效计费消耗减少 `82%–86%`；`#6925` 合入后 14.7h 实测 7 次拦截 `0` 漏网。

### 模式 D：同源前置检查安全去重的“四重守卫”（`#6941`）
- 在下游 Job（如 `rust-baseline`）省去 `fast-preflight` 已通过的检查（如 `cargo fmt --all -- --check`）时，必须同时具备四道防线：
  1. **事件与前置成功守卫**：`github.event_name == 'pull_request' && needs.fast-preflight.result == 'success'`（保留本地 CLI、`main-fast`、`periodic-full`、`candidate-full` 完整执行）；
  2. **控制面与工具链变更回退守卫**：`&& fromJSON(needs.select.outputs.plan).scope != 'full'`（一旦 PR 修改 `rust-toolchain*`、`.cargo/**` 或工作流本身触发 `scope == 'full'`，自动回退在下游按候选工作区默认工具链完整重跑）；
  3. **容器镜像同源防漂移单测**：在 `ci-workflow.test.py` 断言前置与下游 Job 的 `container.image`（含 `@sha256:...` 摘要）完全一致，防止未来单侧升级镜像导致环境漂移；
  4. **Driver 默认开启、显式变量跳过**：在 `scripts/ci-tests.sh` 用 `if [[ "${OASIS7_SKIP_RUSTFMT:-0}" != 1 ]]; then ... fi` 包裹，确保无该环境变量的既有控制面契约测试（如非演进套件 `ci-formal-process.test.py` 中的 `test_format_failure_precedes_fetch_architecture_and_clippy`）及 `FIXED_TESTS` 保持原行为通过。

## 4. 受信控制面分流边界、两步式前置与 `FIXED_TESTS` 跨版本仿真

自 `#6928` 起，`scripts/ci-python-control-contracts.py` 将 `plan-rust-required-scope.test.py`、`ci-bin-ownership.test.py`、`ci-full-result.test.py`、`ci-workflow.test.py` 列入 `EVOLVING_POLICY_TESTS`：

1. **常规拓扑与选测演进默认单 PR 原子收口**：对 `EVOLVING_POLICY_TESTS`，`policy_sha` 侧仅强制执行 `FIXED_TESTS` 中的安全红线子集（如 `ci-workflow.test.py` 的 21 项安全不变量），完整更新后的套件在候选工作区运行。因此普通的 `needs:` 调整、投影字段扩展、步骤级 `if:` 改写可直接与 `.github/workflows/rust.yml` **在同一 PR 内同步更新**，无需无故拆微 PR。
2. **何时才需 `PR-A -> PR-B` 两步式兼容前置**：仅当改动需要调整 `policy_sha` 上 `FIXED_TESTS` 锁死的红线契约（含 `FIXED_TESTS['scripts/ci-required-result.test.py'] = None` 全集），或触及非 `EVOLVING_POLICY_TESTS` 契约套件（如 `ci-formal-process.test.py`、`ci-contract-jobs.test.py`）且本 PR 未触发全量策略迁移（`boundaries` 含 `scripts/ci-required-scope.json` 或 `scripts/ci-python-control-contracts.py`）时，才先提 **PR-A**（受信契约双态兼容前置，走 `scoped_control_paths` 快速合入更新 `policy_sha`，且不在 PR-A 提前写入依赖 PR-B 的断言）再提 **PR-B** 切换收口。
3. **推送前验证 `policy_sha` `FIXED_TESTS` 红线兼容性**：修改 `rust.yml` 前可在本地精确加载 `origin/main` 的 `FIXED_TESTS['scripts/ci-workflow.test.py']` 对当前候选工作区做秒级跨版本校验（改动涉及 `ci-full-result.py` 时一并核对 `FIXED_TESTS` 中的 `ci-full-result.test.py` 项）：
   ```bash
   python3 -c "
   import pathlib, subprocess, sys, types, unittest
   rmod = {}
   exec(subprocess.check_output(['git', 'show', 'origin/main:scripts/ci-python-control-contracts.py'], text=True), rmod)
   fixed = rmod['FIXED_TESTS']['scripts/ci-workflow.test.py']
   wf_src = subprocess.check_output(['git', 'show', 'origin/main:scripts/ci-workflow.test.py'], text=True)
   target = pathlib.Path('scripts/ci-workflow.test.py').resolve()
   sys.path.insert(0, str(target.parent)); sys.argv = [str(target)]
   mod = types.ModuleType('policy_workflow_fixed')
   mod.__file__ = str(target); sys.modules[mod.__name__] = mod
   exec(compile(wf_src, str(target), 'exec'), mod.__dict__)
   suite = unittest.defaultTestLoader.loadTestsFromNames(fixed, mod)
   raise SystemExit(0 if unittest.TextTestRunner(verbosity=1).run(suite).wasSuccessful() else 1)
   "
   ```
4. **受信基线断言写法、扩充 `EVOLVING_POLICY_TESTS` 与受管输入路由**：编写不锁死演进态的受信基线断言、提炼新 `FIXED_TESTS` 子集、治理 `UnknownInputsError` / CodeQL `unregistered input` 前置微 PR 以及门禁松绑后的流程惯性清理，详见 [受信基线与输入治理分册](trusted-baseline-and-input-governance.md)。

## 5. 标准本地闭环命令与防坑核对表

推送前按改动范围执行：

```bash
# 1. 选测规划器、工作流图与门禁结果回归
TMPDIR=/dev/shm python3 scripts/plan-rust-required-scope.test.py
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py
python3 scripts/ci-required-result.test.py
python3 scripts/ci-python-control-contracts.py local-governance

# 2. 若修改了 doc/testing/ci/*.md 等设计文档（需装有 scripts/doc-governance-requirements.txt 的 markdown-it-py==3.0.0）
./scripts/doc-governance-check.sh
python3 scripts/system-design-traceability-check.test.py

# 3. PR 切片边界与快速基线
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates none
python3 scripts/check-local.py --profile quick --base-ref origin/main
```

提交前逐项核对 7 个高频陷阱：
1. **`rust.yml` 双投影列表必须对称同步**：向 `selection.json` 新增任何由工作流消费的字段时，必须同时更新 `select` 的 `workflow-plan` 步骤与门禁共用的 YAML 锚点 `&validate-complete-selection` 两处 `fields` 列表、多键同现校验（`any` + `not all`）和类型/白名单检查，并在 `ci-workflow.test.py` 的 `test_large_workflow_projection_preserves_fields_and_binds_complete_file_and_identity` 登记该 `fromJSON(needs.select.outputs.plan).<key>`。
2. **单行 `if:` 改多行 `if: >-` 破坏静态子串断言**：给既有步骤追加条件前，先在 `scripts/ci-workflow.test.py` 检索该步骤名及 `runner.os ==`，将硬编码 `assertIn("if: runner.os == 'Linux'", step)` 提前放宽为正则 `r"if:\s*(?:>-\s*)?runner\.os == 'Linux'"`（注意单引号 `'` 非单词字符，末尾勿加 `\b`）。
3. **合并组宿主的连带门控覆盖**：检查宿主 Job（如 `node` 承载 `consensus`、`scenario-regression` 承载 `pixel_world_bridge` 与 `cargo_tooling_contracts`、`fleet-health` 承载 `windows_rollout`）时，单测必须分别构造“宿主原组未选中、仅合并子组选中”的组合输入验证 `if:` 求值，防止漏门控或误跳过合并子组。
4. **`plan-rust-required-scope.test.py` 临时 Cargo 夹具硬约束**：`cargo_graph` 要求测试夹具根 `Cargo.toml` 显式列出具体 `members = [...]` 路径（不支持 `"crates/*"` 通配）并包含 `[workspace.dependencies]`（推荐基于 `self.ROOT_TOML` 派生），且夹具内所有文件路径须命中已注册规则以免抛 `UnknownInputsError`。
5. **`FIXED_TESTS` 方法身份与默认行为不可退化**：修改 `scripts/ci-tests.sh` 或控制面测试时，核对 `scripts/ci-python-control-contracts.py` 中的 `FIXED_TESTS` 清单，不得删除受信固定测试方法名或改变无新增环境变量时的默认执行顺序。
6. **独立 `eval()` 测试与跨子目录字面量断言（`#6831`、`#6926`）**：给 job 的 `if:` 新增 `needs.<gate>.result` 时，除共享 `effective_job_condition` 外，须排查 `ci-workflow.test.py` 中绕过该 helper 的独立 `eval()` 测试（如 `test_consumers_require_their_actual_dependency_and_handle_cancellation`）避免 `NameError`；修改 `required-gate` 等核心条件字面量后，须用 `rg` 搜遍整个 `scripts/`（含 `scripts/security/codeql-rust-model-diagnostic.test.py` 等 `run_governance_remaining` 套件）。
7. **PR 标题/正文必须显式声明 `Slice-Type`**：开 PR 时在标题加 `[Slice: <Type>]` 或正文加 `Slice-Type: <Type>`（如 `Slice-Type: Chore` / `Docs`），避免 `Slice, Scope-Lock & Architecture Ring Gate` 在 20 秒内直接判红。
