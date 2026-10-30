# Native 缓存复用诊断

本入口用于区分缓存下载命中、工作区恢复成功和 Cargo 实际复用。它是手动实验，不是新的必需检查，不替代普通测试、Formal、全量回归或发布验收。

Trace refs: O7-REQ-d9292810-5229-417f-9fc8-262ee04759c7。

## 运行

合入并完成适用独立评审后，在 GitHub Actions 的 **Native Cache Diagnostic** 选择 `main`，手动运行一次 `ordinary` 或 `formal`。PR 事件只运行采集器的 Python 测试，不启动 Rust 编译。入口没有定时触发，也不会自动重试。

工作流读取 `rust.yml` 的顶层字面量环境，使用相同的 pinned toolchains、依赖缓存族和工作区恢复器；环境格式或字段变化时要求重新核对，不静默换用另一套配置。诊断代码从 `ci-test-artifacts.py` 读取选定生产路径的第一个 build variant：ordinary 当前为 viewer，formal 当前为 formal-required。只编译该 variant，不运行测试或构建整个工作区。

缓存恢复后，同一命令在同一 target 运行两次，分别记录 `after_restore_attempt` 和 `same_job_warm`。首次命令前仅在子进程注入 Cargo fingerprint 日志，避免污染缓存查找身份。第二次编译是同机热对照，**不是跨 runner 复用证据，也不是冷构建基准**。

仓库缓存权限为只读；依赖 action 使用 `save-if: false`，工作区仅调用 restore。入口不调用 stage、save、清理、删除或扩容接口，不改变正式生产者的可信写入条件。新下载的依赖和编译产物只存在于一次性 runner。

## 读取结果

下载本次 `native-cache-diagnostic-*` artifact，结合该次依赖缓存 action 的日志检查：

- `workspace-identity.json`、`workspace-restore.json`：请求与匹配 key、实际恢复状态及输入刷新范围。Action 的 exact-hit 不能代替恢复成功或编译复用。
- `build.json`：源码 SHA、Cargo.lock 摘要、工具链、实际命令、两阶段本地/第三方编译单元 fresh/rebuilt 数、耗时、退出码及 fingerprint 分类。计数是本次命令的编译单元观察值，不是去重 crate 数。

两次命令的源码、参数和 target 相同，但第一次会写编译产物。首次大规模重编译而第二次大多 fresh，说明同机复用可用；仍须结合 fingerprint 和真实匹配缓存判定跨运行失效原因。两次都重编译时继续检查输入、构建脚本、依赖特性或 fingerprint；不要自动扩大缓存容量。缓存缺失、输入变更或生产者版本不同时，重编译可能是合理结果。

采集器只保留已知原因判别项和受限的包名/版本字段，不保存原始 stderr、路径、环境值或源码。无法识别的记录标为 `unclassified_*`，不能补写成已知根因。`fingerprint_error` 本身不证明文件不存在。原因计数是日志行观察值，不能和编译单元数量一一对应。

每个流最多解析 16 MiB，每行最多 64 KiB，每阶段最多保存 128 个原因样本；超出部分仍被排空并显式计数。stdout 丢失、解析不完整或没有编译单元时，`unit_counts_complete=false`；不能当成“零重编译”。采样截断不改变已经统计的原因计数。命令失败保留失败码并停止热对照；超时返回 124 并清理进程组。两阶段共享 1200 秒内部时限，整个 job 上限为 35 分钟。

缓存的共享依赖 key 尚不由本脚本从 action 日志推导；需要读同次 job 的实际 requested/matched key。Formal 单包预算和共享依赖生产任务失败是独立维度，本入口不修改其策略。修复应由诊断事实决定，修复后的真实消费者运行才是效果验收。

## 本地验证

```bash
python3 -B scripts/ci-cache-diagnostic.test.py -v
```

测试使用假 Cargo 数据与子进程验证脱敏、流量边界、错误传播及超时清理，不声称验证了真实 Rust 编译或远端缓存效果。真实实验只在受限的 GitHub-hosted Linux main 手动入口执行。
