# 分册：容量预算、取消浪费归因与主干节流

本分册聚焦仓库级 CI 算力预算治理：如何用真实遥测拆分取消浪费、如何在不漏测和不损害主线修复时效的前提下压缩主干 `main-fast` 算力，以及如何在合入后做 12–24 小时生产复测闭环。

## 1. 取消浪费的三分法归因口径

分析 GitHub Actions `cancelled` 消耗时，**不能只统计 `conclusion == 'cancelled'` 的 run 次数**，必须拉取 job 级明细（`actions/runs/{id}/jobs`）按时间戳切分为三类：

1. **0-job 排队合并取消（非浪费）**：启用 `cancel-in-progress: false` 的分支（如 `main` `push`）在连续合入期间由 GitHub 自动替换队列中的旧 `pending` 项；此类 run 活跃 job 数为 `0`，真实消耗 `0.0 min`。
2. **先失败后取消（Fail-before-cancel，核心治理大头）**：同一 run 内已有前置/轻量 job 失败（其 `completed_at` 早于被取消 job 的最早终止时间 >10s），但并行重型 job 继续空跑，直到开发者数分钟后推修复 commit 将整轮打断。实测 `2026-11-23` 基线中，此类占 PR 取消浪费的 **79.8%**（`2,206.3 min/天`，若含同 run 已完成重型 job 则无效算力达 `6,777.4 min/天`）。治理手段见 [按需选测与快速失败分册 §3 模式 C](on-demand-scope-and-preflight.md#模式-cfast-preflight-极速前置门控全覆盖重编译宿主692569406941)。
3. **无失败纯中途 push/rebase 打断**：活跃 Ready PR 在尚未出现失败时每 3–15 分钟连续推小改动或同步 `main`。治理手段为多轮迭代期间保持 **Draft PR**（`rust.yml` 已跳过 Draft 重型 job）并在本地完成批次自测后再成批推送。

## 2. 主干冷却预算模式（`main-fast-admission`，`#6831`）

当 `main` `push` 已通过 `cancel-in-progress: false` + `resolve_main_fast_base` 将半程取消浪费归零，但高频合入仍导致“上一轮刚结束立刻启动下一轮”（约每 50 分钟一轮、日均 ~5,400 runner-minutes）时，在 `select` 前增加轻量冷却准入 job（`main-fast-admission`，运行于预构建 `ci-runtime` 镜像，单次延后仅耗 ~15 秒），并严守四条不变量：

1. **按最近真实绿跑完成时间计冷却**：`scripts/ci-full-result.py admit-main-fast` 默认设置 `MAIN_FAST_COOLDOWN_SECONDS = 7200`（120 分钟），从最近一次**真实执行并成功**的 `main` `push` run 完成时间（`updated_at`）起算，自动过滤 `cancelled`、`skipped` 与冷却延后 run。
2. **显式延后标记产物与回溯隔离**：GitHub Actions 中只要 `main-fast-admission` 成功，整次 run 的 `conclusion` 即为 `success`。因此冷却延后时**必须上传显式标记产物**（`ci-main-fast-deferred-${{ github.run_id }}-${{ github.run_attempt }}`），并在 `resolve_main_fast_base()` 与 `admit_main_fast()` 中通过 `actions/runs/{id}/artifacts` 跳过携带该前缀产物的 run，确保下一轮放行的 `main-fast` 自动回溯到上一次真实验证通过的 `main` 提交，绝不漏测任何合入提交。
3. **失败恢复自动绕过冷却**：若最近一次真实执行的 `main-fast` 结论非 `success`（如 `failure`），`admit_main_fast` 返回 `run_main_fast=true`（`reason=recover_after_non_success`），确保主线修复提交立即验证而不卡冷却窗口。
4. **`required-gate` 静默跳过且不扩充 `needs:`**：`main` `push` 被冷却延后导致 `select` 为 `skipped` 时，`required-gate` 通过 `if:` 条件 `(github.ref != 'refs/heads/main' || needs.select.result != 'skipped')` 同步跳过；**不向 `required-gate` 的 `needs:` 追加 `main-fast-admission`**，保持与 `ci-required-result.py` 额外依赖校验完全兼容。

## 3. 合入后 12–24 小时生产遥测验收闭环

容量优化 PR 合入 `main` 后，须拉取合入后 **12–24 小时** 真实 `rust.yml` 运行及 job 耗时复测，对照优化前基线核验五项实证指标：

- **主干 24h 折算算力与延后率**：`main-fast` 折合 24h runner-minutes、冷却延后占比与 `main` `push` 取消耗时（`#6831` 实测 14.7h：`main-fast` 从 ~5,400 min/天降至 **~2,099 min/天 / `-61.1%`**，65 次 push 中 40 次冷却延后、19 次排队合并、仅 6 次实际执行，取消耗时 `0.0 min`）。
- **失败恢复绕过实证**：确认真实 `main-fast` 失败后的下一次 `main` push 未受 120 分钟冷却阻塞。
- **前置门禁拦截与零漏网率**：统计 `fast-preflight` 均值耗时、实际拦截失败次数，以及失败 run 中受控重型 job（含 `inline` 模式 `oasis7-required` 与 `rust-baseline`）的漏网数为 **0**（`#6831` + `#6925` 实测 14.7h：均值 `49s`，拦截 11 次，`#6925` 后 7 次拦截 `0` 漏网）。
- **PR 取消与失败占比变化**：核对 PR `cancelled` 与 `failure` 耗时占 PR 总算力的比例是否实质下降（实测 PR 取消浪费从 `2,765 min/天` / `12.1%` 降至折合 `~511 min/天` / `4.0%`，降幅 **-81.5%**）。
- **相关契约单测**：
  ```bash
  python3 scripts/ci-full-result.test.py
  TMPDIR=/dev/shm python3 scripts/ci-workflow.test.py PriorityRunnerRoutingContractTests
  ```

## 4. 全仓容量盘点的三项联动盲区（Runner 配额、缓存池与冷却耦合）

做日度/滚动 24h 全仓容量审计时，除统计核时与取消率外，必须联查以下三个维度：

1. **异构 Runner 并发槽位配额与排队分位（`q_p50 / q_p90 / q_p99`）**：
   - 不仅核算 `Linux 1× / Windows 2× / macOS 10×` 计费等效分钟，还须按 1 分钟粒度统计各 OS 并发槽位峰值及 `job.started_at - job.created_at` 排队延迟；
   - Hosted `macos-15` 并发上限通常仅 **5 槽位**：实测 `macos-15` 物理耗时仅占全仓 `3.7%`，却可占全仓 **`27.7%`** 计费分钟并频繁打满 5 槽位，拖慢关键路径验签（如 `fleet-health (macos-15)`）排队。审计控费与排队时优先排查占满低配额池的非核心或已暂停桌面任务（如 `#6987` 下线已暂停的 `command-center` 直接释放全仓 `20.7%` 计费分钟与 macOS 槽位）。
   - **API 翻页截断防护**：GitHub Actions `actions/runs` 搜索单查询上限为 **1,000 条**；高吞吐日（`>1,500 runs/24h`）必须按 `created` 时间窗切分（如每 6–8 小时一段）拉取去重，避免静默漏算早间样本。
2. **仓库 20 GB 缓存池家族水位与镜像升级瞬态膨胀**：
   - 通过 `gh api --paginate 'repos/{owner}/{repo}/actions/caches?per_page=100' --jq '.actions_caches[]'` 拉取全部页的缓存记录后按 key 前缀家族聚合体积，监控总用量距 **20 GB** 硬上限的安全余量（建议保留 `>= 3–4 GB` 缓冲），防止触发 GitHub 无序 LRU 驱逐把核心热缓存（如 `ordinary-workspace-v1`、`ci-shared-native-trusted`）挤出；容量压力下的休眠历史环境分级回收与跨环境配对见 [编译产物缓存与镜像加速分册 §3 模式 D](build-artifact-cache-and-image-acceleration.md#模式-d工作区组件缓存高命中率治理与跨环境严格配对6315649265396547658367326929693569686969)；
   - 连续升级 `ci-rust-build` 容器镜像摘要期间，`v0-rust-ci-rust-build-*` 会短暂并存多个旧摘要条目（可达 `5–6 GB`），核实 `ci-cache-maintenance.py` 按休眠窗口正常回收即可，无需手工误删活跃底座。
3. **缓存输入轮换 × `main-fast` 冷却窗口的联动排查（`#6969` 实证）**：
   - 当某个平时稳定的构建 Job（如 `oasis7-test-build-candidate`）耗时突然集体翻倍逼近 `timeout-minutes` 红线（如从 `8–12m` 飙升至 `26–37m / 40m`）时，**切勿盲目只涨超时上限**；
   - 优先联查“近期是否合入了改变容器镜像摘要、工具链或缓存前缀的提交”**且**“`main-fast-admission` 正处于 120 分钟冷却窗口延后了 `main` writer 落盘新缓存”——若新缓存尚未由 `main` 写入，所有并发 PR 都会退化为全冷构建。此时应确保缓存输入轮换时自动绕过主干冷却立即热库，待首轮 `main` writer 落盘后再复核 PR 耗时是否回落。
