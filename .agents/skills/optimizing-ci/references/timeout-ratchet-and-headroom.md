# 分册：`timeout-minutes` 棘轮收紧、冷热余量校准与多层超时不变量

本分册隶属 [optimizing-ci](../SKILL.md)，汇总全仓 27 个工作流 / 137 个 Job 显式超时覆盖与棘轮收紧校准（`#5374`、`#6424`、`#6770`、`#6966`）验证有效的采样去偏口径、线上超时击穿识别法、异构矩阵表达式分档、三层超时单调性不变量与双层棘轮测试闭环。

## 1. 诊断信号与防幸存者偏差采样口径

收紧或复盘 `timeout-minutes` 时，**严禁仅凭近期少量热缓存成功样本的 `p50 / p90` 直接下压上限**。建立容量水位基线必须同时覆盖四类合法尾部场景：

1. **剔除付费大规格 Runner 样本**：统计耗时分位前必须过滤 `oasis7-priority-linux` 执行记录，仅保留标准 Hosted Runner（`ubuntu-24.04`、`windows-2022`、`macos-15`）样本，防止高配 Runner 拉低实测耗时并掩盖标准机器上的超时风险。
2. **冷缓存与周期性缓存过期重编峰值**：依赖定时预热或共享缓存的任务（如 `hosted-login-linux-build.yml`、`oasis7-test-build-candidate`、Windows `compile-metrics`、`rust-cache.yml`）一旦缓存 miss、输入哈希轮换或满 7 天未访问过期，冷构建耗时可达热缓存的 **`1.6×–3.0×`**（实测 `hosted-login` 热跑 `<25m`、冷跑 `40.4m` 击穿 `30m`；Windows `compile-metrics` 热跑 `24.6m`、冷跑 `43.0m` 击穿 `35m`）。无独立降级路径的门禁 Job 必须按冷跑或 `scope=full` 峰值核定上限。
3. **`scope=full` / `test_policy_changed=true` 保守回退路径**：合并多个短合约后的常驻宿主（如 `workflow-governance` 合并 5 组、`rust-baseline` 合并 `controls`、`viewer-js-required` 合并 `viewer_performance` + `site_quality`）在 `scope=full` 下会串跑全部子检查，且 `oasis7-required (all)` 在 `test_policy_changed=true` 时退回单机全特性串跑。必须按 `full` 回退实测峰值留足余量。
4. **受控进程内单次故障重试放大**：具备偶发故障自动重试包装的任务（如 `custody-node` 捕获 `workerd` 偶发 `SIGSEGV` 单次重跑），单分片合法峰值可达常态耗时的 **`~1.8×–2.0×`**（常态 `9.4m`、触发单次重试时 `14.7m`），上限须容纳“正常执行 + 1 次受控重试”。

### 推荐余量健康带

| 任务类型 | 峰值利用率（`Max / timeout-minutes`） | 安全余量建议 | 处置原则 |
| --- | --- | --- | --- |
| **高频 PR 门禁宿主 / 重编译分片** | `60%–78%` | `>= 1.25×–1.35× Max` 且绝对缓冲 `>= 3–5m` | `> 85%`（橙区）跟踪观察；`> 90%` 或剩余 `< 1m`（红区）立即回调恢复余量 |
| **纯脚本 / 门禁汇总短任务**（`< 3m`） | `<= 60%` | 固定 `3m–5m` 下限 | 容器拉取或 GitHub API 偶发抖动即可波动 `30–90s`，不再向 `1–2m` 极限压缩 |
| **周期全量 / 发版打包 / 手工长压测** | `65%–80%` | `>= 1.25×` 冷跑峰值（参数化压测对齐最大支持档位） | 受内层监督器秒级预算约束时，外层 YAML 额外留 `5–10m` 归档与清理缓冲 |

---

## 2. 线上真实超时击穿的识别法（区分普通取消与超时强杀）

**核心诊断盲区**：当 GitHub Actions Job 运行达到 `timeout-minutes` 被平台强杀时，REST API 返回的 Job `conclusion` 为 **`cancelled`**（而非 `timed_out` 或 `failure`），随后下游 `required-gate` / `explicit-full-gate` 因上游非 `success` 报 `failure`。若统计脚本将所有 `cancelled` 笼统归因为“开发者推新 commit 并发取消”，会完全漏掉线上真实超时事故。

**精确识别规则**：拉取 `actions/runs/{id}/jobs` 明细，筛选同时满足以下三项条件的 Job 记录：
1. `job.conclusion == 'cancelled'`，且 `elapsed_minutes = (completed_at - started_at) >= configured_timeout_minutes - 0.5`；
2. 所属 workflow run 并非在相近秒数内所有活跃 Job 被同时批量取消（排除 `concurrency` 整单打断）；
3. 若为 `${{ ... }}` 动态表达式超时，按该矩阵实例的 `job.name` 分片名还原其实际生效档位后再比对 `elapsed_minutes`。

---

## 3. 同 Job 异构矩阵分片的表达式分档超时（`#6966`）

