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
| `scripts/ci-python-control-contracts.py` | 在 `workflow-governance` 中以 `policy_sha` 受信测试检验候选工作流与控制脚本（控制 `migration` 委托与 `FIXED_TESTS` 强制集） | `scripts/ci-python-control-contracts.test.py` |
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
  - 对所有重编译宿主 Job（`rust-baseline`、`oasis7-test-build-candidate`、`oasis7-required`、`workspace-support-other`、`custody-native-contracts`、`custody-native`、`custody-wasm-production`、`distfs`、`node`［含合并 `consensus`］、`net`、`launcher-web`、`scenario-regression`［含合并 `pixel_world_bridge`、`cargo_tooling_contracts`］）统一接入：
    ```yaml
    needs: [select, fast-preflight]
    if: >-
      always() && !cancelled() && needs.select.result == 'success' &&
      (needs.fast-preflight.result == 'success' || needs.fast-preflight.result == 'skipped') &&
      (<原有选测与合并组触发条件>)
    ```
- **关键边界**：
  - 非 Rust PR 与 `push` / `schedule` / `workflow_dispatch` 事件下 `fast-preflight` 为 `skipped`（0 秒等待），下游命中 `== 'skipped'` 正常执行，绝不误阻断非 Rust 改动；
  - Rust PR 格式失败时 `fast-preflight == 'failure'`，下游重编译宿主全部跳过；**切勿将 `fast-preflight` 直接加入 `required-gate` 的 `needs:`**——`ci-required-result.py` 检测到被选中的 `rust_baseline` 等组结果为 `'skipped'` 即确定性判败阻断。
- **实测效果**：格式失败 PR 端到端反馈从 `8.3` 分钟缩短至 `2.0–2.8` 分钟，无效计费消耗减少 `82%–86%`。

### 模式 D：同源前置检查安全去重的“四重守卫”（`#6941`）
- 在下游 Job（如 `rust-baseline`）省去 `fast-preflight` 已通过的检查（如 `cargo fmt --all -- --check`）时，必须同时具备四道防线：
  1. **事件与前置成功守卫**：`github.event_name == 'pull_request' && needs.fast-preflight.result == 'success'`（保留本地 CLI、`main-fast`、`periodic-full`、`candidate-full` 完整执行）；
  2. **控制面与工具链变更回退守卫**：`&& fromJSON(needs.select.outputs.plan).scope != 'full'`（一旦 PR 修改 `rust-toolchain*`、`.cargo/**` 或工作流本身触发 `scope == 'full'`，自动回退在下游按候选工作区默认工具链完整重跑）；
  3. **容器镜像同源防漂移单测**：在 `ci-workflow.test.py` 断言前置与下游 Job 的 `container.image`（含 `@sha256:...` 摘要）完全一致，防止未来单侧升级镜像导致环境漂移；
  4. **Driver 默认开启、显式变量跳过**：在 `scripts/ci-tests.sh` 用 `if [[ "${OASIS7_SKIP_RUSTFMT:-0}" != 1 ]]; then ... fi` 包裹，确保无该环境变量的 `FIXED_TESTS`（如 `ci-formal-process.test.py` 中的 `test_format_failure_precedes_fetch_architecture_and_clippy`）保持原行为通过。

## 4. 两步式可信测试前置切片（PR-A -> PR-B）与跨版本本地仿真

`workflow-governance` 会运行 `ci-python-control-contracts.py`，用 **`policy_sha`（当前 `origin/main`）上的受信测试检验候选分支的 `.github/workflows/rust.yml`**。当 `rust.yml` 修改触及 `ci-workflow.test.py` 中未按 `*_version` 分支的硬编码断言（如 `needs:` 允许集、静态 `fromJSON(needs.select.outputs.plan).*` 白名单、步骤级 `if:` 文本匹配）时：

1. **PR-A（可信测试双态兼容前置）**：先单独（或随规划器新增字段）修改 `scripts/ci-workflow.test.py`，使其同时接受旧版与新版 `rust.yml` 写法。纯改 `scripts/ci-workflow.test.py` 命中 `scoped_control_paths`，仅执行 `baseline` + `workflow-governance`（约 5–8 分钟极速合入）。**严禁在 PR-A 提前写入必须依赖 PR-B 的 `rust.yml` 或 `ci-tests.sh` 改动才能通过的断言**。
2. **PR-B（工作流与 Driver 切换收口）**：待 PR-A 合入 `main` 成为新 `policy_sha` 后，提交 `.github/workflows/rust.yml`、`scripts/ci-tests.sh` 与设计文档变更，同时在 `scripts/ci-workflow.test.py` 移除过渡期旧写法兼容并补齐新执行契约单测。
3. **推送前必跑跨版本 `policy_sha` 内存仿真**：在 PR-B 本地工作区直接加载 `origin/main`（或本地预备 PR-A 分支）的 `ci-workflow.test.py` 检验当前候选文件，提前暴露所有 `policy_sha` 冲突：
   ```bash
   python3 -c "
   import pathlib, subprocess, sys, unittest
   src = subprocess.check_output(['git', 'show', 'origin/main:scripts/ci-workflow.test.py'], text=True)
   target = pathlib.Path('scripts/ci-workflow.test.py').resolve()
   sys.path.insert(0, str(target.parent)); sys.argv = [str(target)]
   g = {'__file__': str(target), '__name__': 'policy_workflow_test'}
   exec(compile(src, str(target), 'exec'), g)
   suite = unittest.TestSuite(); loader = unittest.defaultTestLoader
   for name, obj in g.items():
       if isinstance(obj, type) and issubclass(obj, unittest.TestCase) and name != 'SupportCapacity':
           suite.addTests(loader.loadTestsFromTestCase(obj))
   raise SystemExit(0 if unittest.TextTestRunner(verbosity=1).run(suite).wasSuccessful() else 1)
   "
   ```

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

提交前逐项核对 5 个高频陷阱：
1. **`rust.yml` 双投影列表必须对称同步**：向 `selection.json` 新增任何由工作流消费的字段时，必须同时更新 `select` 的 `workflow-plan` 步骤与门禁共用的 YAML 锚点 `&validate-complete-selection` 两处 `fields` 列表、多键同现校验（`any` + `not all`）和类型/白名单检查，并在 `ci-workflow.test.py` 的 `test_large_workflow_projection_preserves_fields_and_binds_complete_file_and_identity` 登记该 `fromJSON(needs.select.outputs.plan).<key>`。
2. **单行 `if:` 改多行 `if: >-` 破坏静态子串断言**：给既有步骤追加条件前，先在 `scripts/ci-workflow.test.py` 检索该步骤名及 `runner.os ==`，将硬编码 `assertIn("if: runner.os == 'Linux'", step)` 提前放宽为正则 `r"if:\s*(?:>-\s*)?runner\.os == 'Linux'"`（注意单引号 `'` 非单词字符，末尾勿加 `\b`）。
3. **合并组宿主的连带门控覆盖**：检查宿主 Job（如 `node` 承载 `consensus`、`scenario-regression` 承载 `pixel_world_bridge` 与 `cargo_tooling_contracts`、`fleet-health` 承载 `windows_rollout`）时，单测必须分别构造“宿主原组未选中、仅合并子组选中”的组合输入验证 `if:` 求值，防止漏门控或误跳过合并子组。
4. **`plan-rust-required-scope.test.py` 临时 Cargo 夹具硬约束**：`cargo_graph` 要求测试夹具根 `Cargo.toml` 显式列出具体 `members = [...]` 路径（不支持 `"crates/*"` 通配）并包含 `[workspace.dependencies]`（推荐基于 `self.ROOT_TOML` 派生），且夹具内所有文件路径须命中已注册规则以免抛 `UnknownInputsError`。
5. **`FIXED_TESTS` 方法身份与默认行为不可退化**：修改 `scripts/ci-tests.sh` 或控制面测试时，核对 `scripts/ci-python-control-contracts.py` 中的 `FIXED_TESTS` 清单，不得删除受信固定测试方法名或改变无新增环境变量时的默认执行顺序。