当测试瘦身或职责拆分使同一 `strategy.matrix` 内不同分片的真实耗时相差 **`2×` 以上**时，单一静态整数 `timeout-minutes` 会陷入“迁就重分片则轻分片挂起锁死数十分钟槽位、迁就轻分片则重分片频繁超时挂单”的两难。

- **实证案例（`rust.yml :: oasis7-required`）**：
  - 容器产物消费轻分片（`native-0..3`、`integrations-0..1`）瘦身后实测峰值仅 `13.2m–15.8m`；
  - 重分片 `runtime`（`27.1m`）、`component-libs`（`35.3m` 击穿原 `35m`）及 `test_policy_changed` 回退分片 `all`（`35.25m` 击穿原 `35m`）显著更重。
- **标准写法**：使用 GitHub Actions 表达式按 `matrix` 维度显式分档，无需拆分重复的 YAML Job 定义：
  ```yaml
  timeout-minutes: ${{ (matrix.partition == 'component-libs' || matrix.partition == 'runtime' || matrix.partition == 'all') && 50 || 25 }}
  ```
- **适用边界**：分档分支保持在 `2–3` 档以内，所有分支字面量必须是显式正整数，并同步接入下方 §5 的表达式全分支棘轮测试。

---

## 4. 三层超时单调性不变量（防内外层超时倒挂）

仓库工作流普遍存在三层超时控制，调整任意一层时必须满足**严格单调递增**：

$$\text{内层脚本监督器 deadline} < \text{Step } \texttt{timeout-minutes} < \text{Job } \texttt{timeout-minutes}$$

1. **严禁 Step `timeout-minutes` `>=` Job `timeout-minutes`（`#6966` 修复多处倒挂）**：
   - 若 Job 级已收紧至 `10m`，而内部某构建/测试步骤仍写 `timeout-minutes: 15`，一旦该步骤卡死，Job 级超时会直接强杀整个 Runner 进程，导致后续 `if: always()` 的故障日志收集、崩溃栈抓取（如 `workerd` gdb capture）与缓存/临时目录清理步骤全部被跳过。
   - **硬约束**：同一 Job 内任一 Step 的 `timeout-minutes` 必须严格小于该 Job `timeout-minutes` 的最小可能取值（至少预留 `1–2m` 给 `if: always()` 收尾步骤）。
2. **严禁外层 YAML 超时 `<=` 脚本内部监督器预算**：
   - 如 `storage-acceptance.yml` 的 `verify`（内层 `7200s = 120m`，外层 Job 须保持 `130m`）、`attempt-release-reference`（内层 `2400s = 40m`，外层须保持 `50m`）、`codeql.yml`（内层 `codeql-rust-action-deadline.cjs` 预留收尾窗口）及 `ci_process_runner.py` 监督组。若未同步下调内层脚本秒级上限，不得单方面压低外层 YAML `timeout-minutes`。

---

## 5. 双层超时棘轮契约测试闭环

为兼顾“`policy_sha` 安全底线不可退化”与“单 PR 灵活校准具体数值”，超时契约采用双层守护：

1. **`policy_sha` 固定红线层（存在性守卫）**：
   - `scripts/ci-workflow.test.py` 中的 `AllWorkflowJobsTimeoutGuardTests.test_every_executable_workflow_job_declares_timeout_minutes` 位于 `FIXED_TESTS`：强制 `.github/workflows/*.yml` 中每个非 `uses:` 可执行 Job 都显式声明合法 `timeout-minutes`（彻底封堵 GitHub 默认 `360m` 无界敞口），但不硬编码具体分钟数，允许校准 PR 单步合入。
2. **候选演进棘轮层（数值上限与倒挂守卫）**：
   - `StablePrJobTimeoutContractTests` 与 `ReleasePackagingJobTimeoutContractTests`（演进侧执行）逐工作流、逐 Job 锁定允许的最大上限 `max_minutes`，新增工作流或可执行 Job 时必须同步纳入棘轮字典，防止无意回退到宽泛值；
   - 解析 `timeout-minutes` 时同时支持整数字面量与 `${{ ... }}` 条件表达式（提取表达式中所有整数字面量分支，断言分支非空、`min(values) > 0` 且 `max(values) <= max_minutes`），并校验同 Job 内所有 Step `timeout-minutes` 均小于 `min(values)`。

---

## 6. 标准本地验证闭环

修改任何 `.github/workflows/*.yml` 的 `timeout-minutes` 或超时棘轮测试后，推送前执行：

```bash
# 1. 验证全仓超时存在性红线、数值棘轮上限与 Step/Job 非倒挂契约
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py \
  AllWorkflowJobsTimeoutGuardTests \
  StablePrJobTimeoutContractTests \
  ReleasePackagingJobTimeoutContractTests -v

# 2. 若涉及 CodeQL / Storage Acceptance / ECS 度量等专用超时脚本，运行对应契约单测
python3 scripts/security/codeql-workflow.test.py
python3 scripts/ci-storage-acceptance.test.py
python3 scripts/ci-ecs-build-metrics.test.py

# 3. 完整工作流图回归与 PR 边界检查
TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py
./scripts/check-pr-boundaries.sh --slice Chore --allowed-crates none
git diff --check
```
